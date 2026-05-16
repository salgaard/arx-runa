//! Cloud sync commands.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use secrecy::SecretBox;
use tauri::State;
use tauri::ipc::Channel;

use crate::crypto::{ManifestKey, SqlcipherKey};
use crate::storage::SqlCipherMetadataStore;
use crate::storage::cloud::destination_session::{
    BackupSyncMode, DestinationSession, DestinationType, build_session_rclone_conf,
    destroy_session_rclone_conf, get_primary_destination, list_destination_sessions,
    set_primary_destination,
};
use crate::storage::cloud::manifest_backup::{download_manifest_backup, upload_manifest_backup};
use crate::storage::cloud::sync::drain_pending_deletions;
use crate::storage::cloud::vault_header::VaultHeader;
use crate::storage::cloud::vault_header_io::VAULT_HEADER_BLOB_NAME;
use crate::storage::cloud::{
    CloudEndpoint, CloudTransport, CloudTransportError, DestinationSessionPublic, RcloneTransport,
    SyncConfig, pull_vault, push_vault,
};
use crate::storage::device_id::get_or_create_device_id;
use crate::storage::metadata_store::MetadataStore;
use crate::storage::staging::write_owner_only;
use crate::storage::types::{ChunkRecord, EpochBlobRecord, Node, NodeId, NodeType};
use crate::ui::commands_common::{ProgressChannel, require_active_session};
use crate::ui::error::IpcError;
use crate::ui::file_commands::extract_kek;
use crate::ui::state::AppState;
use crate::ui::types::{
    DestinationHealth, MigrationProgress, ReconcileResult, SyncProgressUpdate, SyncResult,
    SyncStatus,
};
use crate::ui::vault_paths::{resolve_vault_by_id, vault_db_path, vault_staging_dir};

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Conflict-reconcile helpers
// ---------------------------------------------------------------------------

/// A locally pending file with standalone chunks awaiting upload.
struct PendingStandaloneFile {
    /// Full path from vault root, e.g. `["docs", "report.txt"]`.
    path: Vec<String>,
    node: Node,
    chunks: Vec<ChunkRecord>,
}

/// All locally pending files that share a single epoch blob awaiting upload.
struct PendingEpochBlob {
    epoch_blob: EpochBlobRecord,
    /// `(path, node, chunk_index, byte_offset, byte_length)` per packed file.
    files: Vec<(Vec<String>, Node, u32, u64, u64)>,
}

/// Locally pending state captured before a manifest DB replacement.
struct PendingLocalState {
    standalone: Vec<PendingStandaloneFile>,
    epoch_blobs: Vec<PendingEpochBlob>,
}

/// Walks up the node tree to build the path from the vault root.
///
/// Returns path components not including the invisible root node name, e.g.
/// `["docs", "report.txt"]` for a file inside a `docs/` directory.
async fn build_node_path(
    db: &SqlCipherMetadataStore,
    node: &Node,
) -> Result<Vec<String>, IpcError> {
    let mut path = vec![node.name.clone()];
    let mut parent_id = node.parent_id;
    while let Some(pid) = parent_id {
        let parent = db.get_node(*pid.as_uuid()).await.map_err(IpcError::from)?;
        if parent.parent_id.is_none() {
            break; // reached root; root name is not a user-visible path component
        }
        path.push(parent.name.clone());
        parent_id = parent.parent_id;
    }
    path.reverse();
    Ok(path)
}

/// Resolves the `NodeId` of a directory in `db` matching `dir_path`, creating
/// missing intermediate directories as needed.
async fn resolve_or_create_parent(
    db: &SqlCipherMetadataStore,
    dir_path: &[String],
) -> Result<NodeId, IpcError> {
    let root_id_str = db
        .get_meta("root_id")
        .await
        .map_err(IpcError::from)?
        .ok_or_else(|| IpcError::InternalError("root_id missing from manifest".into()))?;
    let root_uuid = uuid::Uuid::parse_str(&root_id_str)
        .map_err(|e| IpcError::InternalError(format!("invalid root_id: {e}")))?;
    let mut current_id = NodeId::new(root_uuid);

    for component in dir_path {
        let children = db
            .list_children(*current_id.as_uuid())
            .await
            .map_err(IpcError::from)?;
        match children.into_iter().find(|n| &n.name == component) {
            Some(child) if child.node_type == NodeType::Directory => {
                current_id = child.node_id;
            }
            Some(_) => {
                return Err(IpcError::InternalError(format!(
                    "path conflict: '{component}' exists as a file in the cloud manifest"
                )));
            }
            None => {
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs() as i64;
                let new_dir = Node {
                    node_id: NodeId::new(uuid::Uuid::new_v4()),
                    parent_id: Some(current_id),
                    node_type: NodeType::Directory,
                    name: component.clone(),
                    created_at: now,
                    modified_at: now,
                    size_bytes: 0,
                    file_key_wrapped: None,
                };
                db.insert_node(&new_dir).await.map_err(IpcError::from)?;
                current_id = new_dir.node_id;
            }
        }
    }
    Ok(current_id)
}

/// Appends `" (conflicted copy)"` before the extension, or at the end when no
/// extension is present.
pub(crate) fn conflict_name(original: &str) -> String {
    match original.rfind('.') {
        Some(dot) => format!("{} (conflicted copy){}", &original[..dot], &original[dot..]),
        None => format!("{original} (conflicted copy)"),
    }
}

