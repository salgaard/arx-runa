# Arx Runa: MFA Methods for Vault Authentication and Recovery

> **Document type**: Exploration / feasibility research
> **Status**: Concluded
> **Last updated**: 2026-04-10

Investigates whether authenticator-app TOTP and national digital-identity systems (MitID) can serve as a second factor or recovery mechanism for Arx Runa vault authentication, evaluated against the zero-knowledge threat model.

For background on the key derivation model and session management, see `authentication-and-session-management-review.md`.  
For recovery mechanisms (BIP-39, Shamir's SSS), see `password-and-key-recovery.md`.  
For the canonical auth design, see `docs/architecture/designs/authentication-and-session-management/design.md`.

---

## Table of Contents

1. [The Problem](#the-problem)
2. [How TOTP Works](#how-totp-works)
3. [How MitID Works](#how-mitid-works)
4. [TOTP as a Vault Unlock Factor](#totp-as-a-vault-unlock-factor)
5. [TOTP as a Recovery Mechanism](#totp-as-a-recovery-mechanism)
6. [MitID as an Auth or Recovery Mechanism](#mitid-as-an-auth-or-recovery-mechanism)
7. [ZK Threat Model Evaluation](#zk-threat-model-evaluation)
8. [Comparison Table](#comparison-table)
9. [Recommendation](#recommendation)
10. [Decisions](#decisions)
11. [Open Questions](#open-questions)
12. [Sources](#sources)

---

## The Problem

Arx Runa unlocks a vault via:

```
master_key = Argon2id(password || key_file_bytes, salt)
```

The question is whether adding a **time-based one-time password (TOTP) second factor** (e.g., Google Authenticator, Aegis, Authy) or a **national digital identity service** (MitID — the Danish e-ID used for banking and government services) as an additional authentication or recovery factor makes sense in this architecture — and whether doing so preserves the zero-knowledge property.

This is a non-trivial question because TOTP and national ID systems are designed for client-server architectures where a *server* validates credentials. Arx Runa has no server — only the user's own cloud storage (which stores encrypted blobs). The threat model, key storage location, and validation logic must all be reconsidered from first principles.

---

## How TOTP Works

TOTP (RFC 6238) is an extension of HOTP (RFC 4226). A shared secret `K` is provisioned out-of-band (QR code scan) between the authenticator app and the verifying party. At authentication time, both parties independently compute:

```
TOTP(K, t) = HOTP(K, floor(unix_time / 30))
HOTP(K, c) = Truncate(HMAC-SHA1(K, c)) mod 10^digits
```

The verifier checks if the user-presented code matches the locally-computed value (with a ±1 step window for clock drift).

**Critical structural properties:**
- The shared secret `K` must exist on **both sides** — the app and the verifier
- The verifier checks codes but never knows which specific code the user will present next
- `K` is provisioned once and never changes (unless the user re-registers)
- Loss of `K` on the verifier side = second factor is gone
- Loss of `K` on the app side = second factor is inaccessible until re-provisioned

TOTP is standardized in RFC 6238 (2011) and is the basis for virtually all authenticator apps. Most implementations use HMAC-SHA1; some support HMAC-SHA256 or HMAC-SHA512.

---

## How MitID Works

MitID is the Danish national digital identity system, introduced in 2021 to replace NemID. It is operated by the Danish Agency for Digitisation (Digitaliseringsstyrelsen) and Nets A/S under a public-private partnership.

**Key properties:**
- Tied to a physical Danish national identity (CPR number)
- Requires registration with a government-approved identity provider
- Authentication flows: MitID app (push notification), physical chip card, audio code device
- Used for Danish banking, government services, tax portal, healthcare
- Network-dependent — requires connectivity to Digitaliseringsstyrelsen servers to authenticate
- Identity correlation: every authentication event is logged by the provider
- Available only to Danish citizens/residents with a valid CPR number

**Protocol**: MitID uses OpenID Connect (OIDC) and OAuth 2.0 over HTTPS. The relying party (the service wanting to authenticate the user) receives an ID token from the MitID identity provider. The flow is server-mediated — the relying party must register with MitID and maintain server-side credentials.

---

## TOTP as a Vault Unlock Factor

### The structural problem

For TOTP to protect vault unlock, two things must be true simultaneously:
1. The TOTP secret `K` must be accessible to the verifier (Arx Runa) so it can compute the expected code
2. The TOTP secret `K` must NOT be accessible to an attacker who has compromised the vault

These two requirements are in direct tension in a local application. There are only three places `K` can live:

| Storage location | Can Arx Runa verify TOTP? | Can an attacker extract K? | Notes |
|---|---|---|---|
| Inside the encrypted vault (unlocked by password) | Only after vault is unlocked — too late | No (vault is encrypted) | Circular: you unlock to get K, so K can't protect unlock |
| Outside the vault (plaintext on disk) | Yes | Yes — trivially | Useless: attacker with vault file also gets K |
| Outside the vault (encrypted under a separate key) | Only if user knows that key | Only with that key | Adds another factor, but that factor is now the real second factor, not TOTP |

**The circularity problem**: If `K` is stored inside the vault, TOTP can only be verified *after* the vault is decrypted. This means TOTP cannot protect the vault decryption itself — it could only enforce an additional check after unlock, which is a different threat model (session policy, not vault security).

**The exposure problem**: If `K` is stored outside the vault in plaintext, an attacker who steals the vault file (the usual threat) also steals `K` — TOTP provides no protection.

### The narrow threat model where TOTP does help

TOTP as a local second factor only provides security if the attacker obtains the **vault master password** (or the Argon2id-derived key) but **not** the authenticator device. This scenario:

- Requires the attacker to learn the password (social engineering, keylogger, shoulder-surf) without also taking the phone
- Is more typical in corporate settings (shared workstations, IT admin access) than personal use
- Does NOT protect against: stolen laptop with vault + stolen phone, memory forensics of unlocked session, vault file exfiltration followed by offline dictionary attack

### Prior art: how password managers handle this

| Application | TOTP role | Storage of TOTP secret |
|---|---|---|
| Bitwarden | Second factor for server login, NOT vault decryption | Server-validated; client never holds K |
| 1Password | No TOTP for vault unlock; TOTP only for account login (server) | Server-side |
| KeePassXC | Stores TOTP entries (generates codes for sites), does NOT use TOTP to unlock | TOTP for stored sites, not the database lock |
| Vault (HashiCorp) | TOTP auth supported for server access, not for key unsealing | Server-validated |

The pattern is consistent: **TOTP is used for server authentication, not for protecting local encrypted data**. No mainstream password manager or encryption tool uses TOTP to protect vault decryption, because TOTP is structurally incompatible with pure client-side key derivation.

---

## TOTP as a Recovery Mechanism

TOTP cannot function as a recovery mechanism for two reasons:

1. **Codes are ephemeral** — a 6-digit code valid for 30 seconds cannot encode the master key or serve as a second key slot. There is no way to "recover" a vault by entering a TOTP code.
2. **The TOTP secret `K` itself cannot be used for recovery** — `K` is a ~20-byte shared secret (usually Base32-encoded, 128–160 bits). If a user lost their password, they could theoretically use `K` as a key-derivation input — but this is just "use a separately stored secret as a recovery key," which is exactly what the existing key file and BIP-39 recovery phrase already do, with better-defined semantics and prior art.

**Verdict**: TOTP is architecturally incompatible with local vault recovery.

---

## MitID as an Auth or Recovery Mechanism

### Why MitID is structurally incompatible

MitID is a federated identity system. Every authentication operation requires:

1. A live HTTPS connection to Digitaliseringsstyrelsen/Nets servers
2. The user's CPR number (national ID) — a persistent, linkable identity
3. The relying party (Arx Runa) to be a registered OIDC client with MitID

This breaks Arx Runa's zero-knowledge properties in multiple ways:

**Network dependency**: Vault unlock requires internet access. The vault becomes inaccessible if MitID servers are down, the user is offline, or the user is outside Denmark and MitID connectivity is blocked.

**Identity linkage**: Every vault unlock would correlate a CPR number with a vault access event. If the MitID provider's logs are subpoenaed, the government/operator knows when and how often each user accesses their vault.

**Third-party trust**: The MitID operator (Nets A/S + Danish Agency) becomes a trust anchor for vault access. A government order to Nets could block a specific CPR number from accessing their vault.

**Not universally available**: MitID is available only to Danish residents with a CPR number. Arx Runa targets a general audience.

**Centralization**: If MitID shuts down (service discontinued, operator change), all vaults tied to MitID authentication become permanently inaccessible unless a fallback mechanism exists.

### MitID as a recovery mechanism — even worse

Using MitID to recover a vault (e.g., "prove your identity to unlock a recovery code") requires:
- Arx Runa to act as a registered OIDC relying party with persistent server infrastructure
- A server-side mapping from CPR number to encrypted recovery blob
- The server to decrypt or hand over the recovery material upon successful MitID auth

This is a **custodial recovery model** — the recovery server holds (encrypted) key material and releases it on identity verification. This is the opposite of zero-knowledge. It is essentially the pattern used by online banks (your bank holds your money, MitID proves it's you), transported to a ZK encryption tool where it fundamentally does not belong.

**Verdict**: MitID is architecturally incompatible with Arx Runa's zero-knowledge model in any role.

---

## ZK Threat Model Evaluation

| Mechanism | Requires server? | Requires network? | Exposes identity? | ZK-compatible? | Notes |
|---|---|---|---|---|---|
| TOTP (K inside vault) | No | No | No | ⚠️ Partial | Circular — K accessible only after vault opens |
| TOTP (K outside vault, plaintext) | No | No | No | ❌ No | Attacker who has vault file also has K |
| TOTP (K in OS keychain) | No | No | No | ⚠️ Partial | Adds OS-bound factor; narrow threat model |
| MitID (auth factor) | Yes | Yes | Yes (CPR) | ❌ No | Federated identity, logs all access |
| MitID (recovery) | Yes | Yes | Yes (CPR) | ❌ No | Custodial recovery — server holds key material |

---

## Comparison Table

| Approach | Protects vault decryption? | ZK-compatible? | Offline? | Universal? | Prior art in ZK tools |
|---|---|---|---|---|---|
| TOTP as unlock factor | ⚠️ Circular problem | ⚠️ Partial | ✅ Yes | ✅ Yes | None (password managers don't do this) |
| TOTP as recovery | ❌ No | ❌ No | ✅ Yes | ✅ Yes | None |
| MitID as unlock factor | ❌ Network-dependent | ❌ No | ❌ No | ❌ Denmark only | None |
| MitID as recovery | ❌ Custodial | ❌ No | ❌ No | ❌ Denmark only | None |
| Key file (existing) | ✅ Yes | ✅ Yes | ✅ Yes | ✅ Yes | VeraCrypt, KeePass |
| BIP-39 recovery phrase | ✅ Yes (recovery slot) | ✅ Yes | ✅ Yes | ✅ Yes | Hardware wallets, 1Password |
| FIDO2/WebAuthn hardware key | ✅ Yes (locally) | ✅ Yes | ✅ Yes | ✅ Yes | Future phase |

---

## Recommendation

**Neither authenticator-app TOTP nor MitID should be implemented as a vault authentication or recovery mechanism for Arx Runa.**

### TOTP

TOTP is designed for server-validated authentication — a protocol where the verifier is a remote server that independently computes expected codes. In a purely local, zero-knowledge application:

- Storing the TOTP seed inside the vault creates a circular dependency (you need the vault open to verify TOTP).
- Storing it outside the vault in plaintext trivially leaks to any attacker who accesses the vault file.
- The narrow threat model it *does* address (attacker knows password, doesn't have the phone) is already handled with lower complexity by the existing key file second factor.

The key file (`auth::KeyFile`) already functions as a genuine hardware-token-equivalent second factor: a high-entropy binary file stored separately from the password. Its protection model is identical to TOTP's, without the circular storage problem or the HMAC-SHA1 design of RFC 4226.

### MitID

MitID is a national federated identity system built for client-server applications. It requires network connectivity, government infrastructure, Danish national identity, and server-side relying-party registration. Every vault unlock event would be logged and correlated to a national ID. This is the inverse of Arx Runa's threat model (no server, no identity, no logs).

**What to do instead**: The existing key file and planned BIP-39 recovery phrase (see `password-and-key-recovery.md`) are the appropriate second factor and recovery mechanisms. For users who want hardware-token protection, FIDO2/WebAuthn (planned future phase) allows the private key to reside on a hardware security key (YubiKey, etc.) and authenticate locally without a server.

---

## Decisions

> Choices made during this research session. Updated as the session progresses.

| Decision | Alternatives considered | Rationale |
|---|---|---|
| Do not implement TOTP as a vault unlock factor | TOTP with K in OS keychain (narrow protection) | Circular storage problem makes TOTP structurally incompatible with local vault decryption; existing key file provides equivalent "something you have" protection without the circularity |
| Do not implement TOTP as a recovery mechanism | Using the TOTP seed as a high-entropy recovery input | TOTP codes are ephemeral and cannot encode key material; using the raw seed would just replicate the key file / BIP-39 phrase with worse semantics |
| Do not implement MitID as an auth or recovery mechanism | MitID as an opt-in Danish-only recovery path | Requires network connectivity, Danish CPR number, server-side OIDC registration, and logs every vault access to government infrastructure — the inverse of the ZK threat model |
| Existing key file is the appropriate "second factor" | TOTP, FIDO2 (future), platform biometric | Key file already provides high-entropy hardware-token-equivalent second factor with no server, no shared secret storage problem, and no network dependency |

---

## Open Questions

- Should the research document on password/key recovery (`password-and-key-recovery.md`) note explicitly that TOTP is not a viable recovery mechanism, to pre-empt future re-evaluation?
- Is there a valid niche use case for TOTP as a *session policy enforcement* tool (not vault decryption) — e.g., requiring a TOTP code before performing destructive operations (vault deletion, password change) on an already-unlocked session?
- FIDO2 hardware keys (YubiKey) can perform HMAC-SHA1 challenges locally — is this the appropriate "hardware token" path for Arx Runa, and does it change the TOTP analysis?

---

## Sources

| Source | Topic | URL |
|---|---|---|
| RFC 6238 — TOTP: Time-Based One-Time Password Algorithm | TOTP specification, HMAC-SHA1 derivation, ±1 step window; shared secret "SHOULD be stored protected against unauthorized access" | https://datatracker.ietf.org/doc/html/rfc6238 |
| RFC 4226 — HOTP: An HMAC-Based One-Time Password Algorithm | HOTP foundational algorithm; both prover and verifier must possess the shared secret | https://datatracker.ietf.org/doc/html/rfc4226 |
| OWASP Multifactor Authentication Cheat Sheet | TOTP recommended for server-validated MFA; does not address local-only or offline application scenarios | https://cheatsheetseries.owasp.org/cheatsheets/Multifactor_Authentication_Cheat_Sheet.html |
| Digitaliseringsstyrelsen — MitID portal | MitID operated by the Danish Agency for Digitisation; 24/7 server infrastructure required | https://www.digitaliser.dk/mitid |
| MitID developer documentation | OIDC/OAuth2 relying-party requirements; server registration required | https://developers.mitid.dk/ |
| NIST SP 800-63B §5.1.4 — TOTP authenticators | TOTP secret "SHALL be stored securely by the verifier"; verifier must independently compute expected OTP | https://pages.nist.gov/800-63-3/sp800-63b.html |
