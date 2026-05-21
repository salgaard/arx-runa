//! Re-encryption of staged file blobs with a fresh per-file key.

use std::path::{Path, PathBuf};

use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncWriteExt, BufReader, BufWriter};
use uuid::Uuid;

use crate::crypto::{
    Blake3Hash, ChunkIndex, FileId, FileKey, KeyEncryptionKey, WrappedFileKey, compute_checksum,
    decrypt_chunk, encrypt_chunk, generate_file_key, unwrap_file_key, verify_checksum,
    wrap_file_key,
};

use crate::storage::MetadataStore;
use crate::storage::SqlCipherMetadataStore;
use crate::storage::error::StorageError;
use crate::storage::types::{ChunkRecord, NodeId};

/// Re-encrypts a file in the manifest: generates a fresh file key, decrypts
/// each chunk blob from staging into plaintext, encrypts with the new key,
/// writes new blobs to staging, and atomically replaces the manifest rows.
///
/// Returns the new `ChunkRecord` list on success.
///
/// ## Invariants
/// - Never writes unencrypted data to disk.
/// - Staging blobs are named `<uuid>.blob` (UUID v4).
/// - Old blobs are queued for deletion via `pending_deletions` atomically with
///   the manifest update.
/// - `extra_pending_deletions` (e.g. old shared-path cloud blobs from a strong
///   revocation) are also enqueued inside the same atomic transaction.
/// - Does not import anything from `sharing::`.
pub async fn reencrypt_file(
    file_id: Uuid,
    now_unix_seconds: i64,
    sqlcipher_store: &SqlCipherMetadataStore,
    key_encryption_key: &KeyEncryptionKey,
    staging_directory: &Path,
    extra_pending_deletions: &[String],
) -> Result<Vec<ChunkRecord>, StorageError> {
    let node = sqlcipher_store.get_node(file_id).await?;
    let mut old_chunks = sqlcipher_store.get_chunks(file_id).await?;

    let wrapped_bytes = node
        .file_key_wrapped
        .ok_or_else(|| StorageError::Database("file node has no wrapped file key".to_owned()))?;
    let file_id_crypto = FileId::from_uuid(file_id);
    let old_file_key: FileKey = unwrap_file_key(
        &WrappedFileKey::new(wrapped_bytes),
        &file_id_crypto,
        key_encryption_key,
    )
    .map_err(StorageError::from)?;

    let new_file_key = generate_file_key();
    let new_file_key_wrapped = wrap_file_key(&new_file_key, &file_id_crypto, key_encryption_key)
        .map_err(StorageError::from)?;
    old_chunks.sort_by_key(|c| c.chunk_index);

    let mut new_chunks: Vec<ChunkRecord> = Vec::with_capacity(old_chunks.len());
    let mut new_staged_paths: Vec<PathBuf> = Vec::new();

    for chunk in &old_chunks {
        match re_encrypt_single_chunk(
            chunk,
            &old_file_key,
            &new_file_key,
            &file_id_crypto,
            file_id,
            staging_directory,
        )
        .await
        {
            Ok((new_chunk, new_path)) => {
                new_chunks.push(new_chunk);
                new_staged_paths.push(new_path);
            }
            Err(error) => {
                remove_staged_blobs(&new_staged_paths).await;
                return Err(error);
            }
        }
    }

    if let Err(error) = sqlcipher_store
        .replace_file_key_and_chunks(
            file_id,
            *new_file_key_wrapped.as_bytes(),
            new_chunks.clone(),
            now_unix_seconds,
            extra_pending_deletions,
        )
        .await
    {
        remove_staged_blobs(&new_staged_paths).await;
        return Err(error);
    }

    Ok(new_chunks)
}

