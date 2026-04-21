use serde::Serialize;

/// Argument payload for the `list_directory` Tauri command.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListDirectoryRequest {
    /// Vault-relative path of the directory to list.
    pub path: String,
}
