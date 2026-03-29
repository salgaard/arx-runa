---
paths:
  - "src-tauri/src/auth/**"
---

# Auth module — scoped rules

These rules apply to all files under `src-tauri/src/auth/`.

## USB key file
- The key file is cryptographic material (32 bytes random entropy) — not a
  device identifier, serial number, or hardware token ID
- The key file is a mandatory cryptographic factor for key derivation.
  There must be NO password-only fallback path and NO way to downgrade to
  a device-identity-only check
- Do not use TOTP or authenticator apps as MFA — they are non-deterministic
  and cannot be used as KDF input

## Key derivation
- master_key = Argon2id(password || key_file_bytes, salt)
- The key file bytes are mixed into the KDF input, not checked as a separate
  authentication factor
- Argon2id parameters: m >= 19456, t >= 2, p = 1 (OWASP minimums)
- Salt stored in the unencrypted vault header in cloud — it must be fetched
  before any key derivation can happen on a new device

## Session model
- Read the USB key file once at session start — do not re-read per operation
- Derived session keys are held in mlocked memory (`mlock`/`VirtualLock`)
- Session timeout MUST zero all derived keys: call `zeroize()` on every
  key buffer in the session struct, then drop
- Session keys must not be persisted to disk in any form — only in mlocked RAM

## Trait boundary
- Define `KeySource` trait for the USB key file reader
- Code depends on `KeySource`, not a concrete filesystem type
- This enables mock-based testing without a physical USB device

## Error handling
- Errors from authentication failures must not reveal whether it was the
  password or the key file that was wrong — return a single generic
  "authentication failed" message to the caller
- Never log key file contents, password bytes, or derived keys

## Required tests
- Correct password + correct key file -> derives master_key successfully
- Correct password + wrong key file -> returns error
- Wrong password + correct key file -> returns error
- Session timeout handler zeroes all derived keys in memory
- `MockKeySource` returns controlled bytes for deterministic testing
