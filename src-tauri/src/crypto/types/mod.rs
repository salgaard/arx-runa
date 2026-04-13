//! Newtype wrappers for crypto keys and crypto-domain identifiers.

use secrecy::{ExposeSecret, SecretBox};
use zeroize::ZeroizeOnDrop;

/// 256-bit per-file encryption key.
#[derive(ZeroizeOnDrop)]
pub struct FileKey(SecretBox<[u8; 32]>);

impl FileKey {
    /// Constructs a file key from raw key bytes for deterministic tests.
    ///
    /// Test-only helper to avoid exposing a production constructor that accepts
    /// plain key bytes by value.
    #[cfg(test)]
    pub(crate) fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(SecretBox::new(Box::new(bytes)))
    }

    /// Constructs a file key from protected heap storage.
    ///
    /// Used by `unwrap_file_key` so the decrypted key bytes never exist in a
    /// plain local variable outside the `SecretBox`.
    pub(crate) fn from_secret_box(secret_box: SecretBox<[u8; 32]>) -> Self {
        Self(secret_box)
    }

    /// Exposes the key bytes for cryptographic operations.
    #[allow(dead_code)]
    pub(crate) fn expose(&self) -> &[u8; 32] {
        self.0.expose_secret()
    }
}

/// 256-bit key-encryption key used to wrap file keys.
#[derive(ZeroizeOnDrop)]
pub struct KeyEncryptionKey(SecretBox<[u8; 32]>);

impl KeyEncryptionKey {
    /// Constructs a key-encryption key from protected heap storage.
    pub(crate) fn from_secret_box(secret_box: SecretBox<[u8; 32]>) -> Self {
        Self(secret_box)
    }

    /// Constructs a key-encryption key from raw key bytes for deterministic tests.
    ///
    /// Test-only helper to avoid exposing a production constructor that accepts
    /// plain key bytes by value.
    #[cfg(test)]
    pub(crate) fn from_bytes(bytes: [u8; 32]) -> Self {
        Self::from_secret_box(SecretBox::new(Box::new(bytes)))
    }

    /// Exposes the key bytes for cryptographic operations.
    #[allow(dead_code)]
    pub(crate) fn expose(&self) -> &[u8; 32] {
        self.0.expose_secret()
    }
}

/// 256-bit SQLCipher database encryption key.
#[derive(ZeroizeOnDrop)]
pub struct SqlcipherKey(SecretBox<[u8; 32]>);

impl SqlcipherKey {
    /// Constructs a SQLCipher key from protected heap storage.
    pub(crate) fn from_secret_box(secret_box: SecretBox<[u8; 32]>) -> Self {
        Self(secret_box)
    }

    /// Constructs a SQLCipher key from raw key bytes for deterministic tests.
    ///
    /// Test-only helper to avoid exposing a production constructor that accepts
    /// plain key bytes by value.
    #[cfg(test)]
    pub(crate) fn from_bytes(bytes: [u8; 32]) -> Self {
        Self::from_secret_box(SecretBox::new(Box::new(bytes)))
    }

    /// Exposes the key bytes for cryptographic operations.
    #[allow(dead_code)]
    pub(crate) fn expose(&self) -> &[u8; 32] {
        self.0.expose_secret()
    }
}

/// 256-bit manifest-backup encryption key.
#[derive(ZeroizeOnDrop)]
pub struct ManifestKey(SecretBox<[u8; 32]>);

impl ManifestKey {
    /// Constructs a manifest key from protected heap storage.
    pub(crate) fn from_secret_box(secret_box: SecretBox<[u8; 32]>) -> Self {
        Self(secret_box)
    }

    /// Constructs a manifest key from raw key bytes for deterministic tests.
    ///
    /// Test-only helper to avoid exposing a production constructor that accepts
    /// plain key bytes by value.
    #[cfg(test)]
    pub(crate) fn from_bytes(bytes: [u8; 32]) -> Self {
        Self::from_secret_box(SecretBox::new(Box::new(bytes)))
    }

    /// Exposes the key bytes for cryptographic operations.
    #[allow(dead_code)]
    pub(crate) fn expose(&self) -> &[u8; 32] {
        self.0.expose_secret()
    }
}

