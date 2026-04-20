//! Destination session configuration request type.

use serde::Deserialize;

/// Configuration for adding a destination session, deserialised from the frontend.
///
/// Credentials in `rclone_config_blob` are encrypted into SQLCipher on save and
/// never written to disk in plaintext.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DestinationSessionConfig {
    /// Human-readable label (e.g., `"My Backblaze B2"`).
    pub label: String,
    /// Destination type: `"cloud"`, `"external_drive"`, or `"local_path"`.
    pub destination_type: String,
    /// Non-sensitive cloud provider identifier (e.g., `"s3"`, `"b2"`).
    pub provider: String,
    /// Cloud bucket name.
    pub bucket: String,
    /// Cloud region identifier.
    pub region: String,
    /// Cloud service endpoint URL.
    pub endpoint: String,
    /// Path prefix within the bucket.
    pub path_prefix: String,
    /// Rclone config section for this remote, including credentials.
    /// Encrypted into SQLCipher on save.
    pub rclone_config_blob: String,
    /// Whether this is the primary destination for uploads.
    pub is_primary: bool,
    /// Backup sync mode: `"mirror"` or `"accumulating"`. `None` for primary destinations.
    pub backup_mode: Option<String>,
}
