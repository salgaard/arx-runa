//! OAuth setup response types for the Tauri IPC boundary.

use serde::Serialize;

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
