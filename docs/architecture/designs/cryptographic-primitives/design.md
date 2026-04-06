# Arx Runa — Cryptographic Primitives Design

> Status: Design complete. Implementation target: Phase 1.
> Last updated: 2026-04-01 (reviewed)

---

## Goals

- Provide foundational cryptographic operations for all other Arx Runa modules
- HKDF-SHA256 key derivation producing vault-level keys from `master_key`
- XChaCha20-Poly1305 AEAD encryption with mandatory AAD binding
- Per-file random key generation, wrapping, and unwrapping
- BLAKE3 checksums over encrypted blobs
- All keys secured with `ZeroizeOnDrop` + `Secret<T>`

## Cipher Selection

**Cipher**: `XChaCha20Poly1305` only (not `ChaCha20Poly1305`)

- ✅ Use: `XChaCha20Poly1305` with 192-bit (24-byte) nonces
- ❌ Reject: `ChaCha20Poly1305` (96-bit nonces insufficient for random generation)
- ❌ Reject: AES-GCM (not used in Arx Runa)

**Rationale**: XChaCha20 extended nonce space (192-bit vs 96-bit) enables safe random nonce generation without collision concerns. With 96-bit nonces, the birthday bound becomes concerning after ~2^32 encryptions; with 192-bit, it's negligible even at 2^64 encryptions.

---

## Key Derivation

### Input

`master_key`: 32-byte output from Argon2id (provided by auth module). Held in mlocked memory; never stored.

### HKDF-SHA256 Expansion

RFC 5869 HKDF is used to derive three purpose-specific keys from `master_key`. Each key uses a distinct `info` string to ensure cryptographic separation.

| Key | Info String | Purpose |
|-----|-------------|---------|
| `key_encryption_key` | `b"voidgate-key-encryption"` | Wraps/unwraps per-file `file_key` values |
| `sqlcipher_key` | `b"voidgate-sqlcipher"` | SQLCipher database encryption |
| `manifest_key` | `b"voidgate-manifest-backup"` | Encrypts manifest cloud backup |

**Critical**: `master_key` is zeroed immediately after derivation. Never stored, never logged.

**Salt**: Empty (no salt). Argon2id output has sufficient entropy; a salt provides no additional security benefit here. HKDF internally uses a zero-filled salt of hash length when none is provided.

**Extensibility**: To add a new derived key, use the `derive-hkdf-key` skill. New keys are added by expanding with a distinct `info` string — the existing derived keys remain unchanged because HKDF produces independent outputs for different `info` values.

> **Note**: The file-sharing design (Phase 5) uses a separate HKDF derivation with `info="voidgate-share"` for ECIES share packages. That derivation uses an ECDH shared secret as IKM, not `master_key`, and is documented in `docs/architecture/designs/file-sharing/design.md`. It is a distinct key derivation tree and does not affect the vault-key derivations above.

### Rust Signature

```rust
/// Derives vault-level keys from the master key.
///
/// # Panics
/// Panics if HKDF expansion fails (should never happen with valid inputs).
pub fn derive_vault_keys(master_key: &MasterKey) -> VaultKeys;

pub struct VaultKeys {
    pub key_encryption_key: KeyEncryptionKey,
    pub sqlcipher_key: SqlcipherKey,
    pub manifest_key: ManifestKey,
}
```

---

## Per-File Key Management

Each file has a unique `file_key` — a random 256-bit value generated at file creation.

### Generation

```rust
/// Generates a cryptographically random file key.
pub fn generate_file_key() -> FileKey;
```

Uses `rand::thread_rng().gen::<[u8; 32]>()` (CSPRNG).

### Wrapping and Unwrapping

File keys are stored encrypted (wrapped) with `key_encryption_key`. The wrapping uses XChaCha20-Poly1305 with:

- **Nonce**: Random 24 bytes (same CSPRNG)
- **AAD**: Empty (the wrapped key is self-contained)
- **Plaintext**: 32-byte `file_key`

Wire format for `file_key_wrapped`:

```
[24-byte nonce | 32-byte encrypted file_key | 16-byte tag]
```

Total: 72 bytes.

