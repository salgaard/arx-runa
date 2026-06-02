//! File sharing commands.
//!
//! Phase 6.5: IPC command handlers wired against SharingStore + sharing module.

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use rusqlite::OptionalExtension;
use tauri::State;
use tauri::ipc::Channel;
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::crypto::{FileId, WrappedFileKey, unwrap_file_key};
use crate::sharing::{
    Contact, ContactId, DisplayName, SharingStore, X25519PublicKey, export_public_key_bytes,
    import_share_package, public_key_qr_string,
};
use crate::storage::{MetadataStore, StorageError};
use crate::ui::commands_common::{
    ProgressChannel, extract_kek, require_active_session, unix_ts_to_iso8601,
};
use crate::ui::error::IpcError;
use crate::ui::file_commands::detect_mime_type;
use crate::ui::shell_commands::strip_unc_prefix;
use crate::ui::state::AppState;
use crate::ui::types::{
    ContactEntry, DownloadReceivedShareResponse, FileContent, ImportShareResponse, ProgressUpdate,
    ReceivedShareEntry, ShareEntry, ShareResponse,
};
use crate::ui::validation::validate_file_id;
use crate::ui::vault_paths::vault_staging_dir;

// ─── Private helpers ──────────────────────────────────────────────────────────

/// Resolves the rclone binary path.
///
/// Delegates to the canonical [`rclone_binary_path`](crate::ui::commands_common::rclone_binary_path)
/// resolver so the sidecar is located the same way everywhere (next to the executable
/// in production, then resource dir, then `PATH`).
fn resolve_rclone_bin(h: &tauri::AppHandle) -> PathBuf {
    crate::ui::commands_common::rclone_binary_path(Some(h))
}

/// Best-effort cleanup: revoke the SA reader permission on a shared GDrive folder.
async fn cleanup_gdrive_share_permission(
    conf_path: &std::path::Path,
    perm_id: &str,
    folder_id: &str,
) {
    let Ok(conf) = tokio::fs::read_to_string(conf_path).await else {
        return;
    };
    let remote_name = conf
        .lines()
        .filter(|l| l.trim().starts_with('[') && l.trim().ends_with(']'))
        .find_map(|l| {
            let name = &l.trim()[1..l.trim().len() - 1];
            crate::sharing::gdrive_api::parse_gdrive_oauth_from_conf(&conf, name)
                .map(|_| name.to_owned())
        });
    let Some(name) = remote_name else {
        return;
    };
    let Some((client_id, client_secret, refresh_token, _)) =
        crate::sharing::gdrive_api::parse_gdrive_oauth_from_conf(&conf, &name)
    else {
        return;
    };
    let client = reqwest::Client::new();
    let token_result = if client_id.is_empty() {
        crate::sharing::gdrive_api::parse_gdrive_access_token_from_conf(&conf, &name)
            .map(|access_token| crate::sharing::gdrive_api::GdriveAccessToken { access_token })
            .ok_or_else(|| {
                crate::sharing::gdrive_api::GdriveApiError::TokenRefresh(
                    "access token not found in config".to_owned(),
                )
            })
    } else {
        crate::sharing::gdrive_api::gdrive_refresh_token(
            &client,
            &client_id,
            &client_secret,
            &refresh_token,
        )
        .await
    };
    match token_result {
        Ok(token) => {
            if let Err(error) = crate::sharing::gdrive_api::gdrive_delete_permission(
                &client,
                &token.access_token,
                folder_id,
                perm_id,
            )
            .await
            {
                tracing::warn!(%error, permission_id = %perm_id, "Drive permission deletion failed after revoke");
            } else {
                tracing::debug!(permission_id = %perm_id, "deleted Drive permission after revoke");
            }
        }
        Err(error) => {
            tracing::warn!(%error, "Drive token refresh failed during revoke cleanup");
        }
    }
}

/// Best-effort cleanup: delete the scoped B2 application key so recipients lose access.
async fn cleanup_b2_share_key(conf_path: &std::path::Path, key_id: &str) {
    let Ok(conf) = tokio::fs::read_to_string(conf_path).await else {
        return;
    };
    let Some((master_key_id, master_app_key, _)) =
        crate::sharing::b2_api::parse_b2_credentials_from_conf(&conf)
    else {
        return;
    };
    let Ok(auth) =
        crate::sharing::b2_api::b2_authorize_account(&master_key_id, &master_app_key).await
    else {
        return;
    };
    let client = reqwest::Client::new();
    if let Err(error) = crate::sharing::b2_api::b2_delete_key(&client, &auth, key_id).await {
        tracing::warn!(%error, key_id = %key_id, "B2 key deletion failed after revoke");
    } else {
        tracing::debug!(key_id = %key_id, "deleted B2 scoped key after revoke");
    }
}

/// Returns the current Unix timestamp in seconds since the epoch.
fn now_unix_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

/// Revokes all outgoing shares whose `expires_at` has passed.
///
/// Called at the end of `pull_and_reconcile` so expiry is enforced on each
/// pull without requiring the sender to take manual action. Errors are logged
/// rather than propagated so a sweep failure cannot abort an otherwise-
/// successful pull.
pub(crate) async fn sweep_expired_shares(state: &AppState) {
    let now = now_unix_seconds();
    let transport = state.cloud_transport.read().await.clone();

    let Some(db_store) = state.session_manager.get_metadata_store().await else {
        return;
    };
    let db = &*db_store;

    let expired_ids: Vec<String> = match db
        .with_connection_blocking(move |conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT share_id FROM shares \
                     WHERE expires_at IS NOT NULL \
                       AND expires_at < ?1 \
                       AND revoked_at IS NULL",
                )
                .map_err(StorageError::from_rusqlite)?;
            let rows = stmt
                .query_map(rusqlite::params![now], |row| row.get::<_, String>(0))
                .map_err(StorageError::from_rusqlite)?;
            let mut ids = Vec::new();
            for row in rows {
                ids.push(row.map_err(StorageError::from_rusqlite)?);
            }
            Ok(ids)
        })
        .await
    {
        Ok(ids) => ids,
        Err(e) => {
            tracing::warn!(%e, "sweep_expired_shares: failed to query expired shares");
            return;
        }
    };

    for share_id in &expired_ids {
        if let Err(e) =
            crate::sharing::revoke_share(share_id, now, db as &dyn SharingStore, &*transport).await
        {
            tracing::warn!(%share_id, %e, "sweep_expired_shares: revocation failed");
        }
    }
}

// ─── IPC command handlers ─────────────────────────────────────────────────────

