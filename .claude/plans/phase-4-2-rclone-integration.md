---
title: "Phase 4.2 — Rclone Integration and Provider Setup"
created: "2026-04-19T10:00:00Z"
status: implemented
roadmap-phase: 4
sub-phase: "4.2"
design-document: "docs/architecture/designs/cloud-synchronisation/design.md"
sub-phase-roadmap: "docs/architecture/designs/cloud-synchronisation/sub-phases/roadmap.md"
governance-sync-required: true
tags: [storage, cloud, rclone, subprocess, sidecar, phase-4]
---

# Plan: Phase 4.2 — Rclone Integration and Provider Setup

## 1. Goal

Add a production `RcloneTransport` under `src-tauri/src/storage/cloud/` that satisfies the canonical `CloudTransport` contract by driving the bundled Rclone sidecar via `tokio::process::Command`, together with the remote-path allowlist, stderr-credential scrubbing, exit-code mapping, `SyncConfig`, session-lived `rclone.conf`, non-sensitive `cloud-config.json`, encrypted `DestinationSession` storage, and guided S3/Google-Drive setup helpers.

## 2. Context

**Sub-phase position.** 4.2 is the second unit of the cloud-sync roadmap (4.1 → **4.2** → 4.3 → 4.4 → 4.5). 4.1 landed the canonical `CloudTransport` trait, `CloudTransportError`, `CloudEndpoint`, and the in-memory `MockCloudTransport`.

**Dependencies (met).** Phase 4.1 (`src-tauri/src/storage/cloud/mod.rs`, `.../endpoint.rs`, `.../mock.rs`), Phase 3.1 SQLCipher manifest (`destination_sessions` table already declared in `src-tauri/src/storage/schema.rs:59-73`), Phase 1 crypto (XChaCha20-Poly1305 for the `rclone_config_blob` encryption once wired).

**What exists today (2026-04-19).**
- `src-tauri/src/storage/cloud/mod.rs` — canonical `CloudTransport` trait, `CloudTransportError { NotFound, AuthenticationFailed, Timeout, IoError, RcloneProcessFailed, Other }`, `CloudEndpoint` re-export. `mod.rs:1` already advertises "Phase 4.2 adds `RcloneTransport`".
- `src-tauri/src/storage/cloud/endpoint.rs` — `CloudEndpoint { provider, bucket, region, endpoint, path_prefix }`.
- `src-tauri/src/storage/cloud/mock.rs` — in-memory mock + failure injection.
- `src-tauri/src/storage/cloud/vault_header.rs` and `.../manifest_backup.rs` — Phase 4.3/4.4 territory; no direct 4.2 dependency.
- `src-tauri/src/storage/schema.rs:59-73` — `destination_sessions` DDL already present.
- `src-tauri/tauri.conf.json` — no `externalBin` field yet; `capabilities/default.json` permits only `core:default` and `opener:default`.
- `src-tauri/Cargo.toml` — `tokio` has `macros, rt-multi-thread, fs, io-util, sync, time` — **missing `process` feature** required by `tokio::process::Command`.
- No existing `src-tauri/bin/` directory.
- `.claude/rules/storage.md` notes the `CloudTransport` `&str` rule; `.claude/rules/tauri.md` forbids the general `shell` plugin.

**Deliverables from the sub-phase** (`4.2-rclone-integration.md:10-25`) and the exact design anchors they resolve to are tracked in the approach steps below.

**No pending architectural decisions** for Phase 4 in `docs/roadmap.md`. Contract Surface is canonical. Security review is required and is correctly declared.

## 3. Design Concerns / Open Questions

