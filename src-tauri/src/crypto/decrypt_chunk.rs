//! XChaCha20-Poly1305 chunk decryption with mandatory AAD binding.
//!
//! `decrypt_chunk` consumes a `VerifiedBlob` — the only way to obtain one is
//! via `verify_checksum`, so the BLAKE3 integrity check is enforced at the
//! type level.

use crate::crypto::checksum::VerifiedBlob;
use crate::crypto::error::CryptoError;
use crate::crypto::types::{ChunkIndex, FileId, FileKey};
use chacha20poly1305::{
    AeadInPlace, KeyInit, XChaCha20Poly1305, aead::generic_array::GenericArray,
};
use zeroize::Zeroize;

const NONCE_LEN: usize = 24;
const TAG_LEN: usize = 16;
const MIN_BLOB_LEN: usize = NONCE_LEN + TAG_LEN;

/// Decrypts one chunk from its verified wire-format blob.
///
/// The blob layout is `[24-byte nonce | ciphertext | 16-byte Poly1305 tag]`.
/// AAD is reconstructed as `file_id || chunk_index` (big-endian `u32`) and
/// must match the value used at encrypt time. The caller must have already
/// verified the BLAKE3 checksum via `verify_checksum` to obtain the
/// `VerifiedBlob`.
///
/// # Errors
/// * `CryptoError::InvalidBlobFormat` if the blob is shorter than 40 bytes.
/// * `CryptoError::DecryptionFailed` if the Poly1305 tag verification fails,
///   including wrong key, wrong `file_id`, wrong `chunk_index`, or any
///   ciphertext/tag tampering that the BLAKE3 pre-check did not catch
///   (possible if the stored expected checksum is itself out-of-date).
pub fn decrypt_chunk(
    blob: VerifiedBlob,
    file_key: &FileKey,
    file_id: &FileId,
    chunk_index: ChunkIndex,
) -> Result<Vec<u8>, CryptoError> {
    let blob_bytes: Vec<u8> = blob.into_inner();

    if blob_bytes.len() < MIN_BLOB_LEN {
        return Err(CryptoError::InvalidBlobFormat {
            expected: MIN_BLOB_LEN,
            actual: blob_bytes.len(),
        });
    }

    let (nonce_slice, rest) = blob_bytes.split_at(NONCE_LEN);
    let ciphertext_len = rest.len() - TAG_LEN;
    let (ciphertext_slice, tag_slice) = rest.split_at(ciphertext_len);

    let aad = build_aad(file_id, chunk_index);
    let cipher = XChaCha20Poly1305::new(GenericArray::from_slice(file_key.expose()));
    let nonce = GenericArray::from_slice(nonce_slice);
    let tag = GenericArray::from_slice(tag_slice);

    let mut buffer = ciphertext_slice.to_vec();

    match cipher.decrypt_in_place_detached(nonce, &aad, buffer.as_mut_slice(), tag) {
        Ok(()) => Ok(buffer),
        Err(_) => {
            buffer.zeroize();
            Err(CryptoError::DecryptionFailed)
        }
    }
}

