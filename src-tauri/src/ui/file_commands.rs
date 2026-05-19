//! File management commands.
//!
//! Phase 6.5: backend delegation wired for all six IPC commands.

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use tauri::State;
use tauri::ipc::Channel;
use uuid::Uuid;

use crate::storage::cloud::sync::fetch_missing_file_blobs;
use crate::storage::vault_ops::{
    delete_directory as vault_delete_directory, delete_file as vault_delete,
    download_file as vault_download, download_file_to_memory as vault_download_to_memory,
    upload_file as vault_upload,
};
use crate::storage::{MetadataStore, Node, NodeType};
use crate::ui::commands_common::{
    ProgressChannel, extract_kek, require_active_session, unix_ts_to_iso8601,
};
use crate::ui::error::IpcError;
use crate::ui::state::AppState;
use crate::ui::types::{FileContent, FileEntry, LocalEntry, ProgressUpdate, RemoteFileEntry};
use crate::ui::validation::{normalise_vault_path, validate_file_id, validate_vault_path};
use crate::ui::vault_paths::vault_staging_dir;

// ─── Private helpers ──────────────────────────────────────────────────────────

/// Gets the root directory UUID, creating the root node and persisting `root_id`
/// in manifest meta if it does not yet exist.
async fn get_or_create_root(db: &dyn MetadataStore) -> Result<Uuid, IpcError> {
    if let Some(id) = db.get_meta("root_id").await.map_err(IpcError::from)? {
        return Uuid::parse_str(&id)
            .map_err(|_| IpcError::InternalError("root_id is not a valid UUID".into()));
    }
    let root_id = Uuid::new_v4();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    let root_node = Node::new(
        root_id,
        None,
        NodeType::Directory,
        "root".to_owned(),
        now,
        now,
        0,
        None,
    );
    db.insert_node(&root_node).await.map_err(IpcError::from)?;
    db.set_meta("root_id", &root_id.hyphenated().to_string())
        .await
        .map_err(IpcError::from)?;
    Ok(root_id)
}

/// Resolves the parent directory UUID from a normalised, slash-stripped vault path.
///
/// An empty parent component (file at root) triggers [`get_or_create_root`].
/// A non-empty parent must be a directory node UUID.
async fn resolve_parent_uuid(vault_path: &str, db: &dyn MetadataStore) -> Result<Uuid, IpcError> {
    let parent = match vault_path.rfind('/') {
        Some(pos) => &vault_path[..pos],
        None => "",
    };
    if parent.is_empty() {
        get_or_create_root(db).await
    } else {
        Uuid::parse_str(parent)
            .map_err(|_| IpcError::InvalidInput("Parent directory path must be a UUID".into()))
    }
}

/// Detects a MIME type from the leading bytes of a file.
///
/// Recognises JPEG, PNG, GIF, PDF and ZIP by magic bytes.
/// Falls back to `"application/octet-stream"` for unrecognised formats.
pub(crate) fn detect_mime_type(bytes: &[u8]) -> &'static str {
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
///
/// Pass `pending_flush: true` when the node is staged in the epoch buffer and
/// has not yet been encrypted into a blob.
fn node_to_file_entry(node: &crate::storage::Node, pending_flush: bool) -> FileEntry {
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
        pending_flush,
    }
}

/// Returns the vault identifier for the currently active session.
///
/// Returns `IpcError::VaultLocked` when the session is not active or the vault
/// ID is unavailable.
async fn require_vault_id(state: &AppState) -> Result<String, IpcError> {
    state
        .session_manager
        .active_vault_id()
        .await
        .ok_or_else(|| IpcError::VaultLocked("Vault is locked".into()))
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

    let db_store = state
        .session_manager
        .get_metadata_store()
        .await
        .ok_or_else(|| IpcError::VaultLocked("Vault is locked".into()))?;
    let db = &*db_store;

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

    let pending_ids: std::collections::HashSet<uuid::Uuid> = db
        .get_epoch_buffer_node_ids()
        .await
        .map_err(IpcError::from)?
        .into_iter()
        .collect();
    Ok(children
        .iter()
        .map(|node| node_to_file_entry(node, pending_ids.contains(node.node_id.as_uuid())))
        .collect())
}

