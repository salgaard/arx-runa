---
title: "Phase 1.3 — Key Wrapping, BLAKE3 Checksums, and File Key Generation"
created: "2026-04-13T00:00:00Z"
status: implemented
roadmap-phase: 1
sub-phase: "1.3"
design-document: "docs/architecture/designs/cryptographic-primitives/design.md"
sub-phase-roadmap: "docs/architecture/designs/cryptographic-primitives/sub-phases/roadmap.md"
implementation-agent: rust-implementer
test-agent-required: false
tags: [crypto, phase-1, wrap-key, blake3, checksum, verified-blob]
---

# Plan: Phase 1.3 — Key Wrapping, BLAKE3 Checksums, and File Key Generation

## 1. Goal

Add per-file key generation, XChaCha20-Poly1305 file-key wrapping/unwrapping, BLAKE3 checksums over encrypted blobs, and the `VerifiedBlob` check-before-decrypt enforcement type, completing the Phase 1 crypto primitives surface that Phase 2+ will consume.

## 2. Context

**Roadmap**: Phase 1 — Cryptographic Primitives (`docs/roadmap.md` lines 43–49). Depends on Phase 1.1 (key types, `CryptoError`, `generate_nonce`, `FileKey::from_bytes`, `KeyEncryptionKey::expose`) and Phase 1.2 (`encrypt_chunk`, `decrypt_chunk` with temporary `&[u8]` signature).

