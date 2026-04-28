---
applyTo: "src-tauri/src/auth/ceremonies/**"
---

# Auth ceremonies

> Loads in addition to auth.md.

- `master_key` must not escape ceremony function body — no struct may hold `master_key`/`MasterKey`; bind as `Zeroizing<[u8; 32]>` in ceremony-local scope only
- `install_session` and `swap_active_session` accept pre-derived keys; neither re-runs KDF
- `MANIFEST_BACKUP_BLOB_NAME` is owned by `storage::cloud::manifest_backup` — do not re-declare in `auth::ceremonies`
- `vault_identity` is written once in `create_vault`, re-wrapped in `change_password`/`rotate_key_file`; sharing code may read `vault_identity.public_key` only — never insert, update, or delete that row
- Recovery: opt-in post-creation via `setup_recovery`; no slot = no recovery path
- BIP-39 (English wordlist) only; validate with `Mnemonic::parse_in(Language::English, phrase)` before any Argon2id derivation
- Canonical Argon2id input: `mnemonic.words().collect::<Vec<_>>().join(" ")` — not `to_string()` or other separators
- Recovery slot AEAD: `wrap_master_key_for_recovery`/`unwrap_master_key_from_recovery` with AAD = `b"arx-runa recovery v1" || vault_id_bytes` — never `wrap_file_key`
- Recovery slot Argon2 parameters stored per-slot; default to primary slot values at `setup_recovery` time
- `recover_with_phrase` returns `InvalidRecoveryPhrase` (no Argon2id) on checksum failure, `NoRecoverySlot` on empty slots, `InvalidCredentials` when all slots fail AEAD decrypt
- The 24-word phrase is returned once, wrapped in `Zeroizing<String>`. Display, require acknowledgement, drop. Never log, write to disk, or include in error messages.
