//! Epoch buffer flush: packs staged plaintexts into encrypted epoch blobs.

use std::path::Path;

use uuid::Uuid;
use zeroize::Zeroizing;

use crate::crypto::{
    ChunkIndex, FileId, FileKey, KeyEncryptionKey, compute_checksum, encrypt_chunk,
    generate_file_key, wrap_file_key,
};
use crate::storage::MetadataStore;
use crate::storage::error::StorageError;
use crate::storage::types::EpochBlobRecord;

/// Flushes all staged entries in the epoch buffer into one or more encrypted epoch blobs.
///
/// Each blob is at most `chunk_size_bytes` bytes of plaintext (zero-padded to exactly
/// `chunk_size_bytes`).  After encryption the blob is written to `staging_dir` and the
/// `epoch_blobs` / `chunks` rows are committed atomically.  The epoch buffer is cleared
/// as part of the commit.
///
/// The optional `progress` callback is invoked after each blob is flushed with
/// `(bytes_flushed, bytes_total)`.  Pass `None` to suppress progress reporting.
/// The callback MUST NOT import or depend on `tauri::`.
///
/// Returns `Ok(())` immediately when the buffer is empty.
pub async fn flush_epoch_buffer(
    metadata_store: &dyn MetadataStore,
    kek: &KeyEncryptionKey,
    staging_dir: &Path,
    chunk_size_bytes: u64,
    progress: Option<&(dyn Fn(u64, u64) + Send + Sync)>,
) -> Result<(), StorageError> {
    let entries = metadata_store.get_epoch_buffer_entries().await?;
    if entries.is_empty() {
        return Ok(());
    }

    let total_bytes: u64 = entries.iter().map(|e| e.size_bytes).sum();
    let mut flushed_bytes: u64 = 0;

    let mut current_blob_entries: Vec<(Uuid, Vec<u8>)> = Vec::new();
    let mut current_size: u64 = 0;

    for entry in &entries {
        if current_size + entry.size_bytes > chunk_size_bytes && !current_blob_entries.is_empty() {
            let blob_bytes: u64 = current_blob_entries
                .iter()
                .map(|(_, p)| p.len() as u64)
                .sum();
            flush_one_blob(
                &current_blob_entries,
                kek,
                staging_dir,
                chunk_size_bytes,
                metadata_store,
            )
            .await?;
            flushed_bytes += blob_bytes;
            if let Some(p) = progress {
                p(flushed_bytes, total_bytes);
            }
            current_blob_entries.clear();
            current_size = 0;
        }
        current_blob_entries.push((entry.node_id, entry.plaintext.clone()));
        current_size += entry.size_bytes;
    }

    if !current_blob_entries.is_empty() {
        let blob_bytes: u64 = current_blob_entries
            .iter()
            .map(|(_, p)| p.len() as u64)
            .sum();
        flush_one_blob(
            &current_blob_entries,
            kek,
            staging_dir,
            chunk_size_bytes,
            metadata_store,
        )
        .await?;
        flushed_bytes += blob_bytes;
        if let Some(p) = progress {
            p(flushed_bytes, total_bytes);
        }
    }

    Ok(())
}

