use serde::Deserialize;

/// Mirror of `src-tauri/src/ui/types/file_entry.rs::FileEntry`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileEntry {
    /// Opaque unique identifier assigned by the backend.
    pub id: String,
    /// Display name of the file or directory (filename only, no path).
    pub name: String,
    /// Entry kind discriminator; holds the camelCase values `"file"` or
    /// `"directory"`. Use [`FileEntry::is_directory`] for boolean checks
    /// rather than comparing this field directly.
    pub entry_type: String,
    /// Size of the entry in bytes. `0` for directories.
    pub size_bytes: u64,
    /// ISO-8601 timestamp of the last modification, as provided by the backend.
    pub modified_at: String,
    /// Identifier of the parent directory, or `None` if this entry is at the vault root.
    pub parent_id: Option<String>,
}

impl FileEntry {
    /// Whether this entry is a directory (versus a file).
    pub fn is_directory(&self) -> bool {
        self.entry_type == "directory"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Convenience builder – keeps test bodies readable.
    fn make_entry(entry_type: &str) -> FileEntry {
        FileEntry {
            id: "test-id".to_string(),
            name: "test-name".to_string(),
            entry_type: entry_type.to_string(),
            size_bytes: 0,
            modified_at: "2024-01-01T00:00:00Z".to_string(),
            parent_id: None,
        }
    }

    #[test]
    fn test_file_entry_is_directory_returns_true_for_directory_type() {
        let entry = make_entry("directory");
        assert!(entry.is_directory());
    }

    #[test]
    fn test_file_entry_is_directory_returns_false_for_file_type() {
        let entry = make_entry("file");
        assert!(!entry.is_directory());
    }

    /// Any value other than the exact string `"directory"` must return `false`
    /// to prevent forward-compat surprises from new backend entry kinds.
    #[test]
    fn test_file_entry_is_directory_returns_false_for_unknown_entry_type() {
        let entry = make_entry("symlink");
        assert!(!entry.is_directory());
    }
}
