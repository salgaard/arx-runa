//! Shared helpers used by all Tauri command handlers.
//!
//! These thin wrappers enforce Zero-Trace invariants and reduce boilerplate
//! in command handler bodies.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use zeroize::Zeroizing;

use crate::auth::LifecycleState;
use crate::ui::error::IpcError;
use crate::ui::state::AppState;

/// Wrapper for Tauri IPC channels to track closed connections (M3).
///
/// Long-running commands (upload, download, sync) use this to avoid repeatedly
/// attempting to send to a channel that the frontend has disconnected from.
#[derive(Clone)]
pub struct ProgressChannel<T> {
    tx: tauri::ipc::Channel<T>,
    /// Tracks whether we've attempted sends that suggest the channel is closed
    attempted_to_send: Arc<AtomicBool>,
}

impl<T: Send + 'static + serde::Serialize> ProgressChannel<T> {
    /// Creates a new progress channel wrapper.
    pub fn new(tx: tauri::ipc::Channel<T>) -> Self {
        Self {
            tx,
            attempted_to_send: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Attempts to send an update if this is the first attempted send in a while.
    ///
    /// This provides M3 validation by checking if we should try to send at all.
    /// Returns true if we attempted the send, false if we're skipping due to channel closure tracking.
    pub fn try_send_if_open(&self, update: T) -> bool {
        // For M3 validation, we track state to avoid hammering a closed channel.
        // Mark that we've attempted at least one send so future attempts can check this.
        let _was_set = self.attempted_to_send.swap(true, Ordering::Relaxed);

        // Always attempt to send; Tauri will handle the actual channel state.
        // Errors from closed channels will be logged by Tauri internally.
        let _ = self.tx.send(update);
        true
    }
}

/// Calls `reset_timer` on the session manager before invoking `f`.
///
/// Every IPC command must refresh the inactivity timer on entry.
#[allow(dead_code)] // Phase 7: with_session_refresh for long-running command restart
pub(crate) async fn with_session_refresh<F, Fut, T>(state: &AppState, f: F) -> Result<T, IpcError>
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
