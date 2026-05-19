use std::path::{Path, PathBuf};

use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncWriteExt, BufReader, BufWriter};
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::crypto::{
    Blake3Hash, ChunkIndex, FileId, FileKey, KeyEncryptionKey, WrappedFileKey, decrypt_chunk,
    unwrap_file_key, verify_checksum,
};
use crate::storage::MetadataStore;
use crate::storage::error::StorageError;
use crate::storage::pipeline::read_chunk_size_bytes;
use crate::storage::types::ChunkRecord;
use crate::storage::validation::{
    validate_blob_name_uuid_v4, validate_size_padded_matches_chunk_size,
};

/// Resolves the path of a blob from its name by checking, in order, the
/// `pending/` subdirectory, the `cache/` subdirectory, then falling back to
/// flat staging for backwards-compatibility with blobs written before migration.
async fn resolve_blob_path(staging_dir: &Path, blob_name: &str) -> PathBuf {
    let pending = staging_dir
        .join("pending")
        .join(format!("{blob_name}.blob"));
    if tokio::fs::try_exists(&pending).await.unwrap_or(false) {
        return pending;
    }
    let cache = staging_dir.join("cache").join(format!("{blob_name}.blob"));
    if tokio::fs::try_exists(&cache).await.unwrap_or(false) {
        return cache;
    }
    staging_dir.join(format!("{blob_name}.blob"))
}

///
/// The optional `progress` callback is invoked after each chunk's plaintext is
/// written with `(bytes_decrypted, file_size)`.  Callback fires AFTER the
/// verify+decrypt step, never before.  Pass `None` to suppress progress
/// reporting.  The callback MUST NOT import or depend on `tauri::`.
#[allow(clippy::too_many_arguments)]
pub async fn decrypt_file(
    destination: &Path,
    file_id: Uuid,
    file_key: &FileKey,
    file_size: u64,
    chunks: &[ChunkRecord],
    blob_directory: &Path,
    metadata_store: &dyn MetadataStore,
    progress: Option<&(dyn Fn(u64, u64) + Send + Sync)>,
) -> Result<(), StorageError> {
    let chunk_size_bytes = read_chunk_size_bytes(metadata_store).await?;
    let expected_blob_len = chunk_size_bytes.checked_add(40).ok_or_else(|| {
        StorageError::Database("chunk_size_bytes overflow while sizing blob".to_owned())
    })?;
    let expected_blob_len_usize = usize::try_from(expected_blob_len)
        .map_err(|error| StorageError::Database(error.to_string()))?;
    let expected_chunk_count_u64 = if file_size == 0 {
        0
    } else {
        file_size.div_ceil(chunk_size_bytes)
    };
    let expected_chunk_count = usize::try_from(expected_chunk_count_u64)
        .map_err(|error| StorageError::Database(error.to_string()))?;
    if chunks.len() != expected_chunk_count {
        return Err(StorageError::ConstraintViolation(
            "chunk list is malformed: missing or duplicate chunk_index".to_owned(),
        ));
    }

    let mut sorted_chunks: Vec<&ChunkRecord> = chunks.iter().collect();
    sorted_chunks.sort_by_key(|chunk| chunk.chunk_index);
    for (expected_index, chunk) in sorted_chunks.iter().enumerate() {
        if chunk.chunk_index != expected_index as u32 {
            return Err(StorageError::ConstraintViolation(
                "chunk list is malformed: missing or duplicate chunk_index".to_owned(),
            ));
        }
        validate_size_padded_matches_chunk_size(chunk.size_padded, chunk_size_bytes)?;
    }

    let temporary_destination = temporary_destination_path(destination);
    let destination_file = File::create(&temporary_destination)
        .await
        .map_err(|error| StorageError::Io(error.to_string()))?;
    let mut destination_writer = BufWriter::new(destination_file);
    let crypto_file_id = FileId::from_uuid(file_id);
    let mut bytes_decrypted: u64 = 0;

    let decrypt_result: Result<(), StorageError> = async {
        for (position, chunk) in sorted_chunks.iter().enumerate() {
            validate_blob_name_uuid_v4(&chunk.blob_name)?;
            let blob_path = resolve_blob_path(blob_directory, &chunk.blob_name).await;
            let encrypted_blob =
                read_encrypted_blob(&blob_path, expected_blob_len, expected_blob_len_usize).await?;

            let expected_hash = Blake3Hash(chunk.blake3_checksum);
            let verified_blob =
                verify_checksum(encrypted_blob, &expected_hash).map_err(StorageError::from)?;
            let plaintext = decrypt_chunk(
                verified_blob,
                file_key,
                &crypto_file_id,
                ChunkIndex::new(chunk.chunk_index),
            )
            .map_err(StorageError::from)?;
            let bytes_to_write = if position + 1 == sorted_chunks.len() {
                file_size
                    .checked_sub(u64::from(chunk.chunk_index) * chunk_size_bytes)
                    .ok_or_else(|| {
                        StorageError::ConstraintViolation(
                            "file_size underflow while computing last-chunk truncation".to_owned(),
                        )
                    })?
            } else {
                chunk_size_bytes
            };
            let bytes_to_write_usize = usize::try_from(bytes_to_write)
                .map_err(|error| StorageError::Database(error.to_string()))?;
            if bytes_to_write_usize > plaintext.len() {
                return Err(StorageError::ConstraintViolation(
                    "file_size exceeds padded plaintext length".to_owned(),
                ));
            }
            destination_writer
                .write_all(&plaintext[..bytes_to_write_usize])
                .await
                .map_err(|error| StorageError::Io(error.to_string()))?;
            bytes_decrypted += bytes_to_write_usize as u64;
            if let Some(cb) = progress {
                cb(bytes_decrypted, file_size);
            }
        }
        destination_writer
            .flush()
            .await
            .map_err(|error| StorageError::Io(error.to_string()))?;
        let destination_file = destination_writer.into_inner();
        destination_file
            .sync_all()
            .await
            .map_err(|error| StorageError::Io(error.to_string()))?;
        replace_destination_with_temp(destination, &temporary_destination).await
    }
    .await;

    if decrypt_result.is_err() {
        let _ = tokio::fs::remove_file(&temporary_destination).await;
    }
    decrypt_result
}

