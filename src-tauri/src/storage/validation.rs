//! Shared storage-domain validators.

use uuid::Uuid;

use crate::storage::error::StorageError;
use crate::storage::types::NodeType;

pub(crate) const IMMUTABLE_MANIFEST_META_KEYS: [&str; 5] = [
    "schema_version",
    "vault_id",
    "snapshot_counter",
    "chunk_size_bytes",
    "epoch_buffer_enabled",
];

pub(crate) fn immutable_meta_key_violation(key: &str) -> StorageError {
    StorageError::ConstraintViolation(format!(
        "manifest_meta key `{key}` is immutable; use increment_snapshot_counter for snapshot_counter"
    ))
}

pub(crate) fn is_immutable_manifest_meta_key(key: &str) -> bool {
    IMMUTABLE_MANIFEST_META_KEYS.contains(&key)
}

/// Validates a chunk `blob_name` is a parseable UUIDv4 string.
pub(crate) fn validate_blob_name_uuid_v4(blob_name: &str) -> Result<(), StorageError> {
    let parsed = Uuid::parse_str(blob_name).map_err(|_| {
        StorageError::ConstraintViolation("invalid blob_name: expected UUID v4".to_owned())
    })?;
    if parsed.get_version_num() != 4 {
        return Err(StorageError::ConstraintViolation(
            "invalid blob_name: expected UUID v4".to_owned(),
        ));
    }
    Ok(())
}

/// Validates chunks only target existing file nodes.
pub(crate) fn validate_chunk_target_node(node_type: Option<NodeType>) -> Result<(), StorageError> {
    match node_type {
        Some(NodeType::File) => Ok(()),
        Some(NodeType::Directory) => Err(StorageError::ConstraintViolation(
            "chunks can only target file nodes; directory node_id provided".to_owned(),
        )),
        None => Err(StorageError::ConstraintViolation(
            "missing node_id for chunk insert".to_owned(),
        )),
    }
}

/// Parses and validates canonical `chunk_size_bytes` metadata.
pub(crate) fn parse_chunk_size_bytes(value: &str) -> Result<u64, StorageError> {
    let chunk_size = value
        .parse::<u64>()
        .map_err(|_| StorageError::Database("invalid chunk_size_bytes: not an integer".to_owned()))?;
    if !(131_072..=67_108_864).contains(&chunk_size) {
        return Err(StorageError::Database(
            "invalid chunk_size_bytes: out of range".to_owned(),
        ));
    }
    Ok(chunk_size)
}

/// Validates chunk padding exactly matches vault `chunk_size_bytes`.
pub(crate) fn validate_size_padded_matches_chunk_size(
    size_padded: u64,
    chunk_size_bytes: u64,
) -> Result<(), StorageError> {
    if size_padded != chunk_size_bytes {
        return Err(StorageError::ConstraintViolation(format!(
            "invalid size_padded: expected chunk_size_bytes {chunk_size_bytes}, got {size_padded}"
        )));
    }
    Ok(())
}

/// Validates immutable metadata matches the expected value during `create`.
pub(crate) fn validate_immutable_meta_matches_expected(
    key: &str,
    expected: &str,
    actual: &str,
) -> Result<(), StorageError> {
    if expected != actual {
        return Err(StorageError::ConstraintViolation(format!(
            "immutable manifest_meta mismatch for `{key}`"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        immutable_meta_key_violation, is_immutable_manifest_meta_key, parse_chunk_size_bytes,
        validate_blob_name_uuid_v4, validate_chunk_target_node, validate_immutable_meta_matches_expected,
        validate_size_padded_matches_chunk_size,
    };
    use crate::storage::error::StorageError;
    use crate::storage::types::NodeType;

    #[test]
    fn test_validate_blob_name_uuid_v4_rejects_invalid_uuid() {
        let result = validate_blob_name_uuid_v4("not-a-uuid");
        assert!(matches!(
            result,
            Err(StorageError::ConstraintViolation(message)) if message.contains("blob_name")
        ));
    }

    #[test]
    fn test_validate_blob_name_uuid_v4_rejects_non_v4_uuid() {
        let result = validate_blob_name_uuid_v4("f81d4fae-7dec-11d0-a765-00a0c91e6bf6");
        assert!(matches!(
            result,
            Err(StorageError::ConstraintViolation(message)) if message.contains("UUID v4")
        ));
    }

    #[test]
    fn test_validate_chunk_target_node_rejects_directory() {
        let result = validate_chunk_target_node(Some(NodeType::Directory));
        assert!(matches!(
            result,
            Err(StorageError::ConstraintViolation(message)) if message.contains("directory")
        ));
    }

    #[test]
    fn test_validate_chunk_target_node_rejects_missing_node() {
        let result = validate_chunk_target_node(None);
        assert!(matches!(
            result,
            Err(StorageError::ConstraintViolation(message)) if message.contains("missing")
        ));
    }

    #[test]
    fn test_validate_size_padded_matches_chunk_size_rejects_mismatch() {
        let result = validate_size_padded_matches_chunk_size(1024, 4096);
        assert!(matches!(
            result,
            Err(StorageError::ConstraintViolation(message)) if message.contains("size_padded")
        ));
    }

    #[test]
    fn test_parse_chunk_size_bytes_rejects_out_of_range() {
        let result = parse_chunk_size_bytes("1");
        assert!(matches!(
            result,
            Err(StorageError::Database(message)) if message.contains("chunk_size_bytes")
        ));
    }

    #[test]
    fn test_validate_immutable_meta_matches_expected_rejects_mismatch() {
        let result = validate_immutable_meta_matches_expected("vault_id", "a", "b");
        assert!(matches!(
            result,
            Err(StorageError::ConstraintViolation(message)) if message.contains("vault_id")
        ));
    }

    #[test]
    fn test_immutable_manifest_meta_helpers() {
        assert!(is_immutable_manifest_meta_key("chunk_size_bytes"));
        assert!(matches!(
            immutable_meta_key_violation("chunk_size_bytes"),
            StorageError::ConstraintViolation(message) if message.contains("immutable")
        ));
    }
}
