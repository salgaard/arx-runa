//! File content response type.

use serde::Deserialize;

/// Response returned from a successful `get_file_content` invocation.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileContentResponse {
    /// MIME type of the file (e.g., `"image/jpeg"`, `"text/plain"`).
    pub mime_type: String,
    /// Base64-encoded file content.
    pub data_base64: String,
    /// Original file size in bytes.
    pub size_bytes: u64,
}