/// Reads one encrypted blob with strict fixed-size bounds.
async fn read_encrypted_blob(
    blob_path: &Path,
    expected_blob_len: u64,
    expected_blob_len_usize: usize,
) -> Result<Vec<u8>, StorageError> {
    let blob_metadata = tokio::fs::metadata(blob_path)
        .await
        .map_err(|error| StorageError::Io(error.to_string()))?;
    if blob_metadata.len() != expected_blob_len {
        return Err(StorageError::ConstraintViolation(
            "encrypted blob length mismatch for configured chunk_size_bytes".to_owned(),
        ));
    }
    let encrypted_blob_file = File::open(blob_path)
        .await
        .map_err(|error| StorageError::Io(error.to_string()))?;
    let mut encrypted_blob_reader = BufReader::new(encrypted_blob_file);
    let mut encrypted_blob = vec![0u8; expected_blob_len_usize];
    encrypted_blob_reader
        .read_exact(&mut encrypted_blob)
        .await
        .map_err(|error| StorageError::Io(error.to_string()))?;
    Ok(encrypted_blob)
}

/// Builds a temporary sibling destination path for atomic replacement.
fn temporary_destination_path(destination: &Path) -> PathBuf {
    let temporary_name = format!(".arx-runa-decrypt-{}.tmp", Uuid::new_v4().hyphenated());
    destination
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(temporary_name)
}

/// Replaces destination with a fully written temporary file.
async fn replace_destination_with_temp(
    destination: &Path,
    temporary_destination: &Path,
) -> Result<(), StorageError> {
    if !tokio::fs::try_exists(destination)
        .await
        .map_err(|error| StorageError::Io(error.to_string()))?
    {
        return tokio::fs::rename(temporary_destination, destination)
            .await
            .map_err(|error| StorageError::Io(error.to_string()));
    }
    let backup_destination = temporary_destination_path(destination);
    tokio::fs::rename(destination, &backup_destination)
        .await
        .map_err(|error| StorageError::Io(error.to_string()))?;
    if let Err(error) = tokio::fs::rename(temporary_destination, destination).await {
        let _ = tokio::fs::rename(&backup_destination, destination).await;
        return Err(StorageError::Io(error.to_string()));
    }
    tokio::fs::remove_file(backup_destination)
        .await
        .map_err(|error| StorageError::Io(error.to_string()))
}

/// Decrypts a file into a `Zeroizing<Vec<u8>>` in RAM without writing to disk.
///
/// Identical validation and chunk-decryption logic to [`decrypt_file`], but
/// collects plaintext into a memory buffer instead of a destination file.
/// Callers are responsible for enforcing a size gate before invoking this
/// function (the IPC layer enforces 50 MiB).
#[allow(clippy::too_many_arguments)]
pub async fn decrypt_file_to_memory(
    file_id: Uuid,
    file_key: &FileKey,
    file_size: u64,
    chunks: &[ChunkRecord],
    blob_directory: &Path,
    metadata_store: &dyn MetadataStore,
    progress: Option<&(dyn Fn(u64, u64) + Send + Sync)>,
) -> Result<Zeroizing<Vec<u8>>, StorageError> {
    let chunk_size_bytes = read_chunk_size_bytes(metadata_store).await?;
    let expected_blob_len = chunk_size_bytes.checked_add(40).ok_or_else(|| {
        StorageError::Database("chunk_size_bytes overflow while sizing blob".to_owned())
    })?;
    let expected_blob_len_usize = usize::try_from(expected_blob_len)
        .map_err(|error| StorageError::Database(error.to_string()))?;
    let expected_chunk_count_u64 = if file_size == 0 {
        0
    } else {
        file_size.div_ceil(chunk_size_bytes)
    };
    let expected_chunk_count = usize::try_from(expected_chunk_count_u64)
        .map_err(|error| StorageError::Database(error.to_string()))?;
    if chunks.len() != expected_chunk_count {
        return Err(StorageError::ConstraintViolation(
            "chunk list is malformed: missing or duplicate chunk_index".to_owned(),
        ));
    }

    let mut sorted_chunks: Vec<&ChunkRecord> = chunks.iter().collect();
    sorted_chunks.sort_by_key(|chunk| chunk.chunk_index);
    for (expected_index, chunk) in sorted_chunks.iter().enumerate() {
        if chunk.chunk_index != expected_index as u32 {
            return Err(StorageError::ConstraintViolation(
                "chunk list is malformed: missing or duplicate chunk_index".to_owned(),
            ));
        }
        validate_size_padded_matches_chunk_size(chunk.size_padded, chunk_size_bytes)?;
    }

    let file_size_usize =
        usize::try_from(file_size).map_err(|error| StorageError::Database(error.to_string()))?;
    let mut output: Zeroizing<Vec<u8>> = Zeroizing::new(Vec::with_capacity(file_size_usize));
    let crypto_file_id = FileId::from_uuid(file_id);
    let mut bytes_decrypted: u64 = 0;

    for (position, chunk) in sorted_chunks.iter().enumerate() {
        validate_blob_name_uuid_v4(&chunk.blob_name)?;
        let blob_path = resolve_blob_path(blob_directory, &chunk.blob_name).await;
        let encrypted_blob =
            read_encrypted_blob(&blob_path, expected_blob_len, expected_blob_len_usize).await?;

        let expected_hash = Blake3Hash(chunk.blake3_checksum);
        let verified_blob =
            verify_checksum(encrypted_blob, &expected_hash).map_err(StorageError::from)?;
        let padded_plaintext = decrypt_chunk(
            verified_blob,
            file_key,
            &crypto_file_id,
            ChunkIndex::new(chunk.chunk_index),
        )
        .map_err(StorageError::from)?;
        let plaintext = Zeroizing::new(padded_plaintext);
        let bytes_to_write = if position + 1 == sorted_chunks.len() {
            file_size
                .checked_sub(u64::from(chunk.chunk_index) * chunk_size_bytes)
                .ok_or_else(|| {
                    StorageError::ConstraintViolation(
                        "file_size underflow while computing last-chunk truncation".to_owned(),
                    )
                })?
        } else {
            chunk_size_bytes
        };
        let bytes_to_write_usize = usize::try_from(bytes_to_write)
            .map_err(|error| StorageError::Database(error.to_string()))?;
        if bytes_to_write_usize > plaintext.len() {
            return Err(StorageError::ConstraintViolation(
                "file_size exceeds padded plaintext length".to_owned(),
            ));
        }
        output.extend_from_slice(&plaintext[..bytes_to_write_usize]);
        bytes_decrypted += bytes_to_write_usize as u64;
        if let Some(cb) = progress {
            cb(bytes_decrypted, file_size);
        }
    }

    Ok(output)
}

