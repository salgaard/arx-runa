//! Sync operation progress update type.

use serde::Serialize;

/// Progress update emitted during `sync_to_cloud` and related operations.
#[derive(Debug, Clone, Serialize)]
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
