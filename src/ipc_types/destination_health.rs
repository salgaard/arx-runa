use serde::Deserialize;

/// Mirror of `src-tauri/src/ui/types/destination_health.rs::DestinationHealth`.
/// Kept in sync manually until shared type generation is introduced.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DestinationHealth {
    /// UUID of the backup destination.
    pub destination_id: String,
    /// Number of blobs that failed to upload and are pending retry.
    pub pending_failure_blobs: u32,
    /// Number of blobs queued for upload but not yet attempted.
    pub pending_blobs: u32,
}
