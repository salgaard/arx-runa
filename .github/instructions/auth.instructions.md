---
applyTo: "src-tauri/src/auth/**"
---

# Auth module — rules

**Design specification**: `docs/architecture/designs/authentication-and-session-management/design.md`

## USB key file
- 32 bytes random entropy — not a device serial/ID
- Mandatory factor: no password-only fallback
- No TOTP/authenticator apps — must be deterministic for KDF

## Key derivation
- `master_key = Argon2id(password || key_file_bytes, salt)`
- Salt in unencrypted vault header (cloud) — needed before derivation
- Argon2id minimums: m ≥ 19456, t ≥ 2, p = 1
- HKDF-SHA256 derives vault keys from `master_key`
- See design doc for full parameter table and HKDF tree

## Session
- Read key file once at start — hold derived keys in mlocked memory
- Timeout: zeroize all keys, then drop
- Never persist session keys to disk

## Errors
- Never reveal which factor failed — generic "authentication failed" only
- Never log key file contents or derived keys

## Trait
- `KeySource` trait for key file access — enables mock testing
