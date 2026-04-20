use uuid::Uuid;

use super::NodeId;

/// Domain representation of a row in the `chunks` table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChunkRecord {
    /// Primary key of the chunk row.
    pub chunk_id: Uuid,
    /// Owning file node identifier.
    pub node_id: NodeId,
    /// Zero-based chunk position within a file.
    pub chunk_index: u32,
    /// Cloud blob name (UUID v4).
    pub blob_name: String,
    /// Padded encrypted chunk size in bytes.
    pub size_padded: u64,
    /// BLAKE3 checksum over encrypted blob bytes.
    pub blake3_checksum: [u8; 32],
}
