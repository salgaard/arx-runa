//! BLAKE3 checksums over encrypted blobs and the `VerifiedBlob` type that
//! enforces check-before-decrypt at compile time.

use crate::crypto::error::CryptoError;
use crate::crypto::types::Blake3Hash;

/// Computes a BLAKE3 checksum over an encrypted blob.
///
/// The checksum is computed over **ciphertext**, not plaintext. This allows
/// integrity verification of a downloaded blob before the more expensive
/// XChaCha20-Poly1305 decryption runs.
pub fn compute_checksum(encrypted_blob: &[u8]) -> Blake3Hash {
    let hash = blake3::hash(encrypted_blob);
    Blake3Hash(*hash.as_bytes())
}

/// Verifies the BLAKE3 checksum of an encrypted blob and produces a
/// `VerifiedBlob` that `decrypt_chunk` accepts.
///
/// The `blob` is consumed so a single `Vec<u8>` allocation flows from the
/// download boundary through verification into the decryption step.
///
/// # Errors
/// Returns `CryptoError::ChecksumMismatch` if the computed checksum does not
/// match `expected`. The blob is dropped in that case.
pub fn verify_checksum(blob: Vec<u8>, expected: &Blake3Hash) -> Result<VerifiedBlob, CryptoError> {
    let computed = compute_checksum(&blob);
    if computed.0 == expected.0 {
        Ok(VerifiedBlob(blob))
    } else {
        Err(CryptoError::ChecksumMismatch)
    }
}

/// An encrypted blob whose BLAKE3 checksum has been verified.
///
/// Only constructible via `verify_checksum`. `decrypt_chunk` accepts only
/// `VerifiedBlob` as its first parameter, making it a compile error to skip
/// the checksum step.
#[derive(Debug)]
pub struct VerifiedBlob(Vec<u8>);

impl VerifiedBlob {
    /// Consumes the wrapper and returns the underlying blob bytes.
    ///
    /// `pub(crate)` — callable only from within the `crypto` module, so
    /// consumers outside the module cannot bypass `verify_checksum`.
    pub(crate) fn into_inner(self) -> Vec<u8> {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::{VerifiedBlob, compute_checksum, verify_checksum};
    use crate::crypto::error::CryptoError;
    use assert_matches::assert_matches;

    #[test]
    fn test_compute_checksum_determinism_for_identical_input() {
        let blob = vec![0xABu8; 256];
        let first = compute_checksum(&blob);
        let second = compute_checksum(&blob);
        assert_eq!(first.0, second.0);
    }

    #[test]
    fn test_compute_checksum_detects_corruption_from_single_byte_flip() {
        let mut blob = vec![0xCDu8; 128];
        let before = compute_checksum(&blob);

        blob[42] ^= 0x01;
        let after = compute_checksum(&blob);

        assert_ne!(before.0, after.0);
    }

    #[test]
    fn test_compute_checksum_empty_input_is_well_defined_and_stable() {
        let first = compute_checksum(&[]);
        let second = compute_checksum(&[]);
        assert_eq!(first.0, second.0);
    }

    #[test]
    fn test_verify_checksum_success_returns_verified_blob_with_original_bytes() {
        let blob = vec![1u8, 2, 3, 4, 5];
        let expected = compute_checksum(&blob);
        let verified =
            verify_checksum(blob.clone(), &expected).expect("matching checksum must verify");
        assert_eq!(verified.into_inner(), blob);
    }

    #[test]
    fn test_verify_checksum_rejects_tampered_blob_with_checksum_mismatch() {
        let original = vec![0xAAu8; 64];
        let expected = compute_checksum(&original);

        let mut tampered = original.clone();
        tampered[10] ^= 0x01;

        let result = verify_checksum(tampered, &expected);
        assert_matches!(result, Err(CryptoError::ChecksumMismatch));
    }

    #[test]
    fn test_verify_checksum_rejects_wrong_expected_hash() {
        let blob = vec![0xFFu8; 32];
        let wrong_expected = compute_checksum(&vec![0x00u8; 32]);

        let result = verify_checksum(blob, &wrong_expected);
        assert_matches!(result, Err(CryptoError::ChecksumMismatch));
    }

    #[test]
    fn test_verified_blob_into_inner_returns_original_bytes() {
        let bytes = vec![7u8, 8, 9];
        let expected = compute_checksum(&bytes);
        let verified: VerifiedBlob =
            verify_checksum(bytes.clone(), &expected).expect("must verify");
        assert_eq!(verified.into_inner(), bytes);
    }
}

#[cfg(test)]
mod proptests {
    use super::compute_checksum;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn prop_checksum_deterministic(
            blob in proptest::collection::vec(any::<u8>(), 0..=64 * 1024),
        ) {
            let first = compute_checksum(&blob);
            let second = compute_checksum(&blob);
            prop_assert_eq!(first.0, second.0);
        }
    }
}
