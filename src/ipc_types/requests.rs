//! IPC request payload types — serialised argument structs for every Tauri command.

use serde::Serialize;
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
    /// Target vault identifier; `None` falls back to singleton discovery.
    #[zeroize(skip)]
    pub vault_id: Option<String>,
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

/// Argument payload for the `recover_vault_from_cloud` Tauri command.
#[derive(Debug, Clone, Serialize, Zeroize, ZeroizeOnDrop)]
#[serde(rename_all = "camelCase")]
pub struct RecoverVaultFromCloudRequest {
    /// Vault password (zeroize immediately after IPC call resolves).
    pub password: String,
    /// Absolute path to the key file, or `None` for Tier 1 (password-only) vaults.
    pub key_file_path: Option<String>,
    /// Cloud destination where the vault currently lives.
    #[zeroize(skip)]
    pub primary_destination: DestinationSessionConfig,
}

/// Argument payload for the `upload_file` Tauri command.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UploadFileRequest {
    /// Absolute path of the source file on the local filesystem.
    pub source_path: String,
    /// Vault-relative destination path.
    pub vault_path: String,
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
}

/// Argument payload for the `get_file_content` Tauri command.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GetFileContentRequest {
    /// Manifest node ID of the file to read.
    pub file_id: String,
}

/// Argument payload for the `add_destination` Tauri command.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AddDestinationRequest {
    /// Full destination configuration matching the backend `DestinationSessionConfig`.
    pub config: DestinationSessionConfig,
}

/// Argument payload for the `delete_destination` Tauri command.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteDestinationRequest {
    /// Unique destination identifier to delete.
    pub destination_id: String,
}

/// Argument payload for the `change_password` Tauri command.
#[derive(Debug, Clone, Serialize, Zeroize, ZeroizeOnDrop)]
#[serde(rename_all = "camelCase")]
pub struct ChangePasswordRequest {
    /// Current vault password (zeroise immediately after IPC call resolves).
    pub current_password: String,
    /// New vault password (zeroise immediately after IPC call resolves).
    pub new_password: String,
}

/// Argument payload for the `rotate_key_file` Tauri command.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RotateKeyFileRequest {
    /// Absolute destination path for the new key file.
    pub new_key_file_destination: String,
}

/// Argument payload for the `delete_vault` Tauri command.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteVaultRequest {
    /// Vault name confirmation (must match exactly for deletion to proceed).
    pub confirmation: String,
}

/// Argument payload for the `add_contact` Tauri command.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AddContactRequest {
    /// Human-readable contact name.
    pub display_name: String,
    /// Absolute path to the contact's public key file (32 bytes).
    pub public_key_path: String,
    /// Optional contact email address.
    pub email: Option<String>,
}

/// Argument payload for the `export_public_key` Tauri command.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportPublicKeyRequest {
    /// Absolute path where the 32-byte raw public key will be written.
    pub destination_path: String,
}

/// Argument payload for the `share_file` Tauri command.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShareFileRequest {
    /// Manifest node ID of the file to share.
    pub file_id: String,
    /// Unique contact ID (UUID as string) to share with.
    pub contact_id: String,
    /// Optional expiration period in days from now.
    pub expiration_days: Option<u32>,
    /// Whether to request a delivery receipt from the recipient.
    pub request_receipt: bool,
}

/// Argument payload for the `revoke_share` Tauri command.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RevokeShareRequest {
    /// Unique share ID to revoke.
    pub share_id: String,
}

/// Argument payload for the `configure_cloud` Tauri command.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigureCloudRequest {
    /// Cloud provider identifier (e.g. `"s3"`, `"b2"`, `"google_drive"`).
    pub provider: String,
    /// Storage bucket name.
    pub bucket: String,
    /// Cloud region identifier.
    pub region: String,
    /// Custom endpoint URL, or empty string for the provider default.
    pub endpoint: String,
    /// Path prefix within the bucket.
    pub path_prefix: String,
}

/// Argument payload for the `import_share` Tauri command.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportShareRequest {
    /// Absolute path to the share package file (.arxshare).
    pub share_package_path: String,
}

/// Argument payload for the `download_received_share` Tauri command.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadReceivedShareRequest {
    /// Unique share ID of the received share to download.
    pub share_id: String,
    /// Absolute path to the destination file on the local filesystem.
    pub destination_path: String,
}

/// Argument payload for the `compose_email_with_attachment` Tauri command.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ComposeEmailWithAttachmentRequest {
    /// Absolute filesystem path to the `.arxshare` package to attach.
    pub package_path: String,
    /// Email address of the recipient.
    pub recipient_email: String,
}

/// Argument payload for the `reveal_in_explorer` Tauri command.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RevealInExplorerRequest {
    /// Absolute filesystem path to reveal in the OS file explorer.
    pub path: String,
}
