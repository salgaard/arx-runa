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

## Contract Surface

### Interface contract

- Public API surface includes `derive_vault_keys`, `generate_file_key`, `wrap_file_key`, `unwrap_file_key`, `wrap_master_key_for_recovery`, `unwrap_master_key_from_recovery`, `encrypt_chunk`, `decrypt_chunk`, `generate_nonce`, `compute_checksum`, and `verify_checksum`.
- Chunk encryption/decryption contract is contextual (`FileId`, `ChunkIndex`) and returns typed `CryptoError` results.
- Wire-format outputs are explicit byte-layout contracts for wrapped keys and encrypted chunks.

### Data contract

- Canonical key containers are `VaultKeys`, `FileKey`, `KeyEncryptionKey`, `SqlcipherKey`, `ManifestKey`, `RecoveryKey`, `WrappedFileKey`, and `WrappedMasterKey`.
- Canonical domain/value types are `FileId`, `ChunkIndex`, `Blake3Hash`, and `VerifiedBlob`.
- Canonical encodings are chunk blobs `[24-byte nonce | ciphertext | 16-byte tag]` and wrapped keys `[24-byte nonce | 32-byte ciphertext | 16-byte tag]`.

### Invariant contract

- Cipher contract is `XChaCha20Poly1305` only; nonces are random 24-byte CSPRNG values.
- Chunk AEAD contract requires `AAD = file_id || chunk_index` (big-endian `u32`) for every chunk encrypt/decrypt operation.
- `master_key` is never persisted and is zeroized after derivation; checksum verification precedes decrypt via `VerifiedBlob`.
- Cross-phase invariant reference: [docs/architecture/design-invariants.md](../../design-invariants.md).

### Dependency contract

- Consumes auth-derived `master_key` and produces `key_encryption_key`, `sqlcipher_key`, and `manifest_key` for later phases.
- Depends on `hkdf`/`sha2`, `chacha20poly1305`, `rand`, `blake3`, `secrecy`, `zeroize`, `uuid`, and `thiserror`.
- Recovery-slot wrapping semantics align with the authentication and cloud vault-header designs.

---

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
| `key_encryption_key` | `b"arx-runa-key-encryption"` | Wraps/unwraps per-file `file_key` values |
| `sqlcipher_key` | `b"arx-runa-sqlcipher"` | SQLCipher database encryption |
| `manifest_key` | `b"arx-runa-manifest-backup"` | Encrypts manifest cloud backup |

**Critical**: `master_key` is zeroed immediately after derivation. Never stored, never logged.

**Salt**: Fixed domain separator `b"arx-runa-v1"`. Argon2id output has sufficient entropy, so the salt provides no additional entropy mixing — but RFC 5869 recommends a fixed salt even with high-entropy IKM to act as a domain separator, preventing cross-application key confusion if the same `master_key` were ever fed into a different HKDF context. The value encodes application identity and key hierarchy version.

**Extensibility**: To add a new derived key, use the `derive-hkdf-key` skill. New keys are added by expanding with a distinct `info` string — the existing derived keys remain unchanged because HKDF produces independent outputs for different `info` values.

> **Note**: The file-sharing design (Phase 5) uses a separate HKDF derivation with `info="arx-runa-share"` for ECIES share packages. That derivation uses an ECDH shared secret as IKM, not `master_key`, and is documented in `docs/architecture/designs/file-sharing/design.md`. It is a distinct key derivation tree and does not affect the vault-key derivations above.

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

Uses `rand::rng().random::<[u8; 32]>()` (CSPRNG).

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

### Recovery Master Key Wrapping

The recovery slot wraps `master_key` — not a `file_key`. This is a distinct operation with a distinct type and mandatory non-empty AAD. The AAD binds the ciphertext to its vault and purpose, preventing:

- **Cross-vault transplant attacks**: a recovery slot from vault A cannot unlock vault B (different `vault_id`)
- **Cross-slot confusion**: recovery slot blobs are authenticated differently from `file_key_wrapped` blobs (different AAD)

The wire format is identical to `WrappedFileKey` (72 bytes: 24-byte nonce + 32-byte ciphertext + 16-byte tag).

