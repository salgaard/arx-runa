//! Import share response type.

use serde::Deserialize;

/// Response returned from a successful `import_share` invocation.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportShareResponse {
    /// Display name of the file that was imported.
    pub file_name: String,
    /// Vault-relative path where the file now resides.
    pub vault_path: String,
}
