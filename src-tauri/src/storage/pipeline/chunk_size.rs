use crate::storage::MetadataStore;
use crate::storage::error::StorageError;
use crate::storage::validation::parse_chunk_size_bytes;

/// Reads and validates `chunk_size_bytes` from manifest metadata.
pub(crate) async fn read_chunk_size_bytes(
    metadata_store: &dyn MetadataStore,
) -> Result<u64, StorageError> {
    let chunk_size_text = metadata_store
        .get_meta("chunk_size_bytes")
        .await?
        .ok_or_else(|| {
            StorageError::Database("missing manifest_meta key: chunk_size_bytes".to_owned())
        })?;
    parse_chunk_size_bytes(&chunk_size_text)
}
