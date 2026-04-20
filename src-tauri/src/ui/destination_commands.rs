//! Destination session management commands.
//!
//! Phase 6.1 scaffolds: input validation is wired; backend delegation deferred to Phase 6.5.

use tauri::State;

use crate::ui::error::IpcError;
use crate::ui::state::AppState;
use crate::ui::types::{DestinationEntry, DestinationSessionConfig};

/// Add a new destination session (primary or backup) to the vault.
///
/// Credentials are encrypted and stored in SQLCipher.
#[tauri::command]
pub async fn add_destination(
    config: DestinationSessionConfig,
    state: State<'_, AppState>,
) -> Result<DestinationEntry, IpcError> {
    state.session_manager.reset_timer().await;
    if config.label.is_empty() {
        return Err(IpcError::InvalidInput(
            "Destination label must not be empty".into(),
        ));
    }
    let _ = config;
    // TODO(phase-6.5): wire storage::cloud::destination_session::insert_destination_session
    Err(IpcError::InternalError("command not yet wired".into()))
}

/// List all configured destination sessions for the current vault.
///
/// Returns metadata only — no credential material.
#[tauri::command]
pub async fn list_destinations(
    state: State<'_, AppState>,
) -> Result<Vec<DestinationEntry>, IpcError> {
    state.session_manager.reset_timer().await;
    // TODO(phase-6.5): wire storage::cloud::destination_session::list_destination_sessions
    Err(IpcError::InternalError("command not yet wired".into()))
}

/// Delete a destination session from the vault.
#[tauri::command]
pub async fn delete_destination(
    destination_id: String,
    state: State<'_, AppState>,
) -> Result<(), IpcError> {
    state.session_manager.reset_timer().await;
    if destination_id.is_empty() {
        return Err(IpcError::InvalidInput(
            "Destination ID must not be empty".into(),
        ));
    }
    // TODO(phase-6.5): wire storage::cloud::destination_session::delete_destination_session
    Err(IpcError::InternalError("command not yet wired".into()))
}
