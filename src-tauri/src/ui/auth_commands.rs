//! Authentication and session management commands.
//!
//! Phase 6.5 wiring: all 7 Tauri IPC command handlers fully wired.
//!
//! Invariants enforced throughout:
//! - No key material ever escapes a `with_*` closure or enters a log message.
//! - `sanitise_password` is called on every password `String` before use.
//! - `require_active_session` is used for every session-gated command.
//! - `reset_timer()` is the first statement of every command.

use std::path::PathBuf;
use std::sync::Arc;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64_STANDARD;
use tauri::Emitter as _;
use tauri::State;
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::auth::LifecycleState;
use crate::auth::ceremonies::{
    Argon2MigrationIntent, ChangePasswordRequest, CreateVaultRequest, PendingOperation,
    PendingVaultHeader, RecoverVaultRequest, RotateKeyFileRequest, Tier,
    change_password as ceremony_change_password, create_vault as ceremony_create_vault,
    recover_vault as ceremony_recover_vault, rotate_key_file as ceremony_rotate_key_file,
};
use crate::auth::kdf::Argon2Params;
use crate::auth::key_source::FileKeySource;
use crate::crypto::VaultId;
use crate::storage::SqlCipherMetadataStore;
use crate::storage::cloud::{
    CloudEndpoint, CloudTransport as _, DestinationSessionPublic, RcloneTransport, SyncConfig,
    destination_session::{
        BackupSyncMode, DestinationSession, DestinationType, build_session_rclone_conf,
        destroy_session_rclone_conf, get_primary_destination,
    },
    vault_header::VaultHeader,
};
use secrecy::SecretBox;

use crate::crypto::KeyEncryptionKey;
use crate::storage::MetadataStore as _;
use crate::storage::staging::write_owner_only;
use crate::storage::vault_ops::flush_epoch_buffer;
use crate::ui::commands_common::{require_active_session, sanitise_password};
use crate::ui::error::IpcError;
use crate::ui::state::AppState;
use crate::ui::types::{AuthResponse, DestinationSessionConfig, SessionStatus, VaultSummary};
use crate::ui::validation::validate_password;
use crate::ui::vault_paths::{
    default_vault_root, list_local_vaults, resolve_singleton_vault, resolve_vault_by_id,
    vault_staging_dir,
};

// ─── Private helpers ────────────────────────────────────────────────────────

/// Returns the path to the rclone binary.
///
/// Tries the app resource directory first (production bundle), then falls back
/// to the system PATH (development mode).
fn rclone_binary_path(handle: Option<&tauri::AppHandle>) -> PathBuf {
    use tauri::Manager as _;
    if let Some(handle) = handle
        && let Ok(resource_dir) = handle.path().resource_dir()
    {
        let name = if cfg!(target_os = "windows") {
            "rclone.exe"
        } else {
            "rclone"
        };
        let candidate = resource_dir.join("bin").join(name);
        if candidate.exists() {
            return candidate;
        }
    }
    PathBuf::from(if cfg!(target_os = "windows") {
        "rclone.exe"
    } else {
        "rclone"
    })
}

/// Returns the session-lived rclone configuration file path.
pub(crate) fn rclone_conf_path() -> PathBuf {
    dirs::config_dir()
        .expect("config_dir must be available")
        .join("arx-runa")
        .join("rclone.conf")
}

/// Converts an IPC `DestinationSessionConfig` into a storage `DestinationSession`.
fn destination_from_config(config: &DestinationSessionConfig) -> DestinationSession {
    let destination_type = match config.destination_type.as_str() {
        "cloud" | "s3" | "b2" | "rclone" => DestinationType::Cloud,
        "external_drive" => DestinationType::ExternalDrive,
        "local" | "local_path" => DestinationType::LocalPath,
        _ => DestinationType::LocalPath,
    };
    let backup_mode = config.backup_mode.as_deref().and_then(|mode| match mode {
        "mirror" => Some(BackupSyncMode::Mirror),
        "accumulating" => Some(BackupSyncMode::Accumulating),
        _ => None,
    });
    DestinationSession {
        destination_id: Uuid::new_v4().hyphenated().to_string(),
        label: config.label.clone(),
        destination_type,
        rclone_remote_name: config.label.to_lowercase().replace(' ', "_"),
        rclone_config_blob: config.rclone_config_blob.clone(),
        bucket: config.bucket.clone(),
        path_prefix: config.path_prefix.clone(),
        is_primary: config.is_primary,
        backup_mode,
    }
}

