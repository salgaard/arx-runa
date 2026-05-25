# Dead Code Audit

**Date:** 2026-05-22 (revised — Phase 7 complete, all deferrals are now real debt)  
**Auditor:** GitHub Copilot  
**Method:** jcodemunch static analysis → verified every candidate with `grep` for actual call sites → cross-referenced design docs in `docs/architecture`  
**Scope:** `src-tauri/src/**`

> **Context:** Phase 7 implementation is complete. No deferred work should remain. All `// Phase N` and `// TODO(phase-N)` suppressions are now real technical debt requiring a concrete action. Items below are classified with a definitive verdict.

---

## Summary

| Category | Items | Verdict |
|----------|-------|---------|
| True dead code — must act | 11 items | DELETE or WIRE UP |
| Stale suppressions — code is live | 5 suppressions | REMOVE THE ATTRIBUTE |
| Cfg-gated false positives — correct suppressions | 19 items + 6 cfg items | KEEP AS-IS |
| Unused imports in test modules | 6 files | LOW PRIORITY CLEANUP |
| Confirmed NOT dead (jcodemunch false positive) | 3 items | NO ACTION |

Zero warnings from `cargo check` — all dead code is hidden by explicit `#[allow(dead_code)]` attributes.

---

## A — True Dead Code (Action Required)

All items below were verified with `grep` across the entire `src-tauri/src` tree: **zero call sites found**.

### A1 — `install_session` · `auth/session/manager.rs:546`

```rust
#[allow(dead_code)]
pub(crate) async fn install_session(&self, keys, vault_id, vault_db_path) -> Result<...>
```

A one-method wrapper that calls `reserve_session_install` then `finalize_session_install`. All ceremony code calls those two methods directly; this wrapper is never called.

**Verdict: DELETE.** The wrapper adds no value and is not referenced anywhere.

---

### A2 — `with_manifest_key` · `auth/session/manager.rs:682`

```rust
#[allow(dead_code)]
pub(crate) async fn with_manifest_key<F, R>(&self, callback: F) -> Result<R, ...>
```

Exposes `session.manifest_key` via a closure callback. No caller uses it; manifest key is accessed through other session guard paths.

**Verdict: DELETE.**

---

### A3 — `vault_header_path` · `ui/vault_paths.rs:32`

```rust
#[allow(dead_code)] // Phase 7: used in direct vault-ID resolution
pub(crate) fn vault_header_path(vault_id: &str) -> PathBuf
```

Returns `{vault_root}/{vault_id}/vault-header.json`. The sync_commands and auth_commands import from `vault_paths` but none imports `vault_header_path`. Phase 7 is complete and this was never wired.

**Verdict: DELETE.**

---

### A4 — `with_session_refresh` · `ui/commands_common.rs:57`

```rust
#[allow(dead_code)] // Phase 7: with_session_refresh for long-running command restart
pub(crate) async fn with_session_refresh<F, Fut, T>(state: &AppState, f: F) -> Result<T, IpcError>
```

Body: `state.session_manager.reset_timer().await; f().await`. Every Tauri command already calls `reset_timer()` directly at its top. This wrapper was the Phase 7 plan for long-running command restart but was never adopted. Phase 7 is complete.

**Verdict: DELETE.** The feature was implemented inline in each command rather than through this wrapper.

---

### A5 — `load_primary_cloud_endpoint` · `storage/cloud/cloud_config.rs:111`

```rust
#[allow(dead_code)]
pub async fn load_primary_cloud_endpoint() -> Result<Option<CloudEndpoint>, CloudTransportError>
```

A migration function: tries the canonical config path, falls back to `legacy_cloud_config_path`, migrates file if found at legacy location. Never called at app startup or anywhere else — the app uses `load_primary_cloud_endpoint_from` directly with an explicit path.

**Verdict: DELETE.** The migration was never shipped. If legacy migration were ever needed, it should be added as an explicit startup step with a documented migration plan, not via this unreachable function.

---

### A6 — `legacy_cloud_config_path` · `storage/cloud/cloud_config.rs:18`

```rust
#[allow(dead_code)]
fn legacy_cloud_config_path() -> Option<PathBuf>
```

Returns `dirs::config_dir()/arx-runa/cloud-config.json`. Only ever called inside `load_primary_cloud_endpoint` (A5 above, itself dead). Doubly dead.

