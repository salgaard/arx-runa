//! Minimal manifest-backup encrypt/decrypt helper for Phase 2.4.
//!
//! Phase 4.4 will own the full manifest-backup schema, streaming, and
//! versioning. Phase 2.4 only needs a round-trippable primitive so
//! `recover_vault` and its tests can prove the decrypt path is wired
//! correctly.
//!
//! Wire format: `[24-byte nonce | ciphertext | 16-byte tag]` with
//! XChaCha20-Poly1305 and **no AAD** (matches the design's singleton-blob
//! rule for manifest-backup and cross-phase consistency with the Phase 4.4
//! design's decrypt helper).

use chacha20poly1305::{
    AeadInPlace, KeyInit, XChaCha20Poly1305, aead::generic_array::GenericArray,
};
use zeroize::Zeroizing;

use crate::crypto::error::CryptoError;
use crate::crypto::nonce::generate_nonce;

const NONCE_LEN: usize = 24;
const TAG_LEN: usize = 16;

/// Encrypts caller-owned `plaintext` under `manifest_key` with XChaCha20-Poly1305 and no AAD.
///
/// Returns the wire-format blob `[nonce || ciphertext || tag]`. The
/// caller-owned plaintext buffer is consumed and wrapped in `Zeroizing`
/// for in-place encryption so interrupted operations cannot leave plaintext
/// in memory.
#[allow(dead_code)]
pub(crate) fn encrypt_manifest_backup(
    plaintext: Vec<u8>,
    manifest_key: &[u8; 32],
) -> Result<Vec<u8>, CryptoError> {
    let nonce_bytes = generate_nonce();
    let mut plaintext: Zeroizing<Vec<u8>> = Zeroizing::new(plaintext);
    let cipher = XChaCha20Poly1305::new(GenericArray::from_slice(manifest_key));
    let nonce = GenericArray::from_slice(&nonce_bytes);
    let tag = cipher
        .encrypt_in_place_detached(nonce, &[], plaintext.as_mut_slice())
        .map_err(|_| CryptoError::EncryptionFailed)?;

    let mut wire = Vec::with_capacity(NONCE_LEN + plaintext.len() + TAG_LEN);
    wire.extend_from_slice(&nonce_bytes);
    wire.extend_from_slice(&plaintext);
    wire.extend_from_slice(tag.as_slice());
    Ok(wire)
}

/// Decrypts a wire-format manifest backup blob under `manifest_key`.
///
/// Returns the plaintext bytes as `Zeroizing<Vec<u8>>` so the caller
/// controls plaintext lifetime. The caller is expected to dispatch the
/// result into the SQLCipher import path (Phase 2.4 treats plaintext as a
/// SQL dump; Phase 4.4 may revise).
///
/// # Errors
/// Returns `CryptoError::DecryptionFailed` for wrong-key, truncated blob,
/// or tampered nonce/tag/ciphertext.
pub(crate) fn decrypt_manifest_backup(
    wire: &[u8],
    manifest_key: &[u8; 32],
) -> Result<Zeroizing<Vec<u8>>, CryptoError> {
    if wire.len() < NONCE_LEN + TAG_LEN {
        return Err(CryptoError::DecryptionFailed);
    }
    let nonce_slice = &wire[..NONCE_LEN];
    let ciphertext_slice = &wire[NONCE_LEN..wire.len() - TAG_LEN];
    let tag_slice = &wire[wire.len() - TAG_LEN..];

    let cipher = XChaCha20Poly1305::new(GenericArray::from_slice(manifest_key));
    let nonce = GenericArray::from_slice(nonce_slice);
    let tag = GenericArray::from_slice(tag_slice);

    let mut plaintext: Zeroizing<Vec<u8>> = Zeroizing::new(ciphertext_slice.to_vec());
    cipher
        .decrypt_in_place_detached(nonce, &[], plaintext.as_mut_slice(), tag)
        .map_err(|_| CryptoError::DecryptionFailed)?;
    Ok(plaintext)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_manifest_backup_round_trip_returns_plaintext() {
        let manifest_key = [0x11u8; 32];
        let plaintext = b"CREATE TABLE foo (id INTEGER);";

        let wire = encrypt_manifest_backup(plaintext.to_vec(), &manifest_key)
            .expect("encrypt must succeed");
        let recovered =
            decrypt_manifest_backup(&wire, &manifest_key).expect("decrypt must succeed");

        assert_eq!(recovered.as_slice(), plaintext);
    }

    #[test]
    fn test_manifest_backup_wrong_key_returns_decryption_failed() {
        let wire = encrypt_manifest_backup(b"payload".to_vec(), &[0x11u8; 32])
            .expect("encrypt must succeed");

        let result = decrypt_manifest_backup(&wire, &[0x22u8; 32]);

        assert!(matches!(result, Err(CryptoError::DecryptionFailed)));
    }

    #[test]
    fn test_manifest_backup_truncated_wire_returns_decryption_failed() {
        let result = decrypt_manifest_backup(&[0u8; 10], &[0x11u8; 32]);

        assert!(matches!(result, Err(CryptoError::DecryptionFailed)));
    }

    #[test]
    fn test_manifest_backup_corrupted_tag_returns_decryption_failed() {
        let manifest_key = [0x11u8; 32];
        let mut wire = encrypt_manifest_backup(b"payload".to_vec(), &manifest_key)
            .expect("encrypt must succeed");
        let tag_index = wire.len() - 1;
        wire[tag_index] ^= 0x01;

        let result = decrypt_manifest_backup(&wire, &manifest_key);

        assert!(matches!(result, Err(CryptoError::DecryptionFailed)));
    }
}