**Sub-phase roadmap**: `docs/architecture/designs/cryptographic-primitives/sub-phases/roadmap.md`. Strict order 1.1 → 1.2 → 1.3. Sub-phase 1.3 is the final Phase 1 unit (apart from the recovery-slot APIs deferred to Phase 2 — see Design Concern #1).

**Sub-phase doc**: `docs/architecture/designs/cryptographic-primitives/sub-phases/1.3-key-wrapping-and-checksums.md`. Estimated scope ~80 LOC production + ~80 LOC tests; the plan's final total is closer to ~130 LOC tests after the adversarial additions from Design Concern #5.

**Parent design**: `docs/architecture/designs/cryptographic-primitives/design.md`. Canonical commitments from the Contract Surface (lines 17–43 — canonical per `CLAUDE.md`):

- **Interface contract**: `generate_file_key`, `wrap_file_key`, `unwrap_file_key`, `compute_checksum`, `verify_checksum` are public. The Contract Surface also lists `wrap_master_key_for_recovery`/`unwrap_master_key_from_recovery` — deferred to Phase 2 (Design Concern #1).
- **Data contract**: `WrappedFileKey` wire format is `[24-byte nonce | 32-byte ciphertext | 16-byte tag]` (72 bytes total). `VerifiedBlob` is a canonical domain type that gates `decrypt_chunk`.
- **Invariant contract**: cipher is `XChaCha20Poly1305` only; nonces are random 24-byte CSPRNG values; `wrap_file_key` uses **empty AAD**; BLAKE3 is computed over ciphertext (never plaintext); `VerifiedBlob` enforces check-before-decrypt at compile time.
- **Dependency contract**: consumes `FileKey`, `KeyEncryptionKey`, `Blake3Hash` from `types/`; relies on `blake3 = "1"` (already present in `src-tauri/Cargo.toml`).

**Existing state** (post Phase 1.2, commit `db9a1b5`):

- `src-tauri/Cargo.toml` already pins `blake3 = "1"` (line 37), `chacha20poly1305 = "0.10"` (line 32), `rand = "0.10"` (line 38), `zeroize = { version = "1", features = ["derive"] }` (line 39), `secrecy = "0.10"` (line 40), `thiserror = "2"` (line 27). `proptest = "1"` and `assert_matches = "1"` are already in `[dev-dependencies]`. **No dependency additions required.**
- `src-tauri/src/crypto/mod.rs` declares `pub mod decrypt_chunk; pub mod encrypt_chunk; pub mod error; pub mod hkdf; pub mod nonce; pub mod types;` and re-exports `decrypt_chunk`, `encrypt_chunk`, `CryptoError`, `VaultKeys`, `derive_vault_keys`, `generate_nonce`, plus `Blake3Hash`, `ChunkIndex`, `FileId`, `FileKey`, `KeyEncryptionKey`, `ManifestKey`, `SqlcipherKey`, `WrappedFileKey`.
- `src-tauri/src/crypto/types/mod.rs` defines `WrappedFileKey(pub [u8; 72])` (line 95) and `Blake3Hash(pub [u8; 32])` (line 146). Both have direct public tuple-field access; no accessor methods.
- `src-tauri/src/crypto/error.rs` already defines `CryptoError::{DecryptionFailed, InvalidBlobFormat { expected, actual }, KeyUnwrapFailed, ChecksumMismatch}`. `ChecksumMismatch` exists but has no callsite yet.
- `src-tauri/src/crypto/decrypt_chunk.rs` has the temporary `pub fn decrypt_chunk(blob: &[u8], ...) -> Result<Vec<u8>, CryptoError>` signature from Phase 1.2 plus a module doc-comment flagging the Phase 1.3 migration (lines 1–6).
- `src-tauri/src/crypto/encrypt_chunk.rs` contains round-trip tests and proptests (lines 108, 131) that call `decrypt_chunk(&blob, ...)` — these must also migrate.
- `src-tauri/src/crypto/checksum.rs`, `src-tauri/src/crypto/generate_file_key.rs`, and `src-tauri/src/crypto/wrap_key.rs` do not yet exist.
- `FileKey::from_bytes(bytes: [u8; 32])` and `FileKey::expose(&self) -> &[u8; 32]` are `pub(crate)` and accessible from sibling modules inside `src-tauri/src/crypto/`. Same for `KeyEncryptionKey`. `KeyEncryptionKey::from_secret_box(secret_box: SecretBox<[u8; 32]>)` already exists (types/mod.rs line 30); `FileKey::from_secret_box` does **not** yet exist and will be added by Step 5.2.
- `SecretBox::<[u8; 32]>::init_with_mut` is the established secret-buffer initializer in this codebase (see `hkdf.rs` lines 31–44).

## 3. Design Concerns / Open Questions

### Concern #1 — Contract Surface lists recovery-slot wrapping APIs that Phase 1.3 does not deliver
- **Concern**: Four canonical APIs (`wrap_master_key_for_recovery`, `unwrap_master_key_from_recovery`, `RecoveryKey`, `WrappedMasterKey`) are listed in the parent design's Contract Surface but are not in Phase 1.3's deliverables.
- **Source**: `docs/architecture/designs/cryptographic-primitives/design.md` lines 21, 27 (Contract Surface Interface + Data contracts) and lines 148–213 (Recovery Master Key Wrapping section). Sub-phase 1.3 deliverables (lines 10–19) do not include them; sub-phase 1.3's "Completion" section (lines 77–82) calls 1.3 "the final sub-phase for Phase 1".
- **Impact**: A reader comparing the Contract Surface to Phase 1's implemented surface will see four declared APIs missing and may believe Phase 1 is incomplete or that the sub-phase roadmap is wrong.
- **Classification**: Non-blocking. All four depend on `MasterKey`, which sub-phase 1.1's roadmap note (roadmap line 97) explicitly defers to Phase 2 (authentication): "Phase 1 receives `master_key` as input; its definition and derivation (Argon2id) belong to Phase 2 (authentication)." The recovery wrap/unwrap functions cannot be implemented before `MasterKey` exists, so the sub-phase boundary is correct — only the documentation fails to make it explicit.
- **Resolution**: Phase 1.3 does not implement recovery-slot wrapping. Phase 2 (authentication) will add `MasterKey`, `RecoveryKey`, `WrappedMasterKey`, `wrap_master_key_for_recovery`, and `unwrap_master_key_from_recovery` alongside Argon2id KDF work. This plan makes the boundary explicit in documentation updates (Section 8).
- **Documentation sync required on implementation**:
  - `docs/architecture/designs/cryptographic-primitives/sub-phases/1.3-key-wrapping-and-checksums.md`, "Completion" section (lines 75–82): amend to state that Phase 1.3 closes out Phase 1's vault-key, chunk-AEAD, per-file-key, and integrity-checksum deliverables, while recovery-slot wrapping is owned by Phase 2 because it depends on the `MasterKey` type introduced by the authentication design.
  - `docs/roadmap.md` Phase 1 summary (lines 43–49): add a one-line note that recovery-slot wrapping (`wrap_master_key_for_recovery` / `unwrap_master_key_from_recovery`) is Phase 2 work so that a reader comparing Contract Surface to Phase 1 deliverables understands the phase boundary.

### Concern #2 — `VerifiedBlob` inner-field visibility is not specified
- **Concern**: Sub-phase says `VerifiedBlob` is "only constructible via `verify_checksum`" but does not specify field visibility. If the tuple field is `pub`, any caller can synthesise a `VerifiedBlob` and the compile-time invariant evaporates.
- **Source**: Sub-phase 1.3 Deliverable #5 (line 16): "`VerifiedBlob(Vec<u8>)` opaque newtype in `src-tauri/src/crypto/checksum.rs` — only constructible via `verify_checksum`". Parent design line 341: "Opaque wrapper — only constructible by verify_checksum."
- **Impact**: Without a private field, external code could do `VerifiedBlob(arbitrary_bytes)` and bypass the check. `decrypt_chunk` in a sibling module also needs some way to read the inner bytes; the sub-phase is silent on the accessor.
- **Classification**: Non-blocking.
- **Resolution**: Make the inner field private. Expose `pub(crate) fn into_inner(self) -> Vec<u8>` so `decrypt_chunk` (a sibling module) can consume the bytes. No public accessor, no `Clone`, no `From<Vec<u8>>`, no `Deref`/`AsRef<[u8]>` impls — the type stays opaque to consumers outside `crypto/`.
- **Documentation sync required on implementation**: None. Design.md says "opaque wrapper"; a private field with a `pub(crate)` consumer is a straightforward realisation of "opaque".

### Concern #3 — Constant-time comparison for `verify_checksum` is not specified
- **Concern**: Sub-phase and design do not say whether the checksum equality check must be constant time.
- **Source**: Sub-phase 1.3 Deliverable #6 (line 17) and design.md lines 343–351.
- **Impact**: A timing attacker observing `verify_checksum` could in principle infer how many prefix bytes of the computed hash match the expected value. For an unkeyed hash whose expected value is already attacker-observable (stored in the manifest alongside the downloaded blob), this leaks nothing the attacker doesn't already know.
- **Classification**: Non-blocking.
- **Resolution**: Use plain `==` comparison on `[u8; 32]`. Design.md line 364 explicitly justifies unkeyed BLAKE3: "it provides fast detection of hardware/network corruption before the more expensive AEAD decryption. The manifest (SQLCipher) protects the stored hashes, so unkeyed is operationally sufficient." No security property depends on constant-time comparison; `subtle::ConstantTimeEq` is not pulled in.
- **Documentation sync required on implementation**: None.

### Concern #4 — Zeroization pattern for `unwrap_file_key`'s intermediate 32-byte buffer is not spelled out
- **Concern**: Sub-phase implementation note says "construct `FileKey` inside `SecretBox<[u8; 32]>` immediately" but does not specify *how*. Two reasonable patterns exist and they have different failure-path guarantees.
- **Source**: Sub-phase 1.3 Implementation Notes line 57.
- **Impact**: Pattern (a) — decrypt into a stack-local `[u8; 32]`, copy into `SecretBox::new(Box::new(buffer))`, then manually `zeroize` the stack local — requires the implementer to remember the zeroize on every return path. Pattern (b) — `SecretBox::<[u8; 32]>::init_with_mut` — runs decryption directly inside the `SecretBox`-owned heap buffer, so the `SecretBox`'s `Drop` implementation zeroizes both success and failure paths without manual discipline. Silent divergence between implementers would create an audit blind spot.
- **Classification**: Non-blocking.
- **Resolution**: Use pattern (b), matching the existing `hkdf.rs` idiom (lines 31–44). The decrypt call runs inside the `init_with_mut` closure on a `&mut [u8; 32]` view of the boxed buffer. On authentication failure, the `SecretBox` is dropped at the end of the function and `Drop` clears the allocation; partial-keystream bytes cannot outlive the function. The exact code is given in Step 5.5.
- **Documentation sync required on implementation**: None.

### Concern #5 — Sub-phase test list is under-specified for `wrap_file_key`/`unwrap_file_key` adversarial coverage
- **Concern**: The enumerated test list covers round-trip and wrong-KEK only. It omits nonce uniqueness, explicit wire-format size check, and tampering of each of the three byte regions (nonce, ciphertext, tag).
- **Source**: Sub-phase 1.3 Deliverable #8 (line 19).
- **Impact**: Regression risk — a future change that reuses nonces, or that silently drops tag bytes, could pass the enumerated tests.
- **Classification**: Non-blocking.
- **Resolution**: Extend the test suite (Step 5.5) with: `test_wrap_file_key_wire_format_is_seventy_two_bytes`, `test_wrap_file_key_two_calls_produce_different_blobs`, `test_unwrap_file_key_corrupted_nonce_fails_with_decryption_failed`, `test_unwrap_file_key_corrupted_ciphertext_fails_with_decryption_failed`, `test_unwrap_file_key_corrupted_tag_fails_with_decryption_failed`, `test_unwrap_file_key_all_zero_wrapped_blob_fails_with_decryption_failed`, and `test_generate_file_key_consecutive_calls_produce_different_keys` (in `generate_file_key.rs`).
- **Documentation sync required on implementation**: None.

### Concern #6 — Phase 1.2 callsite migration for `decrypt_chunk(VerifiedBlob, ...)` affects both `decrypt_chunk.rs` and `encrypt_chunk.rs`
- **Concern**: Sub-phase Deliverable #7 and Implementation Notes line 59 mention updating `decrypt_chunk.rs` test callsites, but do not mention `encrypt_chunk.rs`, which also contains round-trip tests and proptests that call `decrypt_chunk(&blob, ...)`.
- **Source**: `src-tauri/src/crypto/encrypt_chunk.rs` line 117 (`test_encrypt_decrypt_round_trip_returns_original_plaintext`) and line 143 (`prop_encrypt_decrypt_identity`).
- **Impact**: A naive reading of the sub-phase would leave these call sites untouched, and the crate would fail to compile after the signature migration.
- **Classification**: Non-blocking.
- **Resolution**: Step 5.7 explicitly enumerates every `decrypt_chunk` callsite that must be updated in `encrypt_chunk.rs` (one in `mod tests`, one in `mod proptests`). The pattern is to wrap the blob in `verify_checksum(blob, &compute_checksum(&blob))` — BLAKE3 accepts any length including zero, so this works even for deliberately-short blobs used in `InvalidBlobFormat` tests.
- **Documentation sync required on implementation**: None.

### Concern #7 — `blake3` dependency note is stale
- **Concern**: Sub-phase Implementation Notes line 61 says "Add `blake3` to `[dependencies]` in `Cargo.toml` before implementation." It is already there.
- **Source**: `src-tauri/Cargo.toml` line 37: `blake3 = "1"`.
- **Impact**: Benign but would cause an alert implementer to pause and wonder if something is wrong.
- **Classification**: Non-blocking.
- **Resolution**: Step 5.1 verifies the pin and skips the addition. No edit to `Cargo.toml`.
- **Documentation sync required on implementation**: None.

## 4. Assumptions

Treat these as plan-level commitments. If any is wrong, pause and re-plan:

1. `blake3 = "1"` is already pinned in `src-tauri/Cargo.toml` (line 37). Version 1.x exposes `blake3::hash(data: &[u8]) -> blake3::Hash` and `blake3::Hash::as_bytes(&self) -> &[u8; 32]`. No feature flags required.
2. `chacha20poly1305 = "0.10"` exposes `XChaCha20Poly1305`, `KeyInit::new(&Key)`, and `AeadInPlace::{encrypt_in_place_detached, decrypt_in_place_detached}` with the signatures already used in Phase 1.2 (`encrypt_chunk.rs`, `decrypt_chunk.rs`). `Tag = GenericArray<u8, U16>`, `XNonce = GenericArray<u8, U24>`. No additional imports are required beyond what those files already use.
3. `FileKey::from_bytes(bytes: [u8; 32])`, `FileKey::expose(&self) -> &[u8; 32]`, and `KeyEncryptionKey::expose(&self) -> &[u8; 32]` are `pub(crate)` and callable from sibling modules inside `src-tauri/src/crypto/`. `FileKey::from_secret_box(secret_box: SecretBox<[u8; 32]>)` does **not** currently exist and is added in Step 5.2, mirroring `KeyEncryptionKey::from_secret_box` (types/mod.rs line 30).
4. `SecretBox::<[u8; 32]>::init_with_mut(|buf| { ... })` is the canonical secret-buffer initializer in this codebase (see `hkdf.rs` lines 31–44). The closure receives `buf: &mut [u8; 32]`, runs to completion (no early return), and returns the constructed `SecretBox<[u8; 32]>`. `SecretBox`'s `Drop` zeroizes the underlying allocation.
5. `generate_nonce()` in `src-tauri/src/crypto/nonce.rs` is the sole nonce source. `wrap_file_key` calls it once per wrap; no local `rand::rng()` call in `wrap_key.rs`.
6. `rand::rng().random::<[u8; 32]>()` (with `use rand::RngExt;`) is the rand 0.10 idiom for generating 32 random bytes — this matches the existing `nonce.rs` usage for `[u8; 24]`.
7. `WrappedFileKey(pub [u8; 72])` is the existing shape from Phase 1.1 (`types/mod.rs` line 95). The plan constructs wrapped blobs via `WrappedFileKey(bytes)` and reads them via `wrapped.0`. The `pub` field visibility is a pre-existing Phase 1.1 decision; the plan does not change it.
8. `Blake3Hash(pub [u8; 32])` is the existing shape from Phase 1.1 (`types/mod.rs` line 146). The plan constructs hashes via `Blake3Hash(*blake3::hash(data).as_bytes())` and compares via `.0 == other.0`. No accessor method is added.
9. Tests live inline as `#[cfg(test)] mod tests { ... }` in the same file as the function under test, matching the existing convention across `nonce.rs`, `hkdf.rs`, `error.rs`, `types/mod.rs`, `encrypt_chunk.rs`, and `decrypt_chunk.rs`. Property tests live in a sibling `#[cfg(test)] mod proptests` block in the same file.
10. `proptest = "1"` is already in `[dev-dependencies]` and supports `proptest::collection::vec(any::<u8>(), 0..=N)`. Property strategy size bound is `0..=64 * 1024` bytes (64 KiB) to keep `prop_checksum_deterministic` fast; 64 KiB is well below the canonical 4 MiB chunk size but exercises empty, single-byte, and multi-block cases.
11. `assert_matches = "1"` is already in `[dev-dependencies]`; `assert_matches::assert_matches!` is the enforcement pattern for `Err` variants.
12. Doc comments on every `pub fn`, `pub struct`, `pub enum` are required by `.claude/rules/rust.md`. Internal helpers get `//` block comments only if they add non-obvious WHY context; otherwise none.
13. `VerifiedBlob` derives only `Debug` — not `Clone`, `PartialEq`, `Eq`, or `Hash`. Ciphertext blobs are large and equality is not meaningful. `Debug` is kept for diagnostic `Result<_, _>` printing.
14. `decrypt_chunk`'s new signature is `pub fn decrypt_chunk(blob: VerifiedBlob, file_key: &FileKey, file_id: &FileId, chunk_index: ChunkIndex) -> Result<Vec<u8>, CryptoError>` — `blob` is consumed. The existing length-check, slice split, and in-place decrypt logic remain unchanged; only the parameter type and the first line that extracts bytes change.
15. `compute_checksum(&[])` on an empty slice is well-defined (BLAKE3 produces a specific 32-byte hash for empty input); no special handling is required.
16. The sub-phase's ~80 LOC test estimate is tight because of the Concern #5 additions; the plan's final test total is closer to ~130 LOC across `checksum.rs`, `wrap_key.rs`, `generate_file_key.rs`, and the `decrypt_chunk.rs`/`encrypt_chunk.rs` migrations.

## 5. Approach

All file paths are absolute. The implementer should execute steps in order. Every step targets exactly the files listed; no other files are touched.

### Step 5.1 — Confirm no dependency edits are required

Verify `C:\Users\chris\source\repos\arx-runa\src-tauri\Cargo.toml` already contains (line numbers for orientation only):

```toml
chacha20poly1305 = "0.10"    # line 32
blake3 = "1"                 # line 37
rand = "0.10"                # line 38
zeroize = { version = "1", features = ["derive"] }  # line 39
secrecy = "0.10"             # line 40
thiserror = "2"              # line 27

[dev-dependencies]
proptest = "1"
assert_matches = "1"
```

All pins are present. Skip the sub-phase's "Add `blake3` to `[dependencies]`" note (Implementation Notes line 61) — it is stale.

### Step 5.2 — Add `FileKey::from_secret_box` constructor

Edit `C:\Users\chris\source\repos\arx-runa\src-tauri\src\crypto\types\mod.rs`. Insert the constructor immediately after `FileKey::from_bytes` (currently at lines 11–14), so the `impl FileKey` block gains a `from_secret_box` method mirroring `KeyEncryptionKey::from_secret_box` (line 30):

```rust
    /// Constructs a file key from protected heap storage.
    ///
    /// Used by `unwrap_file_key` so the decrypted key bytes never exist in a
    /// plain local variable outside the `SecretBox`.
    pub(crate) fn from_secret_box(secret_box: SecretBox<[u8; 32]>) -> Self {
        Self(secret_box)
    }
```

No other changes to `types/mod.rs` in this step. The existing `FileKey::from_bytes` and `FileKey::expose` remain unchanged.

### Step 5.3 — Create `src-tauri/src/crypto/generate_file_key.rs`

Create the file with this exact content:

```rust
//! CSPRNG generation of per-file encryption keys.

use crate::crypto::types::FileKey;
use rand::RngExt;
use secrecy::SecretBox;

/// Generates a cryptographically random 256-bit file key.
///
/// The key is produced by `rand::rng().random::<[u8; 32]>()` and immediately
/// moved into a `SecretBox` so the raw bytes never outlive this function.
pub fn generate_file_key() -> FileKey {
    let random_bytes: [u8; 32] = rand::rng().random::<[u8; 32]>();
    FileKey::from_secret_box(SecretBox::new(Box::new(random_bytes)))
}

#[cfg(test)]
mod tests {
    use super::generate_file_key;

    #[test]
    fn test_generate_file_key_not_all_zeros() {
        let file_key = generate_file_key();
        assert_ne!(*file_key.expose(), [0u8; 32]);
    }

    #[test]
    fn test_generate_file_key_consecutive_calls_produce_different_keys() {
        let first = generate_file_key();
        let second = generate_file_key();
        assert_ne!(first.expose(), second.expose());
    }
}
```

Notes:
- `random_bytes: [u8; 32]` is copied into `Box::new(random_bytes)` and then the stack local goes out of scope. The exposure window is a single statement — the same discipline `FileKey::from_bytes` (types/mod.rs line 13) already uses. The plan accepts this residual exposure because it is the shortest possible window and is consistent with Phase 1.1; a stricter pattern would require `SecretBox::<[u8; 32]>::init_with_mut` here, which is not worth the additional complexity for a CSPRNG read that produces its output into the closure-owned buffer anyway. (If the `security-reviewer` disagrees, switch to `init_with_mut` with `rng.fill_bytes(buffer.as_mut_slice())`; this is a drop-in local change.)
- `file_key.expose()` is `pub(crate)` and accessible from the sibling inline test module.

### Step 5.4 — Create `src-tauri/src/crypto/checksum.rs`

Create the file with this exact content:

```rust
//! BLAKE3 checksums over encrypted blobs and the `VerifiedBlob` type that
//! enforces check-before-decrypt at compile time.

use crate::crypto::error::CryptoError;
use crate::crypto::types::Blake3Hash;

/// Computes a BLAKE3 checksum over an encrypted blob.
///
/// The checksum is computed over **ciphertext**, not plaintext. This allows
/// integrity verification of a downloaded blob before the more expensive
/// XChaCha20-Poly1305 decryption runs.
pub fn compute_checksum(encrypted_blob: &[u8]) -> Blake3Hash {
    let hash = blake3::hash(encrypted_blob);
    Blake3Hash(*hash.as_bytes())
}

/// Verifies the BLAKE3 checksum of an encrypted blob and produces a
/// `VerifiedBlob` that `decrypt_chunk` accepts.
///
/// The `blob` is consumed so a single `Vec<u8>` allocation flows from the
/// download boundary through verification into the decryption step.
///
/// # Errors
/// Returns `CryptoError::ChecksumMismatch` if the computed checksum does not
/// match `expected`. The blob is dropped in that case.
pub fn verify_checksum(
    blob: Vec<u8>,
    expected: &Blake3Hash,
) -> Result<VerifiedBlob, CryptoError> {
    let computed = compute_checksum(&blob);
    if computed.0 == expected.0 {
        Ok(VerifiedBlob(blob))
    } else {
        Err(CryptoError::ChecksumMismatch)
    }
}

/// An encrypted blob whose BLAKE3 checksum has been verified.
///
/// Only constructible via `verify_checksum`. `decrypt_chunk` accepts only
/// `VerifiedBlob` as its first parameter, making it a compile error to skip
/// the checksum step.
#[derive(Debug)]
pub struct VerifiedBlob(Vec<u8>);

impl VerifiedBlob {
    /// Consumes the wrapper and returns the underlying blob bytes.
    ///
    /// `pub(crate)` — callable only from within the `crypto` module, so
    /// consumers outside the module cannot bypass `verify_checksum`.
    pub(crate) fn into_inner(self) -> Vec<u8> {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::{VerifiedBlob, compute_checksum, verify_checksum};
    use crate::crypto::error::CryptoError;
    use assert_matches::assert_matches;

    #[test]
    fn test_compute_checksum_determinism_for_identical_input() {
        let blob = vec![0xABu8; 256];
        let first = compute_checksum(&blob);
        let second = compute_checksum(&blob);
        assert_eq!(first.0, second.0);
    }

    #[test]
    fn test_compute_checksum_detects_corruption_from_single_byte_flip() {
        let mut blob = vec![0xCDu8; 128];
        let before = compute_checksum(&blob);

        blob[42] ^= 0x01;
        let after = compute_checksum(&blob);

        assert_ne!(before.0, after.0);
    }

    #[test]
    fn test_compute_checksum_empty_input_is_well_defined_and_stable() {
        let first = compute_checksum(&[]);
        let second = compute_checksum(&[]);
        assert_eq!(first.0, second.0);
    }

    #[test]
    fn test_verify_checksum_success_returns_verified_blob_with_original_bytes() {
        let blob = vec![1u8, 2, 3, 4, 5];
        let expected = compute_checksum(&blob);
        let verified = verify_checksum(blob.clone(), &expected)
            .expect("matching checksum must verify");
        assert_eq!(verified.into_inner(), blob);
    }

    #[test]
    fn test_verify_checksum_rejects_tampered_blob_with_checksum_mismatch() {
        let original = vec![0xAAu8; 64];
        let expected = compute_checksum(&original);

        let mut tampered = original.clone();
        tampered[10] ^= 0x01;

        let result = verify_checksum(tampered, &expected);
        assert_matches!(result, Err(CryptoError::ChecksumMismatch));
    }

    #[test]
    fn test_verify_checksum_rejects_wrong_expected_hash() {
        let blob = vec![0xFFu8; 32];
        let wrong_expected = compute_checksum(&vec![0x00u8; 32]);

        let result = verify_checksum(blob, &wrong_expected);
        assert_matches!(result, Err(CryptoError::ChecksumMismatch));
    }

    #[test]
    fn test_verified_blob_into_inner_returns_original_bytes() {
        let bytes = vec![7u8, 8, 9];
        let expected = compute_checksum(&bytes);
        let verified: VerifiedBlob = verify_checksum(bytes.clone(), &expected)
            .expect("must verify");
        assert_eq!(verified.into_inner(), bytes);
    }
}

#[cfg(test)]
mod proptests {
    use super::compute_checksum;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn prop_checksum_deterministic(
            blob in proptest::collection::vec(any::<u8>(), 0..=64 * 1024),
        ) {
            let first = compute_checksum(&blob);
            let second = compute_checksum(&blob);
            prop_assert_eq!(first.0, second.0);
        }
    }
}
```

Notes:
- `VerifiedBlob`'s tuple field is private; `into_inner` is `pub(crate)`. Outside the `crypto` module, the type is opaque and can only be produced by `verify_checksum`.
- `verify_checksum` uses plain `==` on `[u8; 32]` (Design Concern #3).
- `compute_checksum` calls `blake3::hash` on the input slice — no allocation of a copy, per sub-phase Implementation Notes line 60.

### Step 5.5 — Create `src-tauri/src/crypto/wrap_key.rs`

Create the file with this exact content:

```rust
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
/// # Panics
/// Panics only if the underlying AEAD call fails, which cannot happen for a
/// 32-byte plaintext with XChaCha20-Poly1305.
pub fn wrap_file_key(
    file_key: &FileKey,
    key_encryption_key: &KeyEncryptionKey,
) -> WrappedFileKey {
    let nonce_bytes = generate_nonce();
    let mut ciphertext: [u8; KEY_LEN] = *file_key.expose();

    let cipher = XChaCha20Poly1305::new(GenericArray::from_slice(key_encryption_key.expose()));
    let nonce = GenericArray::from_slice(&nonce_bytes);

    let tag = cipher
        .encrypt_in_place_detached(nonce, &[], ciphertext.as_mut_slice())
        .expect("XChaCha20-Poly1305 wrap is infallible for a 32-byte key");

    let mut wire = [0u8; WRAPPED_LEN];
    wire[..NONCE_LEN].copy_from_slice(&nonce_bytes);
    wire[NONCE_LEN..NONCE_LEN + KEY_LEN].copy_from_slice(&ciphertext);
    wire[NONCE_LEN + KEY_LEN..].copy_from_slice(tag.as_slice());

    WrappedFileKey(wire)
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
        decrypt_result =
            cipher.decrypt_in_place_detached(nonce, &[], buffer.as_mut_slice(), tag);
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
    use assert_matches::assert_matches;

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

        let wrapped = wrap_file_key(&original, &kek);
        let recovered = unwrap_file_key(&wrapped, &kek).expect("round trip must succeed");

        assert_eq!(*recovered.expose(), original_bytes);
    }

    #[test]
    fn test_wrap_file_key_wire_format_is_seventy_two_bytes() {
        let wrapped = wrap_file_key(&make_file_key(0xAA), &make_kek(0xBB));
        assert_eq!(wrapped.0.len(), WRAPPED_LEN);
        assert_eq!(WRAPPED_LEN, 72);
    }

    #[test]
    fn test_wrap_file_key_two_calls_produce_different_blobs() {
        let file_key = make_file_key(0xCD);
        let kek = make_kek(0xEF);

        let first = wrap_file_key(&file_key, &kek);
        let second = wrap_file_key(&file_key, &kek);

        assert_ne!(first.0, second.0, "random nonce must make wrapped blobs differ");
        assert_ne!(first.0[..24], second.0[..24], "nonce prefix must differ");
    }

    #[test]
    fn test_unwrap_file_key_wrong_kek_fails_with_decryption_failed() {
        let file_key = make_file_key(0x11);
        let wrapped = wrap_file_key(&file_key, &make_kek(0x22));

        let result = unwrap_file_key(&wrapped, &make_kek(0x33));

        assert_matches!(result, Err(CryptoError::DecryptionFailed));
    }

    #[test]
    fn test_unwrap_file_key_corrupted_nonce_fails_with_decryption_failed() {
        let file_key = make_file_key(0x11);
        let kek = make_kek(0x22);
        let mut wrapped = wrap_file_key(&file_key, &kek);

        wrapped.0[0] ^= 0x01;

        let result = unwrap_file_key(&wrapped, &kek);

        assert_matches!(result, Err(CryptoError::DecryptionFailed));
    }

    #[test]
    fn test_unwrap_file_key_corrupted_ciphertext_fails_with_decryption_failed() {
        let file_key = make_file_key(0x11);
        let kek = make_kek(0x22);
        let mut wrapped = wrap_file_key(&file_key, &kek);

        // Offset 24..56 is the 32-byte ciphertext region.
        wrapped.0[24 + 5] ^= 0x01;

        let result = unwrap_file_key(&wrapped, &kek);

        assert_matches!(result, Err(CryptoError::DecryptionFailed));
    }

    #[test]
    fn test_unwrap_file_key_corrupted_tag_fails_with_decryption_failed() {
        let file_key = make_file_key(0x11);
        let kek = make_kek(0x22);
        let mut wrapped = wrap_file_key(&file_key, &kek);

        let tag_index = wrapped.0.len() - 1;
        wrapped.0[tag_index] ^= 0x01;

        let result = unwrap_file_key(&wrapped, &kek);

        assert_matches!(result, Err(CryptoError::DecryptionFailed));
    }

    #[test]
    fn test_unwrap_file_key_all_zero_wrapped_blob_fails_with_decryption_failed() {
        let kek = make_kek(0x22);
        let wrapped = WrappedFileKey([0u8; 72]);

        let result = unwrap_file_key(&wrapped, &kek);

        assert_matches!(result, Err(CryptoError::DecryptionFailed));
    }
}
```

Notes:
- `wrap_file_key`'s local `ciphertext: [u8; 32]` holds a copy of the file-key bytes, which is then overwritten in place by `encrypt_in_place_detached`. After that call the local contains only ciphertext — no plaintext-key residue, no keystream bytes. The copy is unavoidable because `encrypt_in_place_detached` requires `&mut [u8]` and we must not mutate the caller's `FileKey`. No explicit `zeroize` is needed on this local (post-encrypt state is ciphertext, not key material).
- `unwrap_file_key` uses the `SecretBox::<[u8; 32]>::init_with_mut` pattern (Design Concern #4). `decrypt_result` is captured by unique mutable reference into the closure and assigned exactly once. On `Err`, `file_key_secret_box` is dropped at the end of the function and `SecretBox`'s `Drop` (ZeroizeOnDrop) clears the boxed buffer.
- The `test_unwrap_file_key_all_zero_wrapped_blob_fails_with_decryption_failed` test uses the `pub` tuple field on `WrappedFileKey` to construct an attacker-shaped blob; this is why the existing Phase 1.1 decision to expose the field is useful. Do not change that decision in this phase.
- `CryptoError::KeyUnwrapFailed` is *not* used here — per design.md line 445, authentication failures in unwrap flows map to `DecryptionFailed`, and `KeyUnwrapFailed` is reserved for future key-wrapping-adjacent call sites.

### Step 5.6 — Migrate `decrypt_chunk` signature to accept `VerifiedBlob`

Edit `C:\Users\chris\source\repos\arx-runa\src-tauri\src\crypto\decrypt_chunk.rs`.

**5.6.a — Replace the module doc comment** (currently lines 1–6):

```rust
//! XChaCha20-Poly1305 chunk decryption with mandatory AAD binding.
//!
//! `decrypt_chunk` consumes a `VerifiedBlob` — the only way to obtain one is
//! via `verify_checksum`, so the BLAKE3 integrity check is enforced at the
//! type level.
```

**5.6.b — Update the imports** (currently lines 8–13):

```rust
use crate::crypto::checksum::VerifiedBlob;
use crate::crypto::error::CryptoError;
use crate::crypto::types::{ChunkIndex, FileId, FileKey};
use chacha20poly1305::{
    AeadInPlace, KeyInit, XChaCha20Poly1305, aead::generic_array::GenericArray,
};
use zeroize::Zeroize;
```

**5.6.c — Replace the `decrypt_chunk` function body** (currently lines 19–61) with:

```rust
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
```

The `NONCE_LEN`, `TAG_LEN`, `MIN_BLOB_LEN` constants (existing lines 15–17) and `build_aad` helper (existing lines 63–68) remain unchanged.

**5.6.d — Update the test module**. Add the checksum imports at the top of `mod tests` (currently line 73):

```rust
use crate::crypto::checksum::{VerifiedBlob, compute_checksum, verify_checksum};
```

Add this helper function below the existing `make_file_key` / `make_file_id` / `encrypt` helpers:

```rust
    fn verified(blob: Vec<u8>) -> VerifiedBlob {
        let checksum = compute_checksum(&blob);
        verify_checksum(blob, &checksum).expect("self-consistent checksum must verify")
    }
```

Update every call to `decrypt_chunk(&blob, ...)` inside `mod tests` to `decrypt_chunk(verified(blob), ...)`. The affected tests (by current line number) are:

- `test_decrypt_chunk_wrong_file_id_fails_with_decryption_failed` (line 94)
- `test_decrypt_chunk_wrong_chunk_index_fails_with_decryption_failed` (line 104)
- `test_decrypt_chunk_wrong_key_fails_with_decryption_failed` (line 115)
- `test_decrypt_chunk_corrupted_ciphertext_fails_with_decryption_failed` (line 130)
- `test_decrypt_chunk_corrupted_tag_fails_with_decryption_failed` (line 144)
- `test_decrypt_chunk_truncated_blob_returns_invalid_blob_format` (line 158) — `verified(vec![0u8; 20])` works because BLAKE3 accepts any length; `decrypt_chunk` then returns `InvalidBlobFormat`
- `test_decrypt_chunk_blob_thirty_nine_bytes_returns_invalid_blob_format` (line 178) — same pattern with 39 bytes
- `test_decrypt_chunk_exactly_forty_bytes_empty_plaintext_round_trip_succeeds` (line 197)
- `test_decrypt_chunk_empty_blob_returns_invalid_blob_format` (line 211) — `verified(Vec::new())` is legal; BLAKE3 of empty input is a well-defined constant

Because the `verified` helper consumes `blob`, tests like `test_decrypt_chunk_corrupted_ciphertext_fails_with_decryption_failed` that currently mutate `blob` after encrypting must mutate **first**, then call `verified(blob)`. Both ordering choices are semantically equivalent because `verify_checksum` uses the checksum of the passed-in bytes directly (it is a self-consistency check, not a match against an independently-stored hash).

**5.6.e — Add one new end-to-end test** at the end of `mod tests`:

```rust
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
```

### Step 5.7 — Migrate `encrypt_chunk.rs` test and proptest callsites

Edit `C:\Users\chris\source\repos\arx-runa\src-tauri\src\crypto\encrypt_chunk.rs`. Production code (lines 1–55) is unchanged; only the two test modules change.

**5.7.a — `mod tests` (lines 56–121)**. Add to the imports:

```rust
    use crate::crypto::checksum::{VerifiedBlob, compute_checksum, verify_checksum};
```

Add the same `verified` helper used in Step 5.6.d under `mod tests` (sibling to `make_file_key`):

```rust
    fn verified(blob: Vec<u8>) -> VerifiedBlob {
        let checksum = compute_checksum(&blob);
        verify_checksum(blob, &checksum).expect("self-consistent checksum must verify")
    }
```

Update `test_encrypt_decrypt_round_trip_returns_original_plaintext` (current line 108). The final three lines become:

```rust
        let blob = encrypt_chunk(plaintext.clone(), &key, &file_id, chunk_index);
        let recovered =
            decrypt_chunk(verified(blob), &key, &file_id, chunk_index).expect("round trip must succeed");

        assert_eq!(recovered, plaintext);
```

No other test in `mod tests` calls `decrypt_chunk`.

**5.7.b — `mod proptests` (lines 123–163)**. This is a separate `mod` block with its own import scope. Add to the imports:

```rust
    use crate::crypto::checksum::{VerifiedBlob, compute_checksum, verify_checksum};
```

Add a `verified` helper inside `mod proptests`:

```rust
    fn verified(blob: Vec<u8>) -> VerifiedBlob {
        let checksum = compute_checksum(&blob);
        verify_checksum(blob, &checksum).expect("self-consistent checksum must verify")
    }
```

Update `prop_encrypt_decrypt_identity` (current lines 131–146). The relevant lines become:

```rust
            let blob = encrypt_chunk(plaintext.clone(), &key, &file_id, idx);
            let recovered = decrypt_chunk(verified(blob), &key, &file_id, idx)
                .expect("round trip must succeed");
            prop_assert_eq!(recovered, plaintext);
```

`prop_different_nonces_produce_different_blobs` (lines 148–161) does **not** call `decrypt_chunk` — no change.

### Step 5.8 — Register new modules and re-exports in `mod.rs`

Edit `C:\Users\chris\source\repos\arx-runa\src-tauri\src\crypto\mod.rs` to replace its current contents with:

```rust
//! Arx Runa crypto module.
//!
//! Cryptographic primitives: key derivation, chunk encryption, file key
//! management, BLAKE3 checksums.

pub mod checksum;
pub mod decrypt_chunk;
pub mod encrypt_chunk;
pub mod error;
pub mod generate_file_key;
pub mod hkdf;
pub mod nonce;
pub mod types;
pub mod wrap_key;

pub use checksum::{VerifiedBlob, compute_checksum, verify_checksum};
pub use decrypt_chunk::decrypt_chunk;
pub use encrypt_chunk::encrypt_chunk;
pub use error::CryptoError;
pub use generate_file_key::generate_file_key;
pub use hkdf::{VaultKeys, derive_vault_keys};
pub use nonce::generate_nonce;
pub use types::{
    Blake3Hash, ChunkIndex, FileId, FileKey, KeyEncryptionKey, ManifestKey, SqlcipherKey,
    WrappedFileKey,
};
pub use wrap_key::{unwrap_file_key, wrap_file_key};
```

Result — the crypto module's public surface after this step:
- **Types**: `Blake3Hash`, `ChunkIndex`, `CryptoError`, `FileId`, `FileKey`, `KeyEncryptionKey`, `ManifestKey`, `SqlcipherKey`, `VaultKeys`, `VerifiedBlob`, `WrappedFileKey`
- **Functions**: `compute_checksum`, `decrypt_chunk`, `derive_vault_keys`, `encrypt_chunk`, `generate_file_key`, `generate_nonce`, `unwrap_file_key`, `verify_checksum`, `wrap_file_key`

Recovery-slot APIs (`wrap_master_key_for_recovery`, `unwrap_master_key_from_recovery`, `RecoveryKey`, `WrappedMasterKey`) remain unimplemented; they are Phase 2 work (Concern #1).

### Step 5.9 — Run validation checkpoint

From `C:\Users\chris\source\repos\arx-runa\src-tauri\`:

```bash
cargo fmt
cargo clippy -- -D warnings
cargo test crypto::wrap_key
cargo test crypto::checksum
cargo test crypto::generate_file_key
cargo test crypto                                # full module regression
```

All commands must exit zero. If clippy flags anything, fix the underlying warning — do not add `#[allow(...)]`. If a test fails, do not skip it; trace the failure back to the exact step in this plan and patch that step before retrying.

## 6. Security Implications

### a. Expected sensitive path set

Files under `src-tauri/src/crypto/`, `src-tauri/src/auth/`, or `src-tauri/src/storage/` that this plan anticipates touching:

- `src-tauri/src/crypto/types/mod.rs` — **edit** to add `FileKey::from_secret_box` constructor (Step 5.2).
- `src-tauri/src/crypto/mod.rs` — **edit** module declarations and re-exports (Step 5.8).
- `src-tauri/src/crypto/decrypt_chunk.rs` — **edit** signature, imports, tests (Step 5.6).
- `src-tauri/src/crypto/encrypt_chunk.rs` — **edit** test and proptest callsites only; production code unchanged (Step 5.7).
- `src-tauri/src/crypto/checksum.rs` — **create** (Step 5.4).
- `src-tauri/src/crypto/generate_file_key.rs` — **create** (Step 5.3).
- `src-tauri/src/crypto/wrap_key.rs` — **create** (Step 5.5).

No files under `src-tauri/src/auth/` or `src-tauri/src/storage/` are touched.

### b. Invoke `security-reviewer` agent? **YES**

The sub-phase roadmap's Security Review Checkpoints (sub-phases/roadmap.md line 91) explicitly requires a reviewer pass on Phase 1.3, and sub-phase 1.3 line 67 confirms "Required". This plan's independent assessment concurs: the new code introduces a key-wrapping primitive (direct AEAD on secret key material), a `VerifiedBlob` type that gates every downstream decrypt call, and the end-to-end check-before-decrypt flow. All three are in scope for adversarial review, and the intermediate `SecretBox` zeroization pattern is the first use of `init_with_mut` with a mutable outer binding in this codebase — worth a second set of eyes.

### c. What the reviewer should check

1. **`wrap_file_key` nonce freshness**: `generate_nonce()` is called exactly once per invocation; the returned 24 bytes are used without reuse, caching, or truncation. No code path reuses a nonce across two wrap calls.
2. **`wrap_file_key` empty-AAD correctness**: the literal `&[]` is passed as AAD. This matches the design's rationale (wrapped blob is scoped by `key_encryption_key` and the local manifest domain). Recovery-slot wrapping is **not** in this phase; grep for any path that routes a `MasterKey` through `wrap_file_key` — there should be none.
3. **`unwrap_file_key` tag verification ordering**: the `decrypt_in_place_detached` `Result` is captured in `decrypt_result` before `FileKey::from_secret_box` is called. On `Err`, `FileKey` is never constructed from partial-keystream bytes.
4. **`unwrap_file_key` intermediate-buffer zeroization**: `SecretBox::<[u8; 32]>::init_with_mut` is used, so decryption runs inside a `SecretBox`-owned heap buffer; `SecretBox`'s `Drop` (ZeroizeOnDrop) covers both success and failure paths. Confirm no stack-local `[u8; 32]` holds the decrypted bytes at any point.
5. **`generate_file_key` RNG source**: `rand::rng().random::<[u8; 32]>()` (CSPRNG) is used and the raw `[u8; 32]` is immediately moved into `SecretBox::new(Box::new(...))` without being cloned, logged, or passed through a `Vec`. If the reviewer prefers, Step 5.3 can be switched to `SecretBox::<[u8; 32]>::init_with_mut(|buf| rand::rng().fill_bytes(buf.as_mut_slice()))` to eliminate the stack-local window — this is a drop-in local change.
6. **`compute_checksum` input scope**: grep all callsites inside `src-tauri/src/crypto/`. The function must only ever be called on *ciphertext* blobs. Flag any path where plaintext bytes could reach it.
7. **`VerifiedBlob` opacity**: the inner `Vec<u8>` tuple field is **not** `pub`. `into_inner` is `pub(crate)`. No `impl Deref<Target = [u8]>`, `impl AsRef<[u8]>`, `impl From<Vec<u8>>`, `#[derive(Clone)]`, or similar back-doors exist.
8. **`decrypt_chunk` consumes `VerifiedBlob`**: the parameter type is `VerifiedBlob` (not `&VerifiedBlob`), so callers cannot decrypt the same blob twice from the same binding. The failure path still returns the `Vec<u8>`-typed buffer, not the `VerifiedBlob`.
9. **No plaintext copies**: `encrypt_in_place_detached` / `decrypt_in_place_detached` are used throughout. Spot-check the new `wrap_file_key` call.
10. **Buffer state on decrypt failure in `decrypt_chunk`**: the existing `buffer.zeroize()` path from Phase 1.2 is preserved after the signature migration (Step 5.6.c).
11. **`KeyUnwrapFailed` is unused**: confirm `CryptoError::KeyUnwrapFailed` is not produced anywhere in this phase (authentication failures during unwrap map to `DecryptionFailed`, per design.md line 445). The variant remains defined for future use.

## 7. Execution and testing strategy

**Implementation agent**: Invoke `rust-implementer` (Required). Rationale: this plan creates three new Rust modules, edits two more, and must satisfy the project's Rust rules (`thiserror` error handling, `ZeroizeOnDrop` discipline, inline `#[cfg(test)]` modules, `pub(crate)` scoping, doc comments on every `pub` item, no inline `//` narrative comments). The `rust-implementer` agent is specialized for these conventions. **Fallback**: if `rust-implementer` is unavailable or fails repeatedly, mark the plan `blocked` — no manual fallback implementation.

**Test scope**:
- [x] Basic unit tests (rust-implementer writes inline, per Steps 5.3–5.5)
- [x] Adversarial tests: wrong KEK, tampered nonce/ciphertext/tag bytes inside `WrappedFileKey`, all-zero wrapped blob, tampered checksum, wrong expected hash (per Steps 5.4–5.5)
- [x] Property-based tests: `prop_checksum_deterministic` (per Step 5.4)
- [ ] Integration tests — none at this phase (Phase 1 is a pure library surface with no I/O)
- [x] Boundary cases: empty blob (`compute_checksum`), empty plaintext (encrypt/decrypt round trip, already present in Phase 1.2), 39-byte and 40-byte decrypt blobs, consecutive CSPRNG calls

**Validation checkpoint** (mirrors sub-phase 1.3 lines 24–32):

```bash
cargo test crypto::wrap_key
cargo test crypto::checksum
cargo test crypto::generate_file_key
```

All must pass, plus `cargo test crypto` (full-module regression) and `cargo clippy -- -D warnings`.

**Test acceptance criteria** (mirrors sub-phase 1.3 lines 33–43, extended for Concern #5):
- `unwrap_file_key(wrap_file_key(key, kek), kek)` returns the original key bytes.
- Wrong `KeyEncryptionKey` → `CryptoError::DecryptionFailed`.
- `compute_checksum` is deterministic: identical blobs produce identical `Blake3Hash`.
- Single-byte mutation of an encrypted blob produces a different `Blake3Hash`.
- `verify_checksum` returns `Err(CryptoError::ChecksumMismatch)` on tamper and on wrong expected hash.
- `VerifiedBlob` is only constructible via `verify_checksum`; `decrypt_chunk` cannot be called with a raw `Vec<u8>` (this is enforced at compile time by Step 5.8's type surface, verified by the successful compilation of Steps 5.6 and 5.7).
- `generate_file_key` does not return an all-zero key.
- Two consecutive calls to `generate_file_key` produce distinct keys.
- Two consecutive calls to `wrap_file_key` with the same inputs produce distinct wrapped blobs (nonce randomness).
- Corrupted nonce / corrupted ciphertext / corrupted tag inside a `WrappedFileKey` all map to `DecryptionFailed`.
- `WrappedFileKey` wire format is exactly 72 bytes.
- All Phase 1.2 regression tests in `encrypt_chunk.rs` and `decrypt_chunk.rs` pass after the `VerifiedBlob` migration.
- Every `CryptoError` variant used in this phase (`DecryptionFailed`, `ChecksumMismatch`) has at least one test that triggers it (per `.claude/rules/rust.md` testing rule).
- `cargo clippy -- -D warnings` is clean.

**Invoke test-writer agent? NO.** Rationale: the sub-phase's test list plus the adversarial additions from Design Concern #5 are narrow and fully enumerated in Steps 5.3–5.7. `rust-implementer` writes tests inline alongside the production code it creates, matching the pattern Phase 1.1 and Phase 1.2 plans used (both had `test-agent-required: false`). The adversarial dimensions (nonce reuse, tamper of each byte region, checksum mismatch, wrong expected hash, round-trip identity) are exhaustively listed here; a separate adversarial sweep by `test-writer` would be duplicated effort. If the post-implementation `security-reviewer` pass surfaces a missed edge case, re-enter via a targeted patch.

## 8. Documentation impact

1. **Required on implementation** (Design Concern #1 resolution):
   - `docs/architecture/designs/cryptographic-primitives/sub-phases/1.3-key-wrapping-and-checksums.md`, "Completion" section (lines 75–82): amend to state that Phase 1.3 closes out Phase 1's vault-key, chunk-AEAD, per-file-key, and integrity-checksum deliverables, while recovery-slot wrapping (`wrap_master_key_for_recovery`, `unwrap_master_key_from_recovery`, `RecoveryKey`, `WrappedMasterKey`) is owned by Phase 2 because it depends on the `MasterKey` type introduced by the authentication design.
   - `docs/roadmap.md` Phase 1 summary (lines 43–49): add a one-line note that recovery-slot wrapping is Phase 2 work, so that a reader comparing the Contract Surface in `cryptographic-primitives/design.md` to the Phase 1 deliverables understands the phase boundary.

2. **Roadmap checkbox update** (standard Phase 1 completion):
   - After Step 5.9 passes and the `security-reviewer` signs off, update `docs/roadmap.md` to mark Phase 1 complete (sub-phase 1.3 line 80 instructs this).

3. **Diagram freshness check**:
   - `docs/architecture/designs/cryptographic-primitives/diagrams/key-derivation-tree.md` — sub-phase 1.3 line 81 instructs to update only if implementation diverged from the design. This plan does not diverge; no edit expected. The implementer should visually confirm the diagram still matches after Step 5.9 and note in the PR if anything looks stale.

4. **No new contract surface is introduced** beyond what the parent design already specifies. `generate_file_key`, `wrap_file_key`, `unwrap_file_key`, `compute_checksum`, `verify_checksum`, and `VerifiedBlob` are all already listed as canonical in `design.md` Contract Surface (lines 21, 28). No new `## Contract Surface` entries are added; no new rows are added to `docs/architecture/design-invariants.md`.

### Documentation updates applied

- Updated `docs/architecture/designs/cryptographic-primitives/sub-phases/1.3-key-wrapping-and-checksums.md` Completion section to explicitly defer recovery-slot wrapping APIs to Phase 2 because they depend on `MasterKey`.
- Updated `docs/roadmap.md` to mark **Phase 1 — Cryptographic Primitives** as `Complete`.
- Added a Phase 1 note in `docs/roadmap.md` clarifying `wrap_master_key_for_recovery` / `unwrap_master_key_from_recovery` are Phase 2 work.
- Reviewed `docs/architecture/designs/cryptographic-primitives/diagrams/key-derivation-tree.md`; no divergence found, so no diagram edit was required.

## 9. Handoff notes for implementer

You are working in `C:\Users\chris\source\repos\arx-runa`, Rust edition 2024, inside the `src-tauri/` crate. **Do not re-read the sub-phase** — this plan is self-contained and quotes every trait signature, error variant, and wire-format byte offset you need. Execute Steps 5.1 → 5.9 in order; each step targets exactly the files listed. The most error-prone step is 5.6 (migrating `decrypt_chunk`'s signature to `VerifiedBlob`), because every existing Phase 1.2 test callsite must switch from `decrypt_chunk(&blob, ...)` to `decrypt_chunk(verified(blob), ...)` — do not leave a single stale callsite or the crate will not compile. Step 5.7 is the same migration inside `encrypt_chunk.rs`, which has *two* separate `mod` blocks (`mod tests` and `mod proptests`) each with its own imports and its own `verified` helper. Platform note: this crate targets Windows, macOS, and Linux; nothing in Phase 1.3 is platform-specific, so there are no `#[cfg(target_os = ...)]` branches to worry about and no path-separator hazards. After Step 5.9 passes, hand the touched files off to the `security-reviewer` agent per Section 6.c's eleven-item checklist before marking the plan implemented.

## Implementation Log

- **Date**: 2026-04-13T03:20:43Z
- **Branch**: development

### Agent evidence

| Approach step | Agent | Agent ID | Outcome |
| --- | --- | --- | --- |
| 5.1 | rust-implementer | `phase-1-3-impl` | Confirmed required dependency pins already present; no dependency edits. |
| 5.2 | rust-implementer | `phase-1-3-impl` | Added `FileKey::from_secret_box` in `src-tauri/src/crypto/types/mod.rs`. |
| 5.3 | rust-implementer | `phase-1-3-impl` | Created `src-tauri/src/crypto/generate_file_key.rs` with generation API and tests. |
| 5.4 | rust-implementer | `phase-1-3-impl` | Created `src-tauri/src/crypto/checksum.rs` with `compute_checksum`, `verify_checksum`, `VerifiedBlob`, tests, and proptest. |
| 5.5 | rust-implementer | `phase-1-3-impl` | Created `src-tauri/src/crypto/wrap_key.rs` with wrap/unwrap APIs and adversarial tests. |
| 5.5 (follow-up fix) | rust-implementer | `phase-1-3-wrapkey-fix` | Replaced `assert_matches!` usages on `Result<FileKey, _>` with `matches!` assertions to avoid requiring `Debug` on `FileKey`. |
| 5.6 | rust-implementer | `phase-1-3-impl` | Migrated `decrypt_chunk` to consume `VerifiedBlob`; updated docs/imports/tests and added verified-blob end-to-end test. |
| 5.7 | rust-implementer | `phase-1-3-impl` | Updated `encrypt_chunk.rs` test/proptest callsites to verify checksum before decrypt. |
| 5.8 | rust-implementer | `phase-1-3-impl` | Registered/re-exported `checksum`, `generate_file_key`, and `wrap_key` in `src-tauri/src/crypto/mod.rs`. |
| Security review (Section 6.c) | security-reviewer | `phase-1-3-security` | No CRITICAL or WARNING findings; verdict: safe to proceed. |

### Files changed

- `src-tauri/src/crypto/types/mod.rs` (modified)
- `src-tauri/src/crypto/generate_file_key.rs` (created)
- `src-tauri/src/crypto/checksum.rs` (created)
- `src-tauri/src/crypto/wrap_key.rs` (created)
- `src-tauri/src/crypto/decrypt_chunk.rs` (modified)
- `src-tauri/src/crypto/encrypt_chunk.rs` (modified)
- `src-tauri/src/crypto/mod.rs` (modified)

### Test results

- `cargo test crypto::wrap_key`: passed (`8 passed; 0 failed`).
- `cargo test crypto::checksum`: passed (`8 passed; 0 failed`).
- `cargo test crypto::generate_file_key`: passed (`2 passed; 0 failed`).
- `cargo test crypto`: passed (`48 passed; 0 failed`).

### Clippy results

- `cargo clippy --workspace -- -D warnings`: clean (no warnings).

### Security review

- `security-reviewer` run completed on the expected sensitive file set.
- Findings: CRITICAL `0`, WARNING `0`, NOTE-only confirmations.

### Deviations from plan

- The initial `wrap_key` tests used `assert_matches!` for `Result<FileKey, CryptoError>` error assertions, which required `Debug` on `FileKey`. This was adjusted to `assert!(matches!(...))` to preserve secret-key type semantics without adding `Debug` to key newtypes.
- `rust-implementer` agent runs in this environment could not execute `cargo` commands directly, so compile/lint/test validation was executed immediately afterward by the invoking agent with all required commands passing.

### Documentation flagged

1. **Required on implementation** (Design Concern #1 resolution):
   - `docs/architecture/designs/cryptographic-primitives/sub-phases/1.3-key-wrapping-and-checksums.md`, "Completion" section (lines 75–82): amend to state that Phase 1.3 closes out Phase 1's vault-key, chunk-AEAD, per-file-key, and integrity-checksum deliverables, while recovery-slot wrapping (`wrap_master_key_for_recovery`, `unwrap_master_key_from_recovery`, `RecoveryKey`, `WrappedMasterKey`) is owned by Phase 2 because it depends on the `MasterKey` type introduced by the authentication design.
   - `docs/roadmap.md` Phase 1 summary (lines 43–49): add a one-line note that recovery-slot wrapping is Phase 2 work, so that a reader comparing the Contract Surface in `cryptographic-primitives/design.md` to the Phase 1 deliverables understands the phase boundary.

2. **Roadmap checkbox update** (standard Phase 1 completion):
   - After Step 5.9 passes and the `security-reviewer` signs off, update `docs/roadmap.md` to mark Phase 1 complete (sub-phase 1.3 line 80 instructs this).

3. **Diagram freshness check**:
   - `docs/architecture/designs/cryptographic-primitives/diagrams/key-derivation-tree.md` — sub-phase 1.3 line 81 instructs to update only if implementation diverged from the design. This plan does not diverge; no edit expected. The implementer should visually confirm the diagram still matches after Step 5.9 and note in the PR if anything looks stale.

4. **No new contract surface is introduced** beyond what the parent design already specifies. `generate_file_key`, `wrap_file_key`, `unwrap_file_key`, `compute_checksum`, `verify_checksum`, and `VerifiedBlob` are all already listed as canonical in `design.md` Contract Surface (lines 21, 28). No new `## Contract Surface` entries are added; no new rows are added to `docs/architecture/design-invariants.md`.