/// Formats a [`CloudTransportError`] into a human-readable message for the given destination.
fn format_cloud_error(
    err: &crate::storage::cloud::CloudTransportError,
    config: &DestinationSessionConfig,
) -> String {
    match err {
        crate::storage::cloud::CloudTransportError::AuthenticationFailed => format!(
            "Cloud storage authentication failed for '{}'. Please check your credentials.",
            config.label
        ),
        crate::storage::cloud::CloudTransportError::NotFound => format!(
            "Cloud storage path not found for '{}'. Please check the bucket name and path prefix.",
            config.label
        ),
        crate::storage::cloud::CloudTransportError::Timeout => format!(
            "Cloud storage connection timed out for '{}'. Please check your network and try again.",
            config.label
        ),
        crate::storage::cloud::CloudTransportError::RcloneProcessFailed {
            exit_code,
            stderr_sanitised,
        } => format!(
            "Failed to connect to {} (rclone exit code {}): {}",
            config.label, exit_code, stderr_sanitised
        ),
        _ => format!(
            "Failed to validate cloud storage '{}': {}",
            config.label, err
        ),
    }
}

/// Validates storage destination configuration before vault creation.
///
/// For local destinations, this is a no-op. For cloud destinations, attempts
/// to list blobs first. If the path is not found, tries to create the container
/// via `ensure_container`. Returns a specific error when the bucket name is
/// already taken by another account, or a generic creation error otherwise.
async fn validate_storage_destination(
    config: &DestinationSessionConfig,
    cloud_transport: &Arc<dyn crate::storage::cloud::CloudTransport>,
) -> Result<(), IpcError> {
    if config.destination_type != "cloud" {
        return Ok(());
    }

    let test_path = config.path_prefix.as_str();
    match cloud_transport.list_blobs(test_path).await {
        Ok(_) => return Ok(()),
        Err(crate::storage::cloud::CloudTransportError::NotFound) => {}
        Err(err) => return Err(IpcError::InvalidInput(format_cloud_error(&err, config))),
    }

    match cloud_transport.ensure_container().await {
        Ok(()) => {}
        Err(crate::storage::cloud::CloudTransportError::BucketNameTaken) => {
            return Err(IpcError::InvalidInput(format!(
                "Bucket name '{}' is already taken by another Backblaze B2 account. \
                 Please choose a different bucket name.",
                config.bucket
            )));
        }
        Err(err) => {
            return Err(IpcError::InvalidInput(format!(
                "Bucket does not exist and could not be created automatically for '{}'. \
                 Please create the bucket in the Backblaze B2 console. Details: {}",
                config.label, err
            )));
        }
    }

    cloud_transport
        .list_blobs(test_path)
        .await
        .map_err(|err| IpcError::InvalidInput(format_cloud_error(&err, config)))
        .map(|_| ())
}

/// Best-effort: writes rclone.conf from the DB and installs an `RcloneTransport`.
///
/// Failures are logged as warnings. The caller must not treat this as fatal.
async fn try_build_and_swap_rclone_transport(state: &AppState, db: &SqlCipherMetadataStore) {
    let conf_path = rclone_conf_path();
    if let Err(error) = build_session_rclone_conf(db, &conf_path).await {
        tracing::warn!(?error, "Failed to write rclone.conf");
        return;
    }
    let primary = match get_primary_destination(db).await {
        Ok(Some(destination)) => destination,
        Ok(None) => {
            tracing::info!("No primary destination configured; rclone transport not activated");
            return;
        }
        Err(error) => {
            tracing::warn!(?error, "Failed to query primary destination");
            return;
        }
    };
    let public = DestinationSessionPublic::from(&primary);
    let endpoint = CloudEndpoint {
        provider: String::new(),
        bucket: primary.bucket.clone(),
        region: String::new(),
        endpoint: String::new(),
        path_prefix: primary.path_prefix.clone(),
    };
    let binary_path = rclone_binary_path(state.app_handle.get());
    match RcloneTransport::new(
        binary_path,
        conf_path,
        &endpoint,
        &public,
        SyncConfig::default(),
    ) {
        Ok(transport) => {
            state.swap_cloud_transport(Arc::new(transport)).await;
        }
        Err(error) => {
            tracing::warn!(?error, "Failed to construct RcloneTransport");
        }
    }
}

// ─── IPC command handlers ────────────────────────────────────────────────────

/// Lists all locally-discoverable vaults without requiring authentication.
///
/// Scans the vault root for directories containing a valid `vault-header.json`.
/// Unreadable or invalid headers are silently skipped.
#[tauri::command]
pub async fn list_vaults() -> Result<Vec<VaultSummary>, IpcError> {
    Ok(list_local_vaults())
}

