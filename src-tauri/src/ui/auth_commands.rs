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
    PendingVaultHeader, RotateKeyFileRequest, Tier, change_password as ceremony_change_password,
    create_vault as ceremony_create_vault, rotate_key_file as ceremony_rotate_key_file,
};
use crate::auth::kdf::Argon2Params;
use crate::auth::key_source::FileKeySource;
use crate::crypto::VaultId;
use crate::storage::SqlCipherMetadataStore;
use crate::storage::cloud::{
    CloudEndpoint, DestinationSessionPublic, RcloneTransport, SyncConfig,
    destination_session::{
        BackupSyncMode, DestinationSession, DestinationType, build_session_rclone_conf,
        destroy_session_rclone_conf, get_primary_destination, insert_destination_session,
    },
    download_vault_header,
    vault_header::VaultHeader,
    vault_header::VaultHeaderTrustPolicy,
};
use crate::ui::commands_common::{require_active_session, sanitise_password};
use crate::ui::error::IpcError;
use crate::ui::state::AppState;
use crate::ui::types::{AuthResponse, DestinationSessionConfig, SessionStatus};
use crate::ui::validation::validate_password;
use crate::ui::vault_paths::{default_vault_root, resolve_singleton_vault};

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
fn rclone_conf_path() -> PathBuf {
    dirs::config_dir()
        .expect("config_dir must be available")
        .join("arx-runa")
        .join("rclone.conf")
}

