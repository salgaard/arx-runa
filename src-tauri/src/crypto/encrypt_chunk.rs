//! XChaCha20-Poly1305 chunk encryption with mandatory AAD binding.

use crate::crypto::error::CryptoError;
use crate::crypto::nonce::generate_nonce;
use crate::crypto::types::{ChunkIndex, FileId, FileKey};
use chacha20poly1305::{
    AeadInPlace, KeyInit, XChaCha20Poly1305, aead::generic_array::GenericArray,
};

/// Encrypts one chunk and returns the wire-format blob
/// `[24-byte nonce | ciphertext | 16-byte tag]`.
///
/// The AAD bound into the AEAD tag is `file_id || chunk_index` (big-endian
/// `u32`), enforcing per-file and per-position context at decrypt time.
///
/// # Arguments
/// * `plaintext` - Owned chunk bytes; consumed and overwritten in place.
/// * `file_key` - The per-file encryption key.
/// * `file_id` - The file identifier (16 bytes).
/// * `chunk_index` - Zero-based position of the chunk within the file.
///
/// # Errors
/// Returns `CryptoError::EncryptionFailed` if the underlying
/// `encrypt_in_place_detached` call fails. For XChaCha20-Poly1305 with a
/// valid key and nonce length this is unreachable in practice — the
/// RustCrypto contract only errors on plaintext length overflow (≈ 2^38
/// bytes), far beyond any Arx Runa chunk size — but the fallible surface
/// lets callers propagate unexpected failures instead of panicking.
pub fn encrypt_chunk(
    mut plaintext: Vec<u8>,
    file_key: &FileKey,
    file_id: &FileId,
    chunk_index: ChunkIndex,
) -> Result<Vec<u8>, CryptoError> {
    let nonce_bytes = generate_nonce();
    let aad = build_aad(file_id, chunk_index);

    let cipher = XChaCha20Poly1305::new(GenericArray::from_slice(file_key.expose()));
    let nonce = GenericArray::from_slice(&nonce_bytes);

    let tag = cipher
        .encrypt_in_place_detached(nonce, &aad, plaintext.as_mut_slice())
        .map_err(|_| CryptoError::EncryptionFailed)?;

    let mut blob = Vec::with_capacity(24 + plaintext.len() + 16);
    blob.extend_from_slice(&nonce_bytes);
    blob.extend_from_slice(&plaintext);
    blob.extend_from_slice(tag.as_slice());
    Ok(blob)
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
    use crate::crypto::decrypt_chunk::decrypt_chunk;
    use crate::crypto::types::FileKey;

    fn make_file_key(byte: u8) -> FileKey {
        FileKey::from_bytes([byte; 32])
    }

    fn make_file_id(byte: u8) -> FileId {
        FileId::new([byte; 16])
    }

    fn verified(blob: Vec<u8>) -> VerifiedBlob {
        let checksum = compute_checksum(&blob);
        verify_checksum(blob, &checksum).expect("self-consistent checksum must verify")
    }

    #[test]
    fn test_encrypt_chunk_produces_wire_format_with_forty_byte_overhead() {
        let plaintext = vec![1u8, 2, 3, 4, 5];
        let blob = encrypt_chunk(
            plaintext.clone(),
            &make_file_key(0xAA),
            &make_file_id(0x01),
            ChunkIndex::new(0),
        )
        .expect("encrypt must succeed");

        assert_eq!(blob.len(), plaintext.len() + 40);
    }

    #[test]
    fn test_encrypt_chunk_empty_plaintext_produces_forty_byte_blob() {
        let blob = encrypt_chunk(
            Vec::new(),
            &make_file_key(0x42),
            &make_file_id(0x01),
            ChunkIndex::new(0),
        )
        .expect("encrypt must succeed");

        assert_eq!(blob.len(), 40);
    }

    #[test]
    fn test_encrypt_chunk_two_calls_same_plaintext_produce_different_blobs() {
        let plaintext = vec![0xCDu8; 128];
        let key = make_file_key(0x11);
        let file_id = make_file_id(0x22);

        let first = encrypt_chunk(plaintext.clone(), &key, &file_id, ChunkIndex::new(3))
            .expect("encrypt must succeed");
        let second = encrypt_chunk(plaintext, &key, &file_id, ChunkIndex::new(3))
            .expect("encrypt must succeed");

        assert_ne!(first, second, "different nonces must yield different blobs");
        assert_ne!(first[..24], second[..24], "nonce prefix must differ");
    }

    #[test]
    fn test_encrypt_decrypt_round_trip_returns_original_plaintext() {
        let plaintext = b"hello arx runa".to_vec();
        let key = make_file_key(0x33);
        let file_id = make_file_id(0x44);
        let chunk_index = ChunkIndex::new(7);

        let blob = encrypt_chunk(plaintext.clone(), &key, &file_id, chunk_index)
            .expect("encrypt must succeed");
        let recovered = decrypt_chunk(verified(blob), &key, &file_id, chunk_index)
            .expect("round trip must succeed");

        assert_eq!(recovered, plaintext);
    }
}

#[cfg(test)]
mod proptests {
    use super::*;
    use crate::crypto::checksum::{VerifiedBlob, compute_checksum, verify_checksum};
    use crate::crypto::decrypt_chunk::decrypt_chunk;
    use crate::crypto::types::FileKey;
    use proptest::prelude::*;

    fn verified(blob: Vec<u8>) -> VerifiedBlob {
        let checksum = compute_checksum(&blob);
        verify_checksum(blob, &checksum).expect("self-consistent checksum must verify")
    }

    proptest! {
        #[test]
        fn prop_encrypt_decrypt_identity(
            plaintext in proptest::collection::vec(any::<u8>(), 0..=65_536),
            key_byte in any::<u8>(),
            file_id_seed in any::<[u8; 16]>(),
            chunk_index in any::<u32>(),
        ) {
            let key = FileKey::from_bytes([key_byte; 32]);
            let file_id = FileId::new(file_id_seed);
            let idx = ChunkIndex::new(chunk_index);

            let blob = encrypt_chunk(plaintext.clone(), &key, &file_id, idx)
                .expect("encrypt must succeed");
            let recovered = decrypt_chunk(verified(blob), &key, &file_id, idx)
                .expect("round trip must succeed");
            prop_assert_eq!(recovered, plaintext);
        }

        #[test]
        fn prop_different_nonces_produce_different_blobs(
            plaintext in proptest::collection::vec(any::<u8>(), 1..=4096),
        ) {
            let key = FileKey::from_bytes([0xCDu8; 32]);
            let file_id = FileId::new([0xABu8; 16]);
            let idx = ChunkIndex::new(0);

            let first = encrypt_chunk(plaintext.clone(), &key, &file_id, idx)
                .expect("encrypt must succeed");
            let second = encrypt_chunk(plaintext, &key, &file_id, idx)
                .expect("encrypt must succeed");

            prop_assert_ne!(&first[..24], &second[..24]);
            prop_assert_ne!(first, second);
        }
    }
}