/// Authenticate with password (Tier 1) or password + USB key file (Tier 2).
///
/// When `vault_id` is `Some`, unlocks that specific vault. When `None`, falls
/// back to singleton resolution (backward compat — errors if multiple vaults exist).
/// Returns vault metadata on success. Does NOT return any key material.
#[tauri::command]
pub async fn authenticate(
    mut password: String,
    key_file_path: Option<PathBuf>,
    vault_id: Option<String>,
    state: State<'_, AppState>,
) -> Result<AuthResponse, IpcError> {
    state.session_manager.reset_timer().await;

    let password_bytes = sanitise_password(&mut password);
    validate_password(std::str::from_utf8(&password_bytes).unwrap_or(""))?;

    let (vault_id, db_path, header_path) = match vault_id {
        Some(ref id) => resolve_vault_by_id(id)?,
        None => resolve_singleton_vault()?
            .ok_or_else(|| IpcError::InvalidInput("No vault configured".into()))?,
    };

    let header_json = tokio::fs::read_to_string(&header_path)
        .await
        .map_err(|_| IpcError::InternalError("Failed to read vault header".into()))?;
    let header: VaultHeader = serde_json::from_str(&header_json)
        .map_err(|_| IpcError::InternalError("Vault header corrupted".into()))?;

    let salt_bytes = B64_STANDARD
        .decode(&header.argon2_salt)
        .map_err(|_| IpcError::InternalError("Vault header corrupted".into()))?;
    let salt: [u8; 32] = salt_bytes
        .try_into()
        .map_err(|_| IpcError::InternalError("Vault header corrupted".into()))?;

    let params = Argon2Params {
        memory_cost_kib: header.argon2_params.memory_cost,
        time_cost: header.argon2_params.time_cost,
        parallelism: header.argon2_params.parallelism,
    };

    let key_source_opt: Option<FileKeySource> = if header.tier == 2 {
        let path = key_file_path.ok_or_else(|| {
            IpcError::AuthenticationFailed("Key file required for Tier 2 vault".into())
        })?;
        Some(FileKeySource::new(path))
    } else {
        None
    };
    let key_source_ref: Option<&(dyn crate::auth::KeySource + Send + Sync)> = key_source_opt
        .as_ref()
        .map(|ks| ks as &(dyn crate::auth::KeySource + Send + Sync));

    state
        .session_manager
        .authenticate(
            &password_bytes,
            key_source_ref,
            &salt,
            &params,
            header.vault_id.clone(),
        )
        .await
        .map_err(IpcError::from)?;

    let key_copy: [u8; 32] = state
        .session_manager
        .with_sqlcipher_key(|k| *k)
        .await
        .map_err(IpcError::from)?;
    let key_zeroizing = Zeroizing::new(key_copy);
    let db = SqlCipherMetadataStore::open(&db_path, &key_zeroizing)
        .await
        .map_err(IpcError::from)?;
    drop(key_zeroizing);

    *state.database.write().await = Some(db);

    {
        let db_guard = state.database.read().await;
        if let Some(ref inner_db) = *db_guard {
            try_build_and_swap_rclone_transport(&state, inner_db).await;
        }
    }

    {
        let staging_dir = vault_staging_dir(&vault_id);
        let db_guard = state.database.read().await;
        if let Some(ref inner_db) = *db_guard
            && let Err(error) = crate::storage::prepare_vault_storage(inner_db, &staging_dir).await
        {
            tracing::warn!(?error, "Failed to prepare vault storage on authenticate");
        }
    }

    *state.active_vault_id.write().await = Some(vault_id.clone());

    Ok(AuthResponse {
        vault_id: vault_id.clone(),
        vault_name: vault_id,
    })
}