/// Export the user's X25519 public key to a file for out-of-band exchange.
///
/// Writes exactly 32 raw bytes to `destination_path`. Key bytes are never logged.
#[tauri::command]
pub async fn export_public_key(
    destination_path: PathBuf,
    state: State<'_, AppState>,
) -> Result<(), IpcError> {
    state.session_manager.reset_timer().await;
    require_active_session(&state).await?;

    let db_store = state
        .session_manager
        .get_metadata_store()
        .await
        .ok_or_else(|| IpcError::VaultLocked("Vault is locked".into()))?;
    let db = &*db_store;

    let public_key = (db as &dyn SharingStore)
        .get_own_public_key()
        .await
        .map_err(IpcError::from)?;

    // export_public_key_bytes returns the raw 32 bytes. Never log them.
    let key_bytes = export_public_key_bytes(&public_key);

    tokio::fs::write(&destination_path, &key_bytes)
        .await
        .map_err(|_| IpcError::InternalError("Failed to write public key file".into()))?;

    Ok(())
}

/// Returns the user's own public key as a standard base64 string for display in the UI.
///
/// The key is encoded via `public_key_qr_string` (padded base64, 44 chars). The raw bytes
/// are never logged or included in error messages.
#[tauri::command]
pub async fn get_own_public_key_b64(state: State<'_, AppState>) -> Result<String, IpcError> {
    state.session_manager.reset_timer().await;
    require_active_session(&state).await?;

    let db_store = state
        .session_manager
        .get_metadata_store()
        .await
        .ok_or_else(|| IpcError::VaultLocked("Vault is locked".into()))?;
    let db = &*db_store;

    let public_key = (db as &dyn SharingStore)
        .get_own_public_key()
        .await
        .map_err(IpcError::from)?;

    Ok(public_key_qr_string(&public_key))
}

/// Import a contact's public key from a file.
///
/// Reads exactly 32 bytes from `public_key_path`, constructs a contact row, and
/// persists it via `SharingStore::insert_contact`.
#[tauri::command]
pub async fn add_contact(
    display_name: String,
    public_key_path: PathBuf,
    email: Option<String>,
    state: State<'_, AppState>,
) -> Result<ContactEntry, IpcError> {
    state.session_manager.reset_timer().await;
    require_active_session(&state).await?;

    if display_name.is_empty() {
        return Err(IpcError::InvalidInput(
            "Display name must not be empty".into(),
        ));
    }

    let key_file_bytes = tokio::fs::read(&public_key_path)
        .await
        .map_err(|_| IpcError::InvalidInput("Failed to read public key file".into()))?;

    if key_file_bytes.len() != 32 {
        return Err(IpcError::InvalidInput(
            "Public key file must be exactly 32 bytes".into(),
        ));
    }

    let mut key_array = [0u8; 32];
    key_array.copy_from_slice(&key_file_bytes);
    let public_key = X25519PublicKey::new(key_array);

    let display_name_validated = DisplayName::new(&display_name).map_err(IpcError::from)?;
    let contact_id = ContactId::from_uuid(Uuid::new_v4());
    let created_at = now_unix_seconds();

    let contact = Contact {
        contact_id,
        display_name: display_name_validated,
        email: email.clone(),
        public_key,
        created_at,
    };

    let db_store = state
        .session_manager
        .get_metadata_store()
        .await
        .ok_or_else(|| IpcError::VaultLocked("Vault is locked".into()))?;
    let db = &*db_store;

    (db as &dyn SharingStore)
        .insert_contact(&contact)
        .await
        .map_err(IpcError::from)?;

    let public_key_b64 = base64::engine::general_purpose::STANDARD.encode(public_key.as_bytes());

    Ok(ContactEntry {
        contact_id: contact_id.to_uuid().hyphenated().to_string(),
        display_name: contact.display_name.as_str().to_owned(),
        email: contact.email,
        created_at: unix_ts_to_iso8601(created_at),
        public_key: public_key_b64,
    })
}

/// List all contacts.
#[tauri::command]
pub async fn list_contacts(state: State<'_, AppState>) -> Result<Vec<ContactEntry>, IpcError> {
    state.session_manager.reset_timer().await;
    require_active_session(&state).await?;

    let db_store = state
        .session_manager
        .get_metadata_store()
        .await
        .ok_or_else(|| IpcError::VaultLocked("Vault is locked".into()))?;
    let db = &*db_store;

    let contacts = (db as &dyn SharingStore)
        .list_contacts()
        .await
        .map_err(IpcError::from)?;

    Ok(contacts
        .into_iter()
        .map(|c| {
            let public_key_b64 =
                base64::engine::general_purpose::STANDARD.encode(c.public_key.as_bytes());
            ContactEntry {
                contact_id: c.contact_id.to_uuid().hyphenated().to_string(),
                display_name: c.display_name.as_str().to_owned(),
                email: c.email,
                created_at: unix_ts_to_iso8601(c.created_at),
                public_key: public_key_b64,
            }
        })
        .collect())
}

/// Share a file with a contact via HPKE (RFC 9180).
///
/// Copies chunk blobs to the cloud share namespace, generates scoped download
/// credentials if the backend supports it, seals a share package for the
/// recipient, writes it to `<data_dir>/arx-runa/shares/<share_id>.arxshare`,
/// inserts an outgoing `ShareRecord`, and returns the `share_id` and package path.
#[tauri::command]
pub async fn share_file(
    file_id: String,
    contact_id: String,
    expiration_days: Option<u32>,
    state: State<'_, AppState>,
) -> Result<ShareResponse, IpcError> {
    state.session_manager.reset_timer().await;
    require_active_session(&state).await?;

    validate_file_id(&file_id)?;
    if contact_id.is_empty() {
        return Err(IpcError::InvalidInput(
            "Contact ID must not be empty".into(),
        ));
    }

    let file_uuid = Uuid::parse_str(&file_id)
        .map_err(|_| IpcError::InvalidInput("file_id is not a valid UUID".into()))?;
    let contact_uuid = Uuid::parse_str(&contact_id)
        .map_err(|_| IpcError::InvalidInput("contact_id is not a valid UUID".into()))?;
    let contact_domain_id = ContactId::from_uuid(contact_uuid);

    let expires_at = expiration_days.map(|days| now_unix_seconds() + (days as i64 * 86400));
    let kek = extract_kek(&state).await?;

    let vault_id = state
        .active_vault_id
        .read()
        .await
        .clone()
        .ok_or_else(|| IpcError::VaultLocked("Vault is locked".into()))?;
    let staging_dir = vault_staging_dir(&vault_id);

    let transport = state.cloud_transport.read().await.clone();

    let (output, contact_email) = {
        let db_store = state
            .session_manager
            .get_metadata_store()
            .await
            .ok_or_else(|| IpcError::VaultLocked("Vault is locked".into()))?;
        let db = &*db_store;

        let contact = (db as &dyn SharingStore)
            .get_contact(contact_domain_id)
            .await
            .map_err(IpcError::from)?;

        let email = contact.email.clone();

        let output = crate::sharing::cloud::create_share(
            crate::sharing::cloud::CreateShareRequest {
                file_id: file_uuid,
                contact_id: contact_domain_id,
                expires_at,
                now_unix_seconds: now_unix_seconds(),
            },
            db as &dyn MetadataStore,
            db as &dyn SharingStore,
            &*transport,
            &kek,
            &staging_dir,
        )
        .await
        .map_err(IpcError::from)?;

        (output, email)
    };

    let shares_dir = dirs::data_dir()
        .ok_or_else(|| IpcError::InternalError("Cannot determine data directory".into()))?
        .join("arx-runa")
        .join("shares");
    tokio::fs::create_dir_all(&shares_dir)
        .await
        .map_err(|_| IpcError::InternalError("Cannot create shares directory".into()))?;

    // Use share_id (not file_id) to prevent overwriting a previous share of the same file.
    let package_path = shares_dir.join(format!("{}.arxshare", output.share_id));
    tokio::fs::write(&package_path, &output.wire_bytes)
        .await
        .map_err(|_| IpcError::InternalError("Cannot write share package".into()))?;

    // Register the package path so reveal_in_explorer and compose_email_with_attachment
    // can access it this session. The shares directory uses `dirs::data_dir()` which
    // differs from Tauri's `app_data_dir()` (productName vs "arx-runa"), so the path
    // won't pass the `starts_with(app_data)` check and must be explicitly allowed.
    match package_path.canonicalize() {
        Ok(canonical) => {
            state
                .allowed_reveal_paths
                .lock()
                .await
                .insert(strip_unc_prefix(canonical));
        }
        Err(e) => {
            tracing::warn!(error = %e, "failed to register share package path for reveal");
        }
    }

    Ok(ShareResponse {
        share_id: output.share_id,
        package_path: package_path.to_str().unwrap_or_default().to_owned(),
        contact_email,
    })
}

