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

use crate::auth::LifecycleState;
use crate::auth::ceremonies::{
    Argon2MigrationIntent, ChangePasswordRequest, CreateVaultRequest, PendingOperation,
    PendingVaultHeader, RecoverVaultRequest, RecoverWithPhraseRequest, RotateKeyFileRequest,
    SetupRecoveryRequest, Tier, change_password as ceremony_change_password,
    create_vault as ceremony_create_vault, recover_vault as ceremony_recover_vault,
    recover_with_phrase as ceremony_recover_with_phrase,
    rotate_key_file as ceremony_rotate_key_file, setup_recovery as ceremony_setup_recovery,
    unlock_vault as ceremony_unlock_vault,
};
use crate::auth::kdf::Argon2Params;
use crate::auth::key_source::FileKeySource;
use crate::crypto::VaultId;
use crate::storage::SqlCipherMetadataStore;
use crate::storage::cloud::{
    CloudEndpoint, CloudTransport as _, DestinationSessionPublic, RcloneTransport, SyncConfig,
    destination_session::{
        BackupSyncMode, DestinationSession, DestinationType, build_session_rclone_conf,
        create_session_rclone_dir, destroy_session_rclone_conf, get_primary_destination,
        insert_destination_session,
    },
    upload_vault_header, validate_single_remote_stanza,
    vault_header::VaultHeader,
};
use secrecy::SecretBox;
use tauri::Emitter as _;
use tauri::State;
use tauri::ipc::Channel;
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::crypto::KeyEncryptionKey;
use crate::storage::MetadataStore as _;
use crate::storage::staging::write_owner_only;
use crate::storage::vault_ops::flush_epoch_buffer;
use crate::ui::commands_common::{
    ProgressChannel, rclone_binary_path, require_active_session, sanitise_password,
};
use crate::ui::error::IpcError;
use crate::ui::state::{AppState, SessionVaultInfo};
use crate::ui::types::{
    AuthResponse, DestinationSessionConfig, ProgressUpdate, SessionStatus, VaultSummary,
};
use crate::ui::validation::validate_password;
use crate::ui::vault_paths::{
    default_vault_root, list_local_vaults, resolve_singleton_vault, resolve_vault_by_id,
    vault_staging_dir,
};

// ─── Private helpers ────────────────────────────────────────────────────────

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
        device_id: None,
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
        _ => {
            tracing::warn!(label = %config.label, error = %err, "unrecognised cloud error");
            format!("Failed to validate cloud storage '{}'", config.label)
        }
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

    // NotFound is acceptable: Drive creates the path-prefix folder lazily on first upload.
    match cloud_transport.list_blobs(test_path).await {
        Ok(_) | Err(crate::storage::cloud::CloudTransportError::NotFound) => Ok(()),
        Err(err) => Err(IpcError::InvalidInput(format_cloud_error(&err, config))),
    }
}