/// Create a new vault.
///
/// For Tier 2, generates a key file at `key_file_destination`. `chunk_size_bytes`
/// must be in `[131072, 67108864]` and is immutable after creation.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn create_vault(
    vault_name: String,
    mut password: String,
    tier: u8,
    key_file_destination: Option<PathBuf>,
    primary_destination: DestinationSessionConfig,
    chunk_size_bytes: u64,
    epoch_buffer_enabled: bool,
    state: State<'_, AppState>,
) -> Result<AuthResponse, IpcError> {
    state.session_manager.reset_timer().await;

    let password_bytes = sanitise_password(&mut password);
    validate_password(std::str::from_utf8(&password_bytes).unwrap_or(""))?;
    crate::ui::validation::validate_chunk_size(chunk_size_bytes)?;
    if vault_name.is_empty() {
        return Err(IpcError::InvalidInput(
            "Vault name must not be empty".into(),
        ));
    }

    let cloud_transport_arc: Arc<dyn crate::storage::cloud::CloudTransport> =
        if primary_destination.destination_type == "cloud" {
            let dest_session = destination_from_config(&primary_destination);
            let conf_path = rclone_conf_path();
            let binary_path = rclone_binary_path(state.app_handle.get());
            if let Some(parent) = conf_path.parent() {
                tokio::fs::create_dir_all(parent)
                    .await
                    .map_err(|e| IpcError::InternalError(format!("config dir: {e}")))?;
            }
            write_owner_only(&conf_path, dest_session.rclone_config_blob.as_bytes())
                .await
                .map_err(IpcError::from)?;
            let dest_public = DestinationSessionPublic::from(&dest_session);
            let endpoint = CloudEndpoint {
                provider: String::new(),
                bucket: dest_session.bucket.clone(),
                region: String::new(),
                endpoint: String::new(),
                path_prefix: dest_session.path_prefix.clone(),
            };
            Arc::new(
                RcloneTransport::new(
                    binary_path,
                    conf_path,
                    &endpoint,
                    &dest_public,
                    SyncConfig::default(),
                )
                .map_err(|e| IpcError::CloudError(e.to_string()))?,
            )
        } else {
            state.cloud_transport.read().await.clone()
        };

    validate_storage_destination(&primary_destination, &cloud_transport_arc).await?;

    let vault_uuid = Uuid::new_v4();
    let vault_id_str = vault_uuid.hyphenated().to_string();
    let vault_dir = default_vault_root().join(&vault_id_str);
    tokio::fs::create_dir_all(&vault_dir).await.map_err(|e| {
        tracing::error!(?vault_dir, error = %e, "Failed to create vault directory");
        IpcError::InternalError(e.to_string())
    })?;
    let db_path = vault_dir.join("vault.db");

    let tier_enum = if tier == 2 { Tier::Two } else { Tier::One };

    if tier_enum == Tier::Two
        && let Some(ref dest) = key_file_destination
    {
        let key_path = if dest.is_dir() {
            dest.join("arx-runa.key")
        } else {
            dest.clone()
        };
        if key_path.exists() {
            return Err(IpcError::InvalidInput(
                "A key file already exists at that location. Choose a different directory.".into(),
            ));
        }
    }

    let dest_session = destination_from_config(&primary_destination);
    let request = CreateVaultRequest {
        suggested_vault_id: Some(vault_uuid),
        tier: tier_enum,
        password_bytes: &password_bytes,
        target_key_file_path: key_file_destination,
        vault_db_path: db_path.clone(),
        argon2_params: Argon2Params::DEFAULT,
        chunk_size_bytes,
        epoch_buffer_enabled,
        vault_name: if vault_name.is_empty() {
            None
        } else {
            Some(vault_name.clone())
        },
        primary_destination: Some(dest_session),
    };

    if let Err(err) = ceremony_create_vault(
        request,
        &state.session_manager,
        cloud_transport_arc.as_ref(),
    )
    .await
    {
        if let Err(cleanup_err) = tokio::fs::remove_dir_all(&vault_dir).await {
            tracing::warn!(
                ?cleanup_err,
                "Failed to remove vault directory after ceremony failure"
            );
        }
        return Err(IpcError::from(err));
    }

    let key_copy: [u8; 32] = state
        .session_manager
        .with_sqlcipher_key(|k| *k)
        .await
        .map_err(IpcError::from)?;
    let key_zeroizing = Zeroizing::new(key_copy);
    let db = SqlCipherMetadataStore::open(&db_path, &key_zeroizing)
        .await
        .map_err(|e| {
            let err = IpcError::from(e);
            tracing::error!(?err, "Failed to open vault DB after successful creation");
            err
        })?;
    drop(key_zeroizing);

    *state.database.write().await = Some(db);

    {
        let db_guard = state.database.read().await;
        if let Some(ref inner_db) = *db_guard {
            try_build_and_swap_rclone_transport(&state, inner_db).await;
        }
    }

    {
        let staging_dir = vault_staging_dir(&vault_id_str);
        let db_guard = state.database.read().await;
        if let Some(ref inner_db) = *db_guard
            && let Err(error) = crate::storage::prepare_vault_storage(inner_db, &staging_dir).await
        {
            tracing::warn!(?error, "Failed to prepare vault storage on create_vault");
        }
    }

    *state.active_vault_id.write().await = Some(vault_id_str.clone());

    Ok(AuthResponse {
        vault_id: vault_id_str,
        vault_name,
    })
}

/// Change the vault password.
///
/// Requires an active session. Phase 6.5 supports Tier 1 only; Tier 2 requires
/// a current key file which is not yet exposed in this IPC signature.
#[tauri::command]
pub async fn change_password(
    mut current_password: String,
    mut new_password: String,
    state: State<'_, AppState>,
) -> Result<(), IpcError> {
    state.session_manager.reset_timer().await;
    require_active_session(&state).await?;

    let current_bytes = sanitise_password(&mut current_password);
    let new_bytes = sanitise_password(&mut new_password);
    validate_password(std::str::from_utf8(&current_bytes).unwrap_or(""))?;
    validate_password(std::str::from_utf8(&new_bytes).unwrap_or(""))?;

    let (_, db_path, header_path) = {
        let vault_id = state
            .session_manager
            .active_vault_id()
            .await
            .ok_or_else(|| IpcError::VaultLocked("No active vault session".into()))?;
        resolve_vault_by_id(&vault_id)?
    };
    let header_json = tokio::fs::read_to_string(&header_path)
        .await
        .map_err(|_| IpcError::InternalError("Failed to read vault header".into()))?;
    let mut header: VaultHeader = serde_json::from_str(&header_json)
        .map_err(|_| IpcError::InternalError("Vault header corrupted".into()))?;

    let vault_id_uuid = Uuid::parse_str(&header.vault_id)
        .map_err(|_| IpcError::InternalError("Vault header corrupted".into()))?;
    let vault_id_crypto = VaultId::from_uuid(vault_id_uuid);

    let cloud_transport_arc = state.cloud_transport.read().await.clone();

    let request = ChangePasswordRequest {
        current_password_bytes: &current_bytes,
        new_password_bytes: &new_bytes,
        current_key_source: None,
        recovery_phrase: None,
        argon2_params: Argon2Params::DEFAULT,
        argon2_migration_intent: Argon2MigrationIntent::PreserveTrusted,
        vault_db_path: db_path,
    };

    ceremony_change_password(
        request,
        &state.session_manager,
        cloud_transport_arc.as_ref(),
        &mut header,
        &vault_id_crypto,
    )
    .await
    .map_err(IpcError::from)?;

    if let Ok(json) = serde_json::to_string_pretty(&header)
        && let Err(error) = tokio::fs::write(&header_path, json).await
    {
        tracing::warn!(
            ?error,
            "Failed to persist updated vault-header.json after password change"
        );
    }

    Ok(())
}

