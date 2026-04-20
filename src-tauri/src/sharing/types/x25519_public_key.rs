//! X25519 public-key newtype.

use std::fmt::{Debug, Formatter};

/// Strongly typed 32-byte X25519 public key.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct X25519PublicKey([u8; 32]);

impl X25519PublicKey {
    /// Constructs a public key from raw bytes.
    pub fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Returns the inner public-key bytes.
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl Debug for X25519PublicKey {
    /// Renders a redacted debug representation without key bytes.
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("X25519PublicKey")
            .field("bytes", &"<redacted>")
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use crate::sharing::types::X25519PublicKey;

    /// Verifies byte access returns the original key payload.
    #[test]
    fn test_x25519_public_key_new_and_as_bytes_round_trip_preserves_bytes() {
        let key_bytes = [5u8; 32];
        let public_key = X25519PublicKey::new(key_bytes);

        assert_eq!(public_key.as_bytes(), &key_bytes);
    }

    /// Verifies debug rendering does not expose public-key bytes.
    #[test]
    fn test_x25519_public_key_debug_redacts_raw_key_bytes() {
        let public_key = X25519PublicKey::new([1u8; 32]);
        let debug_output = format!("{public_key:?}");

        assert!(debug_output.contains("<redacted>"));
        assert!(!debug_output.contains("1, 1, 1"));
    }
}