**Verdict: DELETE** (automatically resolved when A5 is deleted).

---

### A7 — `upsert_gdrive_sharing_config` · `storage/sqlcipher.rs`

```rust
#[allow(dead_code)] // TODO(phase-6): called from set_gdrive_service_account command
pub(crate) async fn upsert_gdrive_sharing_config(&self, destination_id, config_json) -> ...
```

Writes a Google Drive service account JSON blob to the vault database. Phase 6 is complete. No `set_gdrive_service_account` Tauri command exists.

**Verdict: DELETE** — unless a GDrive service-account onboarding command is explicitly planned, this storage method is orphaned.

---

### A8 — `get_gdrive_sharing_config` · `storage/sqlcipher.rs`

```rust
#[allow(dead_code)] // TODO(phase-6): called from has_gdrive_service_account and sync_commands
pub(crate) async fn get_gdrive_sharing_config(&self) -> Result<Option<String>, StorageError>
```

Reads back the GDrive service account JSON. Same situation as A7 — no command calls it.

**Verdict: DELETE** (alongside A7).

---

### A9–A11 — Strong Revocation · `sharing/revocation.rs`

| Item | Kind |
|------|------|
| `strong_revoke_share` (line 117) | 160-line async fn |
| `StrongRevocationOutput` | struct |
| `ReissuedPackage` | struct |

`strong_revoke_share` implements cryptographic re-keying revocation: re-encrypts all file chunks, uploads new blobs, issues new share packages to remaining recipients, marks old shares revoked, queues old blob deletion. It is fully implemented and has tests. The design doc `5.3-cloud-layout-and-revocation.md` specifies both soft revocation (done — `revoke_share` in sharing_commands.rs) and strong revocation (this). Strong revocation is **not exposed via any Tauri command**.

The existing `revoke_share` command performs only soft revocation (DB mark + blob deletion). There is no `strong_revoke_share` command.

**Verdict: WIRE UP or DELETE** — this is the most significant decision item.
- **Wire up:** Add a `strong_revoke_share` Tauri command; the implementation is complete and tested.
- **Delete:** If strong revocation (re-encryption) is out of scope for the shipped product, delete `strong_revoke_share`, `StrongRevocationOutput`, and `ReissuedPackage`. Also remove the `pub(crate) use revocation::{..., strong_revoke_share, ...}` re-export from `sharing/mod.rs`.

Design intent (from 5.3) was to support both flows. Given Phase 7 is complete, a deliberate decision is needed.

---

## B — Stale Suppressions (Code Is Live — Remove the Attribute)

These items have `#[allow(dead_code)]` or `#[allow(unused_imports)]` but the code **is actually called**. The suppressions are wrong and mask real usage, making the signal-to-noise ratio of future dead-code reviews worse.

### B1 — `mod cloud` in `sharing/mod.rs:4`

```rust
#[allow(dead_code)] // TODO(phase-6): remove when Tauri commands consume cloud
pub(crate) mod cloud;
```

`sharing_commands.rs` calls `crate::sharing::cloud::create_share(...)` and `crate::sharing::cloud::fetch_received_share_to_local(...)`. The module is live.

**Fix:** Remove the `#[allow(dead_code)]` line and the TODO comment.

---

### B2 — `mod ctx_aead` in `sharing/mod.rs:6`

```rust
#[allow(dead_code)] // TODO(phase-6): remove when Tauri commands consume ctx_aead
mod ctx_aead;
```

`sharing/hpke.rs` imports `ctx_open` and `ctx_seal` from this module. It is used by the HPKE sealing path, which is live.

**Fix:** Remove the `#[allow(dead_code)]` line and the TODO comment.

---

### B3 — `gdrive_delete_permission` in `sharing/gdrive_api.rs:339`

```rust
#[allow(dead_code)] // TODO(phase-6): called from revoke_share command
pub(crate) async fn gdrive_delete_permission(...)
```

IS called — from `cleanup_gdrive_share_permission` in `sharing_commands.rs`. The implementation was wired in Phase 6 but the suppression was never removed.

**Fix:** Remove `#[allow(dead_code)]` and the stale TODO comment.

---

### B4 — `b2_delete_key` in `sharing/b2_api.rs:192`

```rust
#[allow(dead_code)] // used in revocation (Step 12)
pub(crate) async fn b2_delete_key(...)
```