**AAD construction**:

```
aad = b"arx-runa recovery v1" || vault_id_bytes
```

where `vault_id_bytes` is the UUID v4 bytes of the vault (16 bytes raw, not the hyphenated string form).

```rust
/// Wraps `master_key` for storage in a vault header recovery slot.
///
/// # Arguments
/// * `master_key` - The 32-byte vault master key; borrowed for the duration of the wrap
///   operation. The caller retains ownership and zeroization is guaranteed by
///   `MasterKey`'s `ZeroizeOnDrop` implementation when the caller's binding is dropped.
/// * `recovery_key` - Derived from the user's BIP-39 recovery phrase via Argon2id
/// * `vault_id` - The vault's UUID v4; included in AAD to prevent cross-vault attacks
///
/// # Returns
/// 72-byte wire blob: [24-byte nonce | 32-byte encrypted master_key | 16-byte tag]
pub fn wrap_master_key_for_recovery(
    master_key: &MasterKey,
    recovery_key: &RecoveryKey,
    vault_id: &VaultId,
) -> WrappedMasterKey;

/// Unwraps `master_key` from a vault header recovery slot.
///
/// # Arguments
/// * `wrapped` - The 72-byte wire blob from the vault header recovery slot
/// * `recovery_key` - Derived from the user's BIP-39 recovery phrase via Argon2id
/// * `vault_id` - Must match the vault_id used during wrapping
///
/// # Errors
/// Returns `CryptoError::DecryptionFailed` if authentication fails (wrong phrase or wrong vault).
pub fn unwrap_master_key_from_recovery(
    wrapped: &WrappedMasterKey,
    recovery_key: &RecoveryKey,
    vault_id: &VaultId,
) -> Result<MasterKey, CryptoError>;
```

New types:

```rust
/// 32-byte recovery key derived from the user's BIP-39 phrase via Argon2id.
/// Zeroized on drop. Never stored — derived on demand from the phrase.
pub struct RecoveryKey(Zeroizing<[u8; 32]>);

/// 72-byte wire blob: [nonce | encrypted master_key | tag].
/// Stored in the vault header recovery slot. Does not implement ZeroizeOnDrop
/// because it is ciphertext, not key material.
pub struct WrappedMasterKey([u8; 72]);
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
- ✅ Use: CSPRNG (e.g., `rand::rng().random::<[u8; 24]>()`)
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
3. Before decryption: call `verify_checksum(blob, expected)` → returns `VerifiedBlob` or `ChecksumMismatch` error
4. Pass `VerifiedBlob` to `decrypt_chunk` — the type system enforces the check-before-decrypt order

### VerifiedBlob Newtype

To structurally prevent calling `decrypt_chunk` on an unverified blob, `verify_checksum` returns an opaque `VerifiedBlob` wrapper. `decrypt_chunk` accepts only `VerifiedBlob`, making it a compile error to skip the checksum check.

```rust
/// Opaque wrapper — only constructible by verify_checksum.
pub struct VerifiedBlob(Vec<u8>);

