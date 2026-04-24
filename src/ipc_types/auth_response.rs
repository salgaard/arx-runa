use serde::Deserialize;

/// Response payload from the `authenticate` and `create_vault` Tauri commands.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthResponse {
    /// Identifier of the newly opened vault.
    pub vault_id: String,
    /// Human-readable display name of the vault.
    pub vault_name: String,
}
