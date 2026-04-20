//! Sharing storage abstractions.

use async_trait::async_trait;

use crate::sharing::error::SharingError;
use crate::sharing::types::{ContactId, DisplayName, X25519PublicKey};

/// Contact row shape persisted in the `contacts` table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Contact {
    /// Stable contact UUID.
    pub contact_id: ContactId,
    /// User-facing display label.
    pub display_name: DisplayName,
    /// Optional display-only e-mail field.
    pub email: Option<String>,
    /// X25519 recipient public key.
    pub public_key: X25519PublicKey,
    /// Unix timestamp when the contact was created.
    pub created_at: i64,
}

/// Domain representation of a row in the `received_shares` table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReceivedShare {
    /// Unique share identifier (UUID v4 hyphenated).
    pub share_id: String,
    /// Contact identifier of the sender, or `None` if the sender is not in local contacts.
    pub sender_contact_id: Option<ContactId>,
    /// X25519 public key of the share sender.
    pub sender_public_key: X25519PublicKey,
    /// Original file name from the share package.
    pub file_name: String,
    /// File key wrapped with the local key-encryption key (72 bytes).
    pub file_key_wrapped: [u8; 72],
    /// Number of chunks in the shared file.
    pub chunk_count: u32,
    /// Chunk size in bytes.
    pub chunk_size: u32,
    /// Ordered blob-name UUIDs for each chunk (UUID v4 hyphenated).
    pub chunk_uuids: Vec<String>,
    /// Cloud endpoint metadata for locating the shared blobs.
    pub cloud_endpoint: serde_json::Value,
    /// Optional Unix timestamp when the share expires.
    pub expires_at: Option<i64>,
    /// Unix timestamp when the share was imported locally.
    pub imported_at: i64,
}

/// Persistence boundary for identity, contacts, and received-shares operations.
#[async_trait]
pub trait SharingStore: Send + Sync {
    /// Returns the vault owner's X25519 public key.
    async fn get_own_public_key(&self) -> Result<X25519PublicKey, SharingError>;

    /// Inserts one contact row.
    async fn insert_contact(&self, contact: &Contact) -> Result<(), SharingError>;

    /// Fetches one contact row by identifier.
    async fn get_contact(&self, contact_id: ContactId) -> Result<Contact, SharingError>;

    /// Lists all contacts in deterministic order.
    async fn list_contacts(&self) -> Result<Vec<Contact>, SharingError>;

    /// Deletes one contact row by identifier.
    async fn delete_contact(&self, contact_id: ContactId) -> Result<(), SharingError>;

    /// Inserts one received-share row.
    async fn insert_received_share(&self, row: &ReceivedShare) -> Result<(), SharingError>;

    /// Fetches one received-share row by share identifier.
    async fn get_received_share(&self, share_id: &str) -> Result<ReceivedShare, SharingError>;

    /// Lists all received shares in deterministic order.
    async fn list_received_shares(&self) -> Result<Vec<ReceivedShare>, SharingError>;
}