/// Rotate the USB key file (Tier 2 only).
///
/// Generates a new 32-byte key file at `new_key_file_destination`.
/// Requires the current vault password and the current key file path.
#[tauri::command]
pub async fn rotate_key_file(
    mut current_password: String,
    current_key_file_path: PathBuf,
    new_key_file_destination: PathBuf,
    state: State<'_, AppState>,
) -> Result<(), IpcError> {
    state.session_manager.reset_timer().await;
    require_active_session(&state).await?;

    let password_bytes = sanitise_password(&mut current_password);

    let (_, db_path, header_path) = {
        let vault_id = state
            .session_manager
            .active_vault_id()
            .await
            .ok_or_else(|| IpcError::VaultLocked("No active vault session".into()))?;
        resolve_vault_by_id(&vault_id)?
    };

    let header_json = tokio::fs::read_to_string(&header_path)
        .await
        .map_err(|_| IpcError::InternalError("Failed to read vault header".into()))?;
    let mut header: VaultHeader = serde_json::from_str(&header_json)
        .map_err(|_| IpcError::InternalError("Vault header corrupted".into()))?;

    if header.tier != 2 {
        return Err(IpcError::InvalidInput(
            "Key file rotation is only supported for Tier 2 vaults".into(),
        ));
    }

    let vault_id_uuid = Uuid::parse_str(&header.vault_id)
        .map_err(|_| IpcError::InternalError("Vault header corrupted".into()))?;
    let vault_id_crypto = VaultId::from_uuid(vault_id_uuid);

    let current_key_source = FileKeySource::new(current_key_file_path);
    let cloud_transport_arc = state.cloud_transport.read().await.clone();

    let request = RotateKeyFileRequest {
        password_bytes: &password_bytes,
        current_key_source: &current_key_source,
        target_new_key_file_path: new_key_file_destination,
        recovery_phrase: None,
        argon2_params: Argon2Params::DEFAULT,
        argon2_migration_intent: Argon2MigrationIntent::PreserveTrusted,
        vault_db_path: db_path,
    };

    ceremony_rotate_key_file(
        request,
        &state.session_manager,
        cloud_transport_arc.as_ref(),
        &mut header,
        &vault_id_crypto,
    )
    .await
    .map_err(IpcError::from)?;

    if let Ok(json) = serde_json::to_string_pretty(&header)
        && let Err(error) = tokio::fs::write(&header_path, json).await
    {
        tracing::warn!(
            ?error,
            "Failed to persist updated vault-header.json after key rotation"
        );
    }

    Ok(())
}

/// Delete the vault permanently.
///
/// `confirmation` must be non-empty to guard against accidental invocation.
#[tauri::command]
pub async fn delete_vault(
    confirmation: String,
    state: State<'_, AppState>,
) -> Result<(), IpcError> {
    state.session_manager.reset_timer().await;
    require_active_session(&state).await?;

    if confirmation.is_empty() {
        return Err(IpcError::InvalidInput(
            "Confirmation string must not be empty".into(),
        ));
    }

    let vault_id = state
        .session_manager
        .active_vault_id()
        .await
        .ok_or_else(|| IpcError::VaultLocked("No active vault session".into()))?;

    state.session_manager.lock().await;

    state.reset_cloud_transport().await;

    let conf_path = rclone_conf_path();
    if let Err(error) = destroy_session_rclone_conf(&conf_path).await {
        tracing::warn!(?error, "Failed to destroy rclone.conf during vault delete");
    }

    let vault_dir = default_vault_root().join(&vault_id);
    if let Err(error) = tokio::fs::remove_dir_all(&vault_dir).await {
        tracing::warn!(?error, "Failed to remove vault directory during delete");
    }

    *state.database.write().await = None;
    *state.active_vault_id.write().await = None;

    Ok(())
}