```rust
/// Wraps a file key with the key encryption key.
pub fn wrap_file_key(
    file_key: &FileKey,
    key_encryption_key: &KeyEncryptionKey,
) -> WrappedFileKey;

/// Unwraps a file key.
///
/// # Errors
/// Returns `CryptoError::DecryptionFailed` if authentication fails.
pub fn unwrap_file_key(
    wrapped: &WrappedFileKey,
    key_encryption_key: &KeyEncryptionKey,
) -> Result<FileKey, CryptoError>;
```

---

## Chunk Encryption and Decryption

### Wire Format

```
[24-byte nonce | ciphertext | 16-byte Poly1305 tag]
```

Total overhead: 40 bytes per chunk.

### AAD Construction

Every AEAD operation on chunk data MUST include Associated Authenticated Data (AAD) binding the ciphertext to its file and position context. Operations on singleton blobs that use a purpose-specific key (e.g., the manifest backup encrypted with `manifest_key`) may omit AAD when there is no multi-instance context to bind — see the cloud-sync design for the manifest backup rationale.

```
AAD = file_id (16 bytes, UUID as raw bytes) || chunk_index (4 bytes, big-endian u32)
```

This prevents:
- Chunk reordering attacks (wrong `chunk_index` fails authentication)
- Cross-file substitution attacks (wrong `file_id` fails authentication)

### Rust Signatures

```rust
/// Encrypts a chunk in place, returning the wire-format blob.
///
/// # Arguments
/// * `plaintext` - The chunk data (will be consumed)
/// * `file_key` - The file's encryption key
/// * `file_id` - The file's unique identifier
/// * `chunk_index` - The chunk's position (0-based)
///
/// # Returns
/// Wire-format blob: [nonce | ciphertext | tag]
pub fn encrypt_chunk(
    plaintext: Vec<u8>,
    file_key: &FileKey,
    file_id: &FileId,
    chunk_index: ChunkIndex,
) -> Vec<u8>;

/// Decrypts a wire-format blob, returning the plaintext.
///
/// # Errors
/// Returns `CryptoError::DecryptionFailed` if authentication fails.
/// Returns `CryptoError::InvalidBlobFormat` if the blob is too short.
pub fn decrypt_chunk(
    blob: &[u8],
    file_key: &FileKey,
    file_id: &FileId,
    chunk_index: ChunkIndex,
) -> Result<Vec<u8>, CryptoError>;
```

### Implementation Strategy

1. **Encrypt**: Use `encrypt_in_place_detached()` for explicit control
   - Generate random 24-byte nonce
   - Construct AAD
   - Encrypt plaintext in-place, receive tag
   - Assemble wire format: `nonce || ciphertext || tag`

2. **Decrypt**: Parse wire format, verify, decrypt
   - Extract nonce (bytes 0..24)
   - Extract tag (last 16 bytes)
   - Extract ciphertext (bytes 24..len-16)
   - Construct AAD
   - Decrypt in-place with `decrypt_in_place_detached()`

---

## Nonce Generation

All nonces are 24 bytes (192 bits) generated via CSPRNG.

```rust
/// Generates a random 24-byte nonce for XChaCha20-Poly1305.
pub fn generate_nonce() -> [u8; 24];
```

**Requirements**:
- ✅ Use: CSPRNG (e.g., `rand::thread_rng()`)
- ❌ Reject: Sequential nonces, counter-based nonces, derived nonces

**Rationale**: Sequential nonces create catastrophic failure if counter is reset or reused. Random 192-bit nonces have negligible collision probability (2^-64 after 2^64 encryptions).

### Security Properties

- **No sequential nonces**: Sequential or counter-based nonces are rejected. They create catastrophic failure modes if the counter is ever reset or reused.
- **Birthday bound**: With 192-bit nonces, the collision probability after 2^64 encryptions is ~2^-64. Arx Runa's use case (personal file storage) will never approach this limit.
- **Per-file key isolation**: Even if a nonce collision occurred, the per-file key model limits the impact to a single file.

---

## BLAKE3 Checksum

Checksums are computed over the **encrypted blob** (not plaintext). This allows integrity verification before decryption.

