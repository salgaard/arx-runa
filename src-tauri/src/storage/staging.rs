//! Staging-directory helpers and orphaned-blob cleanup.

use std::fs::OpenOptions;
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};

use uuid::Uuid;

use crate::storage::error::StorageError;
use crate::storage::sqlcipher::SqlCipherMetadataStore;

/// Resolves the default staging-directory path for the current platform.
pub fn default_staging_directory() -> Result<PathBuf, StorageError> {
    let base = dirs::data_dir()
        .ok_or_else(|| StorageError::Io("data directory unavailable".to_owned()))?;
    Ok(base.join("arx-runa").join("staging"))
}

/// Creates the staging directory if it does not already exist.
pub async fn ensure_staging_directory(path: &Path) -> Result<(), StorageError> {
    tokio::fs::create_dir_all(path)
        .await
        .map_err(|error| StorageError::Io(format!("{}: {error}", path.display())))
}

/// Writes bytes to `path` with owner-only permissions where supported.
pub(crate) async fn write_owner_only(path: &Path, bytes: &[u8]) -> Result<(), StorageError> {
    let path = path.to_path_buf();
    let payload = bytes.to_vec();
    tokio::task::spawn_blocking(move || -> Result<(), StorageError> {
        let mut options = OpenOptions::new();
        options.create(true).write(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }

        let mut file = options
            .open(&path)
            .map_err(|error| StorageError::Io(format!("{}: {error}", path.display())))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            file.set_permissions(std::fs::Permissions::from_mode(0o600))
                .map_err(|error| StorageError::Io(format!("{}: {error}", path.display())))?;
        }
        file.write_all(&payload)
            .map_err(|error| StorageError::Io(format!("{}: {error}", path.display())))?;
        file.sync_all()
            .map_err(|error| StorageError::Io(format!("{}: {error}", path.display())))?;
        Ok(())
    })
    .await
    .map_err(|error| StorageError::Database(error.to_string()))?
}

/// Deletes orphaned staged blobs that are not referenced by manifest chunk rows.
///
/// A staged file is deleted only when all conditions match:
/// - file extension is `.blob`,
/// - file stem parses to a UUID v4 value,
/// - UUID stem is absent from the manifest `chunks.blob_name` set.
pub async fn cleanup_orphaned_blobs(
    staging_directory: &Path,
    sqlcipher_store: &SqlCipherMetadataStore,
) -> Result<usize, StorageError> {
    let known_blob_names = sqlcipher_store.list_all_blob_names().await?;
    let mut deleted_count = 0usize;
    let mut directory_entries = tokio::fs::read_dir(staging_directory)
        .await
        .map_err(|error| StorageError::Io(format!("{}: {error}", staging_directory.display())))?;

    while let Some(entry) = directory_entries
        .next_entry()
        .await
        .map_err(|error| StorageError::Io(format!("{}: {error}", staging_directory.display())))?
    {
        let path = entry.path();
        let file_type = entry
            .file_type()
            .await
            .map_err(|error| StorageError::Io(format!("{}: {error}", path.display())))?;
        if !file_type.is_file() {
            continue;
        }

        if path.extension().and_then(|extension| extension.to_str()) != Some("blob") {
            continue;
        }

        let Some(stem_text) = path.file_stem().and_then(|stem| stem.to_str()) else {
            continue;
        };
        let Ok(parsed_uuid) = Uuid::parse_str(stem_text) else {
            continue;
        };
        if parsed_uuid.get_version_num() != 4 {
            continue;
        }

        let normalized_blob_name = parsed_uuid.hyphenated().to_string();
        if known_blob_names.contains(&normalized_blob_name) {
            continue;
        }

        match tokio::fs::remove_file(&path).await {
            Ok(()) => {
                deleted_count += 1;
            }
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(error) => {
                return Err(StorageError::Io(format!("{}: {error}", path.display())));
            }
        }
    }

    Ok(deleted_count)
}

#[cfg(test)]
mod tests {
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    use tempfile::tempdir;
    use uuid::Uuid;

