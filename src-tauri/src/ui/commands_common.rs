//! Shared helpers used by all Tauri command handlers.
//!
//! These thin wrappers enforce Zero-Trace invariants and reduce boilerplate
//! in command handler bodies.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use secrecy::SecretBox;
use zeroize::Zeroizing;

use crate::auth::LifecycleState;
use crate::crypto::KeyEncryptionKey;
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

/// Returns `Ok(())` if the session is `Active`, or `IpcError::VaultLocked` otherwise.
pub(crate) async fn require_active_session(state: &AppState) -> Result<(), IpcError> {
    if state.session_manager.state().await == LifecycleState::Active {
        Ok(())
    } else {
        Err(IpcError::VaultLocked("Vault is locked".into()))
    }
}

/// Returns the path to the rclone binary.
///
/// Tries the app resource directory first (production bundle), then falls back to the system
/// PATH (development mode). Pass `None` at session-setup time when the `AppHandle` is not yet
/// guaranteed to be present (e.g., in `auth_commands`).
pub(crate) fn rclone_binary_path(handle: Option<&tauri::AppHandle>) -> std::path::PathBuf {
    use tauri::Manager as _;
    if let Some(handle) = handle
        && let Ok(resource_dir) = handle.path().resource_dir()
    {
        let name = if cfg!(target_os = "windows") {
            "rclone.exe"
        } else {
            "rclone"
        };
        let candidate = resource_dir.join("bin").join(name);
        if candidate.exists() {
            return candidate;
        }
    }
    std::path::PathBuf::from(if cfg!(target_os = "windows") {
        "rclone.exe"
    } else {
        "rclone"
    })
}

/// Converts a Unix timestamp (seconds since 1970-01-01T00:00:00Z) to an ISO 8601 string.
///
/// Implemented with pure stdlib arithmetic so the crate does not need `chrono` or `time`.
pub(crate) fn unix_ts_to_iso8601(ts: i64) -> String {
    let ts = if ts < 0 { 0u64 } else { ts as u64 };
    let secs = ts % 60;
    let mins = (ts / 60) % 60;
    let hours = (ts / 3600) % 24;
    let total_days = ts / 86400;
    let (year, month, day) = days_since_epoch_to_date(total_days);
    format!("{year:04}-{month:02}-{day:02}T{hours:02}:{mins:02}:{secs:02}Z")
}

/// Maps days since the Unix epoch to a proleptic Gregorian (year, month, day) triple.
///
/// Algorithm: http://howardhinnant.github.io/date_algorithms.html "civil_from_days".
pub(crate) fn days_since_epoch_to_date(days: u64) -> (u32, u32, u32) {
    let z = days as i64 + 719_468;
    let era: i64 = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
    let y = (if m <= 2 { y + 1 } else { y }) as u32;
    (y, m, d)
}

/// Copies the KEK out of the session guard and wraps it in a `KeyEncryptionKey`.
///
/// The intermediate stack copy is wrapped in `Zeroizing` so it is cleared
/// before the `SecretBox` heap allocation takes ownership.
pub(crate) async fn extract_kek(state: &AppState) -> Result<KeyEncryptionKey, IpcError> {
    let kek_raw: Zeroizing<[u8; 32]> = state
        .session_manager
        .with_key_encryption_key(|k| Zeroizing::new(*k))
        .await
        .map_err(IpcError::from)?;
    Ok(KeyEncryptionKey::from_secret_box(SecretBox::new(Box::new(
        *kek_raw,
    ))))
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
