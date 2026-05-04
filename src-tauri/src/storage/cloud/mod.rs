//! Canonical cloud transport trait, errors, endpoint descriptor, and transport implementations.

use async_trait::async_trait;
use thiserror::Error;

use std::path::Path;

pub(crate) mod cloud_config;
pub(crate) mod destination_session;
mod endpoint;
pub mod manifest_backup;
mod rclone;
pub(crate) mod rclone_subprocess;
pub(crate) mod remote_path;
pub(crate) mod stderr_sanitiser;
pub mod sync;
mod sync_config;
pub mod vault_header;
pub mod vault_header_io;
mod wizard;

#[cfg(any(test, feature = "test-utils"))]
pub mod mock;
pub use destination_session::{BackupSyncMode, DestinationSessionPublic, DestinationType};
pub use endpoint::CloudEndpoint;
pub use manifest_backup::{
    MANIFEST_BACKUP_BLOB_NAME, ManifestBackupSyncError, download_manifest_backup,
    upload_manifest_backup,
};
pub use rclone::RcloneTransport;
pub use sync::{
    CloudDeletionReport, PullReport, PushReport, SyncConflict, SyncError, delete_vault_from_cloud,
    pull_vault, push_vault,
};
pub use sync_config::SyncConfig;
pub use vault_header_io::{
    VAULT_HEADER_BLOB_NAME, VAULT_HEADER_UPLOAD_STAGING_FILE_NAME, VaultHeaderSyncError,
    download_vault_header, upload_vault_header,
};
pub use wizard::{
    GoogleDriveRuntimePaths, GoogleDriveSetupRequest, GoogleDriveSetupResult, OpenerLike,
    S3SetupRequest, setup_google_drive, setup_s3_provider,
};

/// Errors produced by [`CloudTransport`] implementations.
#[derive(Debug, Error)]
pub enum CloudTransportError {
    #[error("blob not found at remote path")]
    NotFound,
    #[error("cloud transport authentication failed")]
    AuthenticationFailed,
    #[error("cloud transport operation timed out")]
    Timeout,
    #[error("cloud transport local I/O error")]
    IoError(#[from] std::io::Error),
    #[error("rclone process failed with exit code {exit_code}")]
    RcloneProcessFailed {
        exit_code: i32,
        stderr_sanitised: String,
    },
    #[error("cloud transport error: {0}")]
    Other(String),
}

/// Cloud transport contract used by ceremonies and cloud sync components.
#[async_trait]
pub trait CloudTransport: Send + Sync {
    /// Uploads a local file to a cloud-relative remote path.
    async fn upload_blob(
        &self,
        local_path: &Path,
        remote_path: &str,
    ) -> Result<(), CloudTransportError>;

    /// Downloads a cloud-relative remote path into a local file path.
    async fn download_blob(
        &self,
        remote_path: &str,
        local_path: &Path,
    ) -> Result<(), CloudTransportError>;

    /// Deletes a cloud-relative remote path.
    async fn delete_blob(&self, remote_path: &str) -> Result<(), CloudTransportError>;

    /// Lists cloud-relative remote paths under a prefix.
    async fn list_blobs(&self, remote_prefix: &str) -> Result<Vec<String>, CloudTransportError>;

    /// Returns `true` when this transport is backed by a real cloud backend.
    ///
    /// The default implementation returns `true`. `NoOpCloudTransport` overrides
    /// this to return `false` so callers can skip cloud operations that are
    /// meaningless for local-only vaults without attempting a round-trip that
    /// will always fail.
    fn is_configured(&self) -> bool {
        true
    }

    /// Cleans up session-scoped artifacts (e.g., rclone.conf temp files).
    ///
    /// Called on session lock or timeout to remove sensitive files from disk.
    /// Implementations must be idempotent (file not found is not an error).
    async fn cleanup_session_artifacts(&self) -> Result<(), CloudTransportError> {
        Ok(())
    }

    /// Generates scoped credentials for a recipient to download a shared prefix.
    ///
    /// Returns `None` when the backend does not support credential generation
    /// (e.g., local-only transports).  Callers must treat `None` as "fall back
    /// to path-prefix-only endpoint".
    async fn generate_share_credentials(
        &self,
        path_prefix: &str,
        ttl_seconds: u32,
        receipt_requested: bool,
    ) -> Result<Option<serde_json::Value>, CloudTransportError> {
        let _ = (path_prefix, ttl_seconds, receipt_requested);
        Ok(None)
    }
}
