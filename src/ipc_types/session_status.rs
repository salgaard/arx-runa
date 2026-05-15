use serde::Deserialize;

/// Mirror of `src-tauri/src/ui/types/session_status.rs::SessionStatus`.
/// Kept in sync manually until shared type generation is introduced.
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SessionStatus {
    /// Whether the vault is currently unlocked and the session is active.
    pub is_unlocked: bool,
    /// Identifier of the currently open vault, or `None` when locked.
    pub vault_id: Option<String>,
    /// Idle-timeout duration in seconds configured for this session,
    /// or `None` if no timeout is set.
    pub timeout_seconds: Option<u64>,
    /// Vault authentication tier (1 or 2) if unlocked, `None` if locked.
    pub vault_tier: Option<u8>,
    /// Whether a BIP-39 recovery slot is configured for this vault; `None` if locked.
    pub has_recovery_slot: Option<bool>,
}
