//! Remote file entry response type.

use serde::Serialize;

/// A file entry returned by `list_remote`, linked to the local manifest.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteFileEntry {
    /// Blob identifier (UUID v4).
    pub blob_id: String,
    /// Resolved filename from the manifest, `None` if not matched.
    pub file_name: Option<String>,
    /// Resolved vault path from the manifest, `None` if not matched.
    pub vault_path: Option<String>,
    /// Blob size in bytes as reported by rclone.
    pub size_bytes: u64,
    /// Whether the blob has no matching manifest entry (orphaned).
    pub is_orphaned: bool,
}
