use serde::Serialize;

/// Frontend mirror of `src-tauri/src/ui/types/destination_session_config.rs`.
///
/// Used in [`CreateVaultRequest`] to specify the primary cloud destination.
/// This type is **send-only** — the frontend never deserialises it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DestinationSessionConfig {
    /// Human-readable label for this destination.
    pub label: String,
    /// Destination type identifier (e.g. `"s3"`, `"gcs"`).
    pub destination_type: String,
    /// Cloud provider name.
    pub provider: String,
    /// Storage bucket name.
    pub bucket: String,
    /// Cloud region identifier.
    pub region: String,
    /// Custom endpoint URL, or empty string for the default.
    pub endpoint: String,
    /// Path prefix within the bucket.
    pub path_prefix: String,
    /// Serialised rclone configuration blob.
    pub rclone_config_blob: String,
    /// Whether this is the primary destination.
    pub is_primary: bool,
    /// Backup mode string, or `None` for no backup.
    pub backup_mode: Option<String>,
}
