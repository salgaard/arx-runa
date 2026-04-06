---
timestamp: "2026-03-29T20:41:53+0200"
type: decision
report-sections:
  - method
  - discussion
tags: [authentication, usb-key-file, argon2id, session-management, mlock, device-detection]
source: agent
commit: "5d71df7"
---

## Authentication and Session Management — Key Design Decisions

## Context

Phase 2 of Arx Runa requires a full authentication system: USB key file as a mandatory cryptographic factor, Argon2id key derivation, session lifecycle with memory-locked keys, and session timeout. Several design questions were open at the start of this phase, the most significant being how USB key file identification and auto-detection should work without requiring a fixed filename convention.

## Substance

### USB key file: auto-detection via BLAKE3 fingerprint

The USB key file is 32 bytes of CSPRNG-generated random data. It has no internal structure and no mandated filename. Arx Runa generates the file during vault creation and writes it to a user-chosen location on a removable drive.

To enable auto-detection without a fixed filename, `blake3(key_file_content)` is stored in the vault header alongside the Argon2id salt. The vault header is public by design (plaintext JSON). BLAKE3 is preimage-resistant — the hash cannot be reversed to obtain the 32-byte key file, and does not provide an attacker any advantage beyond being able to verify a candidate file.

**Auto-detection flow**: when the OS reports a removable drive mount event, Arx Runa scans the drive for files that are exactly 32 bytes (near-instant filter), computes BLAKE3 for each, and matches against the vault header. If a match is found, the key file path is auto-populated in the login UI. The user still enters the password and explicitly confirms — the USB event fills one field, not the entire authentication.

The vault header field list is updated to: `vault_id, schema_version, argon2_salt, argon2_params, key_file_blake3`.
<!-- CITE: BLAKE3 specification — preimage resistance and collision resistance properties -->

### USB device monitoring via OS-native APIs

USB mount events are delivered via OS-native APIs: `RegisterDeviceNotification` (Windows) or `udev` (Linux). A `DeviceMonitor` trait abstracts the platform-specific implementation, with a `MockDeviceMonitor` for testing without hardware.

### Argon2id input construction

The Argon2id "password" input is `password_utf8_bytes || key_file_bytes`. Simple concatenation is unambiguous because the key file is always exactly 32 bytes — the split point is deterministic at `total_length - 32`. Hashing the password before Argon2id would defeat Argon2id's purpose (memory-hard processing of the raw password). Length-prefixing was considered and rejected as unnecessary complexity.
<!-- CITE: OWASP Password Storage Cheat Sheet — Argon2id recommended parameters -->
<!-- CITE: RFC 9106 — Argon2 Memory-Hard Function for Password Hashing and Proof-of-Work Applications -->

Salt size: 32 bytes (CSPRNG), stored in vault header. Exceeds NIST minimum of 16 bytes.

### mlock failure: hard fail

If `mlock` (Linux) or `VirtualLock` (Windows) fails, Arx Runa refuses to create the session and returns a clear error message explaining the cause and the fix. A security product that silently degrades memory protection is not trustworthy. The required memory is under 1 KiB (three 32-byte session keys), well within default system limits.

### Session timeout: activity-based, 15 minutes, configurable

A background `tokio` task resets a timer on every IPC command. When the timer fires, all `SessionKeys` are zeroed via `ZeroizeOnDrop` and the SQLCipher connection is closed. The frontend receives a 60-second warning before expiry. File operations in progress are allowed to complete before keys are zeroed.

### Vault creation flow

The complete first-run sequence: set password → USB insertion detected → generate key file → write to USB → compute BLAKE3 → generate salt → Argon2id + HKDF → create SQLCipher DB → generate X25519 identity keypair (private key wrapped with `key_encryption_key`) → write vault header. `master_key` exists only in the scope between Argon2id output and HKDF completion — it is zeroed immediately after all three derived keys are produced.

### Password change and key file rotation without re-encrypting blobs

Both flows re-derive session keys with the new credentials, re-wrap all `file_key` values and the X25519 identity private key under the new `key_encryption_key`, re-key SQLCipher, and update the vault header with the new salt. A new salt is always generated — reusing a salt with a different password is a security violation.

**Key property of the per-file key model**: no chunk re-encryption is required. The encrypted blobs in the cloud are unchanged because the blobs are encrypted with `file_key` values, which are themselves unchanged — only their wrapping changes.

**Sharing relationships survive key file rotation.** The X25519 identity keypair is re-wrapped under the new `key_encryption_key`, but the keypair itself does not change. Contacts who hold the user's X25519 public key can still address share packages to them. This corrects an earlier assumption in the sharing design that key rotation would invalidate sharing relationships.

## Alternatives considered

### Fixed key file filename

Scan removable drives for a file named `voidgate.key`. Rejected: the filename reveals Arx Runa usage (anti-plausible deniability) and constrains the user's file placement options.

### Arbitrary file as key material (hash user-chosen file)

Allow the user to designate any file as the key file and derive key material from its content via hashing. Rejected: the entropy of the resulting key depends on the file content. A small or low-entropy file (text document, small JPEG) would produce a weak key. The 256-bit CSPRNG-generated file guarantees full entropy.

### Store key file path in SQLCipher

Chicken-and-egg problem: the SQLCipher database requires `sqlcipher_key` to open, which requires the key file to derive. The key file path cannot be retrieved from SQLCipher before the key file is found.

### Soft failure on mlock unavailability

Continue without memory locking, log a warning. Rejected: session keys in swap would be recoverable by an attacker with disk access, undermining the purpose of the session model.

## Implications

- Vault header spec updated to include `key_file_blake3` across all documentation
- `DeviceMonitor` trait added to Phase 2 deliverables alongside `KeySource`
- Phase 2 is more substantial than the initial roadmap scoped it — vault creation and rotation flows are included
- The per-file key model (adopted in Phase 1) delivers a concrete Phase 2 benefit: password and key file changes require no cloud operations beyond re-uploading the vault header and manifest backup

## References

<!-- CITE: RFC 9106 — Argon2 Memory-Hard Function for Password Hashing and Proof-of-Work Applications -->
<!-- CITE: OWASP Password Storage Cheat Sheet — Argon2id parameters and salt requirements -->
<!-- CITE: BLAKE3 specification — https://github.com/BLAKE3-team/BLAKE3-specs — preimage resistance -->
<!-- CITE: NIST SP 800-132 — Recommendation for Password-Based Key Derivation — salt size requirements -->
<!-- CITE: Windows RegisterDeviceNotification — MSDN — DBT_DEVICEARRIVAL device arrival notification -->
<!-- CITE: libudev documentation — Linux udev device event monitoring -->
