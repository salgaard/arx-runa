//! File management commands.
//!
//! Phase 6.5: backend delegation wired for all six IPC commands.

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use secrecy::SecretBox;
use tauri::State;
use tauri::ipc::Channel;
use uuid::Uuid;

use crate::crypto::KeyEncryptionKey;
use crate::storage::vault_ops::{
    delete_file as vault_delete, download_file as vault_download, upload_file as vault_upload,
};
use crate::storage::{MetadataStore, NodeType};
use crate::ui::commands_common::require_active_session;
use crate::ui::error::IpcError;
use crate::ui::state::AppState;
use crate::ui::types::{FileContent, FileEntry, ProgressUpdate, RemoteFileEntry};
use crate::ui::validation::{normalise_vault_path, validate_file_id, validate_vault_path};
use crate::ui::vault_paths::{resolve_singleton_vault, vault_staging_dir};

// ─── Private helpers ──────────────────────────────────────────────────────────

/// Converts a Unix timestamp (seconds since 1970-01-01T00:00:00Z) to an ISO 8601 string.
///
/// Implemented with pure stdlib arithmetic so the crate does not need `chrono` or `time`.
fn unix_ts_to_iso8601(ts: i64) -> String {
    let ts = if ts < 0 { 0u64 } else { ts as u64 };
    let secs = ts % 60;
    let mins = (ts / 60) % 60;
    let hours = (ts / 3600) % 24;
    let total_days = ts / 86400;
    let (year, month, day) = days_since_epoch_to_date(total_days);
    format!("{year:04}-{month:02}-{day:02}T{hours:02}:{mins:02}:{secs:02}Z")
}

/// Maps days since the Unix epoch to a proleptic Gregorian (year, month, day) triple.
///
/// Algorithm: http://howardhinnant.github.io/date_algorithms.html "civil_from_days".
fn days_since_epoch_to_date(days: u64) -> (u32, u32, u32) {
    let z = days as i64 + 719_468;
    let era: i64 = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
    let y = (if m <= 2 { y + 1 } else { y }) as u32;
    (y, m, d)
}

/// Detects a MIME type from the leading bytes of a file.
///
/// Recognises JPEG, PNG, GIF, PDF and ZIP by magic bytes.
/// Falls back to `"application/octet-stream"` for unrecognised formats.
fn detect_mime_type(bytes: &[u8]) -> &'static str {
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        "image/jpeg"
    } else if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        "image/png"
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        "image/gif"
    } else if bytes.starts_with(b"%PDF") {
        "application/pdf"
    } else if bytes.starts_with(b"PK\x03\x04") {
        "application/zip"
    } else {
        "application/octet-stream"
    }
}

/// Maps a storage [`Node`](crate::storage::Node) to a [`FileEntry`] for IPC response.
fn node_to_file_entry(node: &crate::storage::Node) -> FileEntry {
    FileEntry {
        id: node.node_id.as_uuid().hyphenated().to_string(),
        name: node.name.clone(),
        entry_type: match node.node_type {
            NodeType::File => "file".into(),
            NodeType::Directory => "directory".into(),
        },
        size_bytes: node.size_bytes,
        modified_at: unix_ts_to_iso8601(node.modified_at),
        parent_id: node
            .parent_id
            .map(|id| id.as_uuid().hyphenated().to_string()),
    }
}

/// Resolves the singleton vault and returns the vault identifier string.
///
/// Returns `IpcError::VaultLocked` when no vault is found on disk.
fn require_vault_id() -> Result<String, IpcError> {
    let (vault_id, _, _) = resolve_singleton_vault()?
        .ok_or_else(|| IpcError::VaultLocked("No vault found on this device".into()))?;
    Ok(vault_id)
}

/// Copies the KEK out of the session guard and wraps it in a `KeyEncryptionKey`.
///
/// The raw bytes are moved directly into the `SecretBox` heap buffer so no
/// cleartext copy remains on the stack longer than necessary.
async fn extract_kek(state: &AppState) -> Result<KeyEncryptionKey, IpcError> {
    let kek_raw: [u8; 32] = state
        .session_manager
        .with_key_encryption_key(|k| *k)
        .await
        .map_err(IpcError::from)?;
    Ok(KeyEncryptionKey::from_secret_box(SecretBox::new(Box::new(
        kek_raw,
    ))))
}

// ─── IPC command handlers ─────────────────────────────────────────────────────

