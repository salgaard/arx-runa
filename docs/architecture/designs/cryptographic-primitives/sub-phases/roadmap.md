# Cryptographic Primitives — Sub-Phase Roadmap

**Parent design**: [`design.md`](../design.md)  
**Contract anchor**: [`design.md#contract-surface`](../design.md#contract-surface) is canonical for interface/data/invariant/dependency contracts; roadmap and sub-phases should reference it instead of duplicating full contract payloads.  
**Created**: 2026-04-04  
**Status**: Draft  
**Implementation order**: 1.1 → 1.2 → 1.3 (strict dependencies)

---

## Overview

This sub-phase roadmap decomposes the cryptographic primitives design (484 lines) into 3 independently testable implementation units, enabling incremental validation of the crypto layer before any other module depends on it.

**Total sub-phases**: 3 (Phases 1.1 through 1.3)

**Rationale for decomposition**:
-  **Size**: Exceeds ~100-150 lines (474 lines total)
-  **Trait boundaries**: Key types, HKDF derivation, and nonce generation are logically separable from AEAD operations and from key-wrapping concerns
-  **Error surface**: Defines 4 distinct `CryptoError` variants (`DecryptionFailed`, `InvalidBlobFormat`, `KeyUnwrapFailed`, `ChecksumMismatch`) each requiring independent test coverage
-  **Multi-step flows**: Key derivation → chunk encryption → key wrapping forms a strict dependency chain; each step is independently testable

**Implementation strategy**: Establish key types and derivation primitives first (1.1), then build AEAD encrypt/decrypt on top of those types (1.2), then add key-wrapping and BLAKE3 checksums as the final layer (1.3).

---

## Dependency Graph

```
1.1 (Key types + HKDF + Nonce)
 ↓
1.2 (AEAD encrypt/decrypt)
 ↓
1.3 (Key wrapping + BLAKE3)
```

**Legend**:
- `→` strict dependency (must complete predecessor before starting)

---

## Sub-Phases

1. **[Phase 1.1: Key Types, HKDF Derivation, and Nonce Generation](1.1-key-types-and-derivation.md)**
   - All key and domain newtypes with `ZeroizeOnDrop` + `SecretBox<[u8; 32]>`
   - `CryptoError` enum
   - `derive_vault_keys` via HKDF-SHA256
   - `generate_nonce` via CSPRNG
   - **Estimated**: ~200 lines production code, ~120 lines tests

2. **[Phase 1.2: AEAD Encrypt/Decrypt with Wire Format](1.2-aead-encrypt-decrypt.md)**
   - `encrypt_chunk` and `decrypt_chunk` with mandatory AAD binding
   - Wire format `[24-byte nonce | ciphertext | 16-byte tag]`
   - Adversarial test coverage for all failure modes
   - **Estimated**: ~120 lines production code, ~150 lines tests

3. **[Phase 1.3: Key Wrapping, BLAKE3 Checksums, and File Key Generation](1.3-key-wrapping-and-checksums.md)**
   - `generate_file_key` via CSPRNG
   - `wrap_file_key` / `unwrap_file_key` with 72-byte wire format
   - `compute_checksum` over encrypted blobs
   - **Estimated**: ~80 lines production code, ~80 lines tests

---

## Testing Strategy

### Per-Sub-Phase Testing
Each sub-phase includes its own test suite. Tests must pass before proceeding to the next sub-phase.

**Test types**:
- **Unit tests**: Core functionality in isolation (key construction, derivation correctness, nonce generation)
- **Adversarial tests**: Wrong key, wrong `file_id`, wrong `chunk_index`, truncated blob, corrupted tag — all must return `Err`, never panic
- **Property-based tests**: Use `proptest` for round-trip identity and nonce uniqueness (Phases 1.2 and 1.3)
- **Zeroize tests**: Verify `ZeroizeOnDrop` clears memory on drop (Phase 1.1)

### Regression Testing
After completing each sub-phase, run:
```bash
cargo test crypto         # All crypto module tests must pass
cargo clippy -- -D warnings  # No new warnings
```

This ensures new code does not break earlier sub-phases.

---

## Security Review Checkpoints

- **Phase 1.1**: Requires `security-reviewer` agent review (HKDF key separation, info-string uniqueness, zeroization of derived keys)
- **Phase 1.2**: Requires `security-reviewer` agent review (AAD binding correctness, nonce handling, wire format parsing)
- **Phase 1.3**: Requires `security-reviewer` agent review (key-wrapping correctness, zeroize verification on unwrapped key)

---

## Notes

- **`MasterKey` not defined here**: Phase 1 receives `MasterKey` as input; its definition and derivation (Argon2id) belong to Phase 2 (authentication). Phase 1.1 defines only the HKDF consumer interface — accept a `&[u8; 32]` slice internally until Phase 2 provides the type, then align.
- **No async**: All crypto operations are synchronous. No `async_trait` required.
- **`proptest` dependency**: Add to `[dev-dependencies]` in `Cargo.toml` before Phase 1.2; it is also needed in Phase 1.3.
- **BLAKE3 vs integrity in AEAD**: BLAKE3 checksums in Phase 1.3 provide pre-decryption integrity verification over downloaded cloud blobs — they are not a replacement for the Poly1305 tag, which remains the authentication mechanism inside `decrypt_chunk`.

---

## References

- **Parent design**: `docs/architecture/designs/cryptographic-primitives/design.md`
- **Roadmap entry**: `docs/roadmap.md` Phase 1
