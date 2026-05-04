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
    /// File node identifier (UUID v4 hyphenated), used to reconstruct chunk AAD.
    pub file_id: String,
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

/// Domain representation of a row in the `shares` (outgoing shares) table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShareRecord {
    /// Unique share identifier (UUID v4 hyphenated).
    pub share_id: String,
    /// File node identifier (UUID v4 hyphenated, `nodes.node_id`).
    pub file_id: String,
    /// Recipient contact identifier.
    pub contact_id: ContactId,
    /// Groups all recipients of the same file version under one shared-blob prefix.
    pub file_share_id: String,
    /// Cloud path prefix for shared blobs (format: `shared/<file_share_id>/`).
    pub cloud_path: String,
    /// Unix timestamp when the share was created.
    pub created_at: i64,
    /// Optional Unix timestamp when the share expires (`None` = no expiry).
    pub expires_at: Option<i64>,
    /// Unix timestamp when the share was revoked (`None` = still active).
    pub revoked_at: Option<i64>,
    /// B2 scoped application key identifier for this share, if one was generated.
    pub download_key_id: Option<String>,
    /// Whether the recipient was asked to send a receipt after downloading.
    pub receipt_requested: bool,
    /// Unix timestamp when the receipt was received (`None` = not yet received).
    pub receipt_received_at: Option<i64>,
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

    /// Inserts one outgoing share row.
    async fn insert_share(&self, share: &ShareRecord) -> Result<(), SharingError>;

    /// Fetches one outgoing share row by share identifier.
    async fn get_share(&self, share_id: &str) -> Result<ShareRecord, SharingError>;

    /// Lists all share rows for a given file, in deterministic order.
    async fn list_shares_by_file(&self, file_id: &str) -> Result<Vec<ShareRecord>, SharingError>;

    /// Lists only active (non-revoked) share rows for a given file.
    async fn list_active_shares_by_file(
        &self,
        file_id: &str,
    ) -> Result<Vec<ShareRecord>, SharingError>;

    /// Lists only active share rows for a given `file_share_id`.
    async fn list_active_shares_by_file_share_id(
        &self,
        file_share_id: &str,
    ) -> Result<Vec<ShareRecord>, SharingError>;

    /// Sets `revoked_at` timestamp on a share row (only if currently active).
    ///
    /// Returns `ShareNotFound` if the share_id does not exist, or
    /// `ShareAlreadyRevoked` if `revoked_at IS NOT NULL`.
    async fn set_share_revoked_at(
        &self,
        share_id: &str,
        revoked_at: i64,
    ) -> Result<(), SharingError>;
}
