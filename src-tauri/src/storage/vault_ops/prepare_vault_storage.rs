use std::path::Path;

use crate::storage::error::StorageError;
use crate::storage::sqlcipher::SqlCipherMetadataStore;
use crate::storage::staging;

/// Prepares local storage for vault operations.
///
/// The staging directory is ensured first, then orphaned staged blobs are
/// cleaned up against manifest metadata.
pub async fn prepare_vault_storage(
    store: &SqlCipherMetadataStore,
    staging_directory: &Path,
) -> Result<usize, StorageError> {
    staging::ensure_staging_directory(staging_directory).await?;
    staging::migrate_flat_staging_blobs(staging_directory).await?;
    staging::cleanup_orphaned_blobs(staging_directory, store).await
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;
    use tokio::fs;
    use uuid::Uuid;

    use super::prepare_vault_storage;
    use crate::crypto::{KeyEncryptionKey, generate_file_key};
    use crate::storage::MetadataStore;
    use crate::storage::sqlcipher::SqlCipherMetadataStore;
    use crate::storage::vault_ops::{delete_file, upload_file};
    use crate::storage::{encrypt_file, staging};

    async fn create_store(db_path: &std::path::Path) -> SqlCipherMetadataStore {
        SqlCipherMetadataStore::create(db_path, &[5; 32], Uuid::new_v4(), 4_194_304, false)
            .await
            .expect("store should be created")
    }

    #[tokio::test]
    async fn test_prepare_vault_storage_creates_missing_staging_directory_and_runs_cleanup() {
        let temp = tempdir().expect("tempdir should be created");
        let db_path = temp.path().join("manifest.db");
        let staging_directory = temp.path().join("missing").join("staging");
        let orphan_blob_name = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
        let orphan_blob_path = staging_directory.join(format!("{orphan_blob_name}.blob"));
        let store = create_store(&db_path).await;

        let first_count = prepare_vault_storage(&store, &staging_directory)
            .await
            .expect("prepare should create staging directory");
        tokio::fs::write(&orphan_blob_path, b"orphan")
            .await
            .expect("orphan should be staged");
        let second_count = prepare_vault_storage(&store, &staging_directory)
            .await
            .expect("prepare should cleanup orphan");

        assert_eq!(first_count, 0);
        assert_eq!(second_count, 1);
        assert!(staging_directory.exists());
        assert!(!orphan_blob_path.exists());
    }

    #[tokio::test]
    async fn test_crash_after_encrypt_before_commit_cleanup_removes_orphaned_blobs() {
        let temp = tempdir().expect("tempdir should be created");
        let db_path = temp.path().join("manifest.db");
        let source_path = temp.path().join("source.bin");
        let staging_directory = temp.path().join("staging");
        let store = create_store(&db_path).await;
        staging::ensure_staging_directory(&staging_directory)
            .await
            .expect("staging directory should be created");
        fs::write(&source_path, vec![0x51u8; 4_194_304 + 128])
            .await
            .expect("source should be written");
        let file_key = generate_file_key();
        let node_id = Uuid::new_v4();

        let chunks = encrypt_file(
            &source_path,
            node_id,
            &file_key,
            &store,
            &staging_directory,
            None,
        )
        .await
        .expect("encryption should stage blobs");
        let orphan_count = prepare_vault_storage(&store, &staging_directory)
            .await
            .expect("prepare should cleanup orphaned blobs");

        assert_eq!(orphan_count, chunks.len());
        for chunk in chunks {
            let blob_path = staging_directory.join(format!("{}.blob", chunk.blob_name));
            assert!(!blob_path.exists());
        }
    }

    #[tokio::test]
    async fn test_crash_after_commit_before_blob_delete_orphan_scan_noop() {
        let temp = tempdir().expect("tempdir should be created");
        let db_path = temp.path().join("manifest.db");
        let source_path = temp.path().join("source.bin");
        let staging_directory = temp.path().join("staging");
        let store = create_store(&db_path).await;
        staging::ensure_staging_directory(&staging_directory)
            .await
            .expect("staging directory should be created");
        fs::write(&source_path, b"committed payload")
            .await
            .expect("source should be written");
        let node_id = Uuid::new_v4();

        upload_file(
            &source_path,
            node_id,
            None,
            "committed.bin",
            1,
            1,
            &store,
            &KeyEncryptionKey::from_bytes([7; 32]),
            &staging_directory,
            None,
        )
        .await
        .expect("upload should succeed");
        let chunks = store.get_chunks(node_id).await.expect("chunks should load");

        let orphan_count = prepare_vault_storage(&store, &staging_directory)
            .await
            .expect("prepare should run");

        assert_eq!(orphan_count, 0);
        for chunk in &chunks {
            let blob_path = staging_directory
                .join("pending")
                .join(format!("{}.blob", chunk.blob_name));
            assert!(blob_path.exists());
        }

        delete_file(node_id, &store, &staging_directory)
            .await
            .expect("delete_file should succeed");
        let pending_deletions = store
            .list_pending_deletions(10)
            .await
            .expect("pending deletions should load");
        assert_eq!(pending_deletions.len(), chunks.len());
    }
}
