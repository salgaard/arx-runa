use serde::Deserialize;

/// Mirror of `src-tauri/src/ui/types/destination_entry.rs::DestinationEntry`.
/// Kept in sync manually until shared type generation is introduced.
///
/// Never includes credential material.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DestinationEntry {
    /// Unique destination identifier.
    pub destination_id: String,
    /// Human-readable label.
    pub label: String,
    /// Destination type: `"cloud"`, `"external_drive"`, or `"local_path"`.
    pub destination_type: String,
    /// Cloud provider identifier.
    pub provider: String,
    /// rclone backend type (e.g. `"drive"`, `"b2"`, `"onedrive"`). `None` for local destinations.
    pub rclone_type: Option<String>,
    /// Cloud bucket name.
    pub bucket: String,
    /// Whether this is the primary destination.
    pub is_primary: bool,
    /// Backup sync mode, `None` for primary destinations.
    pub backup_mode: Option<String>,
}