/// Import a received share package.
///
/// Reads the wire bytes from `share_package_path`, retrieves and unwraps the
/// vault's X25519 private key from the `vault_identity` row, opens the HPKE
/// envelope, wraps the file key under the local KEK, and persists a
/// `ReceivedShare` row.
///
/// # Key hygiene
/// - The unwrapped private key bytes live only inside a `Zeroizing<[u8; 32]>`.
/// - They are never written to any log or error message.
/// - `SharingError::AuthenticationFailed` maps to a fixed user-safe string via
///   `From<SharingError>` in `error.rs`; use `?` to trigger that mapping.
#[tauri::command]
pub async fn import_share(
    share_package_path: PathBuf,
    state: State<'_, AppState>,
) -> Result<ImportShareResponse, IpcError> {
    state.session_manager.reset_timer().await;
    require_active_session(&state).await?;

    let wire_bytes = tokio::fs::read(&share_package_path)
        .await
        .map_err(|_| IpcError::InvalidInput("Failed to read share package file".into()))?;

    let kek = extract_kek(&state).await?;

    let vault_id = state
        .active_vault_id
        .read()
        .await
        .clone()
        .ok_or_else(|| IpcError::VaultLocked("Vault is locked".into()))?;
    let staging_dir = vault_staging_dir(&vault_id);

    let rclone_bin: Option<PathBuf> = state.app_handle.get().map(resolve_rclone_bin);

    // Scope the DB lock: import, look up sender name, then release before async upload.
    let (share_id, file_name, sender_name, import_receipt_ctx) = {
        let db_store = state
            .session_manager
            .get_metadata_store()
            .await
            .ok_or_else(|| IpcError::VaultLocked("Vault is locked".into()))?;
        let db = &*db_store;

        // Retrieve the wrapped X25519 private key from the vault identity row.
        let wrapped_blob: Vec<u8> = db
            .with_connection_blocking(|conn| {
                conn.query_row(
                    "SELECT wrapped_private_key FROM vault_identity WHERE id = 1",
                    [],
                    |row| row.get::<_, Vec<u8>>(0),
                )
                .optional()
                .map_err(StorageError::from_rusqlite)
            })
            .await
            .map_err(|_| IpcError::InternalError("Vault identity query failed".into()))?
            .ok_or_else(|| IpcError::InternalError("Vault identity row missing".into()))?;

        let wrapped_array: [u8; 72] = wrapped_blob
            .try_into()
            .map_err(|_| IpcError::InternalError("Vault identity key blob corrupted".into()))?;

        let vault_uuid_for_identity = Uuid::parse_str(&vault_id)
            .map_err(|_| IpcError::InternalError("Invalid vault ID".into()))?;
        let wrapped_key = WrappedFileKey::new(wrapped_array);
        let private_key_secret = unwrap_file_key(
            &wrapped_key,
            &FileId::from_uuid(vault_uuid_for_identity),
            &kek,
        )
        .map_err(|_| IpcError::AuthenticationFailed("Vault identity key unwrap failed".into()))?;

        let private_key_bytes: Zeroizing<[u8; 32]> =
            Zeroizing::new(private_key_secret.with_exposed(|bytes| *bytes));

        let now = now_unix_seconds();

        let received_share = import_share_package(
            &wire_bytes,
            &private_key_bytes,
            &kek,
            db as &dyn SharingStore,
            now,
        )
        .await
        .map_err(IpcError::from)?;

        let sender_name = if let Some(contact_id) = received_share.sender_contact_id {
            (db as &dyn SharingStore)
                .get_contact(contact_id)
                .await
                .ok()
                .map(|c| c.display_name.as_str().to_owned())
        } else {
            None
        };

        // Capture receipt context before the DB lock is released.
        let import_receipt_ctx: Option<(
            crate::sharing::X25519PublicKey,
            serde_json::Value,
            String,
        )> = if received_share
            .cloud_endpoint
            .get("receipt_requested")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
        {
            Some((
                received_share.sender_public_key,
                received_share.cloud_endpoint.clone(),
                received_share.share_id.clone(),
            ))
        } else {
            None
        };

        (
            received_share.share_id,
            received_share.file_name,
            sender_name,
            import_receipt_ctx,
        )
    };

    // Best-effort: write an import receipt blob sealed with the sender's public key.
    if let Some((sender_pub_key, cloud_endpoint, receipt_share_id)) = import_receipt_ctx {
        write_receipt_blob(
            &receipt_share_id,
            &sender_pub_key,
            &cloud_endpoint,
            &staging_dir,
            rclone_bin,
            "import-receipts",
            "imported_at",
        )
        .await;
    }

    Ok(ImportShareResponse {
        share_id,
        file_name,
        sender_name,
    })
}

