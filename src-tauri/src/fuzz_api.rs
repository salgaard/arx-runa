//! Public wrappers for `cargo-fuzz` targets.
//!
//! Only compiled when the `fuzzing` feature flag is active. The wrapped
//! functions are `pub(crate)` in production; thin wrappers here expose them
//! to the external fuzz crate without changing their production visibility.
//!
//! # Safety
//!
//! The `fuzzing` feature must never be enabled in production or release builds.
//! It widens the crate's API surface for testing purposes only.

use crate::crypto::error::CryptoError;
use crate::crypto::types::VaultId;
use zeroize::Zeroizing;

/// Wraps `storage::cloud::manifest_backup::decrypt_manifest_backup` for fuzzing.
///
/// Parses and decrypts a wire-format manifest backup blob. The wire format is
/// `[24-byte nonce | ciphertext | 16-byte tag]`. Adversarial inputs must not
/// cause panics; only `CryptoError::DecryptionFailed` is expected on bad input.
pub fn decrypt_manifest_backup(
    wire: &[u8],
    manifest_key: &[u8; 32],
    vault_id: &VaultId,
) -> Result<Zeroizing<Vec<u8>>, CryptoError> {
    crate::storage::cloud::manifest_backup::decrypt_manifest_backup(wire, manifest_key, vault_id)
}

/// Wraps `storage::validation::parse_chunk_size_bytes` for fuzzing.
///
/// Parses a `chunk_size_bytes` string from `manifest_meta` into a `u64`
/// with range validation. Must not panic on arbitrary UTF-8 input.
pub fn parse_chunk_size_bytes(value: &str) -> Result<u64, crate::storage::error::StorageError> {
    crate::storage::validation::parse_chunk_size_bytes(value)
}