    use super::{cleanup_orphaned_blobs, ensure_staging_directory, write_owner_only};
    use crate::storage::MetadataStore;
    use crate::storage::NodeId;
    use crate::storage::sqlcipher::SqlCipherMetadataStore;
    use crate::storage::types::{ChunkRecord, Node, NodeType};

    fn file_node(node_id: Uuid) -> Node {
        Node::new(
            node_id,
            None,
            NodeType::File,
            "file.bin".to_owned(),
            1,
            1,
            11,
            Some([7; 72]),
        )
    }

    fn chunk_for(node_id: Uuid, chunk_index: u32, blob_name: &str) -> ChunkRecord {
        ChunkRecord {
            chunk_id: Uuid::new_v4(),
            node_id: NodeId::new(node_id),
            chunk_index,
            blob_name: blob_name.to_owned(),
            size_padded: 4_194_304,
            blake3_checksum: [3; 32],
            epoch_blob_id: None,
            byte_offset: None,
            byte_length: None,
        }
    }

    #[tokio::test]
    async fn test_ensure_staging_directory_creates_missing_directory() {
        let temp = tempdir().expect("tempdir should be created");
        let staging_directory = temp.path().join("missing").join("staging");

        ensure_staging_directory(&staging_directory)
            .await
            .expect("staging directory should be created");

        assert!(staging_directory.exists());
    }

    #[tokio::test]
    async fn test_ensure_staging_directory_is_idempotent_when_present() {
        let temp = tempdir().expect("tempdir should be created");
        let staging_directory = temp.path().join("staging");
        tokio::fs::create_dir_all(&staging_directory)
            .await
            .expect("staging directory should be created in setup");

        ensure_staging_directory(&staging_directory)
            .await
            .expect("ensure should succeed on existing directory");
        ensure_staging_directory(&staging_directory)
            .await
            .expect("ensure should stay idempotent");

        assert!(staging_directory.exists());
    }

    #[tokio::test]
    async fn test_cleanup_orphaned_blobs_removes_untracked_blob_and_returns_count() {
        let temp = tempdir().expect("tempdir should be created");
        let staging_directory = temp.path().join("staging");
        let db_path = temp.path().join("manifest.db");
        let store =
            SqlCipherMetadataStore::create(&db_path, &[5; 32], Uuid::new_v4(), 4_194_304, false)
                .await
                .expect("store should be created");

        tokio::fs::create_dir_all(&staging_directory)
            .await
            .expect("staging directory should be created");

        let tracked_blob_name = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
        let orphan_blob_name = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb";
        let file_id = Uuid::new_v4();
        store
            .insert_file_with_chunks(
                &file_node(file_id),
                &[chunk_for(file_id, 0, tracked_blob_name)],
            )
            .await
            .expect("tracked manifest rows should insert");

        let tracked_blob_path = staging_directory.join(format!("{tracked_blob_name}.blob"));
        let orphan_blob_path = staging_directory.join(format!("{orphan_blob_name}.blob"));
        tokio::fs::write(&tracked_blob_path, b"tracked")
            .await
            .expect("tracked blob should be staged");
        tokio::fs::write(&orphan_blob_path, b"orphan")
            .await
            .expect("orphan blob should be staged");

        let deleted_count = cleanup_orphaned_blobs(&staging_directory, &store)
            .await
            .expect("cleanup should succeed");

        assert_eq!(deleted_count, 1);
        assert!(tracked_blob_path.exists());
        assert!(!orphan_blob_path.exists());
    }

