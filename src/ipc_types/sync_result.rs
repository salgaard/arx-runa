use serde::Deserialize;

use super::SyncConflict;

/// Mirror of `src-tauri/src/ui/types/sync_result.rs::SyncResult`.
/// Kept in sync manually until shared type generation is introduced.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncResult {
    /// Number of files uploaded to cloud during this sync.
    pub files_uploaded: u32,
    /// Number of files downloaded from cloud during this sync.
    pub files_downloaded: u32,
    /// Number of files deleted from cloud during this sync.
    pub files_deleted: u32,
    /// Conflicts detected that require user attention.
    pub conflicts: Vec<SyncConflict>,
}
