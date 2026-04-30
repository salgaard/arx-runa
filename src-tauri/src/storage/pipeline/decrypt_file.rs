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

/// Decrypts a file from encrypted chunk blobs into a destination path.
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
            let blob_path = blob_directory.join(format!("{}.blob", chunk.blob_name));
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

    let blob_path = blob_directory.join(format!("{}.blob", record.blob_name));
    let encrypted_bytes = tokio::fs::read(&blob_path)
        .await
        .map_err(|error| StorageError::Io(error.to_string()))?;

    let expected_hash = Blake3Hash(record.blake3_checksum);
    let verified_blob =
        verify_checksum(encrypted_bytes, &expected_hash).map_err(StorageError::from)?;

    let wrapped_file_key = WrappedFileKey(record.file_key_wrapped.try_into().map_err(|_| {
        StorageError::Database("epoch blob key_wrapped has wrong length".to_owned())
    })?);
    let file_key = unwrap_file_key(&wrapped_file_key, kek).map_err(StorageError::from)?;

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

#[cfg(test)]
mod tests {
    use std::path::Path;

    use async_trait::async_trait;
    use tempfile::TempDir;
    use tokio::fs;
    use tokio::io::AsyncWriteExt;
    use uuid::Uuid;

    use super::decrypt_file;
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
            _plaintext: Vec<u8>,
        ) -> Result<(), crate::storage::error::StorageError> {
            Err(crate::storage::error::StorageError::Database(
                "unused test helper method".to_owned(),
            ))
        }

        /// Fails for this test helper.
        async fn stage_epoch_entry(
            &self,
            _node_id: uuid::Uuid,
            _plaintext: Vec<u8>,
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
}