/// Wrapped file key in wire format `[24-byte nonce | 32-byte ciphertext | 16-byte tag]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WrappedFileKey(pub [u8; 72]);

/// File identifier represented as compact UUID bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FileId([u8; 16]);

impl FileId {
    /// Creates a file identifier from raw UUID bytes.
    pub fn new(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }

    /// Returns the raw UUID bytes.
    pub fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }

    /// Converts from a `Uuid`.
    pub fn from_uuid(uuid: uuid::Uuid) -> Self {
        Self(*uuid.as_bytes())
    }

    /// Converts to a `Uuid`.
    pub fn to_uuid(&self) -> uuid::Uuid {
        uuid::Uuid::from_bytes(self.0)
    }
}

/// Zero-based chunk position inside a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ChunkIndex(u32);

impl ChunkIndex {
    /// Creates a chunk index.
    pub fn new(index: u32) -> Self {
        Self(index)
    }

    /// Returns the chunk index as a `u32`.
    pub fn as_u32(&self) -> u32 {
        self.0
    }

    /// Returns the big-endian byte representation.
    pub fn to_be_bytes(&self) -> [u8; 4] {
        self.0.to_be_bytes()
    }
}

/// 32-byte BLAKE3 checksum of an encrypted blob.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Blake3Hash(pub [u8; 32]);

#[cfg(test)]
mod tests {
    use super::*;
    use zeroize::Zeroize;

    #[test]
    fn test_file_id_uuid_roundtrip_preserves_value() {
        let uuid = uuid::Uuid::new_v4();
        let file_id = FileId::from_uuid(uuid);

        assert_eq!(file_id.to_uuid(), uuid);
        assert_eq!(file_id.as_bytes(), uuid.as_bytes());
    }

    #[test]
    fn test_file_id_new_preserves_bytes() {
        let bytes = [7u8; 16];
        let file_id = FileId::new(bytes);

        assert_eq!(*file_id.as_bytes(), bytes);
    }

    #[test]
    fn test_chunk_index_to_be_bytes_encodes_expected_order() {
        assert_eq!(ChunkIndex::new(0).to_be_bytes(), [0, 0, 0, 0]);
        assert_eq!(ChunkIndex::new(1).to_be_bytes(), [0, 0, 0, 1]);
        assert_eq!(ChunkIndex::new(0x0102_0304).to_be_bytes(), [1, 2, 3, 4]);
        assert_eq!(
            ChunkIndex::new(u32::MAX).to_be_bytes(),
            [0xFF, 0xFF, 0xFF, 0xFF]
        );
    }

    #[test]
    fn test_file_key_zeroize_trait_clears_memory() {
        let mut file_key = FileKey::from_bytes([0xAAu8; 32]);
        let pointer = file_key.expose().as_ptr();

        // SAFETY: `pointer` comes from `file_key.expose()` and `file_key` remains
        // alive and allocated for the duration of this read.
        let before_zeroize = unsafe { std::slice::from_raw_parts(pointer, 32) };
        assert_eq!(before_zeroize, &[0xAAu8; 32]);

        Zeroize::zeroize(&mut file_key.0);

        // SAFETY: `file_key` is still alive; the pointer remains valid for 32
        // bytes and now points at the zeroized buffer.
        let after_zeroize = unsafe { std::slice::from_raw_parts(pointer, 32) };
        assert_eq!(after_zeroize, &[0u8; 32]);
    }

    #[test]
    fn test_key_encryption_key_zeroize_trait_clears_memory() {
        let mut key_encryption_key = KeyEncryptionKey::from_bytes([0x5Au8; 32]);
        let pointer = key_encryption_key.expose().as_ptr();

        // SAFETY: `pointer` comes from `key_encryption_key.expose()` and the key
        // remains alive and allocated for this read.
        let before_zeroize = unsafe { std::slice::from_raw_parts(pointer, 32) };
        assert_eq!(before_zeroize, &[0x5Au8; 32]);

        Zeroize::zeroize(&mut key_encryption_key.0);

        // SAFETY: `key_encryption_key` is still alive; the pointer remains valid
        // for 32 bytes and now points at the zeroized buffer.
        let after_zeroize = unsafe { std::slice::from_raw_parts(pointer, 32) };
        assert_eq!(after_zeroize, &[0u8; 32]);
    }
}
