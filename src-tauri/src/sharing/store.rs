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

/// Persistence boundary for identity and contacts operations.
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
}
