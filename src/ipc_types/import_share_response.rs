//! Import share response type.

use serde::Deserialize;

/// Response returned from a successful `import_share` invocation.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportShareResponse {
    /// Share identifier extracted from the package.
    pub share_id: String,
    /// Display name of the file that was imported.
    pub file_name: String,
    /// Display name of the sender if they are a known contact, `None` otherwise.
    pub sender_name: Option<String>,
}
