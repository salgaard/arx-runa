use std::path::Path;

use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncWriteExt, BufReader, BufWriter};
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::crypto::{ChunkIndex, FileId, FileKey, compute_checksum, encrypt_chunk};
use crate::storage::MetadataStore;
use crate::storage::error::StorageError;
use crate::storage::pipeline::read_chunk_size_bytes;
use crate::storage::types::{ChunkRecord, NodeId};

/// Encrypts a source file into fixed-size encrypted chunk blobs in staging.
///
/// The optional `progress` callback is invoked after each chunk write with
/// `(bytes_processed, file_size_total)`.  Pass `None` to suppress progress
/// reporting.  The callback MUST NOT import or depend on `tauri::`.
pub async fn encrypt_file(
    source: &Path,
    file_id: Uuid,
    file_key: &FileKey,
    metadata_store: &dyn MetadataStore,
    staging_directory: &Path,
    progress: Option<&(dyn Fn(u64, u64) + Send + Sync)>,
) -> Result<Vec<ChunkRecord>, StorageError> {
    let mut staged_blob_names = Vec::new();
    let result = encrypt_file_inner(
        source,
        file_id,
        file_key,
        metadata_store,
        staging_directory,
        &mut staged_blob_names,
        progress,
    )
    .await;
    if result.is_err() {
        cleanup_staged_blobs(staging_directory, &staged_blob_names).await;
    }
    result
}

/// Performs chunked encryption and records staged blob names for cleanup on failure.
///
/// Invokes `progress(bytes_processed, file_size)` after each successful chunk
/// write, where `file_size` is read from filesystem metadata at entry.
async fn encrypt_file_inner(
    source: &Path,
    file_id: Uuid,
    file_key: &FileKey,
    metadata_store: &dyn MetadataStore,
    staging_directory: &Path,
    staged_blob_names: &mut Vec<String>,
    progress: Option<&(dyn Fn(u64, u64) + Send + Sync)>,
) -> Result<Vec<ChunkRecord>, StorageError> {
    let chunk_size_bytes = read_chunk_size_bytes(metadata_store).await?;
    let chunk_size_usize = usize::try_from(chunk_size_bytes)
        .map_err(|error| StorageError::Database(error.to_string()))?;
    let file_size = tokio::fs::metadata(source)
        .await
        .map(|m| m.len())
        .unwrap_or(0);
    let source_file = File::open(source)
        .await
        .map_err(|error| StorageError::Io(error.to_string()))?;
    let mut source_reader = BufReader::new(source_file);
    let mut chunk_records = Vec::new();
    let mut chunk_index = 0u32;
    let mut bytes_processed: u64 = 0;
    let crypto_file_id = FileId::from_uuid(file_id);

    loop {
        let mut plaintext = Zeroizing::new(vec![0u8; chunk_size_usize]);
        let bytes_read = read_chunk_plaintext(&mut source_reader, plaintext.as_mut_slice()).await?;
        if bytes_read == 0 && chunk_index == 0 {
            return Ok(chunk_records);
        }
        if bytes_read == 0 {
            break;
        }

        let owned_plaintext = std::mem::take(plaintext.as_mut());
        let wire_blob = encrypt_chunk(
            owned_plaintext,
            file_key,
            &crypto_file_id,
            ChunkIndex::new(chunk_index),
        )
        .map_err(StorageError::from)?;
        let checksum = compute_checksum(&wire_blob);
        let blob_name = Uuid::new_v4().hyphenated().to_string();
        staged_blob_names.push(blob_name.clone());
        write_blob_file(staging_directory, &blob_name, &wire_blob).await?;
        bytes_processed += bytes_read as u64;
        if let Some(cb) = progress {
            cb(bytes_processed, file_size);
        }
        chunk_records.push(ChunkRecord {
            chunk_id: Uuid::new_v4(),
            node_id: NodeId::new(Uuid::nil()),
            chunk_index,
            blob_name,
            size_padded: chunk_size_bytes,
            blake3_checksum: checksum.0,
            epoch_blob_id: None,
            byte_offset: None,
            byte_length: None,
        });

        if bytes_read < chunk_size_usize {
            break;
        }

        chunk_index = chunk_index
            .checked_add(1)
            .ok_or_else(|| StorageError::ConstraintViolation("chunk_index overflow".to_owned()))?;
    }

    Ok(chunk_records)
}

/// Reads up to one chunk from the source stream.
async fn read_chunk_plaintext(
    source_reader: &mut BufReader<File>,
    plaintext: &mut [u8],
) -> Result<usize, StorageError> {
    let mut bytes_read = 0usize;
    while bytes_read < plaintext.len() {
        let read_now = source_reader
            .read(&mut plaintext[bytes_read..])
            .await
            .map_err(|error| StorageError::Io(error.to_string()))?;
        if read_now == 0 {
            break;
        }
        bytes_read += read_now;
    }
    Ok(bytes_read)
}

