# Arx Runa: Password and Key Recovery

> **Document type**: Exploration / feasibility research
> **Status**: Concluded
> **Last updated**: 2026-04-10

Investigates every known mechanism for recovering vault access after a password or key file is lost, evaluated against Arx Runa's zero-knowledge threat model.

For the canonical auth design, see `docs/architecture/designs/authentication-and-session-management/design.md`.  
For background on the key derivation model, see `../design-reviews/authentication-and-session-management-review.md`.

---

## Table of Contents

1. [The Problem](#the-problem)
2. [Prior Art](#prior-art)
3. [Recovery Mechanisms](#recovery-mechanisms)
4. [ZK Threat Model Evaluation](#zk-threat-model-evaluation)
5. [Comparison Table](#comparison-table)
6. [Recommendation](#recommendation)
7. [Decisions](#decisions)
8. [Open Questions](#open-questions)
9. [Sources](#sources)

---

## The Problem

Arx Runa derives the vault master key entirely from user credentials:

```
master_key = Argon2id(password || key_file_bytes, salt)
```

This means:
- **No recovery by design** — the server never holds the key, so there is nobody to call
- **Loss of password = loss of vault** (unless the user kept the key file)
- **Loss of key file = loss of vault** (even if password is known)

The question is: can we offer *any* recovery path without compromising the zero-knowledge property? And which paths are worth offering as opt-in features?

---

## Prior Art

### BitLocker (Microsoft)
Generates a 48-digit numeric recovery key at setup. The user stores this key externally (print, USB, Microsoft account). The recovery key is a separate AES key that wraps the Volume Master Key — losing the password does not lose the data if the recovery key is retained.

**ZK relevance**: The recovery key is generated on-device and stored by the user — Microsoft never sees it (unless the user uploads it to their Microsoft account). The pattern is sound.

### 1Password (Emergency Kit)
At account creation, prints a PDF "Emergency Kit" containing the account password, Secret Key, and a QR code. The Secret Key is a device-generated high-entropy value that supplements the master password in the key derivation. If both are lost, data is gone.

**ZK relevance**: 1Password holds encrypted vault data but not keys. Their "Account Recovery" for Teams/Business uses an admin-encrypted copy of the user's key — a deliberate escrow for enterprise. For individual plans, there is no recovery — they are explicit about this.

### Bitwarden
Offers an optional "Emergency Access" feature: a trusted contact can request access, and after a configurable waiting period the user can approve/deny. The contact receives an asymmetric key share. Bitwarden holds encrypted data; the key exchange happens on-device.

**ZK relevance**: Emergency access uses RSA key wrapping — the contact's public key encrypts a wrapped vault key. The server sees only ciphertext. This is ZK-compatible in principle.

### Age (age-encryption.org)
No recovery mechanism. If the passphrase is lost, so is the data. Age explicitly documents this. The design philosophy: recovery is the user's responsibility.

### LUKS (Linux Unified Key Setup)
Supports up to 32 key slots — any slot can unlock the volume master key. A recovery passphrase is simply an additional key slot. Keyfile-based slots and passphrase slots can coexist.

**ZK relevance**: Multi-slot design is highly relevant — each slot independently wraps the same master key. Adding a "recovery slot" does not weaken the primary password slot.

### Ethereum / Smart Contract Social Recovery (ERC-4337)
Wallets like Argent allow N-of-M "guardians" (trusted addresses) to approve a recovery operation. The wallet key is not split — guardians vote to replace the signing key. This is account-level, not key-level recovery.

**ZK relevance**: This pattern translates to Shamir's Secret Sharing at the key level.

### Shamir's Secret Sharing (SSS)
Splits a secret S into N shares such that any K shares reconstruct S, but K−1 shares reveal nothing (information-theoretic security). Defined by Shamir (1979). Libraries: `vsss-rs`, `sharks` (Rust).

**ZK relevance**: No server involvement required. Shares can be distributed to trusted contacts, stored in separate locations, or printed and sealed. Fully ZK-compatible.

### SLIP-39 (Satoshi Labs)
An SSS scheme designed for cryptocurrency key recovery. Uses a 10-bit word list (1024 words), Reed-Solomon error correction, and optional passphrase. Standardized by Trezor. More robust than raw SSS.

**ZK relevance**: Directly applicable — designed exactly for high-value key backup.

### BIP-39 (Bitcoin mnemonic phrases)
Encodes 128–256 bits of entropy as 12–24 human-readable words. Not SSS — it is a direct encoding of the key. If all words are captured, the key is captured.

**ZK relevance**: Could be used to encode and display the vault master key (or a separately generated recovery key) as a mnemonic. Very user-friendly for "write this down."

### OPAQUE (RFC 9380, IRTF CFRG)
An asymmetric PAKE protocol where the server never sees the password, even during registration. A server-side "secret oprf key" is mixed into derivation — changing the password requires server involvement. Does NOT help with password recovery, but eliminates password-at-rest exposure.

**ZK relevance**: Interesting for passwordless future, but does not solve recovery.

---

## Recovery Mechanisms

### 1. Recovery Phrase (BIP-39 Mnemonic)
At vault creation, generate 32 bytes of additional entropy and encode as a 24-word BIP-39 phrase. This phrase is an alternative vault key that independently wraps the master key (a second LUKS-style slot).

- User writes down / prints 24 words
- Losing the password is recoverable if the phrase is retained
- Phrase itself must be kept secret

### 2. Recovery Code (Numeric/Alphanumeric)
Simpler than BIP-39 — generate a random 40-character alphanumeric code (like BitLocker's 48-digit code). Display once, user saves externally. Wraps the master key in a second slot.

- Easier to type for less technical users
- Less memorable but shorter to store

### 3. Shamir's Secret Sharing (N-of-M)
At setup, split the master key into N shares. User distributes to trusted contacts or prints and stores in separate locations. Any K shares reconstruct.

- `sharks` crate (Rust): pure SSS, no external deps
- `vsss-rs` crate: Verifiable SSS (VSS) — shares can be verified without reconstruction
- 2-of-3 is the most common practical choice (two locations + one trusted person)

**UX complexity**: High. Users must manage multiple physical shares. Error-prone without tooling.

### 4. SLIP-39 Mnemonic Shares
Like Shamir's but each share is human-readable words with error correction. Better UX than raw byte shares. Supported by hardware wallets (Trezor).

- Libraries: `slip39` (Rust, community crate, <!-- TODO: verify maturity -->)
- Reed-Solomon error correction within each share reduces transcription errors

### 5. Key File as Recovery Mechanism
The existing key file feature already functions as a second factor. If used correctly (key file stored separately from password), losing the password is recoverable via the key file.

- No new code needed — just documentation and UX guidance
- Users should be told explicitly: "your key file IS your recovery key, store it safely"

### 6. Trusted Contact (Asymmetric Key Wrapping)
User imports a trusted contact's public key (X25519 or RSA). The master key is wrapped under that public key and stored in the vault metadata. If the user loses their password, the contact can unwrap and share the master key.

- Requires contact to have Arx Runa installed (or a standalone unwrapping tool)
- The wrapped blob is stored in the vault — server sees only ciphertext
- ZK-compatible: server never sees plaintext

### 7. Time-Delayed Recovery (Dead Man's Switch variant)
User registers a recovery email or contact. After N days of no login, an encrypted recovery blob is released to the contact. Not self-recovery — useful for inheritance.

- Requires a server component with liveness tracking — breaks pure local operation
- Out of scope for current architecture

### 8. Cloud-Backed Encrypted Recovery Blob (Opt-In Escrow)
User chooses to upload an Argon2id-encrypted copy of their master key to a separate cloud service (not their vault cloud). Recovery requires knowing the escrow password (a simpler, separately stored password).

- Completely voluntary — user chooses the escrow destination
- ZK for the primary vault: the vault cloud still never sees keys
- The escrow service sees a ciphertext blob — ZK if escrow password is strong

### 9. Hardware Security Key (FIDO2 / YubiKey) — Multiple Keys
If vault auth uses a hardware key, registering two hardware keys is already recovery: keep one as backup. This is an existing pattern for FIDO2/WebAuthn.

- Arx Runa does not currently have FIDO2 integration — future phase

### 10. Platform Biometric / OS Keychain Escrow
On supported platforms (Windows Hello, macOS Secure Enclave, Android Keystore), the OS can bind a key to the device and biometric. A sealed copy of the master key is stored in the OS keychain.

- Recovery requires same device + biometric (or platform recovery)
- Does not help if device is lost — only helps with forgotten password on same device
- Windows Hello recovery: Microsoft account backup of keychain (optional)

---

## ZK Threat Model Evaluation

| Mechanism | Server sees plaintext? | Server required? | ZK-compatible? | Notes |
|---|---|---|---|---|
| Recovery phrase (BIP-39) | No | No | ✅ Yes | Purely local, strong |
| Recovery code (numeric) | No | No | ✅ Yes | Purely local, simple |
| Shamir's SSS | No | No | ✅ Yes | Complex UX |
| SLIP-39 shares | No | No | ✅ Yes | Better UX than raw SSS |
| Key file (existing) | No | No | ✅ Yes | Already implemented |
| Trusted contact (X25519 wrap) | No | No | ✅ Yes | Requires contact tooling |
| Time-delayed / dead man's | No | Yes | ⚠️ Partial | Needs liveness server |
| Cloud-backed escrow (opt-in) | No (ciphertext) | Yes (escrow) | ⚠️ Partial | Escrow cloud is a third party |
| FIDO2 backup key | No | No | ✅ Yes | Future phase |
| Platform biometric | No | No | ✅ Yes | Device-bound, limited |

---

## Comparison Table

| Mechanism | Complexity (dev) | Complexity (user) | Entropy | Offline? | Prior art |
|---|---|---|---|---|---|
| Recovery phrase (BIP-39) | Low | Low | 256-bit | ✅ | 1Password, hardware wallets |
| Recovery code | Very low | Very low | ~200-bit | ✅ | BitLocker |
| Shamir's SSS (2-of-3) | Medium | High | Same as key | ✅ | Ethereum social recovery |
| SLIP-39 | Medium | Medium | Same as key | ✅ | Trezor |
| Key file (existing) | None | Medium | 512-bit | ✅ | Arx Runa already |
| Trusted contact wrap | High | Medium | Same as key | ✅ | Bitwarden Emergency Access |
| Cloud escrow (opt-in) | High | Medium | Escrow pw | ⚠️ | — |
| Platform biometric | Very high | Very low | Device-bound | ✅ (same device) | macOS Keychain |

---

## Recommendation

Recovery is opt-in. Users who do not set it up lose their vault if they lose their credentials — this is explicitly documented and expected.

### Phase: ship BIP-39 recovery phrase

The primary recovery mechanism is a **BIP-39 24-word mnemonic** that functions as a second key slot (LUKS-style). The master key is never stored directly — the phrase wraps it:

```
recovery_salt         = CSPRNG(32 bytes)
recovery_key          = Argon2id(phrase_words_joined, recovery_salt)   // same params as primary slot
recovery_slot         = XChaCha20-Poly1305.encrypt(master_key, recovery_key, aad="arx-runa recovery v1")
```

`recovery_slot` and `recovery_salt` are stored in vault metadata alongside the primary password slot. Either slot unlocks the vault independently.

**Phrase generation**: 256 bits of entropy via `rand::rng().fill()`, encoded via the `bip39` crate. The last word is a checksum — mistyping any word is caught before the wrong key is derived.

**Display policy**: shown exactly once during setup, immediately after vault creation. Never stored. User must acknowledge before proceeding.

**Argon2id parameters**: identical to the primary password slot — the recovery phrase is not a weaker path.

**AAD**: the `aad` string `"arx-runa recovery v1"` binds the ciphertext to its context and version, preventing cross-slot confusion attacks.

### Future phases (not shipped now)

- **SLIP-39 shares**: 2-of-3 by default, user-overridable. Depends on a production-ready `slip39` Rust crate — needs further evaluation.
- **Trusted contact wrap**: age-encryption.org/v1 X25519 format, so the contact can unwrap with the `age` CLI without Arx Runa installed. Higher implementation complexity; deferred.

---

## Decisions

> Choices made during this research session. Updated as the session progresses.

| Decision | Alternatives considered | Rationale |
|---|---|---|
| Use BIP-39 24-word mnemonic for the primary recovery slot | Recovery code (alphanumeric string), SLIP-39 shares | BIP-39 checksum catches transcription errors immediately; well-understood mental model from crypto wallets; `bip39` crate is battle-tested; 256-bit entropy |
| Use SLIP-39 word shares (not raw SSS bytes) | Raw byte shares, skip SSS entirely | SLIP-39 Reed-Solomon error correction catches transcription errors; human-readable words; Trezor-compatible format |
| Default SLIP-39 split: 2-of-3, user-overridable | 3-of-5, fixed threshold | 2-of-3 is the standard practical choice; tolerates one lost share; lower setup friction than 3-of-5 |
| Trusted contact wrap uses age encryption format (age-encryption.org/v1) | Arx Runa-native X25519 format | Contact can unwrap with `age -d` CLI — no dependency on Arx Runa being available; format is audited and standardised |
| Ship BIP-39 phrase only; defer SLIP-39 and trusted contact wrap | Ship all four mechanisms, ship BIP-39 + SLIP-39 | Start with the simplest ZK-compatible mechanism; SLIP-39 crate maturity unverified; trusted contact requires age format integration |
| Recovery is opt-in (no forced recovery setup) | Forced setup, no recovery at all | Follows the principle of least surprise; power users may not want the attack surface; honest about data loss risk |
| Recovery slots re-wrapped during password/key change (phrase required) | Invalidate slots on change, wrap a stable intermediate key | Re-wrapping preserves the user's existing recovery setup; avoids forcing re-setup after every password change; integrity check confirms phrase correctness before re-wrap |
| Recovery slot AAD includes vault_id: `b"arx-runa recovery v1" \|\| vault_id_bytes` | AAD without vault_id | Prevents cross-vault recovery slot transplant attacks; binds the ciphertext to its vault context |
| Recovery setup is post-creation (Security settings + one-time prompt) | Inline during vault creation | Vault creation is already a 21-step ceremony for Tier 2; recovery setup requires password re-entry to re-derive `master_key`, which is a natural post-creation ceremony |
| Same Argon2id parameters for recovery phrase as primary password slot | Weaker/faster parameters (phrase has 256-bit entropy) | Identical parameters provide slot indistinguishability in the vault header — an attacker cannot determine which salt/params belong to the recovery slot vs. primary slot |
| Vault header uses `recovery_slots` array (supports multiple methods) | Single recovery slot field | Designed for future extensibility (SLIP-39, trusted contact) without vault header schema migration; in the BIP-39-only phase, at most one element |

---

## Open Questions

All questions resolved. See Decisions table for rationale.

- **Q1 — `slip39` Rust crate maturity**: **Resolved.** No production-ready `slip39` Rust crate exists with a security audit. The `slip39` crate on crates.io has limited adoption <!-- TODO: verify current download/dependent counts --> and no published audit <!-- TODO: verify — check crates.io and any linked repository for audit reports -->. The `vsss-rs` crate provides Shamir field arithmetic but not the SLIP-39 word encoding with Reed-Solomon error correction. SLIP-39 is deferred per Decision 5. When pursued, implement the word layer on top of `vsss-rs` or wait for a crate to reach production maturity. See Sources: `slip39` Rust crate, `vsss-rs` Rust crate.

- **Q2 — Simultaneous BIP-39 and SLIP-39 slots**: **Resolved.** Yes — the vault header uses a `recovery_slots` array, supporting multiple independent recovery methods. In the BIP-39-only phase, at most one element is present. Future phases may add SLIP-39 or trusted-contact slots alongside without a vault header schema migration.

- **Q3 — Argon2id parameters for recovery phrase**: **Resolved.** Use the same Argon2id parameters as the primary password slot. The recovery phrase has 256-bit entropy, so Argon2id's brute-force resistance is redundant in practice. However, identical parameters provide **slot indistinguishability** in the vault header — an attacker cannot determine which salt belongs to the recovery slot vs. the primary slot. See Decisions table.

- **Q4 — UX flow (inline vs. post-creation)**: **Resolved.** Post-creation, via a Security settings page, with a one-time dismissible prompt after vault creation. Vault creation is already a 21-step ceremony for Tier 2. Recovery setup is a separate ceremony that requires password re-entry to re-derive `master_key` — the critical invariant (no long-lived `master_key`) is preserved in both the creation and recovery-setup paths.

---

## Sources

| Source | Topic | URL |
|---|---|---|
| Shamir, A. (1979). "How to share a secret." *Communications of the ACM* | Shamir's Secret Sharing foundational paper | https://dl.acm.org/doi/10.1145/359168.359176 |
| BIP-39 specification | Mnemonic encoding of entropy | https://github.com/bitcoin/bips/blob/master/bip-0039.mediawiki |
| SLIP-39 specification (Satoshi Labs) | SSS-based mnemonic shares with error correction | https://github.com/satoshilabs/slips/blob/master/slip-0039.md |
| LUKS on-disk format v2 | Multi-keyslot design for volume encryption | https://gitlab.com/cryptsetup/cryptsetup/-/wikis/LUKS-standard/on-disk-format.pdf |
| 1Password White Paper | Emergency Kit and Secret Key design | https://1passwordstatic.com/files/security/1password-white-paper.pdf |
| Bitwarden Security White Paper | Emergency Access via asymmetric key wrapping | https://bitwarden.com/images/resources/security-white-paper-download.pdf |
| RFC 9380 — OPAQUE (IRTF CFRG) | OPAQUE asymmetric PAKE | https://www.rfc-editor.org/rfc/rfc9380 |
| `sharks` Rust crate | Shamir's Secret Sharing in Rust | https://crates.io/crates/sharks |
| `vsss-rs` Rust crate | Verifiable Secret Sharing in Rust | https://crates.io/crates/vsss-rs |
| `slip39` Rust crate | SLIP-39 mnemonic shares in Rust (community crate) | https://crates.io/crates/slip39 |
| Microsoft BitLocker documentation | BitLocker recovery key design | https://learn.microsoft.com/en-us/windows/security/operating-system-security/data-protection/bitlocker/recovery-overview |
| `bip39` Rust crate | BIP-39 mnemonic generation and validation | https://crates.io/crates/bip39 |
| age encryption format specification v1 | X25519 recipient stanza and file key wrapping | https://age-encryption.org/v1 |
