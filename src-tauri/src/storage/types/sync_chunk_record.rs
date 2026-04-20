/// Sync-specific chunk projection used by cloud push/pull orchestration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncChunkRecord {
    /// Canonical UUIDv4 blob identifier.
    pub blob_name: String,
    /// Expected BLAKE3 checksum for the encrypted blob.
    pub blake3_checksum: [u8; 32],
}
