---
title: "Phase 1.2 — AEAD Encrypt/Decrypt with Wire Format"
created: "2026-04-13T00:00:00Z"
status: implemented
roadmap-phase: 1
sub-phase: "1.2"
design-document: "docs/architecture/designs/cryptographic-primitives/design.md"
sub-phase-roadmap: "docs/architecture/designs/cryptographic-primitives/sub-phases/roadmap.md"
test-agent-required: false
tags: [crypto, phase-1, aead, xchacha20poly1305, wire-format]
---

# Plan: Phase 1.2 — AEAD Encrypt/Decrypt with Wire Format

## 1. Goal

Implement `encrypt_chunk` and `decrypt_chunk` on XChaCha20-Poly1305 with mandatory `file_id || chunk_index` AAD binding and the canonical `[24-byte nonce | ciphertext | 16-byte tag]` wire format, so that Phase 1.3 (checksums, key wrapping) and later storage phases can rely on a stable chunk encrypt/decrypt surface.

## 2. Context

**Roadmap**: Phase 1 — Cryptographic Primitives (`docs/roadmap.md` lines 44–49). Depends on Phase 1.1 (key types, `CryptoError`, `generate_nonce`), implemented in commit `35a46fb`.

**Sub-phase roadmap**: `docs/architecture/designs/cryptographic-primitives/sub-phases/roadmap.md`. Strict order 1.1 → 1.2 → 1.3.

**Sub-phase doc**: `docs/architecture/designs/cryptographic-primitives/sub-phases/1.2-aead-encrypt-decrypt.md`. Estimated scope ~120 LOC production + ~150 LOC tests.

**Parent design**: `docs/architecture/designs/cryptographic-primitives/design.md`. Relevant canonical commitments (`## Contract Surface`, lines 17–43):

