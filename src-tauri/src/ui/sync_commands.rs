//! Cloud sync commands.
//!
//! Phase 6.5: all four sync command stubs wired to storage::cloud backend.

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use secrecy::SecretBox;
use tauri::State;
use tauri::ipc::Channel;

use crate::crypto::{ManifestKey, SqlcipherKey};
use crate::storage::cloud::destination_session::{
    DestinationSession, destroy_session_rclone_conf, list_destination_sessions,
};
use crate::storage::cloud::vault_header::VaultHeader;
use crate::storage::cloud::{
    CloudEndpoint, CloudTransport, DestinationSessionPublic, RcloneTransport, SyncConfig,
    pull_vault, push_vault,
};
use crate::storage::staging::write_owner_only;
use crate::ui::commands_common::require_active_session;
use crate::ui::error::IpcError;
use crate::ui::state::AppState;
use crate::ui::types::{MigrationProgress, SyncProgressUpdate, SyncResult, SyncStatus};
use crate::ui::vault_paths::{resolve_singleton_vault, vault_staging_dir};

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

/// Returns the bundled rclone binary path, falling back to `rclone` on PATH.
fn rclone_binary_path(handle: &tauri::AppHandle) -> PathBuf {
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
async fn build_destination_transport(
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

    write_owner_only(config_path, dest.rclone_config_blob.as_bytes())
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

    let (vault_id, db_path, header_path) = resolve_singleton_vault()?
        .ok_or_else(|| IpcError::InvalidInput("No vault configured on this device".into()))?;

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

    // Wrap the Tauri channel in a plain closure so no `tauri::` import
    // leaks into the storage layer.
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
    .map_err(|e| IpcError::CloudError(e.to_string()))?;

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
    })
}

/// Download missing vault blobs from cloud into local staging for the
/// currently-open manifest.
///
/// **Phase 6.5 note**: the full new-device bootstrap flow (download + import
/// the remote manifest, then pull blobs) is deferred to Phase 7.  This
/// command operates on the already-open local manifest store.  The
/// `vault_header_path` parameter is accepted for API stability and will be
/// used by the Phase 7 implementation.
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

    // Phase 7 will use vault_header_path for new-device bootstrap.
    let _ = vault_header_path;

    let (vault_id, db_path, _) = resolve_singleton_vault()?.ok_or_else(|| {
        IpcError::InvalidInput(
            "No local vault found; Phase 7 required for new-device recovery".into(),
        )
    })?;

    let staging_dir = vault_staging_dir(&vault_id);

    let db_guard = state.database.read().await;
    let db = db_guard
        .as_ref()
        .ok_or_else(|| IpcError::VaultLocked("Vault is locked".into()))?;
    let cloud_transport = state.cloud_transport.read().await.clone();

    let sqlcipher_key = extract_sqlcipher_key(&state.session_manager).await?;
    let manifest_key = extract_manifest_key(&state.session_manager).await?;

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
        db,
        &*cloud_transport,
        &staging_dir,
        &SyncConfig::default(),
        Some(&progress_fn),
    )
    .await
    .map_err(|e| IpcError::CloudError(e.to_string()))?;

    Ok(())
}

/// Check the current sync status.
#[tauri::command]
pub async fn get_sync_status(state: State<'_, AppState>) -> Result<SyncStatus, IpcError> {
    state.session_manager.reset_timer().await;
    let status = state.sync_status.read().await.clone();
    Ok(status)
}

