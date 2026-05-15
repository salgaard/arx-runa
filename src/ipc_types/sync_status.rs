use serde::Deserialize;

/// Mirror of `src-tauri/src/ui/types/sync_status.rs::SyncStatus`.
/// Kept in sync manually until shared type generation is introduced.
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SyncStatus {
    /// Whether a sync operation is currently in progress.
    pub syncing: bool,
    /// Timestamp of the last successful sync (ISO 8601), `None` if never synced.
    pub last_synced_at: Option<String>,
    /// Number of local changes not yet pushed to cloud.
    pub pending_changes: u32,
}