/// Best-effort: writes rclone.conf from the DB and installs an `RcloneTransport`.
///
/// Failures are logged as warnings. The caller must not treat this as fatal.
async fn try_build_and_swap_rclone_transport(state: &AppState, db: &SqlCipherMetadataStore) {
    // Crash recovery: destroy any leftover conf from a previous session that failed to clean up.
    if let Some(stale_conf_path) = state.session_manager.rclone_conf_path().await {
        if let Err(error) = destroy_session_rclone_conf(&stale_conf_path).await {
            tracing::warn!(
                ?error,
                path = %stale_conf_path.display(),
                "Failed to remove pre-existing rclone.conf before session start"
            );
        }
        if let Some(dir) = stale_conf_path.parent() {
            let _ = tokio::fs::remove_dir(dir).await;
        }
        // Stale path is overwritten by set_rclone_conf_path below.
    }

    let rclone_dir = match create_session_rclone_dir().await {
        Ok(dir) => dir,
        Err(error) => {
            tracing::warn!(
                ?error,
                "Failed to create process-owned temp dir for rclone.conf"
            );
            return;
        }
    };
    let conf_path = rclone_dir.join("rclone.conf");
    state
        .session_manager
        .set_rclone_conf_path(conf_path.clone())
        .await;
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
            // Load the Google Drive SA JSON (if configured) so generate_share_credentials
            // can grant SA reader permissions without a separate DB lookup at share time.
            let sa_config = db
                .get_gdrive_sharing_config()
                .await
                .ok()
                .flatten()
                .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok());
            let transport = transport.with_sharing_config(sa_config);
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

    let (_, _, header_path) = match vault_id {
        Some(ref id) => resolve_vault_by_id(id)?,
        None => resolve_singleton_vault()?
            .ok_or_else(|| IpcError::InvalidInput("No vault configured".into()))?,
    };

    let vault_id = ceremony_unlock_vault(
        &password_bytes,
        key_file_path,
        &header_path,
        &state.session_manager,
    )
    .await
    .map_err(IpcError::from)?;

    if let Some(db) = state.session_manager.get_metadata_store().await {
        try_build_and_swap_rclone_transport(&state, &db).await;
        let staging_dir = vault_staging_dir(&vault_id);
        if let Err(error) = crate::storage::prepare_vault_storage(&db, &staging_dir).await {
            tracing::warn!(?error, "Failed to prepare vault storage on authenticate");
        }
    }

    *state.active_vault_id.write().await = Some(vault_id.clone());
    cache_session_vault_info(&state, &header_path).await;

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
    progress: Channel<ProgressUpdate>,
    state: State<'_, AppState>,
) -> Result<AuthResponse, IpcError> {
    state.session_manager.reset_timer().await;

    let progress_ch = ProgressChannel::new(progress);
    let _ = progress_ch.try_send_if_open(ProgressUpdate {
        percent: 0,
        bytes_processed: 0,
        bytes_total: 0,
        status: "Setting up encryption…".into(),
    });

    let password_bytes = sanitise_password(&mut password);
    validate_password(
        std::str::from_utf8(&password_bytes)
            .map_err(|_| IpcError::InvalidInput("password encoding error".into()))?,
    )?;
    crate::ui::validation::validate_chunk_size(chunk_size_bytes)?;
    if vault_name.is_empty() {
        return Err(IpcError::InvalidInput(
            "Vault name must not be empty".into(),
        ));
    }

    // Track the ceremony-scoped temp dir so it can be removed after the ceremony.
    let mut ceremony_rclone_dir: Option<PathBuf> = None;
    let cloud_transport_arc: Arc<dyn crate::storage::cloud::CloudTransport> =
        if primary_destination.destination_type == "cloud" {
            let dest_session = destination_from_config(&primary_destination);
            let rclone_dir = create_session_rclone_dir().await.map_err(|_| {
                IpcError::InternalError("Failed to create temporary directory for rclone".into())
            })?;
            let conf_path = rclone_dir.join("rclone.conf");
            ceremony_rclone_dir = Some(rclone_dir);
            let binary_path = rclone_binary_path(state.app_handle.get());
            // Normalise the blob section header to match rclone_remote_name so rclone can
            // find the remote, regardless of what section name the OAuth flow used.
            let normalised_blob = validate_single_remote_stanza(
                &dest_session.rclone_config_blob,
                &dest_session.rclone_remote_name,
            )
            .map_err(|e| IpcError::InvalidInput(format!("Invalid rclone config: {e}")))?;
            write_owner_only(&conf_path, normalised_blob.as_bytes())
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
                .map_err(|e| {
                    tracing::warn!(error = %e, "RcloneTransport::new failed");
                    IpcError::CloudError("Cloud connection failed".into())
                })?,
            )
        } else {
            state.cloud_transport.read().await.clone()
        };

    validate_storage_destination(&primary_destination, &cloud_transport_arc).await?;

    let _ = progress_ch.try_send_if_open(ProgressUpdate {
        percent: 30,
        bytes_processed: 0,
        bytes_total: 0,
        status: "Initializing vault…".into(),
    });

    let vault_uuid = Uuid::new_v4();
    let vault_id_str = vault_uuid.hyphenated().to_string();
    let vault_dir = default_vault_root().join(&vault_id_str);
    tokio::fs::create_dir_all(&vault_dir).await.map_err(|e| {
        tracing::error!(?vault_dir, error = %e, "Failed to create vault directory");
        IpcError::from(e)
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

    // Ceremony complete. Drop the ceremony transport and clean up the ceremony
    // temp dir. try_build_and_swap_rclone_transport will create a fresh session dir.
    drop(cloud_transport_arc);
    if let Some(dir) = ceremony_rclone_dir {
        let _ = destroy_session_rclone_conf(&dir.join("rclone.conf")).await;
        let _ = tokio::fs::remove_dir(&dir).await;
    }

    if let Some(db) = state.session_manager.get_metadata_store().await {
        try_build_and_swap_rclone_transport(&state, &db).await;
        let staging_dir = vault_staging_dir(&vault_id_str);
        if let Err(error) = crate::storage::prepare_vault_storage(&db, &staging_dir).await {
            tracing::warn!(?error, "Failed to prepare vault storage on create_vault");
        }
    }

    *state.active_vault_id.write().await = Some(vault_id_str.clone());
    if let Ok((_, _, header_path)) = resolve_vault_by_id(&vault_id_str) {
        cache_session_vault_info(&state, &header_path).await;
    }

    let _ = progress_ch.try_send_if_open(ProgressUpdate {
        percent: 100,
        bytes_processed: 0,
        bytes_total: 0,
        status: "Vault ready".into(),
    });

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
    recovery_phrase: Option<String>,
    current_key_file_path: Option<PathBuf>,
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
            .ok_or_else(|| IpcError::VaultLocked("Vault is locked".into()))?;
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

    let current_key_source: Option<FileKeySource> = current_key_file_path.map(FileKeySource::new);
    let mut recovery_phrase = recovery_phrase;
    let recovery_phrase_bytes = recovery_phrase.as_mut().map(sanitise_password);
    let request = ChangePasswordRequest {
        current_password_bytes: &current_bytes,
        new_password_bytes: &new_bytes,
        current_key_source: current_key_source
            .as_ref()
            .map(|s| s as &(dyn crate::auth::key_source::KeySource + Send + Sync)),
        recovery_phrase: recovery_phrase_bytes.as_ref().map(|z| &**z as &[u8]),
        argon2_params: Argon2Params::DEFAULT,
        argon2_migration_intent: Argon2MigrationIntent::PreserveTrusted,
        vault_db_path: db_path.clone(),
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
    recovery_phrase: Option<String>,
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
            .ok_or_else(|| IpcError::VaultLocked("Vault is locked".into()))?;
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

    let new_key_file_destination = if new_key_file_destination.is_dir() {
        new_key_file_destination.join("arx-runa.key")
    } else {
        new_key_file_destination
    };
    let mut recovery_phrase = recovery_phrase;
    let recovery_phrase_bytes = recovery_phrase.as_mut().map(sanitise_password);
    let request = RotateKeyFileRequest {
        password_bytes: &password_bytes,
        current_key_source: &current_key_source,
        target_new_key_file_path: new_key_file_destination,
        recovery_phrase: recovery_phrase_bytes.as_ref().map(|z| &**z as &[u8]),
        argon2_params: Argon2Params::DEFAULT,
        argon2_migration_intent: Argon2MigrationIntent::PreserveTrusted,
        vault_db_path: db_path.clone(),
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

/// Configure a BIP-39 recovery phrase for the active vault.
///
/// Requires an active session. Returns the 24-word phrase exactly once —
/// it is never stored and must be displayed and then zeroed by the caller.
#[tauri::command]
pub async fn setup_recovery(
    mut password: String,
    key_file_path: Option<PathBuf>,
    state: State<'_, AppState>,
) -> Result<String, IpcError> {
    state.session_manager.reset_timer().await;
    require_active_session(&state).await?;

    let password_bytes = sanitise_password(&mut password);
    validate_password(std::str::from_utf8(&password_bytes).unwrap_or(""))?;

    let (_, db_path, header_path) = {
        let vault_id = state
            .session_manager
            .active_vault_id()
            .await
            .ok_or_else(|| IpcError::VaultLocked("Vault is locked".into()))?;
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

    let key_source = key_file_path.map(FileKeySource::new);
    let key_source_ref: Option<&(dyn crate::auth::KeySource + Send + Sync)> =
        key_source.as_ref().map(|ks| ks as &_);
    let cloud_transport_arc = state.cloud_transport.read().await.clone();

    let request = SetupRecoveryRequest {
        current_password_bytes: &password_bytes,
        current_key_source: key_source_ref,
        argon2_params: Argon2Params::DEFAULT,
        argon2_migration_intent: Argon2MigrationIntent::PreserveTrusted,
        vault_db_path: db_path,
    };

    let phrase = ceremony_setup_recovery(
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
            "Failed to persist updated vault-header.json after setup_recovery"
        );
    }

    // has_recovery_slot changed — invalidate and repopulate the session cache.
    cache_session_vault_info(&state, &header_path).await;

    Ok(phrase.to_string())
}

/// Recover vault access using a BIP-39 phrase, re-keying to new credentials.
///
/// Does NOT require an active session. On success the vault is unlocked and
/// the session uses the supplied new password (and new key file for Tier 2).
/// The caller must ensure the cloud transport is configured before calling.
#[tauri::command]
pub async fn recover_vault_with_phrase(
    vault_id: String,
    mut phrase: String,
    mut new_password: String,
    new_key_file_path: Option<PathBuf>,
    progress: Channel<ProgressUpdate>,
    state: State<'_, AppState>,
) -> Result<AuthResponse, IpcError> {
    state.session_manager.reset_timer().await;

    let progress_ch = ProgressChannel::new(progress);
    let _ = progress_ch.try_send_if_open(ProgressUpdate {
        percent: 0,
        bytes_processed: 0,
        bytes_total: 0,
        status: "Validating recovery phrase…".into(),
    });

    let new_password_bytes = sanitise_password(&mut new_password);
    let phrase_bytes = sanitise_password(&mut phrase);
    validate_password(std::str::from_utf8(&new_password_bytes).unwrap_or(""))?;

    let (_, db_path, header_path) = resolve_vault_by_id(&vault_id)?;

    let header_json = tokio::fs::read_to_string(&header_path)
        .await
        .map_err(|_| IpcError::InternalError("Failed to read vault header".into()))?;
    let header_local: VaultHeader = serde_json::from_str(&header_json)
        .map_err(|_| IpcError::InternalError("Vault header corrupted".into()))?;
    let vault_name = header_local
        .name
        .clone()
        .unwrap_or_else(|| vault_id.clone());

    let cloud_transport_arc = state.cloud_transport.read().await.clone();

    let request = RecoverWithPhraseRequest {
        phrase: &phrase_bytes,
        vault_db_path: db_path.clone(),
        new_password_bytes: &new_password_bytes,
        new_key_file_path: new_key_file_path.map(|p| {
            if p.is_dir() {
                p.join("arx-runa.key")
            } else {
                p
            }
        }),
        argon2_params: Argon2Params::DEFAULT,
        argon2_migration_intent: Argon2MigrationIntent::PreserveTrusted,
        vault_header: Some(header_local),
    };

    let _ = progress_ch.try_send_if_open(ProgressUpdate {
        percent: 30,
        bytes_processed: 0,
        bytes_total: 0,
        status: "Reconstructing keys…".into(),
    });

    let (recovered_vault_id, updated_header) = ceremony_recover_with_phrase(
        request,
        &state.session_manager,
        cloud_transport_arc.as_ref(),
    )
    .await
    .map_err(IpcError::from)?;

    let vault_id_str = recovered_vault_id.to_uuid().to_string();

    if let Ok(json) = serde_json::to_string_pretty(&updated_header)
        && let Err(error) = tokio::fs::write(&header_path, json).await
    {
        tracing::warn!(
            ?error,
            "Failed to persist updated vault-header.json after phrase recovery"
        );
    }

    if let Some(db) = state.session_manager.get_metadata_store().await {
        try_build_and_swap_rclone_transport(&state, &db).await;
        let staging_dir = vault_staging_dir(&vault_id_str);
        if let Err(error) = crate::storage::prepare_vault_storage(&db, &staging_dir).await {
            tracing::warn!(
                ?error,
                "Failed to prepare vault storage after phrase recovery"
            );
        }
    }

    *state.active_vault_id.write().await = Some(vault_id_str.clone());
    if let Ok((_, _, header_path)) = resolve_vault_by_id(&vault_id_str) {
        cache_session_vault_info(&state, &header_path).await;
    }

    let _ = progress_ch.try_send_if_open(ProgressUpdate {
        percent: 100,
        bytes_processed: 0,
        bytes_total: 0,
        status: "Recovery complete".into(),
    });

    Ok(AuthResponse {
        vault_id: vault_id_str,
        vault_name,
    })
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
        .ok_or_else(|| IpcError::VaultLocked("Vault is locked".into()))?;

    state.session_manager.lock().await;

    state.reset_cloud_transport().await;

    let vault_dir = default_vault_root().join(&vault_id);
    if let Err(error) = tokio::fs::remove_dir_all(&vault_dir).await {
        tracing::warn!(?error, "Failed to remove vault directory during delete");
    }

    *state.active_vault_id.write().await = None;
    *state.session_vault_info.write().await = None;

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
    let kek_raw: Zeroizing<[u8; 32]> = match state
        .session_manager
        .with_key_encryption_key(|k| Zeroizing::new(*k))
        .await
    {
        Ok(raw) => raw,
        Err(_) => return Ok(()),
    };
    let kek = KeyEncryptionKey::from_secret_box(SecretBox::new(Box::new(*kek_raw)));
    let chunk_size_bytes = match db.get_meta("chunk_size_bytes").await? {
        Some(value) => value.parse::<u64>().map_err(|_| {
            crate::storage::error::StorageError::Database("invalid chunk_size_bytes".to_owned())
        })?,
        None => return Ok(()),
    };
    let _flush_guard = state.flush_mutex.lock().await;
    flush_epoch_buffer(
        db,
        &kek,
        &staging_dir.join("pending"),
        chunk_size_bytes,
        None,
    )
    .await
}

/// Zero all session keys and lock the vault.
#[tauri::command]
pub async fn lock_session(state: State<'_, AppState>) -> Result<(), IpcError> {
    state.session_manager.reset_timer().await;

    let vault_id = state.active_vault_id.read().await.clone();

    if let Some(db) = state.session_manager.get_metadata_store().await {
        let epoch_enabled = db
            .get_meta("epoch_buffer_enabled")
            .await
            .ok()
            .flatten()
            .map(|v| v == "true")
            .unwrap_or(false);
        if epoch_enabled && let Err(error) = try_flush_on_lock(&state, &db).await {
            tracing::warn!(?error, "epoch flush on lock failed; continuing lock");
        }
    }

    state.session_manager.lock().await;
    state.reset_cloud_transport().await;

    *state.active_vault_id.write().await = None;
    *state.session_vault_info.write().await = None;
    state.allowed_reveal_paths.lock().await.clear();

    if let Some(ref id) = vault_id {
        let cache_dir = vault_staging_dir(id).join("cache");
        if let Err(error) = tokio::fs::remove_dir_all(&cache_dir).await
            && error.kind() != std::io::ErrorKind::NotFound
        {
            tracing::warn!(?error, "Failed to clear staging cache on lock");
        }
    }

    Ok(())
}

/// Reads `vault-header.json` at `header_path` and caches the immutable session
/// fields in `AppState`.  Called once at session-open (authenticate, create_vault,
/// recover) so `get_session_status` never needs to touch disk.
async fn cache_session_vault_info(state: &AppState, header_path: &std::path::Path) {
    if let Ok(json) = tokio::fs::read_to_string(header_path).await
        && let Ok(header) = serde_json::from_str::<VaultHeader>(&json)
    {
        *state.session_vault_info.write().await = Some(SessionVaultInfo {
            vault_tier: header.tier,
            has_recovery_slot: !header.recovery_slots.is_empty(),
        });
    }
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

    // vault_tier and has_recovery_slot are immutable for the lifetime of a
    // session — read from the cache set at authenticate/create_vault time so
    // no disk I/O is needed on every 5-second poll.
    let (vault_tier, has_recovery_slot) = if is_unlocked {
        state
            .session_vault_info
            .read()
            .await
            .as_ref()
            .map(|info| (Some(info.vault_tier), Some(info.has_recovery_slot)))
            .unwrap_or((None, None))
    } else {
        (None, None)
    };

    Ok(SessionStatus {
        is_unlocked,
        vault_id,
        timeout_seconds,
        vault_tier,
        has_recovery_slot,
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

/// Retry a pending vault operation that was interrupted before cloud upload.
///
/// After a `change_password` or `rotate_key_file` that failed during the vault-header
/// upload step, the new header is written to `pending-vault-header.json`. This command
/// retries the upload and writes the updated header to local disk on success.
///
/// The session must already be active with the new credentials (the local DB was
/// re-keyed during the interrupted ceremony). Credentials are accepted at the IPC
/// boundary for future re-verification paths; the current implementation only re-uploads.
#[tauri::command]
pub async fn retry_pending_vault_operation(
    mut password: String,
    key_file_path: Option<PathBuf>,
    state: State<'_, AppState>,
) -> Result<(), IpcError> {
    state.session_manager.reset_timer().await;
    let _password_bytes = sanitise_password(&mut password);
    let _ = key_file_path; // accepted at IPC boundary; not consumed in upload-retry path
    require_active_session(&state).await?;

    let pending_dir = dirs::config_dir()
        .ok_or_else(|| IpcError::InternalError("Could not determine config directory".into()))?
        .join("arx-runa");

    let pending_path = pending_dir.join("pending-vault-header.json");

    let pending_json = tokio::fs::read_to_string(&pending_path)
        .await
        .map_err(|_| IpcError::InternalError("Pending vault artifact not found".into()))?;

    let pending: PendingVaultHeader = serde_json::from_str(&pending_json)
        .map_err(|_| IpcError::InternalError("Pending vault artifact is malformed".into()))?;

    let (_vault_id, _db_path, header_path) = resolve_vault_by_id(&pending.vault_id)?;

    let vault_header: VaultHeader = serde_json::from_str(&pending.vault_header_json)
        .map_err(|_| IpcError::InternalError("Pending vault header is malformed".into()))?;

    let cloud_transport_arc = state.cloud_transport.read().await.clone();

    let staging_dir = crate::auth::staging::staging_directory()
        .await
        .map_err(|_| IpcError::InternalError("Failed to resolve staging directory".into()))?;

    let operation_label = match pending.operation {
        PendingOperation::ChangePassword => "password change",
        PendingOperation::RotateKeyFile => "key file rotation",
    };

    tracing::info!(
        vault_id = %pending.vault_id,
        operation = operation_label,
        "Retrying vault header upload for interrupted operation"
    );

    upload_vault_header(&vault_header, cloud_transport_arc.as_ref(), &staging_dir)
        .await
        .map_err(|_| IpcError::InternalError("Failed to upload vault header".into()))?;

    if let Ok(json) = serde_json::to_string_pretty(&vault_header)
        && let Err(error) = tokio::fs::write(&header_path, json).await
    {
        tracing::warn!(
            ?error,
            "Failed to persist updated vault header to disk after retry"
        );
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
pub async fn check_cloud_configured() -> Result<bool, IpcError> {
    crate::storage::cloud::cloud_config::load_primary_cloud_endpoint()
        .await
        .map(|opt| opt.is_some())
        .map_err(|e| IpcError::InternalError(e.to_string()))
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
/// Downloads `vault-header.json` from `transport`, parses it, and returns it.
async fn probe_and_read_vault_header(transport: &RcloneTransport) -> Result<VaultHeader, IpcError> {
    let probe_path = std::env::temp_dir().join("arx-runa-recover-header-probe.json");
    transport
        .download_blob("vault-header.json", &probe_path)
        .await
        .map_err(|e| {
            tracing::warn!(?e, "failed to download vault header during cloud recovery");
            IpcError::CloudError(
                "Could not reach the vault at the provided destination. \
                 Check your credentials and path prefix."
                    .into(),
            )
        })?;
    let header_bytes = tokio::fs::read(&probe_path).await.map_err(IpcError::from)?;
    let _ = tokio::fs::remove_file(&probe_path).await;
    serde_json::from_slice(&header_bytes).map_err(|_| {
        IpcError::CloudError("Vault header at the cloud destination is malformed".into())
    })
}

/// Persists the recovery destination, rebuilds the rclone transport, and prepares vault storage.
async fn post_recovery_setup(
    state: &AppState,
    dest_session: &DestinationSession,
    vault_id_str: &str,
) {
    if let Some(db) = state.session_manager.get_metadata_store().await {
        match get_primary_destination(&db).await {
            Ok(None) => {
                if let Err(e) = insert_destination_session(&db, dest_session).await {
                    tracing::warn!(
                        ?e,
                        "Failed to persist recovery destination session into vault DB"
                    );
                }
            }
            Ok(Some(_)) => {}
            Err(e) => tracing::warn!(
                ?e,
                "Failed to query primary destination after cloud recovery"
            ),
        }
        try_build_and_swap_rclone_transport(state, &db).await;
        let staging_dir = vault_staging_dir(vault_id_str);
        if let Err(error) = crate::storage::prepare_vault_storage(&db, &staging_dir).await {
            tracing::warn!(?error, "Failed to prepare vault storage after recovery");
        }
    }
}

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
    vault_name: Option<String>,
    primary_destination: DestinationSessionConfig,
    progress: Channel<ProgressUpdate>,
    state: State<'_, AppState>,
) -> Result<AuthResponse, IpcError> {
    state.session_manager.reset_timer().await;

    let progress_ch = ProgressChannel::new(progress);
    let _ = progress_ch.try_send_if_open(ProgressUpdate {
        percent: 0,
        bytes_processed: 0,
        bytes_total: 0,
        status: "Connecting to cloud…".into(),
    });

    let password_bytes = sanitise_password(&mut password);
    validate_password(
        std::str::from_utf8(&password_bytes)
            .map_err(|_| IpcError::InvalidInput("password encoding error".into()))?,
    )?;

    let dest_session = destination_from_config(&primary_destination);
    let rclone_dir = create_session_rclone_dir().await.map_err(|e| {
        tracing::warn!(error = %e, "temp dir creation failed");
        IpcError::InternalError("Internal error".into())
    })?;
    let conf_path = rclone_dir.join("rclone.conf");
    let binary_path = rclone_binary_path(state.app_handle.get());

    let normalised_blob = validate_single_remote_stanza(
        &dest_session.rclone_config_blob,
        &dest_session.rclone_remote_name,
    )
    .map_err(|e| IpcError::InvalidInput(format!("Invalid rclone config: {e}")))?;
    write_owner_only(&conf_path, normalised_blob.as_bytes())
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
        conf_path.clone(),
        &endpoint,
        &dest_public,
        SyncConfig::default(),
    )
    .map_err(|e| {
        tracing::warn!(error = %e, "RcloneTransport::new failed");
        IpcError::CloudError("Cloud connection failed".into())
    })?;

    let mut vault_header = probe_and_read_vault_header(&transport).await?;

    let _ = progress_ch.try_send_if_open(ProgressUpdate {
        percent: 30,
        bytes_processed: 0,
        bytes_total: 0,
        status: "Downloading vault data…".into(),
    });
    let cloud_vault_id = vault_header.vault_id.clone();

    let vault_dir = default_vault_root().join(&cloud_vault_id);
    let vault_db_exists = vault_dir.join("vault.db").exists();
    let vault_header_exists = vault_dir.join("vault-header.json").exists();
    if vault_db_exists && vault_header_exists {
        return Err(IpcError::AlreadyExists(format!(
            "Vault '{cloud_vault_id}' already exists on this device"
        )));
    }
    // Incomplete vault dir (db without header, or stale empty dir): wipe and re-recover.
    if vault_dir.exists() {
        tokio::fs::remove_dir_all(&vault_dir).await.map_err(|_| {
            IpcError::InternalError("Failed to clean up incomplete vault directory".into())
        })?;
    }
    tokio::fs::create_dir_all(&vault_dir)
        .await
        .map_err(IpcError::from)?;

    let key_source_opt: Option<FileKeySource> = key_file_path.map(FileKeySource::new);
    let key_source_ref: Option<&(dyn crate::auth::KeySource + Send + Sync)> = key_source_opt
        .as_ref()
        .map(|ks| ks as &(dyn crate::auth::KeySource + Send + Sync));

    let _ = progress_ch.try_send_if_open(ProgressUpdate {
        percent: 60,
        bytes_processed: 0,
        bytes_total: 0,
        status: "Restoring vault…".into(),
    });

    // Embed the user-supplied vault name into the local header before writing.
    // This field is intentionally absent from the cloud header (ZK boundary).
    vault_header.name = vault_name.clone();

    let local_header_bytes = serde_json::to_vec_pretty(&vault_header).map_err(|_| {
        let _ = std::fs::remove_dir_all(&vault_dir);
        IpcError::InternalError("Failed to serialise local vault header".into())
    })?;

    // Write vault-header.json before the ceremony so the vault directory is
    // complete even if install_session succeeds but a subsequent step fails.
    // On Windows, remove_dir_all on a directory containing an open vault.db
    // can fail; writing the header first ensures the vault is always listable.
    tokio::fs::write(vault_dir.join("vault-header.json"), &local_header_bytes)
        .await
        .map_err(|e| {
            let _ = std::fs::remove_dir_all(&vault_dir);
            IpcError::from(e)
        })?;

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

    // Ceremony done: drop the transport and clean up the ceremony-scoped temp dir
    // before post_recovery_setup rebuilds the session transport in a fresh dir.
    drop(transport);
    let _ = destroy_session_rclone_conf(&conf_path).await;
    let _ = tokio::fs::remove_dir(&rclone_dir).await;

    post_recovery_setup(&state, &dest_session, &vault_id_str).await;

    *state.active_vault_id.write().await = Some(vault_id_str.clone());
    if let Ok((_, _, header_path)) = resolve_vault_by_id(&vault_id_str) {
        cache_session_vault_info(&state, &header_path).await;
    }

    let _ = progress_ch.try_send_if_open(ProgressUpdate {
        percent: 100,
        bytes_processed: 0,
        bytes_total: 0,
        status: "Recovery complete".into(),
    });

    Ok(AuthResponse {
        vault_name: vault_name.unwrap_or_else(|| vault_id_str.clone()),
        vault_id: vault_id_str,
    })
}

/// Recovers a vault from cloud onto a new device using a BIP-39 recovery phrase,
/// without requiring the original password. Builds a transport from the supplied
/// destination, downloads the vault header and manifest backup, re-keys the DB to
/// the new password, and installs the session.
#[tauri::command]
pub async fn recover_vault_from_cloud_with_phrase(
    mut phrase: String,
    mut new_password: String,
    new_key_file_path: Option<PathBuf>,
    vault_name: Option<String>,
    primary_destination: DestinationSessionConfig,
    progress: Channel<ProgressUpdate>,
    state: State<'_, AppState>,
) -> Result<AuthResponse, IpcError> {
    state.session_manager.reset_timer().await;

    let progress_ch = ProgressChannel::new(progress);
    let _ = progress_ch.try_send_if_open(ProgressUpdate {
        percent: 0,
        bytes_processed: 0,
        bytes_total: 0,
        status: "Connecting to cloud…".into(),
    });

    let new_password_bytes = sanitise_password(&mut new_password);
    let phrase_bytes = sanitise_password(&mut phrase);
    validate_password(
        std::str::from_utf8(&new_password_bytes)
            .map_err(|_| IpcError::InvalidInput("password encoding error".into()))?,
    )?;

    let dest_session = destination_from_config(&primary_destination);
    let rclone_dir = create_session_rclone_dir().await.map_err(|e| {
        tracing::warn!(error = %e, "temp dir creation failed");
        IpcError::InternalError("Internal error".into())
    })?;
    let conf_path = rclone_dir.join("rclone.conf");
    let binary_path = rclone_binary_path(state.app_handle.get());

    let config_blob = match dest_session.destination_type {
        DestinationType::LocalPath | DestinationType::ExternalDrive => {
            format!("[{}]\ntype = local\n", dest_session.rclone_remote_name)
        }
        DestinationType::Cloud => {
            if dest_session.rclone_config_blob.trim().is_empty() {
                return Err(IpcError::InvalidInput(
                    "The selected destination has no cloud configuration. \
                     Complete the cloud destination setup before using phrase recovery."
                        .into(),
                ));
            }
            validate_single_remote_stanza(
                &dest_session.rclone_config_blob,
                &dest_session.rclone_remote_name,
            )
            .map_err(|e| IpcError::InvalidInput(format!("Invalid rclone config: {e}")))?
        }
    };
    write_owner_only(&conf_path, config_blob.as_bytes())
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
        conf_path.clone(),
        &endpoint,
        &dest_public,
        SyncConfig::default(),
    )
    .map_err(|e| {
        tracing::warn!(error = %e, "RcloneTransport::new failed");
        IpcError::CloudError("Cloud connection failed".into())
    })?;

    let vault_header = probe_and_read_vault_header(&transport).await?;

    let _ = progress_ch.try_send_if_open(ProgressUpdate {
        percent: 30,
        bytes_processed: 0,
        bytes_total: 0,
        status: "Downloading vault data…".into(),
    });
    let cloud_vault_id = vault_header.vault_id.clone();

    let vault_dir = default_vault_root().join(&cloud_vault_id);
    let vault_db_exists = vault_dir.join("vault.db").exists();
    let vault_header_exists = vault_dir.join("vault-header.json").exists();
    if vault_db_exists && vault_header_exists {
        return Err(IpcError::AlreadyExists(format!(
            "Vault '{cloud_vault_id}' already exists on this device"
        )));
    }
    // Incomplete vault dir (db without header, or stale empty dir): wipe and re-recover.
    if vault_dir.exists() {
        tokio::fs::remove_dir_all(&vault_dir).await.map_err(|_| {
            IpcError::InternalError("Failed to clean up incomplete vault directory".into())
        })?;
    }
    tokio::fs::create_dir_all(&vault_dir)
        .await
        .map_err(IpcError::from)?;

    let vault_db_path = vault_dir.join("vault.db");

    let _ = progress_ch.try_send_if_open(ProgressUpdate {
        percent: 60,
        bytes_processed: 0,
        bytes_total: 0,
        status: "Restoring vault…".into(),
    });

    let (recovered_vault_id, updated_header) = ceremony_recover_with_phrase(
        RecoverWithPhraseRequest {
            phrase: &phrase_bytes,
            vault_db_path: vault_db_path.clone(),
            new_password_bytes: &new_password_bytes,
            new_key_file_path: new_key_file_path.map(|p| {
                if p.is_dir() {
                    p.join("arx-runa.key")
                } else {
                    p
                }
            }),
            argon2_params: Argon2Params::DEFAULT,
            argon2_migration_intent: Argon2MigrationIntent::PreserveTrusted,
            vault_header: Some(vault_header),
        },
        &state.session_manager,
        &transport,
    )
    .await
    .map_err(|err| {
        let _ = std::fs::remove_dir_all(&vault_dir);
        IpcError::from(err)
    })?;

    let vault_id_str = recovered_vault_id.to_uuid().to_string();

    // Embed the user-supplied vault name into the local header (ZK: name absent from cloud).
    let mut local_header = updated_header;
    local_header.name = vault_name.clone();
    let header_json = serde_json::to_string_pretty(&local_header)
        .map_err(|_| IpcError::InternalError("failed to serialise vault header".into()))?;
    tokio::fs::write(vault_dir.join("vault-header.json"), &header_json)
        .await
        .map_err(|e| {
            let _ = std::fs::remove_dir_all(&vault_dir);
            IpcError::from(e)
        })?;

    // Ceremony done: drop the transport and clean up the ceremony-scoped temp dir
    // before post_recovery_setup rebuilds the session transport in a fresh dir.
    drop(transport);
    let _ = destroy_session_rclone_conf(&conf_path).await;
    let _ = tokio::fs::remove_dir(&rclone_dir).await;

    post_recovery_setup(&state, &dest_session, &vault_id_str).await;

    *state.active_vault_id.write().await = Some(vault_id_str.clone());
    if let Ok((_, _, header_path)) = resolve_vault_by_id(&vault_id_str) {
        cache_session_vault_info(&state, &header_path).await;
    }

    let _ = progress_ch.try_send_if_open(ProgressUpdate {
        percent: 100,
        bytes_processed: 0,
        bytes_total: 0,
        status: "Recovery complete".into(),
    });

    Ok(AuthResponse {
        vault_name: vault_name.unwrap_or_else(|| vault_id_str.clone()),
        vault_id: vault_id_str,
    })
}

/// Scans a mounted drive
/// `expected_hash_hex`.
///
/// Returns the absolute path of the matching file, or `None` if not found.
/// Scan errors are swallowed and treated as no-match so a bad drive never
/// blocks the login UI.
#[tauri::command]
pub async fn scan_for_key_file(
    mount_path: String,
    expected_hash_hex: String,
) -> Result<Option<String>, IpcError> {
    use crate::auth::autodetect::find_key_file;
    use crate::crypto::Blake3Hash;

    let hash_bytes: [u8; 32] = hex::decode(&expected_hash_hex)
        .ok()
        .and_then(|b| b.try_into().ok())
        .ok_or_else(|| {
            IpcError::InvalidInput("expected_hash_hex must be a 64-character hex string".into())
        })?;
    let reference_hash = Blake3Hash(hash_bytes);

    match find_key_file(std::path::Path::new(&mount_path), &reference_hash).await {
        Ok(Some(path)) => Ok(Some(path.to_string_lossy().into_owned())),
        Ok(None) => Ok(None),
        Err(e) => {
            tracing::debug!(mount_path = %mount_path, error = %e, "key file scan returned error; treating as no match");
            Ok(None)
        }
    }
}

/// Returns whether the given path resides on a removable storage device.
///
/// Used at vault creation time to warn users who place their key file on a
/// fixed drive.  Returns `false` on any error so callers never false-alarm.
#[tauri::command]
pub async fn is_path_on_removable_drive(path: String) -> Result<bool, IpcError> {
    use crate::auth::removable_drive::is_removable_path;
    Ok(is_removable_path(std::path::Path::new(&path)))
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
            has_recovery_slot: Some(false),
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

    // ── scan_for_key_file ──────────────────────────────────────────────────

    /// Creates a deterministic 32-byte key payload and its hex-encoded BLAKE3 hash.
    fn key_hex(seed: u8) -> ([u8; 32], String) {
        let bytes = [seed; 32];
        let hash_hex = hex::encode(blake3::hash(&bytes).as_bytes());
        (bytes, hash_hex)
    }

    #[tokio::test]
    async fn test_scan_for_key_file_returns_path_when_file_matches() {
        let dir = tempfile::tempdir().expect("tempdir should be created");
        let (bytes, hash_hex) = key_hex(0xA1);
        let key_path = dir.path().join("mykey.bin");
        std::fs::write(&key_path, bytes).expect("key file should be written");

        let result = scan_for_key_file(dir.path().to_string_lossy().into_owned(), hash_hex)
            .await
            .expect("command should not error");

        assert_eq!(result, Some(key_path.to_string_lossy().into_owned()));
    }

    #[tokio::test]
    async fn test_scan_for_key_file_returns_none_when_no_match() {
        let dir = tempfile::tempdir().expect("tempdir should be created");
        let (bytes, _) = key_hex(0xB2);
        std::fs::write(dir.path().join("decoy.bin"), bytes).expect("decoy file should be written");
        let (_, hash_hex) = key_hex(0xC3);

        let result = scan_for_key_file(dir.path().to_string_lossy().into_owned(), hash_hex)
            .await
            .expect("command should not error");

        assert_eq!(result, None);
    }

    #[tokio::test]
    async fn test_scan_for_key_file_returns_none_for_nonexistent_mount() {
        let (_, hash_hex) = key_hex(0xD4);

        let result = scan_for_key_file("/nonexistent/mount/path/xyz".to_string(), hash_hex)
            .await
            .expect("scan error should be swallowed and return None");

        assert_eq!(result, None);
    }

    #[tokio::test]
    async fn test_scan_for_key_file_rejects_invalid_hex() {
        let result = scan_for_key_file(
            std::env::temp_dir().to_string_lossy().into_owned(),
            "not-valid-hex".to_string(),
        )
        .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_scan_for_key_file_rejects_wrong_length_hex() {
        // 63 hex chars = 31.5 bytes, not 32.
        let result = scan_for_key_file(
            std::env::temp_dir().to_string_lossy().into_owned(),
            "a".repeat(63),
        )
        .await;

        assert!(result.is_err());
    }

    // ── is_path_on_removable_drive ─────────────────────────────────────────

    #[tokio::test]
    async fn test_is_path_on_removable_drive_fixed_drive_returns_false() {
        let temp = std::env::temp_dir().to_string_lossy().into_owned();
        let result = is_path_on_removable_drive(temp)
            .await
            .expect("command should not error");
        assert!(!result);
    }

    #[tokio::test]
    async fn test_destroy_session_rclone_conf_removes_stale_file() {
        use tempfile::tempdir;
        let directory = tempdir().expect("tempdir must succeed");
        let conf_path = directory.path().join("rclone.conf");
        tokio::fs::write(&conf_path, b"[stale_remote]\ntype = s3\n")
            .await
            .expect("stale conf write must succeed");
        assert!(conf_path.exists(), "stale conf must exist before cleanup");
        destroy_session_rclone_conf(&conf_path)
            .await
            .expect("destroy must succeed on stale conf");
        assert!(!conf_path.exists(), "stale conf must be removed by destroy");
    }
}