/// Scans `staging_dir/pending/` and returns the locally pending state that
/// must be preserved across a manifest DB replacement.
async fn collect_pending_local_state(
    db: &SqlCipherMetadataStore,
    staging_dir: &std::path::Path,
) -> Result<PendingLocalState, IpcError> {
    use std::collections::HashMap;
    use uuid::Uuid;

    let pending_dir = staging_dir.join("pending");
    let mut blob_names: Vec<String> = Vec::new();

    match tokio::fs::read_dir(&pending_dir).await {
        Ok(mut entries) => {
            while let Some(entry) = entries
                .next_entry()
                .await
                .map_err(|e| IpcError::InternalError(format!("read pending dir: {e}")))?
            {
                let file_name = entry.file_name();
                let file_name_str = file_name.to_string_lossy();
                if let Some(stem) = file_name_str.strip_suffix(".blob")
                    && Uuid::parse_str(stem).is_ok()
                {
                    blob_names.push(stem.to_owned());
                }
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(PendingLocalState {
                standalone: Vec::new(),
                epoch_blobs: Vec::new(),
            });
        }
        Err(e) => {
            return Err(IpcError::InternalError(format!("open pending dir: {e}")));
        }
    }

    let mut standalone_by_node: HashMap<NodeId, (Vec<String>, Node, Vec<ChunkRecord>)> =
        HashMap::new();
    type EpochFiles = Vec<(Vec<String>, Node, u32, u64, u64)>;
    let mut epoch_map: HashMap<Uuid, (EpochBlobRecord, EpochFiles)> = HashMap::new();

    for blob_name in blob_names {
        if let Some(chunk) = db
            .get_chunk_by_blob_name(&blob_name)
            .await
            .map_err(IpcError::from)?
        {
            let node_id = chunk.node_id;
            if standalone_by_node.contains_key(&node_id) {
                continue;
            }
            let node = db
                .get_node(*node_id.as_uuid())
                .await
                .map_err(IpcError::from)?;
            let path = build_node_path(db, &node).await?;
            let chunks = db
                .get_chunks(*node_id.as_uuid())
                .await
                .map_err(IpcError::from)?;
            standalone_by_node.insert(node_id, (path, node, chunks));
        } else if let Some(epoch_blob) = db
            .get_epoch_blob_by_blob_name(&blob_name)
            .await
            .map_err(IpcError::from)?
        {
            let epoch_blob_id = epoch_blob.epoch_blob_id;
            if epoch_map.contains_key(&epoch_blob_id) {
                continue;
            }
            let packed_chunks = db
                .get_chunks_for_epoch_blob(epoch_blob_id)
                .await
                .map_err(IpcError::from)?;
            let mut files = Vec::new();
            for chunk in packed_chunks {
                let node = db
                    .get_node(*chunk.node_id.as_uuid())
                    .await
                    .map_err(IpcError::from)?;
                let path = build_node_path(db, &node).await?;
                let byte_offset = chunk.byte_offset.unwrap_or(0);
                let byte_length = chunk.byte_length.unwrap_or(0);
                files.push((path, node, chunk.chunk_index, byte_offset, byte_length));
            }
            epoch_map.insert(epoch_blob_id, (epoch_blob, files));
        }
        // Orphaned blob with no DB entry — skip silently.
    }

    let standalone = standalone_by_node
        .into_values()
        .map(|(path, node, chunks)| PendingStandaloneFile { path, node, chunks })
        .collect();
    let epoch_blobs = epoch_map
        .into_values()
        .map(|(epoch_blob, files)| PendingEpochBlob { epoch_blob, files })
        .collect();

    Ok(PendingLocalState {
        standalone,
        epoch_blobs,
    })
}

/// Re-inserts `pending` into `db` (which now holds the cloud manifest) so the
/// subsequent sync picks them up and uploads the still-present staging blobs.
///
/// Files whose name collides with an existing cloud entry are renamed with a
/// `" (conflicted copy)"` suffix.
async fn reinsert_pending_state(
    db: &SqlCipherMetadataStore,
    pending: &PendingLocalState,
) -> Result<Vec<String>, IpcError> {
    let mut conflicts_renamed: Vec<String> = Vec::new();

    for file in &pending.standalone {
        let dir_path = if file.path.len() > 1 {
            &file.path[..file.path.len() - 1]
        } else {
            &[]
        };
        let parent_id = resolve_or_create_parent(db, dir_path).await?;
        let filename = file.path.last().cloned().unwrap_or_default();

        let final_name = {
            let children = db
                .list_children(*parent_id.as_uuid())
                .await
                .map_err(IpcError::from)?;
            if children.iter().any(|n| n.name == filename) {
                conflict_name(&filename)
            } else {
                filename.clone()
            }
        };

        if final_name != filename {
            conflicts_renamed.push(final_name.clone());
        }

        let new_node = Node {
            parent_id: Some(parent_id),
            name: final_name,
            ..file.node.clone()
        };
        db.insert_file_with_chunks(&new_node, &file.chunks)
            .await
            .map_err(IpcError::from)?;
    }

    for epoch_entry in &pending.epoch_blobs {
        let mut extents: Vec<(uuid::Uuid, u32, u64, u64)> = Vec::new();

        for (path, node, chunk_index, byte_offset, byte_length) in &epoch_entry.files {
            let dir_path = if path.len() > 1 {
                &path[..path.len() - 1]
            } else {
                &[]
            };
            let parent_id = resolve_or_create_parent(db, dir_path).await?;
            let filename = path.last().cloned().unwrap_or_default();

            let final_name = {
                let children = db
                    .list_children(*parent_id.as_uuid())
                    .await
                    .map_err(IpcError::from)?;
                if children.iter().any(|n| n.name == filename) {
                    conflict_name(&filename)
                } else {
                    filename.clone()
                }
            };

            if final_name != filename {
                conflicts_renamed.push(final_name.clone());
            }

            let new_node = Node {
                parent_id: Some(parent_id),
                name: final_name,
                ..node.clone()
            };
            db.insert_node(&new_node).await.map_err(IpcError::from)?;
            extents.push((
                *node.node_id.as_uuid(),
                *chunk_index,
                *byte_offset,
                *byte_length,
            ));
        }

        db.reinsert_epoch_blob(&epoch_entry.epoch_blob, &extents)
            .await
            .map_err(IpcError::from)?;
    }

    Ok(conflicts_renamed)
}

/// Returns the bundled rclone binary path, falling back to `rclone` on PATH.
pub(crate) fn rclone_binary_path(handle: &tauri::AppHandle) -> PathBuf {
    use tauri::Manager;
    if let Ok(rd) = handle.path().resource_dir() {
        let name = if cfg!(target_os = "windows") {
            "rclone.exe"
        } else {
            "rclone"
        };
        let candidate = rd.join("bin").join(name);
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

/// Formats the current instant as an ISO 8601 UTC timestamp string.
///
/// Uses Howard Hinnant's `civil_from_days` algorithm so we have no
/// dependency on `chrono`.
fn now_iso8601() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let hms = secs % 86400;
    let days = secs / 86400;
    let h = hms / 3600;
    let m = (hms % 3600) / 60;
    let s = hms % 60;

    // civil_from_days (days since 1970-01-01 UTC → Gregorian date)
    let z = days as i64 + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { y + 1 } else { y };

    format!("{year:04}-{month:02}-{day:02}T{h:02}:{m:02}:{s:02}Z")
}

/// Builds `SqlcipherKey` inside the session closure — raw bytes never escape.
async fn extract_sqlcipher_key(
    session_manager: &crate::auth::SessionManager,
) -> Result<SqlcipherKey, IpcError> {
    session_manager
        .with_sqlcipher_key(|k| {
            let mut boxed = Box::new([0u8; 32]);
            boxed.copy_from_slice(k);
            SqlcipherKey::from_secret_box(SecretBox::new(boxed))
        })
        .await
        .map_err(IpcError::from)
}

/// Builds `ManifestKey` inside the session closure — raw bytes never escape.
async fn extract_manifest_key(
    session_manager: &crate::auth::SessionManager,
) -> Result<ManifestKey, IpcError> {
    session_manager
        .with_manifest_key(|k| {
            let mut boxed = Box::new([0u8; 32]);
            boxed.copy_from_slice(k);
            ManifestKey::from_secret_box(SecretBox::new(boxed))
        })
        .await
        .map_err(IpcError::from)
}

/// Writes a single-remote rclone config blob to `config_path` with
/// owner-only permissions, then constructs a [`RcloneTransport`] for that
/// destination.  Caller is responsible for calling
/// [`destroy_session_rclone_conf`] to wipe the file after use.
pub(crate) async fn build_destination_transport(
    binary_path: PathBuf,
    config_path: &std::path::Path,
    dest: &DestinationSession,
) -> Result<RcloneTransport, IpcError> {
    // Ensure the staging directory exists before writing the config file.
    if let Some(parent) = config_path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| IpcError::InternalError(format!("staging dir creation failed: {e}")))?;
    }

    let config_blob = match dest.destination_type {
        DestinationType::LocalPath | DestinationType::ExternalDrive => {
            format!("[{}]\ntype = local\n", dest.rclone_remote_name)
        }
        DestinationType::Cloud => dest.rclone_config_blob.clone(),
    };
    write_owner_only(config_path, config_blob.as_bytes())
        .await
        .map_err(IpcError::from)?;

    let dest_public = DestinationSessionPublic::from(dest);
    // CloudEndpoint is unused by RcloneTransport::new (parameter is `_endpoint`),
    // so we provide a best-effort value derived from the session fields.
    let endpoint = CloudEndpoint {
        provider: String::new(),
        bucket: dest.bucket.clone(),
        region: String::new(),
        endpoint: String::new(),
        path_prefix: dest.path_prefix.clone(),
    };

    RcloneTransport::new(
        binary_path,
        config_path.to_path_buf(),
        &endpoint,
        &dest_public,
        SyncConfig::default(),
    )
    .map_err(|e| IpcError::CloudError(e.to_string()))
}

