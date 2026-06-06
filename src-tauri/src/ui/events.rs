//! Tauri event emission helpers.
//!
//! Backend-initiated UI updates are pushed to the frontend via Tauri events
//! instead of the frontend polling `get_session_status` / `get_sync_status` on a
//! timer. The `device-event` task in `lib.rs` is the reference pattern; like it,
//! emit failures are logged at `warn!` and never propagate.

use serde::Serialize;
use tauri::{AppHandle, Emitter as _};

use crate::storage::MetadataStore as _;
use crate::ui::AppState;
use crate::ui::types::SyncStatus;

/// Event name for session lifecycle changes pushed to the frontend.
pub(crate) const SESSION_CHANGED_EVENT: &str = "session-changed";
/// Event name for an imminent idle-timeout lock warning.
pub(crate) const SESSION_TIMEOUT_WARNING_EVENT: &str = "session-timeout-warning";
/// Event name for sync-status updates (pending changes, last sync).
pub(crate) const SYNC_STATUS_CHANGED_EVENT: &str = "sync-status-changed";

/// Payload for the `session-changed` event.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SessionChangedPayload {
    /// Whether the vault session is currently unlocked.
    pub is_unlocked: bool,
}

/// Payload for the `session-timeout-warning` event.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SessionTimeoutWarningPayload {
    /// Seconds remaining before the session locks automatically.
    pub seconds_remaining: u64,
}

/// Emits a `session-changed` event signalling that the session has locked.
pub(crate) fn emit_session_locked(app: &AppHandle) {
    let payload = SessionChangedPayload { is_unlocked: false };
    if let Err(error) = app.emit(SESSION_CHANGED_EVENT, payload) {
        tracing::warn!("session-changed emit failed: {error}");
    }
}

/// Emits a `session-timeout-warning` event ahead of an automatic lock.
pub(crate) fn emit_timeout_warning(app: &AppHandle, seconds_remaining: u64) {
    let payload = SessionTimeoutWarningPayload { seconds_remaining };
    if let Err(error) = app.emit(SESSION_TIMEOUT_WARNING_EVENT, payload) {
        tracing::warn!("session-timeout-warning emit failed: {error}");
    }
}

/// Builds the current sync status: the cached status plus the live epoch-buffer
/// count. Shared by the `get_sync_status` command and the event emitter so both
/// report identical state.
pub(crate) async fn current_sync_status(state: &AppState) -> SyncStatus {
    let mut status = state.sync_status.read().await.clone();
    if let Some(db_store) = state.session_manager.get_metadata_store().await {
        status.pending_changes = db_store.get_epoch_buffer_count().await.unwrap_or(0);
    }
    status
}

/// Emits a `sync-status-changed` event with the current sync status.
///
/// Best-effort: silently returns if the app handle is not yet initialised (e.g.
/// during early startup), and logs emit failures at `warn!`.
pub(crate) async fn emit_sync_status(state: &AppState) {
    let Some(app) = state.app_handle.get() else {
        return;
    };
    let status = current_sync_status(state).await;
    if let Err(error) = app.emit(SYNC_STATUS_CHANGED_EVENT, status) {
        tracing::warn!("sync-status-changed emit failed: {error}");
    }
}
