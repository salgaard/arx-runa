//! Arx Runa storage module.
//!
//! Storage layer: fixed-size chunking, SQLCipher manifest database, file-to-blob
//! pipeline.

pub mod cloud;
pub mod error;
pub mod metadata_store;
pub(crate) mod schema;
pub(crate) mod validation;
pub mod sqlcipher;
pub mod types;

#[cfg(any(test, feature = "test-utils"))]
pub mod mock;

pub use error::StorageError;
pub use metadata_store::MetadataStore;
pub use sqlcipher::SqlCipherMetadataStore;
pub use types::{BlobName, ChunkRecord, Node, NodeId, NodeType};