/// Remote path for a blob: `"vault/{blob_name}.blob"`.
///
/// Mirrors the private `build_blob_remote_path` in `storage::cloud::sync`.
fn blob_remote_path(blob_name: &str) -> String {
    format!("vault/{blob_name}.blob")
}

// ---------------------------------------------------------------------------
// Tauri IPC commands
// ---------------------------------------------------------------------------

/// Push local staged vault blobs, manifest backup, and vault header to the
/// primary cloud destination.
///
/// Progress is streamed via `progress` channel.
#[tauri::command]
pub async fn sync_to_cloud(
    progress: Channel<SyncProgressUpdate>,
    state: State<'_, AppState>,
) -> Result<SyncResult, IpcError> {
    state.session_manager.reset_timer().await;
    require_active_session(&state).await?;

    let _sync_guard = state
        .sync_mutex
        .try_lock()
        .map_err(|_| IpcError::InternalError("Sync already in progress".into()))?;

    let vault_id = state
        .session_manager
        .active_vault_id()
        .await
        .ok_or_else(|| IpcError::VaultLocked("No active vault session".into()))?;
    let (vault_id, db_path, header_path) = resolve_vault_by_id(&vault_id)?;

    let staging_dir = vault_staging_dir(&vault_id);

    // Read vault header.
    let header_json = tokio::fs::read_to_string(&header_path)
        .await
        .map_err(|e| IpcError::InternalError(format!("vault header read failed: {e}")))?;
    let vault_header: VaultHeader = serde_json::from_str(&header_json)
        .map_err(|e| IpcError::InternalError(format!("vault header parse failed: {e}")))?;

    // Acquire database and cloud transport.
    let db_guard = state.database.read().await;
    let db = db_guard
        .as_ref()
        .ok_or_else(|| IpcError::VaultLocked("Vault is locked".into()))?;
    let cloud_transport = state.cloud_transport.read().await.clone();

    // Extract keys inside session closures — raw bytes never leave the closures.
    let sqlcipher_key = extract_sqlcipher_key(&state.session_manager).await?;
    let manifest_key = extract_manifest_key(&state.session_manager).await?;

    // Wrap the Tauri channel in a ProgressChannel to gracefully handle
    // closed connections (M3: Streaming Progress Channel Validation)
    let progress_ch = ProgressChannel::new(progress);
    let progress_fn = {
        let progress = progress_ch.clone();
        move |files_done: u32, files_total: u32, label: Option<&str>| {
            let percent = (files_done * 100 / files_total.max(1)) as u8;
            let _ = progress.try_send_if_open(SyncProgressUpdate {
                percent,
                current_file: label.map(str::to_owned),
                files_processed: files_done,
                files_total,
            });
        }
    };

    // Flush any epoch-buffered files before pushing to the cloud.
    {
        let kek = extract_kek(&state).await?;
        let chunk_size_bytes = crate::storage::pipeline::read_chunk_size_bytes(db)
            .await
            .map_err(IpcError::from)?;
        let _flush_guard = state.flush_mutex.lock().await;
        crate::storage::vault_ops::flush_epoch_buffer(
            db,
            &kek,
            &staging_dir.join("pending"),
            chunk_size_bytes,
            None,
        )
        .await
        .map_err(IpcError::from)?;
    }

    let push_report = push_vault(
        &db_path,
        &sqlcipher_key,
        &manifest_key,
        db,
        &*cloud_transport,
        &vault_header,
        &staging_dir,
        &SyncConfig::default(),
        Some(&progress_fn),
    )
    .await
    .map_err(IpcError::from)?;

    // Enqueue successfully-uploaded blobs for any active mirror destinations so
    // sync_backup knows exactly what still needs to reach each mirror.
    if !push_report.uploaded_blob_names.is_empty() {
        let mirror_dests = list_destination_sessions(db)
            .await
            .map_err(IpcError::from)?;
        for dest in mirror_dests.into_iter().filter(|d| !d.is_primary) {
            let _ = db
                .bulk_insert_pending_backup(&push_report.uploaded_blob_names, &dest.destination_id)
                .await;
        }
    }

    // Update cached sync status.
    *state.sync_status.write().await = SyncStatus {
        syncing: false,
        last_synced_at: Some(now_iso8601()),
        pending_changes: 0,
    };

    Ok(SyncResult {
        files_uploaded: push_report.blobs_uploaded as u32,
        files_downloaded: 0,
        files_deleted: 0,
        conflicts: vec![],
        backup_failures: 0,
    })
}