```rust
/// Computes a BLAKE3 checksum over encrypted data.
pub fn compute_checksum(encrypted_blob: &[u8]) -> Blake3Hash;

/// 32-byte BLAKE3 hash.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Blake3Hash([u8; 32]);
```

### Usage Flow

1. After encryption: `checksum = compute_checksum(blob)`
2. Store checksum in manifest alongside chunk metadata
3. Before decryption: verify `compute_checksum(downloaded_blob) == stored_checksum`
4. If mismatch: abort without attempting decryption

---

## Type Definitions

### Key Types (all `ZeroizeOnDrop` + `Secret<T>`)

```rust
use secrecy::Secret;
use zeroize::ZeroizeOnDrop;

/// 256-bit file encryption key.
#[derive(ZeroizeOnDrop)]
pub struct FileKey(Secret<[u8; 32]>);

/// 256-bit key encryption key (wraps file keys).
#[derive(ZeroizeOnDrop)]
pub struct KeyEncryptionKey(Secret<[u8; 32]>);

/// 256-bit SQLCipher key.
#[derive(ZeroizeOnDrop)]
pub struct SqlcipherKey(Secret<[u8; 32]>);

/// 256-bit manifest backup key.
#[derive(ZeroizeOnDrop)]
pub struct ManifestKey(Secret<[u8; 32]>);

/// Wrapped file key (72 bytes: nonce + encrypted key + tag).
pub struct WrappedFileKey([u8; 72]);
```

### Domain Types (newtypes for type safety)

```rust
/// File identifier (UUID v4 as raw bytes).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FileId([u8; 16]);

impl FileId {
    pub fn new(bytes: [u8; 16]) -> Self { Self(bytes) }
    pub fn as_bytes(&self) -> &[u8; 16] { &self.0 }
    pub fn from_uuid(uuid: uuid::Uuid) -> Self { Self(*uuid.as_bytes()) }
    pub fn to_uuid(&self) -> uuid::Uuid { uuid::Uuid::from_bytes(self.0) }
}

/// Chunk position within a file (0-based).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ChunkIndex(u32);

impl ChunkIndex {
    pub fn new(index: u32) -> Self { Self(index) }
    pub fn as_u32(&self) -> u32 { self.0 }
    pub fn to_be_bytes(&self) -> [u8; 4] { self.0.to_be_bytes() }
}
```

---

## Error Handling

```rust
use thiserror::Error;

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum CryptoError {
    #[error("decryption failed: authentication tag mismatch")]
    DecryptionFailed,

    #[error("invalid blob format: expected at least {expected} bytes, got {actual}")]
    InvalidBlobFormat { expected: usize, actual: usize },

    #[error("key unwrap failed")]
    KeyUnwrapFailed,
}
```

---

## Module Structure

```
src-tauri/src/crypto/
├── mod.rs                  # Re-exports public API
├── error.rs                # CryptoError enum
├── types/
│   ├── mod.rs              # Re-exports types
│   ├── file_key.rs         # FileKey
│   ├── key_encryption_key.rs
│   ├── sqlcipher_key.rs
│   ├── manifest_key.rs
│   ├── wrapped_file_key.rs
│   ├── file_id.rs          # FileId newtype
│   ├── chunk_index.rs      # ChunkIndex newtype
│   └── blake3_hash.rs      # Blake3Hash
├── hkdf.rs                 # derive_vault_keys()
├── nonce.rs                # generate_nonce()
├── encrypt_chunk.rs        # encrypt_chunk()
├── decrypt_chunk.rs        # decrypt_chunk()
├── wrap_key.rs             # wrap_file_key(), unwrap_file_key()
├── checksum.rs             # compute_checksum()
└── generate_file_key.rs    # generate_file_key()
```

---

## Data Flow Diagrams

- [Chunk Encryption Flow](diagrams/chunk-encryption-flow.md) — internal flow of `encrypt_chunk`
- [Key Derivation Flow](diagrams/key-derivation-flow.md) — HKDF-SHA256 expansion from `master_key` to vault keys

---

## Test Plan

### Unit Tests

