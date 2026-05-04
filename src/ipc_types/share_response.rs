//! Share response type.

use serde::Deserialize;

/// Response returned from a successful `share_file` invocation.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShareResponse {
    /// Unique share identifier.
    pub share_id: String,
    /// Filesystem path to the exported share package file.
    pub package_path: String,
    /// Email address of the recipient contact, if one was recorded.
    pub contact_email: Option<String>,
}
