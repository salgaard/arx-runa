//! Authentication response type.

use serde::Serialize;

/// Response returned from a successful authentication or vault creation.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthResponse {
    /// Opaque vault identifier (not a key, safe to surface to UI).
    pub vault_id: String,
    /// Human-readable vault name.
    pub vault_name: String,
}