/// Writes an encrypted blob to staging with the `.blob` extension.
async fn write_blob_file(
    staging_directory: &Path,
    blob_name: &str,
    wire_blob: &[u8],
) -> Result<(), StorageError> {
    let blob_path = staging_directory.join(format!("{blob_name}.blob"));
    let blob_file = File::create(blob_path)
        .await
        .map_err(|error| StorageError::Io(error.to_string()))?;
    let mut blob_writer = BufWriter::new(blob_file);
    blob_writer
        .write_all(wire_blob)
        .await
        .map_err(|error| StorageError::Io(error.to_string()))?;
    blob_writer
        .flush()
        .await
        .map_err(|error| StorageError::Io(error.to_string()))
}

/// Removes staged blobs after a partial encryption failure.
async fn cleanup_staged_blobs(staging_directory: &Path, blob_names: &[String]) {
    for blob_name in blob_names {
        let blob_path = staging_directory.join(format!("{blob_name}.blob"));
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

    use super::encrypt_file;
    use crate::crypto::{FileKey, compute_checksum};
    use crate::storage::MetadataStore;
    use crate::storage::error::StorageError;
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

    /// Verifies empty input produces no staged blobs.
    #[tokio::test]
    async fn test_encrypt_file_zero_byte_returns_empty_vec_no_staging_files() {
        let temp_dir = TempDir::new().expect("temporary directory should be created");
        let source_path = temp_dir.path().join("source.bin");
        let staging_directory = temp_dir.path().join("staging");
        fs::create_dir_all(&staging_directory)
            .await
            .expect("staging directory should be created");
        write_source_file(&source_path, &[]).await;
        let metadata_store = FixedMetaStore {
            chunk_size_bytes: 131_072,
        };
        let file_key = FileKey::from_bytes([3; 32]);

        let records = encrypt_file(
            &source_path,
            Uuid::new_v4(),
            &file_key,
            &metadata_store,
            &staging_directory,
            None,
        )
        .await
        .expect("encrypt_file should succeed");

        assert!(records.is_empty());
        let mut entries = fs::read_dir(&staging_directory)
            .await
            .expect("staging directory should be readable");
        let first_entry = entries
            .next_entry()
            .await
            .expect("staging directory iteration should succeed");
        assert!(first_entry.is_none());
    }

    /// Verifies one-byte input produces one fixed-size encrypted blob.
    #[tokio::test]
    async fn test_encrypt_file_one_byte_produces_one_blob_of_chunk_size_plus_forty() {
        let temp_dir = TempDir::new().expect("temporary directory should be created");
        let source_path = temp_dir.path().join("source.bin");
        let staging_directory = temp_dir.path().join("staging");
        fs::create_dir_all(&staging_directory)
            .await
            .expect("staging directory should be created");
        write_source_file(&source_path, &[0xA5]).await;
        let metadata_store = FixedMetaStore {
            chunk_size_bytes: 131_072,
        };
        let file_key = FileKey::from_bytes([7; 32]);

        let records = encrypt_file(
            &source_path,
            Uuid::new_v4(),
            &file_key,
            &metadata_store,
            &staging_directory,
            None,
        )
        .await
        .expect("encrypt_file should succeed");

        assert_eq!(records.len(), 1);
        let blob_record = &records[0];
        let parsed_blob_uuid =
            Uuid::parse_str(&blob_record.blob_name).expect("blob name should be UUID");
        assert_eq!(parsed_blob_uuid.get_version_num(), 4);
        let blob_path = staging_directory.join(format!("{}.blob", blob_record.blob_name));
        let blob_metadata = fs::metadata(blob_path)
            .await
            .expect("blob metadata should be readable");
        assert_eq!(blob_metadata.len(), 131_072 + 40);
    }

    /// Verifies chunk indices are sequential and checksums match staged blobs.
    #[tokio::test]
    async fn test_encrypt_file_chunk_index_monotonic_from_zero_and_checksums_match_blob_contents() {
        let temp_dir = TempDir::new().expect("temporary directory should be created");
        let source_path = temp_dir.path().join("source.bin");
        let staging_directory = temp_dir.path().join("staging");
        fs::create_dir_all(&staging_directory)
            .await
            .expect("staging directory should be created");
        let chunk_size = 131_072usize;
        let content = vec![0x5Au8; (2 * chunk_size) + 17];
        write_source_file(&source_path, &content).await;
        let metadata_store = FixedMetaStore {
            chunk_size_bytes: chunk_size as u64,
        };
        let file_key = FileKey::from_bytes([11; 32]);

        let records = encrypt_file(
            &source_path,
            Uuid::new_v4(),
            &file_key,
            &metadata_store,
            &staging_directory,
            None,
        )
        .await
        .expect("encrypt_file should succeed");

        assert_eq!(records.len(), 3);
        assert_eq!(records[0].chunk_index, 0);
        assert_eq!(records[1].chunk_index, 1);
        assert_eq!(records[2].chunk_index, 2);

        for record in &records {
            let blob_path = staging_directory.join(format!("{}.blob", record.blob_name));
            let blob_bytes = fs::read(blob_path)
                .await
                .expect("blob file should be readable");
            let computed = compute_checksum(&blob_bytes);
            assert_eq!(computed.0, record.blake3_checksum);
        }
    }
}
