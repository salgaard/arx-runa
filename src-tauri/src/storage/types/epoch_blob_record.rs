use uuid::Uuid;

/// Domain representation of a row in the `epoch_blobs` table.
#[derive(Debug, Clone)]
pub struct EpochBlobRecord {
    /// Primary key of the epoch blob row.
    pub epoch_blob_id: Uuid,
    /// Cloud blob name (UUID v4).
    pub blob_name: String,
    /// File key wrapped with the key-encryption key.
    pub file_key_wrapped: Vec<u8>,
    /// Padded encrypted blob size in bytes.
    pub size_padded: u64,
    /// BLAKE3 checksum over encrypted blob bytes.
    pub blake3_checksum: [u8; 32],
}