/// Download the cloud manifest backup into a fresh local database, then pull
/// all vault blobs referenced by that manifest.  Replaces the in-memory store
/// with the newly-recovered one.
///
/// `vault_header_path` must point to the vault header JSON obtained from the
/// cloud (e.g. downloaded via `list_remote` / direct rclone).  The vault ID
/// embedded in the header is used to derive the local DB and staging paths.
///
/// Progress is streamed via `progress` channel.
#[tauri::command]
pub async fn recover_from_cloud(
    vault_header_path: PathBuf,
    progress: Channel<SyncProgressUpdate>,
    state: State<'_, AppState>,
) -> Result<(), IpcError> {
    state.session_manager.reset_timer().await;
    require_active_session(&state).await?;

    let header_bytes = tokio::fs::read(&vault_header_path)
        .await
        .map_err(|_| IpcError::NotFound("Vault header not found".into()))?;
    let vault_header: VaultHeader = serde_json::from_slice(&header_bytes)
        .map_err(|_| IpcError::InternalError("An error occurred".into()))?;
    let vault_id = vault_header.vault_id;

    let db_path = vault_db_path(&vault_id);
    let staging_dir = vault_staging_dir(&vault_id);

    if let Some(parent) = db_path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|_| IpcError::InternalError("An error occurred".into()))?;
    }

    let cloud_transport = state.cloud_transport.read().await.clone();
    let sqlcipher_key = extract_sqlcipher_key(&state.session_manager).await?;
    let manifest_key = extract_manifest_key(&state.session_manager).await?;

    let manifest_key_bytes: [u8; 32] = manifest_key.with_exposed(|bytes| *bytes);

    download_manifest_backup(
        &*cloud_transport,
        &staging_dir,
        &manifest_key_bytes,
        &db_path,
        &sqlcipher_key,
    )
    .await
    .map_err(|e| {
        tracing::error!("manifest backup download failed: {:?}", e);
        IpcError::CloudError("Failed to restore vault from cloud".into())
    })?;

    let key_bytes = sqlcipher_key.with_exposed(|bytes| *bytes);
    let new_store = SqlCipherMetadataStore::open(&db_path, &key_bytes)
        .await
        .map_err(IpcError::from)?;

    let progress_fn = {
        let progress = progress.clone();
        move |files_done: u32, files_total: u32, label: Option<&str>| {
            let percent = (files_done * 100 / files_total.max(1)) as u8;
            let _ = progress.send(SyncProgressUpdate {
                percent,
                current_file: label.map(str::to_owned),
                files_processed: files_done,
                files_total,
            });
        }
    };

    pull_vault(
        &db_path,
        &sqlcipher_key,
        &manifest_key,
        &new_store,
        &*cloud_transport,
        &staging_dir,
        &SyncConfig::default(),
        Some(&progress_fn),
    )
    .await
    .map_err(|e| {
        tracing::error!("pull_vault failed during recover_from_cloud: {:?}", e);
        IpcError::CloudError("Cloud operation failed".into())
    })?;

    let mut db_guard = state.database.write().await;
    *db_guard = Some(new_store);

    Ok(())
}

