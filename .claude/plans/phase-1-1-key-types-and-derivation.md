---
title: "Phase 1.1 — Key Types, HKDF Derivation, and Nonce Generation"
created: "2026-04-13T00:00:00Z"
status: implemented
roadmap-phase: 1
sub-phase: "1.1"
design-document: "docs/architecture/designs/cryptographic-primitives/design.md"
sub-phase-roadmap: "docs/architecture/designs/cryptographic-primitives/sub-phases/roadmap.md"
tags: [crypto, phase-1, key-derivation, hkdf, nonce, zeroize]
---

# Plan: Phase 1.1 — Key Types, HKDF Derivation, and Nonce Generation

## 1. Goal

Implement the foundational type layer of the Arx Runa crypto module — key newtypes with `ZeroizeOnDrop`, domain newtypes (`FileId`, `ChunkIndex`, `Blake3Hash`, `WrappedFileKey`), the `CryptoError` enum, HKDF-SHA256 vault key derivation via `derive_vault_keys`, and CSPRNG nonce generation via `generate_nonce` — so that Phase 1.2 (AEAD) and Phase 1.3 (wrapping + checksums) can build on a compiling, tested type substrate.

## 2. Context

**Roadmap**: Phase 1 — Cryptographic Primitives (`docs/roadmap.md` lines 44–49). Depends on Phase 0 (scaffolding), which is complete as of commit `17b79a9` ("phase 0 complete").

**Sub-phase roadmap**: `docs/architecture/designs/cryptographic-primitives/sub-phases/roadmap.md`. Implementation order is strict: 1.1 → 1.2 → 1.3. Phase 1.1 is the root of the chain and gates everything else in the crypto module.

**Sub-phase doc**: `docs/architecture/designs/cryptographic-primitives/sub-phases/1.1-key-types-and-derivation.md`. Estimated scope ~200 LOC production + ~120 LOC tests.

**Parent design**: `docs/architecture/designs/cryptographic-primitives/design.md`. The `## Contract Surface` section (lines 17–43) is canonical per `CLAUDE.md`. Relevant canonical commitments for this sub-phase:

