//! Shared helpers used by all Tauri command handlers.
//!
//! These thin wrappers enforce Zero-Trace invariants and reduce boilerplate
//! in command handler bodies.

use zeroize::Zeroizing;

use crate::auth::LifecycleState;
use crate::ui::error::IpcError;
use crate::ui::state::AppState;

/// Calls `reset_timer` on the session manager before invoking `f`.
///
/// Every IPC command must refresh the inactivity timer on entry.
#[allow(dead_code)] // Phase 7: with_session_refresh for long-running command restart
pub(crate) async fn with_session_refresh<F, Fut, T>(
    state: &AppState,
    f: F,
) -> Result<T, IpcError>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<T, IpcError>>,
{
    state.session_manager.reset_timer().await;
    f().await
}

/// Returns `Ok(())` if the session is `Active`, or `IpcError::VaultLocked` otherwise.
pub(crate) async fn require_active_session(state: &AppState) -> Result<(), IpcError> {
    if state.session_manager.state().await == LifecycleState::Active {
        Ok(())
    } else {
        Err(IpcError::VaultLocked("Vault is locked".into()))
    }
}

/// Converts a password `String` to a `Zeroizing<Vec<u8>>`, scrubbing the original.
///
/// The returned bytes own the password content. The input `String` backing
/// bytes are overwritten with zeros before the `String` is dropped.
pub(crate) fn sanitise_password(password: &mut String) -> Zeroizing<Vec<u8>> {
    let bytes = Zeroizing::new(password.as_bytes().to_vec());
    // SAFETY: overwriting ASCII-range bytes (or UTF-8 bytes we no longer need)
    // with zeros. The String is dropped immediately after this function returns.
    unsafe {
        let ptr = password.as_bytes_mut().as_mut_ptr();
        std::ptr::write_bytes(ptr, 0, password.len());
    }
    bytes
}
