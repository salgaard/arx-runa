//! XChaCha20-Poly1305 file-key wrapping and unwrapping.
//!
//! Wrapped file keys are stored in the local SQLCipher manifest; the wire
//! format is `[24-byte nonce | 32-byte ciphertext | 16-byte tag] = 72 bytes`.
//! AAD is intentionally empty — the wrapped blob is scoped by the
//! `key_encryption_key` domain and the SQLCipher manifest, so no external
//! context binding is required. Recovery-slot wrapping uses distinct
//! functions with non-empty AAD and is owned by Phase 2.

use crate::crypto::error::CryptoError;
use crate::crypto::nonce::generate_nonce;
use crate::crypto::types::{FileKey, KeyEncryptionKey, WrappedFileKey};
use chacha20poly1305::{
    AeadInPlace, KeyInit, XChaCha20Poly1305, aead::generic_array::GenericArray,
};
use secrecy::SecretBox;
use zeroize::Zeroizing;

const NONCE_LEN: usize = 24;
const KEY_LEN: usize = 32;
const TAG_LEN: usize = 16;
const WRAPPED_LEN: usize = NONCE_LEN + KEY_LEN + TAG_LEN;

/// Wraps a 32-byte file key with the key-encryption key.
///
/// Uses XChaCha20-Poly1305 with a fresh CSPRNG nonce and empty AAD. The
/// 72-byte output embeds the nonce, the encrypted key, and the Poly1305 tag
/// so the wrapped blob is self-contained.
///
/// The plaintext file key is copied into a `Zeroizing` buffer so the
/// in-place encryption target is zeroed on drop even if the function
/// returns early via `?`.
///
/// # Errors
/// Returns `CryptoError::KeyWrapFailed` if the underlying AEAD call fails.
/// For a 32-byte plaintext with XChaCha20-Poly1305 this is unreachable in
/// practice, but the fallible surface lets callers propagate unexpected
/// failures instead of panicking.
pub fn wrap_file_key(
    file_key: &FileKey,
    key_encryption_key: &KeyEncryptionKey,
) -> Result<WrappedFileKey, CryptoError> {
    let nonce_bytes = generate_nonce();
    let mut ciphertext: Zeroizing<[u8; KEY_LEN]> = Zeroizing::new([0u8; KEY_LEN]);
    ciphertext.copy_from_slice(file_key.expose());

    let cipher = XChaCha20Poly1305::new(GenericArray::from_slice(key_encryption_key.expose()));
    let nonce = GenericArray::from_slice(&nonce_bytes);

    let tag = cipher
        .encrypt_in_place_detached(nonce, &[], ciphertext.as_mut_slice())
        .map_err(|_| CryptoError::KeyWrapFailed)?;

    let mut wire = [0u8; WRAPPED_LEN];
    wire[..NONCE_LEN].copy_from_slice(&nonce_bytes);
    wire[NONCE_LEN..NONCE_LEN + KEY_LEN].copy_from_slice(ciphertext.as_slice());
    wire[NONCE_LEN + KEY_LEN..].copy_from_slice(tag.as_slice());

    Ok(WrappedFileKey(wire))
}