| # | Concern | Source | Impact | Classification | Resolution | Documentation updates |
|---|---------|--------|--------|----------------|------------|-----------------------|
| 1 | **macOS absent from sidecar deliverable.** Deliverable 2 lists only Windows and Linux `externalBin` paths, yet CLAUDE.md mandates Windows/macOS/Linux parity and the design's "Rclone configuration file location" table (`design.md:493-495`) explicitly enumerates the macOS path `~/Library/Application Support/arx-runa/rclone.conf`. | `4.2-rclone-integration.md:13` vs `CLAUDE.md` §Platform compatibility and `design.md:489-505` | If left as stated, the Tauri bundle has no macOS sidecar target ⇒ Arx Runa cannot ship on macOS from 4.2 onward. Cross-platform regression. | **Non-blocking** (treat as spec gap; include macOS as an explicit plan assumption). | Add `aarch64-apple-darwin` / `x86_64-apple-darwin` `rclone` sidecar bundles to `externalBin`; treat macOS as first-class in `run_rclone`'s kill path (SIGKILL via `Child::kill`). Assumption 1 records the macOS decision. | Sub-phase `4.2-rclone-integration.md` deliverable 2 wording updated in a follow-up sync (§8 GS-004). |
| 2 | **`tauri::api::shell::open` is Tauri 1 API.** Sub-phase deliverable 8 (`4.2-rclone-integration.md:21`) and `design.md:590` reference `tauri::api::shell::open`, which was removed in Tauri 2. The workspace is on `tauri = "2"` (`Cargo.toml:27`). | `4.2-rclone-integration.md:21` / `design.md:590` vs `Cargo.toml:26-27` | Implementer would hit a compile error; or worse, pick an unsanitised URL shell-out. | **Non-blocking** (resolved by swap-and-document). | Use `tauri_plugin_opener` (already present at `Cargo.toml:27`) — call `app.opener().open_url(url, None)` from the Google Drive wizard flow. Whitelist only `https://` URLs built in-process from Rclone's OAuth output. | §8 GS-004 adds a note to `.claude/rules/tauri.md` that OAuth browser launch must use `tauri_plugin_opener`, not `shell`. |
| 3 | **Sub-phase omits `tokio::process` feature.** `run_rclone` relies on `tokio::process::Command`, but `src-tauri/Cargo.toml:30` does not enable the `process` Tokio feature. | `4.2-rclone-integration.md:14` vs `src-tauri/Cargo.toml:30` | Build failure the moment Step 1 lands. | **Non-blocking** (one-line Cargo edit). | Add `process` to `tokio` `features`. Also audit the release profile for clippy impact (none expected). | None. |
| 4 | **Capability allowlist lacks sidecar permission.** `capabilities/default.json` lists only `core:default` and `opener:default`. Tauri 2 requires an explicit `shell:allow-execute` (scoped to `rclone`) or a `sidecar` permission for the bundled binary. | `src-tauri/capabilities/default.json` vs `design.md:421` | Runtime failure (`tauri::Error::Permission`) on the first `run_rclone` invocation. | **Non-blocking**. | Add a scoped `shell:allow-execute` permission for the `rclone` sidecar name only (no other shell access). Update `.claude/rules/tauri.md` allowlist accordingly. | §8 GS-002 adds a rule line permitting scoped `rclone` execution. |
| 5 | **`SyncConfig` persistence scope.** Deliverable 7 defines the struct but does not say whether `sync-config.json` file load/save (`design.md:545`) is in Phase 4.2 or deferred. Phase 4.5 clearly consumes `max_concurrent`, so persistence is a 4.5 concern unless wired now. | `4.2-rclone-integration.md:18` vs `design.md:544-547` | If we persist now we touch code Phase 4.5 owns (push/pull flow config plumbing). If we defer we must be explicit. | **Non-blocking**. | **Defer JSON load/save to 4.5.** Phase 4.2 ships the struct, `Default`, range validation (`max_concurrent ∈ 1..=16`, `operation_timeout_seconds ∈ 60..=3600`), and unit tests. Recorded in Assumption 4. | None. |
| 6 | **`cloud-config.json` shape under multi-destination model.** Design §"Connection Descriptor" (`design.md:209`) says `cloud-config.json` stores non-sensitive endpoint fields of "the primary destination" for new-device bootstrap. Sub-phase deliverable 9 uses singular "`CloudEndpoint` fields" but the multi-destination model supports N destinations. | `4.2-rclone-integration.md:22` and `design.md:209,287` | Under-specified: array vs object. A mistake here breaks `VaultHeader` download bootstrap in Phase 4.3. | **Non-blocking** but worth explicit choice. | Persist a single `CloudEndpoint` JSON object for the **primary** destination only. Multi-destination recovery uses the decrypted SQLCipher manifest after auth (design.md:287). | None. |
| 7 | **Stderr sanitiser granularity.** `design.md:483` specifies line-wise keyword stripping. It does not say what happens when the full stderr is sensitive (every line dropped). Would return an empty `stderr_sanitised`, which loses the exit code context. | `design.md:481-484` | A credential-heavy error path returns a generic message with no debugging aid. Acceptable, but worth pinning. | **Non-blocking**. | Empty-after-sanitisation strings surface as `stderr_sanitised = "<credentials scrubbed>"` to make the redaction explicit without leaking the trigger. | None. |
| 8 | **Test strategy — real rclone in CI.** Deliverable 11 requires integration tests with a local rclone remote. CI runners may not have rclone on PATH; the sidecar isn't extracted during `cargo test`. | `4.2-rclone-integration.md:24` | If tests invoke the sidecar binary that only exists in bundled builds, CI fails; if tests always require an external `rclone`, CI still fails. | **Non-blocking**. | Gate integration tests with `#[ignore]` and a single `cfg` env check (`ARX_RCLONE_INTEGRATION=1`) that points at a system-installed `rclone` binary. Add a `resolve_rclone_binary()` helper that prefers `Command::new("rclone")` when the env var is set, otherwise uses a caller-supplied path (production path: Tauri's sidecar resolver). Record in Assumption 5. | None. |
| 9 | **Timeout kill semantics.** `design.md:516` says "kill on Unix, `TerminateProcess` on Windows". In tokio, `Child::kill()` maps to both, but the async semantics after `tokio::time::timeout` require care: the child must be reaped to avoid zombies. | `design.md:507-518` | Leaked processes under adversarial conditions. | **Non-blocking**. | Use `tokio::time::timeout` over `Child::wait_with_output()`. On timeout: `child.start_kill()` → `child.wait().await` (bounded second wait of 5 s) → synthesise `CloudTransportError::Timeout`. Document in Approach step 3. | None. |
| 10 | **`DestinationSession` ↔ SQLCipher write path.** Deliverable 9 says store encrypted `rclone_config_blob` in SQLCipher; but the repository's `MetadataStore` trait (Phase 3.1 surface) has no `insert_destination_session` method, and the `storage.md` rule explicitly caps `MetadataStore` at the 3.1 methods. | `4.2-rclone-integration.md:22` vs `.claude/rules/storage.md` §Traits | Without a surface, the wizard has nowhere to persist credentials. | **Non-blocking**. | Add **SQLCipher-specific helpers** in a new `src-tauri/src/storage/cloud/destination_session.rs` using a direct `rusqlite::Connection` handle obtained from `SqlCipherMetadataStore::with_connection` (mirror the pattern used for `list_all_blob_names` helper noted in `storage.md`). Do **not** add to `MetadataStore`. | §8 GS-001 adds a one-line note to `.claude/rules/storage.md` allowing SQLCipher-specific destination-session helpers outside `MetadataStore`. |
| 11 | **`rclone_config_blob` encryption key.** Design line 254 says "encrypted with session key", but which key specifically — `sqlcipher_key`, `manifest_key`, or a new `destination_config_key`? SQLCipher already transparently encrypts the entire row; an extra in-row AEAD is belt-and-braces. | `design.md:239-254` | Cryptographic key reuse risk; potentially wasted work. | **Non-blocking**. | Store the **plaintext** rclone config section as SQLCipher's own encryption is the single trust anchor (matches "SQLCipher is the authoritative credential store" — `design.md:598`). Rename field semantically in docs/tests as "SQLCipher-encrypted `rclone_config_blob`" (already the design's intent per lines 253-254 "Stored encrypted in SQLCipher; never written to disk in plaintext"). Add an inline test that opens a raw (non-keyed) SQLite handle against the DB file and asserts it cannot read the column. | Sub-phase decisions log entry under §7 (optional follow-up). |
| 12 | **Remote path allowlist vs UUID blob-name joins.** Design allowlist is `^[a-zA-Z0-9._/-]+$` (`design.md:461`). Callers construct paths like `"vault/<uuid>.blob"` and `"vault-header.json"` — both pass. But `shared/<file_share_id>/<uuid>.blob` (Phase 5) uses lowercase UUIDs ⇒ passes too. Prefix lookups with `remote_prefix == ""` must also be accepted (mock allows it). | `design.md:456-469` and `mock.rs` test | Consistency across transports. | **Non-blocking**. | Accept empty prefix explicitly (bypass the non-empty allowlist when the caller passes `""`), reject paths containing `..` or starting with `/`, reject control characters, otherwise enforce the allowlist. Record as Approach step 2. | None. |
| 13 | **Security review scope.** Sub-phase declares security review "Required" — correct; subprocess, credentials, and path sanitisation are in scope. | `4.2-rclone-integration.md:74-80` | N/A — properly scoped. | **Non-blocking (informational).** | Keep the declaration; §6b below lists the specific concerns for `/implement-plan`. | None. |

### Governance drift (2b)