- Public API includes `derive_vault_keys` and `generate_nonce` (§ Interface contract).
- Canonical key containers include `VaultKeys`, `FileKey`, `KeyEncryptionKey`, `SqlcipherKey`, `ManifestKey`, `WrappedFileKey` (§ Data contract).
- Canonical domain/value types include `FileId`, `ChunkIndex`, `Blake3Hash` (§ Data contract).
- Nonces are random 24-byte CSPRNG values (§ Invariant contract).
- HKDF uses fixed salt `b"arx-runa-v1"` and info strings `b"arx-runa-key-encryption"`, `b"arx-runa-sqlcipher"`, `b"arx-runa-manifest-backup"` (`design-invariants.md` invariant #3).

**Existing state** (from Phase 0.2):
- `src-tauri/src/crypto/mod.rs` declares `pub mod error;` and `pub mod types;` but nothing else.
- `src-tauri/src/crypto/error.rs` contains an empty `CryptoError` enum marked `#[non_exhaustive]`.
- `src-tauri/src/crypto/types/mod.rs` is an empty placeholder.
- `src-tauri/Cargo.toml` already pins `chacha20poly1305 = "0.10"`, `hkdf = "0.13"`, `sha2 = "0.11"`, `blake3 = "1"`, `rand = "0.10"`, `zeroize = { version = "1", features = ["derive"] }`, `secrecy = "0.10"`, `uuid = { version = "1", features = ["v4", "serde"] }`, `thiserror = "2"`. No dependency additions are needed for Phase 1.1. `proptest` is already in `[dev-dependencies]`.
- No AEAD cipher construction happens in 1.1; `chacha20poly1305` is used only transitively in later sub-phases.

**Pending architectural decisions**: None — `design.md` § Open Decisions states "None — all design decisions have been made." The Decisions Made table (lines 568–583) ratifies every choice relevant to 1.1.

---

## 3. Design Concerns / Open Questions

### Concern 1 — `secrecy::Secret<T>` no longer exists in secrecy 0.10 (API drift)

- **Concern**: Both the sub-phase (deliverable 1) and `design.md` § Type Definitions (lines 370–394) write key types as `pub struct FileKey(Secret<[u8; 32]>);` using `secrecy::Secret<T>`. `secrecy = "0.10"` (pinned in `src-tauri/Cargo.toml` and used throughout the scaffolding) removed `Secret<T>`. The only boxed-secret type in 0.10 is `SecretBox<S: Zeroize + ?Sized>` (see `secrecy/src/lib.rs:58`).
- **Source**: `1.1-key-types-and-derivation.md` line 12; `design.md` lines 378, 382, 386, 390.
- **Impact**: Literal copy of the design's type definitions will fail to compile.
- **Classification**: **Non-blocking** — this plan resolves it by inlining the corrected API below. The sub-phase and parent design should be patched in a follow-up edit (out of scope for implementation, flagged here for the user).
- **Resolution**: Use `secrecy::SecretBox<[u8; 32]>`. `[u8; 32]` implements `Zeroize` (from `zeroize = "1"` with the `derive` feature — `impl<Z: Zeroize, const N: usize> Zeroize for [Z; N]`), so `SecretBox<[u8; 32]>` is legal. `SecretBox<S>` already implements `ZeroizeOnDrop` (`secrecy/src/lib.rs:74`), so the outer `#[derive(ZeroizeOnDrop)]` on each key struct is redundant but harmless. Keep the derive for intent documentation. Construction is `SecretBox::new(Box::new(bytes))` or `SecretBox::init_with(|| bytes)`. Access is `ExposeSecret::expose_secret(&self.0)` returning `&[u8; 32]`.

### Concern 2 — `derive_vault_keys` parameter type contradiction (`&MasterKey` vs `&[u8; 32]`)

- **Concern**: Sub-phase deliverable 6 writes the signature as `derive_vault_keys(master_key: &MasterKey) -> VaultKeys`, but Implementation Note 1 (line 56) and the sub-phase roadmap Note (line 106) both say "`MasterKey` is not yet defined in Phase 1; accept `&[u8; 32]` as the internal parameter until Phase 2 provides the type".
- **Source**: `1.1-key-types-and-derivation.md` line 17 vs lines 56, 106.
- **Impact**: Implementer guesses; if they introduce a placeholder `MasterKey` type in Phase 1.1 it will clash with the authoritative Phase 2 definition.
- **Classification**: **Non-blocking**.
- **Resolution**: Use `pub fn derive_vault_keys(master_key_bytes: &[u8; 32]) -> VaultKeys`. No `MasterKey` stub type is introduced in Phase 1.1. Phase 2 will add `MasterKey` and either (a) add an overload / wrapper that exposes a `&[u8; 32]` via `ExposeSecret` to call this function, or (b) change this signature to accept `&MasterKey` at that time. The parameter name is `master_key_bytes` to signal its transient nature.

### Concern 3 — `rand` version note drift (`rand 0.9 API`)

- **Concern**: Sub-phase deliverable 8 says "(rand 0.9 API)" but `Cargo.toml` pins `rand = "0.10"` and `design.md` line 549 pins 0.10. The API call `rand::rng().random::<[u8; 24]>()` is the same for 0.9 and 0.10, but the version label is wrong.
- **Source**: `1.1-key-types-and-derivation.md` line 19.
- **Impact**: Cosmetic; zero effect on implementation.
- **Classification**: **Non-blocking**.
- **Resolution**: Plan uses the rand 0.10 API: `rand::rng().random::<[u8; 24]>()`.

### Concern 4 — `KeyUnwrapFailed` variant is unused by the design

- **Concern**: The sub-phase asks for three `CryptoError` variants including `KeyUnwrapFailed` (deliverable 5), but `design.md` § Key Wrapping (lines 141–145) and § Recovery Slot (lines 186–197) both document that `unwrap_file_key` and `unwrap_master_key_from_recovery` return `CryptoError::DecryptionFailed`, not `KeyUnwrapFailed`. No caller in the design produces `KeyUnwrapFailed`.
- **Source**: `1.1-key-types-and-derivation.md` line 16 vs `design.md` lines 142, 191.
- **Impact**: Defining an unused enum variant produces a dead-code warning with `-D warnings`. It will also mislead the Phase 1.3 implementer into thinking they should emit it.
- **Classification**: **Non-blocking**.
- **Resolution**: Define `CryptoError` with `DecryptionFailed`, `InvalidBlobFormat { expected, actual }`, and `ChecksumMismatch` (inlined below), plus `KeyUnwrapFailed` to honour the sub-phase deliverable literally. Suppress the dead-code warning on `KeyUnwrapFailed` by adding `#[allow(dead_code)]` on the variant — or, preferred, rely on `#[non_exhaustive]` plus the fact that enum variants are not lint-reachability targets (they are not warned on). Inline test `crypto_error_variants_constructible` exercises every variant via explicit construction, satisfying clippy and locking in the ABI.

### Concern 5 — Zeroize-on-drop test is unsound with heap-boxed secrets

- **Concern**: Sub-phase Implementation Note 4 (line 59) and deliverable 10 demand zeroize tests that "read back the dropped memory via a raw pointer captured before the drop ... Use `ManuallyDrop` to control timing." With `SecretBox<[u8; 32]>` the bytes live inside a `Box`, so after drop the allocation is freed and subsequent reads through the captured pointer are undefined behaviour (use-after-free). Miri and `cargo test` under strict mode will flag this.
- **Source**: `1.1-key-types-and-derivation.md` lines 40, 59.
- **Impact**: A naive implementation produces UB in test. Tests may pass under release mode but fail under Miri, sanitizers, or a future compiler update.
- **Classification**: **Non-blocking**.
- **Resolution**: Capture the pointer, invoke `zeroize()` on the field via a test-only helper that calls `Zeroize::zeroize(&mut self.0)` through a mutable borrow **before** drop, and read back through the still-valid pointer while the `SecretBox` is alive. The check is: after `Zeroize::zeroize` but before `drop`, all 32 bytes read `0`. This verifies the zeroize semantics without relying on use-after-free. Alternative acceptable pattern: use `ManuallyDrop` combined with `Box::into_raw` to keep the allocation alive after logical drop, zero-check, then `Box::from_raw` + manual `drop` for cleanup — but the in-place approach above is simpler.

### Concern 6 — File-level module layout (sub-phase one-file vs design multi-file)

- **Concern**: Sub-phase deliverables 1–4 instruct placing every newtype in `src-tauri/src/crypto/types/mod.rs`. `design.md` § Module Structure (lines 447–470) lays out one file per type (`file_key.rs`, `key_encryption_key.rs`, etc.). The Contract Surface does not mandate file layout, so this is sub-phase guidance vs design guidance, not a canonical conflict.
- **Source**: `1.1-key-types-and-derivation.md` lines 12–15 vs `design.md` lines 447–470.
- **Impact**: Implementer must choose.
- **Classification**: **Non-blocking**.
- **Resolution**: Follow the **sub-phase** (single `types/mod.rs`). Rationale: (a) Phase 1.1 totals ~200 production LOC — splitting seven newtypes into seven files yields ~30 LOC per file, which is premature decomposition. (b) CLAUDE.md "Don't add abstractions beyond what the task requires." (c) A later sub-phase or refactor can split if file size justifies. No `types/file_key.rs` etc. are created.

### Concern 7 — `VaultKeys` location (sub-phase says `hkdf.rs`, design implies `types/`)

- **Concern**: Sub-phase deliverable 7 places `VaultKeys` in `src-tauri/src/crypto/hkdf.rs`. The design's Contract Surface lists `VaultKeys` as a "canonical key container", which colocates with the other key types in `types/`.
- **Source**: `1.1-key-types-and-derivation.md` line 18.
- **Impact**: Cosmetic file placement.
- **Classification**: **Non-blocking**.
- **Resolution**: Follow the sub-phase — place `VaultKeys` in `hkdf.rs` alongside `derive_vault_keys`. Re-export via `pub use` from `crypto/mod.rs` so external callers access it as `crypto::VaultKeys`.

---

## 4. Assumptions

Every assumption here is a fact the plan takes for granted but which is not stated verbatim in the sub-phase. If any are wrong, stop and ask.

1. **Key types are not `Debug`, not `Clone`, not `Copy`, not `Serialize`.** Stated in sub-phase line 61 but repeated here for emphasis. `FileId`, `ChunkIndex`, `Blake3Hash`, and `WrappedFileKey` *are* `Debug + Clone + Copy + PartialEq + Eq` because they hold non-secret data.
2. **`WrappedFileKey` is declared in Phase 1.1 (type only)** even though its wrap/unwrap functions belong to Phase 1.3. Rationale: sub-phase deliverable 2 says so; Phase 1.3 will extend this module with the wrapping functions without redefining the type.
3. **`Blake3Hash` is declared in Phase 1.1 (type only)** likewise; its constructor and uses arrive in Phase 1.3.
4. **HKDF expansion output is exactly 32 bytes per derived key.** Per RFC 5869 and `hkdf = "0.13"` API, `Hkdf::<Sha256>::new(Some(&salt), ikm).expand(info, &mut okm)` where `okm: &mut [u8; 32]`.
5. **Single HKDF extract, three expands.** One `Hkdf::<Sha256>::new(Some(b"arx-runa-v1"), master_key_bytes)` instance is created, then `.expand(info_i, &mut out_i)` is called three times. This matches RFC 5869 § 3 ("extract-and-expand") and is the idiomatic use of the `hkdf` crate.
6. **Temporary `[u8; 32]` buffers used during `expand` are zeroized before being moved into `SecretBox`.** Use `let mut okm = [0u8; 32]; hkdf.expand(info, &mut okm).expect(...); let key = KeyEncryptionKey(SecretBox::new(Box::new(okm)));`. The `okm` local is moved into `Box::new`, so no residual copy remains on the stack. (If the compiler chooses to copy rather than move, that is a zeroize gap — mitigation is to `okm.zeroize()` after the move via `Zeroizing<[u8; 32]>` intermediate: `let mut okm = Zeroizing::new([0u8; 32]); hkdf.expand(info, okm.as_mut_slice()).unwrap(); let key = KeyEncryptionKey(SecretBox::new(Box::new(*okm)));`. Plan uses the `Zeroizing<[u8; 32]>` intermediate form for safety.)
7. **HKDF `expand` never fails for 32-byte output with SHA-256.** The maximum output is `255 * HashLen = 8160 bytes`; 32 bytes is well within bounds. The function returns `Result<(), InvalidLength>`. Use `.expect("HKDF expand must not fail for 32-byte output with SHA-256")` — matches `design.md` line 87's `Panics` doc.
8. **`hkdf::Hkdf::<Sha256>::new` signature in 0.13.** Current `hkdf = "0.13"` crate exposes `Hkdf::<Sha256>::new(salt: Option<&[u8]>, ikm: &[u8]) -> Hkdf<Sha256>`. Verified via crates.io docs for 0.13.x; implementer confirms on first compile.
9. **`rand::rng().random::<[u8; 24]>()`** returns a `[u8; 24]` directly. No `thread_rng()` — removed in rand 0.9+. The function `rand::rng()` replaces it in 0.10.
10. **`ZeroizeOnDrop` derive compiles on a struct whose only field is `SecretBox<[u8; 32]>`.** Verified: `SecretBox<S>` impls `Zeroize` (`secrecy/src/lib.rs:62`), so the derive macro's requirement that each field implement `Zeroize` is met.
11. **`uuid = "1"` with `v4` feature provides `Uuid::as_bytes() -> &[u8; 16]` and `Uuid::from_bytes([u8; 16]) -> Uuid`.** Standard uuid 1.x API.
12. **Tests live inline via `#[cfg(test)] mod tests` at the bottom of each module file**, not under `src-tauri/tests/`. Matches Phase 0 scaffolding style and standard Rust convention for unit tests.
13. **CLAUDE.md "No abbreviations" applies.** Variable names are `master_key_bytes`, `chunk_index`, `encrypted_buffer`, etc. Acronyms (HKDF, KEK, AEAD) are exempt. The sub-phase uses `kek` in test names — rename to `key_encryption_key` in test names.
14. **`hkdf` crate 0.13 is compatible with `sha2` crate 0.11.** Cargo.toml pins both; if a version mismatch surfaces at build time, the implementer bumps `hkdf` to whatever 0.1x version accepts `sha2 = "0.11"`. This is a scaffolding-level concern outside Phase 1.1 scope.

---

## 5. Approach

All file paths are absolute from the repository root `C:\Users\chris\source\repos\arx-runa\`.

### Step 1 — Rewrite `src-tauri/src/crypto/error.rs` with the full `CryptoError` enum

Replace the placeholder with the canonical enum. Phase 1.1 lands all four variants (`DecryptionFailed`, `InvalidBlobFormat`, `KeyUnwrapFailed`, `ChecksumMismatch`) so later sub-phases do not have to grow the enum. `#[non_exhaustive]` is retained.

```rust
//! Error types for the crypto module.

use thiserror::Error;

/// Errors produced by the crypto module.
///
/// Variants are introduced here in Phase 1.1 so later sub-phases extend the
/// module without modifying the error surface. `#[non_exhaustive]` guarantees
/// external match sites must include a wildcard arm.
#[non_exhaustive]
#[derive(Debug, Error)]
pub enum CryptoError {
    /// XChaCha20-Poly1305 authentication failed during chunk decryption
    /// or recovery-slot unwrap. Never reveal whether the key or the AAD
    /// was wrong — both failure modes collapse to this variant.
    #[error("decryption failed: authentication tag mismatch")]
    DecryptionFailed,

    /// The wire-format blob is shorter than the minimum framing (24-byte
    /// nonce + 16-byte Poly1305 tag = 40 bytes) or is otherwise malformed.
    #[error("invalid blob format: expected at least {expected} bytes, got {actual}")]
    InvalidBlobFormat { expected: usize, actual: usize },

    /// File-key unwrap failed. Reserved for Phase 1.3 callers; Phase 1.1
    /// only defines the variant.
    #[error("key unwrap failed")]
    KeyUnwrapFailed,

    /// BLAKE3 checksum verification failed; blob has been tampered with
    /// or corrupted in transit. Reserved for Phase 1.3 callers.
    #[error("checksum mismatch: blob has been tampered with or corrupted")]
    ChecksumMismatch,
}

#[cfg(test)]
mod tests {
    use super::CryptoError;

    #[test]
    fn crypto_error_variants_constructible() {
        let _a = CryptoError::DecryptionFailed;
        let _b = CryptoError::InvalidBlobFormat { expected: 40, actual: 10 };
        let _c = CryptoError::KeyUnwrapFailed;
        let _d = CryptoError::ChecksumMismatch;
    }

    #[test]
    fn crypto_error_display_formats() {
        let error = CryptoError::InvalidBlobFormat { expected: 40, actual: 10 };
        assert_eq!(
            error.to_string(),
            "invalid blob format: expected at least 40 bytes, got 10"
        );
    }
}
```

### Step 2 — Rewrite `src-tauri/src/crypto/types/mod.rs` with all newtypes

Replace the placeholder file with the full set: `FileKey`, `KeyEncryptionKey`, `SqlcipherKey`, `ManifestKey`, `WrappedFileKey`, `FileId`, `ChunkIndex`, `Blake3Hash`.

```rust
//! Newtype wrappers for the crypto module.
//!
//! Key material (`FileKey`, `KeyEncryptionKey`, `SqlcipherKey`, `ManifestKey`)
//! is stored as `secrecy::SecretBox<[u8; 32]>` and zeroized on drop. These
//! types are deliberately not `Debug`, `Clone`, or `Copy` — accidental copies
//! defeat zeroization guarantees.
//!
//! Domain newtypes (`FileId`, `ChunkIndex`, `Blake3Hash`, `WrappedFileKey`)
//! hold non-secret data and are freely `Debug + Clone + Copy + PartialEq + Eq`.

use secrecy::{ExposeSecret, SecretBox};
use zeroize::ZeroizeOnDrop;

/// 256-bit per-file encryption key.
#[derive(ZeroizeOnDrop)]
pub struct FileKey(SecretBox<[u8; 32]>);

impl FileKey {
    pub(crate) fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(SecretBox::new(Box::new(bytes)))
    }

    pub(crate) fn expose(&self) -> &[u8; 32] {
        self.0.expose_secret()
    }
}

