//! File operation progress update type.

use serde::Serialize;

/// Progress update for file upload and download operations.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgressUpdate {
    /// Completion percentage from 0 to 100.
    pub percent: u8,
    /// Number of bytes processed so far.
    pub bytes_processed: u64,
    /// Total number of bytes to process.
    pub bytes_total: u64,
    /// Human-readable description of the current operation.
    pub status: String,
}