/// Reads, verifies, decrypts, and re-encrypts one chunk blob, writing the result to staging.
///
/// Returns the new [`ChunkRecord`] and the on-disk path of the written staging blob.
async fn re_encrypt_single_chunk(
    chunk: &ChunkRecord,
    old_file_key: &FileKey,
    new_file_key: &FileKey,
    file_id_crypto: &FileId,
    file_id: Uuid,
    staging_directory: &Path,
) -> Result<(ChunkRecord, PathBuf), StorageError> {
    let source_path = staging_directory.join(format!("{}.blob", chunk.blob_name));
    let encrypted_bytes = read_blob_bytes(&source_path).await?;

    let expected_checksum = Blake3Hash(chunk.blake3_checksum);
    let verified =
        verify_checksum(encrypted_bytes, &expected_checksum).map_err(StorageError::from)?;

    let plaintext = decrypt_chunk(
        verified,
        old_file_key,
        file_id_crypto,
        ChunkIndex::new(chunk.chunk_index),
    )
    .map_err(StorageError::from)?;

    let new_wire_blob = encrypt_chunk(
        plaintext,
        new_file_key,
        file_id_crypto,
        ChunkIndex::new(chunk.chunk_index),
    )
    .map_err(StorageError::from)?;

    let new_checksum = compute_checksum(&new_wire_blob);
    let new_blob_name = Uuid::new_v4().hyphenated().to_string();
    let new_blob_path = staging_directory.join(format!("{}.blob", new_blob_name));

    write_blob_to_disk(&new_blob_path, &new_wire_blob).await?;

    let new_chunk = ChunkRecord {
        chunk_id: Uuid::new_v4(),
        node_id: NodeId::new(file_id),
        chunk_index: chunk.chunk_index,
        blob_name: new_blob_name,
        size_padded: chunk.size_padded,
        blake3_checksum: new_checksum.0,
        epoch_blob_id: None,
        byte_offset: None,
        byte_length: None,
    };

    Ok((new_chunk, new_blob_path))
}

/// Reads a blob file at `path` into a `Vec<u8>`.
async fn read_blob_bytes(path: &Path) -> Result<Vec<u8>, StorageError> {
    let file = File::open(path)
        .await
        .map_err(|error| StorageError::Io(error.to_string()))?;
    let mut reader = BufReader::new(file);
    let mut bytes = Vec::new();
    reader
        .read_to_end(&mut bytes)
        .await
        .map_err(|error| StorageError::Io(error.to_string()))?;
    Ok(bytes)
}

/// Writes `bytes` to `path`, flushing the buffer before returning.
async fn write_blob_to_disk(path: &Path, bytes: &[u8]) -> Result<(), StorageError> {
    let file = File::create(path)
        .await
        .map_err(|error| StorageError::Io(error.to_string()))?;
    let mut writer = BufWriter::new(file);
    writer
        .write_all(bytes)
        .await
        .map_err(|error| StorageError::Io(error.to_string()))?;
    writer
        .flush()
        .await
        .map_err(|error| StorageError::Io(error.to_string()))?;
    Ok(())
}