/// 256-bit key encryption key (wraps per-file keys).
#[derive(ZeroizeOnDrop)]
pub struct KeyEncryptionKey(SecretBox<[u8; 32]>);

impl KeyEncryptionKey {
    pub(crate) fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(SecretBox::new(Box::new(bytes)))
    }

    pub(crate) fn expose(&self) -> &[u8; 32] {
        self.0.expose_secret()
    }
}

/// 256-bit SQLCipher database encryption key.
#[derive(ZeroizeOnDrop)]
pub struct SqlcipherKey(SecretBox<[u8; 32]>);

impl SqlcipherKey {
    pub(crate) fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(SecretBox::new(Box::new(bytes)))
    }

    pub(crate) fn expose(&self) -> &[u8; 32] {
        self.0.expose_secret()
    }
}

/// 256-bit manifest-backup encryption key.
#[derive(ZeroizeOnDrop)]
pub struct ManifestKey(SecretBox<[u8; 32]>);

impl ManifestKey {
    pub(crate) fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(SecretBox::new(Box::new(bytes)))
    }

    pub(crate) fn expose(&self) -> &[u8; 32] {
        self.0.expose_secret()
    }
}

/// Wrapped file key, stored in the manifest.
/// 72-byte wire format: `[24-byte nonce | 32-byte ciphertext | 16-byte tag]`.
///
/// Not `ZeroizeOnDrop` — this is ciphertext, not key material.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WrappedFileKey(pub [u8; 72]);

