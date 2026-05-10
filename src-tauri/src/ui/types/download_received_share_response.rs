use serde::Serialize;

/// Response returned by the `download_received_share` IPC command.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadReceivedShareResponse {
    /// Original file name from the share package.
    pub file_name: String,
}
