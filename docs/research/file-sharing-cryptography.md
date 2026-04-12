# Arx Runa: File-Sharing Cryptography

> **Document type**: Exploration / feasibility research
> **Status**: Concluded
> **Last updated**: 2026-04-12

Justification and alternative analysis for the cryptographic decisions in the Arx Runa Phase 5 file-sharing design: ECIES variant selection (historical draft), elliptic curve choice (X25519 vs P-256), KDF construction inside ECIES (historical draft), and committing AEAD selection (mandated by Phase 1 primitive research).

For background on the cryptographic primitives used throughout Arx Runa, see `cryptographic-primitive-rationale.md`.
For the file-sharing architecture, see `docs/architecture/designs/file-sharing/design.md`.
For the canonical cryptographic specification, see `docs/architecture/designs/cryptographic-primitives/design.md`.

---

## Table of Contents

1. [The Problem](#the-problem)
2. [ECIES Variant Selection](#ecies-variant-selection)
3. [Elliptic Curve: X25519 vs P-256](#elliptic-curve-x25519-vs-p-256)
4. [KDF Inside ECIES](#kdf-inside-ecies)
5. [Committing AEAD Selection](#committing-aead-selection)
6. [Key Commitment and Partition Oracle Attacks](#key-commitment-and-partition-oracle-attacks)
7. [Recommendation](#recommendation)
8. [Decisions](#decisions)
9. [Open Questions](#open-questions)
10. [Sources](#sources)

---

## The Problem

The original Phase 5 draft encrypted share packages using ad-hoc ECIES: the sender performed an ephemeral ECDH with the recipient's long-term X25519 public key, derived a symmetric key via HKDF, and encrypted the share package payload with XChaCha20-Poly1305. This research evaluated that draft and selected HPKE (RFC 9180) + CTX-ChaCha20-Poly1305 for the canonical design.

The design raises four cryptographic questions that require principled justification:

1. **ECIES variant**: Which ECIES construction (ISO 18033-2, SEC1, HPKE RFC 9180, or ad-hoc ECIES-KEM+DEM) should be used? Each has different security properties and library support.
2. **Curve selection**: X25519 (Curve25519 in ECDH mode) or P-256 (NIST curve)? The design specifies X25519 but the rationale needs to be documented.
3. **KDF inside ECIES**: HKDF-SHA256 is used with `info="arx-runa-share"`. What are the alternatives and is this the right choice?
4. **Committing AEAD**: The `cryptographic-primitive-rationale.md` explicitly mandates a committing AEAD for Phase 5 (to defend against partition oracle attacks in the multi-key context of ECIES). XChaCha20-Poly1305 is non-committing. What cipher should replace or supplement it?

---

## ECIES Variant Selection

The original Arx Runa draft used ad-hoc ECIES: a custom composition of X25519 ECDH + HKDF + XChaCha20-Poly1305. The primary alternative evaluated here is **HPKE (RFC 9180)**.

### Ad-Hoc ECIES (original draft)

The current construction — ephemeral X25519 → ECDH → HKDF-SHA256 → XChaCha20-Poly1305 — matches the construction used by the **`age` encryption tool** (Filippo Valsorda, 2019). The `age` tool is widely adopted and well-regarded; its X25519 recipient type uses:

```
shared_secret = ECDH(ephemeral_private, recipient_public)
wrap_key = HKDF-SHA256(IKM=shared_secret, salt=ephemeral_pk||recipient_pk, info="age-encryption.org/v1/X25519")
ciphertext = ChaCha20-Poly1305.Seal(wrap_key, file_key)
```

Notable difference from the current Arx Runa spec: `age` includes **both** the ephemeral and recipient public keys in the HKDF salt, providing explicit key binding. The Arx Runa design uses only the ephemeral public key as the salt.

### HPKE (RFC 9180) — the modern alternative

HPKE (Hybrid Public Key Encryption, RFC 9180, published 2022) is the IETF CFRG standardization of exactly this pattern. Key improvements over ad-hoc ECIES:

| Property | Ad-hoc ECIES | HPKE (RFC 9180) |
|---|---|---|
| Formal IND-CCA2 proof | No (security relies on informal analysis) | Yes |
| Includes both public keys in key schedule | Only if explicitly added (age does this, current Arx Runa spec does not) | Yes — all public keys always included |
| Prevents KEM malleability | Not guaranteed | Formally proven |
| AEAD agility | Manual | Built-in: KEM/KDF/AEAD are modular |
| Test vectors | No | Yes (RFC 9180 Appendix A) |
| Adoption | age, Noise Protocol variants | TLS Encrypted Client Hello (ECH), ODoH, DAP/PPM |
| Rust ecosystem | `x25519-dalek` + `hkdf` + `chacha20poly1305` | `hpke` crate v0.13.0 (~4M downloads) |
| Formal analysis | None specific to this composition | Badertscher et al. 2021 (analysis of HPKE security) |

HPKE does **not** natively address key commitment — it defers to the AEAD layer. If a committing AEAD is needed, it must be plugged in as the AEAD component.

---

## Elliptic Curve: X25519 vs P-256

Both curves provide **~128-bit security** (equivalent to AES-128, RSA-3072). The design specifies X25519; the rationale is documented here.

### X25519 (Curve25519)

Designed by Bernstein (2006), standardized in **RFC 7748** (2016). Key properties:

- **SafeCurves** compliant (Bernstein & Lange, 2013 / updated 2024): meets conservative security criteria that P-256 fails on several axes
- **Constant-time by construction**: the Montgomery ladder scalar multiplication is naturally constant-time; timing side-channel requires deliberate effort to introduce
- **Cofactor clamping**: cofactor h=8 is handled by "clamping" the scalar (RFC 7748 §5), eliminating small-subgroup attacks without requiring the implementation to check point order
- **No twist attacks in practice**: cofactor clamping and the protocol design prevent low-order point injection
- **No patents**: Bernstein explicitly dedicated the curve to the public domain
- **Widely deployed**: TLS 1.3 (RFC 8446), WireGuard, Signal Protocol, SSH, age, Noise Protocol

### P-256 (prime256v1 / secp256r1)

NIST FIPS 186-4 standard. Key issues relative to X25519:

- **Implementation pitfalls**: historically vulnerable to timing side-channels due to incomplete addition formulas in Weierstrass form; constant-time P-256 requires explicit engineering
- **SafeCurves**: P-256 fails several SafeCurves criteria (twist security, completeness)
- **Cofactor h=1**: advantage (no small subgroup issue) but the implementation complexity is higher
- **FIPS compliance**: the only advantage — required in some government/regulated contexts

For Arx Runa (zero-knowledge personal vault, no FIPS requirement), X25519 is the correct choice.

### Comparison table

| Property | X25519 | P-256 |
|---|---|---|
| Security level | ~128-bit | ~128-bit |
| SafeCurves | Yes | No |
| Constant-time by construction | Yes | No — requires explicit effort |
| Cofactor handling | Automatic via clamping (RFC 7748) | h=1, no issue |
| Patent status | Public domain | NIST (no known patents, but NIST origins) |
| FIPS compliance | No (RFC 7748 only) | Yes (FIPS 186-4) |
| Rust ecosystem | `x25519-dalek` (audited by NCC Group) | `p256` (RustCrypto) |
| Used by | TLS 1.3, WireGuard, Signal, age, SSH | TLS, ECDSA certificates, FIDO2 |

---

## KDF Inside ECIES

The current design: `HKDF-SHA256(shared_secret, salt=ephemeral_public_key, info="arx-runa-share")`

### Issue: Missing recipient public key in salt

The `age` tool and HPKE both include **both** public keys (ephemeral and recipient long-term) in the HKDF salt:

```
age:  salt = ephemeral_pk || recipient_pk
HPKE: labeled KDF includes both keys in the "kem_context" via the key schedule
```

Including the recipient's long-term public key provides **explicit key binding**: the derived symmetric key is cryptographically bound to the specific recipient. Without it, an attacker who can construct a different X25519 keypair where the ECDH output is the same (practically infeasible, but theoretically unsound) could substitute keys without detection.

This is a **soundness improvement**, not a practical vulnerability in the current design (the ECDH shared secret already implicitly depends on the recipient's public key). But explicit binding matches the construction in age and HPKE, and is the correct practice.

**Recommended fix**: change the HKDF salt from `ephemeral_public_key` to `ephemeral_public_key || recipient_public_key`, matching the age construction.

### HKDF-SHA256: correct choice

HKDF-SHA256 (RFC 5869) is the right KDF for this context:
- Standardized and well-analyzed
- Used by age, HPKE, TLS 1.3, Signal, WireGuard — all with X25519 ECDH as IKM
- The `info="arx-runa-share"` string provides application-identity domain separation
- Output is independent of any other HKDF derivation in the key tree (different IKM, different info)

No alternative KDF is justified here.

---

## Committing AEAD Selection

### The mandate

`cryptographic-primitive-rationale.md` explicitly mandates a committing AEAD for Phase 5. XChaCha20-Poly1305 is not committing (CMT-1 insecure).

### Why it matters: partition oracle attacks

**Len, Grubbs, Ristenpart — "Partitioning Oracle Attacks" (USENIX Security 2021)** demonstrated that non-committing AEADs enable *partitioning oracle attacks*: an adversary can construct a single ciphertext that decrypts successfully under multiple keys, then use a decryption oracle to determine which key a target holds. Confirmed vulnerable: AES-GCM, ChaCha20-Poly1305, XSalsa20-Poly1305.

In the context of ECIES for file sharing: each share package uses a *fresh* ECDH-derived key per recipient, so the multi-key scenario arises when the same package is sent to multiple recipients (or more relevantly: when an attacker can query an oracle that tries many keys against a single ciphertext). The threat is lower than in systems with a single static key shared across many users, but the mandate stands because the `file_key` inside the package is a real secret that could be targeted.

### Key commitment constructions

| Construction | Source | How it works | Cost | Ciphertext size change |
|---|---|---|---|---|
| **CTX** | Chan & Rogaway, IACR 2022 | Replace AEAD tag with `H(key \|\| nonce \|\| ciphertext)` | One hash over short input | None (same tag size) |
| **UtC** (prepend commitment) | Bellare & Hoang, EUROCRYPT 2022 | Prepend `H(key)` to ciphertext, then AEAD | One short hash | +32 bytes commitment |
| **HtE** | Bellare & Hoang 2022 | Hash key+message, use result to re-key before encrypt | One hash over plaintext | None |
| **AES-GCM-SIV** | RFC 8452 | Nonce-misuse resistant, but NOT committing | — | Requires AES, not native committing |
| **AEGIS-256-MAC** | IETF draft | Purpose-built committing AEAD | — | Not yet RFC; no Rust audit |

**Key finding**: AES-GCM-SIV is nonce-misuse resistant but is **not** a committing AEAD (GCM's GHASH authentication is not collision-resistant under multi-key). The search results and USENIX paper both confirm GCM and GCM-SIV are CMT-1 insecure.

### Practical recommendation path

The simplest production-ready approach for Arx Runa:

**Option A — CTX construction on top of XChaCha20-Poly1305**
Replace the 16-byte Poly1305 authentication tag with a 32-byte commitment tag:
```
commitment = BLAKE3(b"arx-runa-commitment-v1" || key || nonce || ciphertext)
wire: [ephemeral_pk | nonce | ciphertext | commitment(32B)]
```
This achieves CMT-1 (key-committing) and CMT-4 (full commitment) security per the CTX paper. Cost: one BLAKE3 call over a short string. Wire format changes: tag size increases from 16 to 32 bytes.

**Option B — Prepend key commitment prefix (UtC-style)**
Prepend `BLAKE3(b"arx-runa-key-commit-v1" || key)` (32 bytes) before the AEAD ciphertext:
```
wire: [ephemeral_pk | nonce | key_commitment(32B) | ciphertext | Poly1305 tag(16B)]
```
Slightly larger (extra 32 bytes), but allows separate verification of key commitment and ciphertext integrity without re-implementing AEAD internals.

**Option C — Migrate to HPKE + committing AEAD**
Use HPKE (RFC 9180) with a future committing AEAD (e.g., AEGIS-256-MAC when it reaches RFC). This is the most future-proof path but depends on AEGIS-256 standardization.

---

## Key Commitment and Partition Oracle Attacks

### Why ECIES + non-committing AEAD is specifically risky

In ECIES, the AEAD key is derived from an ephemeral ECDH. If the AEAD is not committing:
1. Attacker can construct a ciphertext `C` that decrypts under key `K₁` to malicious content, and under key `K₂` to benign content
2. In a file-sharing context: attacker delivers such a `C` to recipient; depending on which key the recipient uses, they get different content
3. The partition oracle risk is real if the system re-uses or exposes the ECDH-derived key for multiple operations (in Arx Runa: the outer envelope key is also used to decrypt `file_key_wrapped` inside the package — this is a two-application use of the same key)

### Arx Runa-specific concern: double use of the derived key

The current design uses the ECDH-derived symmetric key for **two purposes**:
1. Encrypting the outer envelope (JSON payload)
2. Separately encrypting `file_key_wrapped` inside the envelope

Using the same key for two different ciphertexts creates a key commitment dependency: if the outer AEAD decrypts successfully, you cannot assume the `file_key_wrapped` tag is also valid under a different key. A committing AEAD on the outer envelope resolves this — once the outer envelope verifies, the key is bound.

**Alternative recommendation**: include `file_key` directly in the ECIES-encrypted JSON payload rather than separately re-encrypting it. This eliminates the redundant encryption and simplifies the construction. The outer AEAD already provides confidentiality and integrity for everything inside the envelope.

---

## Recommendation

### 1. Migrate from ad-hoc ECIES to HPKE (RFC 9180)

Use `DHKEM(X25519, HKDF-SHA256) + HKDF-SHA256 + CTX-ChaCha20-Poly1305` as the HPKE ciphersuite.

The current ad-hoc ECIES construction is functional but lacks a formal security proof and requires manual discipline to keep correct. HPKE (RFC 9180) standardizes exactly this pattern with an IND-CCA2 proof, automatic inclusion of both public keys in the key schedule, test vectors, and a modular design that makes algorithm agility straightforward. The `hpke` crate (v0.13.0, ~4M downloads) provides a production-quality Rust implementation.

**Impact on existing cryptographic-primitives design**: none. HPKE is additive — a new Phase 5 module only. The vault encryption stack (XChaCha20-Poly1305, HKDF from `master_key`, Argon2id, BLAKE3, ZeroizeOnDrop) is untouched. Note that the AEAD inside HPKE is ChaCha20-Poly1305 (96-bit nonce managed by HPKE), not XChaCha20-Poly1305 — this is correct and intentional.

### 2. CTX construction for key commitment

Wrap ChaCha20-Poly1305 inside a CTX layer as the AEAD component of HPKE:

```
tag = BLAKE3(b"arx-runa-ctx-v1" || key || nonce || ciphertext)
wire: [ciphertext | tag(32B)]
```

The Poly1305 tag is replaced with a 32-byte BLAKE3 commitment. This achieves CMT-4 security (the strongest committing AEAD notion) per Chan & Rogaway (IACR 2022). Cost: one BLAKE3 call over a short input, constant in message length. Wire format: tag grows from 16 to 32 bytes — negligible for share packages.

This is implemented as a thin wrapper type (`CtxChaCha20Poly1305`) in the sharing crypto module, not a change to the existing `src-tauri/src/crypto/` module.

### 3. Wire format

With HPKE one-shot mode, the new share package wire format is:

```
[enc(32B) | ciphertext | ctx_tag(32B)]
```

Where `enc` is the ephemeral public key output by HPKE's KEM. The 24-byte explicit nonce from the current design disappears — HPKE manages the nonce internally. Net wire format change: −24 bytes (nonce removed) + 16 bytes (tag grows from 16 → 32) = **−8 bytes**.

### 4. `file_key` directly inside the envelope

Replace `file_key_wrapped` with `file_key` as raw bytes in the JSON payload:

```json
{
  "share_id": "...",
  "file_id": "...",
  "file_name": "report.pdf",
  "file_key": "<32 bytes, base64>",
  "chunk_count": 12,
  "chunk_size": 4194304,
  "chunk_uuids": ["..."],
  "cloud_endpoint": { ... },
  "expires_at": null
}
```

The HPKE outer envelope (with CTX-ChaCha20-Poly1305) provides all confidentiality and integrity for `file_key`. The previous `file_key_wrapped` was doubly encrypted with the same derived key, which is redundant and required a second nonce with no clear construction. The "wrapped" terminology is reserved for KEK-based wrapping in the vault.

### 5. Curve: X25519 confirmed

No change. X25519 is SafeCurves-compliant, constant-time by construction, patent-free, and the natural pairing for HPKE's `DHKEM(X25519, HKDF-SHA256)` ciphersuite.

### Summary

| Aspect | Current design | Recommended |
|---|---|---|
| Outer construction | Ad-hoc ECIES | HPKE RFC 9180 |
| KEM | Manual X25519 + HKDF | DHKEM(X25519, HKDF-SHA256) |
| KDF | HKDF-SHA256 (salt = ephemeral_pk only) | HPKE key schedule (includes both public keys automatically) |
| AEAD | XChaCha20-Poly1305 (non-committing) | CTX-ChaCha20-Poly1305 (CMT-4 committing) |
| Key in envelope | `file_key_wrapped` (double-encrypted) | `file_key` (raw, inside HPKE-protected JSON) |
| Wire format | `[epk(32) \| nonce(24) \| ct \| tag(16)]` | `[enc(32) \| ct \| ctx_tag(32)]` |

---

## Decisions

> Choices made during this research session. Updated as the session progresses.

| Decision | Alternatives considered | Rationale |
|---|---|---|
| ECIES construction: HPKE (RFC 9180) | Ad-hoc ECIES (same as `age` tool) | Formal IND-CCA2 proof; both public keys always in key schedule by construction; test vectors; modular AEAD agility; widely deployed in TLS ECH and ODoH |
| Curve: X25519 (confirmed) | P-256 | SafeCurves compliant; constant-time by construction; no patents; HPKE natively supports DHKEM(X25519, HKDF-SHA256) |
| KDF inside ECIES: absorbed into HPKE key schedule | Manual HKDF with ephemeral_pk only as salt | HPKE's key schedule automatically includes both public keys and provides domain separation via labeled ops |
| Committing AEAD: CTX construction over ChaCha20-Poly1305 | UtC prefix, AES-GCM-SIV (not committing), AEGIS-256-MAC (draft only) | CTX achieves CMT-4 (full commitment); one BLAKE3 call over a short string; tag grows from 16 → 32 bytes (negligible at share package size); no plaintext pass required |
| `file_key` included as raw bytes inside HPKE envelope | `file_key_wrapped` (double-encrypted with same key) | HPKE outer envelope already provides confidentiality and integrity; inner wrapping is redundant and requires a second nonce; "wrapped" terminology is reserved for KEK-based wrapping in the vault |

---

## Open Questions

- **AEGIS-256 + HPKE**: When `draft-irtf-cfrg-aegis-aead` reaches RFC status and a Rust audit is available, AEGIS-256-MAC (a purpose-built committing AEAD) could replace the CTX wrapper as the HPKE AEAD component. The wire format and HPKE API call sites would remain identical — only the AEAD type parameter changes.
- **HPKE sender authentication**: The Base mode (used here) provides no sender authentication — any holder of the recipient's public key can create a valid share package. The Auth mode (`SetupAuthS` / `SetupAuthR`) adds sender authentication using the sender's long-term private key. This is not needed for Phase 5 (out-of-band key exchange already implies trust) but is noted as a future option for stronger provenance guarantees.
- **Post-quantum migration**: HPKE's modular KEM design means a PQ-KEM (e.g., ML-KEM / Kyber) can replace DHKEM(X25519) when needed. `PQ-HPKE` (Anastasova et al., IACR 2022) documents this path.

---

## Sources

| Source | Topic | URL |
|---|---|---|
| RFC 9180 — Hybrid Public Key Encryption (Barnes, Bhargavan, Lipp, Wood; 2022) | HPKE: formal specification, IND-CCA2 proof, KEM/KDF/AEAD composition | https://www.rfc-editor.org/rfc/rfc9180 |
| RFC 7748 — Elliptic Curves for Security (Langley, Hamburg, Turner; 2016) | X25519 and X448 specification; cofactor clamping | https://www.rfc-editor.org/rfc/rfc7748 |
| Bernstein & Lange — "Safe Curves for Elliptic-Curve Cryptography" (2013, updated 2024) | SafeCurves criteria; X25519 vs P-256 security properties | https://cr.yp.to/papers/safecurves-20240809.pdf |
| Len, Grubbs, Ristenpart — "Partitioning Oracle Attacks" (USENIX Security 2021) | Partition oracle attacks on AES-GCM, ChaCha20-Poly1305, XSalsa20-Poly1305 | https://www.usenix.org/conference/usenixsecurity21/presentation/len |
| Chan & Rogaway — "On Committing Authenticated Encryption" (IACR 2022) | CTX construction; CMT-1/CMT-4 security notions; key commitment for ECIES | https://eprint.iacr.org/2022/1260 |
| Bellare & Hoang — "Efficient Schemes for Committing Authenticated Encryption" (EUROCRYPT 2022) | UtC, RtC, HtE transforms for adding key commitment | https://eprint.iacr.org/2022/268.pdf |
| Cloudflare Blog — "HPKE: Standardizing public-key encryption (finally!)" | HPKE vs ECIES comparison; problems HPKE fixes; adoption in TLS ECH | https://blog.cloudflare.com/hybrid-public-key-encryption/ |
| `hpke` crate — rust-hpke (rozbb) | RFC 9180 Rust implementation; v0.13.0; ~4M downloads | https://crates.io/crates/hpke |
| `age` X25519 recipient (Filippo Valsorda) — x25519.go | age ECIES construction: HKDF salt = ephemeral_pk \|\| recipient_pk; ChaCha20-Poly1305 | https://github.com/FiloSottile/age/blob/main/x25519.go |
| `aes-gcm-siv` crate — RustCrypto (Tony Arcieri) | AES-GCM-SIV Rust implementation; audit status: no direct audit | https://crates.io/crates/aes-gcm-siv |
| NIST FIPS 186-4 — Digital Signature Standard (2013) | P-256 / secp256r1 curve specification | https://csrc.nist.gov/publications/detail/fips/186/4/final |