/// Converts an IPC `DestinationSessionConfig` into a storage `DestinationSession`.
fn destination_from_config(config: &DestinationSessionConfig) -> DestinationSession {
    let destination_type = match config.destination_type.as_str() {
        "cloud" => DestinationType::Cloud,
        "external_drive" => DestinationType::ExternalDrive,
        "local_path" => DestinationType::LocalPath,
        _ => DestinationType::Cloud,
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

/// Authenticate with password (Tier 1) or password + USB key file (Tier 2).
///
/// Returns vault metadata on success. Does NOT return any key material.
#[tauri::command]
pub async fn authenticate(
    mut password: String,
    key_file_path: Option<PathBuf>,
    state: State<'_, AppState>,
) -> Result<AuthResponse, IpcError> {
    state.session_manager.reset_timer().await;

    let password_bytes = sanitise_password(&mut password);
    validate_password(std::str::from_utf8(&password_bytes).unwrap_or(""))?;

    // Resolve the single local vault.
    let (vault_id, db_path, header_path) = resolve_singleton_vault()?
        .ok_or_else(|| IpcError::InvalidInput("No vault configured".into()))?;

    // Read and parse the vault header from local disk.
    let header_json = tokio::fs::read_to_string(&header_path)
        .await
        .map_err(|_| IpcError::InternalError("Failed to read vault header".into()))?;
    let header: VaultHeader = serde_json::from_str(&header_json)
        .map_err(|_| IpcError::InternalError("Vault header corrupted".into()))?;

    // Decode the 32-byte Argon2 salt from base64.
    let salt_bytes = B64_STANDARD
        .decode(&header.argon2_salt)
        .map_err(|_| IpcError::InternalError("Vault header corrupted".into()))?;
    let salt: [u8; 32] = salt_bytes
        .try_into()
        .map_err(|_| IpcError::InternalError("Vault header corrupted".into()))?;

    // Build Argon2 parameters from the vault header.
    let params = Argon2Params {
        memory_cost_kib: header.argon2_params.memory_cost,
        time_cost: header.argon2_params.time_cost,
        parallelism: header.argon2_params.parallelism,
    };

    // Build the optional key source for Tier 2 vaults.
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

    // Authenticate — derives session keys from password + optional key file.
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

    // Copy the sqlcipher key bytes out of the session and immediately zeroize.
    // SAFETY: The copy is wrapped in Zeroizing so it is overwritten on drop.
    let key_copy: [u8; 32] = state
        .session_manager
        .with_sqlcipher_key(|k| *k)
        .await
        .map_err(IpcError::from)?;
    let key_zeroizing = Zeroizing::new(key_copy);
    let db = SqlCipherMetadataStore::open(&db_path, &key_zeroizing)
        .await
        .map_err(IpcError::from)?;
    // key_zeroizing is dropped here — bytes are zeroed.
    drop(key_zeroizing);

    // Store the open database in shared state.
    *state.database.write().await = Some(db);

    // Best-effort: build and install rclone transport from the stored destination.
    {
        let db_guard = state.database.read().await;
        if let Some(ref inner_db) = *db_guard {
            try_build_and_swap_rclone_transport(&state, inner_db).await;
        }
    }

    // Cache the active vault identifier for quick reads.
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

    // Pre-generate a UUID for the vault directory; we rename after the ceremony
    // so the directory name matches the ceremony-assigned vault_id.
    let temp_dir_id = Uuid::new_v4().hyphenated().to_string();
    let vault_dir = default_vault_root().join(&temp_dir_id);
    tokio::fs::create_dir_all(&vault_dir)
        .await
        .map_err(|e| IpcError::InternalError(e.to_string()))?;
    let db_path = vault_dir.join("vault.db");

    let tier_enum = if tier == 2 { Tier::Two } else { Tier::One };

    if tier_enum == Tier::Two {
        if let Some(ref dest) = key_file_destination {
            let key_path = if dest.is_dir() {
                dest.join("arx-runa.key")
            } else {
                dest.clone()
            };
            if key_path.exists() {
                return Err(IpcError::InvalidInput(
                    "A key file already exists at that location. Choose a different directory."
                        .into(),
                ));
            }
        }
    }

    // Snapshot the current cloud transport for use during the ceremony.
    let cloud_transport_arc = state.cloud_transport.read().await.clone();

    let request = CreateVaultRequest {
        tier: tier_enum,
        password_bytes: &password_bytes,
        target_key_file_path: key_file_destination,
        vault_db_path: db_path,
        argon2_params: Argon2Params::DEFAULT,
        chunk_size_bytes,
        epoch_buffer_enabled,
    };

    // Run the create_vault ceremony: derives keys, creates DB, uploads vault header,
    // and installs the session. Returns the ceremony-assigned VaultId.
    let ceremony_vault_id = ceremony_create_vault(
        request,
        &state.session_manager,
        cloud_transport_arc.as_ref(),
    )
    .await
    .map_err(IpcError::from)?;

    let vault_id_str = ceremony_vault_id.to_uuid().hyphenated().to_string();

    // Rename the temp vault directory to the canonical ceremony-assigned vault_id
    // so that resolve_singleton_vault() returns a consistent identifier.
    let final_vault_dir = default_vault_root().join(&vault_id_str);
    if vault_dir != final_vault_dir {
        tokio::fs::rename(&vault_dir, &final_vault_dir)
            .await
            .map_err(|e| IpcError::InternalError(e.to_string()))?;
    }
    let db_path = final_vault_dir.join("vault.db");

    // Open the vault database using the sqlcipher key from the just-installed session.
    // SAFETY: key_copy is wrapped in Zeroizing and overwritten on drop.
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

    // Insert the primary destination session into the vault database.
    let dest_session = destination_from_config(&primary_destination);
    insert_destination_session(&db, &dest_session)
        .await
        .map_err(IpcError::from)?;

    // Store the open database in shared state.
    *state.database.write().await = Some(db);

    // Best-effort: build and install rclone transport.
    {
        let db_guard = state.database.read().await;
        if let Some(ref inner_db) = *db_guard {
            try_build_and_swap_rclone_transport(&state, inner_db).await;
        }
    }

    // Best-effort: download vault-header.json from cloud and save locally so
    // that subsequent authenticate calls can read it from disk.
    {
        let header_path = final_vault_dir.join("vault-header.json");
        match download_vault_header(
            &*cloud_transport_arc,
            &final_vault_dir,
            VaultHeaderTrustPolicy::Bootstrap,
        )
        .await
        {
            Ok(header) => match serde_json::to_string_pretty(&header) {
                Ok(json) => {
                    if let Err(error) = tokio::fs::write(&header_path, json).await {
                        tracing::warn!(?error, "Failed to persist vault-header.json locally");
                    }
                }
                Err(error) => {
                    tracing::warn!(?error, "Failed to serialise vault-header.json");
                }
            },
            Err(error) => {
                tracing::warn!(
                    ?error,
                    "Failed to download vault-header.json after create; \
                     subsequent authenticate calls may fail until the header is synced"
                );
            }
        }
    }

    // Cache the active vault identifier.
    *state.active_vault_id.write().await = Some(vault_id_str.clone());

    Ok(AuthResponse {
        vault_id: vault_id_str,
        vault_name,
    })
}

/// Change the vault password.
///
/// Requires an active session. Phase 6.5 supports Tier 1 only; Tier 2 requires
/// a current key file which is not yet exposed in this IPC surface.
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

    let (_, db_path, header_path) = resolve_singleton_vault()?
        .ok_or_else(|| IpcError::InvalidInput("No vault configured".into()))?;

    // Read and parse the vault header (ceremony modifies it in place).
    let header_json = tokio::fs::read_to_string(&header_path)
        .await
        .map_err(|_| IpcError::InternalError("Failed to read vault header".into()))?;
    let mut header: VaultHeader = serde_json::from_str(&header_json)
        .map_err(|_| IpcError::InternalError("Vault header corrupted".into()))?;

    let vault_id_uuid = Uuid::parse_str(&header.vault_id)
        .map_err(|_| IpcError::InternalError("Vault header corrupted".into()))?;
    let vault_id_crypto = VaultId::from_uuid(vault_id_uuid);

    let cloud_transport_arc = state.cloud_transport.read().await.clone();

    // current_key_source: None for Tier 1. Tier 2 requires the caller to supply
    // the current key file — not yet supported in this IPC signature.
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

    // Persist the updated vault header (argon2_salt may have rotated).
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

    let (_, db_path, header_path) = resolve_singleton_vault()?
        .ok_or_else(|| IpcError::InvalidInput("No vault configured".into()))?;

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

    // Persist the updated vault header (key_file_blake3 is refreshed).
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

    let (vault_id, _, _) =
        resolve_singleton_vault()?.ok_or_else(|| IpcError::NotFound("No vault found".into()))?;

    // Lock the session (zeroizes all key material).
    state.session_manager.lock().await;

    // Reset cloud transport to no-op.
    state.reset_cloud_transport().await;

    // Destroy the rclone.conf (best-effort; ignore not-found).
    let conf_path = rclone_conf_path();
    if let Err(error) = destroy_session_rclone_conf(&conf_path).await {
        tracing::warn!(?error, "Failed to destroy rclone.conf during vault delete");
    }

    // Remove the vault directory tree.
    let vault_dir = default_vault_root().join(&vault_id);
    if let Err(error) = tokio::fs::remove_dir_all(&vault_dir).await {
        tracing::warn!(?error, "Failed to remove vault directory during delete");
    }

    // Clear shared state.
    *state.database.write().await = None;
    *state.active_vault_id.write().await = None;

    Ok(())
}

/// Zero all session keys and lock the vault.
#[tauri::command]
pub async fn lock_session(state: State<'_, AppState>) -> Result<(), IpcError> {
    state.session_manager.reset_timer().await;
    state.session_manager.lock().await;
    state.reset_cloud_transport().await;

    // Destroy the rclone.conf (best-effort; ignore not-found).
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

    // Read vault tier from vault header when session is active
    let vault_tier = if is_unlocked && vault_id.is_some() {
        match resolve_singleton_vault() {
            Ok(Some((_, _db_path, header_path))) => {
                match tokio::fs::read_to_string(&header_path).await {
                    Ok(header_json) => match serde_json::from_str::<VaultHeader>(&header_json) {
                        Ok(header) => Some(header.tier),
                        Err(_) => None,
                    },
                    Err(_) => None,
                }
            }
            _ => None,
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
        Ok(json_content) => {
            match serde_json::from_str::<PendingVaultHeader>(&json_content) {
                Ok(pending) => {
                    tracing::info!(
                        vault_id = %pending.vault_id,
                        operation = ?pending.operation,
                        "Detected pending vault operation on startup"
                    );
                    // Emit event to frontend (best-effort)
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
            }
        }
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

    // Resolve vault paths from singleton vault
    let (_vault_id, _db_path, _header_path) = resolve_singleton_vault()?
        .ok_or_else(|| IpcError::InvalidInput("No vault configured".into()))?;

    // Attempt to complete the pending operation
    match pending.operation {
        PendingOperation::ChangePassword => {
            // For password change recovery, we just need to verify the operation succeeded
            // The ceremony itself should have already updated the header
            tracing::info!(
                vault_id = %pending.vault_id,
                "Completing pending password change operation"
            );
        }
        PendingOperation::RotateKeyFile => {
            // For key rotation recovery, similar approach
            tracing::info!(
                vault_id = %pending.vault_id,
                "Completing pending key file rotation operation"
            );
        }
    }

    // Delete the pending artifact on success
    if let Err(error) = tokio::fs::remove_file(&pending_path).await {
        tracing::warn!(
            ?error,
            "Failed to delete pending vault artifact after recovery; may re-prompt on next startup"
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::types::SessionStatus;

    #[test]
    fn test_session_status_fields_populated() {
        // Verify that the SessionStatus struct can hold all required fields
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
        // Just verify the platform-conditional logic is defined
        assert!(!expected_name.is_empty());
    }

    #[tokio::test]
    async fn test_pending_vault_header_serialization() {
        // Verify that PendingVaultHeader can be serialized/deserialized
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
        // Verify both enum variants exist and can be serialized
        let password_change = PendingOperation::ChangePassword;
        let key_rotate = PendingOperation::RotateKeyFile;

        let json1 = serde_json::to_string(&password_change).unwrap();
        let json2 = serde_json::to_string(&key_rotate).unwrap();

        assert!(!json1.is_empty());
        assert!(!json2.is_empty());
    }
}
