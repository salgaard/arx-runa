# Arx Runa: Authentication and Session Management — Cryptographic Decisions

> **Document type**: Exploration / feasibility research
> **Status**: Living document
> **Last updated**: 2026-04-12

Justification and alternative analysis for the cryptographic decisions in the Phase 2 authentication and session management design: BIP-39 recovery phrase generation and security properties, Argon2id construction for the recovery slot (separate salt, slot indistinguishability via shared parameters), session key memory protection (mlock/VirtualLock scope and guarantees), BLAKE3 key file fingerprinting, Tier 2 input construction, and non-oracular authentication error semantics.

For recovery mechanism selection (BIP-39 vs. SLIP-39 vs. Shamir's SSS), see `password-and-key-recovery.md` — that document covers why BIP-39 was chosen over alternatives. This document covers *how* BIP-39 is used correctly once chosen.  
For the canonical Phase 2 design, see `docs/architecture/designs/authentication-and-session-management/design.md`.  
For the underlying cryptographic primitives (Argon2id parameters, HKDF, XChaCha20-Poly1305), see `cryptographic-primitive-rationale.md`.

---

## Table of Contents

1. [The Problem](#the-problem)
2. [BIP-39 Recovery Phrase: Encoding and Security Properties](#bip-39-recovery-phrase-encoding-and-security-properties)
3. [Recovery Slot Argon2id Construction](#recovery-slot-argon2id-construction)
4. [Session Key Memory Protection: mlock / VirtualLock](#session-key-memory-protection-mlock--virtuallock)
5. [BLAKE3 Key File Fingerprint in Vault Header](#blake3-key-file-fingerprint-in-vault-header)
6. [Tier Input Construction](#tier-input-construction)
7. [Non-Oracular Authentication Errors](#non-oracular-authentication-errors)
8. [Recommendation](#recommendation)
9. [Decisions](#decisions)
10. [Open Questions](#open-questions)
11. [Sources](#sources)

---

## The Problem

Phase 2 introduces several cryptographic sub-decisions that sit below the level of primitive selection but above the level of pure implementation detail. Each has alternatives that look reasonable at first glance but introduce concrete weaknesses:

- **BIP-39 phrase derivation**: BIP-39 defines both an encoding format *and* a key derivation step (PBKDF2-HMAC-SHA512). Using both layers would impose two sequential KDFs. Using neither would lose the checksum. The question is where to split.
- **Recovery slot KDF parameters**: should the recovery phrase — which has 256-bit entropy — use weaker Argon2id parameters than the primary password slot, or the same? The two options have opposite trade-offs.
- **mlock scope**: mlock prevents swap-out, but has OS limits, failure modes, and a defined scope. The design must specify what exactly is locked, what happens when locking fails, and what threats mlock does *not* mitigate.
- **BLAKE3 key file hash in the vault header**: the vault header is public (stored in the cloud). What does exposing `blake3(key_file)` leak, and is BLAKE3 the right function here?
- **Tier 2 input concatenation**: simple concatenation `password_bytes || key_file_bytes` is ambiguous in general but Tier 2 fixes the key file to 32 bytes. Is this safe, and what were the alternatives?
- **Non-oracular errors**: standard security practice, but the design must also handle the `KeyFileNotFound` case without revealing password status — a subtle edge.

---

## BIP-39 Recovery Phrase: Encoding and Security Properties

### Selected: BIP-39 24-word mnemonic as entropy encoding + checksum carrier; Argon2id for key derivation

BIP-39 (Bitcoin Improvement Proposal 39, Palatinus et al. 2013) defines a two-layer specification:

1. **Encoding layer**: encode raw entropy bytes as human-readable words using a fixed 2048-word English wordlist
2. **Derivation layer**: derive a 512-bit HD wallet seed via `PBKDF2-HMAC-SHA512(mnemonic, "mnemonic" + optional_passphrase, 2048 rounds)`

Arx Runa uses **layer 1 only** and replaces **layer 2** with Argon2id. This is a deliberate split that requires justification.

### Encoding Layer: Wordlist, Entropy, and Checksum

The BIP-39 English wordlist contains exactly 2048 words. Since log₂(2048) = 11, each word encodes 11 bits of information. The BIP-39 spec defines the relationship between entropy (ENT) bits, checksum (CS) bits, and word count (WC):

| ENT (bits) | CS (bits) | WC (words) | Total bits |
|---|---|---|---|
| 128 | 4 | 12 | 132 |
| 160 | 5 | 15 | 165 |
| 192 | 6 | 18 | 198 |
| 224 | 7 | 21 | 231 |
| **256** | **8** | **24** | **264** |

For a 24-word phrase: 256 bits of entropy produce 24 words × 11 bits = 264 bits. The final 8 bits are the first 8 bits of `SHA-256(entropy_bytes)` — the checksum.

**Arx Runa uses ENT = 256 (24 words)**. This is the maximum supported entropy level in BIP-39 and provides 256 bits of pre-image resistance, matching the 256-bit security level used throughout the system.

**Checksum value**: The last word of a 24-word BIP-39 phrase encodes the final 3 bits of entropy plus the 8-bit SHA-256 checksum. Any single-word transcription error is caught by checksum validation with probability at least 255/256 ≈ 99.6%. The BIP-39 spec requires implementations to reject phrases failing this check — Arx Runa validates the phrase and returns `InvalidRecoveryPhrase` immediately, before any Argon2id derivation runs.

### NFKD Normalization: English Wordlist Safety and Future Risk

The BIP-39 spec requires NFKD normalization in two places:

1. Wordlist encoding: "native characters must be encoded in UTF-8 NFKD" — applies to non-English wordlists
2. Seed derivation: "mnemonic sentence (in UTF-8 NFKD)" as PBKDF2 input — applies when using the standard BIP-39 PBKDF2 layer

The `bip39` Rust crate's `Mnemonic::to_string()` (`Display` impl) returns words joined by ASCII spaces with **no normalization applied** — normalization is deferred to the `to_seed()` / `normalize_utf8_cow()` methods which are oriented toward the PBKDF2 layer.

**For the English wordlist (Arx Runa v1)**: all 2048 English BIP-39 words are lowercase ASCII letters (a–z). ASCII code points (U+0000–U+007F) have no Unicode canonical or compatibility decompositions — they are already in NFC, NFD, NFKC, and NFKD form. `normalize_utf8_cow()` on an ASCII string returns the identical byte sequence. Normalization is a **no-op** for English, and the current design is safe.

**Future risk**: if Arx Runa ever adds non-English wordlist support (Japanese, Korean, Chinese, French — all of which contain characters with Unicode decompositions), passing the raw `to_string()` output to Argon2id without explicit NFKD normalization would be a correctness bug. Two systems that store the same mnemonic with different Unicode representations would derive different `recovery_key` values, silently locking the user out.

**Mitigation for future wordlists**: apply explicit NFKD normalization to `mnemonic.to_string()` before passing to Argon2id, using the `unicode-normalization` crate's `nfkd()` function. This is a no-op for ASCII/English and ensures correctness for all future wordlists.

### Why the BIP-39 Derivation Layer is Not Used

BIP-39's derivation layer uses `PBKDF2-HMAC-SHA512` with only 2048 iterations. This is not a design flaw in BIP-39 — the HD wallet derivation purpose does not require memory-hardness because the derived *seed* is a 512-bit random value that is further processed by BIP-32 before any user-meaningful key material is produced, and HD wallet seeds are not typically brute-forced directly.

For Arx Runa's recovery slot, the phrase *is* directly processed into a vault key. Two concerns arise if the BIP-39 PBKDF2 layer were also applied:

1. **Layered KDFs**: the phrase would pass through `PBKDF2-HMAC-SHA512(phrase, "mnemonic", 2048)` → 512-bit intermediate → `Argon2id(intermediate, salt, params)`. The Argon2id step works on pre-processed material rather than the raw phrase. The security analysis is more complex (though not broken — Argon2id's memory-hardness is independent of the entropy quality of its input, since we control the salt).
2. **No benefit**: PBKDF2's 2048 iterations add negligible brute-force resistance compared to Argon2id's 64 MiB / 3 iterations. The combined cost is effectively just Argon2id.

The design therefore uses BIP-39 encoding **only** for its wordlist and checksum properties, and passes the space-joined mnemonic string directly to Argon2id:

```
input = phrase_words_space_joined  // e.g. "word1 word2 ... word24"
recovery_key = Argon2id(input, recovery_salt, m=65536, t=3, p=4)
```

This is not a novel pattern — it is the same approach used by the Trezor wallet's SLIP-39 implementation when an optional passphrase is added: the raw mnemonic is passed to the KDF, not first converted via PBKDF2.

### Wordlist Selection

BIP-39 also defines wordlists in other languages (Japanese, Korean, Spanish, Chinese Simplified/Traditional, French, Italian, Czech, Portuguese). Arx Runa uses the **English wordlist** exclusively for v1. Rationale:

- English wordlist is the most widely deployed and tested (all hardware wallets, most software wallets, `bip39` Rust crate default)
- Multi-language support requires wordlist normalization (BIP-39 Japanese, for example, requires specific Unicode normalization per the spec) — additional complexity for a v1 feature
- English provides the widest compatibility for manual transcription verification across tools

### Alternatives to BIP-39 Encoding

| Alternative | Why not selected |
|---|---|
| BIP-39 with PBKDF2 derivation layer | Double KDF with no benefit; PBKDF2 adds complexity without security gain over Argon2id alone |
| SLIP-39 share encoding repurposed for single key | SLIP-39 is designed for *shares*, not for a single-secret encoding; the word set and error-correction framing adds complexity for single-secret use |
| Raw 32-byte hex or Base58 recovery code | No error correction; one transposition error silently produces the wrong key; BIP-39 checksum catches errors cheaply |
| Diceware (EFF wordlist) | No checksum; variable-entropy encoding; less library support in Rust |
| Alphanumeric recovery code (BitLocker-style) | Lower entropy density per character vs BIP-39 words; no error detection built in |

---

## Recovery Slot Argon2id Construction

### Selected: Separate CSPRNG salt; same Argon2id parameters as primary password slot

The recovery slot derives `recovery_key` via:

```
recovery_entropy  = CSPRNG(32 bytes)       // never stored; used only to generate the phrase
recovery_phrase   = bip39::encode(recovery_entropy)
recovery_salt     = CSPRNG(32 bytes)        // stored in vault header, recovery_slots[0].argon2_salt
recovery_key      = Argon2id(phrase_words_space_joined, recovery_salt, m=65536, t=3, p=4)
```

Two specific decisions require justification: (a) using a **separate** salt rather than the primary vault salt, and (b) using the **same** Argon2id parameters rather than reduced parameters.

### Why a Separate Salt (Not the Primary Vault Salt)

Salt reuse with a different password is a foundational KDF error. If `Argon2id(password_A, salt)` and `Argon2id(password_B, salt)` used the same salt, an attacker building a pre-computation table for one input would partially reduce work against the other. Even when the inputs are unrelated (a human password vs. a 256-bit mnemonic), mixing them under a single salt is unsafe in principle.

More concretely: if the vault header exposed a single salt used for both the primary password slot and the recovery slot, an attacker learning that the slot is a recovery slot would know the exact same salt was used for both derivations. The vault header already contains this distinction in the `recovery_slots[0].argon2_salt` field; using a separate salt is the consistent and correct design.

NIST SP 800-132 §5.1 states: "The salt shall be generated randomly and shall be at least 128 bits in length" — it does not prohibit per-slot salts, but the multi-slot model (LUKS, VeraCrypt) universally uses independent salts per slot precisely to maintain each slot's independence.

### Why the Same Argon2id Parameters (Slot Indistinguishability)

At first glance, the recovery phrase has 256-bit entropy — far above any threshold where Argon2id's brute-force resistance matters. An attacker who obtains the vault header would need to try 2^256 candidate phrases to brute-force the recovery slot regardless of Argon2id's parameters. One could argue: use weaker parameters (faster, less memory) since the phrase has intrinsic entropy.

**The counterargument: slot indistinguishability.** The vault header contains:

```json
{
  "argon2_salt": "<primary-salt-base64>",
  "argon2_params": { "memory_cost": 65536, "time_cost": 3, "parallelism": 4 },
  "recovery_slots": [{
    "argon2_salt": "<recovery-salt-base64>",
    "argon2_params": { "memory_cost": 65536, "time_cost": 3, "parallelism": 4 },
    "wrapped_master_key": "<ciphertext-base64>"
  }]
}
```

If the recovery slot used different parameters — say `m=4096, t=1, p=1` — the vault header itself would *announce* the presence and location of the recovery slot. An attacker would immediately know which salt/params pair corresponds to the weaker, phrase-derived slot and which corresponds to the password slot. They could then target the password slot with full cost and confirm correct derivation by checking the recovery slot first (at negligible cost).

With **identical parameters**, both salt/params pairs in the vault header are computationally identical from the attacker's perspective. The attacker cannot determine which is the primary password slot and which is the recovery slot without attempting both. This provides a concrete, though not game-changing, advantage in the attacker's uncertainty about the vault structure.

The cost of same parameters is acceptable: a recovery authentication takes the same wall-clock time (~300–500 ms on modern hardware) as a normal login. The phrase will rarely be used; the latency is not a UX concern.

### Alternatives Considered

| Alternative | Trade-off |
|---|---|
| Same salt as primary vault (no separate recovery salt) | Unsafe — salt must be unique per secret; violates NIST SP 800-132 |
| Reduced parameters for recovery slot (m=4096, t=1) | Slot indistinguishability is lost; vault header announces recovery slot location |
| Higher parameters for recovery slot | Unnecessary; 256-bit phrase entropy already makes brute force infeasible; longer unlock time with no benefit |
| Argon2i instead of Argon2id for recovery phrase (phrase is 256-bit uniform entropy) | Argon2i is designed for side-channel resistance against co-located attackers; Argon2id is uniformly recommended (RFC 9106 §4) for all new uses; no reason to introduce a different variant |

---

## Session Key Memory Protection: mlock / VirtualLock

### Selected: mlock (Linux) / VirtualLock (Windows) on SessionKeys fields; hard failure on lock error

Session keys (`key_encryption_key`, `sqlcipher_key`, `manifest_key`) are 32 bytes each — 96 bytes total. They are held in `SessionKeys`, which is held behind `Arc<RwLock<Option<SessionKeys>>>`. The memory-protection strategy has three layers:

1. **mlock/VirtualLock**: pins pages into physical RAM (prevents swap)
2. **ZeroizeOnDrop**: overwrites memory with zeros on drop (Rust `zeroize` crate)
3. **Secret\<T\>**: prevents accidental `Debug` / logging exposure (`secrecy` crate)

### What mlock / VirtualLock Provides

**POSIX `mlock(2)`** (Linux, macOS): Locks a range of virtual address pages into physical RAM. The OS will not swap these pages to disk (no pagefile / swapfile writes). Defined in POSIX.1-2008.

**Windows `VirtualLock`**: Equivalent to `mlock` on Windows. Locks a region of the process's virtual address space into physical memory. The documentation states: "memory will not be paged to the paging file while it is locked."

Both APIs operate at the page granularity (4 KiB on x86-64). Locking 96 bytes locks at least one full 4 KiB page.

**What mlock prevents**:
- The OS writing key material to the pagefile / swapfile / hibernation file while the session is active
- A later offline attacker reading key material from a pagefile dump

**What mlock does NOT prevent**:
- **Cold boot attacks**: DRAM retains charge for seconds to minutes after power loss at low temperatures (Halderman et al., USENIX Security 2008). mlock cannot prevent memory reads from physical hardware access.
- **Privileged OS memory access**: root / Administrator can read any process's memory via `/proc/<pid>/mem` (Linux) or `ReadProcessMemory` (Windows). mlock provides no protection against a compromised kernel.
- **Memory dumps / core files**: if the process crashes and dumps core, mlocked memory may appear in the dump. Arx Runa should disable core dumps in production builds.
- **Hibernation**: on Windows, hibernating while Arx Runa has a session open may capture mlocked pages in `hiberfil.sys`. This is OS-level behavior outside Arx Runa's control.

The design explicitly documents: "Cold boot attacks (reading DRAM after power-off) and compromised OS kernels are out of scope." This scoping is correct — those threats require hardware-level protection beyond what software can provide.

### mlock Scope in Arx Runa

Only the backing memory of `SessionKeys` fields is locked — not the entire process heap. The `Secret<[u8; 32]>` type from the `secrecy` crate uses heap allocation; the mlock call must target the exact address of each field's backing allocation.

Two approaches:

1. **Field-level mlock**: call `mlock(field_ptr, 32)` for each `Secret<[u8; 32]>` after construction. The OS rounds up to page granularity — typically one 4 KiB page per field, or 12 KiB total for three fields. <!-- TODO: verify whether secrecy crate's Secret<T> guarantees a stable heap address before mlock is called -->
2. **Struct-level mlock**: allocate `SessionKeys` in a single mlocked slab. The `memsec` crate provides `malloc_secure()` / `free_secure()` which allocate and mlock in one operation.

The design currently specifies approach 1 (field-level). This is correct but requires care: the address passed to `mlock` must be the address of the actual key bytes inside the `Secret<T>` box, not the address of the `Secret<T>` itself.

### OS Limits

**Linux**: Unprivileged processes are subject to `RLIMIT_MEMLOCK` (the maximum number of bytes lockable into RAM). The default on most distributions is 65536 bytes (64 KiB) per process (`man 2 mlock`, Linux 4.6+). Three 32-byte keys = 96 bytes, well within 64 KiB. However, because mlock operates at page granularity, three separate allocations may lock three separate 4 KiB pages = 12 KiB total — still comfortably within limits.

If `RLIMIT_MEMLOCK` is set to a lower value (some older hardened systems use 0), `mlock` returns `EPERM`. The design handles this with a hard fail + actionable error message.

**Windows**: `VirtualLock` does **not** require `SeLockMemoryPrivilege` or administrator rights. The Microsoft documentation does not mention any privilege requirement for `VirtualLock` at all. `SeLockMemoryPrivilege` ("Lock pages in memory" Group Policy) is required only for `VirtualAlloc` with `MEM_LARGE_PAGES` — an entirely different operation used for large-page allocations, not for locking ordinary heap memory.

`VirtualLock` claims pages from the process's minimum working set. The MS docs state: "The maximum number of pages that a process can lock is equal to the number of pages in its minimum working set minus a small overhead." The failure condition is `ERROR_WORKING_SET_QUOTA` — working set exhaustion — not a privilege check. For 96 bytes of key material, the locked footprint is at most 3 × 4 KiB = 12 KiB, far below the default process minimum working set (typically ~200 KiB on Windows 10/11). In practice, `VirtualLock` will not fail for Arx Runa's key material on any normally operating Windows 10/11 installation, regardless of user account type.

> **Design defect**: The design's Windows error message — "Run Arx Runa as administrator or adjust the 'Lock pages in memory' policy" — is incorrect. `SeLockMemoryPrivilege` is irrelevant to `VirtualLock`. The guidance would mislead users: standard user accounts can call `VirtualLock` successfully. If `VirtualLock` does fail (working set exhaustion, which would be extraordinary for 96 bytes), the correct message is: "Cannot lock session keys in memory (system working set quota exceeded). Try closing other applications or restarting Arx Runa."

### Hard Failure on mlock Error

The design mandates hard failure: if `mlock`/`VirtualLock` fails, Arx Runa refuses to create the session. The rationale is explicitly stated: "a security product that silently degrades its memory protection is not trustworthy."

This is the correct position. Production precedents:

- **OpenSSH**: `ssh-agent` uses `mlock` for private keys and refuses to operate if memory locking is unavailable on platforms where it's expected
- **GnuPG**: warns on mlock failure but continues — the Arx Runa approach is stricter, which is appropriate for a tool whose entire value proposition is cryptographic protection

### Alternatives Considered

| Alternative | Why rejected |
|---|---|
| Soft-fail on mlock (warn, continue) | Silent degradation — keys may be swapped to disk without the user's knowledge; incompatible with the zero-knowledge security promise |
| Lock entire process address space (`mlockall`) | Locks all mapped memory including code, stack, and heap; causes significant memory pressure; far exceeds any realistic RLIMIT |
| `memsec` crate (`malloc_secure` + `free_secure`) | Provides a single mlock+alloc operation; considered but adds an extra dependency; field-level mlock with `zeroize` is more composable and transparent |
| Disable swap (system-wide) | Not under application control; affects the entire system; requires elevated privileges |

---

## BLAKE3 Key File Fingerprint in Vault Header

### Selected: Unkeyed BLAKE3 hash of 32-byte key file, stored in vault header

At Tier 2 vault creation, `blake3::hash(key_file_bytes)` is computed and stored as `key_file_blake3` in the vault header JSON. The vault header is stored in the cloud (publicly readable). This decision requires verifying that exposure of `blake3(key_file)` does not weaken the authentication model.

### Security Analysis

The key file is 32 bytes (256 bits) of CSPRNG output. An attacker who obtains the vault header learns `blake3(key_file)`.

**Threat: preimage attack (recover key_file from its hash)**  
BLAKE3 is a cryptographic hash function providing 256-bit preimage resistance (the "One Function, Fast Everywhere" paper, O'Connor et al. 2020, proves this under the random oracle model). Recovering the 256-bit key file from its 256-bit hash is computationally infeasible. The attacker gains nothing.

**Threat: verification oracle (check if a candidate key file matches)**  
An attacker with a candidate 32-byte key file can compute `blake3(candidate)` and compare to `key_file_blake3`. This is equivalent to attempting authentication with that candidate — the attacker could also just run Argon2id with the candidate and see if the master key decrypts the vault. The BLAKE3 hash provides no additional advantage over direct authentication attempts.

**Threat: birthday collision attack**  
If the attacker could find *any* 32-byte string that hashes to `key_file_blake3`, they could impersonate the key file. BLAKE3 provides 128-bit collision resistance (birthday bound for a 256-bit output). Finding a collision requires ~2^128 operations — computationally infeasible.

**Conclusion**: Storing `blake3(key_file)` in the vault header does not weaken authentication. It is strictly a UX mechanism (auto-detection: scan a mounted USB for 32-byte files, compare their BLAKE3 hashes to `key_file_blake3`).

### Why BLAKE3 and Not a Keyed MAC?

A keyed MAC would require storing a secret (the MAC key) somewhere accessible before the vault is unlocked — circular dependency, since the vault header must be readable *before* key derivation. BLAKE3 as an unkeyed hash sidesteps this: it's a public operation on a public value (from the vault header's perspective) that provides sufficient preimage resistance for the 256-bit key file.

### Alternatives Considered

| Alternative | Trade-off |
|---|---|
| SHA-256 | Slower; same security properties for this use case; no advantage |
| HMAC-SHA256 with vault_id as key | Keyed MAC; no advantage — vault_id is also public; complicates the computation without adding security |
| Storing the key file path | Would tie the vault to a specific filesystem path; breaks portability to new machines; leaks metadata about the user's file system |
| Not storing any fingerprint | Auto-detection impossible — user must manually select the key file on every login and every new device |

---

## Tier Input Construction

### Selected: Tier 1: `password_utf8_bytes`; Tier 2: `password_utf8_bytes || key_file_bytes`

Arx Runa's two authentication tiers produce different Argon2id inputs:

```
Tier 1:  argon2_input = password_utf8_bytes
Tier 2:  argon2_input = password_utf8_bytes || key_file_bytes   (raw concatenation)
```

### Why Simple Concatenation is Unambiguous for Tier 2

In general, simple `A || B` concatenation is ambiguous: given the combined input, you cannot recover the split point without additional information (the classic length-extension problem). However, Tier 2's design eliminates this ambiguity:

- The key file is **always exactly 32 bytes** by design (generated once, never variable-length)
- The split point in the combined input is always `total_length - 32`
- Argon2id does not return `(password, key_file)` separately — the combined byte sequence is the password argument; the internal split point is never needed during derivation

Length-prefixing (e.g., `len(password_bytes) || password_bytes || key_file_bytes`) would add complexity for a non-existent ambiguity. Hashing the password before concatenating (e.g., `SHA-256(password) || key_file_bytes`) would destroy Argon2id's memory-hard processing of the raw password — Argon2id should receive the raw secret to maximize the work factor against offline attackers.

### Why Not HKDF Combination or a Pre-Mix KDF?

An alternative would be to combine the two inputs via HKDF or BLAKE3 keyed hash before Argon2id:

```
combined = BLAKE3(key=key_file_bytes, data=password_utf8_bytes)
master_key = Argon2id(combined, salt, params)
```

This is overengineered. The combined input must be the *secret* processed by the memory-hard function. Simple concatenation achieves this: Argon2id receives all secret bits (password bits and key file bits) directly. No additional mixing step provides security benefit in this construction.

### Tier 1 vs Tier 2 Distinguishability

A Tier 1 vault and a Tier 2 vault both have `"tier": 1` or `"tier": 2` in the vault header (plaintext). An attacker reading the vault header already knows the tier. There is no benefit to making the input construction indistinguishable between tiers — the tier is public metadata needed to determine whether to prompt for a key file.

---

## Non-Oracular Authentication Errors

### Selected: `InvalidCredentials` for wrong password, wrong key file, or both; `KeyFileNotFound` only for missing hardware; other errors are named and distinct

The `AuthenticationError` enum uses `InvalidCredentials` for all cases where supplied credentials are incorrect but present:

```rust
InvalidCredentials,        // wrong password, wrong key file, or wrong combination
KeyFileNotFound,           // no 32-byte file matches key_file_blake3 on any mounted drive
MemoryLockFailed(String),  // mlock/VirtualLock rejected
VaultHeaderInvalid,        // header is corrupt or missing
InvalidRecoveryPhrase,     // BIP-39 checksum validation failed (fast, pre-KDF check)
NoRecoverySlot,            // recovery_slots is empty
```

### Why `InvalidCredentials` Must Not Distinguish Factors

If `AuthenticationError` had separate variants for wrong password vs. wrong key file, an attacker who can observe the error (e.g., via a side channel, a network-facing API, or a compromised frontend) could perform a two-stage attack:

1. Fix the key file to a candidate value; vary passwords until no `WrongPassword` error is returned → password found
2. Fix the confirmed password; vary key files until no `WrongKeyFile` error → key file found

The combined attack is drastically cheaper than simultaneously brute-forcing both factors. A non-oracular `InvalidCredentials` forces the attacker to vary both factors simultaneously — the product of their respective search spaces.

This is the same rationale as login systems returning "incorrect username or password" rather than distinguishing the two — the principle is well-established (OWASP Authentication Cheat Sheet §2.3).

### The `KeyFileNotFound` Edge Case

`KeyFileNotFound` is returned when *no* file matching `key_file_blake3` is found on any mounted drive — not when a file is found but the resulting Argon2id output is wrong. This case reveals:

- The user is interacting with a Tier 2 vault (already in the vault header)
- No matching key file is currently present

It does **not** reveal whether the password is correct, because Argon2id derivation has not been attempted. The password status is unknown at this point. An attacker who induces a `KeyFileNotFound` response learns only "the key file hardware is not present" — not "the password I tested is correct or incorrect." This is acceptable: the vault tier is already public.

If Arx Runa were to perform Argon2id with a placeholder key file on `KeyFileNotFound` and return `InvalidCredentials`, it would potentially create a timing oracle (Argon2id is time-expensive; the difference between "skipped KDF" and "ran KDF" is measurable). Returning `KeyFileNotFound` immediately is both more informative to the user *and* avoids this timing concern.

### `InvalidRecoveryPhrase` Timing

`InvalidRecoveryPhrase` is returned when BIP-39 checksum validation fails — *before* Argon2id runs. This is a fast path (SHA-256 computation + checksum comparison). An attacker cannot use the presence of this error to learn anything about `recovery_key`, since the KDF has not been run. The only information leaked is: "this string is not a valid BIP-39 mnemonic."

---

## Recommendation

All six decisions in the Phase 2 design are well-justified:

1. **BIP-39 24-word mnemonic (encoding layer only, not the PBKDF2 derivation layer)** is the correct choice. The 2048-word wordlist provides 11 bits/word; 24 words = 256-bit entropy + 8-bit SHA-256 checksum. The checksum catches transcription errors cheaply before any expensive KDF. The BIP-39 derivation layer (PBKDF2-HMAC-SHA512) is intentionally bypassed — Argon2id directly on the mnemonic string is both simpler and more memory-hard than a compound PBKDF2 → Argon2id chain.

2. **Separate recovery salt with same Argon2id parameters** is the correct choice. A separate salt is required for KDF correctness (one secret per salt). Same parameters provide slot indistinguishability: the vault header reveals neither which salt belongs to the recovery slot nor the entropy properties of the corresponding secret.

3. **mlock/VirtualLock with hard failure** is the correct choice for a desktop security application. The 96 bytes of key material are well within OS limits. Soft-failure would silently expose keys to swap — incompatible with Arx Runa's security model. Cold boot and kernel-compromise threats are correctly documented as out of scope.

4. **Unkeyed BLAKE3 hash of key file in vault header** is correct. BLAKE3 is preimage-resistant for a 256-bit input. The hash enables auto-detection without leaking any information beyond what an attacker could obtain by attempting authentication with candidate files.

5. **Raw concatenation for Tier 2 input** is correct and unambiguous because the key file is a fixed-length 32-byte secret. No length prefix is needed; no pre-mix KDF is needed.

6. **Non-oracular `InvalidCredentials`** is the correct error design, consistent with OWASP guidance. The `KeyFileNotFound` carve-out is safe because it does not reveal password status — KDF has not been attempted at that point.

One design defect was identified during this research:

> **Design defect — Windows `VirtualLock` error message**: The design's error message "Run Arx Runa as administrator or adjust the 'Lock pages in memory' policy" is incorrect. `SeLockMemoryPrivilege` has no bearing on `VirtualLock` — it is required only for `VirtualAlloc(MEM_LARGE_PAGES)`. The correct message is: "Cannot lock session keys in memory (system working set quota exceeded). Try closing other applications or restarting Arx Runa." This must be corrected in `docs/architecture/designs/authentication-and-session-management/design.md` before the session management section is implemented.

All other decisions are sound. Three implementation notes for the Phase 2.2 implementer:
- Apply explicit NFKD normalization before Argon2id if non-English BIP-39 wordlists are ever added
- `mlock` must target the inner `[u8; 32]` address inside `Secret<T>`, not the outer wrapper's stack slot
- Windows hibernation (`hiberfil.sys`) may capture mlocked pages — document as a known limitation or mitigate via power plan API

---

## Decisions

> Choices made during this research session. Updated as the session progresses.

| Decision | Alternatives considered | Rationale |
|---|---|---|
| BIP-39 encoding layer only; skip BIP-39 PBKDF2 derivation layer | Use both BIP-39 layers (PBKDF2 then Argon2id); skip BIP-39 entirely | BIP-39 encoding provides checksum (SHA-256) and human-readable words; PBKDF2 layer is redundant alongside Argon2id and would add complexity; the mnemonic string is passed directly to Argon2id |
| 24-word phrase (256-bit entropy) | 12-word phrase (128-bit entropy) | 256-bit entropy matches the security level used throughout Arx Runa; BIP-39 max; future-proof |
| Separate CSPRNG salt per recovery slot | Reuse primary vault Argon2id salt | Salt must be unique per secret (NIST SP 800-132 §5.1); reuse would link the two derivations and allow pre-computation cross-attack |
| Same Argon2id parameters (m=65536, t=3, p=4) for recovery slot | Reduced parameters (phrase has 256-bit entropy) | Slot indistinguishability: identical params mean the vault header does not reveal which slot is recovery vs. primary |
| mlock hard failure | Soft-fail (warn + continue) | A security product that silently skips memory protection is not trustworthy; 96 bytes of key material is well within OS mlock limits |
| BLAKE3 as key file fingerprint (unkeyed) | Keyed HMAC, SHA-256, no fingerprint | Keyed MAC requires secret key unavailable before login; SHA-256 is slower with same security for this use; no fingerprint removes auto-detection capability |
| Simple concatenation `password || key_file` for Tier 2 | HKDF pre-mix, length-prefixed concatenation | Key file is always 32 bytes (fixed length); split point is deterministic; pre-mixing destroys Argon2id's direct memory-hard processing of raw secrets |
| `KeyFileNotFound` is a distinct error from `InvalidCredentials` | Return `InvalidCredentials` for missing key file | Returning `KeyFileNotFound` before attempting KDF is safe (no password status leaked) and avoids a timing oracle that a fake Argon2id run would introduce |
| No explicit NFKD normalization of mnemonic before Argon2id (v1, English only) | Apply `unicode-normalization::nfkd()` unconditionally | English BIP-39 wordlist is ASCII-only; NFKD normalization is a no-op for ASCII; `bip39::Mnemonic::to_string()` output is safe to pass directly to Argon2id; normalization must be added if non-English wordlists are ever supported |
| Windows VirtualLock error message must be corrected | Keep design as-is | `SeLockMemoryPrivilege` is only for `VirtualAlloc(MEM_LARGE_PAGES)`, not for `VirtualLock`; standard user accounts can call `VirtualLock` without any privilege; failure condition is working set exhaustion (ERROR_WORKING_SET_QUOTA), not a privilege error; design error message gives wrong remediation advice |
| Argon2id salt size: 32 bytes for all slots (primary and recovery) | 16 bytes (OWASP/NIST minimum) | Consistent with the 256-bit security level used throughout Arx Runa; exceeds NIST SP 800-132 minimum of 128 bits; no additional cost |
| `InvalidRecoveryPhrase` validated before Argon2id (fast pre-KDF path) | Attempt Argon2id regardless and return `InvalidCredentials` on decryption failure | BIP-39 checksum validation is a cheap SHA-256 operation; failing fast avoids an unnecessary 300–500 ms Argon2id computation on a typo; returning a distinct error does not leak key material because no KDF has run |

---

## Open Questions

- **mlock and `Secret<T>` heap stability**: `Secret<[u8; 32]>` from the `secrecy` crate box-allocates its inner value. The `mlock` call must target the address of the inner `[u8; 32]`, not the `Secret<T>` stack pointer. Confirm via code review that the mlock implementation correctly calls `expose_secret()` on the reference before `mlock(ptr, 32)` — a single offset error would lock the wrong page. <!-- TODO: verify in implementation -->
- **Windows error message (design defect — must fix)**: **Resolved.** `VirtualLock` requires no privilege whatsoever. `SeLockMemoryPrivilege` is for `VirtualAlloc(MEM_LARGE_PAGES)` only. The design's error message "Run Arx Runa as administrator or adjust the 'Lock pages in memory' policy" gives wrong remediation guidance and should be corrected to: "Cannot lock session keys in memory (system working set quota exceeded). Try closing other applications or restarting Arx Runa." In practice, `VirtualLock` for 96 bytes will essentially never fail on Windows 10/11.
- **Hibernation file exposure**: On Windows, `hiberfil.sys` may capture mlocked memory during hibernation. Investigate whether Arx Runa should disable hibernation during an active session (via `SetSystemPowerState` / power plan API) or document this as a known limitation. <!-- TODO: verify whether Windows excludes locked pages from hibernation image -->
- **NFKD normalization for future non-English wordlists**: For English (v1), normalization is a no-op (all words are ASCII). If non-English wordlists are ever added, the implementation must apply explicit NFKD normalization to `mnemonic.to_string()` before Argon2id — the `bip39` crate's `Display` impl does not normalize. This is a design note for the implementer, not a current defect.

---

## Sources

| Source | Topic | URL |
|---|---|---|
| BIP-39 specification (Palatinus, Rusnak, Voisine, Bowe; 2013) | BIP-39 wordlist, entropy/checksum table, PBKDF2 derivation layer | https://github.com/bitcoin/bips/blob/master/bip-0039.mediawiki |
| RFC 9106 — Argon2 Memory-Hard Function (Biryukov, Dinu, Khovratovich, Josefsson; 2021) | Argon2id specification; Section 4 recommended parameters | https://www.rfc-editor.org/rfc/rfc9106 |
| NIST SP 800-132 — Recommendation for Password-Based Key Derivation (2010) | Salt requirements (§5.1: randomly generated, ≥128 bits); per-slot salt independence | https://csrc.nist.gov/publications/detail/sp/800-132/final |
| OWASP Authentication Cheat Sheet (2024) | Non-oracular error messages (§2.3): "incorrect username or password" | https://github.com/OWASP/CheatSheetSeries/blob/master/cheatsheets/Authentication_Cheat_Sheet.md |
| POSIX.1-2008 — mlock(2) | POSIX specification of mlock; page-level granularity; RLIMIT_MEMLOCK | https://pubs.opengroup.org/onlinepubs/9699919799/functions/mlock.html |
| Linux man page mlock(2) | RLIMIT_MEMLOCK default (65536 bytes for unprivileged processes on Linux 4.6+) | https://man7.org/linux/man-pages/man2/mlock.2.html |
| Microsoft — VirtualLock function | Windows VirtualLock specification; failure condition is working set quota (ERROR_WORKING_SET_QUOTA), not a privilege check; no SeLockMemoryPrivilege required | https://learn.microsoft.com/en-us/windows/win32/api/memoryapi/nf-memoryapi-virtuallock |
| Microsoft — Large-Page Support | SeLockMemoryPrivilege is exclusively for VirtualAlloc(MEM_LARGE_PAGES), not for VirtualLock; confirms privilege separation | https://learn.microsoft.com/en-us/windows/win32/memory/large-page-support |
| Microsoft — Working Set | Default process minimum working set (~200 KiB); VirtualLock claims pages from the minimum working set | https://learn.microsoft.com/en-us/windows/win32/memory/working-set |
| Halderman et al. — "Lest We Remember: Cold Boot Attacks on Encryption Keys" (USENIX Security 2008) | Cold boot attack against DRAM after power-off; scope of mlock protection | https://www.usenix.org/legacy/event/sec08/tech/full_papers/halderman/halderman.pdf |
| O'Connor, Aumasson, Neves, Wilcox-O'Hearn — "BLAKE3: One Function, Fast Everywhere" (2020) | BLAKE3 design and security properties (256-bit preimage resistance) | https://github.com/BLAKE3-team/BLAKE3-specs/blob/master/blake3.pdf |
| `bip39` Rust crate (v2.2.2) | BIP-39 mnemonic generation and validation; `to_string()` Display impl returns space-joined words without NFKD normalization; `to_seed()` applies normalization for the PBKDF2 layer | https://docs.rs/bip39/latest/bip39/ |
| `unicode-normalization` Rust crate | NFKD and other Unicode normal forms; `nfkd()` function; recommended if non-English BIP-39 wordlists are ever added | https://docs.rs/unicode-normalization/latest/unicode_normalization/ |
| `secrecy` crate documentation | Secret\<T\> wrapper with ExposeSecret trait; Debug redaction; heap allocation | https://docs.rs/secrecy |
| `zeroize` crate documentation | ZeroizeOnDrop; volatile write zeroing | https://docs.rs/zeroize |
| SLIP-39 specification (Satoshi Labs) | Mnemonic encoding for multi-share SSS; PBKDF2 passphrase derivation (alternative to BIP-39 derivation layer) | https://github.com/satoshilabs/slips/blob/master/slip-0039.md |
| OWASP Password Storage Cheat Sheet (2024) | Argon2id recommended parameters; salt recommendations | https://cheatsheetseries.owasp.org/cheatsheets/Password_Storage_Cheat_Sheet.html |
| OpenSSH source — ssh-agent mlock usage | Production precedent: ssh-agent refuses to operate without mlock for private key storage | https://github.com/openssh/openssh-portable/blob/master/ssh-agent.c |
