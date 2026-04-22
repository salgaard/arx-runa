//! IPC request payload types — serialised argument structs for every Tauri command.

use serde::Serialize;
use wasm_bindgen::JsValue;
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::ipc_types::DestinationSessionConfig;

/// Argument payload for the `list_directory` Tauri command.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListDirectoryRequest {
    /// Vault-relative path of the directory to list.
    pub path: String,
}

/// Argument payload for the `authenticate` Tauri command.
#[derive(Debug, Clone, Serialize, Zeroize, ZeroizeOnDrop)]
#[serde(rename_all = "camelCase")]
pub struct AuthenticateRequest {
    /// User password (zeroise immediately after IPC call resolves).
    pub password: String,
    /// Absolute path to the key file, or `None` for Tier 1 (password-only) vaults.
    pub key_file_path: Option<String>,
}

/// Argument payload for the `create_vault` Tauri command.
#[derive(Debug, Clone, Serialize, Zeroize, ZeroizeOnDrop)]
#[serde(rename_all = "camelCase")]
pub struct CreateVaultRequest {
    /// Human-readable vault name chosen by the user.
    pub vault_name: String,
    /// Vault password (zeroise immediately after IPC call resolves).
    pub password: String,
    /// Authentication tier: `1` = password-only, `2` = password + key file.
    pub tier: u8,
    /// Absolute destination path for the generated key file (Tier 2 only).
    pub key_file_destination: Option<String>,
    /// Primary cloud destination configuration for this vault.
    #[zeroize(skip)]
    pub primary_destination: DestinationSessionConfig,
    /// Chunk size in bytes, clamped to `[131_072, 67_108_864]` before submission.
    pub chunk_size_bytes: u64,
    /// Whether to enable the epoch buffer for small-file packing.
    pub epoch_buffer_enabled: bool,
}

/// Argument payload for the `upload_file` Tauri command.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UploadFileRequest {
    /// Absolute path of the source file on the local filesystem.
    pub source_path: String,
    /// Vault-relative destination path.
    pub vault_path: String,
    /// Serialised `IpcChannel` handle for streaming progress updates.
    ///
    /// Skipped during serde serialisation; the channel is wired in Phase 6.5.
    #[serde(skip)]
    pub progress: JsValue,
}

/// Argument payload for the `delete_file` Tauri command.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteFileRequest {
    /// Manifest node ID of the file to delete.
    pub file_id: String,
}

/// Argument payload for the `download_file` Tauri command.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadFileRequest {
    /// Manifest node ID of the file to download.
    pub file_id: String,
    /// Absolute path to the destination file on the local filesystem.
    pub destination_path: String,
    /// Serialised `IpcChannel` handle for streaming progress updates.
    ///
    /// Skipped during serde serialisation; the channel is wired at the UI layer.
    #[serde(skip)]
    pub progress: JsValue,
}

/// Argument payload for the `get_file_content` Tauri command.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GetFileContentRequest {
    /// Manifest node ID of the file to read.
    pub file_id: String,
}