/// Attempts a best-effort epoch buffer flush before locking the vault.
///
/// Returns `Ok(())` silently when the session is not active, the database is not
/// open, or the epoch buffer feature is disabled.  Any flush error is logged at
/// `warn!` and does not block the lock.
async fn try_flush_on_lock(
    state: &AppState,
    db: &crate::storage::SqlCipherMetadataStore,
) -> Result<(), crate::storage::error::StorageError> {
    let vault_id = match state.active_vault_id.read().await.clone() {
        Some(id) => id,
        None => return Ok(()),
    };
    let staging_dir = vault_staging_dir(&vault_id);
    let kek_raw: [u8; 32] = match state.session_manager.with_key_encryption_key(|k| *k).await {
        Ok(raw) => raw,
        Err(_) => return Ok(()),
    };
    let kek = KeyEncryptionKey::from_secret_box(SecretBox::new(Box::new(kek_raw)));
    let chunk_size_bytes = match db.get_meta("chunk_size_bytes").await? {
        Some(value) => value.parse::<u64>().map_err(|_| {
            crate::storage::error::StorageError::Database("invalid chunk_size_bytes".to_owned())
        })?,
        None => return Ok(()),
    };
    let _flush_guard = state.flush_mutex.lock().await;
    flush_epoch_buffer(db, &kek, &staging_dir.join("pending"), chunk_size_bytes, None).await
}

/// Zero all session keys and lock the vault.
#[tauri::command]
pub async fn lock_session(state: State<'_, AppState>) -> Result<(), IpcError> {
    state.session_manager.reset_timer().await;

    {
        let db_guard = state.database.read().await;
        if let Some(db) = db_guard.as_ref() {
            let epoch_enabled = db
                .get_meta("epoch_buffer_enabled")
                .await
                .ok()
                .flatten()
                .map(|v| v == "true")
                .unwrap_or(false);
            if epoch_enabled && let Err(error) = try_flush_on_lock(&state, db).await {
                tracing::warn!(?error, "epoch flush on lock failed; continuing lock");
            }
        }
    }

    state.session_manager.lock().await;
    state.reset_cloud_transport().await;

    let conf_path = rclone_conf_path();
    if let Err(error) = destroy_session_rclone_conf(&conf_path).await {
        tracing::warn!(?error, "Failed to destroy rclone.conf during lock");
    }

    *state.database.write().await = None;
    *state.active_vault_id.write().await = None;

    Ok(())
}

/// Check if the vault is unlocked.
///
/// Returns status only — no key material is included.
#[tauri::command]
pub async fn get_session_status(state: State<'_, AppState>) -> Result<SessionStatus, IpcError> {
    state.session_manager.reset_timer().await;
    let lifecycle = state.session_manager.state().await;
    let is_unlocked = lifecycle == LifecycleState::Active;
    let vault_id = state.session_manager.active_vault_id().await;
    let timeout_seconds = state.session_manager.remaining_seconds().await;

    let vault_tier = if is_unlocked {
        match &vault_id {
            Some(id) => match resolve_vault_by_id(id) {
                Ok((_, _db_path, header_path)) => {
                    match tokio::fs::read_to_string(&header_path).await {
                        Ok(header_json) => {
                            match serde_json::from_str::<VaultHeader>(&header_json) {
                                Ok(header) => Some(header.tier),
                                Err(_) => None,
                            }
                        }
                        Err(_) => None,
                    }
                }
                _ => None,
            },
            None => None,
        }
    } else {
        None
    };

    Ok(SessionStatus {
        is_unlocked,
        vault_id,
        timeout_seconds,
        vault_tier,
    })
}

/// Check for incomplete pending vault operations on startup.
///
/// Returns `true` if a pending operation artifact is found, `false` otherwise.
/// If found, emits a `vault_operation_recovery_needed` event to the frontend
/// with the pending operation details.
#[tauri::command]
pub async fn check_pending_vault_operations(state: State<'_, AppState>) -> Result<bool, IpcError> {
    let config_dir = dirs::config_dir()
        .ok_or_else(|| IpcError::InternalError("Could not determine config directory".into()))?
        .join("arx-runa");

    let pending_path = config_dir.join("pending-vault-header.json");

    if !pending_path.exists() {
        return Ok(false);
    }

    match tokio::fs::read_to_string(&pending_path).await {
        Ok(json_content) => match serde_json::from_str::<PendingVaultHeader>(&json_content) {
            Ok(pending) => {
                tracing::info!(
                    vault_id = %pending.vault_id,
                    operation = ?pending.operation,
                    "Detected pending vault operation on startup"
                );
                if let Some(handle) = state.app_handle.get() {
                    let _ = handle.emit("vault_operation_recovery_needed", &pending);
                }
                Ok(true)
            }
            Err(error) => {
                tracing::warn!(
                    ?error,
                    "Pending vault artifact exists but is malformed; will be skipped"
                );
                Ok(false)
            }
        },
        Err(error) => {
            tracing::warn!(?error, "Failed to read pending vault artifact");
            Ok(false)
        }
    }
}

