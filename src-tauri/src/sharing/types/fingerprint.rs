//! Fingerprint newtype.

/// Truncated SHA-256 fingerprint bytes used for UI identity checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fingerprint([u8; 8]);

impl Fingerprint {
    /// Constructs a fingerprint from eight bytes.
    pub fn new(bytes: [u8; 8]) -> Self {
        Self(bytes)
    }

    /// Encodes the fingerprint as 16 lowercase hexadecimal characters.
    pub fn to_hex_lowercase(&self) -> String {
        hex::encode(self.0)
    }

    /// Returns the inner eight-byte fingerprint payload.
    pub fn as_bytes(&self) -> &[u8; 8] {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use crate::sharing::types::Fingerprint;

    /// Verifies hexadecimal rendering is 16 lowercase characters.
    #[test]
    fn test_fingerprint_to_hex_lowercase_returns_16_lowercase_hex_characters() {
        let fingerprint = Fingerprint::new([0xAB; 8]);
        let rendered = fingerprint.to_hex_lowercase();

        assert_eq!(rendered.len(), 16);
        assert!(
            rendered
                .chars()
                .all(|character| character.is_ascii_hexdigit())
        );
        assert_eq!(rendered, "abababababababab");
    }
}
