# Arx Runa: Cryptographic Primitives — Critical Review

> **Document type**: Exploration / feasibility research
> **Status**: Concluded
> **Last updated**: 2026-04-07

Critical review of the existing cryptographic primitives design against academic literature, production systems, and known alternatives. Each design decision is re-examined for correctness, completeness, and missed opportunities.

For the canonical design see `docs/architecture/designs/cryptographic-primitives/design.md`.
For broader context on the "broken cloud storage ecosystem" failures that validate the AAD choices here, see [Sources](#sources).

---

## Table of Contents

1. [The Problem](#the-problem)
2. [Cipher Selection](#cipher-selection)
3. [Nonce Strategy](#nonce-strategy)
4. [Key Derivation](#key-derivation)
5. [Per-File Key Model](#per-file-key-model)
6. [AAD Design](#aad-design)
7. [BLAKE3 Outer Integrity](#blake3-outer-integrity)
8. [Implementation Correctness](#implementation-correctness)
9. [Recommendation](#recommendation)
10. [Decisions](#decisions)
11. [Open Questions](#open-questions)
12. [Sources](#sources)

---

## The Problem

The cryptographic primitives design was written before implementation. This document re-examines each design choice against the broader literature — cipher standards, nonce management approaches, key derivation alternatives, and common mistakes found in production encrypted storage systems — to find gaps, confirm correct choices, and identify any changes worth making before Phase 1 implementation begins.

Six findings were identified. Two were compile-breaking bugs, one was a design gap, two were improvements, and one was a documentation note.

---

## Cipher Selection

### What the design chose

XChaCha20-Poly1305 exclusively. AES-GCM and ChaCha20-Poly1305 (96-bit nonce variant) were explicitly rejected.

### Alternatives evaluated

| Cipher | Nonce size | Nonce strategy | Standardised | AES-NI benefit | Notes |
|--------|-----------|----------------|-------------|----------------|-------|
| **XChaCha20-Poly1305** | 192-bit | Random (safe) | libsodium, widely adopted | No (software stream cipher) | **Current design** |
| ChaCha20-Poly1305 | 96-bit | Sequential (needed) | RFC 8439 | No | Nonces too short for random generation |
| AES-GCM | 96-bit | Sequential (needed) | NIST SP 800-38D, RFC 5116 | Yes (3–10× faster with AES-NI) | Catastrophic failure on nonce reuse |
| AES-GCM-SIV | 96-bit | Random (safe) | RFC 8452 | Yes | Misuse-resistant but slower; limited Rust support |
| AEGIS-256 | 256-bit | Random (safe) | IETF CFRG draft-18 | Yes (~2× faster than AES-GCM) | Still a draft; Rust crate unaudited |

### Why XChaCha20-Poly1305 is correct for Arx Runa

**Safety margin with random nonces.** The extended 192-bit nonce was specifically designed to make random nonce generation safe. With 96-bit nonces (AES-GCM, ChaCha20-Poly1305), the birthday bound becomes a concern at approximately 2^32 encryptions under the same key — roughly 4 billion chunks. This is reachable in a long-lived large vault. With 192-bit nonces, the collision probability is negligible even at 2^64 encryptions. (Soatok 2021; see Sources.)

**No AES-NI requirement.** XChaCha20 is a software stream cipher — it performs consistently across all hardware, including older processors and future RISC-V or WASM targets, without requiring AES hardware acceleration. AES-GCM degrades 3–10× on hardware without AES-NI instructions.

**Multi-user security proof.** A 2023 IACR paper formally proved that ChaCha20-Poly1305 (and by extension XChaCha20-Poly1305) maintains security even when many users share the same cipher with independent keys — a realistic model for a cloud storage system where the same encryption code is deployed by many users. AES-GCM's security proof does not extend to this multi-user setting as cleanly.

**Ecosystem and auditability.** The `chacha20poly1305` crate is part of the RustCrypto suite, which has undergone multiple independent security audits. It is the cipher used by Wireguard, TLS 1.3 (for non-AES deployments), and libsodium. AEGIS-256, while faster, is still an IETF draft (revision 18 as of 2026) and lacks a Rust audit.

### AES-GCM-SIV: why it was considered and rejected

AES-GCM-SIV (RFC 8452) provides nonce-misuse resistance: encrypting two messages with the same key and nonce reveals only whether the messages are equal, not their content. This sounds attractive, but:

- It is approximately 70% the speed of AES-GCM on the encrypt path
- It relies on a MAC-then-Encrypt construction, which is less analysed than Encrypt-then-MAC
- The `aes-gcm-siv` Rust crate has limited adoption compared to `chacha20poly1305`
- Misuse resistance is not needed here — random 192-bit nonces make misuse a non-event

Soatok's cryptography guidelines explicitly recommend XChaCha20-Poly1305 or AEGIS-256 over AES-GCM-SIV for new systems concerned about nonce safety.

### AEGIS-256: future upgrade path

AEGIS-256 is a strong upgrade candidate once it reaches RFC status. It is AES-based (benefits from AES-NI for ~2× throughput vs XChaCha20-Poly1305), uses a 256-bit nonce safe for random generation, and has one unique property: the key can be erased from memory before any data is processed, since the key schedule is folded into initialisation. This is directly relevant to Arx Runa's memory-protection goals.

The wire format and API would be identical to the current design — nonce length and tag length are both compatible. The upgrade would require only a crate swap and a format version bump.

**Verdict: XChaCha20-Poly1305 is correct. No change.**

---

## Nonce Strategy

### What the design chose

Random 192-bit nonces via CSPRNG (`rand::rng().random::<[u8; 24]>()`). Sequential and counter-based nonces were explicitly rejected.

### Why random nonces are the right choice

The two main alternatives to random nonces are:

**Counter/sequential nonces**: require persistent state across process restarts. If the counter file is lost, restored from backup, or shared between two processes (e.g., after a vault restore), nonce reuse becomes possible. For XChaCha20-Poly1305, nonce reuse with the same key leaks the XOR of two plaintexts — catastrophic for confidentiality. For a personal backup vault that may be restored onto a new device, this failure mode is realistic.

**Deterministic/misuse-resistant nonces (AES-GCM-SIV)**: as discussed in the cipher section, the misuse-resistance of AES-GCM-SIV doesn't justify its trade-offs for Arx Runa's use case.

### Birthday bound analysis

The birthday bound for nonce collision is 2^(n/2) for an n-bit nonce.

| Nonce size | Collision at | Practical limit for Arx Runa |
|-----------|-------------|------------------------------|
| 96-bit (AES-GCM) | ~2^48 = 281 trillion | Reachable at scale |
| 192-bit (XChaCha20) | ~2^96 = 79 octillion | Never reachable in practice |

For personal file storage with 4 MiB chunks: a 1 TB vault contains ~256,000 chunks. At 2^96 the collision probability is negligible for any realistic usage lifetime.

Additionally, Arx Runa uses **per-file keys** — even if a nonce collision occurred within a file's key space, the impact would be limited to that single file. The per-file key model further reduces the already-negligible risk.

**Verdict: Random 192-bit nonces via CSPRNG are correct. No change.**

---

## Key Derivation

### What the design chose (original)

HKDF-SHA256 (RFC 5869) with an empty salt, deriving three keys from `master_key`: `key_encryption_key`, `sqlcipher_key`, `manifest_key`.

### Alternative: BLAKE3 key derivation mode

BLAKE3 has a built-in key derivation mode that takes a context string and key material, making it a direct alternative to HKDF for high-entropy IKM. It is faster (~3–5× vs HKDF-SHA256 for the same output), also provides domain separation via context strings, and is already a dependency for checksums.

| Property | HKDF-SHA256 | BLAKE3 KDF mode |
|----------|-------------|-----------------|
| Standard | RFC 5869 (IETF) | C2SP community spec (not IETF RFC) |
| Speed | Baseline | ~3–5× faster |
| Security margin | SHA-256 (128-bit) | BLAKE3 (claimed 128-bit, lower analysed margin) |
| Academic analysis | Extensive (TLS, SSH, Signal) | Limited compared to SHA-2 family |
| Rust crate | `hkdf` (RustCrypto, audited) | `blake3` (audited) |
| Bachelor report justifiability | Easy — RFC-backed | Harder — not yet IETF |

For a security-critical application, HKDF-SHA256 is the better choice: it is formally specified in an IETF RFC, has been analysed extensively in the context of TLS 1.3 and Signal Protocol, and its security properties are well-understood by security reviewers. BLAKE3's security margin for key derivation is lower than SHA-256's and the algorithm has had less cryptanalysis time.

### The empty salt problem (finding, now fixed)

RFC 5869 section 3.1 states: "the use of salt adds significantly to the strength of HKDF" and recommends a fixed salt even when IKM has high entropy, to act as a domain separator. The original design used an empty salt, which HKDF internally replaces with a zero-filled string of hash length — a valid but not recommended choice.

The fix: use `b"arx-runa-v1"` as a fixed salt. This:
- Acts as a domain separator, preventing cross-application key confusion
- Encodes application identity and key hierarchy version
- Costs nothing — same HKDF call, one extra argument

If the key hierarchy ever needs a breaking change (e.g., switching from SHA-256 to SHA-3), bumping to `b"arx-runa-v2"` produces a completely independent key tree.

**Status: Fixed. Salt changed to `b"arx-runa-v1"` in the design document.**

**Verdict: HKDF-SHA256 is correct. Salt fix applied.**

---

## Per-File Key Model

### What the design chose

Each file gets a unique random 256-bit `file_key`, generated at file creation and stored encrypted (wrapped) in the SQLCipher manifest under `key_encryption_key`.

### Alternative: single key + nonce namespace

The simpler alternative is one vault-level key and unique nonces per chunk, with no per-file keys. libsodium's secretstream API uses this model. It has lower overhead (no key wrapping) and is simpler to implement.

| Property | Per-file keys (current) | Single key + nonce namespace |
|----------|------------------------|------------------------------|
| Key compromise scope | One file | Entire vault |
| File sharing | Natural — share the file_key | Must derive file-specific key or share vault key |
| Key rotation | Rotate KEK, re-wrap file_keys only | Must re-encrypt all chunks |
| Implementation complexity | Slightly higher | Simpler |
| Nonce management | Per-file namespace isolated | Global namespace across vault |
| Used by | fscrypt (Linux), Cryptomator | libsodium secretstream |

The per-file key model's critical advantage for Arx Runa is **file sharing (Phase 5)**. ECIES share packages work by exporting the `file_key` encrypted for a recipient's public key. This is impossible without per-file keys — there is no natural unit to share with a single-key model without exposing the entire vault.

The Linux kernel's `fscrypt` filesystem encryption uses per-file key derivation for the same reason: individual files can be shared, deleted (by shredding their key), or accessed independently without affecting other files.

**Key rotation** is also much cheaper with per-file keys. Changing the user's password requires:
- **Per-file keys**: Re-wrap all `file_key_wrapped` values with the new `key_encryption_key`. O(n) cheap operations, no chunk re-encryption.
- **Single key**: Re-encrypt every chunk with the new key. O(n × chunk_count) expensive operations.

**Verdict: Per-file key model is correct and necessary for Phase 5. No change.**

---

## AAD Design

### What the design chose

Every chunk encryption call binds the ciphertext to its context via:

```
AAD = file_id (16 bytes, UUID as raw bytes) || chunk_index (4 bytes, big-endian u32)
```

### Why this matters: the broken ecosystem

A 2023 academic study ("End-to-End Encrypted Cloud Storage in the Wild: A Broken Ecosystem") analysed five major commercial encrypted cloud storage providers — Sync.com, pCloud, Icedrive, Seafile, and Tresorit — and found severe cryptographic vulnerabilities in four of them. The most common failures were:

1. **Missing AAD**: ciphertext not bound to its context, enabling chunk reordering and cross-file substitution attacks
2. **Unauthenticated key metadata**: file encryption keys stored without integrity protection
3. **No chunk ordering enforcement**: encrypted chunks could be swapped between files or within a file without detection

Arx Runa's mandatory AAD binding directly prevents all three attack classes.

### What the AAD prevents

**Cross-file substitution**: an attacker who controls cloud storage replaces blob A (from file X) with blob B (from file Y). Both blobs are valid XChaCha20-Poly1305 ciphertexts. Without AAD, blob B decrypts successfully when the client fetches what it thinks is blob A — and the client receives wrong data without any error. With AAD binding `file_id`, the `file_id` in blob B's AAD doesn't match the expected `file_id` of file X. AEAD authentication fails immediately.

**Chunk reordering**: an attacker swaps chunk 3 and chunk 7 of the same file. Without `chunk_index` in the AAD, both decrypt successfully and the client silently reassembles a corrupted file. With `chunk_index` in the AAD, the expected index doesn't match the actual index. Authentication fails.

**Rollback attack**: an attacker replaces a recently-uploaded blob with an older version of the same blob (same file, same chunk). This attack is not prevented by the current AAD design — the `snapshot_counter` in the manifest handles this at the sync layer (Phase 4), not the crypto layer.

### AAD coverage gaps

The design's AAD omission for `file_key_wrapped` is intentional and documented: the wrapped key is treated as a self-contained cryptographic blob, and the `key_encryption_key` that wraps it is unique to the vault. Cross-vault substitution of wrapped keys is not a concern — a key wrapped in vault A cannot be meaningfully imported into vault B.

The manifest backup (`manifest-backup.blob`) uses no AAD — this is also intentional, documented in the cloud-sync design, and acceptable because `manifest_key` is vault-specific and the manifest is a singleton.

**Verdict: AAD design is correct and complete. No change.**

---

## BLAKE3 Outer Integrity

### What the design chose (original)

An unkeyed BLAKE3 hash computed over each encrypted blob, stored in the SQLCipher manifest. The design stated this should be verified before decryption but did not enforce the order structurally.

### Options evaluated

**Option A — Keep unkeyed BLAKE3, add `VerifiedBlob` newtype (chosen)**

BLAKE3 stays unkeyed. The enforcement gap is closed by making `verify_checksum` return an opaque `VerifiedBlob` wrapper, and `decrypt_chunk` accept only `VerifiedBlob`. The wrong call order becomes a compile error.

```rust
pub struct VerifiedBlob(Vec<u8>);  // opaque — only constructible by verify_checksum

pub fn verify_checksum(blob: Vec<u8>, expected: &Blake3Hash) -> Result<VerifiedBlob, CryptoError>;
pub fn decrypt_chunk(blob: VerifiedBlob, ...) -> Result<Vec<u8>, CryptoError>;
```

**Option B — Switch to keyed BLAKE3**

Derive an `integrity_key` via HKDF and use BLAKE3 in keyed mode. An attacker with blob access cannot compute valid hashes without the key.

This option was rejected because:
- The stored hashes are in SQLCipher — an attacker with blob access but not `sqlcipher_key` cannot read or modify the stored hashes
- If the attacker has `sqlcipher_key`, the game is already over — they can read all metadata
- Adding a 4th derived key to `VaultKeys` adds complexity for zero practical security gain

**Option C — Drop BLAKE3 entirely**

AEAD already authenticates integrity. BLAKE3 is redundant.

This option was rejected because BLAKE3 provides operationally useful differentiation:
- `ChecksumMismatch` = hardware corruption, partial write, network error — the blob arrived damaged
- `DecryptionFailed` = authentication failure — the blob was tampered with or is from the wrong context

The distinction matters for error recovery logic in Phase 3. BLAKE3 is also extremely cheap (~1 ns/byte) — the overhead is negligible.

### The trust chain

```
Cloud storage (attacker-controlled)
        │
        │  blobs (XChaCha20-Poly1305 ciphertext)
        ▼
  verify_checksum()  ──── BLAKE3 hash stored in SQLCipher manifest (encrypted)
        │                               ▲
        │  returns VerifiedBlob          │  requires sqlcipher_key to read/write
        ▼
  decrypt_chunk()  ──── AEAD auth via file_id || chunk_index AAD
        │
        ▼
  plaintext
```

The BLAKE3 check is defence-in-depth for operational reliability. AEAD is the security boundary. SQLCipher protects the stored hashes from attacker manipulation.

**Status: Fixed. `VerifiedBlob` newtype added to the design. `decrypt_chunk` signature updated.**

---

## Implementation Correctness

### Finding 1: `rand = "0.8"` — compile error in Rust edition 2024

The design's dependency block listed `rand = "0.8"`. Rust edition 2024 reserves `gen` as a keyword. The `rand` 0.8 crate exposes a `.gen()` method on `ThreadRng` — this creates a keyword conflict that is a compile error in edition 2024.

**The API also changed** in `rand` 0.9:

```rust
// rand 0.8 — will not compile in edition 2024
rand::thread_rng().gen::<[u8; 32]>()

// rand 0.9 — correct
rand::rng().random::<[u8; 32]>()
```

Note: `thread_rng()` is also renamed to `rng()` in 0.9. Any implementation code copying the old API will fail to compile.

**Status: Fixed. Dep updated to `rand = "0.9"`. API note added to design.**

### Finding 2: Stale dependency versions

The design's dependency block was written before the scaffolding design (ADR-004) was finalised. Several versions were out of date:

| Crate | Design (original) | Scaffolding design | Status |
|-------|------------------|--------------------|--------|
| `rand` | `"0.8"` | `"0.9"` | Bug — edition 2024 conflict |
| `secrecy` | `"0.8"` | `"0.10"` | Stale — breaking API changes between 0.8 and 0.10 |
| `thiserror` | `"1.0"` | `"2"` | Stale — minor API additions in v2 |
| `blake3` | `"1.5"` | `"1"` | Cosmetic — point release pinning unnecessary |
| `uuid` | `"1.0"` | `"1"` | Cosmetic |

**Status: Fixed. Dep block aligned with scaffolding design.**

---

## Recommendation

The cryptographic primitives design is **fundamentally sound**. The cipher choice (XChaCha20-Poly1305), nonce strategy (random 192-bit), HKDF-SHA256 key derivation, per-file key model, and AAD binding (`file_id || chunk_index`) are all well-justified and consistent with current best practice. The design avoids every failure class identified in the "broken ecosystem" study of production encrypted cloud storage systems.

Six findings were identified and resolved:

| # | Finding | Severity | Resolution |
|---|---------|----------|------------|
| 1 | `rand = "0.8"` — compile error in edition 2024 (`gen` keyword) | Bug | `rand = "0.9"`, API updated to `rand::rng().random()` |
| 2 | `secrecy`, `thiserror`, `blake3` versions stale | Bug | Dep block aligned with scaffolding design |
| 3 | BLAKE3 enforcement order — no structural guarantee of check-before-decrypt | Design gap | `VerifiedBlob` newtype; `decrypt_chunk` now requires it |
| 4 | HKDF empty salt | Improvement | Salt set to `b"arx-runa-v1"` per RFC 5869 recommendation |
| 5 | Key non-commitment (XChaCha20-Poly1305) | Note | Documented; deferred to Phase 5 ECIES review |
| 6 | AEGIS-256 as future cipher | Note | Upgrade path documented in Security Considerations |

The design is ready for Phase 1 implementation.

---

## Decisions

> Choices made during this research session. Updated as the session progresses.

| Decision | Alternatives considered | Rationale |
|---|---|---|
| BLAKE3 stays unkeyed; `VerifiedBlob` newtype enforces check-before-decrypt at the type level | Keyed BLAKE3 (adds 4th derived key for zero gain — manifest is SQLCipher-protected); drop BLAKE3 (loses corruption vs. tampering signal distinction) | The enforcement gap is the real problem. `VerifiedBlob` closes it at compile time with zero runtime cost or API complexity. |
| HKDF salt: `b"arx-runa-v1"` fixed domain separator | Empty salt (original design) | RFC 5869 recommends a fixed salt even with high-entropy IKM. Provides domain separation and future versioning capability. |
| Dep versions updated: `rand = "0.9"`, `secrecy = "0.10"`, `thiserror = "2"`, `blake3 = "1"` | Stale versions | Align with scaffolding design (ADR-004); `rand >= 0.9` is required for Rust edition 2024. |
| Key non-commitment: documented as known limitation, deferred to Phase 5 | Switching to a committing AEAD (e.g., AES-GCM with commitment patch, AEGIS with 256-bit tag) | No practical attack vector for symmetric file encryption. The property only matters in protocol contexts where an attacker controls ciphertext presentation — relevant to Phase 5 ECIES, not Phase 1. |
| AEGIS-256: noted as upgrade candidate, no change now | Switch cipher to AEGIS-256 now | Still an IETF CFRG draft (rev 18); Rust crate unaudited vs. RustCrypto suite. Revisit when RFC is published and an audit appears. |
| HKDF-SHA256 retained over BLAKE3 KDF mode | BLAKE3 KDF mode (faster, fewer dependencies) | HKDF-SHA256 is RFC-standardised and extensively analysed in TLS 1.3 and Signal Protocol. BLAKE3 KDF has a lower security margin and less academic scrutiny — harder to justify in a security-critical bachelor project. |

---

## Open Questions

- **Phase 5 — key non-commitment**: when the ECIES share package import is designed, determine whether XChaCha20-Poly1305's non-committing property creates a sender-binding attack vector. If so, either use a committing AEAD for share packages specifically, or add an explicit commitment check.
- **AEGIS-256 readiness**: monitor IETF CFRG draft progression (currently draft-18) and the `aegis` Rust crate audit status. The upgrade is a crate swap — no wire format changes needed.
- **`snapshot_counter` rollback**: chunk-level rollback attacks (replacing a blob with an older valid version of the same blob) are not prevented by AAD. They are handled at the sync layer by `snapshot_counter`. Confirm this is enforced before Phase 4 implementation.

---

## Sources

| Source | Topic | URL |
|---|---|---|
| Soatok, "Comparison of Symmetric Encryption Methods", 2020 | XChaCha20-Poly1305 vs AES-GCM-SIV; cipher recommendations | https://soatok.blog/2020/07/12/comparison-of-symmetric-encryption-methods/ |
| Soatok, "Understanding Extended-Nonce Constructions", 2021 | XChaCha20 nonce safety; birthday bound analysis | https://soatok.blog/2021/03/12/understanding-extended-nonce-constructions/ |
| IACR ePrint 2023/085, "The Security of ChaCha20-Poly1305 in the Multi-user Setting" | Formal multi-user security proof for ChaCha20 | https://eprint.iacr.org/2023/085.pdf |
| RFC 8439, "ChaCha20 and Poly1305 for IETF Protocols" | ChaCha20-Poly1305 specification | https://datatracker.ietf.org/doc/html/rfc8439 |
| RFC 8452, "AES-GCM-SIV: Nonce Misuse-Resistant Authenticated Encryption" | AES-GCM-SIV design, trade-offs vs XChaCha20 | https://www.rfc-editor.org/rfc/rfc8452.html |
| IETF CFRG draft-irtf-cfrg-aegis-aead-18, "The AEGIS Family of Authenticated Encryption Algorithms" | AEGIS-256 design, nonce size, ephemeral key erasure | https://datatracker.ietf.org/doc/draft-irtf-cfrg-aegis-aead/ |
| IACR ePrint 2013/695, "AEGIS: A Fast Authenticated Encryption Algorithm" | AEGIS original academic paper | https://eprint.iacr.org/2013/695.pdf |
| RFC 5869, "HMAC-based Extract-and-Expand Key Derivation Function (HKDF)" | HKDF design; salt recommendations for high-entropy IKM | https://datatracker.ietf.org/doc/html/rfc5869 |
| C2SP BLAKE3 specification | BLAKE3 KDF mode; context string design | https://github.com/C2SP/C2SP/blob/main/BLAKE3.md |
| Neil Madden, "Galois/Counter Mode and random nonces", 2024 | Random vs sequential nonce safety; birthday bound | https://neilmadden.blog/2024/05/23/galois-counter-mode-and-random-nonces/ |
| Linux kernel fscrypt documentation | Per-file key derivation rationale; compartmentalisation | https://www.kernel.org/doc/html/v4.19/filesystems/fscrypt.html |
| libsodium, "Encrypting a set of related messages" | Single-key + nonce namespace alternative to per-file keys | https://libsodium.gitbook.io/doc/secret-key_cryptography/encrypted-messages |
| "End-to-End Encrypted Cloud Storage in the Wild: A Broken Ecosystem" (via rclone forum, 2023) | Missing AAD failures in Sync.com, pCloud, Icedrive, Seafile, Tresorit | https://forum.rclone.org/t/end-to-end-encrypted-cloud-storage-in-the-wild-a-broken-ecosystem/48275 |
| NIST SP 800-38D, "Recommendation for Block Cipher Modes of Operation: Galois/Counter Mode (GCM)" | AES-GCM nonce requirements; catastrophic reuse consequences | https://csrc.nist.gov/publications/detail/sp/800-38d/final |
