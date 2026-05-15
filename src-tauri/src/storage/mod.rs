//! Arx Runa storage module.
//!
//! Storage layer: fixed-size chunking, SQLCipher manifest database, file-to-blob
//! pipeline.

pub mod cloud;
pub mod device_id;
pub mod error;
pub mod metadata_store;
pub mod pipeline;
pub(crate) mod schema;
pub mod sharing;
pub mod sqlcipher;
pub mod staging;
pub mod types;
pub(crate) mod validation;
pub mod vault_ops;

#[cfg(any(test, feature = "test-utils"))]
pub mod mock;

pub use cloud::{
    BackupSyncMode, DestinationSessionPublic, DestinationType, GoogleDriveRuntimePaths,
    GoogleDriveSetupRequest, GoogleDriveSetupResult, OpenerLike, RcloneTransport, S3SetupRequest,
    SyncConfig, setup_google_drive, setup_s3_provider,
};
pub use cloud::{
    CloudDeletionReport, PullReport, PushReport, SyncConflict, SyncError, delete_vault_from_cloud,
    pull_vault, push_vault,
};
pub use cloud::{CloudEndpoint, CloudTransport, CloudTransportError};
pub use error::StorageError;
pub use metadata_store::MetadataStore;
pub use pipeline::{decrypt_file, encrypt_file};
pub use sqlcipher::SqlCipherMetadataStore;
pub use types::{BlobName, ChunkRecord, Node, NodeId, NodeType};
pub use vault_ops::{
    RouteDecision, decide, delete_file, download_file, download_file_to_memory,
    prepare_vault_storage, reencrypt_file, upload_file,
};
