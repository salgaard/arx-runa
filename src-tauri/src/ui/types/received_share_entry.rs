//! Received share entry response type.

use serde::Serialize;

/// A received share entry returned by `list_received_shares`.
#[derive(Debug, Serialize)]
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
    /// File size in bytes (used by the frontend to gate preview availability).
    pub size_bytes: u64,
}
