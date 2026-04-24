//! Received share entry response type.

use serde::Deserialize;

/// A received share entry returned by `list_received_shares`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReceivedShareEntry {
    /// Unique share identifier.
    pub share_id: String,
    /// Name of the received file.
    pub file_name: String,
    /// Display name of the sender if they are a known contact, `None` otherwise.
    pub sender_name: Option<String>,
    /// ISO 8601 timestamp when the share was imported.
    pub imported_at: String,
}
