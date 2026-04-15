//! Cloud transport forward declaration for Phase 4.1 / Phase 4.3.
//!
//! Phase 2.4 defines the minimum surface required by vault ceremonies; Phase 4
//! will expand error variants, add `delete_blob` / `list_blobs`, and replace
//! `MockCloudTransport` with `RcloneTransport`. The trait is intentionally
//! small: ceremonies only need `upload_blob` (for pending vault headers) and
//! `download_blob` (for recovery / vault probe). No deletion or listing is
//! required at this phase.

use async_trait::async_trait;
use thiserror::Error;

pub mod manifest_backup;
pub mod vault_header;

#[cfg(any(test, feature = "test-utils"))]
pub mod mock;

/// Opaque cloud-side blob name.
pub type BlobName = String;

/// Errors produced by [`CloudTransport`] implementations.
#[non_exhaustive]
#[derive(Debug, Error)]
pub enum CloudTransportError {
    /// The requested blob does not exist at the transport endpoint.
    #[error("blob not found")]
    NotFound,

    /// An I/O error occurred while talking to the cloud backend.
    #[error("I/O operation failed: {0}")]
    IoError(String),

    /// Any other transport-level failure that is not `NotFound` or `IoError`.
    #[error("cloud transport error: {0}")]
    Other(String),
}

/// Minimum cloud transport surface used by Phase 2.4 vault ceremonies.
///
/// Phase 4.1 / Phase 4.3 will extend this trait with listing and deletion
/// operations. Until then, ceremonies only upload and download single blobs
/// by name.
#[async_trait]
pub trait CloudTransport: Send + Sync {
    /// Upload `bytes` under `name`, overwriting any prior content.
    async fn upload_blob(
        &self,
        name: &str,
        bytes: &[u8],
    ) -> Result<(), CloudTransportError>;

    /// Download the blob stored under `name`.
    async fn download_blob(&self, name: &str) -> Result<Vec<u8>, CloudTransportError>;
}
