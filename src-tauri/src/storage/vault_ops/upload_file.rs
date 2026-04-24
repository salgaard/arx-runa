use std::path::Path;

use tokio::fs;
use uuid::Uuid;

use crate::crypto::{KeyEncryptionKey, generate_file_key, wrap_file_key};
use crate::storage::error::StorageError;
use crate::storage::pipeline;
use crate::storage::types::{Node, NodeType};
use crate::storage::vault_ops::{RouteDecision, decide};
use crate::storage::{MetadataStore, NodeId};

/// Uploads a local file into staged encrypted chunks and persists manifest rows.
///
/// The optional `progress` callback is forwarded to the encryption pipeline and
/// invoked after each chunk write with `(bytes_processed, file_size_total)`.
/// Pass `None` to suppress progress reporting.  The callback MUST NOT import
/// or depend on `tauri::`.
#[allow(clippy::too_many_arguments)]
pub async fn upload_file(
    source: &Path,
    node_id: Uuid,
    parent_id: Option<Uuid>,
    name: &str,
    created_at: i64,
    modified_at: i64,
    metadata_store: &dyn MetadataStore,
    key_encryption_key: &KeyEncryptionKey,
    staging_directory: &Path,
    progress: Option<&(dyn Fn(u64, u64) + Send + Sync)>,
) -> Result<Node, StorageError> {
    let source_metadata = fs::metadata(source)
        .await
        .map_err(|error| StorageError::Io(error.to_string()))?;
    let file_size = source_metadata.len();
    let chunk_size_bytes = pipeline::read_chunk_size_bytes(metadata_store).await?;
    let epoch_buffer_enabled = read_epoch_buffer_enabled(metadata_store).await?;
    let route_decision = decide(file_size, chunk_size_bytes, epoch_buffer_enabled);
    if matches!(route_decision, RouteDecision::EpochBuffer) {
        return Err(StorageError::ConstraintViolation(
            "epoch buffering not yet available; deferred to Phase 4".to_owned(),
        ));
    }

    let file_key = generate_file_key();
    let wrapped_file_key =
        wrap_file_key(&file_key, key_encryption_key).map_err(StorageError::from)?;
    let mut chunks = pipeline::encrypt_file(
        source,
        node_id,
        &file_key,
        metadata_store,
        staging_directory,
        progress,
    )
    .await?;
    pipeline::assign_node_id(&mut chunks, NodeId::new(node_id));
    let node = Node::new(
        node_id,
        parent_id,
        NodeType::File,
        name.to_owned(),
        created_at,
        modified_at,
        file_size,
        Some(wrapped_file_key.0),
    );
    if let Err(error) = metadata_store.insert_file_with_chunks(&node, &chunks).await {
        cleanup_staged_blobs(staging_directory, &chunks).await;
        return Err(error);
    }
    Ok(node)
}

/// Reads and parses `epoch_buffer_enabled` from manifest metadata.
async fn read_epoch_buffer_enabled(
    metadata_store: &dyn MetadataStore,
) -> Result<bool, StorageError> {
    let epoch_text = metadata_store
        .get_meta("epoch_buffer_enabled")
        .await?
        .ok_or_else(|| {
            StorageError::Database("missing manifest_meta key: epoch_buffer_enabled".to_owned())
        })?;
    match epoch_text.as_str() {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(StorageError::Database(
            "invalid epoch_buffer_enabled: expected true or false".to_owned(),
        )),
    }
}