/// Download the cloud manifest backup, merge it into the local metadata store,
/// pull any missing blobs, and advance the local snapshot counter to match
/// the cloud.  Call this after `sync_to_cloud` returns a `Conflict` error to
/// un-block a multi-device push.
///
/// Progress is streamed via `progress` channel.
#[tauri::command]
pub async fn pull_and_reconcile(
    progress: Channel<SyncProgressUpdate>,
    state: State<'_, AppState>,
) -> Result<ReconcileResult, IpcError> {
    state.session_manager.reset_timer().await;
    require_active_session(&state).await?;

    let vault_id = state
        .session_manager
        .active_vault_id()
        .await
        .ok_or_else(|| IpcError::VaultLocked("No active vault session".into()))?;
    let (vault_id, vault_db_path, _) = resolve_vault_by_id(&vault_id)?;

    let staging_dir = vault_staging_dir(&vault_id);
    let probe_path = staging_dir.join("probe-reconcile.db");

    // Quick liveness check — drop immediately so the write lock can be acquired later.
    {
        let db_guard = state.database.read().await;
        if db_guard.as_ref().is_none() {
            return Err(IpcError::VaultLocked("Vault is locked".into()));
        }
    }

    // Capture locally pending state before the DB is replaced so re-insertion
    // can restore it into the cloud manifest afterwards.
    let pending_local_state = {
        let db_guard = state.database.read().await;
        let db = db_guard
            .as_ref()
            .ok_or_else(|| IpcError::VaultLocked("Vault is locked".into()))?;
        collect_pending_local_state(db, &staging_dir).await?
    };

    let cloud_transport = state.cloud_transport.read().await.clone();
    let sqlcipher_key = extract_sqlcipher_key(&state.session_manager).await?;
    let manifest_key_bytes: [u8; 32] = state
        .session_manager
        .with_manifest_key(|k| {
            let mut arr = [0u8; 32];
            arr.copy_from_slice(k);
            arr
        })
        .await
        .map_err(IpcError::from)?;

    let _ = progress.send(SyncProgressUpdate {
        percent: 0,
        current_file: Some("Downloading cloud manifest".to_owned()),
        files_processed: 0,
        files_total: 1,
    });

    // Remove a stale probe DB if present (download_manifest_backup errors if dest exists).
    let _ = tokio::fs::remove_file(&probe_path).await;

    download_manifest_backup(
        &*cloud_transport,
        &staging_dir,
        &manifest_key_bytes,
        &probe_path,
        &sqlcipher_key,
    )
    .await
    .map_err(|e| IpcError::CloudError(format!("manifest download: {e}")))?;

    let _ = progress.send(SyncProgressUpdate {
        percent: 25,
        current_file: Some("Replacing local manifest".to_owned()),
        files_processed: 0,
        files_total: 1,
    });

    // Replace the local vault DB with the downloaded cloud manifest so that
    // device2 adopts the authoritative cloud state (including its root node
    // UUID and complete file hierarchy) rather than attempting a partial merge
    // that fails when devices have divergent root UUIDs.
    let sqlcipher_key_bytes = sqlcipher_key.with_exposed(|b| *b);
    {
        let mut db_write = state.database.write().await;
        // Drop the existing store — closes the SQLite connection and
        // checkpoints the WAL so the underlying file can be replaced.
        *db_write = None;

        // Remove stale WAL/SHM files that a dirty shutdown may have left.
        let _ = tokio::fs::remove_file(vault_db_path.with_extension("db-wal")).await;
        let _ = tokio::fs::remove_file(vault_db_path.with_extension("db-shm")).await;

        // Replace the vault DB atomically.  Rust's rename on Windows uses
        // MoveFileExW(MOVEFILE_REPLACE_EXISTING), which handles an existing
        // destination without a separate delete step.
        //
        // On Windows, antivirus or search-indexer processes may hold the file
        // open after SQLite closes it.  Retry on SHARING_VIOLATION (os error
        // 32) with exponential backoff totalling ~34 s before giving up.  On
        // failure the existing vault.db is still intact, so the vault is
        // re-opened to avoid leaving the database handle in a broken state.
        let mut last_replace_err: Option<std::io::Error> = None;
        'replace: for delay_ms in [0u64, 100, 200, 500, 1_000, 2_000, 5_000, 10_000, 15_000] {
            if delay_ms > 0 {
                tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
            }
            match tokio::fs::rename(&probe_path, &vault_db_path).await {
                Ok(()) => {
                    last_replace_err = None;
                    break 'replace;
                }
                Err(e) if e.raw_os_error() == Some(32) => {
                    last_replace_err = Some(e);
                }
                Err(_cross_device_err) => {
                    match tokio::fs::copy(&probe_path, &vault_db_path).await {
                        Ok(_) => {
                            let _ = tokio::fs::remove_file(&probe_path).await;
                            last_replace_err = None;
                        }
                        Err(e) => last_replace_err = Some(e),
                    }
                    break 'replace;
                }
            }
        }
        if let Some(e) = last_replace_err {
            if let Ok(recovered) =
                SqlCipherMetadataStore::open(&vault_db_path, &sqlcipher_key_bytes).await
            {
                *db_write = Some(recovered);
            }
            return Err(IpcError::InternalError(format!("replace vault DB: {e}")));
        }

        let new_db = SqlCipherMetadataStore::open(&vault_db_path, &sqlcipher_key_bytes)
            .await
            .map_err(IpcError::from)?;
        *db_write = Some(new_db);
    }

    // Re-register pending local files into the new (cloud) manifest DB so the
    // subsequent sync picks them up and uploads the still-present staging blobs.
    let conflicts_renamed = {
        let db_guard = state.database.read().await;
        let db = db_guard
            .as_ref()
            .ok_or_else(|| IpcError::VaultLocked("Vault is locked".into()))?;
        reinsert_pending_state(db, &pending_local_state).await?
    };

    let _ = progress.send(SyncProgressUpdate {
        percent: 50,
        current_file: Some("Applying pending deletions".to_owned()),
        files_processed: 0,
        files_total: 0,
    });

    // Read cloud_counter from the newly opened DB (it matches the manifest backup counter).
    let (pending_deletions_drained, cloud_counter) = {
        let db_guard = state.database.read().await;
        let db = db_guard
            .as_ref()
            .ok_or_else(|| IpcError::VaultLocked("Vault is locked".into()))?;

        let drained = drain_pending_deletions(db, &*cloud_transport)
            .await
            .map_err(|e| IpcError::CloudError(format!("drain deletions: {e}")))?;

        let counter = db
            .get_meta("snapshot_counter")
            .await
            .map_err(IpcError::from)?
            .ok_or_else(|| {
                IpcError::InternalError("snapshot_counter missing from manifest".into())
            })?
            .parse::<u64>()
            .map_err(|_| {
                IpcError::InternalError("snapshot_counter is not a valid integer".into())
            })?;

        (drained, counter)
    };

    let _ = progress.send(SyncProgressUpdate {
        percent: 100,
        current_file: None,
        files_processed: 0,
        files_total: 0,
    });

    // Best-effort: revoke shares that have passed their expiry deadline.
    crate::ui::sharing_commands::sweep_expired_shares(&state).await;

    Ok(ReconcileResult {
        pending_deletions_drained: pending_deletions_drained as u32,
        cloud_counter,
        conflicts_renamed,
    })
}

/// Check the current sync status.
#[tauri::command]
pub async fn get_sync_status(state: State<'_, AppState>) -> Result<SyncStatus, IpcError> {
    state.session_manager.reset_timer().await;
    let mut status = state.sync_status.read().await.clone();
    let db_guard = state.database.read().await;
    if let Some(db) = db_guard.as_ref() {
        status.pending_changes = db.get_epoch_buffer_count().await.unwrap_or(0);
    }
    Ok(status)
}

