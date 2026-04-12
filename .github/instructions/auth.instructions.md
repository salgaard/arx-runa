---
applyTo: "src-tauri/src/auth/**"
---

# Auth module — rules

**Design specification**: `docs/architecture/designs/authentication-and-session-management/design.md` — last verified against design dated 2026-04-07

## Authentication tiers
- Tier 1: password only — `master_key = Argon2id(password, salt)`
- Tier 2: password + USB key file — `master_key = Argon2id(password || key_file_bytes, salt)`
- Tier selected per vault at creation; applies to entire vault

## USB key file (Tier 2 only)
- 32 bytes random entropy — not a device serial/ID
- Tier 2 mandatory factor: no password-only fallback for Tier 2 vaults
- No TOTP/authenticator apps — must be deterministic for KDF

## Key derivation
- Tier 1: `master_key = Argon2id(password, salt)`
- Tier 2: `master_key = Argon2id(password || key_file_bytes, salt)`
- Salt in unencrypted vault header (cloud) — needed before derivation
- Argon2id parameters: m = 65536 KiB, t = 3, p = 4 (RFC 9106 §4 recommended tier)
- HKDF-SHA256 derives vault keys from `master_key`
- See design doc for full parameter table and HKDF tree

## Session
- Read key file once at start — hold derived keys in mlocked memory
- Timeout: zeroize all keys, then drop
- Never persist session keys to disk

## Errors
- `InvalidCredentials` for wrong password, wrong key file, or both — caller cannot distinguish the cases
- `KeyFileNotFound` when no 32-byte file matches the vault header BLAKE3 hash — does not reveal password status
- Other variants: `MemoryLockFailed`, `VaultHeaderInvalid`, `InvalidRecoveryPhrase`, `NoRecoverySlot`
- Never log key file contents or derived keys

## Traits
- `KeySource` trait — `read_key() -> Result<Zeroizing<[u8; 32]>, KeySourceError>`; implementations: `FileKeySource` (prod), `MockKeySource` (test)
- `DeviceMonitor` trait — `watch() -> Pin<Box<dyn Stream<Item = DeviceEvent> + Send>>`; implementations: `WindowsDeviceMonitor`, `LinuxDeviceMonitor`, `MacOsDeviceMonitor`, `MockDeviceMonitor` (test)
