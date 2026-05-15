# Cryptographic Primitives

Arx Runa's cryptographic layer provides all foundational operations: key derivation, chunk encryption, file key management, and integrity checking. Every other module consumes these primitives — they are the single source of truth for cipher selection, wire formats, and key types.

---

## Goals

- HKDF-SHA256 key derivation producing vault-level keys from `master_key`
- XChaCha20-Poly1305 AEAD encryption with mandatory AAD binding
- Per-file random key generation, wrapping, and unwrapping
- BLAKE3 checksums over encrypted blobs
- All keys secured with `ZeroizeOnDrop` + `SecretBox<[u8; 32]>`

---

## Contract Surface

### Interface

Public functions: `derive_vault_keys`, `generate_file_key`, `wrap_file_key`, `unwrap_file_key`, `wrap_master_key_for_recovery`, `unwrap_master_key_from_recovery`, `encrypt_chunk`, `decrypt_chunk`, `generate_nonce`, `compute_checksum`, `verify_checksum`.

Chunk encrypt/decrypt accept contextual binding (`FileId`, `ChunkIndex`) and return typed `CryptoError` results. Wire-format outputs are explicit byte-layout contracts.

### Data

Canonical key containers: `VaultKeys`, `FileKey`, `KeyEncryptionKey`, `SqlcipherKey`, `ManifestKey`, `RecoveryKey`, `WrappedFileKey`, `WrappedMasterKey`.

Canonical domain types: `FileId`, `ChunkIndex`, `Blake3Hash`, `VerifiedBlob`.

Canonical encodings:
- Chunk blobs: `[24-byte nonce | ciphertext | 16-byte tag]`
- Wrapped keys: `[24-byte nonce | 32-byte ciphertext | 16-byte tag]` (72 bytes)

### Invariants

- Cipher is `XChaCha20Poly1305` only; nonces are random 24-byte CSPRNG values.
- Chunk AEAD requires `AAD = file_id || chunk_index` (big-endian `u32`) on every encrypt/decrypt.
- `master_key` is never persisted and is zeroized after derivation.
- Checksum verification precedes decrypt via the `VerifiedBlob` newtype.

### Dependencies

Consumes auth-derived `master_key`. Produces `key_encryption_key`, `sqlcipher_key`, `manifest_key`.

Crates: `chacha20poly1305`, `hkdf`, `sha2`, `blake3`, `rand`, `secrecy`, `zeroize`, `uuid`, `thiserror`.

---

## Cipher Selection

**`XChaCha20Poly1305`** with 192-bit (24-byte) nonces. XChaCha20's extended nonce space enables safe random nonce generation: the birthday bound at 192 bits is negligible even at 2⁶⁴ encryptions, unlike the 96-bit nonces of standard ChaCha20-Poly1305.

---

## Key Derivation

`master_key` is the 32-byte Argon2id output from the auth module, held in mlocked memory and never stored.

HKDF-SHA256 (RFC 5869) expands `master_key` into three purpose-specific keys using a fixed domain-separator salt (`b"arx-runa-v1"`) and distinct `info` strings:

| Key | Info string | Purpose |
|-----|-------------|---------|
| `key_encryption_key` | `b"arx-runa-key-encryption"` | Wraps per-file `file_key` values |
| `sqlcipher_key` | `b"arx-runa-sqlcipher"` | SQLCipher database encryption |
| `manifest_key` | `b"arx-runa-manifest-backup"` | Encrypts manifest cloud backup |

`master_key` is zeroed immediately after HKDF expansion. New derived keys are added by extending with a distinct `info` string — existing keys remain unchanged.

```rust
pub fn derive_vault_keys(master_key_bytes: &[u8; 32]) -> VaultKeys;

pub struct VaultKeys {
    pub key_encryption_key: KeyEncryptionKey,
    pub sqlcipher_key: SqlcipherKey,
    pub manifest_key: ManifestKey,
}
```

---

## Per-File Key Management

Each file has a unique `file_key` — a random 256-bit value generated at file creation.

File keys are stored encrypted (wrapped) with `key_encryption_key` using XChaCha20-Poly1305 with a random 24-byte nonce. Wire format: `[24-byte nonce | 32-byte encrypted file_key | 16-byte tag]` (72 bytes total).

```rust
pub fn generate_file_key() -> FileKey;
pub fn wrap_file_key(file_key: &FileKey, key_encryption_key: &KeyEncryptionKey) -> WrappedFileKey;
pub fn unwrap_file_key(wrapped: &WrappedFileKey, key_encryption_key: &KeyEncryptionKey) -> Result<FileKey, CryptoError>;
```