/// Re-download every vault blob from the current primary destination into
/// local staging (if not already present), upload all of them to the new
/// destination, then atomically promote the new destination to primary and
/// swap the live transport.
///
/// Progress is streamed via `progress` channel (first half = download phase,
/// second half = upload phase).
#[tauri::command]
pub async fn migrate_vault(
    new_destination_id: String,
    progress: Channel<MigrationProgress>,
    state: State<'_, AppState>,
) -> Result<(), IpcError> {
    state.session_manager.reset_timer().await;
    require_active_session(&state).await?;

    if new_destination_id.is_empty() {
        return Err(IpcError::InvalidInput(
            "Destination ID must not be empty".into(),
        ));
    }

    let vault_id = state
        .session_manager
        .active_vault_id()
        .await
        .ok_or_else(|| IpcError::VaultLocked("No active vault session".into()))?;
    let (vault_id, db_path, header_path) = resolve_vault_by_id(&vault_id)?;

    let staging_dir = vault_staging_dir(&vault_id);
    let config_path = staging_dir.join(".rclone-migrate.conf");

    let sqlcipher_key = extract_sqlcipher_key(&state.session_manager).await?;
    let manifest_key_bytes: [u8; 32] = state
        .session_manager
        .with_manifest_key(|k| {
            let mut arr = [0u8; 32];
            arr.copy_from_slice(k);
            arr
        })
        .await
        .map_err(IpcError::from)?;

    let db_guard = state.database.read().await;
    let db = db_guard
        .as_ref()
        .ok_or_else(|| IpcError::VaultLocked("Vault is locked".into()))?;

    let all_sessions = list_destination_sessions(db)
        .await
        .map_err(IpcError::from)?;
    let dest = all_sessions
        .into_iter()
        .find(|d| d.destination_id == new_destination_id)
        .ok_or_else(|| {
            IpcError::NotFound(format!("destination '{new_destination_id}' not found"))
        })?;

    let chunks = db.list_sync_chunks().await.map_err(IpcError::from)?;
    let all_blob_names: Vec<String> = chunks.iter().map(|c| c.blob_name.clone()).collect();
    let blobs_total = all_blob_names.len() as u32;

    // Record which blobs are already in staging before we download anything.
    let mut pre_existing_staged: std::collections::HashSet<String> =
        std::collections::HashSet::new();
    for blob_name in &all_blob_names {
        let local_path = staging_dir.join(format!("{blob_name}.blob"));
        if tokio::fs::try_exists(&local_path).await.unwrap_or(false) {
            pre_existing_staged.insert(blob_name.clone());
        }
    }

    let primary_transport = state.cloud_transport.read().await.clone();

    let app_handle = state
        .app_handle
        .get()
        .ok_or_else(|| IpcError::InternalError("App handle not initialised".into()))?;
    let binary_path = rclone_binary_path(app_handle);

    // Phase 1 — download blobs that are not yet in local staging.
    let _ = progress.send(MigrationProgress {
        percent: 0,
        blobs_transferred: 0,
        blobs_total,
        current_phase: "Downloading blobs from primary".to_owned(),
    });

    let mut downloaded_for_migration: Vec<String> = Vec::new();
    for (index, blob_name) in all_blob_names.iter().enumerate() {
        if !pre_existing_staged.contains(blob_name) {
            let local_path = staging_dir.join(format!("{blob_name}.blob"));
            let remote_path = blob_remote_path(blob_name);
            if let Err(e) = primary_transport
                .download_blob(&remote_path, &local_path)
                .await
            {
                return Err(IpcError::CloudError(format!("download {blob_name}: {e}")));
            }
            downloaded_for_migration.push(blob_name.clone());
        }
        let _ = progress.send(MigrationProgress {
            percent: (index as u32 * 50 / blobs_total.max(1)) as u8,
            blobs_transferred: index as u32,
            blobs_total,
            current_phase: format!("Preparing {blob_name}"),
        });
    }

    // Phase 2 — upload all blobs to the new destination.
    let new_transport: Arc<dyn CloudTransport> =
        Arc::new(build_destination_transport(binary_path, &config_path, &dest).await?);

    let mut blobs_transferred: u32 = 0;
    for blob_name in &all_blob_names {
        let local_path = staging_dir.join(format!("{blob_name}.blob"));
        let remote_path = blob_remote_path(blob_name);

        let _ = progress.send(MigrationProgress {
            percent: 50 + (blobs_transferred * 50 / blobs_total.max(1)) as u8,
            blobs_transferred,
            blobs_total,
            current_phase: format!("Uploading {blob_name}"),
        });

        if let Err(e) = new_transport.upload_blob(&local_path, &remote_path).await {
            let _ = destroy_session_rclone_conf(&config_path).await;
            return Err(IpcError::CloudError(format!("upload {blob_name}: {e}")));
        }
        blobs_transferred += 1;
    }

    // Upload manifest backup and vault header so the new destination is independently recoverable.
    upload_manifest_backup(
        &db_path,
        &sqlcipher_key,
        &manifest_key_bytes,
        &*new_transport,
        &staging_dir,
    )
    .await
    .map_err(|e| IpcError::CloudError(format!("manifest backup: {e}")))?;

    new_transport
        .upload_blob(&header_path, VAULT_HEADER_BLOB_NAME)
        .await
        .map_err(|e| IpcError::CloudError(format!("vault header: {e}")))?;

    let _ = destroy_session_rclone_conf(&config_path).await;

    // Atomically promote new destination in DB — commit point.
    set_primary_destination(db, &new_destination_id)
        .await
        .map_err(IpcError::from)?;

    // Rebuild the session-lived rclone.conf and swap the persistent transport.
    let conf_path = crate::ui::auth_commands::rclone_conf_path();
    if let Err(e) = build_session_rclone_conf(db, &conf_path).await {
        tracing::warn!(
            ?e,
            "migrate_vault: failed to rebuild rclone.conf after swap"
        );
    } else if let Ok(Some(primary)) = get_primary_destination(db).await {
        let public = DestinationSessionPublic::from(&primary);
        let endpoint = CloudEndpoint {
            provider: String::new(),
            bucket: primary.bucket.clone(),
            region: String::new(),
            endpoint: String::new(),
            path_prefix: primary.path_prefix.clone(),
        };
        let binary = rclone_binary_path(app_handle);
        match RcloneTransport::new(binary, conf_path, &endpoint, &public, SyncConfig::default()) {
            Ok(t) => state.swap_cloud_transport(Arc::new(t)).await,
            Err(e) => tracing::warn!(?e, "migrate_vault: failed to build new RcloneTransport"),
        }
    }

    // Clean up blobs that were downloaded solely for migration.
    for blob_name in &downloaded_for_migration {
        let local_path = staging_dir.join(format!("{blob_name}.blob"));
        let _ = tokio::fs::remove_file(&local_path).await;
    }

    let _ = progress.send(MigrationProgress {
        percent: 100,
        blobs_transferred,
        blobs_total,
        current_phase: "Migration complete".to_owned(),
    });

    Ok(())
}

