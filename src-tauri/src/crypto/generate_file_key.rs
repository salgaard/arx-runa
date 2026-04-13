//! CSPRNG generation of per-file encryption keys.

use crate::crypto::types::FileKey;
use rand::RngExt;
use secrecy::SecretBox;

/// Generates a cryptographically random 256-bit file key.
///
/// The key is produced by `rand::rng().random::<[u8; 32]>()` and immediately
/// moved into a `SecretBox` so the raw bytes never outlive this function.
pub fn generate_file_key() -> FileKey {
    let random_bytes: [u8; 32] = rand::rng().random::<[u8; 32]>();
    FileKey::from_secret_box(SecretBox::new(Box::new(random_bytes)))
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