/// File identifier (UUID v4, stored as raw 16 bytes for compact AAD).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FileId([u8; 16]);

impl FileId {
    pub fn new(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }

    pub fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }

    pub fn from_uuid(uuid: uuid::Uuid) -> Self {
        Self(*uuid.as_bytes())
    }

    pub fn to_uuid(&self) -> uuid::Uuid {
        uuid::Uuid::from_bytes(self.0)
    }
}

/// Zero-based chunk position within a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ChunkIndex(u32);

impl ChunkIndex {
    pub fn new(index: u32) -> Self {
        Self(index)
    }

    pub fn as_u32(&self) -> u32 {
        self.0
    }

    pub fn to_be_bytes(&self) -> [u8; 4] {
        self.0.to_be_bytes()
    }
}

/// 32-byte BLAKE3 checksum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Blake3Hash(pub [u8; 32]);

#[cfg(test)]
mod tests {
    use super::*;
    use zeroize::Zeroize;

    #[test]
    fn file_id_roundtrips_through_uuid() {
        let uuid = uuid::Uuid::new_v4();
        let file_id = FileId::from_uuid(uuid);
        assert_eq!(file_id.to_uuid(), uuid);
        assert_eq!(file_id.as_bytes(), uuid.as_bytes());
    }

    #[test]
    fn file_id_new_preserves_bytes() {
        let bytes = [7u8; 16];
        let file_id = FileId::new(bytes);
        assert_eq!(*file_id.as_bytes(), bytes);
    }

