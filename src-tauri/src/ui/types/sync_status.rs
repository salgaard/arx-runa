//! Sync status response type.

use serde::Serialize;

/// Current sync status returned by `get_sync_status`.
#[derive(Debug, Default, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncStatus {
    /// Whether a sync operation is currently in progress.
    pub syncing: bool,
    /// Timestamp of the last successful sync (ISO 8601), `None` if never synced.
    pub last_synced_at: Option<String>,
    /// Number of local changes not yet pushed to cloud.
    pub pending_changes: u32,
}
