//! Error types for the crypto module.

use thiserror::Error;

/// Errors produced by the crypto module.
///
/// Variants are introduced in Phase 1.1 so later phases can consume a stable
/// error surface.
#[non_exhaustive]
#[derive(Debug, Error)]
pub enum CryptoError {
    /// XChaCha20-Poly1305 authentication failed.
    #[error("decryption failed: authentication tag mismatch")]
    DecryptionFailed,

    /// The encrypted blob did not match the expected wire format shape.
    #[error("invalid blob format: expected at least {expected} bytes, got {actual}")]
    InvalidBlobFormat { expected: usize, actual: usize },

    /// File-key unwrapping failed.
    #[error("key unwrap failed")]
    KeyUnwrapFailed,

    /// File-key wrapping failed (AEAD encryption error).
    #[error("key wrap failed")]
    KeyWrapFailed,

    /// Chunk AEAD encryption failed.
    #[error("chunk encryption failed")]
    EncryptionFailed,

    /// HKDF-SHA256 key derivation failed.
    #[error("key derivation failed")]
    KeyDerivationFailed,

    /// BLAKE3 checksum verification failed for the encrypted blob.
    #[error("checksum mismatch: blob has been tampered with or corrupted")]
    ChecksumMismatch,
}

#[cfg(test)]
mod tests {
    use super::CryptoError;

    #[test]
    fn test_crypto_error_variants_constructible() {
        let _decryption_failed = CryptoError::DecryptionFailed;
        let _invalid_blob_format = CryptoError::InvalidBlobFormat {
            expected: 40,
            actual: 10,
        };
        let _key_unwrap_failed = CryptoError::KeyUnwrapFailed;
        let _key_wrap_failed = CryptoError::KeyWrapFailed;
        let _encryption_failed = CryptoError::EncryptionFailed;
        let _key_derivation_failed = CryptoError::KeyDerivationFailed;
        let _checksum_mismatch = CryptoError::ChecksumMismatch;
    }

    #[test]
    fn test_crypto_error_display_formats_expected_message() {
        let error = CryptoError::InvalidBlobFormat {
            expected: 40,
            actual: 10,
        };

        assert_eq!(
            error.to_string(),
            "invalid blob format: expected at least 40 bytes, got 10"
        );
    }
}