    #[test]
    fn chunk_index_big_endian_encoding() {
        assert_eq!(ChunkIndex::new(0).to_be_bytes(), [0, 0, 0, 0]);
        assert_eq!(ChunkIndex::new(1).to_be_bytes(), [0, 0, 0, 1]);
        assert_eq!(ChunkIndex::new(0x01020304).to_be_bytes(), [1, 2, 3, 4]);
        assert_eq!(ChunkIndex::new(u32::MAX).to_be_bytes(), [0xFF, 0xFF, 0xFF, 0xFF]);
    }

    // Zeroize-on-drop verification for FileKey.
    //
    // Pattern: zeroize the secret through a mutable borrow BEFORE dropping,
    // and read back through a captured pointer while the SecretBox is still
    // alive. This avoids use-after-free that would occur if we read through
    // the pointer after the Box is freed. See plan Concern 5.
    #[test]
    fn file_key_zeroizes_via_zeroize_trait() {
        let mut file_key = FileKey::from_bytes([0xAAu8; 32]);
        let pointer = file_key.expose().as_ptr();

        assert_eq!(
            unsafe { std::slice::from_raw_parts(pointer, 32) },
            &[0xAAu8; 32]
        );

        // Zeroize in place via the SecretBox Zeroize impl reachable through
        // a mutable field borrow. SecretBox<S> implements Zeroize whenever
        // S: Zeroize, which [u8; 32] satisfies.
        Zeroize::zeroize(&mut file_key.0);

        assert_eq!(
            unsafe { std::slice::from_raw_parts(pointer, 32) },
            &[0u8; 32]
        );
        // file_key dropped here; SecretBox::drop is a no-op on already-zero memory.
    }

    #[test]
    fn key_encryption_key_zeroizes_via_zeroize_trait() {
        let mut key_encryption_key = KeyEncryptionKey::from_bytes([0x5Au8; 32]);
        let pointer = key_encryption_key.expose().as_ptr();

        assert_eq!(
            unsafe { std::slice::from_raw_parts(pointer, 32) },
            &[0x5Au8; 32]
        );

        Zeroize::zeroize(&mut key_encryption_key.0);

        assert_eq!(
            unsafe { std::slice::from_raw_parts(pointer, 32) },
            &[0u8; 32]
        );
    }
}
```

**Notes on accessor visibility**: `from_bytes` and `expose` are `pub(crate)` because external callers should not mint keys directly — only the crypto module itself does. Phase 1.2 (`encrypt_chunk.rs`, `decrypt_chunk.rs`) and Phase 1.3 (`wrap_key.rs`, `generate_file_key.rs`) need `FileKey::expose` and `KeyEncryptionKey::expose`; they live inside `src-tauri/src/crypto/` so `pub(crate)` is the correct boundary.

### Step 3 — Create `src-tauri/src/crypto/hkdf.rs`

New file. Implements `derive_vault_keys` and defines `VaultKeys`.

```rust
//! HKDF-SHA256 vault key derivation.
//!
//! Single extract, three expands. Info strings and salt are fixed domain
//! separators per `design-invariants.md` invariant #3.

use crate::crypto::types::{KeyEncryptionKey, ManifestKey, SqlcipherKey};
use hkdf::Hkdf;
use sha2::Sha256;
use zeroize::Zeroizing;

/// Fixed HKDF salt. Domain separator; encodes application identity and
/// key-hierarchy version. Any change is a breaking cryptographic migration.
const HKDF_SALT: &[u8] = b"arx-runa-v1";

const HKDF_INFO_KEY_ENCRYPTION: &[u8] = b"arx-runa-key-encryption";
const HKDF_INFO_SQLCIPHER: &[u8] = b"arx-runa-sqlcipher";
const HKDF_INFO_MANIFEST_BACKUP: &[u8] = b"arx-runa-manifest-backup";

/// Triple of vault-level keys derived from the master key.
pub struct VaultKeys {
    pub key_encryption_key: KeyEncryptionKey,
    pub sqlcipher_key: SqlcipherKey,
    pub manifest_key: ManifestKey,
}

