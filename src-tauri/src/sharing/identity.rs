//! Identity helpers for public-key export and fingerprinting.

use base64::Engine;
use sha2::{Digest, Sha256};

use crate::sharing::types::{Fingerprint, X25519PublicKey};

/// Returns the raw 32-byte public key for file export.
pub fn export_public_key_bytes(public_key: &X25519PublicKey) -> [u8; 32] {
    *public_key.as_bytes()
}

/// Encodes the public key as standard padded base64 for QR payloads.
pub fn public_key_qr_string(public_key: &X25519PublicKey) -> String {
    base64::engine::general_purpose::STANDARD.encode(public_key.as_bytes())
}

/// Computes the sharing fingerprint from the first 8 bytes of SHA-256.
pub fn compute_fingerprint(public_key: &X25519PublicKey) -> Fingerprint {
    let mut hasher = Sha256::new();
    hasher.update(public_key.as_bytes());
    let digest = hasher.finalize();
    let mut fingerprint_bytes = [0u8; 8];
    fingerprint_bytes.copy_from_slice(&digest[0..8]);
    Fingerprint::new(fingerprint_bytes)
}

#[cfg(test)]
mod tests {
    use base64::Engine;

    use crate::sharing::identity::{
        compute_fingerprint, export_public_key_bytes, public_key_qr_string,
    };
    use crate::sharing::types::X25519PublicKey;

    /// Verifies the fingerprint uses the first eight bytes of SHA-256.
    #[test]
    fn test_identity_compute_fingerprint_uses_expected_test_vector() {
        let public_key = X25519PublicKey::new([0u8; 32]);
        let fingerprint = compute_fingerprint(&public_key);

        assert_eq!(fingerprint.to_hex_lowercase(), "66687aadf862bd77");
    }

    /// Verifies raw public-key export returns the original bytes unchanged.
    #[test]
    fn test_identity_export_public_key_bytes_returns_exact_input_bytes() {
        let public_key = X25519PublicKey::new([42u8; 32]);
        let exported = export_public_key_bytes(&public_key);

        assert_eq!(exported, [42u8; 32]);
    }

    /// Verifies QR payload base64 decodes back to the original key bytes.
    #[test]
    fn test_identity_public_key_qr_string_decodes_back_to_original_bytes() {
        let public_key = X25519PublicKey::new([7u8; 32]);
        let encoded = public_key_qr_string(&public_key);
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(encoded.as_bytes())
            .expect("base64 decode should succeed");

        assert_eq!(decoded, [7u8; 32]);
    }

    /// Verifies distinct public keys produce distinct fingerprints.
    #[test]
    fn test_identity_compute_fingerprint_for_distinct_keys_returns_distinct_values() {
        let first_public_key = X25519PublicKey::new([1u8; 32]);
        let second_public_key = X25519PublicKey::new([2u8; 32]);

        assert_ne!(
            compute_fingerprint(&first_public_key),
            compute_fingerprint(&second_public_key)
        );
    }
}
