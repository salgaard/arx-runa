---
paths:
  - "src-tauri/src/auth/**"
---

# Auth module — rules

## USB key file
- 32 bytes random entropy — not a device serial/ID
- Mandatory factor: no password-only fallback
- No TOTP/authenticator apps — must be deterministic for KDF

## Key derivation
- `master_key = Argon2id(password || key_file_bytes, salt)`
- Salt in unencrypted vault header (cloud) — needed before derivation

## Session
- Read key file once at start — hold derived keys in mlocked memory
- Timeout: zeroize all keys, then drop
- Never persist session keys to disk

## Errors
- Never reveal which factor failed — generic "authentication failed" only
- Never log key file contents or derived keys

## Trait
- `KeySource` trait for key file access — enables mock testing