/// Derives the three vault-level keys from `master_key_bytes` via HKDF-SHA256.
///
/// `master_key_bytes` is a transient view of 32 bytes of high-entropy key
/// material (typically Argon2id output; Phase 2 will supply a `MasterKey`
/// newtype and this signature will be updated then — see plan Concern 2).
///
/// # Panics
/// Panics only if HKDF expansion fails, which cannot happen for a 32-byte
/// output with SHA-256 (max okm length is 255 * 32 = 8160 bytes).
pub fn derive_vault_keys(master_key_bytes: &[u8; 32]) -> VaultKeys {
    let hkdf = Hkdf::<Sha256>::new(Some(HKDF_SALT), master_key_bytes);

    let mut kek_bytes = Zeroizing::new([0u8; 32]);
    hkdf.expand(HKDF_INFO_KEY_ENCRYPTION, kek_bytes.as_mut_slice())
        .expect("HKDF expand must not fail for 32-byte output");

    let mut sqlcipher_bytes = Zeroizing::new([0u8; 32]);
    hkdf.expand(HKDF_INFO_SQLCIPHER, sqlcipher_bytes.as_mut_slice())
        .expect("HKDF expand must not fail for 32-byte output");

    let mut manifest_bytes = Zeroizing::new([0u8; 32]);
    hkdf.expand(HKDF_INFO_MANIFEST_BACKUP, manifest_bytes.as_mut_slice())
        .expect("HKDF expand must not fail for 32-byte output");

    VaultKeys {
        key_encryption_key: KeyEncryptionKey::from_bytes(*kek_bytes),
        sqlcipher_key: SqlcipherKey::from_bytes(*sqlcipher_bytes),
        manifest_key: ManifestKey::from_bytes(*manifest_bytes),
    }
    // kek_bytes, sqlcipher_bytes, manifest_bytes all drop here and Zeroizing
    // scrubs them before the stack frame is freed.
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derive_vault_keys_deterministic() {
        let master_key_bytes = [0x42u8; 32];
        let first = derive_vault_keys(&master_key_bytes);
        let second = derive_vault_keys(&master_key_bytes);

        assert_eq!(first.key_encryption_key.expose(), second.key_encryption_key.expose());
        assert_eq!(first.sqlcipher_key.expose(), second.sqlcipher_key.expose());
        assert_eq!(first.manifest_key.expose(), second.manifest_key.expose());
    }

    #[test]
    fn derive_vault_keys_different_inputs_produce_different_outputs() {
        let master_a = [0x01u8; 32];
        let master_b = [0x02u8; 32];

        let keys_a = derive_vault_keys(&master_a);
        let keys_b = derive_vault_keys(&master_b);

        assert_ne!(keys_a.key_encryption_key.expose(), keys_b.key_encryption_key.expose());
        assert_ne!(keys_a.sqlcipher_key.expose(), keys_b.sqlcipher_key.expose());
        assert_ne!(keys_a.manifest_key.expose(), keys_b.manifest_key.expose());
    }

    #[test]
    fn derive_vault_keys_produces_three_mutually_distinct_keys() {
        let master_key_bytes = [0xA5u8; 32];
        let keys = derive_vault_keys(&master_key_bytes);

        assert_ne!(keys.key_encryption_key.expose(), keys.sqlcipher_key.expose());
        assert_ne!(keys.key_encryption_key.expose(), keys.manifest_key.expose());
        assert_ne!(keys.sqlcipher_key.expose(), keys.manifest_key.expose());
    }

    #[test]
    fn derive_vault_keys_from_zero_master_still_succeeds() {
        // Guards against `unwrap`/`expect` regressions in the expand path for
        // degenerate but syntactically valid inputs.
        let master_key_bytes = [0u8; 32];
        let keys = derive_vault_keys(&master_key_bytes);
        // Three keys exist and are distinct — even with an all-zero IKM,
        // distinct info strings guarantee distinct outputs under HKDF.
        assert_ne!(keys.key_encryption_key.expose(), keys.sqlcipher_key.expose());
    }
}
```

### Step 4 — Create `src-tauri/src/crypto/nonce.rs`

New file. Single public function `generate_nonce`.

```rust
//! CSPRNG nonce generation for XChaCha20-Poly1305.
//!
//! 24-byte (192-bit) random nonces. Sequential, counter-based, or derived
//! nonces are forbidden per `design-invariants.md` invariant #2.

use rand::Rng;

/// Generates a random 24-byte nonce for XChaCha20-Poly1305.
///
/// Uses `rand::rng()` (rand 0.10 thread-local CSPRNG). Collision probability
/// after 2^64 encryptions is ~2^-64 — negligible for Arx Runa's use case.
pub fn generate_nonce() -> [u8; 24] {
    rand::rng().random::<[u8; 24]>()
}

#[cfg(test)]
mod tests {
    use super::generate_nonce;
    use std::collections::HashSet;

    #[test]
    fn nonce_is_24_bytes() {
        let nonce = generate_nonce();
        assert_eq!(nonce.len(), 24);
    }

    #[test]
    fn thousand_nonces_are_unique() {
        let mut seen: HashSet<[u8; 24]> = HashSet::with_capacity(1000);
        for _ in 0..1000 {
            let nonce = generate_nonce();
            assert!(
                seen.insert(nonce),
                "nonce collision in 1000-sample test — CSPRNG may be broken"
            );
        }
        assert_eq!(seen.len(), 1000);
    }

    #[test]
    fn nonces_are_not_all_zero() {
        // Smoke test: a constant-zero RNG would pass uniqueness only once.
        // Two non-zero nonces in a row eliminate the all-zero degenerate case.
        let first = generate_nonce();
        let second = generate_nonce();
        assert_ne!(first, [0u8; 24]);
        assert_ne!(second, [0u8; 24]);
        assert_ne!(first, second);
    }
}
```

### Step 5 — Update `src-tauri/src/crypto/mod.rs` to declare and re-export new modules

```rust
//! Arx Runa crypto module.
//!
//! Cryptographic primitives: key derivation, chunk encryption, file key
//! management, BLAKE3 checksums.

pub mod error;
pub mod hkdf;
pub mod nonce;
pub mod types;

