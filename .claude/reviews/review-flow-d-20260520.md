---
title: "Flow D — Cloud Sync & Rclone Subprocess"
date: "2026-05-20"
invariants: [5, 7, 9, 10]
status: complete
---

# Flow D Security Review — Cloud Sync & Rclone Subprocess

**Invariants in scope**: 5 (vault path validation), 9 (Argon2 vault-header trust), 10 (pending_deletions durable retry)

**Files reviewed**:
- `src-tauri/src/storage/cloud/rclone.rs` — transport layer
- `src-tauri/src/storage/cloud/rclone_subprocess.rs` — subprocess runner
- `src-tauri/src/storage/cloud/remote_path.rs` — path validation
- `src-tauri/src/storage/cloud/stderr_sanitiser.rs` — stderr redaction
- `src-tauri/src/storage/cloud/destination_session.rs` — credential persistence + session conf lifecycle
- `src-tauri/src/storage/cloud/vault_header.rs` → `validate_trust_policy` — Argon2 trust policy
- `src-tauri/src/storage/cloud/vault_header_io.rs` → `download_vault_header` — header download
- `src-tauri/src/storage/cloud/sync.rs` → `drain_pending_deletions`
- `src-tauri/src/storage/vault_ops/delete_file.rs` → `delete_file`
- `src-tauri/src/storage/sqlcipher.rs` → `delete_node` (lines 1706–1762)
- `src-tauri/src/ui/auth_commands.rs` → `rclone_conf_path` (line 65)

---

## Findings

### [FLOW-D-001] Local-path `path_prefix` bypasses centralized path validation
**Severity**: medium  
**Invariant**: 5  
**Location**: `src-tauri/src/storage/cloud/rclone.rs` — `build_remote_root` (~line 488)  
**Observation**: For `DestinationType::LocalPath` and `DestinationType::ExternalDrive`, `build_remote_root` only normalizes backslashes to forward slashes before embedding `path_prefix` verbatim into the rclone remote root:
```rust
DestinationType::LocalPath | DestinationType::ExternalDrive => {
    let path = destination.path_prefix.replace('\\', "/");
    Ok(format!("{}:{}", destination.rclone_remote_name, path))
}
```
Cloud destinations take the `_ =>` branch which calls `compose_remote_root`, which validates via `validate_remote_root_component` (rejects `..`, leading/trailing `/`, `//`, control chars, colon, disallowed chars). Local destinations skip all of this. A `path_prefix` such as `../../home/other` would be passed to rclone as `localremote:../../home/other`, causing all blob operations to land outside the intended directory.  
**Violation**: Invariant 5 requires centralized allowlist validation for all user-supplied vault-relative paths. `validate_remote_root_component` is the canonical validator; bypassing it for local destinations is a gap in the contract surface.  
**Recommendation**: Route local-path `path_prefix` through `validate_remote_root_component("path_prefix", &path, true)` before constructing the remote root string. For absolute Windows paths (e.g. `D:/vault`) the validator would need to allow the drive letter colon; consider a dedicated local-path validator that permits a single leading drive colon while still rejecting `..` and control characters.  
**Test coverage**: None — `test_transport_creation_fails_closed_for_invalid_remote_root_components` tests a Cloud path with invalid prefix but does not exercise the LocalPath branch.

---

### [FLOW-D-002] rclone session config stored in persistent config dir, not process-owned temp dir
**Severity**: low  
**Invariant**: 7 (design security property: credential delivery)  
**Location**: `src-tauri/src/ui/auth_commands.rs:65–71` — `rclone_conf_path`; `src-tauri/src/auth/session/manager.rs:798–803` — `session_rclone_conf_path`  
**Observation**: `rclone_conf_path` returns a deterministic path:
```rust
dirs::config_dir()
    .expect("config_dir must be available")
    .join("arx-runa")
    .join("rclone.conf")
```
On all platforms this resolves to a stable, user-owned config directory (e.g. `%APPDATA%\arx-runa\rclone.conf`, `~/.config/arx-runa/rclone.conf`). The design spec states credentials should reach rclone via "a temp file in a process-owned directory". A process-owned directory (e.g. a `0700` tmpdir created by the process at startup) restricts access to the process itself; the current config-dir placement means any other process running as the same OS user can open the file during the vault-unlock window.  
**Violation**: Design-doc property not met: "Cloud credentials reach rclone via a temp file in a process-owned directory". Owner-only filesystem permissions (`write_owner_only`) are enforced and the file is zeroed + deleted on lock, which bounds the exposure window, but the isolation is weaker than a process-controlled tmpdir.  
**Recommendation**: Create a `0700` (Unix) / `DACL`-restricted (Windows) directory under the OS temp path at session open, write `rclone.conf` there with a randomized filename, and clean up the directory on lock. This makes the credential file invisible to same-user processes and removes the predictable path that an attacker with code execution as the same user could target.  
**Test coverage**: `test_lock_securely_deletes_rclone_conf_when_file_exists` verifies deletion on lock but does not assert path properties.

---

