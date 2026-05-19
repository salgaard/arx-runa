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

    /// Invokes `callback` with the key bytes for immediate cryptographic use.
    #[allow(dead_code)]
    pub(crate) fn with_exposed<R>(&self, callback: impl FnOnce(&[u8; 32]) -> R) -> R {
        callback(self.0.expose_secret())
    }

    /// Exposes the key bytes for cryptographic operations.
    #[allow(dead_code)]
    #[cfg(not(test))]
    pub(in crate::crypto) fn expose(&self) -> &[u8; 32] {
        self.0.expose_secret()
    }

    /// Test-only raw key view helper.
    #[allow(dead_code)]
    #[cfg(test)]
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

    /// Invokes `callback` with the key bytes for immediate cryptographic use.
    #[allow(dead_code)]
    pub(crate) fn with_exposed<R>(&self, callback: impl FnOnce(&[u8; 32]) -> R) -> R {
        callback(self.0.expose_secret())
    }

    /// Exposes the key bytes for cryptographic operations.
    #[allow(dead_code)]
    #[cfg(not(test))]
    pub(in crate::crypto) fn expose(&self) -> &[u8; 32] {
        self.0.expose_secret()
    }

    /// Test-only raw key view helper.
    #[allow(dead_code)]
    #[cfg(test)]
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

    /// Constructs a `SqlcipherKey` from a borrowed 32-byte slice.
    ///
    /// `SecretBox` does not mlock the allocation; the page is swappable until
    /// `SqlcipherKey` drops and zeroizes. True mlock requires a custom
    /// allocator — this is the accepted protection level for this type.
    pub(crate) fn from_slice(bytes: &[u8; 32]) -> Self {
        let mut boxed = Box::new([0u8; 32]);
        boxed.copy_from_slice(bytes);
        Self(SecretBox::new(boxed))
    }

    /// Constructs a SQLCipher key from raw key bytes for deterministic tests.
    ///
    /// Test-only helper to avoid exposing a production constructor that accepts
    /// plain key bytes by value.
    #[cfg(test)]
    pub(crate) fn from_bytes(bytes: [u8; 32]) -> Self {
        Self::from_secret_box(SecretBox::new(Box::new(bytes)))
    }

    /// Invokes `callback` with the key bytes for immediate cryptographic use.
    #[allow(dead_code)]
    pub(crate) fn with_exposed<R>(&self, callback: impl FnOnce(&[u8; 32]) -> R) -> R {
        callback(self.0.expose_secret())
    }

    /// Exposes the key bytes for cryptographic operations.
    #[allow(dead_code)]
    #[cfg(not(test))]
    pub(in crate::crypto) fn expose(&self) -> &[u8; 32] {
        self.0.expose_secret()
    }

    /// Test-only raw key view helper.
    #[allow(dead_code)]
    #[cfg(test)]
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

    /// Invokes `callback` with the key bytes for immediate cryptographic use.
    #[allow(dead_code)]
    pub(crate) fn with_exposed<R>(&self, callback: impl FnOnce(&[u8; 32]) -> R) -> R {
        callback(self.0.expose_secret())
    }

    /// Exposes the key bytes for cryptographic operations.
    #[allow(dead_code)]
    #[cfg(not(test))]
    pub(in crate::crypto) fn expose(&self) -> &[u8; 32] {
        self.0.expose_secret()
    }

    /// Test-only raw key view helper.
    #[allow(dead_code)]
    #[cfg(test)]
    pub(crate) fn expose(&self) -> &[u8; 32] {
        self.0.expose_secret()
    }
}

/// 256-bit vault master key produced by Argon2id.
///
/// Held in protected heap storage and zeroed on drop. **Invariant**:
/// `MasterKey` must not be assigned to a struct field outside ceremony-local
/// scope in `src-tauri/src/auth/ceremonies.rs`. Phase 2.4 enforces this by
/// holding the raw bytes as `Zeroizing<[u8; 32]>` inside ceremony function
/// bodies and constructing the `MasterKey` newtype only at the boundary of
/// the recovery-wrap primitives.
#[derive(ZeroizeOnDrop)]
pub struct MasterKey(SecretBox<[u8; 32]>);