| Test | Description |
|------|-------------|
| `test_encrypt_decrypt_round_trip` | Encrypt then decrypt returns original plaintext |
| `test_decrypt_wrong_file_id_fails` | Decryption with different `file_id` fails authentication |
| `test_decrypt_wrong_chunk_index_fails` | Decryption with different `chunk_index` fails authentication |
| `test_decrypt_wrong_key_fails` | Decryption with different `file_key` fails authentication |
| `test_decrypt_corrupted_ciphertext_fails` | Flipping a ciphertext bit fails authentication |
| `test_decrypt_corrupted_tag_fails` | Flipping a tag bit fails authentication |
| `test_decrypt_truncated_blob_fails` | Blob shorter than 40 bytes returns `InvalidBlobFormat` |
| `test_nonce_uniqueness` | 1000 generated nonces are all unique |
| `test_wrap_unwrap_file_key_round_trip` | Wrap then unwrap returns original `file_key` |
| `test_unwrap_wrong_kek_fails` | Unwrapping with different `key_encryption_key` fails |
| `test_derive_vault_keys_deterministic` | Same `master_key` produces same derived keys |
| `test_derive_vault_keys_different_inputs` | Different `master_key` produces different derived keys |
| `test_checksum_detects_corruption` | Flipping a byte changes the checksum |
| `test_zeroize_file_key_on_drop` | After drop, memory contains zeros (unsafe inspection) |
| `test_zeroize_kek_on_drop` | After drop, memory contains zeros (unsafe inspection) |

### Property-Based Tests (proptest)

| Property | Description |
|----------|-------------|
| `prop_encrypt_decrypt_identity` | For all plaintexts, `decrypt(encrypt(p)) == p` |
| `prop_different_nonces` | For all plaintexts, two encryptions produce different ciphertexts |
| `prop_checksum_deterministic` | `checksum(blob) == checksum(blob)` always |

---

## Security Considerations

### Nonce Reuse Prevention

Random nonces from CSPRNG are the only safe strategy. Sequential counters would require persistent state across process restarts, creating failure modes if the counter file is corrupted, restored from backup, or shared between processes.

### AAD Binding

Omitting AAD would allow an attacker who can reorder blobs to:
- Swap chunks between files (if `file_id` is not bound)
- Reorder chunks within a file (if `chunk_index` is not bound)

Both attacks are prevented by mandatory AAD.

### Zeroization

All key types implement `ZeroizeOnDrop` to ensure sensitive key material is overwritten before memory is released. Tests verify this behavior using unsafe pointer inspection.

---

## Dependencies

```toml
[dependencies]
chacha20poly1305 = "0.10"  # XChaCha20-Poly1305 AEAD
hkdf = "0.12"              # HKDF-SHA256
sha2 = "0.10"              # SHA-256 for HKDF
blake3 = "1.5"             # BLAKE3 checksum
rand = "0.8"               # CSPRNG
uuid = { version = "1.0", features = ["v4"] }
zeroize = { version = "1.7", features = ["derive"] }
secrecy = "0.8"            # Secret<T> wrapper
thiserror = "1.0"          # Error handling
```

---

## Open Decisions

None — all design decisions have been made.

---

## Decisions Made

| Decision | Choice | Rationale |
|----------|--------|-----------|
| HKDF salt | Empty | Argon2id output has sufficient entropy |
| File ID representation | `FileId` newtype wrapping `[u8; 16]` | Type safety + compact AAD |
| Encryption API | `encrypt_in_place_detached()` | Explicit control over wire format |
| Zeroize verification | Unsafe pointer inspection in tests | Rigorous verification of memory clearing |
| Nonce strategy | Random 192-bit via CSPRNG | Avoids counter persistence issues |
| AAD format | `file_id \|\| chunk_index` (big-endian) | Binds ciphertext to context |

---

## Related Documents

- [Key Derivation Tree Diagram](diagrams/key-derivation-tree.md)
- [Chunk Encryption Flow](diagrams/chunk-encryption-flow.md)
- [Key Derivation Flow](diagrams/key-derivation-flow.md)
- [File Sharing Architecture](../file-sharing/design.md) — per-file key rationale
- [ADR 001 — Code Structure](../../../architecture-decisions/001-code-structure-and-patterns.md)
- Roadmap Phase 1 deliverables
