//! Backup-destination health response type for `get_backup_health`.

use serde::Serialize;

/// Health summary for a single backup destination.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DestinationHealth {
    /// UUID of the backup destination.
    pub destination_id: String,
    /// Number of blobs that failed to upload and are pending retry.
    pub pending_failure_blobs: u32,
    /// Number of blobs queued for upload but not yet attempted.
    pub pending_blobs: u32,
}