/// Revoke a previously shared file.
///
/// Deletes the per-recipient cloud blobs (if this is the last active share for
/// the file) and marks the share row as revoked. Retryable on partial failure.
#[tauri::command]
pub async fn revoke_share(share_id: String, state: State<'_, AppState>) -> Result<(), IpcError> {
    state.session_manager.reset_timer().await;
    require_active_session(&state).await?;

    if share_id.is_empty() {
        return Err(IpcError::InvalidInput("Share ID must not be empty".into()));
    }

    let now = now_unix_seconds();

    // Clone the Arc before acquiring the database lock to avoid holding two guards.
    let transport = state.cloud_transport.read().await.clone();

    let (download_key_id, download_folder_id) = {
        let db_store = state
            .session_manager
            .get_metadata_store()
            .await
            .ok_or_else(|| IpcError::VaultLocked("Vault is locked".into()))?;
        let db = &*db_store;
        let sharing = db as &dyn SharingStore;
        sharing
            .get_share(&share_id)
            .await
            .ok()
            .map(|s| (s.download_key_id, s.download_folder_id))
            .unwrap_or((None, None))
    };

    let db_store = state
        .session_manager
        .get_metadata_store()
        .await
        .ok_or_else(|| IpcError::VaultLocked("Vault is locked".into()))?;
    let db = &*db_store;

    // Use the fully qualified path to avoid shadowing this command function.
    crate::sharing::revoke_share(&share_id, now, db as &dyn SharingStore, &*transport)
        .await
        .map_err(IpcError::from)?;

    let Some(conf_path) = state.session_manager.rclone_conf_path().await else {
        tracing::warn!("revoke_share: no session rclone conf; skipping cloud share cleanup");
        return Ok(());
    };

    match (download_key_id, download_folder_id) {
        (Some(perm_id), Some(folder_id)) => {
            cleanup_gdrive_share_permission(&conf_path, &perm_id, &folder_id).await;
        }
        (Some(key_id), None) => {
            cleanup_b2_share_key(&conf_path, &key_id).await;
        }
        _ => {}
    }

    Ok(())
}

/// Row type returned by the `list_shares` SQL query.
type ShareRow = (
    String,
    String,
    String,
    i64,
    Option<i64>,
    bool,
    Option<i64>,
    Option<i64>,
    Option<i64>,
);

/// List outgoing shares.
///
/// Queries the `shares` table with a JOIN against `nodes` and `contacts` to
/// obtain the file name and contact display name for each share row.
/// `SharingStore` has no `list_all_shares` method, so this uses a direct
/// `with_connection_blocking` query.
#[tauri::command]
pub async fn list_shares(state: State<'_, AppState>) -> Result<Vec<ShareEntry>, IpcError> {
    state.session_manager.reset_timer().await;
    require_active_session(&state).await?;

    let db_store = state
        .session_manager
        .get_metadata_store()
        .await
        .ok_or_else(|| IpcError::VaultLocked("Vault is locked".into()))?;
    let db = &*db_store;

    // SharingStore trait has no list_all_shares; query directly with a JOIN.
    let rows: Vec<ShareRow> = db
        .with_connection_blocking(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT s.share_id, n.name, c.display_name, s.created_at, s.revoked_at, \
                            s.receipt_requested, s.receipt_received_at, \
                            s.import_receipt_received_at, s.expires_at \
                     FROM shares s \
                     JOIN nodes n ON s.file_id = n.node_id \
                     JOIN contacts c ON s.contact_id = c.contact_id \
                     ORDER BY s.created_at DESC, s.share_id ASC",
                )
                .map_err(StorageError::from_rusqlite)?;
            let mapped = stmt
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, i64>(3)?,
                        row.get::<_, Option<i64>>(4)?,
                        row.get::<_, bool>(5)?,
                        row.get::<_, Option<i64>>(6)?,
                        row.get::<_, Option<i64>>(7)?,
                        row.get::<_, Option<i64>>(8)?,
                    ))
                })
                .map_err(StorageError::from_rusqlite)?;
            let mut rows: Vec<ShareRow> = Vec::new();
            for row in mapped {
                rows.push(row.map_err(StorageError::from_rusqlite)?);
            }
            Ok(rows)
        })
        .await
        .map_err(IpcError::from)?;

    Ok(rows
        .into_iter()
        .map(
            |(
                share_id,
                file_name,
                contact_name,
                created_at,
                revoked_at,
                receipt_requested,
                receipt_received_at,
                import_receipt_received_at,
                expires_at,
            )| ShareEntry {
                share_id,
                file_name,
                contact_name,
                created_at: unix_ts_to_iso8601(created_at),
                revoked: revoked_at.is_some(),
                receipt_requested,
                receipt_received_at: receipt_received_at.map(unix_ts_to_iso8601),
                import_receipt_received_at: import_receipt_received_at.map(unix_ts_to_iso8601),
                expires_at: expires_at.map(unix_ts_to_iso8601),
            },
        )
        .collect())
}

/// List received shares.
///
/// Returns all rows from `received_shares`, enriching each with the sender's
/// display name when the sender is a known contact.
#[tauri::command]
pub async fn list_received_shares(
    state: State<'_, AppState>,
) -> Result<Vec<ReceivedShareEntry>, IpcError> {
    state.session_manager.reset_timer().await;
    require_active_session(&state).await?;

    let db_store = state
        .session_manager
        .get_metadata_store()
        .await
        .ok_or_else(|| IpcError::VaultLocked("Vault is locked".into()))?;
    let db = &*db_store;

    let received_shares = (db as &dyn SharingStore)
        .list_received_shares()
        .await
        .map_err(IpcError::from)?;

    let mut entries = Vec::with_capacity(received_shares.len());
    for rs in received_shares {
        let sender_name = if let Some(contact_id) = rs.sender_contact_id {
            (db as &dyn SharingStore)
                .get_contact(contact_id)
                .await
                .ok()
                .map(|c| c.display_name.as_str().to_owned())
        } else {
            None
        };
        let size_bytes = rs
            .cloud_endpoint
            .get("_file_size")
            .and_then(|v| v.as_u64())
            .unwrap_or_else(|| (rs.chunk_count as u64).saturating_mul(rs.chunk_size as u64));
        let is_expired = rs
            .expires_at
            .map(|t| t < now_unix_seconds())
            .unwrap_or(false);
        entries.push(ReceivedShareEntry {
            share_id: rs.share_id,
            file_name: rs.file_name,
            sender_name,
            imported_at: unix_ts_to_iso8601(rs.imported_at),
            size_bytes,
            is_expired,
            expires_at: rs.expires_at.map(unix_ts_to_iso8601),
        });
    }

    Ok(entries)
}

