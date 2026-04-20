//! File management commands.
//!
//! Phase 6.1 scaffolds: input validation is wired; backend delegation deferred to Phase 6.5.

use std::path::PathBuf;

use tauri::State;
use tauri::ipc::Channel;

use crate::ui::error::IpcError;
use crate::ui::state::AppState;
use crate::ui::types::{FileContent, FileEntry, ProgressUpdate, RemoteFileEntry};
use crate::ui::validation::{normalise_vault_path, validate_file_id, validate_vault_path};

/// List the contents of a directory in the vault.
///
/// An empty path or `"/"` lists the root directory.
#[tauri::command]
pub async fn list_directory(
    path: String,
    state: State<'_, AppState>,
) -> Result<Vec<FileEntry>, IpcError> {
    state.session_manager.reset_timer().await;
    let normalised = normalise_vault_path(&path);
    validate_vault_path(normalised)?;
    // TODO(phase-6.5): wire storage::vault_ops::list_directory
    Err(IpcError::InternalError("command not yet wired".into()))
}

/// Encrypt and upload a file to the vault.
///
/// Progress is streamed via `progress` channel (no emissions in Phase 6.1).
#[tauri::command]
pub async fn upload_file(
    source_path: PathBuf,
    vault_path: String,
    progress: Channel<ProgressUpdate>,
    state: State<'_, AppState>,
) -> Result<FileEntry, IpcError> {
    state.session_manager.reset_timer().await;
    let vault_path = normalise_vault_path(&vault_path);
    if vault_path.is_empty() {
        return Err(IpcError::InvalidInput(
            "Vault path is required for upload".into(),
        ));
    }
    validate_vault_path(vault_path)?;
    let _ = (source_path, progress);
    // TODO(phase-6.5): wire storage::vault_ops::upload_file + progress channel
    Err(IpcError::InternalError("command not yet wired".into()))
}

/// Download and decrypt a file from the vault.
///
/// Progress is streamed via `progress` channel (no emissions in Phase 6.1).
#[tauri::command]
pub async fn download_file(
    file_id: String,
    destination_path: PathBuf,
    progress: Channel<ProgressUpdate>,
    state: State<'_, AppState>,
) -> Result<(), IpcError> {
    state.session_manager.reset_timer().await;
    validate_file_id(&file_id)?;
    let _ = (destination_path, progress);
    // TODO(phase-6.5): wire storage::vault_ops::download_file + progress channel
    Err(IpcError::InternalError("command not yet wired".into()))
}

/// Delete a file from the vault and cloud.
#[tauri::command]
pub async fn delete_file(file_id: String, state: State<'_, AppState>) -> Result<(), IpcError> {
    state.session_manager.reset_timer().await;
    validate_file_id(&file_id)?;
    // TODO(phase-6.5): wire storage::vault_ops::delete_file
    Err(IpcError::InternalError("command not yet wired".into()))
}

/// Decrypt and return file content for in-app viewing (Zero-Trace).
///
/// Returns `InvalidInput` for files above 50 MiB.
#[tauri::command]
pub async fn get_file_content(
    file_id: String,
    state: State<'_, AppState>,
) -> Result<FileContent, IpcError> {
    state.session_manager.reset_timer().await;
    validate_file_id(&file_id)?;
    // TODO(phase-6.5): wire storage::vault_ops::download + base64 encode; enforce 50 MiB limit
    Err(IpcError::InternalError("command not yet wired".into()))
}

/// List files on the primary remote destination.
///
/// Returns a manifest-linked view; blobs absent from the manifest are marked orphaned.
#[tauri::command]
pub async fn list_remote(
    remote_prefix: String,
    state: State<'_, AppState>,
) -> Result<Vec<RemoteFileEntry>, IpcError> {
    state.session_manager.reset_timer().await;
    let _ = remote_prefix;
    // TODO(phase-6.5): wire CloudTransport::list_blobs + manifest lookup
    Err(IpcError::InternalError("command not yet wired".into()))
}