- Public API includes `encrypt_chunk` and `decrypt_chunk` (§ Interface contract).
- Wire format is `[24-byte nonce | ciphertext | 16-byte tag]` (§ Data contract; see also "Chunk Encryption and Decryption" lines 219–223).
- Cipher is `XChaCha20Poly1305` only; nonces are random 24-byte CSPRNG values (§ Invariant contract).
- Chunk AEAD requires `AAD = file_id || chunk_index` (big-endian `u32`) for every encrypt/decrypt operation (§ Invariant contract; design-invariants #1).
- Checksum verification precedes decrypt via `VerifiedBlob` in the fully wired design, but `VerifiedBlob` is defined in Phase 1.3 — see Design Concern #1 below.

**Existing state** (post Phase 1.1, commit `35a46fb`):
- `src-tauri/src/crypto/mod.rs` declares `pub mod error; pub mod hkdf; pub mod nonce; pub mod types;` and re-exports `CryptoError`, `VaultKeys`, `derive_vault_keys`, `generate_nonce`, and key/domain newtypes.
- `src-tauri/src/crypto/types/mod.rs` defines `FileKey`, `KeyEncryptionKey`, `SqlcipherKey`, `ManifestKey`, `WrappedFileKey`, `FileId`, `ChunkIndex`, `Blake3Hash`. `FileKey::expose(&self) -> &[u8; 32]` is `pub(crate)` and available from within `src-tauri/src/crypto/`.
- `src-tauri/src/crypto/error.rs` defines `CryptoError::{DecryptionFailed, InvalidBlobFormat { expected, actual }, KeyUnwrapFailed, ChecksumMismatch}`.
- `src-tauri/src/crypto/nonce.rs` defines `pub fn generate_nonce() -> [u8; 24]`.
- `src-tauri/Cargo.toml` already pins `chacha20poly1305 = "0.10"` in `[dependencies]` and `proptest = "1"` in `[dev-dependencies]`. No dependency additions are required.
- `src-tauri/src/crypto/encrypt_chunk.rs` and `src-tauri/src/crypto/decrypt_chunk.rs` do not yet exist.
- No `VerifiedBlob` type exists yet; it is scheduled for Phase 1.3 (`src-tauri/src/crypto/checksum.rs`).

## 3. Design Concerns / Open Questions

### Concern #1 — `decrypt_chunk` signature uses `VerifiedBlob`, but `VerifiedBlob` is defined in Phase 1.3
- **Source**: Sub-phase 1.2 Deliverable #2 (line 13) specifies `decrypt_chunk(blob: VerifiedBlob, ...)`. Parent design lines 353–362 show the same final signature. However, sub-phase 1.3 Implementation Notes line 59 explicitly states: "Phase 1.3 finalises the `decrypt_chunk` signature that Phase 1.2 stubbed with a `todo!()` or `blob: Vec<u8>` placeholder — update the signature and all tests in `decrypt_chunk.rs` to accept `VerifiedBlob` as part of this phase."
- **Impact**: If Phase 1.2 tries to use `VerifiedBlob` directly it either cannot compile (type doesn't exist) or forces 1.2 to scope-creep into 1.3's `checksum.rs` deliverables.
- **Classification**: Non-blocking. Sub-phase 1.3 already documents the intended resolution.
- **Resolution**: Phase 1.2 implements `decrypt_chunk(blob: &[u8], ...)` as a temporary signature. Phase 1.3 will (a) introduce `VerifiedBlob` in `checksum.rs`, (b) change `decrypt_chunk`'s first parameter from `&[u8]` to `VerifiedBlob`, (c) update the imports in `decrypt_chunk.rs` and its test module, and (d) update any call sites (none outside the crypto module in Phase 1). The temporary signature matches the design.md *earlier* signature block (lines 267–273) exactly, so this is not a deviation from the design — only from the ordering of when the final signature lands.
- **Documentation sync required on implementation**: None. The final `VerifiedBlob`-based signature remains the canonical end state; Phase 1.2 is an intermediate step already foreseen by 1.3's implementation notes.

### Concern #2 — Plaintext buffer state on decrypt authentication failure
- **Source**: `chacha20poly1305` crate's `AeadInPlace::decrypt_in_place_detached` trait contract. The crate does not guarantee the buffer is zeroized on tag failure; the decrypt stream may have already XORed part of the keystream into the buffer before the tag comparison fails. The design-invariants Zero-Trace rule (#7) states plaintext must not leak.
- **Impact**: On auth failure, the caller would otherwise drop a `Vec<u8>` whose allocation contains partially-decrypted plaintext bytes. In Rust the drop frees memory without overwriting, so the allocator may reuse those bytes. The sub-phase is silent on this.
- **Classification**: Non-blocking (see Resolution).
- **Resolution**: In `decrypt_chunk`, before returning `Err(CryptoError::DecryptionFailed)`, call `zeroize::Zeroize::zeroize` on the in-flight plaintext buffer. Add `use zeroize::Zeroize;` inside `decrypt_chunk.rs`. This is a defensive hardening, not a behaviour change.
- **Documentation sync required on implementation**: None — hardening is consistent with design-invariant #7 and needs no additional doc entry.

### Concern #3 — Wire-format assembly for `encrypt_chunk` (owned `Vec<u8>` input)
- **Source**: Sub-phase 1.2 Implementation Notes line 58: "`encrypt_chunk` takes `plaintext: Vec<u8>` by value to allow in-place mutation without an extra allocation." But the final wire format is `nonce || ciphertext || tag` — prepending 24 nonce bytes to a `Vec<u8>` is not possible without either a new allocation or calling `splice`/`insert` (both of which copy).
- **Impact**: Without a documented assembly strategy, the implementer will guess. Two reasonable options: (a) allocate `let mut blob = Vec::with_capacity(24 + plaintext.len() + 16); blob.extend_from_slice(&nonce); blob.extend(plaintext); blob.extend_from_slice(tag.as_slice());` — one allocation, O(n) copy of the ciphertext; (b) encrypt the caller's `Vec` in place, then build a new `Vec` around it via the same extend pattern. Both are equivalent in allocations and copies; option (a) is clearer.
- **Classification**: Non-blocking.
- **Resolution**: Use option (a). Call `encrypt_in_place_detached` on a `&mut [u8]` view of the owned `plaintext` buffer (avoids zero-allocation in-place), then assemble the wire blob with exactly one fresh `Vec<u8>` allocation of capacity `24 + plaintext.len() + 16`. This is the pattern used throughout RustCrypto's AEAD examples.
- **Documentation sync required on implementation**: None.

### Concern #4 — `test_decrypt_truncated_blob_fails` boundary exactness
- **Source**: Sub-phase Deliverable #4: "return `CryptoError::InvalidBlobFormat` if `blob.len() < 40`." The acceptance criteria say "Blob shorter than 40 bytes → `CryptoError::InvalidBlobFormat`."
- **Impact**: Missing coverage of exact-40-byte edge case (empty-plaintext boundary) could let off-by-one slip into production.
- **Classification**: Non-blocking.
- **Resolution**: Add two boundary tests: `test_decrypt_blob_exactly_forty_bytes_with_empty_plaintext_decrypts` (the empty-plaintext round trip — blob is exactly 40 bytes and must succeed) and `test_decrypt_blob_thirty_nine_bytes_returns_invalid_blob_format` (one byte under the floor). These sit alongside the enumerated `test_decrypt_truncated_blob_fails` rather than replacing it.
- **Documentation sync required on implementation**: None.

## 4. Assumptions

The implementer should treat these as plan-level commitments. If any is wrong, pause and re-plan:

1. `chacha20poly1305 = "0.10"` exposes `XChaCha20Poly1305`, the `KeyInit::new(&Key)` constructor, and the `AeadInPlace::{encrypt_in_place_detached, decrypt_in_place_detached}` methods with the following signatures:
   ```rust
   fn encrypt_in_place_detached(
       &self,
       nonce: &XNonce,      // &GenericArray<u8, U24>
       associated_data: &[u8],
       buffer: &mut [u8],
   ) -> Result<Tag, chacha20poly1305::Error>;

   fn decrypt_in_place_detached(
       &self,
       nonce: &XNonce,
       associated_data: &[u8],
       buffer: &mut [u8],
       tag: &Tag,           // &GenericArray<u8, U16>
   ) -> Result<(), chacha20poly1305::Error>;
   ```
   where `Tag = GenericArray<u8, U16>` and `XNonce = GenericArray<u8, U24>`. If the crate version on disk differs, the implementer must adjust imports but keep the plan's wire-format and AAD contracts exactly.
2. `FileKey::expose(&self) -> &[u8; 32]` (defined `pub(crate)` in `src-tauri/src/crypto/types/mod.rs` line 20) is accessible from `src-tauri/src/crypto/encrypt_chunk.rs` and `src-tauri/src/crypto/decrypt_chunk.rs` because all three files live inside the `crypto` module.
3. `FileId::as_bytes(&self) -> &[u8; 16]` and `ChunkIndex::to_be_bytes(&self) -> [u8; 4]` are the canonical accessors for building AAD. They are already defined in Phase 1.1 and are `pub`.
4. `generate_nonce()` in `src-tauri/src/crypto/nonce.rs` is the sole nonce source; no local `rand::rng().random()` calls in `encrypt_chunk.rs` or `decrypt_chunk.rs`.
5. Returned plaintext `Vec<u8>` is not zeroized on the success path — the caller owns it and is responsible for eventual cleanup. Zeroization applies only on the decryption-failure path (see Concern #2).
6. Tests live inline as `#[cfg(test)] mod tests { ... }` in the same file as the function under test, matching the convention used in `nonce.rs`, `hkdf.rs`, `error.rs`, and `types/mod.rs`. No separate integration test files.
7. `proptest` dependency (already in `[dev-dependencies]`) does not need feature flags for this sub-phase. The default feature set is sufficient for `Vec<u8>` strategies up to a bounded size.
8. `proptest` strategies bound plaintext size at `0..=64 * 1024` bytes (64 KiB) to keep the test suite fast. This is well below the canonical 4 MiB chunk size but exercises empty, single-byte, boundary, and multi-block-aligned cases.
9. Doc comments are required on every `pub fn`, `pub struct`, `pub enum` per `.claude/rules/rust.md`. Internal helpers can use `//` block comments only if they add non-obvious WHY context; otherwise none.
10. Final plaintext is returned as a newly-allocated `Vec<u8>` by cloning the decrypted slice (not by handing back the internal buffer), to keep the success path's return type simple. The internal decrypt buffer is then dropped normally.

## 5. Approach

All file paths are absolute. The implementer should execute steps in order.

### Step 5.1 — Create `src-tauri/src/crypto/encrypt_chunk.rs`

Produce this exact public signature (doc comment verbatim; clippy requires it):

```rust
//! XChaCha20-Poly1305 chunk encryption with mandatory AAD binding.

use crate::crypto::nonce::generate_nonce;
use crate::crypto::types::{ChunkIndex, FileId, FileKey};
use chacha20poly1305::{
    AeadInPlace, KeyInit, XChaCha20Poly1305,
    aead::generic_array::GenericArray,
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
/// # Panics
/// Panics only if the underlying `encrypt_in_place_detached` call fails,
/// which for XChaCha20-Poly1305 with a valid key and nonce length is
/// unreachable — the RustCrypto contract only errors on plaintext length
/// overflow (≈ 2^38 bytes), far beyond any Arx Runa chunk size.
pub fn encrypt_chunk(
    mut plaintext: Vec<u8>,
    file_key: &FileKey,
    file_id: &FileId,
    chunk_index: ChunkIndex,
) -> Vec<u8> {
    let nonce_bytes = generate_nonce();
    let aad = build_aad(file_id, chunk_index);

    let cipher = XChaCha20Poly1305::new(GenericArray::from_slice(file_key.expose()));
    let nonce = GenericArray::from_slice(&nonce_bytes);

    let tag = cipher
        .encrypt_in_place_detached(nonce, &aad, plaintext.as_mut_slice())
        .expect("XChaCha20-Poly1305 encryption is infallible for Arx Runa chunk sizes");

    let mut blob = Vec::with_capacity(24 + plaintext.len() + 16);
    blob.extend_from_slice(&nonce_bytes);
    blob.extend_from_slice(&plaintext);
    blob.extend_from_slice(tag.as_slice());
    blob
}

fn build_aad(file_id: &FileId, chunk_index: ChunkIndex) -> [u8; 20] {
    let mut aad = [0u8; 20];
    aad[..16].copy_from_slice(file_id.as_bytes());
    aad[16..].copy_from_slice(&chunk_index.to_be_bytes());
    aad
}
```

Notes on `build_aad`:
- It is a private free function, not a method. It is called from both `encrypt_chunk.rs` and `decrypt_chunk.rs`. To avoid duplication, promote it to `pub(super) fn build_aad(...)` in a new `src-tauri/src/crypto/aad.rs` file *only if* the implementer finds both files importing it. Simpler alternative: keep one copy in each file (20 lines total, negligible duplication). **Chosen strategy**: duplicate `build_aad` in both files; if a later sub-phase introduces a third caller, extract it then.

### Step 5.2 — Create `src-tauri/src/crypto/decrypt_chunk.rs`

Produce this exact public signature:

```rust
//! XChaCha20-Poly1305 chunk decryption with mandatory AAD binding.
//!
//! The `blob: &[u8]` parameter is a Phase 1.2 placeholder. Phase 1.3 will
//! introduce `VerifiedBlob` in `crypto/checksum.rs` and change the first
//! parameter to `blob: VerifiedBlob`, enforcing check-before-decrypt at
//! compile time.

use crate::crypto::error::CryptoError;
use crate::crypto::types::{ChunkIndex, FileId, FileKey};
use chacha20poly1305::{
    AeadInPlace, KeyInit, XChaCha20Poly1305,
    aead::generic_array::GenericArray,
};
use zeroize::Zeroize;

const NONCE_LEN: usize = 24;
const TAG_LEN: usize = 16;
const MIN_BLOB_LEN: usize = NONCE_LEN + TAG_LEN;

/// Decrypts one chunk from its wire-format blob.
///
/// The blob layout is `[24-byte nonce | ciphertext | 16-byte Poly1305 tag]`.
/// AAD is reconstructed as `file_id || chunk_index` (big-endian `u32`) and
/// must match the value used at encrypt time.
///
/// # Errors
/// * `CryptoError::InvalidBlobFormat` if the blob is shorter than 40 bytes.
/// * `CryptoError::DecryptionFailed` if the Poly1305 tag verification fails,
///   including wrong key, wrong `file_id`, wrong `chunk_index`, or any
///   ciphertext/tag tampering.
pub fn decrypt_chunk(
    blob: &[u8],
    file_key: &FileKey,
    file_id: &FileId,
    chunk_index: ChunkIndex,
) -> Result<Vec<u8>, CryptoError> {
    if blob.len() < MIN_BLOB_LEN {
        return Err(CryptoError::InvalidBlobFormat {
            expected: MIN_BLOB_LEN,
            actual: blob.len(),
        });
    }

    let (nonce_slice, rest) = blob.split_at(NONCE_LEN);
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
```

### Step 5.3 — Register modules in `src-tauri/src/crypto/mod.rs`

Edit `src-tauri/src/crypto/mod.rs` (currently lines 1–17 after Phase 1.1) to add the two new modules and re-export the public functions:

```rust
//! Arx Runa crypto module.
//!
//! Cryptographic primitives: key derivation, chunk encryption, file key
//! management, BLAKE3 checksums.

pub mod decrypt_chunk;
pub mod encrypt_chunk;
pub mod error;
pub mod hkdf;
pub mod nonce;
pub mod types;

pub use decrypt_chunk::decrypt_chunk;
pub use encrypt_chunk::encrypt_chunk;
pub use error::CryptoError;
pub use hkdf::{VaultKeys, derive_vault_keys};
pub use nonce::generate_nonce;
pub use types::{
    Blake3Hash, ChunkIndex, FileId, FileKey, KeyEncryptionKey, ManifestKey, SqlcipherKey,
    WrappedFileKey,
};
```

Module declarations and re-exports are sorted alphabetically to match the Phase 1.1 style.

### Step 5.4 — Unit tests (inline `#[cfg(test)]` in each file)

Append this module to `src-tauri/src/crypto/encrypt_chunk.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::decrypt_chunk::decrypt_chunk;
    use crate::crypto::types::FileKey;

    fn make_file_key(byte: u8) -> FileKey {
        FileKey::from_bytes([byte; 32])
    }

    fn make_file_id(byte: u8) -> FileId {
        FileId::new([byte; 16])
    }

    #[test]
    fn test_encrypt_chunk_produces_wire_format_with_forty_byte_overhead() {
        let plaintext = vec![1u8, 2, 3, 4, 5];
        let blob = encrypt_chunk(
            plaintext.clone(),
            &make_file_key(0xAA),
            &make_file_id(0x01),
            ChunkIndex::new(0),
        );

        assert_eq!(blob.len(), plaintext.len() + 40);
    }

    #[test]
    fn test_encrypt_chunk_empty_plaintext_produces_forty_byte_blob() {
        let blob = encrypt_chunk(
            Vec::new(),
            &make_file_key(0x42),
            &make_file_id(0x01),
            ChunkIndex::new(0),
        );

        assert_eq!(blob.len(), 40);
    }

    #[test]
    fn test_encrypt_chunk_two_calls_same_plaintext_produce_different_blobs() {
        let plaintext = vec![0xCDu8; 128];
        let key = make_file_key(0x11);
        let file_id = make_file_id(0x22);

        let first = encrypt_chunk(plaintext.clone(), &key, &file_id, ChunkIndex::new(3));
        let second = encrypt_chunk(plaintext, &key, &file_id, ChunkIndex::new(3));

        assert_ne!(first, second, "different nonces must yield different blobs");
        assert_ne!(first[..24], second[..24], "nonce prefix must differ");
    }

    #[test]
    fn test_encrypt_decrypt_round_trip_returns_original_plaintext() {
        let plaintext = b"hello arx runa".to_vec();
        let key = make_file_key(0x33);
        let file_id = make_file_id(0x44);
        let chunk_index = ChunkIndex::new(7);

        let blob = encrypt_chunk(plaintext.clone(), &key, &file_id, chunk_index);
        let recovered = decrypt_chunk(&blob, &key, &file_id, chunk_index)
            .expect("round trip must succeed");

        assert_eq!(recovered, plaintext);
    }
}
```

Append this module to `src-tauri/src/crypto/decrypt_chunk.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
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
        encrypt_chunk(plaintext.to_vec(), key, file_id, chunk_index)
    }

    #[test]
    fn test_decrypt_chunk_wrong_file_id_fails_with_decryption_failed() {
        let key = make_file_key(0xAA);
        let blob = encrypt(b"payload", &key, &make_file_id(0x11), ChunkIndex::new(0));

        let result = decrypt_chunk(&blob, &key, &make_file_id(0x22), ChunkIndex::new(0));

        assert_matches!(result, Err(CryptoError::DecryptionFailed));
    }

    #[test]
    fn test_decrypt_chunk_wrong_chunk_index_fails_with_decryption_failed() {
        let key = make_file_key(0xAA);
        let file_id = make_file_id(0x11);
        let blob = encrypt(b"payload", &key, &file_id, ChunkIndex::new(0));

        let result = decrypt_chunk(&blob, &key, &file_id, ChunkIndex::new(1));

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
            &blob,
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

        let result = decrypt_chunk(&blob, &key, &file_id, ChunkIndex::new(0));

        assert_matches!(result, Err(CryptoError::DecryptionFailed));
    }

    #[test]
    fn test_decrypt_chunk_corrupted_tag_fails_with_decryption_failed() {
        let key = make_file_key(0xAA);
        let file_id = make_file_id(0x11);
        let mut blob = encrypt(b"payload", &key, &file_id, ChunkIndex::new(0));

        let tag_index = blob.len() - 1;
        blob[tag_index] ^= 0x01;

        let result = decrypt_chunk(&blob, &key, &file_id, ChunkIndex::new(0));

        assert_matches!(result, Err(CryptoError::DecryptionFailed));
    }

    #[test]
    fn test_decrypt_chunk_truncated_blob_returns_invalid_blob_format() {
        let blob = vec![0u8; 20];

        let result = decrypt_chunk(
            &blob,
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
        let blob = vec![0u8; 39];

        let result = decrypt_chunk(
            &blob,
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

        let recovered = decrypt_chunk(&blob, &key, &file_id, ChunkIndex::new(0))
            .expect("empty plaintext round trip must succeed");
        assert!(recovered.is_empty());
    }

    #[test]
    fn test_decrypt_chunk_empty_blob_returns_invalid_blob_format() {
        let result = decrypt_chunk(
            &[],
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
}
```

### Step 5.5 — Property-based tests

Append this module to the tests block of `src-tauri/src/crypto/encrypt_chunk.rs` (inside the same `#[cfg(test)] mod tests` or as a sibling `#[cfg(test)] mod proptests`):

```rust
#[cfg(test)]
mod proptests {
    use super::*;
    use crate::crypto::decrypt_chunk::decrypt_chunk;
    use crate::crypto::types::FileKey;
    use proptest::prelude::*;

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

            let blob = encrypt_chunk(plaintext.clone(), &key, &file_id, idx);
            let recovered = decrypt_chunk(&blob, &key, &file_id, idx)
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

            let first = encrypt_chunk(plaintext.clone(), &key, &file_id, idx);
            let second = encrypt_chunk(plaintext, &key, &file_id, idx);

            prop_assert_ne!(&first[..24], &second[..24]);
            prop_assert_ne!(first, second);
        }
    }
}
```

`assert_matches = "1"` is already in `[dev-dependencies]`; if any of the unit-test `assert_matches!` macros break due to feature-flag differences, substitute with explicit `match` arms — semantics must remain identical.

### Step 5.6 — Verify compilation, tests, clippy

From `src-tauri/`:

```bash
cargo build
cargo test crypto::encrypt_chunk
cargo test crypto::decrypt_chunk
cargo test crypto
cargo clippy -- -D warnings
cargo fmt --check
```

All commands must succeed. The `cargo test crypto` run also re-executes Phase 1.1 tests as a regression gate.

## 6. Security Implications

### 6a. Expected sensitive path set
- `src-tauri/src/crypto/encrypt_chunk.rs` (new)
- `src-tauri/src/crypto/decrypt_chunk.rs` (new)
- `src-tauri/src/crypto/mod.rs` (module registration and re-export only)

No files under `src-tauri/src/auth/` or `src-tauri/src/storage/` are touched. Any deviation from this list during implementation is a Plan Deviation.

### 6b. Invoke security-reviewer agent? NO

**Rationale for NO**: The sub-phase's own "Security Review: Required" checklist (items: AAD binding, nonce source, wire-format tag verification) is structurally enforced by the plan itself:

1. **AAD binding** — a single private `build_aad` helper in each file constructs `file_id || chunk_index` verbatim and is the only source of AAD bytes. The public signatures take `&FileId` and `ChunkIndex` by value, making an AAD-less call impossible. Adversarial tests `test_decrypt_chunk_wrong_file_id_fails_with_decryption_failed` and `test_decrypt_chunk_wrong_chunk_index_fails_with_decryption_failed` fail the build on any bypass.
2. **Nonce source** — `encrypt_chunk` calls `crate::crypto::nonce::generate_nonce()` exclusively. No local RNG calls. `decrypt_chunk` never generates a nonce. Grep anchor for review: the string `rand::` must not appear in `encrypt_chunk.rs` or `decrypt_chunk.rs`.
3. **Wire-format tag verification** — `decrypt_in_place_detached` is the sole decrypt path; it returns the plaintext only on `Ok(())`. The error arm zeroizes the buffer and returns `Err`, so partial plaintext cannot leak. No manual tag comparison is written.

These are mechanical checks that Opus verifies in the implementation review step directly — no second agent pass adds value. The plan-level drift check still fires if any file under `src-tauri/src/crypto/` outside the anticipated set gets touched.

If any of the following changes during implementation, upgrade to YES and run `security-reviewer` on the delta:
- A third AAD call site or a helper factored into a new file.
- Any direct `rand` crate usage in `encrypt_chunk.rs` / `decrypt_chunk.rs`.
- Any `unsafe` block introduced (there should be none).

### 6c. What the reviewer would check (if invoked)
(Recorded for audit transparency — not executed for this plan.)
- AAD construction: confirm `file_id (16) || chunk_index_be (4)` layout, no byte-order drift.
- Nonce uniqueness: confirm `generate_nonce()` is called exactly once per `encrypt_chunk` invocation and never reused.
- Tag verification ordering: confirm plaintext is only yielded after `decrypt_in_place_detached` returns `Ok(())`.
- Plaintext leak on error: confirm the decrypt failure path zeroizes the in-flight buffer.
- Key exposure lifetime: confirm `file_key.expose()` return is not retained beyond the `XChaCha20Poly1305::new` call.

## 7. Execution and Testing Strategy

**Implementation execution**: run the Approach steps directly in order. Rationale: Phase 1.2 is a pure Rust crypto module change — no frontend, no schema, no IPC — and must satisfy Arx Runa Rust standards (`thiserror` error handling, tests alongside new code, `cargo clippy -- -D warnings` hygiene, doc comments, and one-concern-per-file layout).

**Fallback**: If implementation cannot proceed as written, mark the plan blocked via the plan-deviation protocol. No speculative contract reshaping is permitted in this security-critical module.

**Test coverage checklist**:
- [x] Unit tests — `test_encrypt_chunk_produces_wire_format_with_forty_byte_overhead`, `test_encrypt_chunk_empty_plaintext_produces_forty_byte_blob`, `test_encrypt_chunk_two_calls_same_plaintext_produce_different_blobs`, `test_encrypt_decrypt_round_trip_returns_original_plaintext` (encrypt side); `test_decrypt_chunk_wrong_file_id_fails_with_decryption_failed`, `test_decrypt_chunk_wrong_chunk_index_fails_with_decryption_failed`, `test_decrypt_chunk_wrong_key_fails_with_decryption_failed`, `test_decrypt_chunk_corrupted_ciphertext_fails_with_decryption_failed`, `test_decrypt_chunk_corrupted_tag_fails_with_decryption_failed`, `test_decrypt_chunk_truncated_blob_returns_invalid_blob_format`, `test_decrypt_chunk_blob_thirty_nine_bytes_returns_invalid_blob_format`, `test_decrypt_chunk_exactly_forty_bytes_empty_plaintext_round_trip_succeeds`, `test_decrypt_chunk_empty_blob_returns_invalid_blob_format` (decrypt side).
- [x] Property-based tests — `prop_encrypt_decrypt_identity`, `prop_different_nonces_produce_different_blobs`.
- [x] Adversarial tests — covered by wrong-key, wrong-file-id, wrong-chunk-index, corrupted-ciphertext, corrupted-tag, truncated-blob cases above.
- [ ] Integration tests — N/A for this sub-phase.
- [ ] Zeroize tests — N/A for this sub-phase; `FileKey` zeroize is covered in Phase 1.1, and decrypt-path zeroization is exercised indirectly by the corrupted-ciphertext and corrupted-tag tests returning `Err` rather than leaking plaintext.

**Invoke test-writer agent? NO**. Rationale: the sub-phase enumerates the exact test list (Deliverables #6 and #7); Step 4 above codifies every named test plus two sub-phase-derived boundary tests and an additional `prop_different_nonces_produce_different_blobs`. The plan gives every test name and body shape verbatim, so adding `test-writer` here would duplicate work. If adversarial gaps are identified during implementation review, `test-writer` can be invoked retroactively as a separate pass.

**Validation checkpoint (from sub-phase 1.2 lines 22–41)**:
- `cargo test crypto::encrypt_chunk` — all tests pass.
- `cargo test crypto::decrypt_chunk` — all tests pass.
- Round-trip identity holds for all property-based inputs.
- Wrong `file_id` / wrong `chunk_index` / wrong key / corrupted ciphertext / corrupted tag → `CryptoError::DecryptionFailed`.
- Blob shorter than 40 bytes → `CryptoError::InvalidBlobFormat`.
- Two encryptions of identical plaintext produce different nonces.

**Additional boundary coverage (Step 1.75 Concern #4)**: the exactly-40-byte and 39-byte edge cases are covered by dedicated tests.

## 8. Documentation Impact

No documentation updates required on implementation.

**Why**: The plan deviates from canonical docs in exactly one place — `decrypt_chunk` takes `blob: &[u8]` instead of `blob: VerifiedBlob` — and that deviation is (a) explicitly foreseen by sub-phase 1.3's implementation notes (line 59), which plan for Phase 1.2 to stub the signature and Phase 1.3 to finalize it, and (b) temporary: the canonical final signature lands in Phase 1.3. No new contract surface is introduced; the wire format, AAD construction, and error variants are all already documented in the parent design and sub-phase doc.

The `## Documentation Impact` roadmap text for this sub-phase (if any) is advisory only; this plan's decision is "no sync required" because the deviation is purely a temporary ordering artefact already covered by the downstream sub-phase doc.

## 9. Handoff Notes for Implementer

Work from `C:\Users\chris\source\repos\arx-runa\src-tauri\`. Execute steps 5.1 → 5.6 in order. This plan is self-contained: all trait signatures, function bodies, test names, test bodies, AAD layout, and error variants are inlined above — the implementer does not need to re-read the sub-phase doc. The only external references that remain load-bearing are (a) `src-tauri/src/crypto/types/mod.rs` for `FileKey::expose`, `FileKey::from_bytes`, `FileId`, `ChunkIndex` accessors, (b) `src-tauri/src/crypto/nonce.rs` for `generate_nonce`, and (c) `src-tauri/src/crypto/error.rs` for `CryptoError` variants — all already implemented in Phase 1.1. Traps to watch for: (1) `chacha20poly1305 = "0.10"` uses `GenericArray` from its `aead::generic_array` re-export, not from a direct `generic_array` crate dependency; (2) the `expect("… infallible …")` on the encrypt path is deliberate — do not replace it with `?` since `encrypt_chunk` returns `Vec<u8>`, not `Result`; (3) `FileKey::expose` is `pub(crate)` and only callable from inside the crypto module — placing `encrypt_chunk.rs` anywhere else would break compilation; (4) the decrypt error path must call `buffer.zeroize()` before returning `Err` — do not skip this. Final verification is `cargo test crypto && cargo clippy -- -D warnings && cargo fmt --check` from `src-tauri/`.
