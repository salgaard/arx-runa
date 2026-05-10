use serde::Deserialize;

/// Progress update emitted by the `sync_to_cloud` command while syncing files.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncProgressUpdate {
    /// Overall sync progress from 0 to 100.
    pub percent: u8,
    /// Name of the file currently being synced, `None` between files.
    pub current_file: Option<String>,
    /// Number of files processed so far.
    pub files_processed: u32,
    /// Total number of files to process.
    pub files_total: u32,
}