/// Removes staged blobs when upload persistence fails after encryption.
async fn cleanup_staged_blobs(staging_directory: &Path, chunks: &[crate::storage::ChunkRecord]) {
    for chunk in chunks {
        let blob_path = staging_directory.join(format!("{}.blob", chunk.blob_name));
        let _ = tokio::fs::remove_file(blob_path).await;
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use async_trait::async_trait;
    use tempfile::TempDir;
    use tokio::fs;
    use tokio::io::AsyncWriteExt;
    use uuid::Uuid;

    use super::upload_file;
    use crate::crypto::KeyEncryptionKey;
    use crate::storage::mock::MockMetadataStore;
    use crate::storage::{ChunkRecord, MetadataStore, Node, StorageError};

    /// Metadata-store wrapper that overrides only `epoch_buffer_enabled`.
    struct EpochOverrideStore {
        inner: MockMetadataStore,
        epoch_enabled: bool,
    }

    #[async_trait]
    impl MetadataStore for EpochOverrideStore {
        /// Delegates node insert.
        async fn insert_node(&self, node: &Node) -> Result<(), StorageError> {
            self.inner.insert_node(node).await
        }

        /// Delegates chunk insert.
        async fn insert_chunks(&self, chunks: &[ChunkRecord]) -> Result<(), StorageError> {
            self.inner.insert_chunks(chunks).await
        }

        /// Delegates atomic file-plus-chunks insert.
        async fn insert_file_with_chunks(
            &self,
            node: &Node,
            chunks: &[ChunkRecord],
        ) -> Result<(), StorageError> {
            self.inner.insert_file_with_chunks(node, chunks).await
        }

        /// Delegates node fetch.
        async fn get_node(&self, node_id: Uuid) -> Result<Node, StorageError> {
            self.inner.get_node(node_id).await
        }

        /// Delegates child listing.
        async fn list_children(&self, parent_id: Uuid) -> Result<Vec<Node>, StorageError> {
            self.inner.list_children(parent_id).await
        }

        /// Delegates chunk listing.
        async fn get_chunks(&self, node_id: Uuid) -> Result<Vec<ChunkRecord>, StorageError> {
            self.inner.get_chunks(node_id).await
        }

        /// Delegates rename.
        async fn rename_node(
            &self,
            node_id: Uuid,
            new_name: &str,
            modified_at: i64,
        ) -> Result<(), StorageError> {
            self.inner.rename_node(node_id, new_name, modified_at).await
        }

        /// Delegates move.
        async fn move_node(
            &self,
            node_id: Uuid,
            new_parent_id: Option<Uuid>,
            modified_at: i64,
        ) -> Result<(), StorageError> {
            self.inner
                .move_node(node_id, new_parent_id, modified_at)
                .await
        }

        /// Delegates delete.
        async fn delete_node(&self, node_id: Uuid) -> Result<(), StorageError> {
            self.inner.delete_node(node_id).await
        }

        /// Delegates pending-deletions listing.
        async fn list_pending_deletions(&self, limit: usize) -> Result<Vec<String>, StorageError> {
            self.inner.list_pending_deletions(limit).await
        }

        /// Delegates deletion completion.
        async fn mark_deletion_complete(&self, blob_name: &str) -> Result<(), StorageError> {
            self.inner.mark_deletion_complete(blob_name).await
        }

        /// Overrides epoch meta and delegates all other metadata reads.
        async fn get_meta(&self, key: &str) -> Result<Option<String>, StorageError> {
            if key == "epoch_buffer_enabled" {
                if self.epoch_enabled {
                    return Ok(Some("true".to_owned()));
                }
                return Ok(Some("false".to_owned()));
            }
            self.inner.get_meta(key).await
        }

        /// Delegates metadata mutation.
        async fn set_meta(&self, key: &str, value: &str) -> Result<(), StorageError> {
            self.inner.set_meta(key, value).await
        }

        /// Delegates snapshot increment.
        async fn increment_snapshot_counter(&self) -> Result<u64, StorageError> {
            self.inner.increment_snapshot_counter().await
        }
    }

    /// Writes source-file bytes for upload tests.
    async fn write_source_file(path: &Path, content: &[u8]) {
        let mut source_file = fs::File::create(path)
            .await
            .expect("source file should be created");
        source_file
            .write_all(content)
            .await
            .expect("source content should be written");
        source_file
            .flush()
            .await
            .expect("source file should be flushed");
    }

    /// Verifies epoch-enabled small files are deferred to phase 4 branch.
    #[tokio::test]
    async fn test_upload_file_with_epoch_enabled_small_file_returns_constraint_violation() {
        let temp_dir = TempDir::new().expect("temporary directory should be created");
        let source_path = temp_dir.path().join("source.bin");
        let staging_directory = temp_dir.path().join("staging");
        fs::create_dir_all(&staging_directory)
            .await
            .expect("staging directory should be created");
        write_source_file(&source_path, &[0x01]).await;
        let store = EpochOverrideStore {
            inner: MockMetadataStore::new(),
            epoch_enabled: true,
        };
        let key_encryption_key = KeyEncryptionKey::from_bytes([3; 32]);

        let result = upload_file(
            &source_path,
            Uuid::new_v4(),
            None,
            "small.bin",
            1,
            1,
            &store,
            &key_encryption_key,
            &staging_directory,
            None,
        )
        .await;

        assert!(matches!(
            result,
            Err(StorageError::ConstraintViolation(message))
                if message.contains("epoch buffering not yet available")
        ));
    }

    /// Verifies epoch-enabled files at or above chunk size stay on immediate route.
    #[tokio::test]
    async fn test_upload_file_with_epoch_enabled_large_file_succeeds() {
        let temp_dir = TempDir::new().expect("temporary directory should be created");
        let source_path = temp_dir.path().join("source.bin");
        let staging_directory = temp_dir.path().join("staging");
        fs::create_dir_all(&staging_directory)
            .await
            .expect("staging directory should be created");
        let content = vec![0x77u8; 4_194_304];
        write_source_file(&source_path, &content).await;
        let store = EpochOverrideStore {
            inner: MockMetadataStore::new(),
            epoch_enabled: true,
        };
        let key_encryption_key = KeyEncryptionKey::from_bytes([5; 32]);
        let node_id = Uuid::new_v4();

        let result = upload_file(
            &source_path,
            node_id,
            None,
            "large.bin",
            1,
            1,
            &store,
            &key_encryption_key,
            &staging_directory,
            None,
        )
        .await;

        assert!(result.is_ok());
        let stored_node = store
            .get_node(node_id)
            .await
            .expect("uploaded node should be persisted");
        assert_eq!(stored_node.size_bytes, content.len() as u64);
    }

    /// Verifies upload persists wrapped key and chunk rows.
    #[tokio::test]
    async fn test_upload_file_persists_wrapped_key_and_chunks() {
        let temp_dir = TempDir::new().expect("temporary directory should be created");
        let source_path = temp_dir.path().join("source.bin");
        let staging_directory = temp_dir.path().join("staging");
        fs::create_dir_all(&staging_directory)
            .await
            .expect("staging directory should be created");
        write_source_file(&source_path, b"hello upload").await;
        let store = EpochOverrideStore {
            inner: MockMetadataStore::new(),
            epoch_enabled: false,
        };
        let key_encryption_key = KeyEncryptionKey::from_bytes([7; 32]);
        let node_id = Uuid::new_v4();

        upload_file(
            &source_path,
            node_id,
            None,
            "hello.bin",
            1,
            1,
            &store,
            &key_encryption_key,
            &staging_directory,
            None,
        )
        .await
        .expect("upload_file should succeed");

        let stored_node = store
            .get_node(node_id)
            .await
            .expect("uploaded node should be stored");
        assert!(stored_node.file_key_wrapped.is_some());
        let stored_chunks = store
            .get_chunks(node_id)
            .await
            .expect("uploaded chunks should be stored");
        assert_eq!(stored_chunks.len(), 1);
    }
}
