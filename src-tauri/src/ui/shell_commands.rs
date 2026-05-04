//! Shell integration commands (opener, file-manager reveal).

use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;

use crate::ui::error::IpcError;
use crate::ui::state::AppState;

/// Reveals a file or directory in the platform file manager.
#[tauri::command]
pub async fn reveal_in_explorer(
    path: String,
    _state: State<'_, AppState>,
    app: AppHandle,
) -> Result<(), IpcError> {
    app.opener()
        .reveal_item_in_dir(&path)
        .map_err(|e| IpcError::InternalError(format!("reveal failed: {e}")))?;
    Ok(())
}