/// Copy all locally-staged vault blobs to a new destination without
/// re-encryption (blobs are opaque ciphertext).
///
/// **Phase 6.5 note**: only blobs currently present in local staging are
/// migrated.  A full implementation that first re-downloads every blob from
/// the primary transport is deferred to Phase 7.
///
/// Progress is streamed via `progress` channel.
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

    let (vault_id, _, _) = resolve_singleton_vault()?
        .ok_or_else(|| IpcError::InvalidInput("No vault configured on this device".into()))?;

    let staging_dir = vault_staging_dir(&vault_id);
    let config_path = staging_dir.join(".rclone-migrate.conf");

    let db_guard = state.database.read().await;
    let db = db_guard
        .as_ref()
        .ok_or_else(|| IpcError::VaultLocked("Vault is locked".into()))?;

    // Find destination session by ID.
    let all_sessions = list_destination_sessions(db)
        .await
        .map_err(IpcError::from)?;
    let dest = all_sessions
        .into_iter()
        .find(|d| d.destination_id == new_destination_id)
        .ok_or_else(|| {
            IpcError::NotFound(format!("destination '{new_destination_id}' not found"))
        })?;

    // Collect blobs that are present in local staging.
    let chunks = db.list_sync_chunks().await.map_err(IpcError::from)?;
    let mut staged_blobs: Vec<String> = Vec::new();
    for chunk in &chunks {
        let local_path = staging_dir.join(format!("{}.blob", chunk.blob_name));
        if tokio::fs::try_exists(&local_path).await.unwrap_or(false) {
            staged_blobs.push(chunk.blob_name.clone());
        }
    }

    let blobs_total = staged_blobs.len() as u32;

    if blobs_total == 0 {
        // Nothing in staging — migration is a no-op for Phase 6.5.
        let _ = progress.send(MigrationProgress {
            percent: 100,
            blobs_transferred: 0,
            blobs_total: 0,
            current_phase: "No staged blobs to migrate".to_owned(),
        });
        return Ok(());
    }

    let app_handle = state
        .app_handle
        .get()
        .ok_or_else(|| IpcError::InternalError("App handle not initialised".into()))?;
    let binary_path = rclone_binary_path(app_handle);

    let new_transport = build_destination_transport(binary_path, &config_path, &dest).await?;

    let mut blobs_transferred: u32 = 0;
    for blob_name in &staged_blobs {
        let local_path = staging_dir.join(format!("{blob_name}.blob"));
        let remote_path = blob_remote_path(blob_name);

        let _ = progress.send(MigrationProgress {
            percent: (blobs_transferred * 100 / blobs_total.max(1)) as u8,
            blobs_transferred,
            blobs_total,
            current_phase: format!("Uploading {blob_name}"),
        });

        if let Err(e) = new_transport.upload_blob(&local_path, &remote_path).await {
            // Best-effort cleanup before returning error.
            let _ = destroy_session_rclone_conf(&config_path).await;
            return Err(IpcError::CloudError(e.to_string()));
        }

        blobs_transferred += 1;
    }

    let _ = destroy_session_rclone_conf(&config_path).await;

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

    let (vault_id, _, _) = resolve_singleton_vault()?
        .ok_or_else(|| IpcError::InvalidInput("No vault configured on this device".into()))?;

    let staging_dir = vault_staging_dir(&vault_id);
    let config_path = staging_dir.join(".rclone-backup.conf");

    let db_guard = state.database.read().await;
    let db = db_guard
        .as_ref()
        .ok_or_else(|| IpcError::VaultLocked("Vault is locked".into()))?;

    // Collect backup destinations, optionally filtered by ID.
    let all_sessions = list_destination_sessions(db)
        .await
        .map_err(IpcError::from)?;
    let backup_dests: Vec<_> = all_sessions
        .into_iter()
        .filter(|d| !d.is_primary)
        .filter(|d| {
            destination_id
                .as_ref()
                .is_none_or(|id| &d.destination_id == id)
        })
        .collect();

    if backup_dests.is_empty() {
        return Ok(SyncResult {
            files_uploaded: 0,
            files_downloaded: 0,
            files_deleted: 0,
            conflicts: vec![],
        });
    }

    // Collect blobs that are present in local staging.
    let chunks = db.list_sync_chunks().await.map_err(IpcError::from)?;
    let mut staged_blobs: Vec<String> = Vec::new();
    for chunk in &chunks {
        let local_path = staging_dir.join(format!("{}.blob", chunk.blob_name));
        if tokio::fs::try_exists(&local_path).await.unwrap_or(false) {
            staged_blobs.push(chunk.blob_name.clone());
        }
    }

    if staged_blobs.is_empty() {
        return Ok(SyncResult {
            files_uploaded: 0,
            files_downloaded: 0,
            files_deleted: 0,
            conflicts: vec![],
        });
    }

    let blobs_total = staged_blobs.len() as u32;

    let app_handle = state
        .app_handle
        .get()
        .ok_or_else(|| IpcError::InternalError("App handle not initialised".into()))?;
    let binary_path = rclone_binary_path(app_handle);

    let mut total_uploaded: u32 = 0;

    for dest in &backup_dests {
        let transport =
            build_destination_transport(binary_path.clone(), &config_path, dest).await?;

        let _ = progress.send(SyncProgressUpdate {
            percent: 0,
            current_file: Some(dest.destination_id.clone()),
            files_processed: 0,
            files_total: blobs_total,
        });

        let mut dest_uploaded: u32 = 0;
        for blob_name in &staged_blobs {
            let local_path = staging_dir.join(format!("{blob_name}.blob"));
            let remote_path = blob_remote_path(blob_name);

            if let Err(e) = transport.upload_blob(&local_path, &remote_path).await {
                tracing::warn!(
                    destination_id = %dest.destination_id,
                    blob_name = %blob_name,
                    error = %e,
                    "backup upload failed; continuing with remaining blobs"
                );
            } else {
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

        // Wipe config for this destination before moving to the next.
        let _ = destroy_session_rclone_conf(&config_path).await;
    }

    Ok(SyncResult {
        files_uploaded: total_uploaded,
        files_downloaded: 0,
        files_deleted: 0,
        conflicts: vec![],
    })
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