    #[tokio::test]
    async fn test_cleanup_orphaned_blobs_preserves_referenced_blob_when_manifest_lists_it() {
        let temp = tempdir().expect("tempdir should be created");
        let staging_directory = temp.path().join("staging");
        let db_path = temp.path().join("manifest.db");
        let store =
            SqlCipherMetadataStore::create(&db_path, &[5; 32], Uuid::new_v4(), 4_194_304, false)
                .await
                .expect("store should be created");

        tokio::fs::create_dir_all(&staging_directory)
            .await
            .expect("staging directory should be created");

        let tracked_blob_name = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
        let file_id = Uuid::new_v4();
        store
            .insert_file_with_chunks(
                &file_node(file_id),
                &[chunk_for(file_id, 0, tracked_blob_name)],
            )
            .await
            .expect("tracked manifest rows should insert");

        let tracked_blob_path = staging_directory.join(format!("{tracked_blob_name}.blob"));
        tokio::fs::write(&tracked_blob_path, b"tracked")
            .await
            .expect("tracked blob should be staged");

        let deleted_count = cleanup_orphaned_blobs(&staging_directory, &store)
            .await
            .expect("cleanup should succeed");

        assert_eq!(deleted_count, 0);
        assert!(tracked_blob_path.exists());
    }

    #[tokio::test]
    async fn test_cleanup_orphaned_blobs_skips_non_blob_and_non_uuid_files() {
        let temp = tempdir().expect("tempdir should be created");
        let staging_directory = temp.path().join("staging");
        let db_path = temp.path().join("manifest.db");
        let store =
            SqlCipherMetadataStore::create(&db_path, &[5; 32], Uuid::new_v4(), 4_194_304, false)
                .await
                .expect("store should be created");

        tokio::fs::create_dir_all(&staging_directory)
            .await
            .expect("staging directory should be created");

        let readme_path = staging_directory.join("readme.txt");
        let invalid_uuid_blob_path = staging_directory.join("not-a-uuid.blob");
        let non_v4_blob_path = staging_directory.join("f81d4fae-7dec-11d0-a765-00a0c91e6bf6.blob");
        tokio::fs::write(&readme_path, b"text")
            .await
            .expect("readme should be written");
        tokio::fs::write(&invalid_uuid_blob_path, b"text")
            .await
            .expect("invalid uuid blob should be written");
        tokio::fs::write(&non_v4_blob_path, b"text")
            .await
            .expect("non-v4 blob should be written");

        let deleted_count = cleanup_orphaned_blobs(&staging_directory, &store)
            .await
            .expect("cleanup should succeed");

        assert_eq!(deleted_count, 0);
        assert!(readme_path.exists());
        assert!(invalid_uuid_blob_path.exists());
        assert!(non_v4_blob_path.exists());
    }

    #[tokio::test]
    async fn test_cleanup_orphaned_blobs_tolerates_concurrently_removed_file() {
        let temp = tempdir().expect("tempdir should be created");
        let staging_directory = temp.path().join("staging");
        let db_path = temp.path().join("manifest.db");
        let store =
            SqlCipherMetadataStore::create(&db_path, &[5; 32], Uuid::new_v4(), 4_194_304, false)
                .await
                .expect("store should be created");

        tokio::fs::create_dir_all(&staging_directory)
            .await
            .expect("staging directory should be created");

        let orphan_blob_name = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb";
        let orphan_blob_path = staging_directory.join(format!("{orphan_blob_name}.blob"));
        tokio::fs::write(&orphan_blob_path, b"orphan")
            .await
            .expect("orphan blob should be staged");
        tokio::fs::remove_file(&orphan_blob_path)
            .await
            .expect("orphan blob should be removed in setup");

        let result = cleanup_orphaned_blobs(&staging_directory, &store).await;

        assert!(matches!(result, Ok(0)));
    }

    #[tokio::test]
    async fn test_write_owner_only_writes_bytes() {
        let temp = tempdir().expect("tempdir should be created");
        let path = temp.path().join("owner-only.bin");
        write_owner_only(&path, b"payload")
            .await
            .expect("write should succeed");
        assert_eq!(std::fs::read(path).unwrap(), b"payload");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn test_write_owner_only_sets_0600_permissions_on_unix() {
        let temp = tempdir().expect("tempdir should be created");
        let path = temp.path().join("owner-only.bin");
        write_owner_only(&path, b"payload")
            .await
            .expect("write should succeed");
        let metadata = std::fs::metadata(path).expect("metadata should be readable");
        assert_eq!(metadata.permissions().mode() & 0o777, 0o600);
    }
}
