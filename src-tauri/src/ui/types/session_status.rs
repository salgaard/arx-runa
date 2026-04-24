//! Session status response type.

use serde::Serialize;

/// Current session status returned by `get_session_status`.
#[derive(Debug, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SessionStatus {
    /// Whether the vault is currently unlocked and a session is active.
    pub is_unlocked: bool,
    /// Vault ID if unlocked, `None` otherwise.
    pub vault_id: Option<String>,
    /// Seconds remaining until session timeout, `None` if locked.
    pub timeout_seconds: Option<u64>,
    /// Vault authentication tier (1 or 2) if unlocked, `None` if locked.
    pub vault_tier: Option<u8>,
}