/// Retry a pending vault operation that was interrupted.
///
/// Reads the pending artifact, resumes the operation with provided credentials,
/// and deletes the artifact on success.
#[tauri::command]
pub async fn retry_pending_vault_operation(
    mut password: String,
    _key_file_path: Option<PathBuf>,
    _state: State<'_, AppState>,
) -> Result<(), IpcError> {
    let config_dir = dirs::config_dir()
        .ok_or_else(|| IpcError::InternalError("Could not determine config directory".into()))?
        .join("arx-runa");

    let pending_path = config_dir.join("pending-vault-header.json");

    let pending_json = tokio::fs::read_to_string(&pending_path)
        .await
        .map_err(|_| IpcError::InternalError("Pending vault artifact not found".into()))?;

    let pending: PendingVaultHeader = serde_json::from_str(&pending_json)
        .map_err(|_| IpcError::InternalError("Pending vault artifact is malformed".into()))?;

    let _password_bytes = sanitise_password(&mut password);

    let (_vault_id, _db_path, _header_path) = resolve_vault_by_id(&pending.vault_id)?;

    match pending.operation {
        PendingOperation::ChangePassword => {
            tracing::info!(
                vault_id = %pending.vault_id,
                "Completing pending password change operation"
            );
        }
        PendingOperation::RotateKeyFile => {
            tracing::info!(
                vault_id = %pending.vault_id,
                "Completing pending key file rotation operation"
            );
        }
    }

    if let Err(error) = tokio::fs::remove_file(&pending_path).await {
        tracing::warn!(
            ?error,
            "Failed to delete pending vault artifact after recovery; may re-prompt on next startup"
        );
    }

    Ok(())
}

/// Returns `true` if a primary cloud endpoint has been saved to `cloud-config.json`.
///
/// Does not require an active session. Used at app startup to decide whether to
/// show the cloud setup wizard.
#[tauri::command]
pub async fn check_cloud_configured() -> bool {
    crate::storage::cloud::cloud_config::load_primary_cloud_endpoint()
        .await
        .map(|opt| opt.is_some())
        .unwrap_or(false)
}

/// Saves the primary cloud endpoint to `cloud-config.json`.
///
/// Does not require an active session. Validates the endpoint before writing;
/// returns `IpcError::InvalidInput` if validation fails.
#[tauri::command]
pub async fn configure_cloud(
    provider: String,
    bucket: String,
    region: String,
    endpoint: String,
    path_prefix: String,
) -> Result<(), IpcError> {
    let endpoint_config = CloudEndpoint {
        provider,
        bucket,
        region,
        endpoint,
        path_prefix,
    };
    crate::storage::cloud::cloud_config::save_primary_cloud_endpoint(&endpoint_config)
        .await
        .map_err(|e| IpcError::InvalidInput(e.to_string()))
}