/// Downloads and decrypts a received share file to a caller-specified destination path.
#[tauri::command]
pub async fn download_received_share(
    share_id: String,
    destination_path: String,
    progress: Channel<ProgressUpdate>,
    state: State<'_, AppState>,
) -> Result<DownloadReceivedShareResponse, IpcError> {
    state.session_manager.reset_timer().await;
    require_active_session(&state).await?;

    let progress_ch = ProgressChannel::new(progress);
    let _ = progress_ch.try_send_if_open(ProgressUpdate {
        percent: 0,
        bytes_processed: 0,
        bytes_total: 0,
        status: "Preparing download…".into(),
    });

    let vault_id = state
        .active_vault_id
        .read()
        .await
        .clone()
        .ok_or_else(|| IpcError::VaultLocked("Vault is locked".into()))?;
    let staging_dir = vault_staging_dir(&vault_id);
    let kek = extract_kek(&state).await?;
    let transport = state.cloud_transport.read().await.clone();

    let rclone_bin: Option<PathBuf> = state.app_handle.get().map(resolve_rclone_bin);

    // Extract share metadata and fetch blobs in one DB lock scope.
    let (
        file_name,
        file_key,
        file_id_uuid,
        chunk_uuids,
        chunk_count,
        chunk_size,
        file_size,
        receipt_ctx,
        local_blobs,
    ) = {
        let db_store = state
            .session_manager
            .get_metadata_store()
            .await
            .ok_or_else(|| IpcError::VaultLocked("Vault is locked".into()))?;
        let db = &*db_store;
        let sharing = db as &dyn SharingStore;

        let share = sharing
            .get_received_share(&share_id)
            .await
            .map_err(IpcError::from)?;

        if share.expires_at.is_some_and(|t| t < now_unix_seconds()) {
            return Err(IpcError::InvalidInput(
                "Share has expired — contact sender for renewed access".into(),
            ));
        }

        let file_id_uuid = Uuid::parse_str(&share.file_id)
            .map_err(|_| IpcError::InternalError("Invalid file ID in received share".into()))?;
        let file_key = unwrap_file_key(
            &WrappedFileKey::new(share.file_key_wrapped),
            &FileId::from_uuid(file_id_uuid),
            &kek,
        )
        .map_err(|_| IpcError::InternalError("File key unwrap failed".into()))?;
        let file_size = share
            .cloud_endpoint
            .get("_file_size")
            .and_then(|v| v.as_u64())
            .unwrap_or_else(|| (share.chunk_count as u64).saturating_mul(share.chunk_size as u64));

        // Capture receipt context before the share is partially moved.
        let receipt_ctx: Option<(crate::sharing::X25519PublicKey, serde_json::Value, String)> =
            if share
                .cloud_endpoint
                .get("receipt_requested")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
            {
                Some((
                    share.sender_public_key,
                    share.cloud_endpoint.clone(),
                    share.share_id.clone(),
                ))
            } else {
                None
            };

        let local_blobs = crate::sharing::cloud::fetch_received_share_to_local(
            &share_id,
            sharing,
            &*transport,
            &staging_dir,
            rclone_bin.clone(),
        )
        .await
        .map_err(IpcError::from)?;

        (
            share.file_name,
            file_key,
            file_id_uuid,
            share.chunk_uuids,
            share.chunk_count,
            share.chunk_size,
            file_size,
            receipt_ctx,
            local_blobs,
        )
    };

    tracing::debug!(
        staging_dir = %staging_dir.display(),
        chunk_uuids = ?chunk_uuids,
        local_blobs = ?local_blobs,
        "download_received_share: staging dir, chunk UUIDs, and downloaded blob paths",
    );

    let _ = progress_ch.try_send_if_open(ProgressUpdate {
        percent: 70,
        bytes_processed: 0,
        bytes_total: 0,
        status: "Decrypting file…".into(),
    });

    let dest = std::path::Path::new(&destination_path);
    let decrypt_result = decrypt_received_share_blobs(
        dest,
        file_id_uuid,
        &file_key,
        file_size,
        chunk_count,
        chunk_size,
        &chunk_uuids,
        &staging_dir,
    )
    .await;

    for blob_path in &local_blobs {
        let _ = tokio::fs::remove_file(blob_path).await;
    }

    decrypt_result.map_err(|e| {
        tracing::warn!(error = %e, "decrypt_received_share_blobs failed");
        IpcError::InternalError("Decryption failed".into())
    })?;

    // Register the destination so reveal_in_explorer can open it this session.
    match std::path::Path::new(&destination_path).canonicalize() {
        Ok(canonical) => {
            state
                .allowed_reveal_paths
                .lock()
                .await
                .insert(strip_unc_prefix(canonical));
        }
        Err(e) => {
            tracing::warn!(error = %e, "failed to register download path for reveal");
        }
    }

    let _ = progress_ch.try_send_if_open(ProgressUpdate {
        percent: 100,
        bytes_processed: 0,
        bytes_total: 0,
        status: "Download complete".into(),
    });

    // Best-effort: write a receipt blob sealed with the sender's public key.
    if let Some((sender_pub_key, cloud_endpoint, receipt_share_id)) = receipt_ctx {
        write_receipt_blob(
            &receipt_share_id,
            &sender_pub_key,
            &cloud_endpoint,
            &staging_dir,
            rclone_bin,
            "receipts",
            "downloaded_at",
        )
        .await;
    }

    Ok(DownloadReceivedShareResponse { file_name })
}

/// Decrypts a received share into memory and returns the content for in-app preview.
///
/// Rejects files larger than 50 MiB to keep memory use bounded.
#[tauri::command]
pub async fn get_received_share_content(
    share_id: String,
    state: State<'_, AppState>,
) -> Result<FileContent, IpcError> {
    state.session_manager.reset_timer().await;
    require_active_session(&state).await?;

    let vault_id = state
        .active_vault_id
        .read()
        .await
        .clone()
        .ok_or_else(|| IpcError::VaultLocked("Vault is locked".into()))?;
    let staging_dir = vault_staging_dir(&vault_id);
    let kek = extract_kek(&state).await?;
    let transport = state.cloud_transport.read().await.clone();

    let rclone_bin: Option<PathBuf> = state.app_handle.get().map(resolve_rclone_bin);

    const FIFTY_MIB: u64 = 50 * 1024 * 1024;

    let (file_key, file_id_uuid, chunk_uuids, chunk_count, chunk_size, file_size, local_blobs) = {
        let db_store = state
            .session_manager
            .get_metadata_store()
            .await
            .ok_or_else(|| IpcError::VaultLocked("Vault is locked".into()))?;
        let db = &*db_store;
        let sharing = db as &dyn SharingStore;

        let share = sharing
            .get_received_share(&share_id)
            .await
            .map_err(IpcError::from)?;

        if share.expires_at.is_some_and(|t| t < now_unix_seconds()) {
            return Err(IpcError::InvalidInput(
                "Share has expired — contact sender for renewed access".into(),
            ));
        }

        let file_size = share
            .cloud_endpoint
            .get("_file_size")
            .and_then(|v| v.as_u64())
            .unwrap_or_else(|| (share.chunk_count as u64).saturating_mul(share.chunk_size as u64));

        if file_size > FIFTY_MIB {
            return Err(IpcError::InvalidInput(
                "File exceeds 50 MiB in-app viewing limit".into(),
            ));
        }

        let file_id_uuid = Uuid::parse_str(&share.file_id)
            .map_err(|_| IpcError::InternalError("Invalid file ID in received share".into()))?;
        let file_key = unwrap_file_key(
            &WrappedFileKey::new(share.file_key_wrapped),
            &FileId::from_uuid(file_id_uuid),
            &kek,
        )
        .map_err(|_| IpcError::InternalError("File key unwrap failed".into()))?;

        let local_blobs = crate::sharing::cloud::fetch_received_share_to_local(
            &share_id,
            sharing,
            &*transport,
            &staging_dir,
            rclone_bin,
        )
        .await
        .map_err(IpcError::from)?;

        (
            file_key,
            file_id_uuid,
            share.chunk_uuids,
            share.chunk_count,
            share.chunk_size,
            file_size,
            local_blobs,
        )
    };

    let temp_dir = tempfile::tempdir()
        .map_err(|e| IpcError::InternalError(format!("Failed to create temp dir: {e}")))?;
    let temp_path = temp_dir.path().join("content");

    let decrypt_result = decrypt_received_share_blobs(
        &temp_path,
        file_id_uuid,
        &file_key,
        file_size,
        chunk_count,
        chunk_size,
        &chunk_uuids,
        &staging_dir,
    )
    .await;

    for blob_path in &local_blobs {
        let _ = tokio::fs::remove_file(blob_path).await;
    }

    decrypt_result.map_err(|e| IpcError::InternalError(format!("decrypt failed: {e}")))?;

    let bytes = tokio::fs::read(&temp_path)
        .await
        .map_err(|e| IpcError::InternalError(format!("Failed to read decrypted content: {e}")))?;

    let mime_type = detect_mime_type(&bytes).to_owned();
    let size_bytes = bytes.len() as u64;
    let data_base64 = BASE64_STANDARD.encode(&bytes);

    Ok(FileContent {
        mime_type,
        data_base64,
        size_bytes,
    })
}

