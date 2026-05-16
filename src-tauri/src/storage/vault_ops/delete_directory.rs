use std::path::Path;

use uuid::Uuid;

use crate::storage::MetadataStore;
use crate::storage::error::StorageError;
use crate::storage::types::NodeType;

/// Recursively deletes a directory node and all of its descendants.
///
/// Files in the subtree are deleted via [`super::delete_file`], which removes
/// both the metadata node and any local staged blobs. Directory nodes have no
/// blobs and are removed with `delete_node` directly. The root directory node
/// itself is deleted last.
///
/// Returns `ConstraintViolation` if `node_id` refers to a file rather than a
/// directory.
pub async fn delete_directory(
    node_id: Uuid,
    metadata_store: &dyn MetadataStore,
    staging_directory: &Path,
) -> Result<(), StorageError> {
    let node = metadata_store.get_node(node_id).await?;
    if node.node_type != NodeType::Directory {
        return Err(StorageError::ConstraintViolation(
            "target is not a directory".to_owned(),
        ));
    }
    delete_subtree(node_id, metadata_store, staging_directory).await
}

/// Deletes all descendants of `node_id` recursively, then deletes `node_id`.
async fn delete_subtree(
    node_id: Uuid,
    metadata_store: &dyn MetadataStore,
    staging_directory: &Path,
) -> Result<(), StorageError> {
    let children = metadata_store.list_children(node_id).await?;
    for child in children {
        let child_uuid = *child.node_id.as_uuid();
        if child.node_type == NodeType::Directory {
            Box::pin(delete_subtree(
                child_uuid,
                metadata_store,
                staging_directory,
            ))
            .await?;
        } else {
            super::delete_file(child_uuid, metadata_store, staging_directory).await?;
        }
    }
    metadata_store.delete_node(node_id).await
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;
    use tokio::fs;
    use uuid::Uuid;

    use super::delete_directory;
    use crate::crypto::KeyEncryptionKey;
    use crate::storage::error::StorageError;
    use crate::storage::types::{Node, NodeType};
    use crate::storage::vault_ops::upload_file;
    use crate::storage::{MetadataStore, SqlCipherMetadataStore, staging};

    async fn create_store(db_path: &std::path::Path) -> SqlCipherMetadataStore {
        SqlCipherMetadataStore::create(db_path, &[5; 32], Uuid::new_v4(), 4_194_304, false)
            .await
            .expect("store should be created")
    }

    fn dir_node(node_id: Uuid, parent_id: Option<Uuid>, name: &str) -> Node {
        Node::new(
            node_id,
            parent_id,
            NodeType::Directory,
            name.to_owned(),
            1,
            1,
            0,
            None,
        )
    }

    #[tokio::test]
    async fn test_delete_directory_rejects_file_target_with_constraint_violation() {
        let temp = tempdir().expect("tempdir should be created");
        let db_path = temp.path().join("manifest.db");
        let source_path = temp.path().join("source.bin");
        let staging_directory = temp.path().join("staging");
        let store = create_store(&db_path).await;
        staging::ensure_staging_directory(&staging_directory)
            .await
            .expect("staging directory should be ensured");
        fs::write(&source_path, b"payload")
            .await
            .expect("source file should be written");
        let file_id = Uuid::new_v4();
        upload_file(
            &source_path,
            file_id,
            None,
            "file.txt",
            1,
            1,
            &store,
            &KeyEncryptionKey::from_bytes([1; 32]),
            &staging_directory,
            None,
        )
        .await
        .expect("upload should succeed");

        let result = delete_directory(file_id, &store, &staging_directory).await;

        assert!(matches!(
            result,
            Err(StorageError::ConstraintViolation(ref msg)) if msg == "target is not a directory"
        ));
        store
            .get_node(file_id)
            .await
            .expect("file node should still exist");
    }

    #[tokio::test]
    async fn test_delete_directory_empty_directory_removes_node() {
        let temp = tempdir().expect("tempdir should be created");
        let db_path = temp.path().join("manifest.db");
        let staging_directory = temp.path().join("staging");
        let store = create_store(&db_path).await;
        staging::ensure_staging_directory(&staging_directory)
            .await
            .expect("staging directory should be ensured");
        let dir_id = Uuid::new_v4();
        store
            .insert_node(&dir_node(dir_id, None, "empty_dir"))
            .await
            .expect("directory node should insert");

        delete_directory(dir_id, &store, &staging_directory)
            .await
            .expect("delete_directory should succeed");

        assert!(matches!(
            store.get_node(dir_id).await,
            Err(StorageError::NotFound)
        ));
    }

    #[tokio::test]
    async fn test_delete_directory_removes_child_files_and_their_blobs() {
        let temp = tempdir().expect("tempdir should be created");
        let db_path = temp.path().join("manifest.db");
        let source_path = temp.path().join("source.bin");
        let staging_directory = temp.path().join("staging");
        let store = create_store(&db_path).await;
        staging::ensure_staging_directory(&staging_directory)
            .await
            .expect("staging directory should be ensured");
        fs::write(&source_path, b"child file payload")
            .await
            .expect("source file should be written");
        let dir_id = Uuid::new_v4();
        let child_id = Uuid::new_v4();
        store
            .insert_node(&dir_node(dir_id, None, "parent_dir"))
            .await
            .expect("directory node should insert");
        upload_file(
            &source_path,
            child_id,
            Some(dir_id),
            "child.txt",
            1,
            1,
            &store,
            &KeyEncryptionKey::from_bytes([2; 32]),
            &staging_directory,
            None,
        )
        .await
        .expect("child upload should succeed");
        let child_chunks = store
            .get_chunks(child_id)
            .await
            .expect("child chunks should load");
        let blob_paths: Vec<_> = child_chunks
            .iter()
            .map(|c| staging_directory.join(format!("{}.blob", c.blob_name)))
            .collect();

        delete_directory(dir_id, &store, &staging_directory)
            .await
            .expect("delete_directory should succeed");

        assert!(matches!(
            store.get_node(dir_id).await,
            Err(StorageError::NotFound)
        ));
        assert!(matches!(
            store.get_node(child_id).await,
            Err(StorageError::NotFound)
        ));
        for blob_path in blob_paths {
            assert!(!blob_path.exists(), "blob should be removed: {blob_path:?}");
        }
    }

    #[tokio::test]
    async fn test_delete_directory_removes_nested_subdirectory_tree() {
        let temp = tempdir().expect("tempdir should be created");
        let db_path = temp.path().join("manifest.db");
        let source_path = temp.path().join("source.bin");
        let staging_directory = temp.path().join("staging");
        let store = create_store(&db_path).await;
        staging::ensure_staging_directory(&staging_directory)
            .await
            .expect("staging directory should be ensured");
        fs::write(&source_path, b"deeply nested file")
            .await
            .expect("source file should be written");
        let parent_dir_id = Uuid::new_v4();
        let child_dir_id = Uuid::new_v4();
        let grandchild_file_id = Uuid::new_v4();
        store
            .insert_node(&dir_node(parent_dir_id, None, "parent"))
            .await
            .expect("parent directory should insert");
        store
            .insert_node(&dir_node(child_dir_id, Some(parent_dir_id), "child"))
            .await
            .expect("child directory should insert");
        upload_file(
            &source_path,
            grandchild_file_id,
            Some(child_dir_id),
            "file.txt",
            1,
            1,
            &store,
            &KeyEncryptionKey::from_bytes([3; 32]),
            &staging_directory,
            None,
        )
        .await
        .expect("grandchild upload should succeed");
        let blob_paths: Vec<_> = store
            .get_chunks(grandchild_file_id)
            .await
            .expect("chunks should load")
            .iter()
            .map(|c| staging_directory.join(format!("{}.blob", c.blob_name)))
            .collect();

        delete_directory(parent_dir_id, &store, &staging_directory)
            .await
            .expect("delete_directory should succeed");

        assert!(matches!(
            store.get_node(parent_dir_id).await,
            Err(StorageError::NotFound)
        ));
        assert!(matches!(
            store.get_node(child_dir_id).await,
            Err(StorageError::NotFound)
        ));
        assert!(matches!(
            store.get_node(grandchild_file_id).await,
            Err(StorageError::NotFound)
        ));
        for blob_path in blob_paths {
            assert!(!blob_path.exists(), "blob should be removed: {blob_path:?}");
        }
    }
}