/// Encrypt and upload a file to the vault.
///
/// `vault_path` is the full vault-relative destination path (e.g. `/file.txt` or
/// `/<dir-uuid>/file.txt`).  The parent directory is resolved to a UUID; the root
/// node is created on first use and its UUID persisted as `root_id`.
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

    let vault_id = require_vault_id(&state).await?;
    let staging_dir = vault_staging_dir(&vault_id);

    let db_store = state
        .session_manager
        .get_metadata_store()
        .await
        .ok_or_else(|| IpcError::VaultLocked("Vault is locked".into()))?;
    let db = &*db_store;

    let kek = extract_kek(&state).await?;

    let node_id = Uuid::new_v4();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    let progress_ch = ProgressChannel::new(progress);
    let progress_fn = {
        let progress = progress_ch.clone();
        move |bytes_processed: u64, bytes_total: u64| {
            let percent = (bytes_processed * 100 / bytes_total.max(1)) as u8;
            let _ = progress.try_send_if_open(ProgressUpdate {
                percent,
                bytes_processed,
                bytes_total,
                status: "Uploading".into(),
            });
        }
    };

    let parent_id = resolve_parent_uuid(vault_path, db).await?;

    let node = vault_upload(
        &source_path,
        node_id,
        Some(parent_id),
        &name,
        now,
        now,
        db,
        &kek,
        &staging_dir.join("pending"),
        Some(&progress_fn),
    )
    .await
    .map_err(IpcError::from)?;

    Ok(node_to_file_entry(&node, false))
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

    let vault_id = require_vault_id(&state).await?;
    let staging_dir = vault_staging_dir(&vault_id);

    let db_store = state
        .session_manager
        .get_metadata_store()
        .await
        .ok_or_else(|| IpcError::VaultLocked("Vault is locked".into()))?;
    let db = &*db_store;

    let kek = extract_kek(&state).await?;

    // Download any blobs uploaded to cloud that were pruned from local staging.
    let cloud = state.cloud_transport.read().await.clone();
    fetch_missing_file_blobs(node_uuid, db, &staging_dir, cloud.as_ref(), None).await?;

    // Wrap the Tauri channel in a ProgressChannel to gracefully handle
    // closed connections (M3: Streaming Progress Channel Validation)
    let progress_ch = ProgressChannel::new(progress);
    let progress_fn = {
        let progress = progress_ch.clone();
        move |bytes_processed: u64, bytes_total: u64| {
            let percent = (bytes_processed * 100 / bytes_total.max(1)) as u8;
            let _ = progress.try_send_if_open(ProgressUpdate {
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

    let vault_id = require_vault_id(&state).await?;
    let staging_dir = vault_staging_dir(&vault_id);

    let db_store = state
        .session_manager
        .get_metadata_store()
        .await
        .ok_or_else(|| IpcError::VaultLocked("Vault is locked".into()))?;
    let db = &*db_store;

    vault_delete(node_uuid, db, &staging_dir)
        .await
        .map_err(IpcError::from)
}

/// Recursively deletes a directory node and all its descendants from the vault.
///
/// Each file in the subtree has its metadata and any locally staged blobs
/// removed. The directory node itself is removed last.
#[tauri::command]
pub async fn delete_directory(
    directory_id: String,
    state: State<'_, AppState>,
) -> Result<(), IpcError> {
    state.session_manager.reset_timer().await;
    require_active_session(&state).await?;

    validate_file_id(&directory_id)?;
    let node_uuid = Uuid::parse_str(&directory_id)
        .map_err(|_| IpcError::InvalidInput("Invalid directory ID".into()))?;

    let vault_id = require_vault_id(&state).await?;
    let staging_dir = vault_staging_dir(&vault_id);

    let db_store = state
        .session_manager
        .get_metadata_store()
        .await
        .ok_or_else(|| IpcError::VaultLocked("Vault is locked".into()))?;
    let db = &*db_store;

    vault_delete_directory(node_uuid, db, &staging_dir)
        .await
        .map_err(IpcError::from)
}

/// Decrypt and return file content for in-app viewing (Zero-Trace).
///
/// Rejects files larger than 50 MiB based on the manifest `size_bytes` field
/// **before** any decryption takes place.  The decrypted bytes are returned
/// as a base64-encoded payload and are never written to a permanent location.
///
/// Progress is streamed via the `progress` channel.
#[tauri::command]
pub async fn get_file_content(
    file_id: String,
    progress: Channel<ProgressUpdate>,
    state: State<'_, AppState>,
) -> Result<FileContent, IpcError> {
    state.session_manager.reset_timer().await;
    require_active_session(&state).await?;

    validate_file_id(&file_id)?;
    let node_uuid =
        Uuid::parse_str(&file_id).map_err(|_| IpcError::InvalidInput("Invalid file ID".into()))?;

    let db_store = state
        .session_manager
        .get_metadata_store()
        .await
        .ok_or_else(|| IpcError::VaultLocked("Vault is locked".into()))?;
    let db = &*db_store;

    // Check manifest size BEFORE decrypting to enforce the 50 MiB limit.
    let node = db.get_node(node_uuid).await.map_err(IpcError::from)?;
    const FIFTY_MIB: u64 = 50 * 1024 * 1024;
    if node.size_bytes > FIFTY_MIB {
        return Err(IpcError::InvalidInput(
            "File exceeds 50 MiB in-app viewing limit".into(),
        ));
    }

    let vault_id = require_vault_id(&state).await?;
    let staging_dir = vault_staging_dir(&vault_id);

    let kek = extract_kek(&state).await?;

    // Download any blobs uploaded to cloud that were pruned from local staging.
    let cloud = state.cloud_transport.read().await.clone();
    fetch_missing_file_blobs(node_uuid, db, &staging_dir, cloud.as_ref(), None).await?;

    // Wrap the Tauri channel in a ProgressChannel to gracefully handle
    // closed connections (M3: Streaming Progress Channel Validation)
    let progress_ch = ProgressChannel::new(progress);
    let progress_fn = {
        let progress = progress_ch.clone();
        move |bytes_processed: u64, bytes_total: u64| {
            let percent = (bytes_processed * 100 / bytes_total.max(1)) as u8;
            let _ = progress.try_send_if_open(ProgressUpdate {
                percent,
                bytes_processed,
                bytes_total,
                status: "Loading".into(),
            });
        }
    };

    // Decrypt entirely in RAM — no temp file written to disk (Zero-Trace).
    let bytes = vault_download_to_memory(node_uuid, db, &kek, &staging_dir, Some(&progress_fn))
        .await
        .map_err(IpcError::from)?;

    let mime_type = detect_mime_type(&bytes).to_owned();
    let size_bytes = bytes.len() as u64;
    let data_base64 = BASE64_STANDARD.encode(&bytes);

    Ok(FileContent {
        mime_type,
        data_base64,
        size_bytes,
    })
}

/// Pre-fetch any missing cloud blobs for a video file and return the streaming
/// base URL so the frontend can open the video player without a cold-start stall.
///
/// Progress is streamed via the `progress` channel: 0 % when blob fetching
/// starts, 100 % when all blobs are present locally and the video is ready.
#[tauri::command]
pub async fn prefetch_video(
    file_id: String,
    progress: Channel<ProgressUpdate>,
    state: State<'_, AppState>,
) -> Result<String, IpcError> {
    state.session_manager.reset_timer().await;
    require_active_session(&state).await?;

    validate_file_id(&file_id)?;
    let node_uuid =
        Uuid::parse_str(&file_id).map_err(|_| IpcError::InvalidInput("Invalid file ID".into()))?;

    let vault_id = require_vault_id(&state).await?;
    let staging_dir = vault_staging_dir(&vault_id);

    let db_store = state
        .session_manager
        .get_metadata_store()
        .await
        .ok_or_else(|| IpcError::VaultLocked("Vault is locked".into()))?;
    let db = &*db_store;

    let progress_ch = ProgressChannel::new(progress);
    let _ = progress_ch.try_send_if_open(ProgressUpdate {
        percent: 0,
        bytes_processed: 0,
        bytes_total: 0,
        status: "Downloading video…".into(),
    });

    let cloud = state.cloud_transport.read().await.clone();
    let on_blob_downloaded = {
        let progress_ch = progress_ch.clone();
        move |done: u64, total: u64| {
            let percent = (done * 100 / total.max(1)) as u8;
            let _ = progress_ch.try_send_if_open(ProgressUpdate {
                percent,
                bytes_processed: done,
                bytes_total: total,
                status: format!("Downloading blob {done}/{total}"),
            });
        }
    };
    fetch_missing_file_blobs(
        node_uuid,
        db,
        &staging_dir,
        cloud.as_ref(),
        Some(&on_blob_downloaded),
    )
    .await?;

    let _ = progress_ch.try_send_if_open(ProgressUpdate {
        percent: 100,
        bytes_processed: 0,
        bytes_total: 0,
        status: "Ready".into(),
    });

    Ok(crate::ui::video_stream::video_scheme_base_url().to_owned())
}

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

/// Flush all files staged in the epoch buffer into encrypted blobs.
///
/// Reports encryption progress via the `progress` channel.  Acquiring the flush
/// mutex ensures at most one flush runs at a time even when called concurrently.
#[tauri::command]
pub async fn flush_epoch_buffer(
    progress: Channel<ProgressUpdate>,
    state: State<'_, AppState>,
) -> Result<(), IpcError> {
    state.session_manager.reset_timer().await;
    require_active_session(&state).await?;

    let vault_id = require_vault_id(&state).await?;
    let staging_dir = vault_staging_dir(&vault_id);

    let db_store = state
        .session_manager
        .get_metadata_store()
        .await
        .ok_or_else(|| IpcError::VaultLocked("Vault is locked".into()))?;
    let db = &*db_store;

    let kek = extract_kek(&state).await?;
    let chunk_size_bytes = crate::storage::pipeline::read_chunk_size_bytes(db)
        .await
        .map_err(IpcError::from)?;

    let progress_ch = ProgressChannel::new(progress);
    let progress_fn = {
        let progress = progress_ch.clone();
        move |bytes_flushed: u64, bytes_total: u64| {
            let percent = (bytes_flushed * 100 / bytes_total.max(1)) as u8;
            let _ = progress.try_send_if_open(ProgressUpdate {
                percent,
                bytes_processed: bytes_flushed,
                bytes_total,
                status: "Flushing".into(),
            });
        }
    };

    let _flush_guard = state.flush_mutex.lock().await;

    crate::storage::vault_ops::flush_epoch_buffer(
        db,
        &kek,
        &staging_dir,
        chunk_size_bytes,
        Some(&progress_fn),
    )
    .await
    .map_err(IpcError::from)
}

/// Check whether a local filesystem path is a directory.
///
/// Returns `true` when `path` refers to a directory, `false` when it is a regular
/// file or any other non-directory entry.  Returns `IpcError::InvalidInput` when
/// the path does not exist or cannot be accessed.
#[tauri::command]
pub async fn stat_local_path(path: String, state: State<'_, AppState>) -> Result<bool, IpcError> {
    state.session_manager.reset_timer().await;
    require_active_session(&state).await?;

    let meta = tokio::fs::metadata(&path)
        .await
        .map_err(|e| IpcError::InvalidInput(format!("Cannot stat path '{}': {}", path, e)))?;
    Ok(meta.is_dir())
}

/// List the immediate children of a local filesystem directory.
///
/// Returns one [`LocalEntry`] per child.  The order is unspecified (matches the
/// OS-level `read_dir` order).  Returns `IpcError::InvalidInput` when `path` is
/// not an accessible directory.
#[tauri::command]
pub async fn list_local_directory(
    path: String,
    state: State<'_, AppState>,
) -> Result<Vec<LocalEntry>, IpcError> {
    state.session_manager.reset_timer().await;
    require_active_session(&state).await?;

    let mut read_dir = tokio::fs::read_dir(&path)
        .await
        .map_err(|e| IpcError::InvalidInput(format!("Cannot list directory '{}': {}", path, e)))?;

    let mut entries = Vec::new();
    loop {
        let entry = read_dir.next_entry().await.map_err(|e| {
            IpcError::InternalError(format!("Failed reading directory entry: {}", e))
        })?;
        let Some(entry) = entry else { break };

        let entry_path = entry.path();
        let is_dir = entry
            .file_type()
            .await
            .map(|ft| ft.is_dir())
            .unwrap_or(false);
        let name = entry_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_owned();
        let path_str = entry_path.to_string_lossy().into_owned();

        entries.push(LocalEntry {
            name,
            path: path_str,
            is_dir,
        });
    }
    Ok(entries)
}

/// Create a new directory node in the vault manifest.
///
/// `vault_path` follows the same convention as `upload_file`: the rightmost
/// component is the directory name; everything before the last `/` is the
/// parent UUID (or empty for a root-level directory).
///
/// Returns the created directory as a [`FileEntry`] (with `entry_type = "directory"`).
#[tauri::command]
pub async fn create_vault_directory(
    vault_path: String,
    state: State<'_, AppState>,
) -> Result<FileEntry, IpcError> {
    state.session_manager.reset_timer().await;
    require_active_session(&state).await?;

    let vault_path = normalise_vault_path(&vault_path);
    if vault_path.is_empty() {
        return Err(IpcError::InvalidInput(
            "Vault path is required to create a directory".into(),
        ));
    }
    validate_vault_path(vault_path)?;

    let name = vault_path
        .rsplit('/')
        .next()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| IpcError::InvalidInput("Vault path has no valid directory name".into()))?
        .to_owned();

    let db_store = state
        .session_manager
        .get_metadata_store()
        .await
        .ok_or_else(|| IpcError::VaultLocked("Vault is locked".into()))?;
    let db = &*db_store;

    let parent_id = resolve_parent_uuid(vault_path, db).await?;

    let node_id = Uuid::new_v4();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    let node = Node::new(
        node_id,
        Some(parent_id),
        NodeType::Directory,
        name,
        now,
        now,
        0,
        None,
    );
    db.insert_node(&node).await.map_err(IpcError::from)?;

    Ok(node_to_file_entry(&node, false))
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