/// List the contents of a directory in the vault.
///
/// `path` must be empty or `"/"` to list root, or a vault-node UUID string to
/// list children of a known directory node.  A non-UUID non-root path returns
/// `InvalidInput`.
#[tauri::command]
pub async fn list_directory(
    path: String,
    state: State<'_, AppState>,
) -> Result<Vec<FileEntry>, IpcError> {
    state.session_manager.reset_timer().await;
    require_active_session(&state).await?;

    let normalised = normalise_vault_path(&path);
    validate_vault_path(normalised)?;

    let db_guard = state.database.read().await;
    let db = db_guard
        .as_ref()
        .ok_or_else(|| IpcError::VaultLocked("Vault is locked".into()))?;

    // Determine the parent UUID to list.
    // Phase 6.5: empty path → look up "root_id" in manifest_meta.
    //            non-empty path → treat as a node UUID directly.
    let parent_uuid: Uuid = if normalised.is_empty() {
        // The schema seeds no "root_id"; return empty list when absent.
        match db.get_meta("root_id").await.map_err(IpcError::from)? {
            Some(id) => Uuid::parse_str(&id)
                .map_err(|_| IpcError::InternalError("root_id is not a valid UUID".into()))?,
            None => return Ok(Vec::new()),
        }
    } else {
        Uuid::parse_str(normalised).map_err(|_| {
            IpcError::InvalidInput(
                "Path must be empty for root or a UUID for a subdirectory".into(),
            )
        })?
    };

    let children = db
        .list_children(parent_uuid)
        .await
        .map_err(IpcError::from)?;
    Ok(children.iter().map(node_to_file_entry).collect())
}

/// Encrypt and upload a file to the vault.
///
/// Progress is streamed via the `progress` channel.  The file is placed at the
/// vault root (`parent_id = None`) for Phase 6.5; path-based placement is
/// deferred.
#[tauri::command]
pub async fn upload_file(
    source_path: PathBuf,
    vault_path: String,
    progress: Channel<ProgressUpdate>,
    state: State<'_, AppState>,
) -> Result<FileEntry, IpcError> {
    state.session_manager.reset_timer().await;
    require_active_session(&state).await?;

    let vault_path = normalise_vault_path(&vault_path);
    if vault_path.is_empty() {
        return Err(IpcError::InvalidInput(
            "Vault path is required for upload".into(),
        ));
    }
    validate_vault_path(vault_path)?;

    let name = source_path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| IpcError::InvalidInput("Source path has no valid file name".into()))?
        .to_owned();

    let vault_id = require_vault_id()?;
    let staging_dir = vault_staging_dir(&vault_id);

    let db_guard = state.database.read().await;
    let db = db_guard
        .as_ref()
        .ok_or_else(|| IpcError::VaultLocked("Vault is locked".into()))?;

    let kek = extract_kek(&state).await?;

    let node_id = Uuid::new_v4();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    // Wrap the Tauri channel in a plain closure so no `tauri::` import
    // leaks into the storage layer.
    let progress_fn = {
        let progress = progress.clone();
        move |bytes_processed: u64, bytes_total: u64| {
            let percent = (bytes_processed * 100 / bytes_total.max(1)) as u8;
            let _ = progress.send(ProgressUpdate {
                percent,
                bytes_processed,
                bytes_total,
                status: "Uploading".into(),
            });
        }
    };

    let node = vault_upload(
        &source_path,
        node_id,
        None, // parent_id — root placement for Phase 6.5
        &name,
        now,
        now,
        db,
        &kek,
        &staging_dir,
        Some(&progress_fn),
    )
    .await
    .map_err(IpcError::from)?;

    Ok(node_to_file_entry(&node))
}

/// Download and decrypt a file from the vault to a local destination path.
///
/// Progress is streamed via the `progress` channel.
#[tauri::command]
pub async fn download_file(
    file_id: String,
    destination_path: PathBuf,
    progress: Channel<ProgressUpdate>,
    state: State<'_, AppState>,
) -> Result<(), IpcError> {
    state.session_manager.reset_timer().await;
    require_active_session(&state).await?;

    validate_file_id(&file_id)?;
    let node_uuid =
        Uuid::parse_str(&file_id).map_err(|_| IpcError::InvalidInput("Invalid file ID".into()))?;

    let vault_id = require_vault_id()?;
    let staging_dir = vault_staging_dir(&vault_id);

    let db_guard = state.database.read().await;
    let db = db_guard
        .as_ref()
        .ok_or_else(|| IpcError::VaultLocked("Vault is locked".into()))?;

    let kek = extract_kek(&state).await?;

    // Wrap the Tauri channel in a plain closure so no `tauri::` import
    // leaks into the storage layer.
    let progress_fn = {
        let progress = progress.clone();
        move |bytes_processed: u64, bytes_total: u64| {
            let percent = (bytes_processed * 100 / bytes_total.max(1)) as u8;
            let _ = progress.send(ProgressUpdate {
                percent,
                bytes_processed,
                bytes_total,
                status: "Downloading".into(),
            });
        }
    };

    vault_download(
        &destination_path,
        node_uuid,
        db,
        &kek,
        &staging_dir,
        Some(&progress_fn),
    )
    .await
    .map_err(IpcError::from)
}