### [FLOW-D-003] B2/OAuth session config read into non-`Zeroizing` `String` in `generate_share_credentials`
**Severity**: low  
**Invariant**: 7  
**Location**: `src-tauri/src/storage/cloud/rclone.rs:305` and `:458`  
**Observation**: Both `generate_b2_share_credentials` and `generate_gdrive_share_credentials` read the full session config file (which contains B2 master app key or OAuth refresh token) into a plain `String` via `tokio::fs::read_to_string`:
```rust
let conf = tokio::fs::read_to_string(&self.session_config_path).await?;
```
`String` does not implement `Zeroize` and is not wrapped in `Zeroizing`. The credential bytes may persist in heap memory until reallocation. The GDrive path additionally converts the SA JSON `serde_json::Value` to a `String` (`sa_json_str`) without Zeroizing wrapping.  
**Violation**: Invariant 7 / defense-in-depth. The credentials are already on disk in the session config, so the marginal risk is low; however, the in-memory copy should be minimized for consistency with the rest of the codebase.  
**Recommendation**: Wrap the `read_to_string` result in `Zeroizing::<String>::from(...)` and propagate `Zeroizing` through the parse/use chain where practical. For `sa_json_str`, use `zeroize::Zeroizing::new(serde_json::to_string(...)?`.  
**Test coverage**: None.

---

## Confirmed Pass

| Check | Result |
|---|---|
| Rclone args use `Vec<OsString>` — no shell interpolation at any call site | ✅ Pass |
| `validate_remote_path` called before every upload/download/delete/list operation | ✅ Pass |
| `validate_remote_prefix` called before list_blobs | ✅ Pass |
| Cloud destination `remote_root` built via `compose_remote_root` (validates remote_name, bucket, path_prefix) | ✅ Pass |
| `bucket_root` for cloud types inherits validated components (construction only reachable after `build_remote_root` succeeds) | ✅ Pass |
| `stdin` of rclone process is `Stdio::null()` — no stdin injection surface | ✅ Pass |
| Credentials delivered to rclone via `--config <file>` argument, not CLI args or env vars | ✅ Pass |
| rclone.conf written with `write_owner_only` (owner-only file permissions) | ✅ Pass |
| `build_session_rclone_conf` assembles config blob in `Zeroizing<String>` | ✅ Pass |
| `destroy_session_rclone_conf` zeroes file bytes before deletion (`sync_all` included) | ✅ Pass |
| `DestinationSession::Debug` impl redacts `rclone_config_blob` as `<redacted>` | ✅ Pass |
| `rclone_config_blob` stored in SQLCipher; plain-sqlite read returns error (test-verified) | ✅ Pass |
| `sanitise_stderr` drops lines containing `token`, `key`, `secret`, `password`, `credential` | ✅ Pass |
| `is_authentication_failure` checks raw stderr for pattern detection; only sanitized output is logged/returned | ✅ Pass |
| Auth pattern detected and reported as `AuthenticationFailed` before generic `RcloneProcessFailed` | ✅ Pass |
| `classify_non_zero_exit` logs only `stderr_sanitised`, never `stderr_raw` | ✅ Pass |
| Vault-header download: `validate_trust_policy` called immediately after deserialization | ✅ Pass |
| Bootstrap mode: `validate_argon2_parameters` enforces OWASP floor (19456/2/1); rejects below floor | ✅ Pass |
| ExistingDevice mode: `vault_id`, `argon2_salt`, `argon2_params` compared byte-for-byte against trusted anchor | ✅ Pass |
| Recovery-slot Argon2 params also floor-validated in `validate_trust_policy` | ✅ Pass |
| `delete_node` SQL: `INSERT OR IGNORE INTO pending_deletions` executes **before** `DELETE FROM nodes` within single transaction | ✅ Pass |
| `delete_node` transaction is atomic: both enqueue and node-delete commit together | ✅ Pass |
| `drain_pending_deletions`: `mark_deletion_complete` called only after `cloud_transport.delete_blob` returns `Ok(())` or `NotFound` | ✅ Pass |
| `NotFound` during drain treated as already-deleted and marked complete (correct behavior for never-uploaded blobs) | ✅ Pass |
| Cloud error during drain leaves row queued; logs warning | ✅ Pass |
| No B2 key material logged in `generate_b2_share_credentials` (`key_id` logged, not `application_key`) | ✅ Pass |
| No GDrive SA credentials logged in `generate_gdrive_share_credentials` (`permission_id` logged, not SA JSON) | ✅ Pass |
| `validate_single_remote_stanza` enforces exactly one remote stanza in stored config blob | ✅ Pass |
| Config blob stanza header rewritten to backend-assigned `rclone_remote_name` on insert and on `build_session_rclone_conf` | ✅ Pass |
| Windows: `CREATE_NO_WINDOW` flag set on rclone process spawn (no console window exposes args) | ✅ Pass |

---

## Summary

| Severity | Count |
|---|---|
| Critical | 0 |
| High | 0 |
| Medium | 1 |
| Low | 2 |

**Invariants fully confirmed (no findings)**:
- **Invariant 9** — Argon2 vault-header trust (Bootstrap floor rejection + ExistingDevice byte-exact comparison both verified).
- **Invariant 10** — `pending_deletions` durable retry (transaction order correct in `delete_node`; `drain_pending_deletions` only marks complete after confirmed cloud delete).
- **No command injection** — rclone subprocess uses typed argument list throughout; no shell string construction anywhere in the call path.

**Invariants with gap**:
- **Invariant 5** — [FLOW-D-001]: Local-path destinations bypass the centralized path validator for `path_prefix`.

**Design property gap**:
- [FLOW-D-002]: Session config in persistent config dir vs. process-owned temp dir (low, mitigated by owner-only perms + zeroize-on-lock).

**Follow-up fix session recommended**: Yes for [FLOW-D-001] (medium, testable, single-function change). [FLOW-D-002] and [FLOW-D-003] can be addressed in the same pass but are lower priority.