/// Decrypts a byte-range `[range_start, range_end]` (inclusive) of a multi-chunk file into
/// memory without writing to disk.
///
/// Only the chunks that overlap with the requested range are downloaded and decrypted,
/// so RAM usage stays proportional to the range size rather than the whole file.
/// The streaming invariant holds: at most one chunk's plaintext exists in RAM at a time.
///
/// Both `range_start` and `range_end` must satisfy `range_start <= range_end < file_size`.
#[allow(clippy::too_many_arguments)]
pub async fn decrypt_file_range_to_memory(
    file_id: Uuid,
    file_key: &FileKey,
    file_size: u64,
    chunks: &[ChunkRecord],
    blob_directory: &Path,
    metadata_store: &dyn MetadataStore,
    range_start: u64,
    range_end: u64,
) -> Result<Zeroizing<Vec<u8>>, StorageError> {
    if range_start > range_end || range_end >= file_size {
        return Err(StorageError::ConstraintViolation(
            "range is out of bounds for this file".to_owned(),
        ));
    }

    let chunk_size_bytes = read_chunk_size_bytes(metadata_store).await?;
    let expected_blob_len = chunk_size_bytes.checked_add(40).ok_or_else(|| {
        StorageError::Database("chunk_size_bytes overflow while sizing blob".to_owned())
    })?;
    let expected_blob_len_usize = usize::try_from(expected_blob_len)
        .map_err(|error| StorageError::Database(error.to_string()))?;

    let expected_chunk_count_u64 = file_size.div_ceil(chunk_size_bytes);
    let expected_chunk_count = usize::try_from(expected_chunk_count_u64)
        .map_err(|error| StorageError::Database(error.to_string()))?;
    if chunks.len() != expected_chunk_count {
        return Err(StorageError::ConstraintViolation(
            "chunk list is malformed: missing or duplicate chunk_index".to_owned(),
        ));
    }

    let mut sorted_chunks: Vec<&ChunkRecord> = chunks.iter().collect();
    sorted_chunks.sort_by_key(|chunk| chunk.chunk_index);
    for (expected_index, chunk) in sorted_chunks.iter().enumerate() {
        if chunk.chunk_index != expected_index as u32 {
            return Err(StorageError::ConstraintViolation(
                "chunk list is malformed: missing or duplicate chunk_index".to_owned(),
            ));
        }
        validate_size_padded_matches_chunk_size(chunk.size_padded, chunk_size_bytes)?;
    }

    let first_chunk_index = (range_start / chunk_size_bytes) as usize;
    let last_chunk_index = (range_end / chunk_size_bytes) as usize;
    let range_len = usize::try_from(range_end - range_start + 1)
        .map_err(|e| StorageError::Database(e.to_string()))?;
    let mut output: Zeroizing<Vec<u8>> = Zeroizing::new(Vec::with_capacity(range_len));
    let crypto_file_id = FileId::from_uuid(file_id);

    for chunk in sorted_chunks
        .iter()
        .skip(first_chunk_index)
        .take(last_chunk_index - first_chunk_index + 1)
    {
        validate_blob_name_uuid_v4(&chunk.blob_name)?;
        let blob_path = resolve_blob_path(blob_directory, &chunk.blob_name).await;
        let encrypted_blob =
            read_encrypted_blob(&blob_path, expected_blob_len, expected_blob_len_usize).await?;

        let expected_hash = Blake3Hash(chunk.blake3_checksum);
        let verified_blob =
            verify_checksum(encrypted_blob, &expected_hash).map_err(StorageError::from)?;
        let padded_plaintext = decrypt_chunk(
            verified_blob,
            file_key,
            &crypto_file_id,
            ChunkIndex::new(chunk.chunk_index),
        )
        .map_err(StorageError::from)?;
        let plaintext = Zeroizing::new(padded_plaintext);

        let chunk_file_offset = u64::from(chunk.chunk_index) * chunk_size_bytes;
        let real_bytes_in_chunk = if chunk.chunk_index as usize + 1 == sorted_chunks.len() {
            file_size.checked_sub(chunk_file_offset).ok_or_else(|| {
                StorageError::ConstraintViolation(
                    "file_size underflow while computing last-chunk truncation".to_owned(),
                )
            })?
        } else {
            chunk_size_bytes
        };

        let chunk_file_end = chunk_file_offset + real_bytes_in_chunk - 1;
        let overlap_start = range_start.max(chunk_file_offset);
        let overlap_end = range_end.min(chunk_file_end);

        if overlap_start > overlap_end {
            continue;
        }

        let plain_slice_start = usize::try_from(overlap_start - chunk_file_offset)
            .map_err(|e| StorageError::Database(e.to_string()))?;
        let plain_slice_end = usize::try_from(overlap_end - chunk_file_offset + 1)
            .map_err(|e| StorageError::Database(e.to_string()))?;

        if plain_slice_end > plaintext.len() {
            return Err(StorageError::ConstraintViolation(
                "range slice exceeds padded plaintext length".to_owned(),
            ));
        }

        output.extend_from_slice(&plaintext[plain_slice_start..plain_slice_end]);
    }

    Ok(output)
}

/// Decrypts a file packed into an epoch blob into a destination path.
///
/// Reads the epoch blob, verifies its BLAKE3 checksum, decrypts with the epoch key,
/// then slices the relevant byte range for the given chunk extent.
pub async fn decrypt_epoch_file(
    destination: &Path,
    chunk: &ChunkRecord,
    kek: &KeyEncryptionKey,
    blob_directory: &Path,
    metadata_store: &dyn MetadataStore,
    progress: Option<&(dyn Fn(u64, u64) + Send + Sync)>,
) -> Result<(), StorageError> {
    let epoch_blob_id = chunk.epoch_blob_id.ok_or_else(|| {
        StorageError::ConstraintViolation("decrypt_epoch_file called on non-epoch chunk".to_owned())
    })?;
    let byte_offset = chunk.byte_offset.ok_or_else(|| {
        StorageError::ConstraintViolation("epoch chunk missing byte_offset".to_owned())
    })?;
    let byte_length = chunk.byte_length.ok_or_else(|| {
        StorageError::ConstraintViolation("epoch chunk missing byte_length".to_owned())
    })?;

    let record = metadata_store.get_epoch_blob(epoch_blob_id).await?;

    let blob_path = resolve_blob_path(blob_directory, &record.blob_name).await;
    let encrypted_bytes = tokio::fs::read(&blob_path)
        .await
        .map_err(|error| StorageError::Io(error.to_string()))?;

    let expected_hash = Blake3Hash(record.blake3_checksum);
    let verified_blob =
        verify_checksum(encrypted_bytes, &expected_hash).map_err(StorageError::from)?;

    let wrapped_file_key =
        WrappedFileKey::new(record.file_key_wrapped.try_into().map_err(|_| {
            StorageError::Database("epoch blob key_wrapped has wrong length".to_owned())
        })?);
    let file_key = unwrap_file_key(
        &wrapped_file_key,
        &FileId::from_uuid(record.epoch_blob_id),
        kek,
    )
    .map_err(StorageError::from)?;

    let decrypted = decrypt_chunk(
        verified_blob,
        &file_key,
        &FileId::from_uuid(record.epoch_blob_id),
        ChunkIndex::new(0),
    )
    .map_err(StorageError::from)?;

    let start = byte_offset as usize;
    let end = (byte_offset + byte_length) as usize;
    if end > decrypted.len() {
        return Err(StorageError::ConstraintViolation(
            "epoch byte range exceeds decrypted blob length".to_owned(),
        ));
    }
    let file_bytes = &decrypted[start..end];

    let temporary_destination = temporary_destination_path(destination);
    let dest_file = tokio::fs::File::create(&temporary_destination)
        .await
        .map_err(|error| StorageError::Io(error.to_string()))?;
    let mut dest_writer = BufWriter::new(dest_file);
    dest_writer
        .write_all(file_bytes)
        .await
        .map_err(|error| StorageError::Io(error.to_string()))?;
    dest_writer
        .flush()
        .await
        .map_err(|error| StorageError::Io(error.to_string()))?;
    let dest_file = dest_writer.into_inner();
    dest_file
        .sync_all()
        .await
        .map_err(|error| StorageError::Io(error.to_string()))?;

    let replace_result = replace_destination_with_temp(destination, &temporary_destination).await;
    if replace_result.is_err() {
        let _ = tokio::fs::remove_file(&temporary_destination).await;
        return replace_result;
    }

    if let Some(cb) = progress {
        cb(byte_length, byte_length);
    }

    Ok(())
}