/// Verifies the BLAKE3 checksum of an encrypted blob.
///
/// # Errors
/// Returns `CryptoError::ChecksumMismatch` if the checksum does not match.
pub fn verify_checksum(
    blob: Vec<u8>,
    expected: &Blake3Hash,
) -> Result<VerifiedBlob, CryptoError>;
```

`decrypt_chunk` signature updated to accept `VerifiedBlob`:

```rust
pub fn decrypt_chunk(
    blob: VerifiedBlob,
    file_key: &FileKey,
    file_id: &FileId,
    chunk_index: ChunkIndex,
) -> Result<Vec<u8>, CryptoError>;
```

**Rationale**: The BLAKE3 checksum is unkeyed — it provides fast detection of hardware/network corruption before the more expensive AEAD decryption. The manifest (SQLCipher) protects the stored hashes, so unkeyed is operationally sufficient. The `VerifiedBlob` newtype is a zero-cost mechanism that enforces the correct call order at compile time.

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

    #[error("checksum mismatch: blob has been tampered with or corrupted")]
    ChecksumMismatch,
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

### Key Non-Commitment

XChaCha20-Poly1305 is not a *committing* AEAD — it does not provide binding security, meaning it is theoretically possible to find two different keys that both authenticate the same ciphertext. For symmetric file encryption this is not a practical concern: there is no protocol interaction where an attacker can present a ciphertext and ask the vault to try multiple keys. This limitation is relevant to the Phase 5 file-sharing layer (ECIES share package import), where it should be revisited.

### Cipher Upgrade Path

AEGIS-256 (IETF CFRG draft-irtf-cfrg-aegis-aead) offers ~2× higher throughput on hardware with AES-NI instructions (~0.7 cpb vs ~1.5 cpb), a 256-bit nonce safe for random generation, and ephemeral key erasure before data processing. Once AEGIS-256 reaches RFC status and a Rust audit is available, it is a strong upgrade candidate for XChaCha20-Poly1305 — the wire format and API surface would remain identical.

---

## Dependencies

```toml
[dependencies]
chacha20poly1305 = "0.10"  # XChaCha20-Poly1305 AEAD
hkdf = "0.13"              # HKDF-SHA256
sha2 = "0.11"              # SHA-256 for HKDF
blake3 = "1"               # BLAKE3 checksum
rand = "0.10"              # CSPRNG — must be >= 0.9 for Rust edition 2024 (`gen` keyword reserved); 0.10 is current stable
uuid = { version = "1", features = ["v4"] }
zeroize = { version = "1", features = ["derive"] }
secrecy = "0.10"           # Secret<T> wrapper
thiserror = "2"            # Error handling
```

> **Note on `rand` 0.10 API**: `rand` 0.10 drops `thread_rng()` and the `.gen()` method. Use `rand::rng().random::<[u8; 32]>()`. In Rust 2024, `gen` is a reserved keyword — `.gen()` is unavailable anyway. The scaffolding pins `rand = "0.10"` (current stable since 2026-02-08).

---

## Open Decisions

None — all design decisions have been made.

---

## Decisions Made

| Decision | Choice | Rationale |
|----------|--------|-----------|
| HKDF salt | `b"arx-runa-v1"` (domain separator) | RFC 5869 recommends a fixed salt for domain separation even with high-entropy IKM |
| File ID representation | `FileId` newtype wrapping `[u8; 16]` | Type safety + compact AAD |
| Encryption API | `encrypt_in_place_detached()` | Explicit control over wire format |
| Zeroize verification | Unsafe pointer inspection in tests | Rigorous verification of memory clearing |
| Nonce strategy | Random 192-bit via CSPRNG | Avoids counter persistence issues |
| AAD format | `file_id \|\| chunk_index` (big-endian) | Binds ciphertext to context |
| BLAKE3 mode | Unkeyed; `VerifiedBlob` newtype enforces check-before-decrypt | Manifest is SQLCipher-encrypted so unkeyed is operationally sufficient; newtype makes skipping the check a compile error |
| Recovery slot AAD | `b"arx-runa recovery v1" \|\| vault_id_bytes` | Prevents cross-vault transplant attacks (vault_id binds to vault) and cross-slot confusion with `file_key_wrapped` blobs (different AAD domain) |
| Recovery wrapping uses dedicated functions | `wrap_master_key_for_recovery` / `unwrap_master_key_from_recovery` distinct from `wrap_file_key` | Type system enforces correct AAD usage; `MasterKey` and `FileKey` wrapping cannot be confused |

---

## Related Documents

- [Key Derivation Tree Diagram](diagrams/key-derivation-tree.md)
- [Key Derivation — Recovery Slot Diagram](diagrams/key-derivation-recovery-slot.md)
- [Chunk Encryption Flow](diagrams/chunk-encryption-flow.md)
- [Key Derivation Flow](diagrams/key-derivation-flow.md)
- [File Sharing Architecture](../file-sharing/design.md) — per-file key rationale
- [ADR 001 — Code Structure](../../../architecture-decisions/001-code-structure-and-patterns.md)
- Roadmap Phase 1 deliverables