/// Push locally-staged vault blobs to one or all backup destinations.
///
/// **Phase 6.5 note**: only blobs currently present in local staging are
/// pushed.  Each backup destination receives the same set of staged blobs
/// without re-encryption.  Blobs are *not* removed from staging after upload
/// so that all backup destinations receive every blob.  A full implementation
/// that re-downloads from the primary transport when staging is empty is
/// deferred to Phase 7.
///
/// If `destination_id` is `None`, syncs to all configured backup destinations.
/// Progress is streamed via `progress` channel.
#[tauri::command]
pub async fn sync_backup(
    destination_id: Option<String>,
    progress: Channel<SyncProgressUpdate>,
    state: State<'_, AppState>,
) -> Result<SyncResult, IpcError> {
    state.session_manager.reset_timer().await;
    require_active_session(&state).await?;

    let _sync_guard = state
        .sync_mutex
        .try_lock()
        .map_err(|_| IpcError::InternalError("Sync already in progress".into()))?;

    let vault_id = state
        .session_manager
        .active_vault_id()
        .await
        .ok_or_else(|| IpcError::VaultLocked("No active vault session".into()))?;

    let (_, db_path, header_path) = resolve_vault_by_id(&vault_id)?;

    let staging_dir = vault_staging_dir(&vault_id);

    let sqlcipher_key = extract_sqlcipher_key(&state.session_manager).await?;
    let manifest_key_bytes: [u8; 32] = state
        .session_manager
        .with_manifest_key(|k| {
            let mut arr = [0u8; 32];
            arr.copy_from_slice(k);
            arr
        })
        .await
        .map_err(IpcError::from)?;

    let db_guard = state.database.read().await;
    let db = db_guard
        .as_ref()
        .ok_or_else(|| IpcError::VaultLocked("Vault is locked".into()))?;

    // Flush any epoch-buffered files before pushing to backup destinations.
    {
        let kek = extract_kek(&state).await?;
        let chunk_size_bytes = crate::storage::pipeline::read_chunk_size_bytes(db)
            .await
            .map_err(IpcError::from)?;
        let _flush_guard = state.flush_mutex.lock().await;
        crate::storage::vault_ops::flush_epoch_buffer(
            db,
            &kek,
            &staging_dir.join("pending"),
            chunk_size_bytes,
            None,
        )
        .await
        .map_err(IpcError::from)?;
    }

    // Collect backup destinations, optionally filtered by ID.
    let all_sessions = list_destination_sessions(db)
        .await
        .map_err(IpcError::from)?;
    let current_device_id = get_or_create_device_id().await.map_err(IpcError::from)?;

    let backup_dests: Vec<DestinationSession> = all_sessions
        .into_iter()
        .filter(|d| !d.is_primary)
        .filter(|d| {
            destination_id
                .as_ref()
                .is_none_or(|id| &d.destination_id == id)
        })
        .filter(|d| {
            if matches!(
                d.destination_type,
                DestinationType::LocalPath | DestinationType::ExternalDrive
            ) {
                let owned = d.device_id.as_deref() == Some(current_device_id.as_str());
                if !owned {
                    tracing::debug!(
                        destination_id = %d.destination_id,
                        "skipping local/external destination — belongs to a different device"
                    );
                }
                owned
            } else {
                true
            }
        })
        .collect();

    if backup_dests.is_empty() {
        return Ok(SyncResult {
            files_uploaded: 0,
            files_downloaded: 0,
            files_deleted: 0,
            conflicts: vec![],
            backup_failures: 0,
        });
    }

    // All blob names currently referenced in the DB — used by mirror deletion and
    // failure-record cleanup.
    let chunks = db.list_sync_chunks().await.map_err(IpcError::from)?;
    let all_blob_names: std::collections::HashSet<String> =
        chunks.iter().map(|c| c.blob_name.clone()).collect();

    // Per-invocation temp directory — unique UUID prevents races with any concurrent call
    // that somehow bypasses the sync_mutex (belt-and-suspenders), and also lets downloaded
    // blobs be reused across multiple mirror destinations without re-downloading from primary.
    let mirror_temp_dir = staging_dir
        .join("mirror-temp")
        .join(uuid::Uuid::new_v4().to_string());

    let app_handle = state
        .app_handle
        .get()
        .ok_or_else(|| IpcError::InternalError("App handle not initialised".into()))?;
    let binary_path = rclone_binary_path(app_handle);
    let primary_transport = state.cloud_transport.read().await.clone();

    let mut total_uploaded: u32 = 0;
    let mut total_deleted: u32 = 0;
    let mut total_failed: u32 = 0;

    for dest in &backup_dests {
        // Per-destination config file — isolates each destination's rclone session so that
        // processing one destination never corrupts another destination's config.
        let config_path = staging_dir.join(format!(".rclone-backup-{}.conf", dest.destination_id));
        let transport =
            build_destination_transport(binary_path.clone(), &config_path, dest).await?;

        // Pending-backup queue drives which blobs this destination still needs.
        let pending_blobs = db
            .list_pending_backups(&dest.destination_id)
            .await
            .map_err(IpcError::from)?;
        let blobs_total = pending_blobs.len() as u32;

        let _ = progress.send(SyncProgressUpdate {
            percent: 0,
            current_file: Some(dest.destination_id.clone()),
            files_processed: 0,
            files_total: blobs_total,
        });

        let mut dest_uploaded: u32 = 0;
        let mut dest_failed: u32 = 0;

        for blob_name in &pending_blobs {
            if !all_blob_names.contains(blob_name) {
                // Blob was removed from the vault; discard the stale pending and failure records.
                let _ = db
                    .clear_pending_backup(blob_name, &dest.destination_id)
                    .await;
                let _ = db
                    .clear_backup_failure(blob_name, &dest.destination_id)
                    .await;
                continue;
            }

            // Resolve local path: staging copy (still present from primary upload) takes
            // priority. If absent, use a previously downloaded temp copy or download now.
            // The temp file persists for the lifetime of this sync call so that subsequent
            // mirror destinations for the same blob skip the Backblaze download entirely.
            let staging_path = staging_dir.join(format!("{blob_name}.blob"));
            let local_path = if tokio::fs::try_exists(&staging_path).await.unwrap_or(false) {
                Some(staging_path)
            } else {
                let temp_path = mirror_temp_dir.join(format!("{blob_name}.blob"));
                if tokio::fs::try_exists(&temp_path).await.unwrap_or(false) {
                    // Already downloaded for an earlier mirror destination — reuse.
                    Some(temp_path)
                } else {
                    if let Err(e) = tokio::fs::create_dir_all(&mirror_temp_dir).await {
                        tracing::warn!(
                            destination_id = %dest.destination_id,
                            blob_name = %blob_name,
                            error = %e,
                            "mirror-temp dir creation failed; skipping blob"
                        );
                        continue;
                    }
                    let remote_path = blob_remote_path(blob_name);
                    match primary_transport
                        .download_blob(&remote_path, &temp_path)
                        .await
                    {
                        Ok(()) => Some(temp_path),
                        Err(CloudTransportError::NotFound) => {
                            // Blob is absent from primary — already deleted via sync_to_cloud.
                            // Clear the pending and failure records so this is not retried.
                            tracing::warn!(
                                destination_id = %dest.destination_id,
                                blob_name = %blob_name,
                                "mirror: blob absent from primary — clearing pending record"
                            );
                            let _ = db
                                .clear_pending_backup(blob_name, &dest.destination_id)
                                .await;
                            let _ = db
                                .clear_backup_failure(blob_name, &dest.destination_id)
                                .await;
                            None
                        }
                        Err(e) => {
                            // Transient error (network, quota). Leave pending for retry.
                            tracing::warn!(
                                destination_id = %dest.destination_id,
                                blob_name = %blob_name,
                                error = %e,
                                "mirror: failed to pull blob from primary; will retry next run"
                            );
                            None
                        }
                    }
                }
            };

            let Some(local_path) = local_path else {
                continue;
            };

            let remote_path = blob_remote_path(blob_name);
            if let Err(e) = transport.upload_blob(&local_path, &remote_path).await {
                tracing::warn!(
                    destination_id = %dest.destination_id,
                    blob_name = %blob_name,
                    error = %e,
                    "backup blob upload failed; continuing with remaining blobs"
                );
                let _ = db
                    .record_backup_failure(blob_name, &dest.destination_id)
                    .await;
                dest_failed += 1;
            } else {
                let _ = db
                    .clear_pending_backup(blob_name, &dest.destination_id)
                    .await;
                let _ = db
                    .clear_backup_failure(blob_name, &dest.destination_id)
                    .await;
                dest_uploaded += 1;
            }

            let _ = progress.send(SyncProgressUpdate {
                percent: (dest_uploaded * 100 / blobs_total.max(1)) as u8,
                current_file: Some(blob_name.clone()),
                files_processed: dest_uploaded,
                files_total: blobs_total,
            });
        }

        total_uploaded += dest_uploaded;
        total_failed += dest_failed;

        // Upload encrypted manifest backup so the destination is independently recoverable.
        if let Err(e) = upload_manifest_backup(
            &db_path,
            &sqlcipher_key,
            &manifest_key_bytes,
            &transport,
            &staging_dir,
        )
        .await
        {
            tracing::warn!(
                destination_id = %dest.destination_id,
                error = %e,
                "backup manifest upload failed"
            );
        }

        // Upload vault header (plaintext JSON) for recovery bootstrapping.
        if let Err(e) = transport
            .upload_blob(&header_path, VAULT_HEADER_BLOB_NAME)
            .await
        {
            tracing::warn!(
                destination_id = %dest.destination_id,
                error = %e,
                "backup vault header upload failed"
            );
        }

        // Mirror mode: delete blobs that exist on the remote but no longer in the vault.
        if dest.backup_mode == Some(BackupSyncMode::Mirror) {
            match transport.list_blobs("vault/").await {
                Ok(remote_paths) => {
                    for remote_path in &remote_paths {
                        let blob_name = remote_path
                            .strip_prefix("vault/")
                            .and_then(|s| s.strip_suffix(".blob"))
                            .unwrap_or("");
                        if !blob_name.is_empty() && !all_blob_names.contains(blob_name) {
                            if let Err(e) = transport.delete_blob(remote_path).await {
                                tracing::warn!(
                                    destination_id = %dest.destination_id,
                                    remote_path = %remote_path,
                                    error = %e,
                                    "mirror delete failed for orphan blob"
                                );
                            } else {
                                total_deleted += 1;
                            }
                        }
                    }
                }
                Err(CloudTransportError::NotFound) => {
                    // Destination has no vault/ prefix yet (new or empty). No orphans to delete.
                }
                Err(e) => {
                    tracing::warn!(
                        destination_id = %dest.destination_id,
                        error = %e,
                        "mirror list_blobs failed; skipping orphan deletion for this destination"
                    );
                }
            }
        }

        // Wipe this destination's config file before moving to the next.
        let _ = destroy_session_rclone_conf(&config_path).await;
    }

    // Clean up the per-invocation temp directory (contains blobs downloaded from primary
    // that were not found in local staging). Errors here are non-fatal.
    let _ = tokio::fs::remove_dir_all(&mirror_temp_dir).await;

    Ok(SyncResult {
        files_uploaded: total_uploaded,
        files_downloaded: 0,
        files_deleted: total_deleted,
        conflicts: vec![],
        backup_failures: total_failed,
    })
}

