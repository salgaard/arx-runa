---
timestamp: "2026-04-01T15:50:33+02:00"
type: decision
report-sections:
  - method
  - analysis
tags:
  - cryptography
  - design-review
  - HKDF
  - XChaCha20-Poly1305
source: manual
commit: "28bd43c"
---

## Cryptographic Primitives Design Review — Cross-Design Consistency

## Context

The Arx Runa cryptographic primitives design document underwent a structured review to verify its alignment with related design documents (authentication, chunking, cloud sync) and adherence to project security invariants defined in CLAUDE.md. The review was conducted interactively, section by section, following the `/interactive-design review` workflow.

## Substance

The review validated the core cryptographic architecture:

- **HKDF-SHA256 key derivation** from Argon2id output produces three purpose-specific keys using distinct `info` strings, following RFC 5869
- **XChaCha20-Poly1305 AEAD** with mandatory AAD binding (`file_id || chunk_index`) prevents chunk reordering and cross-file substitution attacks
- **Per-file random keys** wrapped with `key_encryption_key` enable future file sharing without vault-wide re-encryption
- **BLAKE3 checksums** over encrypted blobs enable verify-before-decrypt, avoiding decryption oracle attacks
- **Zeroization** of all key types via `ZeroizeOnDrop` + `Secret<T>`, with test verification using unsafe pointer inspection

Two findings were identified and resolved:

1. **Extensibility gap**: The HKDF section lacked guidance on adding future derived keys. An extensibility note was added, referencing the `derive-hkdf-key` skill as the canonical procedure.

2. **Cross-design inconsistency**: The crypto design specifies `chunk_index` as u32 (4 bytes) in AAD construction, while the chunking design specified u64 (8 bytes). Analysis determined u32 is sufficient (max 2^32 chunks × 4 MiB = 16 PiB per file). The chunking design was updated to use u32.

<!-- CITE: RFC 5869 — HMAC-based Extract-and-Expand Key Derivation Function (HKDF) -->

## Alternatives considered

For the chunk_index size inconsistency:

| Option | Pros | Cons |
|--------|------|------|
| u32 (4 bytes) | Smaller AAD, sufficient for 16 PiB files | Theoretical limit, unlikely to matter |
| u64 (8 bytes) | Unlimited file size | Unnecessary bytes in every chunk's AAD |

u32 was selected because 16 PiB exceeds any practical file size, and smaller AAD marginally reduces overhead.

For the extensibility note:

| Option | Pros | Cons |
|--------|------|------|
| Add note with skill reference | Future implementers know the safe path | Minor documentation overhead |
| Keep implicit | Minimal change | Risk of ad-hoc key additions without proper isolation |

The note was added because HKDF key derivation correctness depends on using distinct `info` values, and the skill encodes this requirement.

## Implications

- The crypto primitives design is validated for Phase 1 implementation
- Cross-design consistency is established for AAD construction
- Future derived keys must follow the `derive-hkdf-key` skill workflow to ensure cryptographic separation
- The streaming invariant is correctly owned by the chunking design (caller), not the crypto design (callee) — proper layering confirmed

## References

<!-- CITE: RFC 5869 — HMAC-based Extract-and-Expand Key Derivation Function (HKDF) -->
<!-- CITE: Bernstein, D.J. — ChaCha, a variant of Salsa20 -->