/// Removes all blob paths in `paths` on a best-effort basis, ignoring individual errors.
async fn remove_staged_blobs(paths: &[PathBuf]) {
    for path in paths {
        let _ = tokio::fs::remove_file(path).await;
    }
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;
    use uuid::Uuid;
    use zeroize::Zeroizing;

    use super::reencrypt_file;
    use crate::crypto::{
        ChunkIndex, FileId, KeyEncryptionKey, compute_checksum, encrypt_chunk, generate_file_key,
        wrap_file_key,
    };
    use crate::storage::MetadataStore;
    use crate::storage::SqlCipherMetadataStore;
    use crate::storage::error::StorageError;
    use crate::storage::types::{ChunkRecord, Node, NodeId, NodeType};

    /// Verifies that `reencrypt_file` replaces chunk blobs and updates the manifest atomically.
    #[tokio::test]
    async fn test_reencrypt_file_produces_different_chunks_and_updates_manifest() {
        let temp = TempDir::new().expect("tempdir should be created");
        let db_path = temp.path().join("manifest.db");
        let store =
            SqlCipherMetadataStore::create(&db_path, &[1u8; 32], Uuid::new_v4(), 4_194_304, false)
                .await
                .expect("store should be created");

        let staging_dir = temp.path().join("staging");
        tokio::fs::create_dir_all(&staging_dir)
            .await
            .expect("staging dir should be created");

        let kek = KeyEncryptionKey::from_bytes([42u8; 32]);
        let file_id = Uuid::new_v4();
        let file_key = generate_file_key();
        let file_id_crypto = FileId::from_uuid(file_id);
        let wrapped = wrap_file_key(&file_key, &file_id_crypto, &kek).expect("wrap should succeed");

        let blob_0 = encrypt_chunk(
            Zeroizing::new(vec![0xAAu8; 64]),
            &file_key,
            &file_id_crypto,
            ChunkIndex::new(0),
        )
        .expect("encrypt chunk 0 should succeed");
        let blob_1 = encrypt_chunk(
            Zeroizing::new(vec![0xBBu8; 64]),
            &file_key,
            &file_id_crypto,
            ChunkIndex::new(1),
        )
        .expect("encrypt chunk 1 should succeed");

        let checksum_0 = compute_checksum(&blob_0);
        let checksum_1 = compute_checksum(&blob_1);

        let blob_name_0 = Uuid::new_v4().hyphenated().to_string();
        let blob_name_1 = Uuid::new_v4().hyphenated().to_string();

        tokio::fs::write(staging_dir.join(format!("{}.blob", blob_name_0)), &blob_0)
            .await
            .expect("blob 0 should write to staging");
        tokio::fs::write(staging_dir.join(format!("{}.blob", blob_name_1)), &blob_1)
            .await
            .expect("blob 1 should write to staging");

        let node = Node::new(
            file_id,
            None,
            NodeType::File,
            "test.bin".to_owned(),
            1,
            1,
            128,
            Some(*wrapped.as_bytes()),
        );
        let initial_chunks = vec![
            ChunkRecord {
                chunk_id: Uuid::new_v4(),
                node_id: NodeId::new(file_id),
                chunk_index: 0,
                blob_name: blob_name_0.clone(),
                size_padded: 4_194_304,
                blake3_checksum: checksum_0.0,
                epoch_blob_id: None,
                byte_offset: None,
                byte_length: None,
            },
            ChunkRecord {
                chunk_id: Uuid::new_v4(),
                node_id: NodeId::new(file_id),
                chunk_index: 1,
                blob_name: blob_name_1.clone(),
                size_padded: 4_194_304,
                blake3_checksum: checksum_1.0,
                epoch_blob_id: None,
                byte_offset: None,
                byte_length: None,
            },
        ];
        store
            .insert_file_with_chunks(&node, &initial_chunks)
            .await
            .expect("file with chunks should insert");

        let new_chunks = reencrypt_file(file_id, 99_999, &store, &kek, &staging_dir, &[])
            .await
            .expect("reencrypt_file should succeed");

        assert_eq!(new_chunks.len(), 2);
        assert_ne!(new_chunks[0].blob_name, blob_name_0);
        assert_ne!(new_chunks[1].blob_name, blob_name_1);
        assert_ne!(new_chunks[0].blake3_checksum, checksum_0.0);
        assert_ne!(new_chunks[1].blake3_checksum, checksum_1.0);

        let pending = store
            .list_pending_deletions(10)
            .await
            .expect("pending_deletions should load");
        assert!(pending.contains(&blob_name_0));
        assert!(pending.contains(&blob_name_1));

        assert!(
            staging_dir
                .join(format!("{}.blob", new_chunks[0].blob_name))
                .exists()
        );
        assert!(
            staging_dir
                .join(format!("{}.blob", new_chunks[1].blob_name))
                .exists()
        );

        let updated_node = store.get_node(file_id).await.expect("node should load");
        assert_ne!(updated_node.file_key_wrapped, Some(*wrapped.as_bytes()));
    }

    /// Verifies that `reencrypt_file` returns `NotFound` when the file node does not exist.
    #[tokio::test]
    async fn test_reencrypt_file_on_missing_file_returns_not_found() {
        let temp = TempDir::new().expect("tempdir should be created");
        let db_path = temp.path().join("manifest.db");
        let store =
            SqlCipherMetadataStore::create(&db_path, &[1u8; 32], Uuid::new_v4(), 4_194_304, false)
                .await
                .expect("store should be created");

        let staging_dir = temp.path().join("staging");
        tokio::fs::create_dir_all(&staging_dir)
            .await
            .expect("staging dir should be created");

        let kek = KeyEncryptionKey::from_bytes([42u8; 32]);

        let result = reencrypt_file(Uuid::new_v4(), 99_999, &store, &kek, &staging_dir, &[]).await;

        assert!(matches!(result, Err(StorageError::NotFound)));
    }
}
