//! Arx Runa storage module.
//!
//! Storage layer: fixed-size chunking, SQLCipher manifest database, file-to-blob
//! pipeline.

pub mod cloud;
pub mod error;
pub mod metadata_store;
pub mod pipeline;
pub(crate) mod schema;
pub mod sqlcipher;
pub mod staging;
pub mod types;
pub(crate) mod validation;
pub mod vault_ops;

#[cfg(any(test, feature = "test-utils"))]
pub mod mock;

pub use error::StorageError;
pub use metadata_store::MetadataStore;
pub use pipeline::{decrypt_file, encrypt_file};
pub use sqlcipher::SqlCipherMetadataStore;
pub use types::{BlobName, ChunkRecord, Node, NodeId, NodeType};
pub use vault_ops::{
    RouteDecision, decide, delete_file, download_file, prepare_vault_storage, upload_file,
};
