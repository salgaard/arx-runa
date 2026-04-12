# Arx Runa: Cryptographic Primitive Rationale

> **Document type**: Exploration / feasibility research
> **Status**: Draft
> **Last updated**: 2026-04-12

Justification and alternative analysis for every cryptographic primitive selected in the Arx Runa cryptographic-primitives design: XChaCha20-Poly1305 AEAD, HKDF-SHA256 key derivation, Argon2id password hashing, per-file random key generation and wrapping, BLAKE3 checksums, and the `ZeroizeOnDrop` + `Secret<T>` memory-protection stack.

For background on the auth/session design, see `docs/architecture/designs/auth-and-session/design.md`.
For the canonical cryptographic specification, see `docs/architecture/designs/cryptographic-primitives/design.md`.
For password and key recovery, see `password-and-key-recovery.md`.

---

## Table of Contents

1. [The Problem](#the-problem)
2. [AEAD Cipher: XChaCha20-Poly1305](#aead-cipher-xchacha20-poly1305)
3. [Key Derivation: HKDF-SHA256](#key-derivation-hkdf-sha256)
4. [Password Hashing: Argon2id](#password-hashing-argon2id)
5. [Per-File Key Generation and Wrapping](#per-file-key-generation-and-wrapping)
6. [BLAKE3 Checksums](#blake3-checksums)
7. [Memory Protection: ZeroizeOnDrop + Secret\<T\>](#memory-protection-zeroizeondrop--secrett)
8. [Recommendation](#recommendation)
9. [Decisions](#decisions)
10. [Open Questions](#open-questions)
11. [Sources](#sources)

---

## The Problem

Arx Runa is a zero-knowledge, bring-your-own-cloud file encryption tool. Every cryptographic primitive must be justified against a threat model where the cloud provider is a potential adversary — they hold opaque blobs and nothing else. The design must be:

- **Correct**: proven-secure constructions only, no custom cryptography
- **Implementation-safe**: resistant to common implementation errors (nonce reuse, timing attacks, memory disclosure)
- **Auditable**: well-documented in published standards (NIST, IETF RFC, IACR) with real-world prior art
- **Future-proof**: upgrade paths documented when better alternatives exist or are maturing

This document justifies each primitive selected, presents alternatives that were considered and rejected, and provides authoritative sources for each claim.

---

## AEAD Cipher: XChaCha20-Poly1305

### Selected: XChaCha20-Poly1305 (192-bit nonce)

XChaCha20-Poly1305 is the extended-nonce variant of ChaCha20-Poly1305, standardized in RFC 8439. The "X" prefix extends the nonce from 96 bits to 192 bits by running an additional HChaCha20 subkey derivation step, making random nonce generation safe at any practical volume.

### Alternatives Considered

| Alternative | Why rejected |
|---|---|
| AES-256-GCM | Nonce reuse is catastrophic (auth key leaks); constant-time requires AES-NI hardware; shorter GCM nonce (96-bit) requires counter discipline |
| ChaCha20-Poly1305 (RFC 8439) | 96-bit nonce is too short for random generation (birthday bound ~2^32 before collision risk); requires counter/state |
| AES-256-GCM-SIV (RFC 8452) | Nonce-misuse resistant, but maximum message length is 4 GiB and there is a multi-key safety limit; less library support in the Rust ecosystem |
| AEGIS-256 | ~2× higher throughput on AES-NI hardware, 256-bit nonce, ephemeral key erasure — but still in IETF CFRG draft (draft-irtf-cfrg-aegis-aead); no completed RFC or independent Rust crate audit available yet |

### Nonce Safety

draft-irtf-cfrg-xchacha-03 Section 3.1 states verbatim:

> "Assuming a secure random number generator, random 192-bit nonces should experience a single collision (with probability 50%) after roughly 2^96 messages. A more conservative threshold (2^-32 chance of collision) still allows for 2^80 messages."

Applying the birthday bound formula (collision probability ≈ q² / 2¹⁹³):
- After 2⁶⁴ encryptions: ~2⁻⁶⁵ collision probability (negligible)
- After 2⁸⁰ encryptions: ~2⁻³³ collision probability (conservative safe threshold per the draft)
- With 96-bit nonces: collision probability becomes non-negligible around 2³² encryptions

Bernstein (SKEW 2011) provides the underlying security proof that the HChaCha20 subkey derivation step makes the extended-nonce construction secure under the same assumptions as the base cipher.

For a personal vault encrypting thousands of chunks per file, the 192-bit nonce space is effectively unbounded.

### AES-GCM Nonce Reuse Catastrophe

When AES-GCM reuses a nonce, the Poly1305 authentication key (H) is directly computable by XOR of the two ciphertexts, and the auth tag reduces to a linear equation over GF(2^128). This allows both confidentiality breach (plaintext recovery) and integrity forgery. The 2016 "Nonce-Disrespecting Adversaries" paper (Böck et al., USENIX WOOT 2016) demonstrated this attack against real TLS implementations.

ChaCha20-Poly1305 nonce reuse leaks XOR of plaintexts but does not break the authentication key — a bad outcome but not as catastrophic.

### Key Non-Commitment

XChaCha20-Poly1305 is not a *committing* AEAD. It does not provide binding security (also called "key commitment" or "CMT-1 security"). In theory, it is possible to find two different keys that both authenticate the same ciphertext (a "multi-key" or "partition oracle" attack). For symmetric file encryption with a single key per file, this is not a practical threat. Phase 5 file sharing addresses this by using HPKE with `CTX-ChaCha20-Poly1305` — a CMT-4 committing AEAD — for share package encryption. See `docs/research/file-sharing-cryptography.md`.

### Upgrade Path

AEGIS-256 (IETF CFRG draft-irtf-cfrg-aegis-aead) is the leading candidate for a future upgrade:
- ~0.7 cycles/byte vs ~1.5 cycles/byte for XChaCha20-Poly1305 on AES-NI hardware
- 256-bit nonce (safe for random generation)
- Ephemeral key erasure before data processing (forward secrecy property)
- Committing AEAD variant (AEGIS-256-MAC) under development

The wire format and API surface would remain identical; only the cipher primitive changes.

---

## Key Derivation: HKDF-SHA256

### Selected: HKDF-SHA256 (RFC 5869)

HKDF (HMAC-based Key Derivation Function) is a two-step construction: an Extract step that produces a uniform pseudorandom key (PRK) from the IKM, and an Expand step that stretches PRK into multiple derived keys using domain-separated `info` strings.

In Arx Runa, Argon2id already produces a high-entropy 32-byte `master_key`, so HKDF is used in Expand-only mode (the salt acts as a domain separator, not for entropy extraction).

### Key Derivation Tree

```
master_key (Argon2id output, 32 bytes)
    │
    ├── HKDF-SHA256(info = "arx-runa-key-encryption")  → key_encryption_key (32 bytes)
    ├── HKDF-SHA256(info = "arx-runa-sqlcipher")       → sqlcipher_key (32 bytes)
    └── HKDF-SHA256(info = "arx-runa-manifest-backup") → manifest_key (32 bytes)
```

Salt: Fixed domain separator `b"arx-runa-v1"` — provides application-identity binding even though Argon2id output already has full entropy.

### Alternatives Considered

| Alternative | Why rejected |
|---|---|
| BLAKE3-derive_key | Fast, elegant, but less widely audited for key derivation specifically; HKDF-SHA256 has broader standards recognition (NIST SP 800-56C, RFC 5869) |
| SP 800-108 KBKDF (Counter Mode) | Counter-based KDF; more complex implementation; primarily used in FIPS contexts where HKDF is not acceptable |
| Direct SHA-256 truncation | Not a KDF — lacks domain separation; XOR of outputs is trivially related to the master key |
| Repeat Argon2id calls | Prohibitively expensive for vault-open latency; Argon2id is designed for password stretching, not fast key expansion |

### Why SHA-256, not SHA-3 or BLAKE2?

HKDF is defined over HMAC-SHA2. SHA-256 is the baseline:
- Standardized: NIST FIPS 180-4
- Proven security reduction: HMAC security relies on PRF assumption of SHA-256, which is well-studied
- Widely used: TLS 1.3 (RFC 8446), Signal Protocol, WireGuard all use HKDF-SHA256
- SHA-3 (Keccak) would also be fine but offers no practical advantage here and has less Rust ecosystem history in this role

### Info String Domain Separation

Each derived key uses a distinct `info` string. HKDF's Expand step is a PRF keyed by PRK, so outputs for distinct `info` values are computationally independent. Knowing `key_encryption_key` gives no information about `sqlcipher_key` under standard HMAC assumptions.

### Extensibility

New keys are added by expanding with a new `info` string. Existing keys are unaffected — HKDF outputs are independent by construction. This is used in Phase 5 (file sharing) which adds a separate derivation tree using ECDH shared secrets as IKM, documented separately.

---

## Password Hashing: Argon2id

### Selected: Argon2id (RFC 9106)

Argon2id is the winner of the Password Hashing Competition (PHC, 2015) and is the current OWASP, NIST SP 800-63B, and RFC 9106 recommendation for password-based key derivation.

The "id" variant combines:
- **Argon2i**: data-independent memory access pattern (side-channel resistant)
- **Argon2d**: data-dependent memory access (GPU/ASIC resistant)

Argon2id uses data-independent access for the first pass (protecting against side-channel attacks from co-located processes) and data-dependent for subsequent passes (making GPU/ASIC optimization expensive).

### Recommended Parameters

RFC 9106 Section 4 states two recommended parameter sets. The second (higher-security) option is verbatim:

> "t=3 iterations, p=4 lanes, m=2^16 (64 MiB of RAM), 128-bit salt, 256-bit tag size."

**Arx Runa uses this set: 64 MiB memory, 3 iterations, parallelism 4.**

This is not an arbitrary choice — it is the IETF standard recommendation for non-interactive or high-security contexts. OWASP also recommends these parameters as the upper tier of interactive desktop authentication.

### Alternatives Considered

| Alternative | Why rejected |
|---|---|
| bcrypt | Maximum 72-byte password limit; no memory hardness; output entropy limited to 184 bits; not suitable for key derivation |
| scrypt | Predecessor to Argon2; time-memory trade-off is less favorable; RFC 7914 but not recommended by OWASP for new designs |
| PBKDF2-SHA256 | No memory hardness; GPU-parallelizable; NIST still recommends for FIPS contexts but Argon2id is strictly superior for key derivation |
| Balloon Hashing | NIST 800-63B mentions it as a future option; not yet in RFC; less library support |

### Output

Argon2id produces a 32-byte (256-bit) `master_key`. This is the only point where a user-controlled secret (password + optional recovery phrase) is converted to key material.

### Salt

The Argon2id salt is a random 32-byte value stored in the vault header (plaintext). This is the standard design: the salt prevents pre-computation attacks (rainbow tables) but has no secrecy requirement. Without the password, the salt provides no advantage to an attacker. 32 bytes exceeds the NIST SP 800-132 minimum of 128 bits and is consistent with the 256-bit security level used throughout Arx Runa.

---

## Per-File Key Generation and Wrapping

### Selected: Per-file random 256-bit key, wrapped with key_encryption_key

Each file is encrypted with a unique `file_key` generated from CSPRNG. The `file_key` is never stored in plaintext — it is wrapped (encrypted) using `key_encryption_key` (XChaCha20-Poly1305) and stored in the manifest. To decrypt a file, the vault must be unlocked (deriving `key_encryption_key`), then the `file_key` is unwrapped just-in-time.

### Key Hierarchy

```
master_key
    └── key_encryption_key (HKDF)
            └── file_key_1 (random, wrapped)
            └── file_key_2 (random, wrapped)
            └── ...
```

### Alternatives Considered

| Alternative | Why rejected |
|---|---|
| Single vault-wide chunk key | Compromise of one file's key would compromise all files; no key rotation granularity |
| Derive file_key from master_key + file_id (deterministic) | Deterministic derivation means key rotation requires re-encrypting all files; also, if HKDF info is guessable, the file_key is computable without the wrapped blob |
| Per-user key + per-file nonce/tweak | Less standard; AEAD already provides per-operation randomness via nonce; this mixes tweak and key concepts |

### Key Isolation Properties

NIST SP 800-57 Part 1 Rev. 5 Section 6.2 defines the purpose of a Key Encryption Key (KEK) hierarchy: "keys used to encrypt other keys" with the explicit goal of **limited exposure** — a data-encrypting key (DEK) compromise affects only the data it protects, not other DEKs. In Arx Runa: `key_encryption_key` is the KEK; each `file_key` is a DEK.

Per-file random keys therefore provide:
1. **Limited exposure** (NIST SP 800-57 §6.2): compromising one `file_key` exposes only that file's data — other files remain protected under independent keys
2. **Key rotation**: individual files can be re-encrypted by generating a new `file_key` without touching other files or the `key_encryption_key`
3. **Sharing**: Phase 5 file sharing includes a single `file_key` inside an HPKE share package envelope — the vault's `key_encryption_key` is never shared

LUKS (Linux Unified Key Setup) and Linux fscrypt implement the same pattern as production precedents: per-volume or per-file keys independently wrapped by passphrase-derived keys, so that passphrase compromise does not automatically compromise data keys.

### Wire Format

Wrapped file key: `[24-byte nonce | 32-byte encrypted file_key | 16-byte Poly1305 tag]` = 72 bytes

The wrapping uses XChaCha20-Poly1305 with empty AAD (the wrapped key is self-contained — its identity comes from where it is stored in the manifest, which is itself authenticated by SQLCipher). Recovery slot wrapping uses non-empty AAD (`b"arx-runa recovery v1" || vault_id_bytes`) to bind the blob to a specific vault.

---

## BLAKE3 Checksums

### Selected: BLAKE3 (unkeyed, over encrypted blob)

BLAKE3 checksums are computed over the **encrypted blob** (not plaintext) and stored in the SQLCipher manifest. Before decryption, the checksum is verified via the `VerifiedBlob` newtype — a type-system enforced check that makes skipping verification a compile error.

### What the Checksum Provides

BLAKE3 here is a **corruption detection** check, not an **authentication** check. The AEAD tag already provides INT-CTXT security (ciphertext integrity) — an adversary cannot produce a new valid ciphertext without the key (Bellare & Namprempre 2000). BLAKE3 provides:
- Fast pre-decryption corruption detection (hardware errors, network corruption, partial downloads)
- Actionable error messages: "blob is corrupt" vs "authentication failed" are different failure modes
- Early failure before the more expensive AEAD operation

Krawczyk (CRYPTO 2001) proves that checking integrity over the ciphertext before decryption — Encrypt-then-MAC — is the only generically secure ordering. BLAKE3 follows this ordering.

### Why Unkeyed?

The literature on Encrypt-then-MAC (Krawczyk 2001, Bellare-Namprempre 2000) requires a *keyed* MAC to provide INT-CTXT. BLAKE3 here is unkeyed — this is a deliberate scope reduction, justified by the specific trust model:

1. **The AEAD tag already provides INT-CTXT.** An adversary cannot forge a new valid ciphertext without the `file_key`. BLAKE3 does not need to provide authentication — it only needs to detect accidental corruption.
2. **The hash is stored inside SQLCipher.** SQLCipher encrypts and authenticates the entire manifest database. An adversary who can modify a cloud blob cannot also silently modify the corresponding BLAKE3 hash in the manifest — the SQLCipher authentication tag would fail. The hash is therefore protected from adversarial manipulation by an independent authenticated channel.
3. **Corruption detection only requires collision resistance, not a keyed MAC.** BLAKE3 is collision-resistant — accidentally corrupted data will produce a different hash with overwhelming probability. This is sufficient for the stated purpose.

This is a design argument, not a directly citable theorem. The claim is: *when a hash is stored in an independently authenticated channel (SQLCipher) and is only used for corruption detection rather than authentication, an unkeyed collision-resistant hash is operationally sufficient.*

### BLAKE3 vs Alternatives

| Alternative | Trade-offs |
|---|---|
| SHA-256 | ~3-4× slower on modern hardware; well-standardized (FIPS 180-4) but no advantage here |
| SHA-3 (Keccak-256) | Slower than BLAKE3; no advantage; primarily useful for FIPS 202 compliance |
| BLAKE2b | Predecessor to BLAKE3; BLAKE3 is strictly faster and has a simpler streaming API |
| CRC32/Adler32 | Not cryptographic; trivially forgeable; only useful for hardware error detection |
| xxHash | Non-cryptographic; fast; not collision-resistant against adversarial input |

### VerifiedBlob Type Safety

The `VerifiedBlob` newtype is a zero-cost mechanism:
```rust
pub struct VerifiedBlob(Vec<u8>);  // opaque — only constructible by verify_checksum
pub fn decrypt_chunk(blob: VerifiedBlob, ...) -> Result<Vec<u8>, CryptoError>;
```
This enforces the check-before-decrypt order at compile time, making it impossible to call `decrypt_chunk` on an unverified blob.

---

## Memory Protection: ZeroizeOnDrop + Secret\<T\>

### Selected: `zeroize` crate (ZeroizeOnDrop) + `secrecy` crate (Secret\<T\>)

All key types in Arx Runa implement `ZeroizeOnDrop`, which overwrites the backing memory with zeros when the value is dropped. The `Secret<T>` wrapper (from the `secrecy` crate) prevents accidental logging or debug-printing of sensitive data.

### Why Explicit Zeroization is Necessary

In Rust, the compiler is free to elide "dead stores" — writes to memory that are never subsequently read before the memory is freed. A naive `let mut key = [0u8; 32]; key.copy_from_slice(key_material);` followed by `key.fill(0)` may have the zeroing optimized away. The `zeroize` crate uses `core::ptr::write_volatile` (or OS-level APIs on supported platforms) to ensure the zeroing is not elided.

### `Secret<T>` Benefits

- Implements `Debug` as `Secret([REDACTED])`, preventing key material from appearing in logs
- Does not implement `Display`, `Serialize`, or `Clone` by default
- Forces the programmer to explicitly call `.expose_secret()` when the value is needed, making accidental exposure visible in code review

### Alternatives Considered

| Alternative | Why rejected |
|---|---|
| Manual `ptr::write_volatile` | Correct but verbose; easy to forget; `zeroize` is the standard Rust approach |
| OS-level `SecureZeroMemory` / `explicit_bzero` | Platform-specific; `zeroize` wraps these when available |
| Garbage-collected languages | GC languages cannot guarantee when (or if) memory is zeroed — sensitive data may linger in the heap |
| `memsec` crate | Provides `mlock` + zeroize; considered but `zeroize` + `secrecy` is more composable |

### mlock / VirtualLock

The design notes that session keys are held in mlocked memory (preventing swap-out to disk). This is a separate concern from zeroization:
- **Zeroization** (`zeroize`): ensures key bytes are overwritten when Rust drops the value
- **mlock**: prevents the OS from paging the memory to disk while the session is active

Cold boot attacks (reading DRAM after power-off) and compromised OS kernels are out of scope.

---

## Recommendation

All six primitives in the Arx Runa cryptographic design are well-justified:

1. **XChaCha20-Poly1305** is the correct choice for a random-nonce AEAD in a system that cannot manage counter state. The 192-bit nonce eliminates birthday-bound concerns at any practical volume. The primary future consideration is AEGIS-256 once it reaches RFC status.

2. **HKDF-SHA256** is the standard and correct choice for key expansion from high-entropy material. The use of distinct `info` strings provides cryptographic domain separation between all derived keys.

3. **Argon2id** is the current gold standard for password-based key derivation, with RFC 9106, OWASP, and NIST SP 800-63B all recommending it. The OWASP minimum parameters are appropriate for interactive vault unlock.

4. **Per-file random keys** with `key_encryption_key` wrapping is the correct architecture for limiting blast radius and enabling future key rotation and file sharing. It follows the same pattern used by commercial encrypted storage systems (e.g., LUKS, VeraCrypt, age).

5. **BLAKE3 checksums** over encrypted blobs are correct for fast pre-decryption corruption detection. Unkeyed is operationally sufficient because the hash is stored inside a SQLCipher-encrypted manifest. The `VerifiedBlob` newtype provides a compile-time safety guarantee.

6. **`ZeroizeOnDrop` + `Secret<T>`** is the Rust-ecosystem standard for sensitive key material. It addresses both the compiler-elision problem (volatile writes) and the accidental-logging problem (`Debug` redaction).

No changes to the design are recommended based on this research. The one open consideration is monitoring AEGIS-256 for RFC completion as a future upgrade to XChaCha20-Poly1305.

---

## Decisions

> Choices made during this research session. Updated as the session progresses.

| Decision | Alternatives considered | Rationale |
|---|---|---|
| Argon2id parameters: 64 MiB, 3 iterations, parallelism 4 | OWASP minimum (19 MiB / 2 / 1); 1Password-tier (650 MiB / 3 / 4) | OWASP recommended tier; matches Bitwarden and KeePassXC; ~300–500 ms on modern desktop; significantly stronger against GPU attackers than the minimum |
| Phase 5 file sharing uses CTX-ChaCha20-Poly1305 as committing AEAD | AES-GCM-SIV (not committing), AEGIS-256-MAC (draft only), UtC prefix | XChaCha20-Poly1305 is non-committing — partition oracle attacks are theoretically possible in multi-key ECIES contexts; CTX construction (Chan & Rogaway, IACR 2022) replaces Poly1305 tag with BLAKE3 commitment, achieving CMT-4 security; decided in file-sharing-cryptography research |

---

## Open Questions

- **AEGIS-256 readiness**: When does `draft-irtf-cfrg-aegis-aead` reach RFC status? When will an independent Rust crate audit be available? These are the two gates before it can replace XChaCha20-Poly1305.
- **Key commitment cipher**: Resolved — CTX-ChaCha20-Poly1305 selected for Phase 5 HPKE share packages. See `docs/research/file-sharing-cryptography.md`.

---

## Sources

| Source | Topic | URL |
|---|---|---|
| RFC 8439 — ChaCha20 and Poly1305 for IETF Protocols (2018) | ChaCha20-Poly1305 specification | https://www.rfc-editor.org/rfc/rfc8439 |
| draft-irtf-cfrg-xchacha-03 — XChaCha: eXtended-nonce ChaCha and AEAD_XChaCha20_Poly1305 (Arciszewski, 2020) | XChaCha20 specification; Section 3.1 contains verbatim birthday bound figures | https://datatracker.ietf.org/doc/html/draft-irtf-cfrg-xchacha-03 |
| Bernstein — "Extending the Salsa20 nonce" (SKEW 2011) | Security proof that extended-nonce construction is secure under base cipher assumptions | http://cr.yp.to/snuffle/xsalsa-20110204.pdf |
| NIST SP 800-38D — Recommendation for Block Cipher Modes: GCM (2007) | AES-GCM specification and nonce requirements | https://csrc.nist.gov/pubs/sp/800/38/d/final |
| RFC 8452 — AES-GCM-SIV (2019) | Nonce-misuse resistant AEAD | https://www.rfc-editor.org/rfc/rfc8452 |
| draft-irtf-cfrg-aegis-aead — The AEGIS Family of Authenticated Encryption Algorithms | AEGIS-256 candidate for future upgrade | https://datatracker.ietf.org/doc/draft-irtf-cfrg-aegis-aead/ |
| Böck, Zauner, Devlin, Somorovsky, Jovanovic — "Nonce-Disrespecting Adversaries: Practical Forgery Attacks on GCM in TLS" (USENIX WOOT 2016) | AES-GCM nonce reuse catastrophe demonstrated against real TLS implementations | https://eprint.iacr.org/2016/475 |
| Chan & Rogaway — "On Committing Authenticated Encryption" (IACR 2022) | AEAD key non-commitment; committing AE (cAE) framework and CTX construction | https://eprint.iacr.org/2022/1260 |
| RFC 5869 — HMAC-based Key Derivation Function (HKDF) (2010) | HKDF specification | https://www.rfc-editor.org/rfc/rfc5869 |
| NIST SP 800-56C Rev 2 — Two-Step Key Derivation (2020) | HKDF as NIST-approved KDF | https://csrc.nist.gov/pubs/sp/800/56/c/r2/final |
| RFC 8446 — TLS 1.3 (2018) | HKDF-SHA256 use in production protocol | https://www.rfc-editor.org/rfc/rfc8446 |
| NIST FIPS 180-4 — Secure Hash Standard (SHA-2) (2015) | SHA-256 specification | https://csrc.nist.gov/pubs/fips/180-4/upd1/final |
| RFC 9106 — Argon2 Memory-Hard Function (Biryukov, Dinu, Khovratovich, Josefsson; 2021) | Argon2id specification; Section 4 verbatim recommends 64 MiB / t=3 / p=4 | https://www.rfc-editor.org/rfc/rfc9106 |
| Biryukov, Dinu, Khovratovich — "Argon2: New Generation of Memory-Hard Functions" (IEEE EuroS&P 2016) | Theoretical AT product analysis; tradeoff-attack reduction factor ≤ 1.33× for Argon2id | https://ieeexplore.ieee.org/document/7467361 |
| OWASP Password Storage Cheat Sheet (2024) | Argon2id recommended parameters for interactive applications | https://cheatsheetseries.owasp.org/cheatsheets/Password_Storage_Cheat_Sheet.html |
| NIST SP 800-63B — Digital Identity Guidelines (2017, updated 2024) | Password-based authentication and KDF recommendations | https://pages.nist.gov/800-63-4/sp800-63b.html |
| RFC 7914 — scrypt (2016) | scrypt specification (rejected alternative to Argon2id) | https://www.rfc-editor.org/rfc/rfc7914 |
| NIST SP 800-57 Part 1 Rev. 5 — Recommendation for Key Management (2020) | Section 6.2 defines DEK/KEK hierarchy and "limited exposure" rationale for key wrapping | https://csrc.nist.gov/publications/detail/sp/800-57-part-1/rev-5/final |
| LUKS On-Disk Format Specification (Fruhwirth et al.; v1.2.3 / LUKS2 v1.1.4) | Production precedent: per-keyslot key wrapping so passphrase compromise does not compromise volume key | https://gitlab.com/cryptsetup/LUKS2-docs |
| Linux fscrypt documentation (kernel.org) | Production precedent: per-file key derivation for cryptographic isolation in filesystems | https://docs.kernel.org/filesystems/fscrypt.html |
| Krawczyk — "The Order of Encryption and Authentication for Protecting Communications" (CRYPTO 2001) | Proves Encrypt-then-MAC is the only generically secure ordering; INT-CTXT via check-before-decrypt | https://eprint.iacr.org/2001/045 |
| Bellare & Namprempre — "Authenticated Encryption: Relations among Notions and Analysis of the Generic Composition Paradigm" (ASIACRYPT 2000 / JoC 2008) | Defines INT-CTXT; proves Encrypt-then-MAC achieves it; foundational for AEAD ordering | https://eprint.iacr.org/2000/025 |
| O'Brien & Paterson — "Security of Symmetric Encryption against Mass Surveillance" (IACR 2013) | Key isolation properties under mass surveillance threat model | https://eprint.iacr.org/2013/130 |
| BLAKE3 — "BLAKE3: One Function, Fast Everywhere" (O'Connor, Aumasson, Neves, Wilcox-O'Hearn; 2020) | BLAKE3 design, performance, and security properties | https://github.com/BLAKE3-team/BLAKE3-specs/blob/master/blake3.pdf |
| Aumasson et al. — BLAKE2 specification (2012) | BLAKE2 predecessor to BLAKE3 | https://www.blake2.net/blake2.pdf |
| zeroize crate documentation | Compiler-resistant memory zeroing via volatile writes in Rust | https://docs.rs/zeroize |
| secrecy crate documentation | Secret\<T\> / SecretBox wrapper with ExposeSecret trait; Debug redaction | https://docs.rs/secrecy |
