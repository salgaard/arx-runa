//! Cloud sync commands.
//!
//! Phase 6.1 scaffolds: signatures are stable; backend delegation deferred to Phase 6.5.

use std::path::PathBuf;

use tauri::State;
use tauri::ipc::Channel;

use crate::ui::error::IpcError;
use crate::ui::state::AppState;
use crate::ui::types::{MigrationProgress, SyncProgressUpdate, SyncResult, SyncStatus};

/// Push local changes to cloud.
///
/// Progress is streamed via `progress` channel (no emissions in Phase 6.1).
#[tauri::command]
pub async fn sync_to_cloud(
    progress: Channel<SyncProgressUpdate>,
    state: State<'_, AppState>,
) -> Result<SyncResult, IpcError> {
    state.session_manager.reset_timer().await;
    let _ = progress;
    // TODO(phase-6.5): wire storage::cloud::push_vault
    Err(IpcError::InternalError("command not yet wired".into()))
}

/// Recover vault from cloud on a new device.
///
/// Progress is streamed via `progress` channel (no emissions in Phase 6.1).
#[tauri::command]
pub async fn recover_from_cloud(
    vault_header_path: PathBuf,
    progress: Channel<SyncProgressUpdate>,
    state: State<'_, AppState>,
) -> Result<(), IpcError> {
    state.session_manager.reset_timer().await;
    let _ = (vault_header_path, progress);
    // TODO(phase-6.5): wire storage::cloud::pull_vault
    Err(IpcError::InternalError("command not yet wired".into()))
}

/// Check the current sync status.
#[tauri::command]
pub async fn get_sync_status(state: State<'_, AppState>) -> Result<SyncStatus, IpcError> {
    state.session_manager.reset_timer().await;
    let status = state.sync_status.read().await.clone();
    Ok(status)
}

/// Migrate vault blobs to a new destination.
///
/// No re-encryption is required — blobs are opaque ciphertext.
/// Progress is streamed via `progress` channel (no emissions in Phase 6.1).
#[tauri::command]
pub async fn migrate_vault(
    new_destination_id: String,
    progress: Channel<MigrationProgress>,
    state: State<'_, AppState>,
) -> Result<(), IpcError> {
    state.session_manager.reset_timer().await;
    if new_destination_id.is_empty() {
        return Err(IpcError::InvalidInput(
            "Destination ID must not be empty".into(),
        ));
    }
    let _ = progress;
    // TODO(phase-6.5): wire vault blob migration
    Err(IpcError::InternalError("command not yet wired".into()))
}

/// Sync the primary destination to one or more backup destinations.
///
/// If `destination_id` is `None`, syncs to all configured backup destinations.
/// Progress is streamed via `progress` channel (no emissions in Phase 6.1).
#[tauri::command]
pub async fn sync_backup(
    destination_id: Option<String>,
    progress: Channel<SyncProgressUpdate>,
    state: State<'_, AppState>,
) -> Result<SyncResult, IpcError> {
    state.session_manager.reset_timer().await;
    let _ = (destination_id, progress);
    // TODO(phase-6.5): wire backup sync
    Err(IpcError::InternalError("command not yet wired".into()))
}