/// Unwraps a `WrappedFileKey`, returning a fresh `FileKey`.
///
/// Decryption runs inside a `SecretBox<[u8; 32]>` via `init_with_mut`, so on
/// authentication failure the partial-keystream buffer is zeroized by the
/// `SecretBox`'s `Drop` rather than lingering on the stack.
///
/// # Errors
/// Returns `CryptoError::DecryptionFailed` if the authentication tag does
/// not verify (wrong `key_encryption_key` or tampered wrapped blob).
pub fn unwrap_file_key(
    wrapped: &WrappedFileKey,
    key_encryption_key: &KeyEncryptionKey,
) -> Result<FileKey, CryptoError> {
    let nonce_slice = &wrapped.0[..NONCE_LEN];
    let ciphertext_slice = &wrapped.0[NONCE_LEN..NONCE_LEN + KEY_LEN];
    let tag_slice = &wrapped.0[NONCE_LEN + KEY_LEN..];

    let cipher = XChaCha20Poly1305::new(GenericArray::from_slice(key_encryption_key.expose()));
    let nonce = GenericArray::from_slice(nonce_slice);
    let tag = GenericArray::from_slice(tag_slice);

    let mut decrypt_result: Result<(), chacha20poly1305::Error> = Ok(());
    let file_key_secret_box = SecretBox::<[u8; KEY_LEN]>::init_with_mut(|buffer| {
        buffer.copy_from_slice(ciphertext_slice);
        decrypt_result = cipher.decrypt_in_place_detached(nonce, &[], buffer.as_mut_slice(), tag);
    });

    match decrypt_result {
        Ok(()) => Ok(FileKey::from_secret_box(file_key_secret_box)),
        Err(_) => {
            // `file_key_secret_box` is dropped here; SecretBox's Drop
            // zeroizes the heap allocation so partial-keystream bytes
            // cannot outlive this function.
            Err(CryptoError::DecryptionFailed)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{WRAPPED_LEN, unwrap_file_key, wrap_file_key};
    use crate::crypto::error::CryptoError;
    use crate::crypto::types::{FileKey, KeyEncryptionKey, WrappedFileKey};

    fn make_file_key(byte: u8) -> FileKey {
        FileKey::from_bytes([byte; 32])
    }

    fn make_kek(byte: u8) -> KeyEncryptionKey {
        KeyEncryptionKey::from_bytes([byte; 32])
    }

    #[test]
    fn test_wrap_unwrap_file_key_round_trip_returns_original_bytes() {
        let original = make_file_key(0x11);
        let kek = make_kek(0x22);
        let original_bytes = *original.expose();

        let wrapped = wrap_file_key(&original, &kek).expect("wrap must succeed");
        let recovered = unwrap_file_key(&wrapped, &kek).expect("round trip must succeed");

        assert_eq!(*recovered.expose(), original_bytes);
    }

    #[test]
    fn test_wrap_file_key_wire_format_is_seventy_two_bytes() {
        let wrapped =
            wrap_file_key(&make_file_key(0xAA), &make_kek(0xBB)).expect("wrap must succeed");
        assert_eq!(wrapped.0.len(), WRAPPED_LEN);
        assert_eq!(WRAPPED_LEN, 72);
    }

    #[test]
    fn test_wrap_file_key_two_calls_produce_different_blobs() {
        let file_key = make_file_key(0xCD);
        let kek = make_kek(0xEF);

        let first = wrap_file_key(&file_key, &kek).expect("wrap must succeed");
        let second = wrap_file_key(&file_key, &kek).expect("wrap must succeed");

        assert_ne!(
            first.0, second.0,
            "random nonce must make wrapped blobs differ"
        );
        assert_ne!(first.0[..24], second.0[..24], "nonce prefix must differ");
    }

    #[test]
    fn test_unwrap_file_key_wrong_kek_fails_with_decryption_failed() {
        let file_key = make_file_key(0x11);
        let wrapped = wrap_file_key(&file_key, &make_kek(0x22)).expect("wrap must succeed");

        let result = unwrap_file_key(&wrapped, &make_kek(0x33));

        assert!(matches!(result, Err(CryptoError::DecryptionFailed)));
    }

    #[test]
    fn test_unwrap_file_key_corrupted_nonce_fails_with_decryption_failed() {
        let file_key = make_file_key(0x11);
        let kek = make_kek(0x22);
        let mut wrapped = wrap_file_key(&file_key, &kek).expect("wrap must succeed");

        wrapped.0[0] ^= 0x01;

        let result = unwrap_file_key(&wrapped, &kek);

        assert!(matches!(result, Err(CryptoError::DecryptionFailed)));
    }

    #[test]
    fn test_unwrap_file_key_corrupted_ciphertext_fails_with_decryption_failed() {
        let file_key = make_file_key(0x11);
        let kek = make_kek(0x22);
        let mut wrapped = wrap_file_key(&file_key, &kek).expect("wrap must succeed");

        // Offset 24..56 is the 32-byte ciphertext region.
        wrapped.0[24 + 5] ^= 0x01;

        let result = unwrap_file_key(&wrapped, &kek);

        assert!(matches!(result, Err(CryptoError::DecryptionFailed)));
    }

    #[test]
    fn test_unwrap_file_key_corrupted_tag_fails_with_decryption_failed() {
        let file_key = make_file_key(0x11);
        let kek = make_kek(0x22);
        let mut wrapped = wrap_file_key(&file_key, &kek).expect("wrap must succeed");

        let tag_index = wrapped.0.len() - 1;
        wrapped.0[tag_index] ^= 0x01;

        let result = unwrap_file_key(&wrapped, &kek);

        assert!(matches!(result, Err(CryptoError::DecryptionFailed)));
    }

    #[test]
    fn test_unwrap_file_key_all_zero_wrapped_blob_fails_with_decryption_failed() {
        let kek = make_kek(0x22);
        let wrapped = WrappedFileKey([0u8; 72]);

        let result = unwrap_file_key(&wrapped, &kek);

        assert!(matches!(result, Err(CryptoError::DecryptionFailed)));
    }
}
