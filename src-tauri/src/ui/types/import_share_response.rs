//! Import share response type.

use serde::Serialize;

/// Response returned from a successful `import_share` invocation.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportShareResponse {
    /// Share identifier extracted from the package.
    pub share_id: String,
    /// Name of the file in the share package.
    pub file_name: String,
    /// Display name of the sender if they are a known contact, `None` otherwise.
    pub sender_name: Option<String>,
}