impl MasterKey {
    /// Constructs a master key from raw key bytes for deterministic tests.
    ///
    /// Test-only helper to avoid exposing a production constructor that accepts
    /// plain key bytes by value.
    #[cfg(test)]
    pub(crate) fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(SecretBox::new(Box::new(bytes)))
    }

    /// Constructs a master key from protected heap storage.
    #[allow(dead_code)]
    pub(crate) fn from_secret_box(secret_box: SecretBox<[u8; 32]>) -> Self {
        Self(secret_box)
    }

    /// Invokes `callback` with the key bytes for immediate cryptographic use.
    pub(crate) fn with_exposed<R>(&self, callback: impl FnOnce(&[u8; 32]) -> R) -> R {
        callback(self.0.expose_secret())
    }

    /// Exposes the key bytes for cryptographic operations.
    #[allow(dead_code)]
    #[cfg(not(test))]
    pub(in crate::crypto) fn expose(&self) -> &[u8; 32] {
        self.0.expose_secret()
    }

    /// Test-only raw key view helper.
    #[allow(dead_code)]
    #[cfg(test)]
    pub(crate) fn expose(&self) -> &[u8; 32] {
        self.0.expose_secret()
    }
}

/// 256-bit recovery key derived from a BIP-39 phrase via Argon2id.
///
/// Never persisted; derived on demand from the user's recovery phrase and
/// zeroized on drop. The recovery key is the AEAD key used to wrap / unwrap
/// `MasterKey` inside vault-header recovery slots.
#[derive(ZeroizeOnDrop)]
pub struct RecoveryKey(SecretBox<[u8; 32]>);

impl RecoveryKey {
    /// Constructs a recovery key from protected heap storage.
    #[allow(dead_code)]
    pub(crate) fn from_secret_box(secret_box: SecretBox<[u8; 32]>) -> Self {
        Self(secret_box)
    }

    /// Constructs a recovery key from raw key bytes for deterministic tests.
    ///
    /// Test-only helper to avoid exposing a production constructor that accepts
    /// plain key bytes by value.
    #[cfg(test)]
    pub(crate) fn from_bytes(bytes: [u8; 32]) -> Self {
        Self::from_secret_box(SecretBox::new(Box::new(bytes)))
    }

    /// Invokes `callback` with the key bytes for immediate cryptographic use.
    #[allow(dead_code)]
    pub(crate) fn with_exposed<R>(&self, callback: impl FnOnce(&[u8; 32]) -> R) -> R {
        callback(self.0.expose_secret())
    }

    /// Exposes the key bytes for cryptographic operations.
    #[allow(dead_code)]
    #[cfg(not(test))]
    pub(in crate::crypto) fn expose(&self) -> &[u8; 32] {
        self.0.expose_secret()
    }

    /// Test-only raw key view helper.
    #[allow(dead_code)]
    #[cfg(test)]
    pub(crate) fn expose(&self) -> &[u8; 32] {
        self.0.expose_secret()
    }
}

/// Wrapped master key in recovery-slot wire format
/// `[24-byte nonce | 32-byte ciphertext | 16-byte tag]`.
///
/// Stored inside the vault header alongside the recovery slot's Argon2
/// parameters. Unlike `WrappedFileKey`, the recovery-slot wrap uses a
/// non-empty AAD (`b"arx-runa recovery v1" || vault_id_bytes`) to bind the
/// ciphertext to vault identity and recovery purpose.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WrappedMasterKey(pub(in crate::crypto) [u8; 72]);

impl WrappedMasterKey {
    /// Constructs a wrapped master key from a 72-byte wire blob.
    ///
    /// Used by callers that reconstruct a wrapped blob from stored bytes (e.g.
    /// a vault header field) before passing it to `unwrap_master_key_from_recovery`.
    pub fn new(bytes: [u8; 72]) -> Self {
        Self(bytes)
    }

    /// Returns the 72-byte wire blob.
    pub fn as_bytes(&self) -> &[u8; 72] {
        &self.0
    }
}

/// Vault identifier — raw 128-bit UUID bytes (not the hyphenated text form).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct VaultId([u8; 16]);

impl VaultId {
    /// Creates a vault identifier from raw UUID bytes.
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

/// Wrapped file key in wire format `[24-byte nonce | 32-byte ciphertext | 16-byte tag]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WrappedFileKey(pub(in crate::crypto) [u8; 72]);

impl WrappedFileKey {
    /// Constructs a wrapped file key from a 72-byte wire blob.
    ///
    /// Used by callers that reconstruct a wrapped blob from stored bytes (e.g.
    /// a manifest record) before passing it to `unwrap_file_key`.
    pub fn new(bytes: [u8; 72]) -> Self {
        Self(bytes)
    }

