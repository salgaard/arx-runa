//! File sharing commands.
//!
//! Phase 6.1 scaffolds: input validation is wired; backend delegation deferred to Phase 6.5.

use std::path::PathBuf;

use tauri::State;

use crate::ui::error::IpcError;
use crate::ui::state::AppState;
use crate::ui::types::{
    ContactEntry, ImportShareResponse, ReceivedShareEntry, ShareEntry, ShareResponse,
};
use crate::ui::validation::validate_file_id;

/// Export the user's X25519 public key to a file for out-of-band exchange.
#[tauri::command]
pub async fn export_public_key(
    destination_path: PathBuf,
    state: State<'_, AppState>,
) -> Result<(), IpcError> {
    state.session_manager.reset_timer().await;
    let _ = destination_path;
    // TODO(phase-6.5): wire sharing::identity::export_public_key_bytes + write to file
    Err(IpcError::InternalError("command not yet wired".into()))
}

/// Import a contact's public key from a file.
#[tauri::command]
pub async fn add_contact(
    display_name: String,
    public_key_path: PathBuf,
    email: Option<String>,
    state: State<'_, AppState>,
) -> Result<ContactEntry, IpcError> {
    state.session_manager.reset_timer().await;
    if display_name.is_empty() {
        return Err(IpcError::InvalidInput(
            "Display name must not be empty".into(),
        ));
    }
    let _ = (public_key_path, email);
    // TODO(phase-6.5): wire sharing::SharingStore::insert_contact
    Err(IpcError::InternalError("command not yet wired".into()))
}

/// List all contacts.
#[tauri::command]
pub async fn list_contacts(state: State<'_, AppState>) -> Result<Vec<ContactEntry>, IpcError> {
    state.session_manager.reset_timer().await;
    // TODO(phase-6.5): wire sharing::SharingStore::list_contacts
    Err(IpcError::InternalError("command not yet wired".into()))
}

/// Share a file with a contact via HPKE (RFC 9180).
#[tauri::command]
pub async fn share_file(
    file_id: String,
    contact_id: String,
    expiration_days: Option<u32>,
    state: State<'_, AppState>,
) -> Result<ShareResponse, IpcError> {
    state.session_manager.reset_timer().await;
    validate_file_id(&file_id)?;
    if contact_id.is_empty() {
        return Err(IpcError::InvalidInput(
            "Contact ID must not be empty".into(),
        ));
    }
    let _ = expiration_days;
    // TODO(phase-6.5): wire sharing::create_share_package
    Err(IpcError::InternalError("command not yet wired".into()))
}

/// Import a received share package.
#[tauri::command]
pub async fn import_share(
    share_package_path: PathBuf,
    state: State<'_, AppState>,
) -> Result<ImportShareResponse, IpcError> {
    state.session_manager.reset_timer().await;
    let _ = share_package_path;
    // TODO(phase-6.5): wire sharing::import_share_package
    Err(IpcError::InternalError("command not yet wired".into()))
}

/// Revoke a previously shared file.
#[tauri::command]
pub async fn revoke_share(share_id: String, state: State<'_, AppState>) -> Result<(), IpcError> {
    state.session_manager.reset_timer().await;
    if share_id.is_empty() {
        return Err(IpcError::InvalidInput("Share ID must not be empty".into()));
    }
    // TODO(phase-6.5): wire sharing::revocation::revoke_share
    Err(IpcError::InternalError("command not yet wired".into()))
}

/// List outgoing shares.
#[tauri::command]
pub async fn list_shares(state: State<'_, AppState>) -> Result<Vec<ShareEntry>, IpcError> {
    state.session_manager.reset_timer().await;
    // TODO(phase-6.5): wire sharing::SharingStore list outgoing shares
    Err(IpcError::InternalError("command not yet wired".into()))
}

/// List received shares.
#[tauri::command]
pub async fn list_received_shares(
    state: State<'_, AppState>,
) -> Result<Vec<ReceivedShareEntry>, IpcError> {
    state.session_manager.reset_timer().await;
    // TODO(phase-6.5): wire sharing::SharingStore::list_received_shares
    Err(IpcError::InternalError("command not yet wired".into()))
}
