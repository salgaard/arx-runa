//! Share response type.

use serde::Serialize;

/// Response returned from a successful `share_file` invocation.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShareResponse {
    /// Unique share identifier.
    pub share_id: String,
    /// Filesystem path to the exported share package file.
    pub package_path: String,
}
