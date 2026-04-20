use std::path::Path;

use uuid::Uuid;

use crate::crypto::{KeyEncryptionKey, WrappedFileKey, unwrap_file_key};
use crate::storage::MetadataStore;
use crate::storage::error::StorageError;
use crate::storage::pipeline::decrypt_file;
use crate::storage::types::NodeType;

/// Downloads a file node from staged encrypted chunks into a destination file.
pub async fn download_file(
    destination: &Path,
    node_id: Uuid,
    metadata_store: &dyn MetadataStore,
    key_encryption_key: &KeyEncryptionKey,
    blob_directory: &Path,
) -> Result<(), StorageError> {
    let node = metadata_store.get_node(node_id).await?;
    if !matches!(node.node_type, NodeType::File) {
        return Err(StorageError::ConstraintViolation(
            "target is a directory".to_owned(),
        ));
    }
    let wrapped_key = node.file_key_wrapped.ok_or_else(|| {
        StorageError::ConstraintViolation("file node missing wrapped key".to_owned())
    })?;
    let wrapped_file_key = WrappedFileKey(wrapped_key);
    let file_key =
        unwrap_file_key(&wrapped_file_key, key_encryption_key).map_err(StorageError::from)?;
    let chunks = metadata_store.get_chunks(node_id).await?;
    decrypt_file(
        destination,
        *node.node_id.as_uuid(),
        &file_key,
        node.size_bytes,
        &chunks,
        blob_directory,
        metadata_store,
    )
    .await
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use async_trait::async_trait;
    use tempfile::TempDir;
    use tokio::fs;
    use tokio::io::AsyncWriteExt;
    use uuid::Uuid;

    use super::download_file;
    use crate::crypto::KeyEncryptionKey;
    use crate::storage::mock::MockMetadataStore;
    use crate::storage::vault_ops::upload_file;
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

    /// Writes source-file bytes for round-trip tests.
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

    /// Verifies one-chunk upload/download round trip.
    #[tokio::test]
    async fn test_upload_download_round_trip_one_chunk() {
        let temp_dir = TempDir::new().expect("temporary directory should be created");
        let source_path = temp_dir.path().join("source.bin");
        let destination_path = temp_dir.path().join("destination.bin");
        let staging_directory = temp_dir.path().join("staging");
        fs::create_dir_all(&staging_directory)
            .await
            .expect("staging directory should be created");
        let content = b"one chunk payload".to_vec();
        write_source_file(&source_path, &content).await;
        let store = EpochOverrideStore {
            inner: MockMetadataStore::new(),
            epoch_enabled: false,
        };
        let node_id = Uuid::new_v4();
        let key_encryption_key = KeyEncryptionKey::from_bytes([9; 32]);

        upload_file(
            &source_path,
            node_id,
            None,
            "one.bin",
            1,
            1,
            &store,
            &key_encryption_key,
            &staging_directory,
        )
        .await
        .expect("upload_file should succeed");
        download_file(
            &destination_path,
            node_id,
            &store,
            &key_encryption_key,
            &staging_directory,
        )
        .await
        .expect("download_file should succeed");

        let recovered = fs::read(destination_path)
            .await
            .expect("destination should be readable");
        assert_eq!(recovered, content);
    }

    /// Verifies multi-chunk upload/download round trip.
    #[tokio::test]
    async fn test_upload_download_round_trip_multi_chunk() {
        let temp_dir = TempDir::new().expect("temporary directory should be created");
        let source_path = temp_dir.path().join("source.bin");
        let destination_path = temp_dir.path().join("destination.bin");
        let staging_directory = temp_dir.path().join("staging");
        fs::create_dir_all(&staging_directory)
            .await
            .expect("staging directory should be created");
        let content = vec![0x53u8; 4_194_304 + 1024];
        write_source_file(&source_path, &content).await;
        let store = EpochOverrideStore {
            inner: MockMetadataStore::new(),
            epoch_enabled: false,
        };
        let node_id = Uuid::new_v4();
        let key_encryption_key = KeyEncryptionKey::from_bytes([11; 32]);

        upload_file(
            &source_path,
            node_id,
            None,
            "multi.bin",
            1,
            1,
            &store,
            &key_encryption_key,
            &staging_directory,
        )
        .await
        .expect("upload_file should succeed");
        download_file(
            &destination_path,
            node_id,
            &store,
            &key_encryption_key,
            &staging_directory,
        )
        .await
        .expect("download_file should succeed");

        let recovered = fs::read(destination_path)
            .await
            .expect("destination should be readable");
        assert_eq!(recovered, content);
    }

    /// Verifies wrong KEK produces a decrypt failure surfaced as storage error.
    #[tokio::test]
    async fn test_download_file_wrong_kek_fails_with_database_or_checksum_error() {
        let temp_dir = TempDir::new().expect("temporary directory should be created");
        let source_path = temp_dir.path().join("source.bin");
        let destination_path = temp_dir.path().join("destination.bin");
        let staging_directory = temp_dir.path().join("staging");
        fs::create_dir_all(&staging_directory)
            .await
            .expect("staging directory should be created");
        write_source_file(&source_path, b"wrong kek payload").await;
        let store = EpochOverrideStore {
            inner: MockMetadataStore::new(),
            epoch_enabled: false,
        };
        let node_id = Uuid::new_v4();
        let upload_key = KeyEncryptionKey::from_bytes([13; 32]);
        let wrong_key = KeyEncryptionKey::from_bytes([17; 32]);

        upload_file(
            &source_path,
            node_id,
            None,
            "wrong-kek.bin",
            1,
            1,
            &store,
            &upload_key,
            &staging_directory,
        )
        .await
        .expect("upload_file should succeed");

        let result = download_file(
            &destination_path,
            node_id,
            &store,
            &wrong_key,
            &staging_directory,
        )
        .await;

        assert!(matches!(
            result,
            Err(StorageError::Database(_)) | Err(StorageError::ChecksumMismatch)
        ));
    }
}
