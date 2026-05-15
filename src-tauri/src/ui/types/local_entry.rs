//! Local filesystem entry type returned by `list_local_directory`.

use serde::Serialize;

/// A single entry (file or directory) returned by `list_local_directory`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalEntry {
    /// Display name of the entry (filename only, no path component).
    pub name: String,
    /// Absolute filesystem path of the entry.
    pub path: String,
    /// `true` if the entry is a directory; `false` if it is a regular file.
    pub is_dir: bool,
}