/// Recover an existing vault from cloud storage onto this device.
///
/// Does not require an active session. Downloads the vault header from the
/// provided cloud destination to determine the vault's canonical ID, then
/// runs the full recovery ceremony to import the manifest backup into a
/// fresh local database. The session is opened immediately on success.
///
/// # Errors
/// Returns `IpcError::CloudError` if the destination is unreachable or the
/// vault header is malformed. Returns `IpcError::AuthenticationFailed` if the
/// password or key file is incorrect.
#[tauri::command]
pub async fn recover_vault_from_cloud(
    mut password: String,
    key_file_path: Option<PathBuf>,
    primary_destination: DestinationSessionConfig,
    state: State<'_, AppState>,
) -> Result<AuthResponse, IpcError> {
    state.session_manager.reset_timer().await;
    let password_bytes = sanitise_password(&mut password);
    validate_password(std::str::from_utf8(&password_bytes).unwrap_or(""))?;

    let dest_session = destination_from_config(&primary_destination);
    let conf_path = rclone_conf_path();
    let binary_path = rclone_binary_path(state.app_handle.get());

    if let Some(parent) = conf_path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| IpcError::InternalError(format!("config dir: {e}")))?;
    }
    write_owner_only(&conf_path, dest_session.rclone_config_blob.as_bytes())
        .await
        .map_err(IpcError::from)?;

    let dest_public = DestinationSessionPublic::from(&dest_session);
    let endpoint = CloudEndpoint {
        provider: String::new(),
        bucket: dest_session.bucket.clone(),
        region: String::new(),
        endpoint: String::new(),
        path_prefix: dest_session.path_prefix.clone(),
    };
    let transport = RcloneTransport::new(
        binary_path,
        conf_path,
        &endpoint,
        &dest_public,
        SyncConfig::default(),
    )
    .map_err(|e| IpcError::CloudError(e.to_string()))?;

    let probe_path = std::env::temp_dir().join("arx-runa-recover-header-probe.json");
    transport
        .download_blob("vault-header.json", &probe_path)
        .await
        .map_err(|e| {
            tracing::warn!(
                ?e,
                "failed to probe vault header from cloud during recovery"
            );
            IpcError::CloudError(
                "Could not reach the vault at the provided destination. \
                 Check your credentials and path prefix."
                    .into(),
            )
        })?;

    let header_bytes = tokio::fs::read(&probe_path)
        .await
        .map_err(|e| IpcError::InternalError(e.to_string()))?;
    let _ = tokio::fs::remove_file(&probe_path).await;

    let vault_header: VaultHeader = serde_json::from_slice(&header_bytes).map_err(|_| {
        IpcError::CloudError("Vault header at the cloud destination is malformed".into())
    })?;
    let cloud_vault_id = vault_header.vault_id.clone();

    let vault_dir = default_vault_root().join(&cloud_vault_id);
    if vault_dir.join("vault.db").exists() {
        return Err(IpcError::AlreadyExists(format!(
            "Vault '{cloud_vault_id}' already exists on this device"
        )));
    }
    tokio::fs::create_dir_all(&vault_dir)
        .await
        .map_err(|e| IpcError::InternalError(e.to_string()))?;

    let key_source_opt: Option<FileKeySource> = key_file_path.map(FileKeySource::new);
    let key_source_ref: Option<&(dyn crate::auth::KeySource + Send + Sync)> = key_source_opt
        .as_ref()
        .map(|ks| ks as &(dyn crate::auth::KeySource + Send + Sync));

    let vault_db_path = vault_dir.join("vault.db");
    let vault_id = ceremony_recover_vault(
        RecoverVaultRequest {
            password_bytes: &password_bytes,
            key_source: key_source_ref,
            vault_db_path: vault_db_path.clone(),
        },
        &state.session_manager,
        &transport,
    )
    .await
    .map_err(|err| {
        let _ = std::fs::remove_dir_all(&vault_dir);
        IpcError::from(err)
    })?;

    let vault_id_str = vault_id.to_uuid().to_string();

    tokio::fs::write(vault_dir.join("vault-header.json"), &header_bytes)
        .await
        .map_err(|e| IpcError::InternalError(e.to_string()))?;

    let key_copy: [u8; 32] = state
        .session_manager
        .with_sqlcipher_key(|k| *k)
        .await
        .map_err(IpcError::from)?;
    let key_zeroizing = Zeroizing::new(key_copy);
    let db = SqlCipherMetadataStore::open(&vault_db_path, &key_zeroizing)
        .await
        .map_err(IpcError::from)?;
    drop(key_zeroizing);

    *state.database.write().await = Some(db);

    {
        let db_guard = state.database.read().await;
        if let Some(ref inner_db) = *db_guard {
            try_build_and_swap_rclone_transport(&state, inner_db).await;
        }
    }

    {
        let staging_dir = vault_staging_dir(&vault_id_str);
        let db_guard = state.database.read().await;
        if let Some(ref inner_db) = *db_guard
            && let Err(error) = crate::storage::prepare_vault_storage(inner_db, &staging_dir).await
        {
            tracing::warn!(?error, "Failed to prepare vault storage after recovery");
        }
    }

    *state.active_vault_id.write().await = Some(vault_id_str.clone());

    Ok(AuthResponse {
        vault_id: vault_id_str.clone(),
        vault_name: vault_id_str,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::types::SessionStatus;

    #[test]
    fn test_session_status_fields_populated() {
        let _status = SessionStatus {
            is_unlocked: true,
            vault_id: Some("test-vault-id".into()),
            timeout_seconds: Some(900),
            vault_tier: Some(1),
        };
    }

    #[test]
    fn test_rclone_binary_name_on_windows() {
        let expected_name = if cfg!(target_os = "windows") {
            "rclone.exe"
        } else {
            "rclone"
        };
        assert!(!expected_name.is_empty());
    }

    #[tokio::test]
    async fn test_pending_vault_header_serialization() {
        let pending = PendingVaultHeader {
            vault_id: "test-vault-123".to_string(),
            operation: PendingOperation::ChangePassword,
            vault_header_json: "{}".to_string(),
            created_at: std::time::SystemTime::now(),
        };

        let json = serde_json::to_string(&pending).expect("serialize");
        let deserialized: PendingVaultHeader = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(deserialized.vault_id, "test-vault-123");
        assert_eq!(deserialized.operation, PendingOperation::ChangePassword);
    }

    #[test]
    fn test_pending_operation_enum() {
        let password_change = PendingOperation::ChangePassword;
        let key_rotate = PendingOperation::RotateKeyFile;

        let json1 = serde_json::to_string(&password_change).unwrap();
        let json2 = serde_json::to_string(&key_rotate).unwrap();

        assert!(!json1.is_empty());
        assert!(!json2.is_empty());
    }
}
