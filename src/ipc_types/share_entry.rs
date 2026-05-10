//! Share entry response type.

use serde::Deserialize;

/// An outgoing share entry returned by `list_shares`.
#[derive(Debug, Clone, Deserialize)]
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
    /// Whether a delivery receipt was requested.
    pub receipt_requested: bool,
    /// ISO 8601 timestamp when the download receipt was received, or `None` if not yet received.
    pub receipt_received_at: Option<String>,
    /// ISO 8601 timestamp when the import receipt was received, or `None` if not yet received.
    pub import_receipt_received_at: Option<String>,
}