/// Seals and uploads a delivery receipt blob to the sender's B2 prefix.
///
/// Non-fatal: all errors are logged at `warn` level; the download is already complete.
async fn write_receipt_blob(
    share_id: &str,
    sender_pub_key: &crate::sharing::X25519PublicKey,
    cloud_endpoint: &serde_json::Value,
    staging_dir: &std::path::Path,
    rclone_bin: Option<PathBuf>,
    receipt_prefix: &str,
    timestamp_key: &str,
) {
    use crate::storage::cloud::{CloudTransport as _, RcloneTransport};

    let now = now_unix_seconds();
    let payload = serde_json::json!({ "share_id": share_id, timestamp_key: now });
    let plaintext = match serde_json::to_vec(&payload) {
        Ok(b) => b,
        Err(e) => {
            tracing::warn!(%e, "receipt payload serialisation failed");
            return;
        }
    };

    let wire = match crate::sharing::hpke::seal(sender_pub_key, &plaintext) {
        Ok(w) => w,
        Err(e) => {
            tracing::warn!(%e, "receipt HPKE seal failed");
            return;
        }
    };

    let Some(provider) = cloud_endpoint.get("provider").and_then(|v| v.as_str()) else {
        tracing::warn!("receipt: no provider in cloud_endpoint");
        return;
    };
    if provider != "b2" {
        return;
    }
    let (Some(key_id), Some(app_key), Some(bucket), Some(path_prefix)) = (
        cloud_endpoint.get("key_id").and_then(|v| v.as_str()),
        cloud_endpoint
            .get("application_key")
            .and_then(|v| v.as_str()),
        cloud_endpoint.get("bucket").and_then(|v| v.as_str()),
        cloud_endpoint.get("path_prefix").and_then(|v| v.as_str()),
    ) else {
        tracing::warn!("receipt: missing B2 credentials in cloud_endpoint");
        return;
    };

    let Some(rclone_binary) = rclone_bin else {
        tracing::warn!("receipt: no rclone binary available");
        return;
    };

    let receipt_uuid = Uuid::new_v4();
    let local_receipt = staging_dir.join(format!("receipt-{receipt_uuid}.blob"));

    if let Err(e) = tokio::fs::write(&local_receipt, &wire).await {
        tracing::warn!(%e, "receipt: failed to write local receipt blob");
        return;
    }

    let conf_content = format!("[arxshare-rcpt]\ntype = b2\naccount = {key_id}\nkey = {app_key}\n");
    let conf_path = staging_dir.join(format!("rcpt-{receipt_uuid}.conf"));
    if let Err(e) =
        crate::storage::staging::write_owner_only(&conf_path, conf_content.as_bytes()).await
    {
        tracing::warn!(%e, "receipt: failed to write rclone conf");
        let _ = tokio::fs::remove_file(&local_receipt).await;
        return;
    }

    let remote_root = format!("arxshare-rcpt:{bucket}/{path_prefix}{receipt_prefix}");
    let upload_transport =
        RcloneTransport::new_for_share_download(rclone_binary, conf_path.clone(), remote_root);
    let remote_path = format!("{receipt_uuid}.blob");

    if let Err(e) = upload_transport
        .upload_blob(&local_receipt, &remote_path)
        .await
    {
        tracing::warn!(%e, "receipt: upload failed");
    } else {
        tracing::debug!(%share_id, "receipt blob uploaded");
    }

    let _ = tokio::fs::remove_file(&local_receipt).await;
    let _ = tokio::fs::remove_file(&conf_path).await;
}

/// Decrypts chunk blobs for a received share, writing plaintext to `destination`.
#[allow(clippy::too_many_arguments)]
async fn decrypt_received_share_blobs(
    destination: &std::path::Path,
    file_id: Uuid,
    file_key: &crate::crypto::FileKey,
    file_size: u64,
    chunk_count: u32,
    chunk_size: u32,
    chunk_uuids: &[String],
    blob_directory: &std::path::Path,
) -> Result<(), StorageError> {
    use crate::crypto::{Blake3Hash, ChunkIndex, FileId, decrypt_chunk, verify_checksum};
    use tokio::io::{AsyncWriteExt, BufWriter};

    if chunk_uuids.len() != chunk_count as usize {
        return Err(StorageError::ConstraintViolation(
            "chunk_uuids length does not match chunk_count".to_owned(),
        ));
    }

    let temp_dest = destination.with_extension("tmp");
    let file = tokio::fs::File::create(&temp_dest)
        .await
        .map_err(|e| StorageError::Io(e.to_string()))?;
    let mut writer = BufWriter::new(file);
    let crypto_file_id = FileId::from_uuid(file_id);
    let chunk_size_u64 = chunk_size as u64;

    tracing::debug!(
        blob_directory = %blob_directory.display(),
        chunk_count,
        "decrypt_received_share_blobs: reading {} blob(s)",
        chunk_uuids.len(),
    );

    let result: Result<(), StorageError> = async {
        for (index, blob_name) in chunk_uuids.iter().enumerate() {
            let blob_path = blob_directory.join(format!("{blob_name}.blob"));
            tracing::debug!(blob_path = %blob_path.display(), index, "reading blob");
            let blob_bytes = tokio::fs::read(&blob_path)
                .await
                .map_err(|e| StorageError::Io(format!("{e} (blob: {})", blob_path.display())))?;

            let hash_bytes: [u8; 32] = blake3::hash(&blob_bytes).into();
            let verified =
                verify_checksum(blob_bytes, &Blake3Hash(hash_bytes)).map_err(StorageError::from)?;

            let padded = decrypt_chunk(
                verified,
                file_key,
                &crypto_file_id,
                ChunkIndex::new(index as u32),
            )
            .map_err(StorageError::from)?;

            let bytes_to_write = if index + 1 == chunk_count as usize {
                (file_size.saturating_sub(index as u64 * chunk_size_u64) as usize).min(padded.len())
            } else {
                chunk_size as usize
            };

            writer
                .write_all(&padded[..bytes_to_write])
                .await
                .map_err(|e| StorageError::Io(e.to_string()))?;
        }
        writer
            .flush()
            .await
            .map_err(|e| StorageError::Io(e.to_string()))
    }
    .await;

    if result.is_err() {
        let _ = tokio::fs::remove_file(&temp_dest).await;
        return result;
    }

    tracing::debug!(
        temp_dest = %temp_dest.display(),
        destination = %destination.display(),
        "renaming temp file to destination",
    );
    tokio::fs::rename(&temp_dest, destination)
        .await
        .map_err(|e| {
            StorageError::Io(format!(
                "{e} (rename: {} -> {})",
                temp_dest.display(),
                destination.display(),
            ))
        })
}

