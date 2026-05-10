---
applyTo: "src-tauri/src/auth/**"
---

# Auth

> Design: `docs/architecture/designs/authentication-and-session-management/design.md`

- Tier 1: `master_key = Argon2id(password, salt)`; Tier 2: `master_key = Argon2id(password || key_file_bytes, salt)` — selected at vault creation, applies to entire vault
- USB key file (Tier 2): 32 bytes random entropy, not device serial; mandatory factor — no password-only fallback; no TOTP (must be deterministic for KDF)
- Salt in unencrypted vault header (cloud) — needed before derivation; HKDF-SHA256 derives vault keys from `master_key` — see crypto.md for Argon2id parameters
- Session: read key file once at start; hold derived keys in mlocked memory; timeout = zeroize all keys then drop; never persist to disk
- Check session state before any session-scoped work; re-auth while `Active` returns `SessionAlreadyActive` — call `lock()` first
- `reset_timer()` must be called by IPC dispatcher on every Tauri command invocation while session is `Active`
- Long-running ops: `let _guard = session_manager.begin_operation();` — `lock()` waits for counter to reach zero before zeroizing
- `SessionEvent` must never contain secret material
- Auth backoff: delay = min(30s, 2^(attempts-1) s) before returning `InvalidCredentials`; counts consecutive failures after KDF derivation; resets on successful session installation
- Key accessor methods invoke closures under session read lock — no key buffer may escape the closure
- Errors: `InvalidCredentials` (wrong password or key file — caller cannot distinguish); `KeyFileNotFound` (no 32-byte file matches vault header BLAKE3 hash — does not reveal password status); other: `MemoryLockFailed`, `VaultHeaderInvalid`, `InvalidRecoveryPhrase`, `NoRecoverySlot`, `SessionAlreadyActive`
- Never log key file contents or derived keys
- Traits: `KeySource`: `read_key() -> Result<Zeroizing<[u8; 32]>, KeySourceError>`; `DeviceMonitor`: `watch() -> Pin<Box<dyn Stream<Item = DeviceEvent> + Send>>`
- Ceremony and recovery slot rules: see auth-ceremonies.md (loads automatically on src-tauri/src/auth/ceremonies/**)
