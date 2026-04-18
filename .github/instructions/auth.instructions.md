---
applyTo: "src-tauri/src/auth/**"
---


# Auth module — rules

**Design specification**: `docs/architecture/designs/authentication-and-session-management/design.md` — last verified against design dated 2026-04-12

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
- Session keys live in `SessionKeys` (`src-tauri/src/auth/session/keys.rs`) with fields backed by `SecureBytes<32>`; drop order runs zeroize -> munlock/VirtualUnlock -> free.
- `SessionManager` (`src-tauri/src/auth/session/manager.rs`) owns the `NoSession → Active → Expired` state machine; check `state().await` before any session-scoped work.
- `SessionManager::authenticate(password, key_source, salt, params)` is the only entry to `Active`; re-auth while `Active` returns `SessionAlreadyActive` — call `lock()` first.
- `reset_timer()` must be called by the IPC dispatcher on every Tauri command invocation while the session is `Active` (Phase 6.1 wires this).
- Long-running operations must bracket their work with `let _guard = session_manager.begin_operation();` — `lock()` and the timeout task wait for the counter to reach zero before zeroizing.
- The session timeout is loaded from `dirs::config_dir() / "arx-runa/config.json"` (schema `{ "schema_version": 1, "session_timeout_secs": u64 }`); default 900 s; clamp to `[60, 86400]`.
- Session events are broadcast on an internal `tokio::sync::broadcast::Sender<SessionEvent>` (`TimeoutWarning { seconds_remaining }` 60 s before expiry, `Locked` after zeroize). Never add secret material to this enum.

## Errors
- `InvalidCredentials` for wrong password, wrong key file, or both — caller cannot distinguish the cases
- `KeyFileNotFound` when no 32-byte file matches the vault header BLAKE3 hash — does not reveal password status
- Other variants: `MemoryLockFailed`, `VaultHeaderInvalid`, `InvalidRecoveryPhrase`, `NoRecoverySlot`, `SessionAlreadyActive`
- Never log key file contents or derived keys

## Traits
- `KeySource` trait — `read_key() -> Result<Zeroizing<[u8; 32]>, KeySourceError>`; implementations: `FileKeySource` (prod), `MockKeySource` (test)
- `DeviceMonitor` trait — `watch() -> Pin<Box<dyn Stream<Item = DeviceEvent> + Send>>`; implementations: `WindowsDeviceMonitor`, `LinuxDeviceMonitor`, `MacOsDeviceMonitor`, `MockDeviceMonitor` (test)

## Ceremonies
- `src-tauri/src/auth/ceremonies/mod.rs` is the entry-point re-export layer for all ceremony APIs.
- Six ceremony entry-point functions are split by flow under `src-tauri/src/auth/ceremonies/`: `create.rs` (`create_vault`), `change_password.rs` (`change_password`), `rotate_key_file.rs` (`rotate_key_file`), `recover_vault.rs` (`recover_vault`), `setup_recovery.rs` (`setup_recovery`), `recover_with_phrase.rs` (`recover_with_phrase`).
- Ceremony module structure keeps request/enum types in `src-tauri/src/auth/ceremonies/types.rs`, shared internals in `src-tauri/src/auth/ceremonies/helpers.rs`, shared test fixtures in `src-tauri/src/auth/ceremonies/test_support.rs`, and tests colocated in each ceremony flow file.
- `master_key` is bound as `Zeroizing<[u8; 32]>` inside ceremony-local scope and must not escape the function body. No struct may hold a `master_key` or `MasterKey` field.
- `SessionKeys::from_master_key_bytes` is the ceremony entry point for HKDF expansion; `SessionKeys::derive` is preserved for direct `SessionManager::authenticate` callers.
- `SessionManager::install_session` transitions `NoSession | Expired → Active` with pre-derived keys; `SessionManager::swap_active_session` rotates keys while staying `Active` (used by password change and key file rotation). Neither method re-runs KDF.
- The `pending-vault-header.json` staging file is written under `dirs::config_dir() / "arx-runa/"` with owner-only permissions during password change and key rotation. The startup retry loop is Phase 4.3 territory.
- Forward declarations: `VaultHeader` (`src-tauri/src/storage/cloud/vault_header.rs`) originates in Phase 2.4 and is extended by Phase 4.3. `CloudTransport` (`src-tauri/src/storage/cloud/mod.rs`) is replaced by the canonical 4-method surface in Phase 4.1; ceremonies call it via staging-file semantics.

## Recovery slots
- Recovery is opt-in and post-creation via `setup_recovery`; users who do not configure a slot cannot recover from lost credentials.
- BIP-39 (English wordlist) is the only Phase 2.4 recovery method. `Mnemonic::parse_in(Language::English, phrase)` validates the phrase before any Argon2id derivation.
- The canonical Argon2id input for both `setup_recovery` and `recover_with_phrase` is `mnemonic.words().collect::<Vec<_>>().join(" ")`. Do not use `to_string()` or other separators.
- Recovery slot AEAD uses the dedicated `wrap_master_key_for_recovery` / `unwrap_master_key_from_recovery` functions with AAD = `b"arx-runa recovery v1" || vault_id_bytes`. Never use `wrap_file_key` for recovery slot material.
- Recovery slot Argon2 parameters are stored per-slot (independent of the primary slot) but default to the same values at `setup_recovery` time.
- `recover_with_phrase` returns `InvalidRecoveryPhrase` (no Argon2id) on checksum failure, `NoRecoverySlot` on empty `recovery_slots`, and `InvalidCredentials` when all slots fail AEAD decrypt.
- The 24-word phrase is returned from `setup_recovery` exactly once, wrapped in `Zeroizing<String>`. The caller must display, require acknowledgement, and drop. Never log the phrase, never write it to disk, never include it in error messages.