    /// Returns the 72-byte wire blob.
    pub fn as_bytes(&self) -> &[u8; 72] {
        &self.0
    }
}

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

/// Builds the 20-byte chunk AAD `file_id || chunk_index` (big-endian `u32`).
///
/// Shared by `encrypt_chunk` and `decrypt_chunk` to guarantee identical AAD
/// construction on both sides.
pub(crate) fn build_chunk_aad(file_id: &FileId, chunk_index: ChunkIndex) -> [u8; 20] {
    let mut aad = [0u8; 20];
    aad[..16].copy_from_slice(file_id.as_bytes());
    aad[16..].copy_from_slice(&chunk_index.to_be_bytes());
    aad
}

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
        {
            let before_zeroize = unsafe { std::slice::from_raw_parts(pointer, 32) };
            assert_eq!(before_zeroize, &[0xAAu8; 32]);
        }

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
        {
            let before_zeroize = unsafe { std::slice::from_raw_parts(pointer, 32) };
            assert_eq!(before_zeroize, &[0x5Au8; 32]);
        }

        Zeroize::zeroize(&mut key_encryption_key.0);

        // SAFETY: `key_encryption_key` is still alive; the pointer remains valid
        // for 32 bytes and now points at the zeroized buffer.
        let after_zeroize = unsafe { std::slice::from_raw_parts(pointer, 32) };
        assert_eq!(after_zeroize, &[0u8; 32]);
    }

    #[test]
    fn test_sqlcipher_key_from_bytes_preserves_input() {
        let sqlcipher_key = SqlcipherKey::from_bytes([0x33u8; 32]);
        assert_eq!(sqlcipher_key.expose(), &[0x33u8; 32]);
    }

    #[test]
    fn test_manifest_key_from_bytes_preserves_input() {
        let manifest_key = ManifestKey::from_bytes([0x44u8; 32]);
        assert_eq!(manifest_key.expose(), &[0x44u8; 32]);
    }

    #[test]
    fn test_master_key_zeroize_trait_clears_memory() {
        let mut master_key = MasterKey::from_bytes([0x7Cu8; 32]);
        let pointer = master_key.expose().as_ptr();

        // SAFETY: `pointer` comes from `master_key.expose()` and the key remains
        // alive and allocated for this read.
        {
            let before_zeroize = unsafe { std::slice::from_raw_parts(pointer, 32) };
            assert_eq!(before_zeroize, &[0x7Cu8; 32]);
        }

        Zeroize::zeroize(&mut master_key.0);

        // SAFETY: `master_key` is still alive; the pointer remains valid for 32
        // bytes and now points at the zeroized buffer.
        let after_zeroize = unsafe { std::slice::from_raw_parts(pointer, 32) };
        assert_eq!(after_zeroize, &[0u8; 32]);
    }

    #[test]
    fn test_recovery_key_from_bytes_preserves_input() {
        let recovery_key = RecoveryKey::from_bytes([0x9Eu8; 32]);
        assert_eq!(recovery_key.expose(), &[0x9Eu8; 32]);
    }

    #[test]
    fn test_recovery_key_zeroize_trait_clears_memory() {
        let mut recovery_key = RecoveryKey::from_bytes([0x55u8; 32]);
        let pointer = recovery_key.expose().as_ptr();

        // SAFETY: `pointer` comes from `recovery_key.expose()` and the key
        // remains alive and allocated for this read.
        {
            let before_zeroize = unsafe { std::slice::from_raw_parts(pointer, 32) };
            assert_eq!(before_zeroize, &[0x55u8; 32]);
        }

        Zeroize::zeroize(&mut recovery_key.0);

        // SAFETY: `recovery_key` is still alive; the pointer remains valid for
        // 32 bytes and now points at the zeroized buffer.
        let after_zeroize = unsafe { std::slice::from_raw_parts(pointer, 32) };
        assert_eq!(after_zeroize, &[0u8; 32]);
    }

    #[test]
    fn test_vault_id_uuid_roundtrip_preserves_value() {
        let uuid = uuid::Uuid::new_v4();
        let vault_id = VaultId::from_uuid(uuid);

        assert_eq!(vault_id.to_uuid(), uuid);
        assert_eq!(vault_id.as_bytes(), uuid.as_bytes());
    }

    #[test]
    fn test_vault_id_new_preserves_bytes() {
        let bytes = [0x13u8; 16];
        let vault_id = VaultId::new(bytes);

        assert_eq!(*vault_id.as_bytes(), bytes);
    }
}
