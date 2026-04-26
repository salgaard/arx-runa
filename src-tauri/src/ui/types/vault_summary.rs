//! IPC response type for vault discovery.

use serde::Serialize;

/// Summary of a locally-discoverable vault returned by `list_vaults`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultSummary {
    /// Hyphenated UUID v4 vault identifier.
    pub vault_id: String,
    /// Human-readable name set at vault creation time, if present.
    pub name: Option<String>,
    /// Authentication tier: `1` (password only) or `2` (password + key file).
    pub tier: u8,
}
