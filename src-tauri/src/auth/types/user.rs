//! AuthUser adapter type for auth↔storage boundary.
//!
//! Encapsulates minimal fields needed by auth logic; maps to storage schema at boundary.
//! Decouples auth ceremonies from storage schema evolution.

use async_trait::async_trait;

use crate::crypto::VaultId;
use crate::storage::error::StorageError;

/// Minimal user representation for auth ceremonies.
///
/// Decouples auth logic from storage schema changes by encapsulating only the fields
/// that auth ceremonies actually require. Storage-specific fields (created_at, updated_at, etc.)
/// are excluded and managed by the storage layer.
#[derive(Clone, Debug)]
pub struct AuthUser {
    /// Vault identifier (UUID v4)
    pub vault_id: VaultId,

    /// Argon2id salt (16 bytes) for key derivation
    pub salt: [u8; 16],

    /// Optional BLAKE3 hash of the USB key file (32 bytes) for Tier 2 vaults
    pub key_file_hash: Option<[u8; 32]>,
}

/// Trait for auth-related user storage operations.
///
/// Implementations handle the conversion between `AuthUser` and the storage schema,
/// ensuring that auth logic remains decoupled from storage implementation details.
///
/// All methods are async to support blocking operations in separate thread pools
/// (e.g., SQLCipher operations in `tokio::task::spawn_blocking`).
#[async_trait]
pub trait AuthUserStore: Send + Sync {
    /// Create a new vault user entry (post-authentication).
    ///
    /// Called during `create_vault` ceremony after the vault header is uploaded
    /// and the manifest database is initialized. Persists the auth metadata needed
    /// for future authentication.
    ///
    /// # Errors
    /// - `StorageError::ConstraintViolation` if vault already exists
    /// - Other storage errors on database failures
    async fn create_vault_user(&self, user: AuthUser) -> Result<(), StorageError>;

    /// Update password hash and salt (after re-authentication).
    ///
    /// Called during `change_password` ceremony to persist the new salt and
    /// re-derived authentication metadata without changing the vault_id.
    ///
    /// # Errors
    /// - `StorageError::NotFound` if vault does not exist
    /// - Other storage errors on database failures
    async fn update_password(&self, vault_id: &VaultId, salt: [u8; 16]) -> Result<(), StorageError>;

    /// Rotate key file hash (post-authentication).
    ///
    /// Called during `rotate_key_file` ceremony to update the stored USB key file
    /// hash without changing the password salt.
    ///
    /// # Errors
    /// - `StorageError::NotFound` if vault does not exist
    /// - Other storage errors on database failures
    async fn rotate_key_file(
        &self,
        vault_id: &VaultId,
        key_file_hash: [u8; 32],
    ) -> Result<(), StorageError>;

    /// Look up user by vault ID (for session establishment).
    ///
    /// Retrieves minimal user data needed to validate credentials during authentication.
    /// Called when establishing a session to obtain the salt and key file hash
    /// needed for key derivation.
    ///
    /// # Errors
    /// - `StorageError::NotFound` if vault does not exist
    /// - Other storage errors on database failures
    async fn get_user(&self, vault_id: &VaultId) -> Result<AuthUser, StorageError>;
}