pub use error::CryptoError;
pub use hkdf::{VaultKeys, derive_vault_keys};
pub use nonce::generate_nonce;
pub use types::{
    Blake3Hash, ChunkIndex, FileId, FileKey, KeyEncryptionKey, ManifestKey, SqlcipherKey,
    WrappedFileKey,
};
```

Re-exports keep Phase 1.2/1.3 and downstream modules on the stable path `crate::crypto::FileKey` rather than `crate::crypto::types::FileKey`.

### Step 6 — Run the verification checklist

From `src-tauri/`:

```
cargo fmt
cargo clippy --all-targets -- -D warnings
cargo test crypto
```

All three must pass. Expected tests (11 total in 1.1):

- `crypto::error::tests::crypto_error_variants_constructible`
- `crypto::error::tests::crypto_error_display_formats`
- `crypto::types::tests::file_id_roundtrips_through_uuid`
- `crypto::types::tests::file_id_new_preserves_bytes`
- `crypto::types::tests::chunk_index_big_endian_encoding`
- `crypto::types::tests::file_key_zeroizes_via_zeroize_trait`
- `crypto::types::tests::key_encryption_key_zeroizes_via_zeroize_trait`
- `crypto::hkdf::tests::derive_vault_keys_deterministic`
- `crypto::hkdf::tests::derive_vault_keys_different_inputs_produce_different_outputs`
- `crypto::hkdf::tests::derive_vault_keys_produces_three_mutually_distinct_keys`
- `crypto::hkdf::tests::derive_vault_keys_from_zero_master_still_succeeds`
- `crypto::nonce::tests::nonce_is_24_bytes`
- `crypto::nonce::tests::thousand_nonces_are_unique`
- `crypto::nonce::tests::nonces_are_not_all_zero`

(Count = 14. Sub-phase requires the five named tests; the plan adds nine supporting tests that map to acceptance criteria and surface-level safety nets.)

### Step 7 — Sanity checks before handoff completion

- `cargo check -p arx-runa-tauri` completes without warnings.
- No new dependencies added to `Cargo.toml`.
- No `use std::mem::transmute` or other sharp unsafe anywhere except the documented zeroize-verification tests.
- `src-tauri/src/crypto/mod.rs` compiles with the five `pub use` re-exports listed in step 5.

---

## 6. Security Implications

### a. Expected sensitive path set

- `src-tauri/src/crypto/error.rs` — rewritten
- `src-tauri/src/crypto/types/mod.rs` — rewritten
- `src-tauri/src/crypto/hkdf.rs` — created
- `src-tauri/src/crypto/nonce.rs` — created
- `src-tauri/src/crypto/mod.rs` — edited (re-export block)

Anything touched outside this list under `src-tauri/src/crypto/`, `src-tauri/src/auth/`, or `src-tauri/src/storage/` is a plan deviation and must be flagged.

### b. Invoke `security-reviewer` agent? **YES**

The sub-phase mandates it (§ Security Review, line 67). Independent review by this plan confirms: this sub-phase introduces the type substrate and the HKDF extract/expand operation, which are load-bearing for every downstream crypto operation. A review is warranted.

### c. What the reviewer should check

1. **HKDF info-string uniqueness**: confirm `HKDF_INFO_KEY_ENCRYPTION`, `HKDF_INFO_SQLCIPHER`, `HKDF_INFO_MANIFEST_BACKUP` are byte-distinct and match `design-invariants.md` invariant #3 exactly. Check that no typo (e.g., `arx_runa_sqlcipher` with underscore) was introduced.
2. **Zeroization coverage**:
   - Every `SecretBox<[u8; 32]>` field zeroizes on drop (derived via `SecretBox<S>: ZeroizeOnDrop`).
   - Intermediate `Zeroizing<[u8; 32]>` buffers in `derive_vault_keys` scrub on drop; no `[u8; 32]` copy escapes the function without being wrapped.
   - `master_key_bytes` parameter is borrowed, not owned — the caller remains responsible for zeroization. Confirm no interior copies are made (e.g., no `let m = *master_key_bytes;`).
3. **No `Debug`, `Clone`, `Copy`, `Serialize`, or `Deserialize` on `FileKey`, `KeyEncryptionKey`, `SqlcipherKey`, `ManifestKey`**. Compile-time check via `static_assertions` is optional; visual inspection suffices.
4. **CSPRNG source**: `rand::rng()` in rand 0.10 returns `ThreadRng`, which is seeded from the OS CSPRNG and is cryptographically secure. Confirm the codebase does not shadow `rand::rng` with a test double.
5. **HKDF salt immutability**: `HKDF_SALT` is `const`, not `static mut` or env-configurable. Hardcoded as `b"arx-runa-v1"`.
6. **Test soundness**: the `file_key_zeroizes_via_zeroize_trait` test is the only place where raw pointer arithmetic reads `SecretBox` bytes. Confirm the pointer is only dereferenced while the `SecretBox` is live (see plan Concern 5) — i.e., the read happens after `Zeroize::zeroize(&mut self.0)` but before the drop at end-of-scope.
7. **No logging of secrets**: no `println!`, `eprintln!`, `dbg!`, `tracing::*!`, or `log::*!` macro calls touch any value produced by `expose_secret`, `expose`, or the HKDF output buffers.

---

## 7. Testing Strategy

**Test scope**:
- [x] Basic unit tests (inline in each module)
- [x] Adversarial tests (zeroize verification, HKDF input separation)
- [ ] Property-based tests (deferred to Phase 1.2 — no encrypt/decrypt round-trip surface yet)
- [ ] Integration tests (not applicable — single-module scope)
- [x] Boundary cases (`[0u8; 32]` master key, `u32::MAX` chunk index, 1000-sample nonce uniqueness)

**Coverage target**: 100% of the Phase 1.1 public surface (`derive_vault_keys`, `generate_nonce`, every `FileId`/`ChunkIndex` accessor, `CryptoError` Display, zeroize on `FileKey` and `KeyEncryptionKey`).

**Boundary cases covered**:
- All-zero IKM for HKDF (degenerate but valid)
- `ChunkIndex(0)`, `ChunkIndex(1)`, `ChunkIndex(0x01020304)`, `ChunkIndex(u32::MAX)`
- `FileId` round-trip through `uuid::Uuid`
- 1000-sample nonce uniqueness (matches acceptance criterion)
- HKDF expansion on three distinct info strings produces three mutually distinct outputs

**Invoke test-writer agent?** **NO** — rust-implementer's inline tests cover the full sub-phase acceptance criteria and the plan's supplementary cases. Rationale: (a) the test surface is modest (~14 tests, ~150 LOC); (b) `proptest` property tests are premature without encrypt/decrypt in scope; (c) the zeroize test is a delicate ownership/lifetime pattern that benefits from being authored alongside the type definition, not retroactively; (d) `cargo-tarpaulin` or manual review can confirm branch coverage during security review. The crypto-roundtrip-test skill becomes appropriate in Phase 1.2 when `encrypt_chunk`/`decrypt_chunk` land.

**Test acceptance criteria**:
- All 14 tests pass on `cargo test crypto` with no warnings under `-D warnings`.
- `derive_vault_keys` is deterministic for identical inputs (test: `derive_vault_keys_deterministic`).
- `derive_vault_keys` produces distinct outputs for distinct inputs (test: `derive_vault_keys_different_inputs_produce_different_outputs`).
- `derive_vault_keys` produces three mutually distinct keys from one input (test: `derive_vault_keys_produces_three_mutually_distinct_keys`).
- 1000 generated nonces are all unique (test: `thousand_nonces_are_unique`).
- Zeroize is observable on `FileKey` and `KeyEncryptionKey` memory (tests: `file_key_zeroizes_via_zeroize_trait`, `key_encryption_key_zeroizes_via_zeroize_trait`).
- `CryptoError` variants are constructible and display correctly (tests: `crypto_error_variants_constructible`, `crypto_error_display_formats`).
- No new clippy warnings under `cargo clippy --all-targets -- -D warnings`.

**Validation checkpoint commands** (from sub-phase, run from `src-tauri/`):
```
cargo test crypto::types
cargo test crypto::hkdf
cargo test crypto::nonce
cargo test crypto::error
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

