//! CSPRNG generation of per-file encryption keys.

use crate::crypto::types::FileKey;
use rand::Rng;
use secrecy::SecretBox;

/// Generates a cryptographically random 256-bit file key.
///
/// The CSPRNG fills the `SecretBox` heap buffer directly via
/// `RngCore::fill_bytes`, so the raw key bytes never exist in a stack
/// local. `SecretBox`'s `Drop` zeroizes the buffer if the key is dropped.
pub fn generate_file_key() -> FileKey {
    let secret_box = SecretBox::<[u8; 32]>::init_with_mut(|buffer| {
        rand::rng().fill_bytes(buffer.as_mut_slice());
    });
    FileKey::from_secret_box(secret_box)
}

#[cfg(test)]
mod tests {
    use super::generate_file_key;

    #[test]
    fn test_generate_file_key_not_all_zeros() {
        let file_key = generate_file_key();
        assert_ne!(*file_key.expose(), [0u8; 32]);
    }

    #[test]
    fn test_generate_file_key_consecutive_calls_produce_different_keys() {
        let first = generate_file_key();
        let second = generate_file_key();
        assert_ne!(first.expose(), second.expose());
    }
}