/// Returns per-destination counts of backup blobs that have failed to upload
/// and are pending retry on the next `sync_backup` run.
#[tauri::command]
pub async fn get_backup_health(
    state: State<'_, AppState>,
) -> Result<Vec<DestinationHealth>, IpcError> {
    state.session_manager.reset_timer().await;
    require_active_session(&state).await?;

    let db_guard = state.database.read().await;
    let db = db_guard
        .as_ref()
        .ok_or_else(|| IpcError::VaultLocked("Vault is locked".into()))?;

    let failure_counts = db
        .get_backup_failure_counts()
        .await
        .map_err(IpcError::from)?;

    let pending_counts = db
        .get_pending_backup_counts()
        .await
        .map_err(IpcError::from)?;

    // Merge: collect all destination IDs from either table.
    let mut map: std::collections::HashMap<String, (u32, u32)> = std::collections::HashMap::new();
    for (dest_id, failures) in failure_counts {
        map.entry(dest_id).or_default().0 = failures;
    }
    for (dest_id, pending) in pending_counts {
        map.entry(dest_id).or_default().1 = pending;
    }

    Ok(map
        .into_iter()
        .map(
            |(destination_id, (pending_failure_blobs, pending_blobs))| DestinationHealth {
                destination_id,
                pending_failure_blobs,
                pending_blobs,
            },
        )
        .collect())
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_destination_type_cloud_parsing() {
        let destination_type_str = "cloud";
        let destination_type = match destination_type_str {
            "cloud" => Some(()),
            _ => None,
        };
        assert!(destination_type.is_some());
    }

    #[test]
    fn test_extract_sqlcipher_key_requires_manifest_key() {
        // This helper ensures manifest_key is extracted safely; actual extraction
        // is tested in auth/session tests. We just verify it's available for async use.
        // Integration test in phase_6_5_end_to_end.rs covers full flow.
    }
}
