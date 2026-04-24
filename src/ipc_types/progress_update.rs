use serde::Deserialize;

/// Streaming progress payload received from long-running Tauri commands via `IpcChannel`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgressUpdate {
    /// Completion percentage in the range `[0, 100]`.
    pub percent: u8,
    /// Number of bytes processed so far.
    pub bytes_processed: u64,
    /// Total bytes to process.
    pub bytes_total: u64,
    /// Human-readable status description.
    pub status: String,
}