---

## 8. Documentation Impact

- **None required by Phase 1.1**. Sub-phase line 98 confirms: "Phase 1.1: No doc updates needed."
- **Deferred to Phase 1.3**: `docs/architecture/designs/cryptographic-primitives/diagrams/key-derivation-tree.md` review and `docs/roadmap.md` Phase 1 status update.
- **Recommended follow-up edits (out of scope for this plan)** — to be handled via a separate `/design` or manual edit pass, driven by Design Concerns 1–3 above:
  - Patch `design.md` § Type Definitions (lines 370–394) to use `SecretBox<[u8; 32]>` instead of `Secret<[u8; 32]>`.
  - Patch `1.1-key-types-and-derivation.md` deliverable 1 same fix.
  - Patch `1.1-key-types-and-derivation.md` deliverable 6 to use `&[u8; 32]` instead of `&MasterKey` to match Implementation Note 1.
  - Patch `1.1-key-types-and-derivation.md` deliverable 8 to say "(rand 0.10 API)".
  - Optionally update `CryptoError` section in `design.md` to document that `KeyUnwrapFailed` is reserved for Phase 1.3 use.

---

## 9. Handoff Notes for Implementer

You are Codex. Working directory is `C:\Users\chris\source\repos\arx-runa\`. Platform: Windows 11, shell is bash (use forward slashes and Unix syntax). Rust toolchain is whatever `rust-toolchain.toml` pins — do not reinitialise it.

This plan is **self-contained**: every type, every function signature, every test is inlined. You do NOT need to re-read the sub-phase or the parent design to implement Phase 1.1. The plan supersedes them on every point where Design Concerns 1–7 flagged a drift (notably: use `SecretBox<[u8; 32]>`, not `Secret<[u8; 32]>`; use `&[u8; 32]`, not `&MasterKey`; keep `VaultKeys` in `hkdf.rs`).

**Order of operations**:
1. Step 1 — rewrite `error.rs`.
2. Step 2 — rewrite `types/mod.rs`.
3. Step 3 — create `hkdf.rs`.
4. Step 4 — create `nonce.rs`.
5. Step 5 — update `mod.rs` with new submodules and re-exports.
6. Step 6 — run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, `cargo test crypto`.
7. If all pass, invoke `security-reviewer` agent on the five files listed in § 6a with the checklist in § 6c.
8. If the reviewer returns clean, update this plan's frontmatter `status: implemented` and commit.

**Traps**:
- `Secret<T>` is not exported by `secrecy = "0.10"`. Use `SecretBox<T>`. If the implementer writes `use secrecy::Secret;` the build will fail immediately — do not work around this by adding a type alias; the correct fix is the code in step 2.
- `rand::thread_rng()` was removed in rand 0.9. Use `rand::rng()`.
- The `hkdf` crate's `Hkdf::<Sha256>::new` takes `Option<&[u8]>` for salt — pass `Some(HKDF_SALT)`, not `HKDF_SALT` directly.
- `SecretBox::new` takes `Box<S>`, not `S` — `SecretBox::new(Box::new(bytes))` is correct; `SecretBox::new(bytes)` is a type error.
- The zeroize test in `types/mod.rs` uses `Zeroize::zeroize(&mut self.0)` *before* drop to avoid use-after-free. Do not replicate the UB pattern from the sub-phase line 59 ("read back the dropped memory via a raw pointer").
- No `Clone` derive on any key type. If clippy suggests `#[derive(Clone)]` on `FileKey`, ignore it — the absence is intentional.
- Windows-only: `cargo test` output paths may use backslashes; this does not affect test correctness. If any test uses path literals, use `std::path::MAIN_SEPARATOR` or forward slashes — but Phase 1.1 has no filesystem tests.

No feature flags, no `#[cfg(...)]` platform gates, no conditional compilation. Phase 1.1 is fully cross-platform by construction.