fn build_aad(file_id: &FileId, chunk_index: ChunkIndex) -> [u8; 20] {
    let mut aad = [0u8; 20];
    aad[..16].copy_from_slice(file_id.as_bytes());
    aad[16..].copy_from_slice(&chunk_index.to_be_bytes());
    aad
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::checksum::{VerifiedBlob, compute_checksum, verify_checksum};
    use crate::crypto::encrypt_chunk::encrypt_chunk;
    use crate::crypto::types::FileKey;
    use assert_matches::assert_matches;

    fn make_file_key(byte: u8) -> FileKey {
        FileKey::from_bytes([byte; 32])
    }

    fn make_file_id(byte: u8) -> FileId {
        FileId::new([byte; 16])
    }

    fn encrypt(
        plaintext: &[u8],
        key: &FileKey,
        file_id: &FileId,
        chunk_index: ChunkIndex,
    ) -> Vec<u8> {
        encrypt_chunk(plaintext.to_vec(), key, file_id, chunk_index).expect("encrypt must succeed")
    }

    fn verified(blob: Vec<u8>) -> VerifiedBlob {
        let checksum = compute_checksum(&blob);
        verify_checksum(blob, &checksum).expect("self-consistent checksum must verify")
    }

    #[test]
    fn test_decrypt_chunk_wrong_file_id_fails_with_decryption_failed() {
        let key = make_file_key(0xAA);
        let blob = encrypt(b"payload", &key, &make_file_id(0x11), ChunkIndex::new(0));

        let result = decrypt_chunk(
            verified(blob),
            &key,
            &make_file_id(0x22),
            ChunkIndex::new(0),
        );

        assert_matches!(result, Err(CryptoError::DecryptionFailed));
    }

    #[test]
    fn test_decrypt_chunk_wrong_chunk_index_fails_with_decryption_failed() {
        let key = make_file_key(0xAA);
        let file_id = make_file_id(0x11);
        let blob = encrypt(b"payload", &key, &file_id, ChunkIndex::new(0));

        let result = decrypt_chunk(verified(blob), &key, &file_id, ChunkIndex::new(1));

        assert_matches!(result, Err(CryptoError::DecryptionFailed));
    }

    #[test]
    fn test_decrypt_chunk_wrong_key_fails_with_decryption_failed() {
        let file_id = make_file_id(0x11);
        let blob = encrypt(
            b"payload",
            &make_file_key(0xAA),
            &file_id,
            ChunkIndex::new(0),
        );

        let result = decrypt_chunk(
            verified(blob),
            &make_file_key(0xBB),
            &file_id,
            ChunkIndex::new(0),
        );

        assert_matches!(result, Err(CryptoError::DecryptionFailed));
    }

    #[test]
    fn test_decrypt_chunk_corrupted_ciphertext_fails_with_decryption_failed() {
        let key = make_file_key(0xAA);
        let file_id = make_file_id(0x11);
        let mut blob = encrypt(b"payload bytes here", &key, &file_id, ChunkIndex::new(0));

        let target = 24 + 2;
        blob[target] ^= 0x01;

        let result = decrypt_chunk(verified(blob), &key, &file_id, ChunkIndex::new(0));

        assert_matches!(result, Err(CryptoError::DecryptionFailed));
    }

    #[test]
    fn test_decrypt_chunk_corrupted_tag_fails_with_decryption_failed() {
        let key = make_file_key(0xAA);
        let file_id = make_file_id(0x11);
        let mut blob = encrypt(b"payload", &key, &file_id, ChunkIndex::new(0));

        let tag_index = blob.len() - 1;
        blob[tag_index] ^= 0x01;

        let result = decrypt_chunk(verified(blob), &key, &file_id, ChunkIndex::new(0));

        assert_matches!(result, Err(CryptoError::DecryptionFailed));
    }

    #[test]
    fn test_decrypt_chunk_truncated_blob_returns_invalid_blob_format() {
        let result = decrypt_chunk(
            verified(vec![0u8; 20]),
            &make_file_key(0xAA),
            &make_file_id(0x11),
            ChunkIndex::new(0),
        );

        assert_matches!(
            result,
            Err(CryptoError::InvalidBlobFormat {
                expected: 40,
                actual: 20
            })
        );
    }

    #[test]
    fn test_decrypt_chunk_blob_thirty_nine_bytes_returns_invalid_blob_format() {
        let result = decrypt_chunk(
            verified(vec![0u8; 39]),
            &make_file_key(0xAA),
            &make_file_id(0x11),
            ChunkIndex::new(0),
        );

        assert_matches!(
            result,
            Err(CryptoError::InvalidBlobFormat {
                expected: 40,
                actual: 39
            })
        );
    }

    #[test]
    fn test_decrypt_chunk_exactly_forty_bytes_empty_plaintext_round_trip_succeeds() {
        let key = make_file_key(0xAA);
        let file_id = make_file_id(0x11);
        let blob = encrypt(b"", &key, &file_id, ChunkIndex::new(0));

        assert_eq!(blob.len(), 40);

        let recovered = decrypt_chunk(verified(blob), &key, &file_id, ChunkIndex::new(0))
            .expect("empty plaintext round trip must succeed");
        assert!(recovered.is_empty());
    }

    #[test]
    fn test_decrypt_chunk_empty_blob_returns_invalid_blob_format() {
        let result = decrypt_chunk(
            verified(Vec::new()),
            &make_file_key(0xAA),
            &make_file_id(0x11),
            ChunkIndex::new(0),
        );

        assert_matches!(
            result,
            Err(CryptoError::InvalidBlobFormat {
                expected: 40,
                actual: 0
            })
        );
    }

    #[test]
    fn test_decrypt_chunk_accepts_verified_blob_produced_by_verify_checksum() {
        let key = make_file_key(0xAA);
        let file_id = make_file_id(0x11);
        let blob = encrypt(b"hello verified world", &key, &file_id, ChunkIndex::new(0));

        let verified_blob = verified(blob);
        let plaintext = decrypt_chunk(verified_blob, &key, &file_id, ChunkIndex::new(0))
            .expect("verified round trip must succeed");

        assert_eq!(plaintext, b"hello verified world");
    }
}
