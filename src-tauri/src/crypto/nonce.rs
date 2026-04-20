//! CSPRNG nonce generation for XChaCha20-Poly1305.

use rand::RngExt;

/// Generates a random 24-byte nonce.
pub fn generate_nonce() -> [u8; 24] {
    rand::rng().random::<[u8; 24]>()
}

#[cfg(test)]
mod tests {
    use super::generate_nonce;
    use std::collections::HashSet;

    #[test]
    fn test_generate_nonce_returns_twenty_four_bytes() {
        let nonce = generate_nonce();
        assert_eq!(nonce.len(), 24);
    }

    #[test]
    fn test_generate_nonce_thousand_samples_are_unique() {
        let mut seen = HashSet::with_capacity(1000);

        for _ in 0..1000 {
            let nonce = generate_nonce();
            assert!(
                seen.insert(nonce),
                "nonce collision in 1000-sample test, expected uniqueness"
            );
        }

        assert_eq!(seen.len(), 1000);
    }

    #[test]
    fn test_generate_nonce_non_zero_and_not_repeated() {
        let first = generate_nonce();
        let second = generate_nonce();

        assert_ne!(first, [0u8; 24]);
        assert_ne!(second, [0u8; 24]);
        assert_ne!(first, second);
    }
}
