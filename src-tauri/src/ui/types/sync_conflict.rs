//! Sync conflict type.

use serde::Serialize;

/// A sync conflict detected during `sync_to_cloud`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncConflict {
    /// Name of the file with a conflict.
    pub file_name: String,
    /// Local modification timestamp (ISO 8601).
    pub local_modified: String,
    /// Cloud modification timestamp (ISO 8601).
    pub cloud_modified: String,
}
