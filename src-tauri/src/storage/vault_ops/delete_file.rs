use std::path::Path;

use uuid::Uuid;

use crate::storage::MetadataStore;
use crate::storage::error::StorageError;
use crate::storage::types::NodeType;
use crate::storage::validation::validate_blob_name_uuid_v4;

/// Deletes a file node from metadata and removes any local staged blob files.
///
/// Metadata deletion is transactional via [`MetadataStore::delete_node`]. Local
/// blob deletion is best-effort after commit: missing files are tolerated and
/// other per-file removal failures are logged. Directory targets are rejected
/// with `ConstraintViolation`.
pub async fn delete_file(
    node_id: Uuid,
    metadata_store: &dyn MetadataStore,
    staging_directory: &Path,
) -> Result<(), StorageError> {
    let node = metadata_store.get_node(node_id).await?;
    if node.node_type == NodeType::Directory {
        return Err(StorageError::ConstraintViolation(
            "target is a directory".to_owned(),
        ));
    }
    let chunks = metadata_store.get_chunks(node_id).await?;
    let mut validated_blob_paths = Vec::with_capacity(chunks.len() * 3);
    for chunk in &chunks {
        validate_blob_name_uuid_v4(&chunk.blob_name)?;
        let blob_file = format!("{}.blob", chunk.blob_name);
        validated_blob_paths.push(staging_directory.join("pending").join(&blob_file));
        validated_blob_paths.push(staging_directory.join("cache").join(&blob_file));
        validated_blob_paths.push(staging_directory.join(&blob_file));
    }
    metadata_store.delete_node(node_id).await?;
    for blob_path in validated_blob_paths {
        if let Err(error) = tokio::fs::remove_file(&blob_path).await
            && error.kind() != std::io::ErrorKind::NotFound
        {
            tracing::warn!(
                path = %blob_path.display(),
                ?error,
                "local staged blob delete failed after manifest commit"
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    use async_trait::async_trait;
    use tempfile::tempdir;
    use tokio::fs;
    use uuid::Uuid;

    use super::delete_file;
    use crate::crypto::KeyEncryptionKey;
    use crate::storage::MetadataStore;
    use crate::storage::error::StorageError;
    use crate::storage::types::{ChunkRecord, Node, NodeId, NodeType};
    use crate::storage::vault_ops::upload_file;
    use crate::storage::{SqlCipherMetadataStore, staging};

    struct BadChunkStore {
        node: Node,
        chunk: ChunkRecord,
        delete_called: Arc<AtomicBool>,
    }

    #[async_trait]
    impl MetadataStore for BadChunkStore {
        async fn insert_node(&self, _node: &Node) -> Result<(), StorageError> {
            Err(StorageError::Database(
                "unused test helper method".to_owned(),
            ))
        }

        async fn insert_chunks(&self, _chunks: &[ChunkRecord]) -> Result<(), StorageError> {
            Err(StorageError::Database(
                "unused test helper method".to_owned(),
            ))
        }

        async fn insert_file_with_chunks(
            &self,
            _node: &Node,
            _chunks: &[ChunkRecord],
        ) -> Result<(), StorageError> {
            Err(StorageError::Database(
                "unused test helper method".to_owned(),
            ))
        }

        async fn get_node(&self, _node_id: Uuid) -> Result<Node, StorageError> {
            Ok(self.node.clone())
        }

        async fn list_children(&self, _parent_id: Uuid) -> Result<Vec<Node>, StorageError> {
            Err(StorageError::Database(
                "unused test helper method".to_owned(),
            ))
        }

        async fn get_chunks(&self, _node_id: Uuid) -> Result<Vec<ChunkRecord>, StorageError> {
            Ok(vec![self.chunk.clone()])
        }

        async fn rename_node(
            &self,
            _node_id: Uuid,
            _new_name: &str,
            _modified_at: i64,
        ) -> Result<(), StorageError> {
            Err(StorageError::Database(
                "unused test helper method".to_owned(),
            ))
        }

        async fn move_node(
            &self,
            _node_id: Uuid,
            _new_parent_id: Option<Uuid>,
            _modified_at: i64,
        ) -> Result<(), StorageError> {
            Err(StorageError::Database(
                "unused test helper method".to_owned(),
            ))
        }

        async fn delete_node(&self, _node_id: Uuid) -> Result<(), StorageError> {
            self.delete_called.store(true, Ordering::Relaxed);
            Ok(())
        }

        async fn list_pending_deletions(&self, _limit: usize) -> Result<Vec<String>, StorageError> {
            Err(StorageError::Database(
                "unused test helper method".to_owned(),
            ))
        }

        async fn mark_deletion_complete(&self, _blob_name: &str) -> Result<(), StorageError> {
            Err(StorageError::Database(
                "unused test helper method".to_owned(),
            ))
        }

        async fn get_meta(&self, _key: &str) -> Result<Option<String>, StorageError> {
            Err(StorageError::Database(
                "unused test helper method".to_owned(),
            ))
        }

        async fn set_meta(&self, _key: &str, _value: &str) -> Result<(), StorageError> {
            Err(StorageError::Database(
                "unused test helper method".to_owned(),
            ))
        }

        async fn increment_snapshot_counter(&self) -> Result<u64, StorageError> {
            Err(StorageError::Database(
                "unused test helper method".to_owned(),
            ))
        }
        /// Fails for this test helper.
        async fn insert_file_node_only(
            &self,
            _node: &crate::storage::types::Node,
        ) -> Result<(), crate::storage::error::StorageError> {
            Err(crate::storage::error::StorageError::Database(
                "unused test helper method".to_owned(),
            ))
        }

        /// Fails for this test helper.
        async fn insert_file_node_and_stage_epoch_entry(
            &self,
            _node: &crate::storage::types::Node,
            _plaintext: zeroize::Zeroizing<Vec<u8>>,
        ) -> Result<(), crate::storage::error::StorageError> {
            Err(crate::storage::error::StorageError::Database(
                "unused test helper method".to_owned(),
            ))
        }

        /// Fails for this test helper.
        async fn stage_epoch_entry(
            &self,
            _node_id: uuid::Uuid,
            _plaintext: zeroize::Zeroizing<Vec<u8>>,
        ) -> Result<(), crate::storage::error::StorageError> {
            Err(crate::storage::error::StorageError::Database(
                "unused test helper method".to_owned(),
            ))
        }

        /// Fails for this test helper.
        async fn get_epoch_buffer_total_bytes(
            &self,
        ) -> Result<u64, crate::storage::error::StorageError> {
            Err(crate::storage::error::StorageError::Database(
                "unused test helper method".to_owned(),
            ))
        }

        /// Fails for this test helper.
        async fn get_epoch_buffer_entries(
            &self,
        ) -> Result<Vec<crate::storage::types::EpochBufferEntry>, crate::storage::error::StorageError>
        {
            Err(crate::storage::error::StorageError::Database(
                "unused test helper method".to_owned(),
            ))
        }

        /// Fails for this test helper.
        async fn commit_epoch_flush(
            &self,
            _record: &crate::storage::types::EpochBlobRecord,
            _extents: &[(uuid::Uuid, u32, u64, u64)],
        ) -> Result<(), crate::storage::error::StorageError> {
            Err(crate::storage::error::StorageError::Database(
                "unused test helper method".to_owned(),
            ))
        }

        /// Fails for this test helper.
        async fn get_epoch_blob(
            &self,
            _epoch_blob_id: uuid::Uuid,
        ) -> Result<crate::storage::types::EpochBlobRecord, crate::storage::error::StorageError>
        {
            Err(crate::storage::error::StorageError::Database(
                "unused test helper method".to_owned(),
            ))
        }

        /// Returns an empty list for this test helper.
        async fn get_epoch_buffer_node_ids(
            &self,
        ) -> Result<Vec<uuid::Uuid>, crate::storage::error::StorageError> {
            Ok(vec![])
        }
    }

    fn zero_byte_node(node_id: Uuid) -> Node {
        Node::new(
            node_id,
            None,
            NodeType::File,
            "empty.txt".to_owned(),
            1,
            1,
            0,
            Some([9; 72]),
        )
    }

    fn directory_node(node_id: Uuid) -> Node {
        Node::new(
            node_id,
            None,
            NodeType::Directory,
            "folder".to_owned(),
            1,
            1,
            0,
            None,
        )
    }

    async fn create_store(db_path: &std::path::Path) -> SqlCipherMetadataStore {
        SqlCipherMetadataStore::create(db_path, &[5; 32], Uuid::new_v4(), 4_194_304, false)
            .await
            .expect("store should be created")
    }

    #[tokio::test]
    async fn test_delete_file_removes_node_chunks_pending_queue_and_local_blobs() {
        let temp = tempdir().expect("tempdir should be created");
        let db_path = temp.path().join("manifest.db");
        let source_path = temp.path().join("source.bin");
        let staging_directory = temp.path().join("staging");
        let store = create_store(&db_path).await;
        staging::ensure_staging_directory(&staging_directory)
            .await
            .expect("staging directory should be ensured");
        fs::write(&source_path, vec![0x33u8; 4_194_304 + 64])
            .await
            .expect("source file should be written");
        let node_id = Uuid::new_v4();

        upload_file(
            &source_path,
            node_id,
            None,
            "payload.bin",
            1,
            1,
            &store,
            &KeyEncryptionKey::from_bytes([1; 32]),
            &staging_directory,
            None,
        )
        .await
        .expect("upload should succeed");
        let chunks = store
            .get_chunks(node_id)
            .await
            .expect("chunks should be persisted");
        let expected_blob_names = chunks
            .iter()
            .map(|chunk| chunk.blob_name.clone())
            .collect::<std::collections::HashSet<_>>();

        delete_file(node_id, &store, &staging_directory)
            .await
            .expect("delete_file should succeed");

        assert!(matches!(
            store.get_node(node_id).await,
            Err(StorageError::NotFound)
        ));
        for chunk in &chunks {
            let blob_path = staging_directory.join(format!("{}.blob", chunk.blob_name));
            assert!(!blob_path.exists());
        }
        let pending = store
            .list_pending_deletions(10)
            .await
            .expect("pending deletions should load");
        assert_eq!(
            pending
                .into_iter()
                .collect::<std::collections::HashSet<_>>(),
            expected_blob_names
        );
    }

    #[tokio::test]
    async fn test_delete_file_zero_byte_node_only_deletes_node_row() {
        let temp = tempdir().expect("tempdir should be created");
        let db_path = temp.path().join("manifest.db");
        let staging_directory = temp.path().join("staging");
        let store = create_store(&db_path).await;
        staging::ensure_staging_directory(&staging_directory)
            .await
            .expect("staging directory should be ensured");
        let node_id = Uuid::new_v4();
        store
            .insert_node(&zero_byte_node(node_id))
            .await
            .expect("zero-byte node should insert");

        delete_file(node_id, &store, &staging_directory)
            .await
            .expect("delete_file should succeed");

        assert!(matches!(
            store.get_node(node_id).await,
            Err(StorageError::NotFound)
        ));
    }

    #[tokio::test]
    async fn test_delete_file_missing_local_blob_still_succeeds() {
        let temp = tempdir().expect("tempdir should be created");
        let db_path = temp.path().join("manifest.db");
        let source_path = temp.path().join("source.bin");
        let staging_directory = temp.path().join("staging");
        let store = create_store(&db_path).await;
        staging::ensure_staging_directory(&staging_directory)
            .await
            .expect("staging directory should be ensured");
        fs::write(&source_path, b"single chunk payload")
            .await
            .expect("source file should be written");
        let node_id = Uuid::new_v4();

        upload_file(
            &source_path,
            node_id,
            None,
            "payload.bin",
            1,
            1,
            &store,
            &KeyEncryptionKey::from_bytes([2; 32]),
            &staging_directory,
            None,
        )
        .await
        .expect("upload should succeed");
        let chunks = store
            .get_chunks(node_id)
            .await
            .expect("chunks should be persisted");
        let first_blob_path = staging_directory.join(format!("{}.blob", chunks[0].blob_name));
        fs::remove_file(first_blob_path)
            .await
            .expect("blob should be removed before orchestrator call");

        delete_file(node_id, &store, &staging_directory)
            .await
            .expect("delete_file should still succeed");

        assert!(matches!(
            store.get_node(node_id).await,
            Err(StorageError::NotFound)
        ));
    }

    #[tokio::test]
    async fn test_delete_file_non_not_found_local_blob_delete_error_is_non_fatal() {
        let temp = tempdir().expect("tempdir should be created");
        let db_path = temp.path().join("manifest.db");
        let source_path = temp.path().join("source.bin");
        let staging_directory = temp.path().join("staging");
        let store = create_store(&db_path).await;
        staging::ensure_staging_directory(&staging_directory)
            .await
            .expect("staging directory should be ensured");
        fs::write(&source_path, b"single chunk payload")
            .await
            .expect("source file should be written");
        let node_id = Uuid::new_v4();

        upload_file(
            &source_path,
            node_id,
            None,
            "payload.bin",
            1,
            1,
            &store,
            &KeyEncryptionKey::from_bytes([4; 32]),
            &staging_directory,
            None,
        )
        .await
        .expect("upload should succeed");
        let chunks = store
            .get_chunks(node_id)
            .await
            .expect("chunks should be persisted");
        let first_blob_path = staging_directory.join(format!("{}.blob", chunks[0].blob_name));
        fs::remove_file(&first_blob_path)
            .await
            .expect("blob file should be removed");
        fs::create_dir(&first_blob_path)
            .await
            .expect("directory should be created at blob path");

        let result = delete_file(node_id, &store, &staging_directory).await;

        assert!(result.is_ok());
        assert!(matches!(
            store.get_node(node_id).await,
            Err(StorageError::NotFound)
        ));
        let pending = store
            .list_pending_deletions(10)
            .await
            .expect("pending deletions should load");
        assert_eq!(pending.len(), chunks.len());
    }

    #[tokio::test]
    async fn test_delete_file_rejects_invalid_blob_name_before_path_join() {
        let temp = tempdir().expect("tempdir should be created");
        let staging_directory = temp.path().join("staging");
        staging::ensure_staging_directory(&staging_directory)
            .await
            .expect("staging directory should be ensured");
        let node_id = Uuid::new_v4();
        let store = BadChunkStore {
            node: zero_byte_node(node_id),
            chunk: ChunkRecord {
                chunk_id: Uuid::new_v4(),
                node_id: NodeId::new(node_id),
                chunk_index: 0,
                blob_name: "not-a-canonical-uuid-v4".to_owned(),
                size_padded: 4096,
                blake3_checksum: [0; 32],
                epoch_blob_id: None,
                byte_offset: None,
                byte_length: None,
            },
            delete_called: Arc::new(AtomicBool::new(false)),
        };

        let result = delete_file(node_id, &store, &staging_directory).await;

        assert!(matches!(
            result,
            Err(StorageError::ConstraintViolation(message))
                if message == "invalid blob_name: expected canonical UUID v4"
        ));
        assert!(!store.delete_called.load(Ordering::Relaxed));
    }

    #[tokio::test]
    async fn test_delete_file_rejects_directory_target_without_modifying_subtree_or_blobs() {
        let temp = tempdir().expect("tempdir should be created");
        let db_path = temp.path().join("manifest.db");
        let source_path = temp.path().join("source.bin");
        let staging_directory = temp.path().join("staging");
        let store = create_store(&db_path).await;
        staging::ensure_staging_directory(&staging_directory)
            .await
            .expect("staging directory should be ensured");
        let directory_id = Uuid::new_v4();
        let child_file_id = Uuid::new_v4();
        store
            .insert_node(&directory_node(directory_id))
            .await
            .expect("directory node should insert");
        fs::write(&source_path, b"child payload")
            .await
            .expect("source file should be written");
        upload_file(
            &source_path,
            child_file_id,
            Some(directory_id),
            "child.bin",
            1,
            1,
            &store,
            &KeyEncryptionKey::from_bytes([3; 32]),
            &staging_directory,
            None,
        )
        .await
        .expect("child upload should succeed");
        let child_chunks_before = store
            .get_chunks(child_file_id)
            .await
            .expect("child chunks should load");
        let child_blob_paths = child_chunks_before
            .iter()
            .map(|chunk| staging_directory.join(format!("{}.blob", chunk.blob_name)))
            .collect::<Vec<_>>();
        let pending_before = store
            .list_pending_deletions(10)
            .await
            .expect("pending deletions should load");
        assert!(pending_before.is_empty());

        let delete_result = delete_file(directory_id, &store, &staging_directory).await;

        assert!(matches!(
            delete_result,
            Err(StorageError::ConstraintViolation(message)) if message == "target is a directory"
        ));
        let directory_after = store
            .get_node(directory_id)
            .await
            .expect("directory should remain");
        assert_eq!(directory_after.node_type, NodeType::Directory);
        let children_after = store
            .list_children(directory_id)
            .await
            .expect("children should load");
        assert_eq!(children_after.len(), 1);
        assert_eq!(*children_after[0].node_id.as_uuid(), child_file_id);
        let child_chunks_after = store
            .get_chunks(child_file_id)
            .await
            .expect("child chunks should remain");
        assert_eq!(child_chunks_after, child_chunks_before);
        for blob_path in child_blob_paths {
            assert!(blob_path.exists());
        }
        let pending_after = store
            .list_pending_deletions(10)
            .await
            .expect("pending deletions should load");
        assert!(pending_after.is_empty());
    }
}
