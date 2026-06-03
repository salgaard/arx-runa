use serde::Deserialize;

/// A drive offered by the OneDrive chooser, shown in the drive-picker dialog.
///
/// Mirror of `src-tauri/src/ui/types/oauth_setup_response.rs::DriveChoice`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DriveChoice {
    /// Opaque rclone drive identifier, echoed back via `select_oauth_drive`.
    pub id: String,
    /// Human-readable label (`"<DriveName> (<DriveType>)"`).
    pub label: String,
}

/// Returned by `begin_google_drive_setup` and `begin_onedrive_setup`.
///
/// Mirror of `src-tauri/src/ui/types/oauth_setup_response.rs::BeginOauthSetupResponse`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BeginOauthSetupResponse {
    /// Opaque identifier used to poll or cancel this setup.
    pub setup_id: String,
    /// Local rclone auth URL to open in the system browser.
    pub auth_url: String,
}

/// Returned by `poll_oauth_setup`.
///
/// Mirror of `src-tauri/src/ui/types/oauth_setup_response.rs::OauthPollResponse`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", tag = "status")]
pub enum OauthPollResponse {
    /// OAuth callback not yet received; poll again.
    #[serde(rename = "pending")]
    Pending,
    /// Multiple drives available; stop polling and show the drive picker.
    #[serde(rename = "needsDriveSelection")]
    NeedsDriveSelection {
        /// Drives to present in the picker.
        drives: Vec<DriveChoice>,
    },
    /// OAuth completed; `rclone_config_blob` must be passed to `add_destination`.
    #[serde(rename = "completed")]
    Completed {
        /// Serialised rclone INI stanza.
        rclone_config_blob: String,
    },
    /// OAuth flow failed; display `message` and offer a retry.
    #[serde(rename = "failed")]
    Failed {
        /// User-safe failure description.
        message: String,
    },
}
