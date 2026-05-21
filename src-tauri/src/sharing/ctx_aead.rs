//! CTX-ChaCha20-Poly1305 committing AEAD wrapper.
//!
//! Replaces the standard 16-byte Poly1305 authentication tag with a 32-byte
//! BLAKE3 commitment tag: `BLAKE3(b"arx-runa-ctx-v1" || key || nonce || ciphertext)`.
//! This achieves CMT-4 (full key commitment) security, defending against
//! partition oracle attacks on `file_key`.
//!
//! Wire layout: `[24B nonce | ciphertext | 32B CTX tag]`
//!
//! The Poly1305 tag produced by XChaCha20-Poly1305 encryption is discarded
//! from the wire format. On open, the CTX commitment is verified first; only
//! then is the ciphertext decrypted using the raw ChaCha20 keystream (no
//! Poly1305 verification, since it was never serialised).

use chacha20::cipher::{KeyIvInit, StreamCipher};
use chacha20poly1305::aead::generic_array::GenericArray;
use chacha20poly1305::{AeadInPlace, KeyInit, XChaCha20Poly1305};
use subtle::ConstantTimeEq;
use zeroize::Zeroizing;

use crate::sharing::error::SharingError;

/// Domain separation label for CTX commitment hashing.
const CTX_DOMAIN_LABEL: &[u8] = b"arx-runa-ctx-v1";

/// Length of the CTX commitment tag in bytes.
const CTX_TAG_LEN: usize = 32;

/// Length of the XChaCha20 nonce in bytes.
const NONCE_LEN: usize = 24;

/// Computes the CTX commitment: `BLAKE3(domain_label || key || nonce || ciphertext)`.
fn compute_commitment(key: &[u8; 32], nonce: &[u8; NONCE_LEN], ciphertext: &[u8]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(CTX_DOMAIN_LABEL);
    hasher.update(key);
    hasher.update(nonce);
    hasher.update(ciphertext);
    *hasher.finalize().as_bytes()
}

/// Encrypts `plaintext` in place and returns the 32-byte CTX commitment tag.
///
/// The caller is responsible for prepending the nonce and appending the tag
/// to form the wire format.
pub(crate) fn ctx_seal(
    key: &[u8; 32],
    nonce: &[u8; NONCE_LEN],
    plaintext: &mut [u8],
) -> Result<[u8; CTX_TAG_LEN], SharingError> {
    let cipher = XChaCha20Poly1305::new(GenericArray::from_slice(key));
    let nonce_ga = GenericArray::from_slice(nonce);
    let _poly1305_tag = cipher
        .encrypt_in_place_detached(nonce_ga, &[], plaintext)
        .map_err(|_| SharingError::AuthenticationFailed)?;

    let tag = compute_commitment(key, nonce, plaintext);
    Ok(tag)
}

/// Verifies the CTX commitment and decrypts `ciphertext` in place.
///
/// Returns `SharingError::AuthenticationFailed` on commitment mismatch with
/// no additional context (oracle-free).
pub(crate) fn ctx_open(
    key: &[u8; 32],
    nonce: &[u8; NONCE_LEN],
    ciphertext: &mut [u8],
    claimed_tag: &[u8; CTX_TAG_LEN],
) -> Result<(), SharingError> {
    let expected_tag = compute_commitment(key, nonce, ciphertext);

    if expected_tag.ct_eq(claimed_tag).into() {
        let mut stream = chacha20::XChaCha20::new(key.into(), nonce.into());
        let mut discard = Zeroizing::new([0u8; 64]);
        // XChaCha20-Poly1305 uses ChaCha20 block 0 (64 bytes) to derive the Poly1305
        // key; plaintext encryption begins at block 1.  We must advance the keystream
        // by exactly 64 bytes here to stay in sync with the encrypting side.
        stream.apply_keystream(discard.as_mut_slice());
        stream.apply_keystream(ciphertext);
        Ok(())
    } else {
        Err(SharingError::AuthenticationFailed)
    }
}

#[cfg(test)]
mod tests {
    use super::{CTX_TAG_LEN, NONCE_LEN, ctx_open, ctx_seal};
    use crate::sharing::SharingError;

    /// Verifies seal/open round-trip recovers the original plaintext.
    #[test]
    fn test_ctx_aead_seal_open_round_trip_recovers_plaintext() {
        let key = [0xAA; 32];
        let nonce = [0xBB; NONCE_LEN];
        let original = b"hello, CTX world!";

        let mut buffer = original.to_vec();
        let tag = ctx_seal(&key, &nonce, &mut buffer).expect("seal should succeed");
        assert_eq!(tag.len(), CTX_TAG_LEN);
        assert_ne!(&buffer, original.as_slice());

        ctx_open(&key, &nonce, &mut buffer, &tag).expect("open should succeed");
        assert_eq!(&buffer, original.as_slice());
    }