IS called — from `cleanup_b2_share_key` in `sharing_commands.rs`.

**Fix:** Remove `#[allow(dead_code)]`.

---

### B5 — `create_share_package` / `import_share_package` re-export in `sharing/mod.rs`

```rust
#[allow(unused_imports)]
pub(crate) use packages::create_share_package;
pub(crate) use packages::import_share_package;
```

Both are used: `create_share_package` is called from `sharing/cloud.rs`, `sharing/revocation.rs`, and integration tests. `import_share_package` is called from `sharing_commands.rs` and tests.

**Fix:** Remove `#[allow(unused_imports)]`. (The re-export on the same line as `import_share_package` has no separate allow, so only the one above `create_share_package` needs removing.)

---

## C — Correct Suppressions (Keep As-Is)

### C1 — `crypto/types/mod.rs` — 19 `#[allow(dead_code)]` attributes

The key types (`MasterKey`, `RecoveryKey`, `Kek`, `FileKey`, `WrappedKey`) have implementations split by `#[cfg(test)]` / `#[cfg(not(test))]`. The compiler sees one branch as unused per compilation mode. The suppressions are architecturally correct.

**No action needed.**

### C2 — `storage/cloud/destination_session.rs` — 6 `#[cfg_attr(not(test), allow(dead_code))]`

Test-only helper types and constructors. Standard pattern; no action needed.

### C3 — `storage/cloud/cloud_config.rs` — 1 `#[cfg_attr(not(test), allow(dead_code))]`

Test-only config builder. No action needed.

---

## D — Unused Imports in Ceremony Test Modules (Low Priority)

Six ceremony files suppress all unused-import warnings at the module level inside `#[cfg(test)]`. The correct fix is to remove the blanket allow and prune unused imports individually.

| File |
|------|
| `src-tauri/src/auth/ceremonies/create.rs` |
| `src-tauri/src/auth/ceremonies/change_password.rs` |
| `src-tauri/src/auth/ceremonies/recover_with_phrase.rs` |
| `src-tauri/src/auth/ceremonies/recover_vault.rs` |
| `src-tauri/src/auth/ceremonies/setup_recovery.rs` |
| `src-tauri/src/auth/ceremonies/rotate_key_file.rs` |

**Action:** On the next edit to each file, remove `#![allow(unused_imports)]` from the `#[cfg(test)]` module and fix the resulting compiler errors.

---

## E — Confirmed NOT Dead (jcodemunch False Positives)

The import-graph analyser flags method calls within the same crate as dead because it only tracks `use` imports, not `.method()` calls. The following were verified by grep and are live:

| Item | Actual caller |
|------|--------------|
| `swap_active_session` (manager.rs) | `change_password.rs:134`, `rotate_key_file.rs` |
| `create_share_package` re-export | `cloud.rs`, `revocation.rs`, test scenarios |
| `import_share_package` re-export | `sharing_commands.rs`, test scenarios |

No action needed on the code; see B5 for the stale suppression to remove.

---

## Out-of-Scope False Positives (Static Analysis Noise)

- `docs/mermaid-init.js::addZoomToDiagrams` — called from inline HTML `<script>`, not via JS module import. Not dead.
- `src-tauri/build.rs::main` — build script entry point; Cargo calls it directly. Not dead.
- ~78 test functions — not reachable via import graph because test modules are not imported from `main`. All legitimate tests.

---

## Action Plan

| Priority | Item | Action |
|----------|------|--------|
| 1 | `strong_revoke_share` + structs (A9-A11) | **Decision required:** wire to a Tauri command or delete |
| 2 | Stale suppressions B1–B5 | Remove attributes — mechanical, no logic change |
| 3 | `install_session` (A1) | Delete function + `#[allow]` |
| 4 | `with_manifest_key` (A2) | Delete function + `#[allow]` |
| 5 | `vault_header_path` (A3) | Delete function + `#[allow]` |
| 6 | `with_session_refresh` (A4) | Delete function + `#[allow]` |
| 7 | `load_primary_cloud_endpoint` + `legacy_cloud_config_path` (A5–A6) | Delete both functions |
| 8 | `upsert_gdrive_sharing_config` + `get_gdrive_sharing_config` (A7–A8) | Delete both methods |
| 9 | Ceremony test unused imports (D) | Fix per file on next touch |