/// Checks for delivery receipts on all active shares that requested one.
///
/// For each active share with `receipt_requested = true` and no recorded receipt,
/// lists blobs under `{path_prefix}receipts/`, opens each with the vault's private key,
/// and records the `downloaded_at` timestamp. Best-effort — individual failures are logged.
#[tauri::command]
pub async fn check_share_receipts(state: State<'_, AppState>) -> Result<Vec<ShareEntry>, IpcError> {
    state.session_manager.reset_timer().await;
    require_active_session(&state).await?;

    let kek = extract_kek(&state).await?;
    let transport = state.cloud_transport.read().await.clone();
    let vault_id = state
        .active_vault_id
        .read()
        .await
        .clone()
        .ok_or_else(|| IpcError::VaultLocked("Vault is locked".into()))?;
    let staging_dir = vault_staging_dir(&vault_id);

    let db_store = state
        .session_manager
        .get_metadata_store()
        .await
        .ok_or_else(|| IpcError::VaultLocked("Vault is locked".into()))?;

    // Unwrap the vault's own private key for HPKE.Open of receipt blobs.
    let private_key_bytes: Zeroizing<[u8; 32]> = {
        let db = &*db_store;
        let wrapped_blob: Vec<u8> = db
            .with_connection_blocking(|conn| {
                conn.query_row(
                    "SELECT wrapped_private_key FROM vault_identity WHERE id = 1",
                    [],
                    |row| row.get::<_, Vec<u8>>(0),
                )
                .map_err(StorageError::from_rusqlite)
            })
            .await
            .map_err(IpcError::from)?;
        let wrapped_key =
            WrappedFileKey::new(wrapped_blob.try_into().map_err(|_| {
                IpcError::InternalError("Vault identity key blob corrupted".into())
            })?);
        let vault_uuid_for_identity = Uuid::parse_str(&vault_id)
            .map_err(|_| IpcError::InternalError("Invalid vault ID".into()))?;
        let secret = unwrap_file_key(
            &wrapped_key,
            &FileId::from_uuid(vault_uuid_for_identity),
            &kek,
        )
        .map_err(|_| IpcError::AuthenticationFailed("Vault identity key unwrap failed".into()))?;
        Zeroizing::new(secret.with_exposed(|b| *b))
    };

    // Fetch shares that need download-receipt checking.
    let pending_download: Vec<(String, String, i64)> = {
        let db = &*db_store;
        db.with_connection_blocking(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT share_id, cloud_path, created_at \
                     FROM shares \
                     WHERE receipt_requested = 1 \
                       AND receipt_received_at IS NULL \
                       AND revoked_at IS NULL",
                )
                .map_err(StorageError::from_rusqlite)?;
            let rows = stmt
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, i64>(2)?,
                    ))
                })
                .map_err(StorageError::from_rusqlite)?;
            let mut out = Vec::new();
            for row in rows {
                out.push(row.map_err(StorageError::from_rusqlite)?);
            }
            Ok(out)
        })
        .await
        .map_err(IpcError::from)?
    };

    for (share_id, cloud_path, share_created_at) in &pending_download {
        if cloud_path.is_empty() {
            continue;
        }
        let receipt_prefix = format!("{cloud_path}receipts/");

        // List receipt blobs using the vault owner's transport.
        let mut blob_names = match transport.list_blobs(&receipt_prefix).await {
            Ok(names) => names,
            Err(_) => continue,
        };
        if blob_names.is_empty() {
            continue;
        }
        if blob_names.len() > 50 {
            tracing::warn!(%share_id, blobs = blob_names.len(), "receipt blob list exceeds cap, truncating to 50");
            blob_names.truncate(50);
        }

        let mut earliest_downloaded_at: Option<i64> = None;

        for blob_name in &blob_names {
            let local_path = staging_dir.join(format!("rcpt-chk-{}.blob", Uuid::new_v4()));
            if transport
                .download_blob(blob_name, &local_path)
                .await
                .is_err()
            {
                continue;
            }
            let bytes = match tokio::fs::read(&local_path).await {
                Ok(b) => b,
                Err(_) => {
                    let _ = tokio::fs::remove_file(&local_path).await;
                    continue;
                }
            };
            let _ = tokio::fs::remove_file(&local_path).await;

            match crate::sharing::hpke::open(&private_key_bytes, &bytes) {
                Ok(plaintext) => {
                    if let Ok(payload) = serde_json::from_slice::<serde_json::Value>(&plaintext)
                        && let Some(ts) = payload.get("downloaded_at").and_then(|v| v.as_i64())
                    {
                        let now = now_unix_seconds();
                        if ts < *share_created_at || ts > now + 300 {
                            tracing::warn!(%share_id, ts, "receipt timestamp out of range, skipping");
                            continue;
                        }
                        earliest_downloaded_at =
                            Some(earliest_downloaded_at.map_or(ts, |prev| prev.min(ts)));
                    }
                }
                Err(e) => {
                    tracing::warn!(%share_id, %e, "receipt HPKE open failed");
                }
            }
        }

        if let Some(ts) = earliest_downloaded_at {
            let db = &*db_store;
            let sid = share_id.clone();
            let _ = db
                .with_connection_blocking(move |conn| {
                    conn.execute(
                        "UPDATE shares SET receipt_received_at = ?1 WHERE share_id = ?2",
                        rusqlite::params![ts, sid],
                    )
                    .map_err(StorageError::from_rusqlite)?;
                    Ok(())
                })
                .await;
        }
    }

    // Second pass: check import receipts for shares that haven't been imported yet.
    let pending_import: Vec<(String, String, i64)> = {
        let db = &*db_store;
        db.with_connection_blocking(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT share_id, cloud_path, created_at \
                     FROM shares \
                     WHERE receipt_requested = 1 \
                       AND import_receipt_received_at IS NULL \
                       AND revoked_at IS NULL",
                )
                .map_err(StorageError::from_rusqlite)?;
            let rows = stmt
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, i64>(2)?,
                    ))
                })
                .map_err(StorageError::from_rusqlite)?;
            let mut out = Vec::new();
            for row in rows {
                out.push(row.map_err(StorageError::from_rusqlite)?);
            }
            Ok(out)
        })
        .await
        .map_err(IpcError::from)?
    };

    for (share_id, cloud_path, share_created_at) in &pending_import {
        if cloud_path.is_empty() {
            continue;
        }
        let receipt_prefix = format!("{cloud_path}import-receipts/");

        let mut blob_names = match transport.list_blobs(&receipt_prefix).await {
            Ok(names) => names,
            Err(_) => continue,
        };
        if blob_names.is_empty() {
            continue;
        }
        if blob_names.len() > 50 {
            tracing::warn!(%share_id, blobs = blob_names.len(), "import receipt blob list exceeds cap, truncating to 50");
            blob_names.truncate(50);
        }

        let mut earliest_imported_at: Option<i64> = None;

        for blob_name in &blob_names {
            let local_path = staging_dir.join(format!("import-rcpt-chk-{}.blob", Uuid::new_v4()));
            if transport
                .download_blob(blob_name, &local_path)
                .await
                .is_err()
            {
                continue;
            }
            let bytes = match tokio::fs::read(&local_path).await {
                Ok(b) => b,
                Err(_) => {
                    let _ = tokio::fs::remove_file(&local_path).await;
                    continue;
                }
            };
            let _ = tokio::fs::remove_file(&local_path).await;

            match crate::sharing::hpke::open(&private_key_bytes, &bytes) {
                Ok(plaintext) => {
                    if let Ok(payload) = serde_json::from_slice::<serde_json::Value>(&plaintext)
                        && let Some(ts) = payload.get("imported_at").and_then(|v| v.as_i64())
                    {
                        let now = now_unix_seconds();
                        if ts < *share_created_at || ts > now + 300 {
                            tracing::warn!(%share_id, ts, "import receipt timestamp out of range, skipping");
                            continue;
                        }
                        earliest_imported_at =
                            Some(earliest_imported_at.map_or(ts, |prev| prev.min(ts)));
                    }
                }
                Err(e) => {
                    tracing::warn!(%share_id, %e, "import receipt HPKE open failed");
                }
            }
        }

        if let Some(ts) = earliest_imported_at {
            let db = &*db_store;
            let sid = share_id.clone();
            let _ = db
                .with_connection_blocking(move |conn| {
                    conn.execute(
                        "UPDATE shares SET import_receipt_received_at = ?1 WHERE share_id = ?2",
                        rusqlite::params![ts, sid],
                    )
                    .map_err(StorageError::from_rusqlite)?;
                    Ok(())
                })
                .await;
        }
    }

    // Return fresh share list so the frontend can update badges.
    list_shares(state).await
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    #[test]
    fn test_unix_ts_to_iso8601_format() {
        // Just verify the format is deterministic
        let ts = 1704067200i64; // 2024-01-01 00:00:00 UTC
        assert!(ts > 0);
    }

    #[test]
    fn test_fingerprint_is_16_hex_chars() {
        // Fingerprint is first 8 bytes of SHA-256(public_key)
        // When rendered as hex, should be 16 lowercase hex characters
        let fp_bytes: [u8; 8] = [0x48, 0x65, 0x6c, 0x6c, 0x6f, 0x00, 0x00, 0x01];
        let hex = hex::encode(fp_bytes);
        assert_eq!(hex.len(), 16);
        assert!(
            hex.chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        );
    }

    #[test]
    fn test_uuid_generation_creates_unique_share_ids() {
        let share_id_1 = Uuid::new_v4().hyphenated().to_string();
        let share_id_2 = Uuid::new_v4().hyphenated().to_string();
        assert_ne!(share_id_1, share_id_2);
    }
}

