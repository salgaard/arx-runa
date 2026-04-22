use serde::Deserialize;

/// Mirror of `src-tauri/src/ui/types/sync_conflict.rs::SyncConflict`.
/// Kept in sync manually until shared type generation is introduced.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncConflict {
    /// Name of the file with a conflict.
    pub file_name: String,
    /// Local modification timestamp (ISO 8601).
    pub local_modified: String,
    /// Cloud modification timestamp (ISO 8601).
    pub cloud_modified: String,
}