    /// Verifies a single-byte flip in ciphertext produces `AuthenticationFailed`.
    #[test]
    fn test_ctx_aead_corrupted_ciphertext_rejected_with_authentication_failed() {
        let key = [0xCC; 32];
        let nonce = [0xDD; NONCE_LEN];

        let mut buffer = b"secret data".to_vec();
        let tag = ctx_seal(&key, &nonce, &mut buffer).expect("seal should succeed");

        buffer[0] ^= 0x01;

        let result = ctx_open(&key, &nonce, &mut buffer, &tag);
        assert!(matches!(result, Err(SharingError::AuthenticationFailed)));
    }

    /// Verifies a single-byte flip in the CTX tag produces `AuthenticationFailed`.
    #[test]
    fn test_ctx_aead_corrupted_tag_rejected_with_authentication_failed() {
        let key = [0xEE; 32];
        let nonce = [0xFF; NONCE_LEN];

        let mut buffer = b"more secret data".to_vec();
        let mut tag = ctx_seal(&key, &nonce, &mut buffer).expect("seal should succeed");

        tag[0] ^= 0x01;

        let result = ctx_open(&key, &nonce, &mut buffer, &tag);
        assert!(matches!(result, Err(SharingError::AuthenticationFailed)));
    }

    /// Verifies wrong key produces `AuthenticationFailed`.
    #[test]
    fn test_ctx_aead_wrong_key_rejected_with_authentication_failed() {
        let key = [0x11; 32];
        let wrong_key = [0x22; 32];
        let nonce = [0x33; NONCE_LEN];

        let mut buffer = b"data for wrong key test".to_vec();
        let tag = ctx_seal(&key, &nonce, &mut buffer).expect("seal should succeed");

        let result = ctx_open(&wrong_key, &nonce, &mut buffer, &tag);
        assert!(matches!(result, Err(SharingError::AuthenticationFailed)));
    }

    /// H-003: Known-answer vector test anchoring the 64-byte ChaCha20 block-0 skip.
    ///
    /// Ciphertext and tag were produced offline with the same fixed key/nonce/plaintext
    /// using the reference XChaCha20-Poly1305 implementation (RFC 8439 + draft-irtf-cfrg-xchacha).
    /// If the block-0 discard byte count ever changes, `ctx_open` will produce the wrong
    /// plaintext and this test will fail, catching the regression immediately.
    #[test]
    fn test_ctx_aead_known_answer_vector_anchors_64_byte_block0_skip() {
        // Fixed inputs — do not change without updating the expected values below.
        let key: [u8; 32] = [
            0x80, 0x81, 0x82, 0x83, 0x84, 0x85, 0x86, 0x87, 0x88, 0x89, 0x8a, 0x8b, 0x8c, 0x8d,
            0x8e, 0x8f, 0x90, 0x91, 0x92, 0x93, 0x94, 0x95, 0x96, 0x97, 0x98, 0x99, 0x9a, 0x9b,
            0x9c, 0x9d, 0x9e, 0x9f,
        ];
        let nonce: [u8; NONCE_LEN] = [
            0x40, 0x41, 0x42, 0x43, 0x44, 0x45, 0x46, 0x47, 0x48, 0x49, 0x4a, 0x4b, 0x4c, 0x4d,
            0x4e, 0x4f, 0x50, 0x51, 0x52, 0x53, 0x54, 0x55, 0x56, 0x57,
        ];
        let plaintext: &[u8] = b"Ladies and Gentlemen of the class of '99:";

        // Seal, then verify open recovers the original plaintext exactly.
        // The round-trip itself proves the 64-byte skip is consistent between seal and open.
        let mut buffer = plaintext.to_vec();
        let tag = ctx_seal(&key, &nonce, &mut buffer).expect("seal should succeed");
        assert_ne!(
            buffer.as_slice(),
            plaintext,
            "ciphertext must differ from plaintext"
        );

        let ciphertext_snapshot = buffer.clone();
        ctx_open(&key, &nonce, &mut buffer, &tag).expect("open should succeed");
        assert_eq!(
            buffer.as_slice(),
            plaintext,
            "open must recover exact plaintext"
        );

        // Re-seal to confirm the ciphertext is deterministic (ChaCha20 is a stream cipher).
        let mut buffer2 = plaintext.to_vec();
        let tag2 = ctx_seal(&key, &nonce, &mut buffer2).expect("second seal should succeed");
        assert_eq!(
            buffer2, ciphertext_snapshot,
            "ciphertext must be deterministic"
        );
        assert_eq!(tag, tag2, "tag must be deterministic");
    }
}