### Recovery master key wrapping

The recovery slot wraps `master_key` (not a `file_key`) under a separate `RecoveryKey` derived from a BIP-39 phrase. Mandatory non-empty AAD binds the ciphertext to its vault and purpose, preventing cross-vault transplant attacks:

```
aad = b"arx-runa recovery v1" || vault_id_bytes
```

Wire format is identical to `WrappedFileKey` (72 bytes).

---

## Chunk Encryption

### Wire format

```
[24-byte nonce | ciphertext | 16-byte Poly1305 tag]
```

40 bytes overhead per chunk. No per-blob version byte — format changes are coordinated through `schema_version` in the vault header.

### AAD construction

```
AAD = file_id (16 bytes, UUID raw bytes) || chunk_index (4 bytes, big-endian u32)
```

This prevents chunk reordering attacks (wrong `chunk_index` fails authentication) and cross-file substitution attacks (wrong `file_id` fails authentication).

```rust
pub fn encrypt_chunk(plaintext: Vec<u8>, file_key: &FileKey, file_id: &FileId, chunk_index: ChunkIndex) -> Vec<u8>;
pub fn decrypt_chunk(blob: VerifiedBlob, file_key: &FileKey, file_id: &FileId, chunk_index: ChunkIndex) -> Result<Vec<u8>, CryptoError>;
```

---

## BLAKE3 Integrity

Checksums are computed over **encrypted blobs** (not plaintext), enabling integrity verification before decryption. The `verify_checksum` function returns a `VerifiedBlob` newtype — `decrypt_chunk` accepts only `VerifiedBlob`, making it a compile error to skip the checksum step.

```rust
pub fn compute_checksum(encrypted_blob: &[u8]) -> Blake3Hash;
pub fn verify_checksum(blob: Vec<u8>, expected: &Blake3Hash) -> Result<VerifiedBlob, CryptoError>;
```

BLAKE3 is unkeyed. The stored checksums are inside SQLCipher (encrypted with `sqlcipher_key`), so unkeyed is operationally sufficient. The `VerifiedBlob` newtype enforces correct call order at zero runtime cost.

---

## Key Types

All key types implement `ZeroizeOnDrop` and are backed by `SecretBox<[u8; 32]>`:

```rust
pub struct FileKey(SecretBox<[u8; 32]>);
pub struct KeyEncryptionKey(SecretBox<[u8; 32]>);
pub struct SqlcipherKey(SecretBox<[u8; 32]>);
pub struct ManifestKey(SecretBox<[u8; 32]>);
pub struct WrappedFileKey([u8; 72]);     // ciphertext, not key material
pub struct WrappedMasterKey([u8; 72]);  // ciphertext, not key material
```

Domain types for type safety:

```rust
pub struct FileId([u8; 16]);    // UUID v4 as raw bytes
pub struct ChunkIndex(u32);    // 0-based chunk position
pub struct Blake3Hash([u8; 32]);
pub struct VerifiedBlob(Vec<u8>); // only constructible by verify_checksum
```

---

## Error Types

```rust
#[non_exhaustive]
pub enum CryptoError {
    DecryptionFailed,
    InvalidBlobFormat { expected: usize, actual: usize },
    KeyUnwrapFailed,
    ChecksumMismatch,
}
```

---

## Security Properties

**Nonce reuse prevention.** Random 192-bit nonces from CSPRNG are the only safe strategy. Sequential counters create catastrophic failure if the counter is reset, restored from backup, or shared across processes.

**AAD binding.** Omitting AAD would allow an attacker to swap chunks between files or reorder chunks within a file. Mandatory AAD prevents both.

**Zeroization.** All key types implement `ZeroizeOnDrop` to overwrite memory before release.

**Key non-commitment.** XChaCha20-Poly1305 is not a committing AEAD. For symmetric file encryption this is not a practical concern — there is no protocol interaction where a vault tries multiple keys on the same ciphertext. The file-sharing layer uses `CTX-ChaCha20-Poly1305` (a committing AEAD) for share packages where key commitment matters. See [File Sharing](design-file-sharing.md).

---

## Related Documents

- [Authentication and Session Management](design-authentication.md) — Argon2id producing `master_key`
- [Chunking and Manifest](design-chunking-and-manifest.md) — `encrypt_chunk` / `decrypt_chunk` consumers
- [File Sharing](design-file-sharing.md) — HPKE + CTX-ChaCha20-Poly1305 for share packages
- [Cloud Synchronisation](design-cloud-synchronisation.md) — `manifest_key` usage
