//! Share entry response type.

use serde::Serialize;

/// An outgoing share entry returned by `list_shares`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShareEntry {
    /// Unique share identifier.
    pub share_id: String,
    /// Name of the shared file.
    pub file_name: String,
    /// Display name of the contact the file was shared with.
    pub contact_name: String,
    /// ISO 8601 timestamp when the share was created.
    pub created_at: String,
    /// Whether the share has been revoked.
    pub revoked: bool,
}
