//! OAuth setup response types for the Tauri IPC boundary.

use serde::Serialize;

use crate::storage::cloud::DriveChoice as WizardDriveChoice;

/// A drive offered by the OneDrive chooser, surfaced to the UI picker.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DriveChoice {
    /// Opaque rclone drive identifier, echoed back via `select_oauth_drive`.
    pub id: String,
    /// Human-readable label (`"<DriveName> (<DriveType>)"`).
    pub label: String,
}

impl From<WizardDriveChoice> for DriveChoice {
    /// Maps the storage-layer drive choice onto the IPC type.
    fn from(choice: WizardDriveChoice) -> Self {
        Self {
            id: choice.id,
            label: choice.label,
        }
    }
}

/// Returned by `begin_google_drive_setup` and `begin_onedrive_setup`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BeginOauthSetupResponse {
    /// Opaque identifier used to poll or cancel this setup.
    pub setup_id: String,
    /// Local rclone auth URL the frontend must open in the system browser.
    pub auth_url: String,
}

/// Returned by `poll_oauth_setup`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase", tag = "status")]
pub enum OauthPollResponse {
    /// OAuth callback has not yet been received; poll again.
    Pending,
    /// The account exposes multiple drives; the user must choose one. Stop
    /// polling and call `select_oauth_drive` with the chosen drive's `id`.
    NeedsDriveSelection {
        /// Drives to present in the picker.
        drives: Vec<DriveChoice>,
    },
    /// OAuth completed. `rclone_config_blob` is the credential INI stanza.
    Completed {
        /// Serialised rclone INI stanza — pass directly to `add_destination`.
        rclone_config_blob: String,
    },
    /// OAuth flow failed. Show `message` and offer a retry.
    Failed {
        /// User-safe failure description.
        message: String,
    },
}
