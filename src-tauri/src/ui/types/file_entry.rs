//! File entry response type.

use serde::Serialize;

/// A file or directory entry in the vault returned by `list_directory`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileEntry {
    /// Unique file identifier (UUID v4).
    pub id: String,
    /// File or directory name (decrypted).
    pub name: String,
    /// Entry type: `"file"` or `"directory"`.
    pub entry_type: String,
    /// Size in bytes (0 for directories).
    pub size_bytes: u64,
    /// Last modified timestamp (ISO 8601).
    pub modified_at: String,
    /// Parent directory ID, `None` for root entries.
    pub parent_id: Option<String>,
    /// Whether this file is staged in the epoch buffer and not yet encrypted.
    ///
    /// Downloads are not possible until the epoch buffer is flushed.
    /// Always `false` for directory entries.
    pub pending_flush: bool,
}
