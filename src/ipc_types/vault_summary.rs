//! IPC response type mirroring the backend `VaultSummary`.

use serde::Deserialize;

/// Summary of a locally-discoverable vault returned by `list_vaults`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultSummary {
    /// Hyphenated UUID v4 vault identifier.
    pub vault_id: String,
    /// Human-readable name set at vault creation time, if present.
    pub name: Option<String>,
    /// Authentication tier: `1` (password only) or `2` (password + key file).
    pub tier: u8,
    /// Hex-encoded BLAKE3 hash of the key file; `None` for Tier 1 vaults.
    ///
    /// Used by auto-detection: when a removable drive mounts the frontend calls
    /// `scan_for_key_file` with this hash to find the matching 32-byte file.
    pub key_file_blake3: Option<String>,
}
