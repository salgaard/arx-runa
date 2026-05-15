# Arx Runa Security Model

This document describes the security model of Arx Runa for a technically literate audience. It covers authentication tiers, brute force resistance, attack chains, endpoint threats, the cloud access model, and the identity system used for file sharing.

---

## Zero-Trace Interpretation (Transient vs Persisted Plaintext)

Arx Runa's zero-trace policy separates transient runtime exposure from prohibited persistence:

- **Expected (transient in-memory use)**: decrypted plaintext may exist briefly in process memory while the user is actively decrypting, viewing, or processing data.
- **Prohibited (persisted/logged leakage)**: decrypted plaintext must not be written to durable or externally emitted outputs such as disk files, logs, telemetry, or developer-tooling output (for example debug traces or diagnostic command output).

Zero-trace means no persisted plaintext artifacts under application control. It does not claim plaintext is "never in memory."

---

## Authentication Tiers

Arx Runa supports two authentication tiers. Both use Argon2id as the password-based key derivation function (KDF), but they differ in what material is fed into it.

**Tier 1 — password only**

```
master_key = Argon2id(password_utf8_bytes, argon2_salt, params)
```

The 32-byte `argon2_salt` is generated via CSPRNG at vault creation and stored in the vault header. New vaults use Argon2id defaults (`m=65536 KiB`, `t=3`, `p=4`), and these parameters are stored in the header for cross-device bootstrap. On existing devices, downloaded `vault_id`/Argon2 values are treated as untrusted cloud input and must exactly match locally cached `local-vault-params.json`; only first bootstrap (no cache) accepts OWASP floors (`19456/2/1`) with a warning below Arx defaults.
<!-- CITE: OWASP Password Storage Cheat Sheet — Argon2id baseline guidance; Arx Runa uses stronger defaults -->

**Tier 2 — password + USB key file**

```
master_key = Argon2id(password_utf8_bytes || key_file_bytes, argon2_salt, params)
```

The key file is 32 bytes (256 bits) of CSPRNG entropy with no internal structure. It is concatenated with the password bytes before being passed to Argon2id. Because the key file is always exactly 32 bytes, the split point in the combined input is unambiguous at `total_length - 32`.

In both tiers, `master_key` is used only as input to HKDF-SHA256 (RFC 5869) to derive three purpose-specific keys, after which it is zeroed from memory and never stored.
<!-- CITE: RFC 5869 — HMAC-based Extract-and-Expand Key Derivation Function -->

---

## Key Derivation from master_key

After Argon2id produces `master_key`, HKDF-SHA256 expands it into three vault-level keys using distinct `info` strings to guarantee cryptographic separation:

| Derived Key | HKDF `info` string | Purpose |
|---|---|---|
| `key_encryption_key` | `arx-runa-key-encryption` | Wraps and unwraps per-file `file_key` values |
| `sqlcipher_key` | `arx-runa-sqlcipher` | Keys the local SQLCipher database |
| `manifest_key` | `arx-runa-manifest-backup` | Encrypts the cloud manifest backup |

`master_key` is zeroed immediately after all three derivations complete. It is never assigned to a struct field, returned from a function, or written to any storage.


---

## Brute Force Resistance

### Why offline attacks differ from online attacks

An attacker who can submit authentication attempts to a server is rate-limited and eventually locked out after a small number of failures. With cloud-stored vaults, this defence does not apply to the password derivation itself: the attacker can fetch the vault header (which contains `argon2_salt` and `argon2_params`) and run derivation attempts locally, without rate limiting.

The defence against this offline attack is the computational cost of Argon2id.

### Tier 1 offline resistance

Argon2id is the recommended algorithm for offline password hashing because its large memory requirement defeats GPU-parallel attacks.
<!-- CITE: OWASP Password Storage Cheat Sheet — Argon2id as preferred algorithm -->
<!-- CITE: RFC 9106 — Argon2 Memory-Hard Function for Password Hashing and Proof-of-Work Applications -->

At Arx Runa defaults (`m=65536 KiB`, `t=3`), each derivation requires approximately 64 MiB of RAM. GPU cores have limited per-core memory bandwidth; the memory requirement prevents the massive parallelism that makes GPUs effective against algorithms such as PBKDF2 or bcrypt.
<!-- CITE: Argon2 original paper — Biryukov, Dinu, Khovratovich 2016 — GPU memory-hardness analysis -->

The practical result is that GPU parallelism is severely limited compared to algorithms such as PBKDF2 or bcrypt, making offline brute force substantially harder for a given hardware budget.

Tier 1 brute force resistance depends entirely on password quality. A password with insufficient entropy can be found within a practical time budget regardless of Argon2id cost. A minimum of 12 randomly chosen characters from a broad character set is recommended; passphrase-style passwords of equivalent or higher entropy are equally acceptable.

### Tier 2 offline resistance

When a key file is present, the Argon2id input is `password_utf8_bytes || key_file_bytes`. The key file contributes 256 bits of CSPRNG entropy. An attacker who does not possess the physical key file must search a 2^256-element space in addition to the password space. This is computationally infeasible regardless of hardware, and the Argon2id memory-hardness cost compounds the infeasibility.

Tier 2 brute force resistance is not dependent on password quality in the same way as Tier 1.

---

## Attack Chain for a Compromised Password (Tier 1)

If an attacker obtains the correct password, the following steps are required to access vault contents. Each step is a prerequisite for the next.