/// Encrypts one batch of entries into a single epoch blob and commits the result.
async fn flush_one_blob(
    entries: &[(Uuid, Vec<u8>)],
    kek: &KeyEncryptionKey,
    staging_dir: &Path,
    chunk_size_bytes: u64,
    metadata_store: &dyn MetadataStore,
) -> Result<(), StorageError> {
    let mut packed: Zeroizing<Vec<u8>> = Zeroizing::new(Vec::new());
    let mut extents: Vec<(Uuid, u32, u64, u64)> = Vec::new();

    for (node_id, plaintext) in entries {
        let byte_offset = packed.len() as u64;
        let byte_length = plaintext.len() as u64;
        packed.extend_from_slice(plaintext);
        extents.push((*node_id, 0u32, byte_offset, byte_length));
    }

    let chunk_size_usize = usize::try_from(chunk_size_bytes)
        .map_err(|error| StorageError::Database(error.to_string()))?;
    if packed.len() < chunk_size_usize {
        packed.resize(chunk_size_usize, 0u8);
    }

    let epoch_blob_id = Uuid::new_v4();
    let file_key: FileKey = generate_file_key();
    let wrapped_file_key = wrap_file_key(&file_key, &FileId::from_uuid(epoch_blob_id), kek)
        .map_err(StorageError::from)?;

    let packed_owned = std::mem::take(packed.as_mut());
    let encrypted = encrypt_chunk(
        packed_owned,
        &file_key,
        &FileId::from_uuid(epoch_blob_id),
        ChunkIndex::new(0),
    )
    .map_err(StorageError::from)?;

    let blake3_checksum = compute_checksum(&encrypted).0;
    let size_padded = encrypted.len() as u64;

    let blob_name = epoch_blob_id.hyphenated().to_string();
    let blob_path = staging_dir.join(format!("{blob_name}.blob"));
    tokio::fs::write(&blob_path, &encrypted)
        .await
        .map_err(|error| StorageError::Io(error.to_string()))?;

    let record = EpochBlobRecord {
        epoch_blob_id,
        blob_name,
        file_key_wrapped: wrapped_file_key.as_bytes().to_vec(),
        size_padded,
        blake3_checksum,
    };

    metadata_store.commit_epoch_flush(&record, &extents).await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;
    use uuid::Uuid;

    use super::flush_epoch_buffer;
    use crate::crypto::KeyEncryptionKey;
    use crate::storage::MetadataStore;
    use crate::storage::mock::MockMetadataStore;

    /// Returns a mock store with epoch enabled.
    fn epoch_store() -> MockMetadataStore {
        MockMetadataStore::new()
    }

    /// Returns a test KEK.
    fn test_kek() -> KeyEncryptionKey {
        KeyEncryptionKey::from_bytes([0xAB; 32])
    }

    /// Flush on empty buffer is a no-op.
    #[tokio::test]
    async fn test_flush_empty_buffer_is_noop() {
        let temp = TempDir::new().expect("temp dir should be created");
        let store = epoch_store();
        let kek = test_kek();

        let result = flush_epoch_buffer(&store, &kek, temp.path(), 4_194_304, None).await;

        assert!(result.is_ok());
        let entries = store.get_epoch_buffer_entries().await.expect("should list");
        assert!(entries.is_empty());
    }

    /// One small file fills one epoch blob and clears the buffer.
    #[tokio::test]
    async fn test_flush_one_small_file_produces_one_epoch_blob() {
        let temp = TempDir::new().expect("temp dir should be created");
        let store = epoch_store();
        let kek = test_kek();
        let node_id = Uuid::new_v4();

        let plaintext = vec![0xBEu8; 512];
        store
            .stage_epoch_entry(node_id, plaintext.clone())
            .await
            .expect("stage should succeed");

        flush_epoch_buffer(&store, &kek, temp.path(), 4_194_304, None)
            .await
            .expect("flush should succeed");

        let buffer = store.get_epoch_buffer_entries().await.expect("should list");
        assert!(buffer.is_empty(), "buffer should be cleared after flush");

        let chunks = store
            .get_chunks(node_id)
            .await
            .expect("chunks should be readable");
        assert_eq!(chunks.len(), 1, "one chunk row should be created");
        assert!(
            chunks[0].epoch_blob_id.is_some(),
            "chunk should reference an epoch blob"
        );
        assert_eq!(
            chunks[0].byte_offset,
            Some(0),
            "byte_offset should be 0 for first entry"
        );
        assert_eq!(
            chunks[0].byte_length,
            Some(plaintext.len() as u64),
            "byte_length should match plaintext size"
        );
    }

    /// Entries totalling exactly chunk_size_bytes produce exactly one blob.
    #[tokio::test]
    async fn test_flush_exactly_full_blob() {
        let temp = TempDir::new().expect("temp dir should be created");
        let store = epoch_store();
        let kek = test_kek();
        let chunk_size: u64 = 1024;

        let node_id = Uuid::new_v4();
        store
            .stage_epoch_entry(node_id, vec![0x11u8; 1024])
            .await
            .expect("stage should succeed");

        flush_epoch_buffer(&store, &kek, temp.path(), chunk_size, None)
            .await
            .expect("flush should succeed");

        let chunks = store
            .get_chunks(node_id)
            .await
            .expect("chunks should be readable");
        assert_eq!(chunks.len(), 1, "exactly one blob expected");
    }

    /// Entries totalling more than chunk_size_bytes produce two blobs.
    #[tokio::test]
    async fn test_flush_overflow_produces_two_blobs() {
        let temp = TempDir::new().expect("temp dir should be created");
        let store = epoch_store();
        let kek = test_kek();
        let chunk_size: u64 = 512;

        let node_a = Uuid::new_v4();
        let node_b = Uuid::new_v4();
        store
            .stage_epoch_entry(node_a, vec![0x11u8; 400])
            .await
            .expect("stage a should succeed");
        store
            .stage_epoch_entry(node_b, vec![0x22u8; 200])
            .await
            .expect("stage b should succeed");

        flush_epoch_buffer(&store, &kek, temp.path(), chunk_size, None)
            .await
            .expect("flush should succeed");

        let chunks_a = store
            .get_chunks(node_a)
            .await
            .expect("chunks a should be readable");
        let chunks_b = store
            .get_chunks(node_b)
            .await
            .expect("chunks b should be readable");
        assert_eq!(chunks_a.len(), 1);
        assert_eq!(chunks_b.len(), 1);
        assert_ne!(
            chunks_a[0].epoch_blob_id, chunks_b[0].epoch_blob_id,
            "overflow should produce two distinct epoch blobs"
        );
    }
}