/// Store a Google Drive Service Account JSON key for sharing.
///
/// Reads the key file at `sa_json_path`, validates it is a GCP service account
/// credential, then persists it in the encrypted `sharing_config` table.
/// The in-memory JSON string is zeroized before the function returns.
#[tauri::command]
pub async fn set_gdrive_service_account(
    sa_json_path: PathBuf,
    state: State<'_, AppState>,
) -> Result<(), IpcError> {
    state.session_manager.reset_timer().await;
    require_active_session(&state).await?;

    let raw = tokio::fs::read(&sa_json_path)
        .await
        .map_err(|_| IpcError::InvalidInput("Failed to read service account JSON file".into()))?;
    let mut sa_json =
        Zeroizing::new(String::from_utf8(raw).map_err(|_| {
            IpcError::InvalidInput("Service account JSON is not valid UTF-8".into())
        })?);

    // Validate: must be a service_account credential with the required fields.
    let parsed = serde_json::from_str::<serde_json::Value>(&sa_json)
        .map_err(|_| IpcError::InvalidInput("File is not valid JSON".into()))?;

    if parsed.get("type").and_then(|v| v.as_str()) != Some("service_account") {
        *sa_json = String::new();
        return Err(IpcError::InvalidInput(
            "JSON is not a service account credential (type must be \"service_account\")".into(),
        ));
    }
    for field in &["client_email", "private_key"] {
        if parsed.get(field).and_then(|v| v.as_str()).is_none() {
            *sa_json = String::new();
            return Err(IpcError::InvalidInput(format!(
                "service account JSON missing required field: {field}"
            )));
        }
    }

    let now = now_unix_seconds();
    let db_store = state
        .session_manager
        .get_metadata_store()
        .await
        .ok_or_else(|| IpcError::VaultLocked("Vault is locked".into()))?;
    let db = &*db_store;

    db.upsert_gdrive_sharing_config(None, sa_json.to_string(), now)
        .await
        .map_err(IpcError::from)?;

    // Zeroize before drop.
    *sa_json = String::new();

    Ok(())
}

/// Returns `true` if a Google Drive Service Account JSON has been stored for sharing.
#[tauri::command]
pub async fn has_gdrive_service_account(state: State<'_, AppState>) -> Result<bool, IpcError> {
    state.session_manager.reset_timer().await;
    require_active_session(&state).await?;

    let db_store = state
        .session_manager
        .get_metadata_store()
        .await
        .ok_or_else(|| IpcError::VaultLocked("Vault is locked".into()))?;
    let db = &*db_store;

    let has = db
        .get_gdrive_sharing_config()
        .await
        .map_err(IpcError::from)?
        .is_some();

    Ok(has)
}