1. Obtain the vault header from cloud storage — plaintext bootstrap metadata that must be treated as untrusted input
2. Extract `argon2_salt` and `argon2_params` from the vault header
3. Derive `master_key = Argon2id(password, argon2_salt, params)`
4. Derive `key_encryption_key` and `sqlcipher_key` via HKDF-SHA256
5. Authenticate to the cloud provider and download the encrypted SQLCipher manifest backup
6. Open the manifest with `sqlcipher_key` to obtain the list of files and their wrapped `file_key` values
7. Unwrap per-file `file_key` values using `key_encryption_key`
8. Authenticate to the cloud provider and download individual encrypted blobs
9. Decrypt blobs using the per-file `file_key` and the AEAD construction

The password alone is not sufficient. For owner-private data, steps 5 and 8 still require independent cloud provider credentials. Existing devices also pin `vault_id` + Argon2 salt/params in `local-vault-params.json`, so cloud-side header tampering is rejected before derivation.

**Effect of Tier 2**: step 3 requires `password || key_file_bytes` as Argon2id input. Without the physical USB key file, `master_key` cannot be derived from the correct password alone. The entire chain from step 3 onward is blocked.

---

## Endpoint Threats

The cryptographic model provides strong guarantees against offline attacks and network-level adversaries. It does not protect against a compromised endpoint. The following attacks bypass Argon2id and the AEAD layer entirely:

- **Keystroke logging**: malware capturing the password at entry time
- **Memory scraping**: OS-level inspection of process memory after `master_key` or `SessionKeys` have been derived and placed in mlocked RAM
- **Screen capture or shoulder surfing**: observing the password during entry
- **Phishing or credential reuse**: the user is deceived into entering the password in a false context, or the same password is used on a compromised service

These are not cryptographic weaknesses. They are endpoint security concerns outside the scope of what any client-side encryption scheme can address.

**Tier 2 materially reduces the impact of endpoint threats.** A stolen password is useless without the physical USB drive; a stolen USB drive is useless without the password. An attacker must simultaneously compromise both factors. This does not prevent all endpoint threats — malware with full runtime access to the process at authentication time could capture both factors simultaneously — but it eliminates the large class of attacks that obtain only one factor (phishing, credential database leaks, single-keylogger sessions before the USB drive is inserted).

Arx Runa mitigates memory scraping with `mlock`/`VirtualLock` on all session key buffers and `ZeroizeOnDrop` on all key types, which overwrites key material before memory is released to the allocator.

---

## Cloud Access Model

A common point of confusion is which vault resources require cloud authentication and which are publicly accessible. The table below defines the access requirement for each resource category.

| Resource | Access requirement | Rationale |
|---|---|---|
| Vault header (`vault-header.json`) | Cloud provider credentials (owner remote), no vault auth | Plaintext bootstrap metadata fetched before vault authentication; existing devices validate `vault_id` + Argon2 salt/params against local `local-vault-params.json` trust anchor |
| Private vault blobs (`vault/<uuid>.blob`) | Cloud provider credentials | Owner's encrypted chunks; accessible only to the authenticated cloud account |
| SQLCipher manifest backup (`manifest/manifest-backup.blob`) | Cloud provider credentials | Encrypted, but also gated by cloud provider authentication |
| Shared blobs (`shared/<file_share_id>/<uuid>.blob`) | None (publicly readable) | Recipients hold no cloud credentials; AEAD with per-file `file_key` protects content |
| Shared blob content | `file_key` from share package | Delivered inside an HPKE-encrypted share package; without `file_key` the ciphertext is permanently inaccessible |

### Rationale for the public shared/ path

Recipients of a shared file are not expected to hold the owner's cloud credentials. Requiring recipients to authenticate would introduce credential management complexity and tightly couple sharing to a specific provider's permission model. The `shared/` path is designed to be publicly readable because the blobs are opaque AEAD ciphertext — blob names are UUID v4 (122 bits of entropy, not guessable), and without the `file_key` the contents cannot be decrypted.

This is an accepted architectural property stated in the threat model: ciphertext exposure to a party who discovers a `shared/` folder UUID is not a security failure, because AEAD without the key provides no information about plaintext.


---

## X25519 Identity and Key Exchange

Arx Runa generates a local X25519 keypair at vault creation. This keypair is the user's cryptographic identity for file sharing. There is no central key server or registration requirement.

The X25519 private key is stored in SQLCipher, wrapped with `key_encryption_key`. It is protected by the same authentication flow as all other vault content. If the vault password and key file are rotated, the private key is re-wrapped under the new `key_encryption_key`; the keypair itself does not change, and existing sharing relationships are preserved.

The X25519 public key is shared out-of-band — exported as a file or QR code and delivered via the user's own channel (email, messaging application, physical media). Arx Runa does not publish public keys to any directory and does not require an email address or any network-accessible identity.

Only parties to whom the user has explicitly delivered their public key can construct share packages addressed to that user. The security of key exchange is as strong as the out-of-band delivery channel. Arx Runa displays a short fingerprint (first 16 hex characters of the SHA-256 hash of the public key) to allow out-of-band verification; this verification is opt-in.
<!-- CITE: age encryption tool (filippo.io/age) — same out-of-band key exchange model -->

Share packages are encrypted using HPKE (RFC 9180) with `DHKEM(X25519, HKDF-SHA256) + HKDF-SHA256 + CTX-ChaCha20-Poly1305`. The owner seals a JSON payload (including per-file `file_key`) to the recipient's long-term X25519 public key, and the ephemeral private key used for encapsulation is discarded after use.

---

## References

- [Cryptographic Primitive Rationale](../research/cryptographic-primitive-rationale.md)
- [File-Sharing Cryptography](../research/file-sharing-cryptography.md)
- [Password and Key Recovery](../research/password-and-key-recovery.md)
- [Glossary](glossary.md)
