//! File content response type.

use serde::Serialize;

/// Decrypted file content returned by `get_file_content` for in-app viewing.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileContent {
    /// MIME type of the file (e.g., `"image/jpeg"`, `"text/plain"`).
    pub mime_type: String,
    /// Base64-encoded file content.
    pub data_base64: String,
    /// Original file size in bytes.
    pub size_bytes: u64,
}
