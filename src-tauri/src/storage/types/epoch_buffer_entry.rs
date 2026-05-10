use uuid::Uuid;

/// Domain representation of a staged plaintext entry in the epoch buffer.
#[derive(Debug, Clone)]
pub struct EpochBufferEntry {
    /// Primary key of the buffer entry.
    pub entry_id: Uuid,
    /// Owning file node identifier.
    pub node_id: Uuid,
    /// Raw plaintext bytes for this file.
    pub plaintext: Vec<u8>,
    /// Size of the plaintext in bytes.
    pub size_bytes: u64,
}
