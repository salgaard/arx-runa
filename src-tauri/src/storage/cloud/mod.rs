//! Canonical cloud transport trait, errors, and endpoint descriptor; Phase 4.2 adds `RcloneTransport`.

use async_trait::async_trait;
use thiserror::Error;

use std::path::Path;

mod endpoint;
pub mod manifest_backup;
pub mod vault_header;

#[cfg(any(test, feature = "test-utils"))]
pub mod mock;
pub use endpoint::CloudEndpoint;

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
}