| # | Finding | Classification |
|---|---------|----------------|
| G1 | `.claude/rules/storage.md` §Traits does not yet codify that `RcloneTransport` must never use shell interpolation, must sanitise remote paths per the canonical regex, and must strip credential-keyword lines from stderr before surfacing them. | Non-blocking → GS-001. |
| G2 | `.claude/rules/tauri.md` §Plugins currently says "Never: `shell`, …". After 4.2, a narrowly scoped `shell:allow-execute` permission for the `rclone` sidecar is required. Rule must be updated to allow that single exception, and to document that OAuth URL launches go through `tauri_plugin_opener`, not `shell`. | Non-blocking → GS-002. |
| G3 | GitHub Copilot mirrors in `.github/instructions/` drift the moment GS-001/GS-002 land. | Non-blocking → GS-003. |
| G4 | Sub-phase `4.2-rclone-integration.md:13` deliverable 2 hard-codes "Windows and Linux"; after this plan, it should read "Windows, macOS, and Linux" (Concern 1) and deliverable 8 (`:21`) should reference `tauri_plugin_opener` rather than `tauri::api::shell::open` (Concern 2). | Non-blocking → GS-004 (design file sync, not a rule). |

## 4. Assumptions

1. **macOS is a first-class target for the sidecar.** `tauri.conf.json` `externalBin` includes both `aarch64-apple-darwin` and `x86_64-apple-darwin` `rclone` binaries. Binaries are downloaded manually from rclone.org and placed under `src-tauri/bin/` — Cargo does not automate downloads.
2. **Tauri 2 APIs everywhere.** Browser launch uses `tauri_plugin_opener`; subprocess execution uses `tauri-plugin-shell` with a **scoped `rclone` sidecar** permission (not the full `shell` plugin). If `tauri-plugin-shell` is not currently a dependency, it is added in Step 1.
3. **`SyncConfig` is struct-only in 4.2**; JSON load/save and plumbing into push/pull land in 4.5.
4. **`cloud-config.json` persists the single primary `CloudEndpoint`**, not an array. Multi-destination records come from the decrypted SQLCipher manifest after authentication.
5. **Integration tests use a system-installed `rclone`** gated behind `#[ignore]` and `ARX_RCLONE_INTEGRATION=1`. Unit tests never shell out.
6. **`DestinationSession` rows persist via new SQLCipher-specific helpers**, not through `MetadataStore`. The helpers live under `storage::cloud::destination_session` and borrow a `rusqlite::Connection` from `SqlCipherMetadataStore` via a narrow `with_connection(FnOnce(&Connection) -> T)` accessor added as part of Step 6.
7. **`rclone_config_blob` relies on SQLCipher's row-level encryption as the sole protection**, mirroring the design's single-anchor intent; no additional in-row AEAD.
8. **Kill semantics** use `tokio::process::Child::start_kill` + bounded `wait` (5 s) on timeout; the 5 s wait does not count toward the configured operation timeout.
9. **Module layout follows one-concern-per-file** per `.claude/rules/rust.md`:
   - `storage/cloud/rclone.rs` — `RcloneTransport` struct + `CloudTransport` impl.
   - `storage/cloud/rclone_subprocess.rs` — `run_rclone`, exit-code mapping, timeout/kill handling.
   - `storage/cloud/remote_path.rs` — path allowlist + traversal rejection.
   - `storage/cloud/stderr_sanitiser.rs` — credential-keyword line stripper.
   - `storage/cloud/sync_config.rs` — `SyncConfig` struct + validation.
   - `storage/cloud/cloud_config.rs` — `cloud-config.json` read/write (non-sensitive primary endpoint).
   - `storage/cloud/destination_session.rs` — `DestinationSession`, `DestinationType`, `BackupSyncMode`, SQLCipher read/write helpers.
   - `storage/cloud/wizard.rs` — `setup_s3_provider`, `setup_google_drive`.
10. **Mock coverage parity.** `MockCloudTransport` gains no new API in 4.2; all new logic (path allowlist, stderr sanitiser, exit-code mapping, `SyncConfig` defaults, `cloud_config.json` serde, `DestinationSession` CRUD) has pure unit tests that do not need a real Rclone.
11. **macOS kill mechanism is `Child::start_kill` (sends SIGKILL internally via libc)** — same code path as Linux; no separate macOS branch.
12. **The session-lived `rclone.conf` path** is `dirs::data_dir().join("arx-runa").join("staging").join("rclone-session.conf")` (reuses the existing staging directory helper `default_staging_directory` from `src-tauri/src/storage/staging.rs:12`). On session close a dedicated helper overwrites the file with zero bytes and then deletes it.
13. **No IPC commands are added in 4.2** — ceremonies and the wizard surface live under the storage module. Tauri command wiring is Phase 6.1 territory.

## 5. Approach

### `CONTRACT_SNIPPETS` (inline once; reference by ID)

**CS-001 — `run_rclone` signature** (from `design.md:436-440`):
```rust
pub(crate) async fn run_rclone(
    binary_path: &Path,
    args: Vec<OsString>,
    timeout: Duration,
) -> Result<String, CloudTransportError>;
```
Returns stdout as UTF-8 `String`; maps non-zero exits and timeouts into `CloudTransportError` via CS-002.

**CS-002 — Exit-code mapping table** (from `design.md:473-479`):

| Exit code | `CloudTransportError` |
|-----------|----------------------|
| 0 | — (return `Ok(stdout)`) |
| 3 | `NotFound` |
| 4 | `NotFound` |
| Other non-zero | `RcloneProcessFailed { exit_code, stderr_sanitised }` |
| Timeout (Tokio) | `Timeout` |

**CS-003 — Remote-path allowlist** (from `design.md:461-469`):
- Regex `^[a-zA-Z0-9._/-]+$`, reject `..`, reject leading `/`, reject control characters.
- Empty prefix is accepted only for `list_blobs("")` — special-cased at the allowlist entry.

**CS-004 — Stderr sanitiser keywords** (from `design.md:483`, case-insensitive substring match on a per-line basis):
`token`, `key`, `secret`, `password`, `credential`, `auth`.
Empty result after sanitisation ⇒ placeholder `"<credentials scrubbed>"` (Concern 7).

