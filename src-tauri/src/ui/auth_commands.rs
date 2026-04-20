//! Authentication and session management commands.
//!
//! These are Phase 6.1 scaffolds. Commands that require vault header access
//! (`authenticate`, `create_vault`, `change_password`, `rotate_key_file`, `delete_vault`)
//! return `InternalError("command not yet wired")` until Phase 6.5 wires the
//! full orchestration.

use std::path::PathBuf;

use tauri::State;
use zeroize::Zeroizing;

use crate::ui::error::IpcError;
use crate::ui::state::AppState;
use crate::ui::types::{AuthResponse, SessionStatus};
use crate::ui::validation::validate_password;

/// Authenticate with password (Tier 1) or password + USB key file (Tier 2).
///
/// Returns vault metadata on success. Does NOT return any key material.
#[tauri::command]
pub async fn authenticate(
    password: String,
    key_file_path: Option<PathBuf>,
    state: State<'_, AppState>,
) -> Result<AuthResponse, IpcError> {
    let password_bytes = Zeroizing::new(password.into_bytes());
    state.session_manager.reset_timer().await;
    validate_password(std::str::from_utf8(&password_bytes).unwrap_or(""))?;
    let _ = key_file_path;
    // TODO(phase-6.5): wire vault header download + SessionManager::authenticate
    // TODO(phase-6.4): backoff
    Err(IpcError::InternalError("command not yet wired".into()))
}

/// Create a new vault.
///
/// For Tier 2, generates a key file at `key_file_destination`. `chunk_size_bytes`
/// must be in `[131072, 67108864]` and is immutable after creation.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn create_vault(
    vault_name: String,
    password: String,
    tier: u8,
    key_file_destination: Option<PathBuf>,
    primary_destination: crate::ui::types::DestinationSessionConfig,
    chunk_size_bytes: u64,
    epoch_buffer_enabled: bool,
    state: State<'_, AppState>,
) -> Result<AuthResponse, IpcError> {
    let password_bytes = Zeroizing::new(password.into_bytes());
    state.session_manager.reset_timer().await;
    validate_password(std::str::from_utf8(&password_bytes).unwrap_or(""))?;
    crate::ui::validation::validate_chunk_size(chunk_size_bytes)?;
    if vault_name.is_empty() {
        return Err(IpcError::InvalidInput(
            "Vault name must not be empty".into(),
        ));
    }
    let _ = (
        tier,
        key_file_destination,
        primary_destination,
        epoch_buffer_enabled,
    );
    // TODO(phase-6.5): wire auth::create_vault ceremony
    Err(IpcError::InternalError("command not yet wired".into()))
}

/// Change the vault password.
///
/// Requires an active session. For Tier 2, the USB key file must be present.
#[tauri::command]
pub async fn change_password(
    current_password: String,
    new_password: String,
    state: State<'_, AppState>,
) -> Result<(), IpcError> {
    let current_password_bytes = Zeroizing::new(current_password.into_bytes());
    let new_password_bytes = Zeroizing::new(new_password.into_bytes());
    state.session_manager.reset_timer().await;
    validate_password(std::str::from_utf8(&current_password_bytes).unwrap_or(""))?;
    validate_password(std::str::from_utf8(&new_password_bytes).unwrap_or(""))?;
    // TODO(phase-6.5): wire auth::change_password ceremony
    Err(IpcError::InternalError("command not yet wired".into()))
}

/// Rotate the USB key file (Tier 2 only).
///
/// Generates a new 32-byte key file at `new_key_file_destination`.
#[tauri::command]
pub async fn rotate_key_file(
    new_key_file_destination: PathBuf,
    state: State<'_, AppState>,
) -> Result<(), IpcError> {
    state.session_manager.reset_timer().await;
    let _ = new_key_file_destination;
    // TODO(phase-6.5): wire auth::rotate_key_file ceremony
    Err(IpcError::InternalError("command not yet wired".into()))
}

/// Delete the vault permanently.
///
/// `confirmation` must be non-empty; full vault-name matching is wired in Phase 6.5.
#[tauri::command]
pub async fn delete_vault(
    confirmation: String,
    state: State<'_, AppState>,
) -> Result<(), IpcError> {
    state.session_manager.reset_timer().await;
    if confirmation.is_empty() {
        return Err(IpcError::InvalidInput(
            "Confirmation string must not be empty".into(),
        ));
    }
    // TODO(phase-6.5): wire vault header lookup + deletion
    Err(IpcError::InternalError("command not yet wired".into()))
}

/// Zero all session keys and lock the vault.
#[tauri::command]
pub async fn lock_session(state: State<'_, AppState>) -> Result<(), IpcError> {
    state.session_manager.reset_timer().await;
    state.session_manager.lock().await;
    Ok(())
}

/// Check if the vault is unlocked.
///
/// Returns status only — no key material is included.
#[tauri::command]
pub async fn get_session_status(state: State<'_, AppState>) -> Result<SessionStatus, IpcError> {
    state.session_manager.reset_timer().await;
    let lifecycle = state.session_manager.state().await;
    let is_unlocked = lifecycle == crate::auth::LifecycleState::Active;
    // TODO(phase-6.5): populate vault_id from active session record and timeout_seconds from SessionManager::remaining_seconds()
    Ok(SessionStatus {
        is_unlocked,
        vault_id: None,
        timeout_seconds: None,
    })
}