/// Decrypts a file packed into an epoch blob into a `Zeroizing<Vec<u8>>` in RAM.
///
/// Mirrors [`decrypt_epoch_file`] but returns the extracted byte range as an
/// in-memory buffer instead of writing it to disk.
pub async fn decrypt_epoch_file_to_memory(
    chunk: &ChunkRecord,
    kek: &KeyEncryptionKey,
    blob_directory: &Path,
    metadata_store: &dyn MetadataStore,
    progress: Option<&(dyn Fn(u64, u64) + Send + Sync)>,
) -> Result<Zeroizing<Vec<u8>>, StorageError> {
    let epoch_blob_id = chunk.epoch_blob_id.ok_or_else(|| {
        StorageError::ConstraintViolation(
            "decrypt_epoch_file_to_memory called on non-epoch chunk".to_owned(),
        )
    })?;
    let byte_offset = chunk.byte_offset.ok_or_else(|| {
        StorageError::ConstraintViolation("epoch chunk missing byte_offset".to_owned())
    })?;
    let byte_length = chunk.byte_length.ok_or_else(|| {
        StorageError::ConstraintViolation("epoch chunk missing byte_length".to_owned())
    })?;

    let record = metadata_store.get_epoch_blob(epoch_blob_id).await?;

    let blob_path = resolve_blob_path(blob_directory, &record.blob_name).await;
    let encrypted_bytes = tokio::fs::read(&blob_path)
        .await
        .map_err(|error| StorageError::Io(error.to_string()))?;

    let expected_hash = Blake3Hash(record.blake3_checksum);
    let verified_blob =
        verify_checksum(encrypted_bytes, &expected_hash).map_err(StorageError::from)?;

    let wrapped_file_key =
        WrappedFileKey::new(record.file_key_wrapped.try_into().map_err(|_| {
            StorageError::Database("epoch blob key_wrapped has wrong length".to_owned())
        })?);
    let file_key = unwrap_file_key(
        &wrapped_file_key,
        &FileId::from_uuid(record.epoch_blob_id),
        kek,
    )
    .map_err(StorageError::from)?;

    let decrypted = decrypt_chunk(
        verified_blob,
        &file_key,
        &FileId::from_uuid(record.epoch_blob_id),
        ChunkIndex::new(0),
    )
    .map_err(StorageError::from)?;

    let start = byte_offset as usize;
    let end = (byte_offset + byte_length) as usize;
    if end > decrypted.len() {
        return Err(StorageError::ConstraintViolation(
            "epoch byte range exceeds decrypted blob length".to_owned(),
        ));
    }

    let output = Zeroizing::new(decrypted[start..end].to_vec());

    if let Some(cb) = progress {
        cb(byte_length, byte_length);
    }

    Ok(output)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use async_trait::async_trait;
    use tempfile::TempDir;
    use tokio::fs;
    use tokio::io::AsyncWriteExt;
    use uuid::Uuid;

    use zeroize::Zeroizing;

    use super::{decrypt_file, decrypt_file_range_to_memory};
    use crate::crypto::FileKey;
    use crate::storage::MetadataStore;
    use crate::storage::error::StorageError;
    use crate::storage::pipeline::encrypt_file;
    use crate::storage::types::{ChunkRecord, Node};

    /// Test metadata store that exposes only `chunk_size_bytes`.
    struct FixedMetaStore {
        chunk_size_bytes: u64,
    }

    #[async_trait]
    impl MetadataStore for FixedMetaStore {
        /// Fails for this test helper.
        async fn insert_node(&self, _node: &Node) -> Result<(), StorageError> {
            Err(StorageError::Database(
                "unused test helper method".to_owned(),
            ))
        }

        /// Fails for this test helper.
        async fn insert_chunks(&self, _chunks: &[ChunkRecord]) -> Result<(), StorageError> {
            Err(StorageError::Database(
                "unused test helper method".to_owned(),
            ))
        }

        /// Fails for this test helper.
        async fn insert_file_with_chunks(
            &self,
            _node: &Node,
            _chunks: &[ChunkRecord],
        ) -> Result<(), StorageError> {
            Err(StorageError::Database(
                "unused test helper method".to_owned(),
            ))
        }

        /// Fails for this test helper.
        async fn get_node(&self, _node_id: Uuid) -> Result<Node, StorageError> {
            Err(StorageError::Database(
                "unused test helper method".to_owned(),
            ))
        }

        /// Fails for this test helper.
        async fn list_children(&self, _parent_id: Uuid) -> Result<Vec<Node>, StorageError> {
            Err(StorageError::Database(
                "unused test helper method".to_owned(),
            ))
        }

        /// Fails for this test helper.
        async fn get_chunks(&self, _node_id: Uuid) -> Result<Vec<ChunkRecord>, StorageError> {
            Err(StorageError::Database(
                "unused test helper method".to_owned(),
            ))
        }

        /// Fails for this test helper.
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

        /// Fails for this test helper.
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

        /// Fails for this test helper.
        async fn delete_node(&self, _node_id: Uuid) -> Result<(), StorageError> {
            Err(StorageError::Database(
                "unused test helper method".to_owned(),
            ))
        }

        /// Fails for this test helper.
        async fn list_pending_deletions(&self, _limit: usize) -> Result<Vec<String>, StorageError> {
            Err(StorageError::Database(
                "unused test helper method".to_owned(),
            ))
        }

        /// Fails for this test helper.
        async fn mark_deletion_complete(&self, _blob_name: &str) -> Result<(), StorageError> {
            Err(StorageError::Database(
                "unused test helper method".to_owned(),
            ))
        }

        /// Returns configured metadata values.
        async fn get_meta(&self, key: &str) -> Result<Option<String>, StorageError> {
            if key == "chunk_size_bytes" {
                return Ok(Some(self.chunk_size_bytes.to_string()));
            }
            Ok(None)
        }

        /// Fails for this test helper.
        async fn set_meta(&self, _key: &str, _value: &str) -> Result<(), StorageError> {
            Err(StorageError::Database(
                "unused test helper method".to_owned(),
            ))
        }

        /// Fails for this test helper.
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

    /// Creates a source file with provided content.
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

    /// Encrypts source content and returns chunk metadata.
    async fn stage_encrypted_chunks(
        metadata_store: &FixedMetaStore,
        source_path: &Path,
        staging_directory: &Path,
        file_id: Uuid,
        file_key: &FileKey,
    ) -> Vec<ChunkRecord> {
        encrypt_file(
            source_path,
            file_id,
            file_key,
            metadata_store,
            staging_directory,
            None,
        )
        .await
        .expect("encrypt_file should succeed")
    }

    /// Verifies one-chunk decrypt round trip returns original plaintext.
    #[tokio::test]
    async fn test_decrypt_file_round_trip_single_chunk_returns_original() {
        let temp_dir = TempDir::new().expect("temporary directory should be created");
        let source_path = temp_dir.path().join("source.bin");
        let destination_path = temp_dir.path().join("destination.bin");
        let staging_directory = temp_dir.path().join("staging");
        fs::create_dir_all(&staging_directory)
            .await
            .expect("staging directory should be created");
        let plaintext = b"hello arx runa".to_vec();
        write_source_file(&source_path, &plaintext).await;
        let metadata_store = FixedMetaStore {
            chunk_size_bytes: 131_072,
        };
        let file_id = Uuid::new_v4();
        let file_key = FileKey::from_bytes([13; 32]);
        let chunks = stage_encrypted_chunks(
            &metadata_store,
            &source_path,
            &staging_directory,
            file_id,
            &file_key,
        )
        .await;

        decrypt_file(
            &destination_path,
            file_id,
            &file_key,
            plaintext.len() as u64,
            &chunks,
            &staging_directory,
            &metadata_store,
            None,
        )
        .await
        .expect("decrypt_file should succeed");

        let recovered = fs::read(destination_path)
            .await
            .expect("destination should be readable");
        assert_eq!(recovered, plaintext);
    }

    /// Verifies multi-chunk decrypt round trip returns original plaintext.
    #[tokio::test]
    async fn test_decrypt_file_round_trip_multi_chunk_returns_original() {
        let temp_dir = TempDir::new().expect("temporary directory should be created");
        let source_path = temp_dir.path().join("source.bin");
        let destination_path = temp_dir.path().join("destination.bin");
        let staging_directory = temp_dir.path().join("staging");
        fs::create_dir_all(&staging_directory)
            .await
            .expect("staging directory should be created");
        let chunk_size = 131_072usize;
        let plaintext = vec![0x44u8; (3 * chunk_size) + (chunk_size / 2)];
        write_source_file(&source_path, &plaintext).await;
        let metadata_store = FixedMetaStore {
            chunk_size_bytes: chunk_size as u64,
        };
        let file_id = Uuid::new_v4();
        let file_key = FileKey::from_bytes([17; 32]);
        let chunks = stage_encrypted_chunks(
            &metadata_store,
            &source_path,
            &staging_directory,
            file_id,
            &file_key,
        )
        .await;

        decrypt_file(
            &destination_path,
            file_id,
            &file_key,
            plaintext.len() as u64,
            &chunks,
            &staging_directory,
            &metadata_store,
            None,
        )
        .await
        .expect("decrypt_file should succeed");

        let recovered = fs::read(destination_path)
            .await
            .expect("destination should be readable");
        assert_eq!(recovered, plaintext);
    }

    /// Verifies zero-byte files decrypt to zero-byte outputs.
    #[tokio::test]
    async fn test_decrypt_file_zero_byte_produces_zero_byte_output() {
        let temp_dir = TempDir::new().expect("temporary directory should be created");
        let destination_path = temp_dir.path().join("destination.bin");
        let staging_directory = temp_dir.path().join("staging");
        fs::create_dir_all(&staging_directory)
            .await
            .expect("staging directory should be created");
        let metadata_store = FixedMetaStore {
            chunk_size_bytes: 131_072,
        };
        let file_key = FileKey::from_bytes([19; 32]);

        decrypt_file(
            &destination_path,
            Uuid::new_v4(),
            &file_key,
            0,
            &[],
            &staging_directory,
            &metadata_store,
            None,
        )
        .await
        .expect("decrypt_file should succeed");

        let metadata = fs::metadata(destination_path)
            .await
            .expect("destination metadata should be readable");
        assert_eq!(metadata.len(), 0);
    }

    /// Verifies checksum tampering is detected before decrypt.
    #[tokio::test]
    async fn test_decrypt_file_blake3_mismatch_returns_checksum_mismatch_without_calling_decrypt() {
        let temp_dir = TempDir::new().expect("temporary directory should be created");
        let source_path = temp_dir.path().join("source.bin");
        let destination_path = temp_dir.path().join("destination.bin");
        let staging_directory = temp_dir.path().join("staging");
        fs::create_dir_all(&staging_directory)
            .await
            .expect("staging directory should be created");
        let plaintext = vec![0x11u8; 2048];
        write_source_file(&source_path, &plaintext).await;
        let metadata_store = FixedMetaStore {
            chunk_size_bytes: 131_072,
        };
        let file_id = Uuid::new_v4();
        let file_key = FileKey::from_bytes([23; 32]);
        let chunks = stage_encrypted_chunks(
            &metadata_store,
            &source_path,
            &staging_directory,
            file_id,
            &file_key,
        )
        .await;
        let tampered_blob_path = staging_directory.join(format!("{}.blob", chunks[0].blob_name));
        let mut tampered_blob = fs::read(&tampered_blob_path)
            .await
            .expect("staged blob should be readable");
        tampered_blob[0] ^= 0x01;
        fs::write(&tampered_blob_path, tampered_blob)
            .await
            .expect("tampered blob should be written");

        let result = decrypt_file(
            &destination_path,
            file_id,
            &file_key,
            plaintext.len() as u64,
            &chunks,
            &staging_directory,
            &metadata_store,
            None,
        )
        .await;

        assert!(matches!(result, Err(StorageError::ChecksumMismatch)));
    }

    /// Verifies malformed gap chunk lists are rejected.
    #[tokio::test]
    async fn test_decrypt_file_malformed_chunk_list_gaps_returns_constraint_violation() {
        let temp_dir = TempDir::new().expect("temporary directory should be created");
        let source_path = temp_dir.path().join("source.bin");
        let destination_path = temp_dir.path().join("destination.bin");
        let staging_directory = temp_dir.path().join("staging");
        fs::create_dir_all(&staging_directory)
            .await
            .expect("staging directory should be created");
        let plaintext = vec![0x21u8; 131_072 + 1];
        write_source_file(&source_path, &plaintext).await;
        let metadata_store = FixedMetaStore {
            chunk_size_bytes: 131_072,
        };
        let file_id = Uuid::new_v4();
        let file_key = FileKey::from_bytes([29; 32]);
        let chunks = stage_encrypted_chunks(
            &metadata_store,
            &source_path,
            &staging_directory,
            file_id,
            &file_key,
        )
        .await;
        let malformed = vec![chunks[0].clone()];

        let result = decrypt_file(
            &destination_path,
            file_id,
            &file_key,
            plaintext.len() as u64,
            &malformed,
            &staging_directory,
            &metadata_store,
            None,
        )
        .await;

        assert!(matches!(
            result,
            Err(StorageError::ConstraintViolation(message))
                if message.contains("chunk list is malformed")
        ));
    }

    /// Verifies malformed duplicate-index chunk lists are rejected.
    #[tokio::test]
    async fn test_decrypt_file_malformed_chunk_list_duplicate_index_returns_constraint_violation() {
        let temp_dir = TempDir::new().expect("temporary directory should be created");
        let source_path = temp_dir.path().join("source.bin");
        let destination_path = temp_dir.path().join("destination.bin");
        let staging_directory = temp_dir.path().join("staging");
        fs::create_dir_all(&staging_directory)
            .await
            .expect("staging directory should be created");
        let plaintext = vec![0x31u8; 131_072 + 1];
        write_source_file(&source_path, &plaintext).await;
        let metadata_store = FixedMetaStore {
            chunk_size_bytes: 131_072,
        };
        let file_id = Uuid::new_v4();
        let file_key = FileKey::from_bytes([31; 32]);
        let chunks = stage_encrypted_chunks(
            &metadata_store,
            &source_path,
            &staging_directory,
            file_id,
            &file_key,
        )
        .await;
        let malformed = vec![chunks[0].clone(), chunks[0].clone()];

        let result = decrypt_file(
            &destination_path,
            file_id,
            &file_key,
            plaintext.len() as u64,
            &malformed,
            &staging_directory,
            &metadata_store,
            None,
        )
        .await;

        assert!(matches!(
            result,
            Err(StorageError::ConstraintViolation(message))
                if message.contains("chunk list is malformed")
        ));
    }

    /// Verifies unsorted chunks are sorted before decrypt.
    #[tokio::test]
    async fn test_decrypt_file_unsorted_chunks_are_sorted_before_decrypt() {
        let temp_dir = TempDir::new().expect("temporary directory should be created");
        let source_path = temp_dir.path().join("source.bin");
        let destination_path = temp_dir.path().join("destination.bin");
        let staging_directory = temp_dir.path().join("staging");
        fs::create_dir_all(&staging_directory)
            .await
            .expect("staging directory should be created");
        let chunk_size = 131_072usize;
        let plaintext = vec![0x39u8; (2 * chunk_size) + 9];
        write_source_file(&source_path, &plaintext).await;
        let metadata_store = FixedMetaStore {
            chunk_size_bytes: chunk_size as u64,
        };
        let file_id = Uuid::new_v4();
        let file_key = FileKey::from_bytes([37; 32]);
        let mut chunks = stage_encrypted_chunks(
            &metadata_store,
            &source_path,
            &staging_directory,
            file_id,
            &file_key,
        )
        .await;
        chunks.reverse();

        decrypt_file(
            &destination_path,
            file_id,
            &file_key,
            plaintext.len() as u64,
            &chunks,
            &staging_directory,
            &metadata_store,
            None,
        )
        .await
        .expect("decrypt_file should succeed");

        let recovered = fs::read(destination_path)
            .await
            .expect("destination should be readable");
        assert_eq!(recovered, plaintext);
    }

    /// MetadataStore that returns a fixed EpochBlobRecord for get_epoch_blob.
    struct EpochMetaStore {
        record: crate::storage::types::EpochBlobRecord,
    }

    #[async_trait]
    impl MetadataStore for EpochMetaStore {
        async fn insert_node(&self, _: &Node) -> Result<(), StorageError> {
            Err(StorageError::Database("unused".to_owned()))
        }

        async fn insert_chunks(&self, _: &[ChunkRecord]) -> Result<(), StorageError> {
            Err(StorageError::Database("unused".to_owned()))
        }

        async fn insert_file_with_chunks(
            &self,
            _: &Node,
            _: &[ChunkRecord],
        ) -> Result<(), StorageError> {
            Err(StorageError::Database("unused".to_owned()))
        }

        async fn get_node(&self, _: Uuid) -> Result<Node, StorageError> {
            Err(StorageError::Database("unused".to_owned()))
        }

        async fn list_children(&self, _: Uuid) -> Result<Vec<Node>, StorageError> {
            Err(StorageError::Database("unused".to_owned()))
        }

        async fn get_chunks(&self, _: Uuid) -> Result<Vec<ChunkRecord>, StorageError> {
            Err(StorageError::Database("unused".to_owned()))
        }

        async fn rename_node(&self, _: Uuid, _: &str, _: i64) -> Result<(), StorageError> {
            Err(StorageError::Database("unused".to_owned()))
        }

        async fn move_node(&self, _: Uuid, _: Option<Uuid>, _: i64) -> Result<(), StorageError> {
            Err(StorageError::Database("unused".to_owned()))
        }

        async fn delete_node(&self, _: Uuid) -> Result<(), StorageError> {
            Err(StorageError::Database("unused".to_owned()))
        }

        async fn list_pending_deletions(&self, _: usize) -> Result<Vec<String>, StorageError> {
            Err(StorageError::Database("unused".to_owned()))
        }

        async fn mark_deletion_complete(&self, _: &str) -> Result<(), StorageError> {
            Err(StorageError::Database("unused".to_owned()))
        }

        async fn get_meta(&self, _: &str) -> Result<Option<String>, StorageError> {
            Ok(None)
        }

        async fn set_meta(&self, _: &str, _: &str) -> Result<(), StorageError> {
            Err(StorageError::Database("unused".to_owned()))
        }

        async fn increment_snapshot_counter(&self) -> Result<u64, StorageError> {
            Err(StorageError::Database("unused".to_owned()))
        }

        async fn insert_file_node_only(&self, _: &Node) -> Result<(), StorageError> {
            Err(StorageError::Database("unused".to_owned()))
        }

        async fn insert_file_node_and_stage_epoch_entry(
            &self,
            _: &Node,
            _: zeroize::Zeroizing<Vec<u8>>,
        ) -> Result<(), StorageError> {
            Err(StorageError::Database("unused".to_owned()))
        }

        async fn stage_epoch_entry(
            &self,
            _: Uuid,
            _: zeroize::Zeroizing<Vec<u8>>,
        ) -> Result<(), StorageError> {
            Err(StorageError::Database("unused".to_owned()))
        }

        async fn get_epoch_buffer_total_bytes(&self) -> Result<u64, StorageError> {
            Err(StorageError::Database("unused".to_owned()))
        }

        async fn get_epoch_buffer_entries(
            &self,
        ) -> Result<Vec<crate::storage::types::EpochBufferEntry>, StorageError> {
            Err(StorageError::Database("unused".to_owned()))
        }

        async fn commit_epoch_flush(
            &self,
            _: &crate::storage::types::EpochBlobRecord,
            _: &[(Uuid, u32, u64, u64)],
        ) -> Result<(), StorageError> {
            Err(StorageError::Database("unused".to_owned()))
        }

        async fn get_epoch_blob(
            &self,
            _epoch_blob_id: Uuid,
        ) -> Result<crate::storage::types::EpochBlobRecord, StorageError> {
            Ok(self.record.clone())
        }

        async fn get_epoch_buffer_node_ids(&self) -> Result<Vec<Uuid>, StorageError> {
            Ok(vec![])
        }
    }

    /// Verifies that decrypt_epoch_file succeeds when the blob is in the pending/ subdirectory,
    /// which is the normal location after a cloud fetch.
    #[tokio::test]
    async fn test_decrypt_epoch_file_blob_in_pending_subdir_returns_correct_bytes() {
        use crate::crypto::{
            ChunkIndex, FileId, KeyEncryptionKey, compute_checksum, encrypt_chunk,
            generate_file_key, wrap_file_key,
        };
        use crate::storage::pipeline::decrypt_epoch_file;
        use crate::storage::types::EpochBlobRecord;
        use zeroize::Zeroizing;

        let temp = TempDir::new().expect("temp dir should be created");
        let staging_dir = temp.path().join("staging");
        let pending_dir = staging_dir.join("pending");
        fs::create_dir_all(&pending_dir)
            .await
            .expect("pending dir should be created");

        let kek = KeyEncryptionKey::from_bytes([0xAB; 32]);
        let file_key = generate_file_key();
        let epoch_blob_id = Uuid::new_v4();
        let wrapped = wrap_file_key(&file_key, &FileId::from_uuid(epoch_blob_id), &kek)
            .expect("wrap should succeed");

        let plaintext_a = vec![0x11u8; 300];
        let plaintext_b = vec![0x22u8; 200];
        let chunk_size: usize = 4_194_304;

        let mut packed: Zeroizing<Vec<u8>> = Zeroizing::new(Vec::new());
        packed.extend_from_slice(&plaintext_a);
        packed.extend_from_slice(&plaintext_b);
        packed.resize(chunk_size, 0u8);
        let blob_name = epoch_blob_id.hyphenated().to_string();

        let encrypted = encrypt_chunk(
            Zeroizing::new(packed.to_vec()),
            &file_key,
            &FileId::from_uuid(epoch_blob_id),
            ChunkIndex::new(0),
        )
        .expect("encrypt should succeed");

        let blake3_checksum = compute_checksum(&encrypted).0;
        let size_padded = encrypted.len() as u64;

        // Write blob to pending/ (simulating cloud fetch)
        let blob_path = pending_dir.join(format!("{blob_name}.blob"));
        fs::write(&blob_path, &encrypted)
            .await
            .expect("blob should be written");

        let record = EpochBlobRecord {
            epoch_blob_id,
            blob_name,
            file_key_wrapped: wrapped.as_bytes().to_vec(),
            size_padded,
            blake3_checksum,
        };

        // Extract the second file (plaintext_b) from the epoch blob
        let chunk = ChunkRecord {
            chunk_id: Uuid::new_v4(),
            node_id: Uuid::new_v4().into(),
            chunk_index: 0,
            blob_name: record.blob_name.clone(),
            size_padded: record.size_padded,
            blake3_checksum: record.blake3_checksum,
            epoch_blob_id: Some(epoch_blob_id),
            byte_offset: Some(plaintext_a.len() as u64),
            byte_length: Some(plaintext_b.len() as u64),
        };

        let meta_store = EpochMetaStore { record };
        let destination = temp.path().join("out.bin");

        decrypt_epoch_file(&destination, &chunk, &kek, &staging_dir, &meta_store, None)
            .await
            .expect("decrypt_epoch_file should succeed");

        let recovered = fs::read(&destination)
            .await
            .expect("destination should be readable");
        assert_eq!(recovered, plaintext_b);
    }

    /// Verifies size-padded mismatches are rejected.
    #[tokio::test]
    async fn test_decrypt_file_size_padded_mismatch_returns_constraint_violation() {
        let temp_dir = TempDir::new().expect("temporary directory should be created");
        let source_path = temp_dir.path().join("source.bin");
        let destination_path = temp_dir.path().join("destination.bin");
        let staging_directory = temp_dir.path().join("staging");
        fs::create_dir_all(&staging_directory)
            .await
            .expect("staging directory should be created");
        let plaintext = vec![0x41u8; 131_072];
        write_source_file(&source_path, &plaintext).await;
        let metadata_store = FixedMetaStore {
            chunk_size_bytes: 131_072,
        };
        let file_id = Uuid::new_v4();
        let file_key = FileKey::from_bytes([41; 32]);
        let mut chunks = stage_encrypted_chunks(
            &metadata_store,
            &source_path,
            &staging_directory,
            file_id,
            &file_key,
        )
        .await;
        chunks[0].size_padded = 131_073;

        let result = decrypt_file(
            &destination_path,
            file_id,
            &file_key,
            plaintext.len() as u64,
            &chunks,
            &staging_directory,
            &metadata_store,
            None,
        )
        .await;

        assert!(matches!(
            result,
            Err(StorageError::ConstraintViolation(message)) if message.contains("size_padded")
        ));
    }

    // ─── decrypt_file_range_to_memory ─────────────────────────────────────────

    /// Verifies a range within a single chunk returns only the requested slice.
    #[tokio::test]
    async fn test_decrypt_file_range_to_memory_single_chunk_mid_range_returns_slice() {
        let temp_dir = TempDir::new().expect("temporary directory should be created");
        let source_path = temp_dir.path().join("source.bin");
        let staging_directory = temp_dir.path().join("staging");
        fs::create_dir_all(&staging_directory)
            .await
            .expect("staging directory should be created");
        let chunk_size = 131_072usize;
        let plaintext: Vec<u8> = (0u8..=255).cycle().take(chunk_size / 2).collect();
        write_source_file(&source_path, &plaintext).await;
        let metadata_store = FixedMetaStore {
            chunk_size_bytes: chunk_size as u64,
        };
        let file_id = Uuid::new_v4();
        let file_key = FileKey::from_bytes([51; 32]);
        let chunks = stage_encrypted_chunks(
            &metadata_store,
            &source_path,
            &staging_directory,
            file_id,
            &file_key,
        )
        .await;

        let range_start = 10u64;
        let range_end = 99u64;
        let result = decrypt_file_range_to_memory(
            file_id,
            &file_key,
            plaintext.len() as u64,
            &chunks,
            &staging_directory,
            &metadata_store,
            range_start,
            range_end,
        )
        .await
        .expect("range decrypt should succeed");

        assert_eq!(result.len(), (range_end - range_start + 1) as usize);
        assert_eq!(
            &*result,
            &plaintext[range_start as usize..=range_end as usize]
        );
    }

    /// Verifies a range spanning two chunk boundaries returns the correct bytes.
    #[tokio::test]
    async fn test_decrypt_file_range_to_memory_cross_chunk_boundary_returns_correct_bytes() {
        let temp_dir = TempDir::new().expect("temporary directory should be created");
        let source_path = temp_dir.path().join("source.bin");
        let staging_directory = temp_dir.path().join("staging");
        fs::create_dir_all(&staging_directory)
            .await
            .expect("staging directory should be created");
        let chunk_size = 131_072usize;
        let plaintext: Vec<u8> = (0u8..=255).cycle().take(chunk_size * 2 + 500).collect();
        write_source_file(&source_path, &plaintext).await;
        let metadata_store = FixedMetaStore {
            chunk_size_bytes: chunk_size as u64,
        };
        let file_id = Uuid::new_v4();
        let file_key = FileKey::from_bytes([53; 32]);
        let chunks = stage_encrypted_chunks(
            &metadata_store,
            &source_path,
            &staging_directory,
            file_id,
            &file_key,
        )
        .await;

        // Straddle the boundary between chunk 0 and chunk 1.
        let range_start = (chunk_size - 4) as u64;
        let range_end = (chunk_size + 4) as u64;
        let result = decrypt_file_range_to_memory(
            file_id,
            &file_key,
            plaintext.len() as u64,
            &chunks,
            &staging_directory,
            &metadata_store,
            range_start,
            range_end,
        )
        .await
        .expect("cross-chunk range decrypt should succeed");

        assert_eq!(result.len(), (range_end - range_start + 1) as usize);
        assert_eq!(
            &*result,
            &plaintext[range_start as usize..=range_end as usize]
        );
    }

    /// Verifies a range ending at the last byte of the file strips padding correctly.
    #[tokio::test]
    async fn test_decrypt_file_range_to_memory_last_byte_range_strips_padding() {
        let temp_dir = TempDir::new().expect("temporary directory should be created");
        let source_path = temp_dir.path().join("source.bin");
        let staging_directory = temp_dir.path().join("staging");
        fs::create_dir_all(&staging_directory)
            .await
            .expect("staging directory should be created");
        let chunk_size = 131_072usize;
        // Non-chunk-aligned size so the last chunk has padding.
        let plaintext: Vec<u8> = (0u8..=255).cycle().take(chunk_size + 37).collect();
        write_source_file(&source_path, &plaintext).await;
        let metadata_store = FixedMetaStore {
            chunk_size_bytes: chunk_size as u64,
        };
        let file_id = Uuid::new_v4();
        let file_key = FileKey::from_bytes([59; 32]);
        let chunks = stage_encrypted_chunks(
            &metadata_store,
            &source_path,
            &staging_directory,
            file_id,
            &file_key,
        )
        .await;

        let range_start = (chunk_size + 10) as u64;
        let range_end = (plaintext.len() - 1) as u64;
        let result = decrypt_file_range_to_memory(
            file_id,
            &file_key,
            plaintext.len() as u64,
            &chunks,
            &staging_directory,
            &metadata_store,
            range_start,
            range_end,
        )
        .await
        .expect("last-byte range decrypt should succeed");

        assert_eq!(result.len(), (range_end - range_start + 1) as usize);
        assert_eq!(
            &*result,
            &plaintext[range_start as usize..=range_end as usize]
        );
    }

    /// Verifies a full-file range returns the entire plaintext.
    #[tokio::test]
    async fn test_decrypt_file_range_to_memory_full_range_returns_all_plaintext() {
        let temp_dir = TempDir::new().expect("temporary directory should be created");
        let source_path = temp_dir.path().join("source.bin");
        let staging_directory = temp_dir.path().join("staging");
        fs::create_dir_all(&staging_directory)
            .await
            .expect("staging directory should be created");
        let chunk_size = 131_072usize;
        let plaintext = vec![0x7Eu8; chunk_size * 2 + 1];
        write_source_file(&source_path, &plaintext).await;
        let metadata_store = FixedMetaStore {
            chunk_size_bytes: chunk_size as u64,
        };
        let file_id = Uuid::new_v4();
        let file_key = FileKey::from_bytes([61; 32]);
        let chunks = stage_encrypted_chunks(
            &metadata_store,
            &source_path,
            &staging_directory,
            file_id,
            &file_key,
        )
        .await;

        let result = decrypt_file_range_to_memory(
            file_id,
            &file_key,
            plaintext.len() as u64,
            &chunks,
            &staging_directory,
            &metadata_store,
            0,
            (plaintext.len() - 1) as u64,
        )
        .await
        .expect("full-file range decrypt should succeed");

        assert_eq!(&*result, plaintext.as_slice());
    }

    /// Verifies that `Zeroizing<Vec<u8>>` zeroes its bytes when zeroize is called.
    ///
    /// The decrypt pipeline wraps each chunk's plaintext in `Zeroizing<Vec<u8>>`
    /// so it is wiped from memory when the binding is dropped.  This test
    /// confirms the `zeroize` crate's guarantees hold in this build configuration.
    #[test]
    fn test_zeroizing_vec_zeroes_chunk_buffer_on_drop() {
        use zeroize::Zeroize;
        let known_bytes = vec![0xABu8; 64];
        let mut buffer = Zeroizing::new(known_bytes);
        buffer.zeroize();
        assert!(
            buffer.iter().all(|&b| b == 0),
            "Zeroizing<Vec<u8>> must zero its buffer on drop"
        );
    }
}