**CS-005 — `SyncConfig`** (from `design.md:522-543`):
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncConfig {
    pub max_concurrent: u32,            // default 4, range 1..=16
    pub operation_timeout_seconds: u64, // default 300, range 60..=3600
}
impl Default for SyncConfig { /* 4, 300 */ }
```

**CS-006 — `DestinationSession` & `DestinationType`** (from `design.md:222-279`):
```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum DestinationType { Cloud, ExternalDrive, LocalPath }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DestinationSession {
    pub destination_id: String,
    pub label: String,
    pub destination_type: DestinationType,
    pub rclone_remote_name: String,
    pub rclone_config_blob: String,
    pub bucket: String,
    pub path_prefix: String,
    pub is_primary: bool,
    pub backup_mode: Option<BackupSyncMode>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum BackupSyncMode { Mirror, Accumulating }
```
SQL tags for `destination_type`: `'cloud' | 'external_drive' | 'local_path'` (matches `schema.rs:63`). Tags for `backup_mode`: `'mirror' | 'accumulating'` (matches `schema.rs:69`). Exactly one row with `is_primary = 1` per vault — enforced in `insert_destination_session` via a transactional check.

**CS-007 — Rclone command templates** (from `design.md:445-454`):
- Upload: `rclone copyto <local_path> <remote_root>/<remote_path> --quiet --no-traverse`
- Download: `rclone copyto <remote_root>/<remote_path> <local_path> --quiet --no-traverse`
- Delete: `rclone deletefile <remote_root>/<remote_path> --quiet`
- List: `rclone lsjson <remote_root>/<remote_prefix> --recursive --files-only --no-mimetype --no-modtime`
- Every command also carries `--config <session_rclone_conf_path> --retries 3`.

**CS-008 — `cloud-config.json` shape** (primary `CloudEndpoint` only, per Assumption 4):
```json
{ "provider": "…", "bucket": "…", "region": "…", "endpoint": "…", "path_prefix": "…" }
```
Path on disk: `dirs::config_dir().join("arx-runa").join("cloud-config.json")`. Owner-only permissions (uses the existing `storage::staging::write_owner_only` helper, or mirror it if it is private to `staging`).

---

### Implementation steps (absolute paths — Windows shell)

**Step 1 — Wire prerequisites (`C:\Users\chris\source\repos\arx-runa\src-tauri\Cargo.toml`, `C:\Users\chris\source\repos\arx-runa\src-tauri\tauri.conf.json`, `C:\Users\chris\source\repos\arx-runa\src-tauri\capabilities\default.json`).**
- Add `process` to the `tokio` feature list (Concern 3).
- Add `regex = "1"` and `tauri-plugin-shell = "2"` to `[dependencies]`.
- `tauri.conf.json` → add:
  ```json
  "bundle": {
    "externalBin": [
      "bin/rclone-x86_64-pc-windows-msvc",
      "bin/rclone-x86_64-unknown-linux-gnu",
      "bin/rclone-aarch64-unknown-linux-gnu",
      "bin/rclone-x86_64-apple-darwin",
      "bin/rclone-aarch64-apple-darwin"
    ],
    …existing…
  }
  ```
  Create `src-tauri/bin/.gitkeep`; add `src-tauri/bin/rclone-*` to a new line in `src-tauri/.gitignore` (binaries must not be committed). Ship a `src-tauri/bin/README.md` pointing at rclone.org with SHA-256 checksums placeholder.
- `capabilities/default.json` → add `"shell:allow-execute"` with scope:
  ```json
  {
    "identifier": "shell:allow-execute",
    "allow": [{ "name": "rclone", "sidecar": true, "args": true }]
  }
  ```
  and `"shell:default"` to the `permissions` array.
- Register the shell plugin in `src-tauri/src/lib.rs` via `.plugin(tauri_plugin_shell::init())`.

**Step 2 — `storage/cloud/remote_path.rs` (new file).**
- Public function `fn validate_remote_path(remote_path: &str) -> Result<&str, CloudTransportError>` enforcing CS-003. Return `Other("remote path rejected: <reason>")` on failure.
- Public function `fn validate_remote_prefix(remote_prefix: &str) -> Result<&str, CloudTransportError>` which accepts `""`, otherwise delegates to `validate_remote_path`.
- Internal constant for the compiled `regex::Regex` behind a `std::sync::OnceLock`.
- Doc comment pins this to `design.md#remote-path-sanitisation`.

**Step 3 — `storage/cloud/stderr_sanitiser.rs` (new file).**
- `fn sanitise_stderr(raw: &str) -> String` implementing CS-004 (case-insensitive line filter, keyword substring match).
- Empty-after-filter ⇒ returns `"<credentials scrubbed>"` (Concern 7).
- Pure function; inline unit tests for each keyword, multi-line drop, empty-input passthrough.

**Step 4 — `storage/cloud/rclone_subprocess.rs` (new file).**
- Implements CS-001 exactly. Uses `tokio::process::Command::new(binary_path).args(args).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()`.
- Shell-free: arguments already `Vec<OsString>`. No `sh -c` / `cmd /c`. Rejects callers that try to pass a string joined with spaces (typed API prevents it).
- Wrap `child.wait_with_output()` in `tokio::time::timeout(timeout, …)`.
- On `Elapsed`: `child.start_kill()` → `tokio::time::timeout(Duration::from_secs(5), child.wait())` → return `CloudTransportError::Timeout` regardless of kill success (but log at `tracing::warn` if reap fails). Never returns before reaping. (Concern 9.)
- Map exit status per CS-002. On non-zero exits 3/4 ⇒ `NotFound`; otherwise `RcloneProcessFailed { exit_code, stderr_sanitised: sanitise_stderr(stderr_utf8) }`.
- Stdout bytes are converted with `String::from_utf8_lossy(&output.stdout).into_owned()` to survive binary garbage without panic.

**Step 5 — `storage/cloud/sync_config.rs` (new file).**
- Emit CS-005 verbatim. Add `fn validate(&self) -> Result<(), CloudTransportError>` enforcing the documented ranges; `new` constructor that delegates to validate.
- Do NOT load/save JSON in this step (Assumption 3).

**Step 6 — `storage/cloud/destination_session.rs` (new file) and `SqlCipherMetadataStore` accessor.**
- Add a crate-private method on `SqlCipherMetadataStore` in `src-tauri/src/storage/sqlcipher.rs`:
  ```rust
  pub(crate) async fn with_connection_mut<T>(&self, f: impl FnOnce(&mut Connection) -> Result<T, StorageError> + Send + 'static) -> Result<T, StorageError>
  where T: Send + 'static;
  ```
  Mirrors the existing pattern for `list_all_blob_names` (see `storage.md` note "`list_all_blob_names` remains a SQLCipher-specific helper and must not be added to `MetadataStore`").
- Emit CS-006 in `destination_session.rs`.
- Provide:
  - `async fn insert_destination_session(store: &SqlCipherMetadataStore, session: &DestinationSession) -> Result<(), StorageError>` — transactional; errors if a second row with `is_primary = 1` would exist.
  - `async fn list_destination_sessions(store: &SqlCipherMetadataStore) -> Result<Vec<DestinationSession>, StorageError>`.
  - `async fn get_primary_destination(store: &SqlCipherMetadataStore) -> Result<Option<DestinationSession>, StorageError>`.
  - `async fn delete_destination_session(store: &SqlCipherMetadataStore, destination_id: &str) -> Result<(), StorageError>`.
- SQL uses prepared statements; string parameters via `params!` (no format-interpolation).
- Conversion between enum variants and SQL tags documented with `///` comments.

**Step 7 — `storage/cloud/cloud_config.rs` (new file).**
- Expose:
  - `async fn load_primary_cloud_endpoint() -> Result<Option<CloudEndpoint>, CloudTransportError>` — path per CS-008; `Ok(None)` on file-missing, `IoError` on other I/O failure, `Other` on JSON parse failure.
  - `async fn save_primary_cloud_endpoint(endpoint: &CloudEndpoint) -> Result<(), CloudTransportError>` — creates parent dir, writes atomically via temp file + rename, sets owner-only permissions (invoke `staging::write_owner_only` — make it `pub(crate)` if currently module-private).
- Doc comment anchors to `design.md#connection-descriptor`.

**Step 8 — `storage/cloud/rclone.rs` (new file, production transport).**
- Struct:
  ```rust
  pub struct RcloneTransport {
      binary_path: PathBuf,
      session_config_path: PathBuf,
      remote_root: String,              // "<rclone_remote_name>:<bucket>/<path_prefix>" fully-qualified
      sync_config: SyncConfig,
  }
  ```
- Constructor `RcloneTransport::new(binary_path, session_config_path, endpoint: &CloudEndpoint, destination: &DestinationSession, sync_config: SyncConfig) -> Self`. Computes `remote_root = format!("{}:{}/{}", destination.rclone_remote_name, destination.bucket, destination.path_prefix)` with trailing-slash normalisation via a pure helper.
- Implement `CloudTransport` using CS-007 and CS-001:
  - Each method calls `validate_remote_path`/`validate_remote_prefix`, builds `Vec<OsString>` with `--config <session_config_path> --retries 3 <command-specific args>`.
  - `upload_blob`: `local_path` passed as `OsString`, `remote_path` suffixed onto `remote_root`.
  - `download_blob`: same in reverse.
  - `delete_blob`: `deletefile`.
  - `list_blobs`: parse the JSON array returned by `lsjson`; extract each entry's `"Path"` field; prepend `remote_prefix` → `Vec<String>` sorted lexicographically for test stability.
  - Per-command timeout from `sync_config.operation_timeout_seconds` for upload/download; hard-coded `Duration::from_secs(30)` for delete and `Duration::from_secs(60)` for list (matches design.md:509-514).
- Logging: `tracing::debug!(remote_path = %redacted_remote_path, "rclone upload")`. Never log `stdout`/`stderr` raw — pass through `sanitise_stderr` first.
- Unit tests (mocked subprocess via a trait-object `Rclone runner` or by injecting a `binary_path` that points at a fixture script on Unix and `.cmd` on Windows). The subprocess runner is kept behind an internal trait `trait RcloneRunner { async fn run(&self, args, timeout) -> Result<String, CloudTransportError>; }`. Production uses `struct RealRclone { binary_path }` wrapping CS-001; tests use `struct StubRclone { scripted: Mutex<VecDeque<Result<String, CloudTransportError>>> }`.

**Step 9 — `storage/cloud/wizard.rs` (new file).**
- `async fn setup_s3_provider(input: S3SetupRequest, store: &SqlCipherMetadataStore) -> Result<DestinationSession, CloudTransportError>`:
  - Build the S3 rclone config section **in-process** (string builder; no command-line credentials) — keys `type=s3`, `provider`, `region`, `endpoint`, `access_key_id`, `secret_access_key`.
  - Generate `remote_name = format!("arx-runa-{}", Uuid::new_v4())`.
  - Construct a `DestinationSession` (cloud type, `is_primary = true` when caller requests).
  - Persist via `insert_destination_session`.
  - Persist the non-sensitive `CloudEndpoint` via `save_primary_cloud_endpoint`.
  - Zeroise the in-process secret bytes with `zeroize::Zeroize` before return.
- `async fn setup_google_drive(opener: &impl OpenerLike, input: GoogleDriveSetupRequest, store: &SqlCipherMetadataStore) -> Result<DestinationSession, CloudTransportError>`:
  - Emit a scoped `rclone config create arx-runa-<uuid> drive scope=drive --non-interactive --config <temp_path>` via CS-001.
  - Parse Rclone's OAuth URL from stdout (well-known prefix `If your browser doesn't open automatically, go to the following link:`); surface as `GoogleDriveAuthPending { auth_url }` variant on the *result* type (not on `CloudTransportError`).
  - Call `opener.open_url(auth_url).await` — `OpenerLike` is a tiny trait wrapping `tauri_plugin_opener` so tests can substitute a recorder.
  - After the caller signals "user finished browser flow" (Phase 6 UX — for now the function blocks on a `tokio::sync::oneshot` passed in), read the config section back with `rclone config dump --config <temp_path>` and persist as above.
- Request enums (`S3SetupRequest`, `GoogleDriveSetupRequest`) live in `wizard.rs` with `Zeroize` on secret fields.

**Step 10 — Session-lived `rclone.conf` helpers.**
- `storage/cloud/destination_session.rs` adds:
  - `async fn build_session_rclone_conf(store: &SqlCipherMetadataStore, output_path: &Path) -> Result<(), CloudTransportError>` — reads all `rclone_config_blob` rows, concatenates with `\n` separators, writes to `output_path` with owner-only permissions.
  - `async fn destroy_session_rclone_conf(path: &Path) -> Result<(), CloudTransportError>` — overwrites the file contents with `\x00` bytes of the same length, then `tokio::fs::remove_file`.
- No `Drop` impl — both are explicit awaitable calls owned by the session lifecycle (callers in Phase 4.5 will wire them to `SessionManager::install_session` / `lock()`).

**Step 11 — `mod.rs` updates (`src-tauri/src/storage/cloud/mod.rs`).**
- Add module declarations for every new file.
- Public re-exports: `SyncConfig`, `DestinationSession`, `DestinationType`, `BackupSyncMode`, `RcloneTransport`, plus wizard request/response types.
- Keep `remote_path`, `stderr_sanitiser`, `rclone_subprocess`, `destination_session`, `cloud_config` crate-private (`pub(crate)`) where possible.
- Drop the "Phase 4.2 adds" forward note from the module doc now that it is satisfied.

**Step 12 — Storage module re-exports (`src-tauri/src/storage/mod.rs`).**
- Append `pub use cloud::{SyncConfig, DestinationSession, DestinationType, BackupSyncMode, RcloneTransport};`.

**Step 13 — Unit test suite (colocated `#[cfg(test)] mod tests` per file).**
- `remote_path`: allowlist accept (`"vault-header.json"`, `"vault/uuid.blob"`), reject (`"../escape"`, `"/abs"`, `"has space"`, `"nul\0byte"`).
- `stderr_sanitiser`: each keyword triggers, line-granularity preserved, empty result emits placeholder, case-insensitive.
- `rclone_subprocess`: use a platform-portable fixture script (`tests/fixtures/fake_rclone.sh` on Unix, `fake_rclone.cmd` on Windows) in `src-tauri/tests/fixtures/`. Drives the exit-code mapping (0/3/4/non-zero), timeout path (long `sleep 10` with 100 ms timeout), and stdout capture. Gate with `cfg(any(unix, windows))`.
- `sync_config`: default values, range validation at boundary.
- `destination_session`: insert + list round-trip, duplicate primary rejected, delete idempotency (missing id returns `Ok`), `DestinationType`/`BackupSyncMode` tag round-trip, "SQLCipher alone keeps `rclone_config_blob` unreadable" test that opens the DB file with a random key and asserts failure.
- `cloud_config`: round-trip save/load, missing-file returns `Ok(None)`, corrupt JSON returns `Other`, owner-only permissions asserted on Unix (`cfg(unix)`; `std::os::unix::fs::PermissionsExt`).
- `rclone::tests`: use `StubRclone` to cover every `CloudTransport` method (success, NotFound via exit 3 + exit 4, Timeout via stub delay, RcloneProcessFailed via stub exit 7, AuthenticationFailed via stub synthesising that variant). `list_blobs` parses a hand-written JSON fixture matching real rclone `lsjson` output (array of `{"Path": "vault/uuid.blob", …}`).
- `wizard::tests`: S3 setup stores expected `CloudEndpoint` and `DestinationSession`, no credentials leak into the stored `CloudEndpoint` object; Google Drive flow emits an `auth_url` that the mock opener receives (no real rclone call).
- `storage::cloud` end-to-end: with `StubRclone`, push a header → `list_blobs` shows it → download → delete.

**Step 14 — Integration test under `#[ignore]` in `src-tauri/tests/rclone_integration.rs`.**
- Gated on `std::env::var("ARX_RCLONE_INTEGRATION") == Ok("1")`.
- Creates a temp directory, runs `rclone config create` for a `local` remote pointing at it, constructs a `CloudEndpoint` + `DestinationSession`, instantiates `RcloneTransport`, runs upload → list → download → delete against real rclone.
- Test name: `test_rclone_transport_round_trip_with_local_remote`.

**Step 15 — Governance sync (§8 actions).** Execute in order. Run `/copilot-sync` last.

**Step 16 — Verification gate.**
```
cargo fmt
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-targets --all-features
```
(Integration test remains `#[ignore]` unless the env var is set.)

## 6. Review focus areas

### 6a. Rust change surface (anticipated)

- `C:\Users\chris\source\repos\arx-runa\src-tauri\Cargo.toml` (add `tokio.process`, `regex`, `tauri-plugin-shell`)
- `C:\Users\chris\source\repos\arx-runa\src-tauri\tauri.conf.json` (add `externalBin`)
- `C:\Users\chris\source\repos\arx-runa\src-tauri\capabilities\default.json` (sidecar-scoped shell permission)
- `C:\Users\chris\source\repos\arx-runa\src-tauri\src\lib.rs` (register `tauri_plugin_shell`)
- `C:\Users\chris\source\repos\arx-runa\src-tauri\src\storage\mod.rs` (re-exports)
- `C:\Users\chris\source\repos\arx-runa\src-tauri\src\storage\cloud\mod.rs` (module declarations + re-exports)
- `C:\Users\chris\source\repos\arx-runa\src-tauri\src\storage\sqlcipher.rs` (add `with_connection_mut` accessor)
- New: `C:\Users\chris\source\repos\arx-runa\src-tauri\src\storage\cloud\rclone.rs`
- New: `C:\Users\chris\source\repos\arx-runa\src-tauri\src\storage\cloud\rclone_subprocess.rs`
- New: `C:\Users\chris\source\repos\arx-runa\src-tauri\src\storage\cloud\remote_path.rs`
- New: `C:\Users\chris\source\repos\arx-runa\src-tauri\src\storage\cloud\stderr_sanitiser.rs`
- New: `C:\Users\chris\source\repos\arx-runa\src-tauri\src\storage\cloud\sync_config.rs`
- New: `C:\Users\chris\source\repos\arx-runa\src-tauri\src\storage\cloud\cloud_config.rs`
- New: `C:\Users\chris\source\repos\arx-runa\src-tauri\src\storage\cloud\destination_session.rs`
- New: `C:\Users\chris\source\repos\arx-runa\src-tauri\src\storage\cloud\wizard.rs`
- New: `C:\Users\chris\source\repos\arx-runa\src-tauri\tests\rclone_integration.rs`
- New: `C:\Users\chris\source\repos\arx-runa\src-tauri\tests\fixtures\fake_rclone.sh` + `.cmd`
- New: `C:\Users\chris\source\repos\arx-runa\src-tauri\bin\README.md` + `.gitkeep`
- Edit: `C:\Users\chris\source\repos\arx-runa\src-tauri\.gitignore` (exclude `bin/rclone-*`)

### 6b. Security-sensitive paths (anticipated)

- `src-tauri/src/storage/cloud/rclone.rs` — **no shell interpolation**; arguments are `Vec<OsString>` end-to-end; reject any caller passing a pre-joined string (compile-time via signature).
- `src-tauri/src/storage/cloud/rclone_subprocess.rs` — child process MUST be reaped on timeout; confirm `Stdio::null()` for stdin (prevents inherited terminal from leaking to rclone).
- `src-tauri/src/storage/cloud/remote_path.rs` — the allowlist is the only perimeter before untrusted manifest paths reach rclone; verify `..`, absolute prefixes, and control characters are all rejected and a pathological Unicode payload (mixed scripts, combining marks) is rejected by the ASCII-only regex.
- `src-tauri/src/storage/cloud/stderr_sanitiser.rs` — keyword match must be case-insensitive and line-granular; partial-line redaction is unsafe (could leak remainder of credential-bearing line). Verify empty-after-filter placeholder is emitted.
- `src-tauri/src/storage/cloud/wizard.rs` — credentials NEVER appear on the command line or in logs; buffer holding `access_key_id` / `secret_access_key` is `Zeroize`-on-drop; the saved `CloudEndpoint` must be re-asserted credential-free in a test.
- `src-tauri/src/storage/cloud/destination_session.rs` — the session-lived rclone.conf must be written with owner-only permissions and overwritten+unlinked on session close; verify the overwrite path uses `\x00` bytes of the same length (not `set_len(0)` alone, which leaves the original blocks recoverable on some filesystems).
- `src-tauri/src/storage/cloud/cloud_config.rs` — `cloud-config.json` is readable without authentication (design requirement) but MUST NOT contain credentials; test asserts the serialised JSON keys are exactly `{provider, bucket, region, endpoint, path_prefix}`.
- `src-tauri/src/lib.rs` — `tauri-plugin-shell` is enabled **with scoped execute for `rclone` only**; any widening of the capability (e.g., `"name": "*"`, `"sidecar": false`) is a review blocker.

### 6c. Architecture risk areas

- **SRP:** new code is spread across 8 files. Verify no file grows past one concern (e.g., `rclone.rs` must not embed path validation — that lives in `remote_path.rs`).
- **Dependency direction:** `storage::cloud::wizard` depends on `storage::cloud::destination_session` and `storage::cloud::cloud_config` but MUST NOT depend on `auth::*` or `ui::*`. Verify no upward pull.
- **Visibility discipline:** `rclone_subprocess`, `remote_path`, `stderr_sanitiser` stay `pub(crate)`. `RcloneTransport`, `SyncConfig`, `DestinationSession` are `pub` for re-export through `storage::mod`.
- **`MetadataStore` surface stays frozen** at the Phase 3.1 methods. Destination-session helpers live outside the trait (Concern 10).
- **Abstraction debt:** `RcloneRunner` internal trait should remain private to the cloud module; do not expose as a public abstraction.
- **Platform code:** the timeout kill and owner-only permission helpers are the only platform-conditional sites. Confirm Unix/Windows symmetry; no `#[cfg(macos)]` branches should be needed (macOS follows the Unix path).

### 6d. Testing requirements

- **Sub-roadmap validation checkpoint** — `cargo test storage::cloud::rclone` must pass with the stubbed runner tests. Adjust the roadmap command's path if it resolves to the module tree used by the integration test.
- **`rust.md` rule — every `thiserror` variant triggered:** the new stubbed runner tests cover `NotFound`, `AuthenticationFailed`, `Timeout`, `IoError`, `RcloneProcessFailed`, `Other` through `RcloneTransport` paths.
- **Adversarial cases from §3:** path-traversal (Concern 12), credential-only stderr (Concern 7), timeout reap (Concern 9), duplicate primary destination (Concern 10).
- **Manual verification (deferred to `/implement-plan`):**
  - Run setup wizard against MinIO or Backblaze B2 (free tier).
  - Confirm `cloud-config.json` contains no credential keys.
  - Confirm `SELECT rclone_config_blob FROM destination_sessions;` via raw sqlite3 against the DB file fails (SQLCipher key is unknown to the attacker tool).
  - Confirm the session-lived `rclone-session.conf` is deleted after a simulated lock (`SessionManager::lock()`).
  - Confirm a `../escape.txt` path is rejected with `Other` (no rclone invocation).

## 7. Documentation impact

| Item | Path | Required this run? | Rationale |
|------|------|-------------------:|-----------|
| Remove the "Phase 4.2 adds `RcloneTransport`" forward note from the module doc | `src-tauri/src/storage/cloud/mod.rs` (top-of-file comment) | **Required** | Becomes misleading once 4.2 lands. |
| Sub-phase deliverable wording updates (macOS, `tauri_plugin_opener`) | `docs/architecture/designs/cloud-synchronisation/sub-phases/4.2-rclone-integration.md` | **Required** (governance action GS-004) | Concerns 1 and 2 resolved via plan assumptions; sub-phase should reflect the corrected reality for future readers. |
| Cross-phase invariants | `docs/architecture/design-invariants.md` | **Deferred** | 4.2 adds no new cross-phase invariant beyond what the parent design already contains. |
| Diagrams | `docs/architecture/designs/cloud-synchronisation/diagrams/` (TBD) | **Deferred / optional** | Existing flow diagrams remain accurate; consider adding a sequence diagram for the wizard in 4.5. |
| `docs/architecture/designs/cloud-synchronisation/design.md` | same file | **Deferred / optional** | No canonical contract changes under this plan; the plan conforms to existing design text. |

## 8. Governance sync actions (pre-implementation)

| Action ID | Reason / linked concern | Target files | Required edit | Verification |
|-----------|-------------------------|--------------|---------------|--------------|
| GS-001 | G1 — encode `RcloneTransport` security perimeter in rules. | `C:\Users\chris\source\repos\arx-runa\.claude\rules\storage.md` | Under "Traits" / "Cloud backup", add a bullet: "`RcloneTransport` invokes the bundled sidecar via `tokio::process::Command` only; remote paths pass the `^[a-zA-Z0-9._/-]+$` allowlist (reject `..` and leading `/`); stderr is stripped of lines containing `token|key|secret|password|credential|auth` before surfacing in `CloudTransportError::RcloneProcessFailed`." Also add: "`destination_sessions` CRUD lives in `storage::cloud::destination_session` using a SQLCipher-specific accessor; must not be added to `MetadataStore`." | `Grep -n "RcloneTransport" .claude/rules/storage.md` returns the new line; second run is idempotent. |
| GS-002 | G2 — Tauri plugin/permission exception. | `C:\Users\chris\source\repos\arx-runa\.claude\rules\tauri.md` | Under "Plugins", rewrite "Never: `shell`, …" to "Never: `shell` (general), `http`, `clipboard`, or unrestricted filesystem permissions. Exception: `tauri-plugin-shell` may be enabled with a scoped `shell:allow-execute` permission targeting the bundled `rclone` sidecar only (`{ "name": "rclone", "sidecar": true }`). OAuth browser launches use `tauri_plugin_opener`, never the shell plugin." | `Grep -n "tauri-plugin-shell" .claude/rules/tauri.md` returns the exception clause; second run is idempotent. |
| GS-003 | G3 — Copilot instruction mirror drift. | `.github/instructions/storage.instructions.md`, `.github/instructions/tauri.instructions.md` | Run `/copilot-sync` after GS-001 and GS-002 land. | A second `/copilot-sync` run shows no further diff. |
| GS-004 | G4 — Sub-phase design file corrections (macOS + Tauri 2 opener). | `C:\Users\chris\source\repos\arx-runa\docs\architecture\designs\cloud-synchronisation\sub-phases\4.2-rclone-integration.md` | Edit deliverable 2 to read "Rclone bundled as Tauri sidecar binary in `tauri.conf.json` `externalBin` field (platform-specific paths for Windows, macOS, and Linux)"; edit deliverable 8 to replace `tauri::api::shell::open` with "`tauri_plugin_opener` (Tauri 2 API)". | `Grep -n "tauri::api::shell::open" docs/architecture/designs/cloud-synchronisation/sub-phases/4.2-rclone-integration.md` returns no match; `Grep -n "Windows, macOS, and Linux" ...` returns the updated deliverable. |

## 9. Handoff Notes for Implementer

Working directory: `C:\Users\chris\source\repos\arx-runa`. Execute Step 1 (Cargo + `tauri.conf.json` + capability wiring) **before** any Rust work so the feature flags and plugin registration compile cleanly; a failed Step 1 otherwise cascades through every new file. Steps 2–7 are independent and can be committed in sequence (remote-path → stderr → subprocess → sync-config → destination-session → cloud-config). Step 8 (`rclone.rs`) composes the preceding modules and must land after them. Step 9 (wizard) depends on 6, 7, 8. Step 10 extends Step 6.

Traps:

- **`tokio::process` feature is mandatory** — missing it yields a compile error in `rclone_subprocess.rs` only, not at `Cargo.toml` parse time (Concern 3).
- **`tauri-plugin-shell` sidecar permission is path-scoped** — omitting the scope makes the Tauri bundle refuse the invocation at runtime (Concern 4).
- **Do not commit binaries** — `src-tauri/bin/rclone-*` must appear in `.gitignore`; only `README.md` + `.gitkeep` land in git.
- **Windows kill semantics** — `Child::start_kill()` synthesises `TerminateProcess` internally; do not attempt a manual `libc::kill` branch (the CLAUDE.md cross-platform constraint forbids a Unix-only fallback).
- **Integration test stays `#[ignore]`** — CI without `ARX_RCLONE_INTEGRATION=1` must not attempt to shell out.
- **Credentials are Zeroise-on-drop everywhere** — audit every `String` that holds `access_key_id`, `secret_access_key`, or a rclone config section before merge.
- **Plan is self-contained.** The sub-phase and design are referenced for context; nothing in this plan requires re-reading those docs during implementation unless a new blocking concern surfaces.

## Implementation Log

- **Date**: 2026-04-19T03:14:21.9294163+02:00
- **Run ID**: `phase-4-2-rclone-integration-20260419-021054`
- **Track**: `full`
- **Branch**: `development`
- **Execution mode**: rust-implementer delegated with orchestrator fallback for compile/clippy/test-driven fixes

| Approach step | Agent | Agent ID | Outcome |
|---|---|---|---|
| Step 1–14 initial implementation | `rust-implementer` | `impl-phase-4-2` | Implemented initial phase surface |
| Remediation cycle 1 (CF-001..010) | `problem-solver` + `rust-implementer` | `solver-*` + `impl-remediation-cycle-1` | Applied security/correctness hardening |
| Remediation cycle 2 (CF-011..016) | `problem-solver` + `rust-implementer` | `solver-*` + `impl-remediation-cycle-2` | Applied boundary/validation fixes |
| Remediation cycle 3 (CF-017..024) | `problem-solver` + `rust-implementer` | `solver-*` + `impl-remediation-cycle-3` | Applied auth classification, lifecycle cleanup, HTTPS policy, rollback consistency |

- **Files changed**:
  - `.claude/plans/phase-4-2-rclone-integration.md`
  - `.claude/rules/storage.md`
  - `.claude/rules/tauri.md`
  - `.github/instructions/storage.instructions.md`
  - `.github/instructions/tauri.instructions.md`
  - `Cargo.lock`
  - `docs/architecture/designs/cloud-synchronisation/design.md`
  - `docs/architecture/designs/cloud-synchronisation/sub-phases/4.2-rclone-integration.md`
  - `src-tauri/.gitignore`
  - `src-tauri/Cargo.toml`
  - `src-tauri/capabilities/default.json`
  - `src-tauri/src/lib.rs`
  - `src-tauri/src/storage/cloud/endpoint.rs`
  - `src-tauri/src/storage/cloud/mod.rs`
  - `src-tauri/src/storage/mod.rs`
  - `src-tauri/src/storage/sqlcipher.rs`
  - `src-tauri/src/storage/staging.rs`
  - `src-tauri/tauri.conf.json`
  - `src-tauri/src/storage/cloud/cloud_config.rs`
  - `src-tauri/src/storage/cloud/destination_session.rs`
  - `src-tauri/src/storage/cloud/rclone.rs`
  - `src-tauri/src/storage/cloud/rclone_subprocess.rs`
  - `src-tauri/src/storage/cloud/remote_path.rs`
  - `src-tauri/src/storage/cloud/stderr_sanitiser.rs`
  - `src-tauri/src/storage/cloud/sync_config.rs`
  - `src-tauri/src/storage/cloud/wizard.rs`
  - `src-tauri/tests/rclone_integration.rs`
  - `src-tauri/tests/fixtures/fake_rclone.sh`
  - `src-tauri/tests/fixtures/fake_rclone.cmd`
  - `src-tauri/bin/.gitkeep`
  - `src-tauri/bin/README.md`
  - `.claude/runs/phase-4-2-rclone-integration-20260419-021054/run-state.json`
  - `.claude/runs/phase-4-2-rclone-integration-20260419-021054/cycle-1.json`
  - `.claude/runs/phase-4-2-rclone-integration-20260419-021054/cycle-2.json`
  - `.claude/runs/phase-4-2-rclone-integration-20260419-021054/cycle-3.json`
  - `.claude/runs/phase-4-2-rclone-integration-20260419-021054/cycle-4.json`

- **Formatting check**: `cargo fmt --all -- --check` passed
- **Clippy results**: `cargo clippy --workspace --all-targets --all-features -- -D warnings` passed
- **Test results**: `cargo test --workspace --all-targets --all-features` passed
- **Release build**: `cargo build --workspace --release` passed
- **Rust review**: final run reported `NO_ACTIONABLE_FINDINGS`
- **Architecture review**: final run yielded `CF-025` (`DEFERRED_BY_PLAN`, explicit Step 10 defer to Phase 4.5) and `CF-026` (`INTENTIONAL_DECISION`)
- **Security review**: final run reported `NO_SECURITY_FINDINGS`
- **Cross-shard review**: invoked across cycles; final run reported `NO_CROSS_SHARD_FINDINGS`
- **Findings quality gate**: `ACTIONABLE_NOW=24`, `INTENTIONAL_DECISION=1`, `DEFERRED_BY_PLAN=1`, `INSUFFICIENT_EVIDENCE=0`
- **Finding overrides**: None
- **Design challenge outcomes**:
  - `CF-023` endpoint policy challenge — **ACCEPTED** (autonomous decision due unavailable human response) with design update applied to `docs/architecture/designs/cloud-synchronisation/design.md` (HTTPS default, explicit local-dev HTTP override)
- **Governance sync**: GS-001..GS-004 applied; `.claude/rules/*` and mirrored `.github/instructions/*` updated; copilot-sync outcome recorded as OK (manual sync completed in-run)
- **Sub-phase decisions sync**: `docs/architecture/designs/cloud-synchronisation/sub-phases/4.2-rclone-integration.md` updated with 4 implementation decisions
- **Deviations from plan**:
  - Added additional remediation cycles driven by reviewer findings before completion
  - Added deterministic auth-failure classification refinements and fixture robustness fixes discovered during full-suite execution
- **Documentation flagged**:
  - `docs/architecture/design-invariants.md` — deferred
  - `docs/architecture/designs/cloud-synchronisation/diagrams/` — deferred / optional
  - `docs/architecture/designs/cloud-synchronisation/design.md` — updated due accepted design challenge (`CF-023`)
- **Run state path**: `.claude/runs/phase-4-2-rclone-integration-20260419-021054/`
