//! Manifest metadata storage abstraction.

use async_trait::async_trait;
use uuid::Uuid;

use crate::storage::error::StorageError;
use crate::storage::types::{ChunkRecord, EpochBlobRecord, EpochBufferEntry, Node};

/// Abstraction over manifest metadata persistence.
#[async_trait]
pub trait MetadataStore: Send + Sync {
    /// Inserts a node row into the manifest.
    ///
    /// Returns `ConstraintViolation` when primary/foreign/check constraints fail,
    /// or when the provided `parent_id` is not a directory (including self-parent).
    async fn insert_node(&self, node: &Node) -> Result<(), StorageError>;

    /// Inserts one or more chunk rows into the manifest.
    ///
    /// Returns `ConstraintViolation` for duplicate `(node_id, chunk_index)` or
    /// `blob_name` collisions.
    async fn insert_chunks(&self, chunks: &[ChunkRecord]) -> Result<(), StorageError>;

    /// Inserts a file node and all associated chunk rows atomically.
    ///
    /// Implementations must ensure both inserts succeed or neither is persisted.
    async fn insert_file_with_chunks(
        &self,
        node: &Node,
        chunks: &[ChunkRecord],
    ) -> Result<(), StorageError>;

    /// Loads a node by identifier.
    ///
    /// Returns `NotFound` when no row matches.
    async fn get_node(&self, node_id: Uuid) -> Result<Node, StorageError>;

    /// Lists direct children for the provided parent node identifier.
    ///
    /// Returns an empty vector when the parent has no children.
    async fn list_children(&self, parent_id: Uuid) -> Result<Vec<Node>, StorageError>;

    /// Returns all chunk rows for a node ordered by `chunk_index`.
    ///
    /// Returns an empty vector when the file has no chunk rows.
    async fn get_chunks(&self, node_id: Uuid) -> Result<Vec<ChunkRecord>, StorageError>;

    /// Renames a node and updates `modified_at` in one mutation.
    ///
    /// Returns `NotFound` when the target node does not exist.
    async fn rename_node(
        &self,
        node_id: Uuid,
        new_name: &str,
        modified_at: i64,
    ) -> Result<(), StorageError>;

    /// Moves a node to a new parent and updates `modified_at` in one mutation.
    ///
    /// `new_parent_id = None` moves the node to root.
    /// Returns `ConstraintViolation` when the move violates hierarchy rules:
    /// parent must be a directory, self-parent is forbidden, and cycles are rejected.
    async fn move_node(
        &self,
        node_id: Uuid,
        new_parent_id: Option<Uuid>,
        modified_at: i64,
    ) -> Result<(), StorageError>;

    /// Deletes a node and its cascading chunk rows.
    ///
    /// Implementations must enqueue `blob_name` entries into `pending_deletions`
    /// in the same transaction as the node delete.
    async fn delete_node(&self, node_id: Uuid) -> Result<(), StorageError>;

    /// Lists queued blob names from `pending_deletions`.
    ///
    /// Implementations should return at most `limit` entries.
    async fn list_pending_deletions(&self, limit: usize) -> Result<Vec<String>, StorageError>;

    /// Removes a blob name from `pending_deletions` after successful cloud delete.
    async fn mark_deletion_complete(&self, blob_name: &str) -> Result<(), StorageError>;

    /// Retrieves a manifest-meta value by key.
    async fn get_meta(&self, key: &str) -> Result<Option<String>, StorageError>;

    /// Sets or replaces a manifest-meta key/value pair.
    ///
    /// Returns `ConstraintViolation` when attempting to mutate immutable keys:
    /// `schema_version`, `vault_id`, `snapshot_counter`, `chunk_size_bytes`,
    /// and `epoch_buffer_enabled`.
    async fn set_meta(&self, key: &str, value: &str) -> Result<(), StorageError>;

    /// Atomically increments and returns `snapshot_counter`.
    /// This is the only supported mutation path for `snapshot_counter`.
    async fn increment_snapshot_counter(&self) -> Result<u64, StorageError>;

    /// Inserts a file node row without any associated chunk rows.
    async fn insert_file_node_only(&self, node: &Node) -> Result<(), StorageError>;

    /// Stages a plaintext entry in the epoch buffer for the given node.
    async fn stage_epoch_entry(&self, node_id: Uuid, plaintext: Vec<u8>)
        -> Result<(), StorageError>;

    /// Returns the total number of bytes currently staged in the epoch buffer.
    async fn get_epoch_buffer_total_bytes(&self) -> Result<u64, StorageError>;

    /// Returns all entries currently staged in the epoch buffer.
    async fn get_epoch_buffer_entries(&self) -> Result<Vec<EpochBufferEntry>, StorageError>;

    /// Atomically: insert epoch_blobs row, insert epoch chunk rows into chunks table,
    /// and clear the flushed epoch_buffer entries.
    /// extents: (node_id, chunk_index, byte_offset, byte_length)
    async fn commit_epoch_flush(
        &self,
        record: &EpochBlobRecord,
        extents: &[(Uuid, u32, u64, u64)],
    ) -> Result<(), StorageError>;

    /// Retrieves an epoch blob record by identifier.
    ///
    /// Returns `NotFound` when no row matches.
    async fn get_epoch_blob(&self, epoch_blob_id: Uuid) -> Result<EpochBlobRecord, StorageError>;
}