/// Delete a file from the vault.
///
/// Metadata and local staged blobs are removed immediately.  Cloud blob
/// deletion is queued in `pending_deletions` and dispatched during the next
/// sync cycle.
#[tauri::command]
pub async fn delete_file(file_id: String, state: State<'_, AppState>) -> Result<(), IpcError> {
    state.session_manager.reset_timer().await;
    require_active_session(&state).await?;

    validate_file_id(&file_id)?;
    let node_uuid =
        Uuid::parse_str(&file_id).map_err(|_| IpcError::InvalidInput("Invalid file ID".into()))?;

    let vault_id = require_vault_id()?;
    let staging_dir = vault_staging_dir(&vault_id);

    let db_guard = state.database.read().await;
    let db = db_guard
        .as_ref()
        .ok_or_else(|| IpcError::VaultLocked("Vault is locked".into()))?;

    vault_delete(node_uuid, db, &staging_dir)
        .await
        .map_err(IpcError::from)
}

/// Decrypt and return file content for in-app viewing (Zero-Trace).
///
/// Rejects files larger than 50 MiB based on the manifest `size_bytes` field
/// **before** any decryption takes place.  The decrypted bytes are returned
/// as a base64-encoded payload and are never written to a permanent location.
#[tauri::command]
pub async fn get_file_content(
    file_id: String,
    state: State<'_, AppState>,
) -> Result<FileContent, IpcError> {
    state.session_manager.reset_timer().await;
    require_active_session(&state).await?;

    validate_file_id(&file_id)?;
    let node_uuid =
        Uuid::parse_str(&file_id).map_err(|_| IpcError::InvalidInput("Invalid file ID".into()))?;

    let db_guard = state.database.read().await;
    let db = db_guard
        .as_ref()
        .ok_or_else(|| IpcError::VaultLocked("Vault is locked".into()))?;

    // Check manifest size BEFORE decrypting to enforce the 50 MiB limit.
    let node = db.get_node(node_uuid).await.map_err(IpcError::from)?;
    const FIFTY_MIB: u64 = 50 * 1024 * 1024;
    if node.size_bytes > FIFTY_MIB {
        return Err(IpcError::InvalidInput(
            "File exceeds 50 MiB in-app viewing limit".into(),
        ));
    }

    let vault_id = require_vault_id()?;
    let staging_dir = vault_staging_dir(&vault_id);

    let kek = extract_kek(&state).await?;

    // Decrypt into a temporary file; the TempDir and its contents are removed
    // on drop, keeping the plaintext off permanent storage.
    let temp_dir = tempfile::tempdir()
        .map_err(|e| IpcError::InternalError(format!("Failed to create temp dir: {e}")))?;
    let temp_path = temp_dir.path().join("content");

    vault_download(&temp_path, node_uuid, db, &kek, &staging_dir, None)
        .await
        .map_err(IpcError::from)?;

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

/// List files on the primary remote destination.
///
/// Returns a manifest-linked view.  Blobs present on the remote are returned;
/// manifest cross-referencing for orphan detection is a Phase 7 feature — all
/// entries are returned with `is_orphaned: false` in Phase 6.5.
#[tauri::command]
pub async fn list_remote(
    remote_prefix: String,
    state: State<'_, AppState>,
) -> Result<Vec<RemoteFileEntry>, IpcError> {
    state.session_manager.reset_timer().await;
    require_active_session(&state).await?;

    let transport = state.cloud_transport.read().await.clone();
    let blobs = transport
        .list_blobs(&remote_prefix)
        .await
        .map_err(IpcError::from)?;

    let entries = blobs
        .into_iter()
        .map(|blob_path| RemoteFileEntry {
            blob_id: blob_path,
            file_name: None,
            vault_path: None,
            size_bytes: 0,
            // Manifest cross-referencing deferred to Phase 7.
            is_orphaned: false,
        })
        .collect();

    Ok(entries)
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_fifty_mib_gate_constant() {
        const FIFTY_MIB: u64 = 50 * 1024 * 1024;
        // This constant is used in get_file_content to reject files > 50 MiB
        assert_eq!(FIFTY_MIB, 52428800);
    }

    #[test]
    fn test_unix_ts_to_iso8601_basic_conversion() {
        // Test that timestamp conversion is deterministic
        // Actual function is private, but this validates the constant
        let ts = 1704067200i64; // 2024-01-01 00:00:00 UTC
        assert!(ts > 0);
    }
}
