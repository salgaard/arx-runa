//! Destination entry response type.

use serde::Serialize;

/// Destination metadata returned by `list_destinations` or `add_destination`.
///
/// Never includes credential material.
#[derive(Debug, Serialize)]
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
    /// rclone backend type extracted from the config blob (e.g. `"drive"`, `"b2"`, `"onedrive"`).
    /// `None` for local / external-drive destinations that have no rclone config.
    pub rclone_type: Option<String>,
    /// Cloud bucket name.
    pub bucket: String,
    /// Whether this is the primary destination.
    pub is_primary: bool,
    /// Backup sync mode, `None` for primary destinations.
    pub backup_mode: Option<String>,
    /// Whether this destination supports file sharing.
    ///
    /// Only Backblaze B2 (`rclone_type = "b2"`) and Google Drive
    /// (`rclone_type = "drive"`) support sharing.
    pub sharing_supported: bool,
}
