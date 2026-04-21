---
title: "Phase 6.5 — Backend Command Wiring"
created: "2026-04-21T00:00:00Z"
status: approved
roadmap-phase: 6
sub-phase: "6.5"
design-document: "docs/architecture/designs/tauri-ipc-and-frontend/design.md"
sub-phase-roadmap: "docs/architecture/designs/tauri-ipc-and-frontend/sub-phases/roadmap.md"
governance-sync-required: true
tags: [tauri, ipc, backend-wiring, sessions, device-events, progress-channel, sharing, sync]
---

## 1. Goal

Wire the 29 Tauri IPC command stubs in `src-tauri/src/ui/` to their Phase 2–5 backend entry points, emit `device-event` through `AppHandle::emit` from the `DeviceMonitor` stream, and drive `tauri::ipc::Channel<ProgressUpdate>` / `Channel<SyncProgressUpdate>` / `Channel<MigrationProgress>` from `storage::vault_ops` and `storage::cloud` so that end-to-end authenticate → browse → upload → download → sync → share flows become functional against a real local vault.

---

## 2. Context

**Phase type**: sub-phase 6.5 under `tauri-ipc-and-frontend` design.

**Sub-phase dependencies** (strict, per roadmap): 6.1 → 6.2 → 6.3 → 6.4 → 6.5.

**State after Phase 6.4**:
- All 29 command handlers live in `src-tauri/src/ui/{auth,file,sync,destination,sharing}_commands.rs`, registered in `src-tauri/src/lib.rs` (lib.rs:18–53), and allowlisted in `src-tauri/build.rs`.
- Every handler returns `IpcError::InternalError("command not yet wired")` except `lock_session` (fully wired), `get_session_status` (partial — `is_unlocked` only), and `get_sync_status` (reads cached state).
- `AppState` (src-tauri/src/ui/state.rs:22–33) is assembled in `AppState::construct_default()` with `NoOpCloudTransport` and a platform-selected `DeviceMonitor`. Cloud transport stays `NoOpCloudTransport` at runtime; `RcloneTransport` exists but is unreferenced from `AppState`.
- `tauri::Builder::default()` in `lib.rs` has no `.setup()` hook, so `AppHandle` is not captured anywhere inside `src-tauri/`.
- Password-zeroization pattern at the IPC boundary is already implemented (`Zeroizing::new(password.into_bytes())`, `std::mem::take(&mut password)` + fill-with-zero in Rust-side String bytes) — commands inherit it verbatim.
- Ceremonies (`auth::ceremonies::{create_vault, change_password, rotate_key_file, recover_vault, setup_recovery, recover_with_phrase}`) all exist and install session keys via `SessionManager::reserve_session_install()` + `finalize_session_install()` or `swap_active_session()`.
- `SessionManager` already exposes `with_key_encryption_key<F, R>` and `with_sqlcipher_key<F, R>` crate-visible closure accessors (manager.rs:434–468). There is no `with_manifest_key` accessor, no `remaining_seconds()` accessor, and no `active_vault_id()` accessor.
- `storage::vault_ops::{upload_file, download_file, delete_file}` accept `&dyn MetadataStore` + `&KeyEncryptionKey` + staging/blob directories but take no progress callback.
- `storage::cloud::{push_vault, pull_vault, delete_vault_from_cloud}` take `sqlcipher_key` + `manifest_key` + `&SqlCipherMetadataStore` + `&dyn CloudTransport` + staging dir.
- `storage::cloud::destination_session::{insert, list, get_primary, delete}` are SQLCipher-specific helpers (per `.claude/rules/storage.md`).
- `sharing::{create_share_package, import_share_package, revoke_share, strong_revoke_share}` are `pub(crate)` and accept `&dyn MetadataStore` + `&dyn SharingStore` + `&KeyEncryptionKey`.
- `DeviceMonitor::watch()` returns `Pin<Box<dyn Stream<Item = DeviceEvent> + Send>>` with `DeviceEvent::{Mounted{mount_path}, Unmounted{mount_path}}`.
- Zero-Trace audit suite (`src-tauri/src/ui/security_audit.rs`) is in place and MUST continue to pass — no new plaintext sensitive identifiers in the IPC layer.
- Backoff gate (Phase 6.4) applies on every authenticate path.

**Deferred items picked up here** (from the sub-phase table): wire `authenticate`/`create_vault`/`list_directory`/`upload_file`/`download_file`/`delete_file`/`sync_to_cloud`/`recover_from_cloud`/`get_sync_status`/`change_password`/`rotate_key_file`/`lock_session`/`get_session_status`/`add_destination`/`list_destinations`/`delete_destination`/all sharing commands, plus the `AppHandle::emit("device-event")` bridge and `tauri::ipc::Channel<ProgressUpdate>` plumbing.

**Parent design anchors**:
- `## Canonical Command Surface (Normative)` (design.md:89–101) — canonical for command membership.
- `## Command Signatures` (design.md:103–361) — illustrative signatures; must align with Canonical Command Surface.
- `## IpcError Enum` + `## Error Mapping via From Traits` (design.md:463–591).
- `## IPC Response Types` (design.md:594–831).
- `## Application State` (design.md:835–867) — `database`, `cloud_transport`, `device_monitor`, `session_manager`, `sync_status`.
- `## Input Validation` (design.md:882–951) — allowlist + explicit traversal/absolute rejection; already wired in Phase 6.1.
- `## Zero-Trace Compliance` (design.md:1569–1627) — no new sensitive identifiers; audit tests must remain green.

---

## 3. Design Concerns / Open Questions

| # | Concern | Source | Impact | Classification | Resolution | Documentation updates |
|---|---|---|---|---|---|---|
| C1 | **No `with_manifest_key` accessor on `SessionManager`.** `push_vault` / `pull_vault` require `manifest_key: &[u8; 32]`, and sharing re-encryption does not, but sync commands need it from the active session. | `src-tauri/src/auth/session/manager.rs:434–468`; `storage::cloud::sync::{push_vault, pull_vault}` take `manifest_key`. | Sync commands cannot access the manifest key without a new accessor; adding a public `SessionKeys` getter breaks the sealed-keys invariant (`SessionKeys` is `pub(crate)` with no public accessor by design). | Non-blocking | Add `SessionManager::with_manifest_key<F, R>(closure) -> Result<R, AuthenticationError>` mirroring `with_key_encryption_key` (manager.rs:434–446). Callback receives `&[u8; 32]`; no key material escapes. Carry into Section 7 for rule propagation. | Update `.claude/rules/auth.md` `## Session` to list `with_manifest_key` alongside the existing two accessors. |
| C2 | **`AppHandle` is not captured anywhere in `src-tauri/`.** Sub-phase D2 requires `AppHandle::emit("device-event", payload)` from the `DeviceMonitor` stream; currently there is no `Builder::setup` hook, no `OnceLock<AppHandle>`, and `AppState` holds no handle. | `src-tauri/src/lib.rs:14–54` (no `.setup()` call); sub-phase 6.5 Deliverable 2. | Without capturing `AppHandle`, there is no way to emit window events from the spawned subscriber task; `KeyFileIndicator` would never receive insert/remove events. | Non-blocking | Add a `.setup(|app| { … })` closure in `lib.rs::run()` that: (a) stores `app.handle().clone()` into a new `AppState::app_handle: OnceLock<AppHandle>` field, (b) spawns `tokio::spawn(async move { pin_mut!(stream); while let Some(event) = stream.next().await { handle.emit("device-event", &payload)?; } })`, (c) uses the `&'static str` event name `"device-event"` and payload `{ kind: "mounted" \| "unmounted", mountPath: String }` (`#[serde(rename_all = "camelCase")]`). | Update `.claude/rules/tauri.md` `## IPC / UI layer` to document the `"device-event"` event contract. |
| C3 | **Single-vault `authenticate` IPC takes no `vault_id`; vault-db path must be resolved deterministically.** Multi-vault support is out of scope for Phase 6, but ceremonies and session install require `vault_db_path: PathBuf`, and vault-header lookup happens before `authenticate()`. | Parent design `## Command Signatures` `authenticate` (design.md:113–119); `CreateVaultRequest.vault_db_path` (ceremonies/types.rs:47). | Without a resolver, command handlers would guess paths or hard-code them, violating the CLAUDE.md cross-platform rule. | Non-blocking | Introduce `src-tauri/src/ui/vault_paths.rs` exporting `fn default_vault_root() -> PathBuf { dirs::data_dir().expect("data_dir").join("arx-runa").join("vaults") }` and `fn vault_db_path(vault_id: &str) -> PathBuf { default_vault_root().join(vault_id).join("vault.db") }`. `authenticate` scans `default_vault_root()` for exactly one sub-directory containing `vault.db` and `vault-header.json`; if zero or multiple, return `IpcError::InvalidInput("no vault configured" \| "multi-vault unsupported in Phase 6")`. `create_vault` reserves the path `default_vault_root().join(new_vault_id)`. | None (scope is Phase 6 only; multi-vault lookup is Phase 8 concern — tracked as deferred in parent roadmap). |
| C4 | **`tauri::ipc::Channel<ProgressUpdate>` threading through `vault_ops::upload_file` / `download_file`.** Pipeline does not yet accept a progress callback; passing `Channel` into `storage` crosses a boundary (storage must not know about `tauri::`). | `src-tauri/src/storage/vault_ops/upload_file.rs`, `download_file.rs`; sub-phase 6.5 Deliverable 3. | Direct import of `tauri::ipc::Channel` in `storage::` would create a reverse-layer dependency (IPC → storage is OK; storage → IPC is forbidden per `.claude/rules/rust.md` `## Patterns`). | Non-blocking | Add a plain callback parameter: `progress_callback: Option<&(dyn Fn(u64 /*bytes_processed*/, u64 /*bytes_total*/) + Send + Sync)>` to `upload_file` / `download_file` / `encrypt_file` / `decrypt_file`. The IPC handler wraps `Channel<ProgressUpdate>` into a closure that serialises `ProgressUpdate { percent, bytes_processed, bytes_total, status }`. Storage has no knowledge of `tauri::`. Same pattern for `Channel<SyncProgressUpdate>` in sync commands (callback signature: `Fn(u32 /*files_processed*/, u32 /*files_total*/, Option<&str>)`) and `Channel<MigrationProgress>` in `migrate_vault`. | Add a short note to `.claude/rules/storage.md` `## Pipeline` that `upload_file` / `download_file` accept an optional callback; storage must not depend on `tauri::`. |
| C5 | **`NoOpCloudTransport` is wired into `AppState` at construction; `RcloneTransport` is never swapped in.** After authenticate, the primary destination session stores the rclone config blob; the IPC layer must build an `RcloneTransport` for that session and mutate `AppState.cloud_transport`. | `src-tauri/src/ui/state.rs:88–112`; `storage::cloud::rclone::RcloneTransport`. | Sync / list_remote / delete_file cloud ops all dispatch through `AppState.cloud_transport` — without replacement, all cloud operations error with `"cloud transport not configured"`. | Non-blocking | Convert `AppState.cloud_transport: Arc<dyn CloudTransport>` → `Arc<RwLock<Arc<dyn CloudTransport>>>`. In `authenticate` / `create_vault` / `recover_from_cloud`, after session install: read primary destination session from SQLCipher, materialise an `rclone.conf` under `dirs::config_dir().join("arx-runa").join("rclone.conf")` (owner-only perms on Unix; Windows ACL inherited per staging-file precedent), construct `RcloneTransport::new(rclone_conf_path, remote_name)`, and `*cloud_transport.write().await = Arc::new(new_transport)`. On `lock_session` / `delete_vault`, reset to `NoOpCloudTransport`. | Add a note to `.claude/rules/tauri.md` `## IPC / UI layer` that `AppState.cloud_transport` is swappable post-authenticate, gated by session state. |
| C6 | **`get_session_status` cannot populate `vault_id` or `timeout_seconds`.** `SessionManager` has no `remaining_seconds()` accessor and no vault_id tracking. | `src-tauri/src/ui/auth_commands.rs` (partial stub); design.md `SessionStatus` type (design.md:610–620). | Frontend `SessionStatus` display relies on both fields. Partial stub violates the canonical contract. | Non-blocking | Add `SessionManager::remaining_seconds(&self) -> Option<u64>` (subtracts `now()` from the timer deadline tracked in the timer context; returns `None` if `NoSession`/`Expired`). Add `SessionManager::active_vault_id(&self) -> Option<String>` — store `vault_id: Option<String>` inside `SessionManager` alongside `session: SharedSession`, populated by `install_session` / `finalize_session_install` / `authenticate`, cleared by `lock`. | Update `.claude/rules/auth.md` `## Session` to mention the new accessors and the `vault_id` field on `SessionManager`. |
| C7 | **Sharing IPC commands need the active X25519 private key unwrap.** `sharing::create_share_package` / `import_share_package` / `revoke_share` take `&dyn MetadataStore` + `&dyn SharingStore` + `&KeyEncryptionKey`; the KEK comes from the session. | `src-tauri/src/sharing/packages.rs`; `.claude/rules/sharing.md` `## Identity ownership`. | Without an IPC-level helper, every sharing handler re-implements session-key extraction and risks leaking key material across `await` points. | Non-blocking | Wrap sharing flows in a single internal helper `ui::sharing_commands::with_active_kek_and_stores<F, R>(state, closure) -> Result<R, IpcError>` that: (a) validates `state.session_manager.state() == Active`, (b) opens/borrows the `SqlCipherMetadataStore` from `AppState.database`, (c) calls `session_manager.with_key_encryption_key(\|kek_bytes\| closure(kek_bytes, &store))`, (d) maps `SharingError::AuthenticationFailed` → `IpcError::AuthenticationFailed` per `.claude/rules/sharing.md` `## HPKE error hygiene`. | None (helper is internal to `ui::sharing_commands`). |
| C8 | **`vault_identity` read must not mutate.** Sharing reads `vault_identity.public_key`; the `.claude/rules/sharing.md` invariant says sharing code must never insert/update/delete that row. | `.claude/rules/sharing.md` `## Identity ownership`; `.claude/rules/auth.md` line 60. | If the sharing command handler accidentally upserts the identity row, invariant is violated. | Non-blocking | `export_public_key` and `share_file` handlers must call `storage::sharing::get_own_public_key(&store)` only; the row is created exclusively by `auth::ceremonies::create::create_vault`. No governance edit required — existing rules cover this. | None. |
| C9 | **Deferred: `get_file_content`, `list_remote`, `sync_backup`, `migrate_vault`.** Listed in the Canonical Command Surface; handlers exist as stubs but the parent design still places them under Phase 6. | Canonical Command Surface (design.md:93–99); sub-phase 6.5 Deliverables only enumerate the flows explicitly (Deliverable 1 says "each Tauri command"). | Excluding any of these would break the normative contract. | Non-blocking | Wire all 29 commands in a single pass. For `get_file_content` apply the 50 MiB cap per `.claude/rules/tauri.md` line 17 and base64-encode decrypted bytes; for `list_remote` use `CloudTransport::list_blobs(remote_prefix)` + manifest join; for `sync_backup` iterate destination sessions where `is_primary == false`; for `migrate_vault` use `CloudTransport::list_blobs` + sequential `upload_blob`/`delete_blob` on the new/old remotes. | None. |

Classification rationale: no finding forces a contract or design-invariant change; every gap is solvable with a bounded, additive code change. Status remains `draft` (not `blocked`).

---

## 4. Assumptions

- **A1** — `default_vault_root()` resolves to `dirs::data_dir().expect(...).join("arx-runa").join("vaults")` on all three platforms; single-vault lookup fails with `IpcError::InvalidInput` if zero or more than one sub-directory contains a `vault.db`.
- **A2** — `AppHandle` is captured via `Builder::setup()` and stored as `OnceLock<AppHandle>` on `AppState`; the device-event subscriber task is spawned exactly once during setup and lives for the process lifetime.
- **A3** — The device-event payload is `{ "kind": "mounted" \| "unmounted", "mountPath": String }` under `#[serde(rename_all = "camelCase")]`; frontend `KeyFileIndicator` consumes the same shape (verified in Phase 6.3).
- **A4** — Progress callbacks are `Option<&(dyn Fn(u64, u64) + Send + Sync)>` on `storage::vault_ops::{upload_file, download_file}` and `Option<&(dyn Fn(u32, u32, Option<&str>) + Send + Sync)>` on `storage::cloud::{push_vault, pull_vault, migrate}`; storage has no `tauri::` dependency.
- **A5** — `AppState.cloud_transport` is refactored to `Arc<RwLock<Arc<dyn CloudTransport>>>` so it can swap from `NoOpCloudTransport` → `RcloneTransport` on authenticate and back on lock/delete.
- **A6** — Rclone config materialisation uses `dirs::config_dir().join("arx-runa").join("rclone.conf")` with owner-only permissions (`0o600` on Unix via `std::os::unix::fs::PermissionsExt`; NTFS ACLs inherit from `arx-runa/` dir on Windows).
- **A7** — `SessionManager::remaining_seconds()` subtracts `tokio::time::Instant::now()` from the currently active deadline (stored in a new `timer_deadline: Arc<RwLock<Option<tokio::time::Instant>>>` field updated on every `reset_timer` / `restart_timer`); returns `None` outside `Active`.
- **A8** — `vault_id` is stored as `Option<String>` inside `SessionManager`, populated by ceremony install paths and `authenticate`; cleared in `lock` after key zeroization.
- **A9** — The `reset_timer()` invocation on every IPC call (per `.claude/rules/auth.md` line 36) is applied via a small helper wrapper `ui::commands_common::with_session_refresh(state, closure)` used by every command handler; long-running commands also hold `let _op = state.session_manager.begin_operation();` per rules.
- **A10** — Password bytes arrive as `String`, are converted to `Zeroizing<Vec<u8>>` using the existing Phase 6.4 pattern (see `src-tauri/src/ui/auth_commands.rs` existing `create_vault` stub structure), and no plaintext password or derived key lands in any log line or error message.
- **A11** — `base64::engine::general_purpose::STANDARD` is the canonical base64 encoder for `FileContent.data_base64`; MIME detection uses the `infer` crate on the first ~128 bytes of decrypted content.
- **A12** — `migrate_vault` uses `rclone copy`-equivalent loop via `CloudTransport::upload_blob` + `delete_blob`; manifest is unchanged because blobs are opaque ciphertext (per parent design `## Command Signatures` `migrate_vault` comment).

---

## 5. Approach

### CONTRACT_SNIPPETS (inlined once, verbatim)

> **CS-001** — Canonical Command Surface (29 commands across 5 domains):

| Domain | Commands |
|--------|----------|
| Auth | `authenticate`, `create_vault`, `change_password`, `rotate_key_file`, `delete_vault`, `lock_session`, `get_session_status` |
| File | `list_directory`, `upload_file`, `download_file`, `delete_file`, `get_file_content`, `list_remote` |
| Sync | `sync_to_cloud`, `recover_from_cloud`, `get_sync_status`, `migrate_vault`, `sync_backup` |
| Destination | `add_destination`, `list_destinations`, `delete_destination` |
| Sharing | `export_public_key`, `add_contact`, `list_contacts`, `share_file`, `import_share`, `revoke_share`, `list_shares`, `list_received_shares` |

> **CS-002** — `IpcError` enum (`src-tauri/src/ui/error.rs`):
```rust
#[derive(Debug, Serialize)]
#[serde(tag = "kind", content = "message")]
#[serde(rename_all = "camelCase")]
pub enum IpcError {
    VaultLocked(String),
    AuthenticationFailed(String),
    NotFound(String),
    AlreadyExists(String),
    CloudError(String),
    InvalidInput(String),
    InternalError(String),
}
```
`From<AuthenticationError>`, `From<StorageError>`, `From<SharingError>`, `From<SyncError>`, `From<CloudTransportError>` impls already exist (src-tauri/src/ui/error.rs).

> **CS-003** — `AppState` (after this phase):
```rust
pub struct AppState {
    pub(crate) database: Arc<RwLock<Option<SqlCipherMetadataStore>>>,
    pub(crate) cloud_transport: Arc<RwLock<Arc<dyn CloudTransport>>>, // refactored (see C5)
    pub(crate) device_monitor: Arc<dyn DeviceMonitor>,
    pub(crate) session_manager: Arc<SessionManager>,
    pub(crate) sync_status: Arc<RwLock<SyncStatus>>,
    pub(crate) app_handle: OnceLock<tauri::AppHandle>,   // new (C2)
    pub(crate) active_vault_id: Arc<RwLock<Option<String>>>, // mirrors SessionManager for quick reads (C6)
}
```

> **CS-004** — New `SessionManager` methods:
```rust
pub(crate) async fn with_manifest_key<F, R>(&self, cb: F) -> Result<R, AuthenticationError>
where F: FnOnce(&[u8; 32]) -> R;

pub async fn remaining_seconds(&self) -> Option<u64>;
pub async fn active_vault_id(&self) -> Option<String>;
```

> **CS-005** — `DeviceMonitor` interface and event payload:
```rust
// existing:
pub trait DeviceMonitor: Send + Sync {
    fn watch(&self) -> Pin<Box<dyn Stream<Item = DeviceEvent> + Send>>;
}
pub enum DeviceEvent { Mounted { mount_path: PathBuf }, Unmounted { mount_path: PathBuf } }

// new payload emitted to frontend:
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct DeviceEventPayload {
    kind: &'static str,         // "mounted" | "unmounted"
    mount_path: String,
}
```

> **CS-006** — Progress callback signatures threaded into storage:
```rust
pub async fn upload_file(
    source: &Path, node_id: NodeId, parent_id: Option<NodeId>, name: String,
    created_at: DateTime<Utc>, modified_at: DateTime<Utc>,
    metadata_store: &dyn MetadataStore,
    key_encryption_key: &KeyEncryptionKey,
    staging_directory: &Path,
    progress: Option<&(dyn Fn(u64, u64) + Send + Sync)>, // NEW
) -> Result<Node, StorageError>;

pub async fn download_file(
    destination: &Path, node_id: NodeId,
    metadata_store: &dyn MetadataStore,
    key_encryption_key: &KeyEncryptionKey,
    blob_directory: &Path,
    progress: Option<&(dyn Fn(u64, u64) + Send + Sync)>, // NEW
) -> Result<(), StorageError>;

pub async fn push_vault(
    vault_db_path: &Path, sqlcipher_key: &[u8; 32], manifest_key: &[u8; 32],
    metadata_store: &SqlCipherMetadataStore,
    transport: &dyn CloudTransport,
    header: &VaultHeader,
    staging_dir: &Path,
    config: &SyncConfig,
    progress: Option<&(dyn Fn(u32, u32, Option<&str>) + Send + Sync)>, // NEW
) -> Result<PushReport, SyncError>;
```
(Equivalent `progress` param added to `pull_vault` and a migration loop helper.)

> **CS-007** — Ceremony request types (already present in `src-tauri/src/auth/ceremonies/types.rs`):
- `CreateVaultRequest<'a>`: `tier, password_bytes, target_key_file_path, vault_db_path, argon2_params, chunk_size_bytes, epoch_buffer_enabled`.
- `ChangePasswordRequest<'a>`: `current_password_bytes, new_password_bytes, current_key_source, recovery_phrase, argon2_params, argon2_migration_intent, vault_db_path`.
- `RotateKeyFileRequest<'a>`: `password_bytes, current_key_source, target_new_key_file_path, recovery_phrase, argon2_params, argon2_migration_intent, vault_db_path`.
- `RecoverVaultRequest<'a>`: `password_bytes, key_source, vault_db_path`.

> **CS-008** — IPC response types used by Section 5 (`ProgressUpdate`, `SyncProgressUpdate`, `MigrationProgress`, `AuthResponse`, `SessionStatus`, `FileEntry`, `FileContent`, `SyncStatus`, `SyncResult`, `ShareResponse`, `ImportShareResponse`, `ContactEntry`, `ShareEntry`, `ReceivedShareEntry`, `DestinationSessionConfig`, `DestinationEntry`, `RemoteFileEntry`) are already defined in `src-tauri/src/ui/types.rs` (per design.md:594–831). No new response types are added.

---

### Step-by-step plan

**Step 1 — Extend `SessionManager` (C1, C6)**
- Path: `src-tauri/src/auth/session/manager.rs`.
- Add `with_manifest_key<F, R>` (CS-004) mirroring `with_sqlcipher_key` at manager.rs:456–468.
- Add `timer_deadline: Arc<RwLock<Option<tokio::time::Instant>>>` field; set in `restart_timer` to `Instant::now() + self.timeout`; clear in `cancel_timer`.
- Add `remaining_seconds(&self) -> Option<u64>` (CS-004): reads state+deadline; returns `Some(d - now)` when state is `Active` and deadline exists, else `None`.
- Add `vault_id: Arc<RwLock<Option<String>>>` field; `authenticate(..., vault_id: String)` — extend signature to carry vault_id; existing call sites: none external (ceremonies use `install_session`/`finalize_session_install`, which also gain a `vault_id` parameter). Update all callers in `auth::ceremonies::{create, change_password, rotate_key_file, recover_vault}`. Clear on `lock`.
- Add `active_vault_id(&self) -> Option<String>`.
- Update `.claude/rules/auth.md` per Section 8.

**Step 2 — Thread progress callback into storage (C4)**
- Paths: `src-tauri/src/storage/vault_ops/upload_file.rs`, `download_file.rs`, `src-tauri/src/storage/pipeline/encrypt_file.rs`, `decrypt_file.rs`, `src-tauri/src/storage/cloud/sync.rs`, plus any migration helper under `storage::cloud::migration` (create if missing).
- Extend signatures per CS-006. Pipeline invokes callback after each chunk write: `if let Some(cb) = progress { cb(bytes_processed, bytes_total); }`.
- Existing call sites in `auth::ceremonies::recover_vault` pass `None`.
- Storage **must not** import `tauri::ipc::Channel` — only the `dyn Fn` callback crosses the boundary.

**Step 3 — Refactor `AppState` and capture `AppHandle` (C2, C5, CS-003)**
- Path: `src-tauri/src/ui/state.rs`.
- Refactor `cloud_transport: Arc<dyn CloudTransport>` → `Arc<RwLock<Arc<dyn CloudTransport>>>`.
- Add `app_handle: OnceLock<tauri::AppHandle>` field.
- Add `active_vault_id: Arc<RwLock<Option<String>>>` field (used for `get_session_status`'s read path; source of truth remains `SessionManager::active_vault_id`).
- Add helper `AppState::swap_cloud_transport(&self, Arc<dyn CloudTransport>)` that takes the write lock and replaces.
- Add helper `AppState::reset_cloud_transport(&self)` that restores `NoOpCloudTransport`.

**Step 4 — Setup hook and device-event bridge (C2, CS-005)**
- Path: `src-tauri/src/lib.rs`.
- Replace:
  ```rust
  tauri::Builder::default()
      .plugin(tauri_plugin_shell::init())
      .plugin(tauri_plugin_dialog::init())
      .manage(crate::ui::AppState::construct_default())
      .invoke_handler(...)
      .run(...)
  ```
  with a `.setup(|app| { ... Ok(()) })` call placed *before* `.run()` that:
  1. Reads `app.state::<AppState>().inner()` (not `app.manage` — `AppState` is already managed).
  2. Stores `app.handle().clone()` into `app_handle.set(...)`.
  3. Spawns the device-event subscriber task using `tokio::spawn` (Tauri's async runtime): it calls `state.device_monitor.watch()` and awaits events from the returned `Stream`, emitting `{ kind, mountPath }` via `app_handle.emit("device-event", payload)`. The task lives for process lifetime; errors on `emit` are logged at `warn!` level and do not terminate the loop.
- Ensure `AppHandle` clone is not held across zeroization — it holds no key material.

**Step 5 — Vault-path resolver (C3)**
- Path: `src-tauri/src/ui/vault_paths.rs` (new file).
- Export `fn default_vault_root() -> PathBuf`, `fn vault_db_path(vault_id: &str) -> PathBuf`, `fn vault_header_path(vault_id: &str) -> PathBuf`, and `fn resolve_singleton_vault() -> Result<Option<(String, PathBuf, PathBuf)>, IpcError>` which returns `(vault_id, vault_db_path, vault_header_path)` or an error.
- Register module in `src-tauri/src/ui/mod.rs`.

**Step 6 — Common command wrapper (A9)**
- Path: `src-tauri/src/ui/commands_common.rs` (new file).
- Export:
  - `async fn with_session_refresh<F, Fut, T>(state: &AppState, f: F) -> Result<T, IpcError>` — calls `state.session_manager.reset_timer().await` before invoking `f()`; does **not** hold `begin_operation` (caller decides).
  - `async fn require_active_session(state: &AppState) -> Result<(), IpcError>` — `session_manager.state().await == Active` or returns `IpcError::VaultLocked("Vault is locked".into())`.
  - `async fn sanitise_password(pw: &mut String) -> Zeroizing<Vec<u8>>` — implements the canonical scrub pattern.

**Step 7 — Wire `auth` commands**
- Path: `src-tauri/src/ui/auth_commands.rs`.
- `authenticate`: Zeroize password, resolve singleton vault (Step 5), read vault header JSON, derive `KeySource` from `key_file_path` (or `None` for Tier 1), call `SessionManager::authenticate(...)` with `vault_id`, then read primary destination session from SQLCipher → build `RcloneTransport` → swap into `AppState.cloud_transport` (Step 3). Return `AuthResponse { vault_id, vault_name }`. All errors flow through `IpcError` From impls (CS-002).
- `create_vault`: Zeroize password, generate a new UUID v4 `vault_id`, construct `CreateVaultRequest` (CS-007), invoke `auth::ceremonies::create_vault(request, &session_manager, &*cloud_transport.read().await)`, then persist primary destination session via `storage::cloud::destination_session::insert_destination_session`, then swap cloud transport as above.
- `change_password` / `rotate_key_file`: `require_active_session`, Zeroize passwords, use current `vault_db_path` from `active_vault_id`, build `ChangePasswordRequest` / `RotateKeyFileRequest` (CS-007), call ceremony entry point.
- `delete_vault`: Require active session, match `confirmation` against `vault_name` from the header (reject with `IpcError::InvalidInput` on mismatch), call `storage::cloud::delete_vault_from_cloud` with sqlcipher_key + manifest_key via `SessionManager::with_sqlcipher_key` + `with_manifest_key`, then delete local vault_db_path + staging dir + rclone.conf; finally `session_manager.lock()`.
- `lock_session`: Already wired; no change.
- `get_session_status`: Fill `is_unlocked`, `vault_id` from `SessionManager::active_vault_id`, `timeout_seconds` from `SessionManager::remaining_seconds`.

**Step 8 — Wire `file` commands**
- Path: `src-tauri/src/ui/file_commands.rs`.
- Apply `reset_timer` + `require_active_session` + `begin_operation` on every handler.
- `list_directory`: Normalize vault_path, borrow `&SqlCipherMetadataStore` from `AppState.database`, call `MetadataStore::list_children(parent_id)` via path resolution (walk from root by segment), map each `Node` into `FileEntry` (CS-008).
- `upload_file`: Validate paths, resolve parent, generate UUID v4 `node_id`, build a closure that calls `Channel<ProgressUpdate>::send(ProgressUpdate { percent: ((bytes_processed * 100) / bytes_total.max(1)) as u8, bytes_processed, bytes_total, status: "uploading".into() })`, pass as `Some(&closure)` into `vault_ops::upload_file` (CS-006). Retrieve KEK via `with_key_encryption_key`.
- `download_file`: Validate `file_id` (UUID v4), closure sends `ProgressUpdate` with `status: "downloading"`, pass into `vault_ops::download_file`.
- `delete_file`: Validate `file_id`, call `vault_ops::delete_file`.
- `get_file_content`: Validate `file_id`, call `vault_ops::download_file` to a `tempfile::NamedTempFile`, check size vs 50 MiB cap, `infer::get_from_path` for MIME, base64-encode, return `FileContent { mime_type, data_base64, size_bytes }`. Temp file dropped synchronously before return.
- `list_remote`: `CloudTransport::list_blobs(&remote_prefix)`, join against `MetadataStore::list_pending_deletions` and chunk table to resolve filenames; orphans get `is_orphaned: true`, `file_name: None`, `vault_path: None`.

**Step 9 — Wire `sync` commands**
- Path: `src-tauri/src/ui/sync_commands.rs`.
- `sync_to_cloud`: `with_sqlcipher_key` + `with_manifest_key`, borrow transport, build `Channel<SyncProgressUpdate>` callback, invoke `push_vault` (CS-006). On success, update `sync_status.last_synced_at = Utc::now()`; return `SyncResult { files_uploaded, files_downloaded: 0, files_deleted, conflicts: vec![] }` from `PushReport`.
- `recover_from_cloud`: Parse `vault_header_path` file, resolve target DB path (Step 5), require active session (the user already authenticated), call `pull_vault` with progress callback, after success the manifest DB is usable.
- `get_sync_status`: Already wired; no change.
- `migrate_vault`: For each blob in source remote, `upload_blob` to new remote → `delete_blob` from old; emit `MigrationProgress { percent, blobs_transferred, blobs_total, current_phase }`. Finally update the primary destination session to reference the new remote.
- `sync_backup`: Enumerate destinations where `is_primary == false`; for each matching `destination_id` (or all if `None`), materialize that destination's `RcloneTransport` on-the-fly and call `push_vault` against it; aggregate `SyncResult`.

**Step 10 — Wire `destination` commands**
- Path: `src-tauri/src/ui/destination_commands.rs`.
- `add_destination`: Validate config, invoke probe (`RcloneTransport::list_blobs("")` — reject if `CloudTransportError`), call `storage::cloud::destination_session::insert_destination_session`, return `DestinationEntry`.
- `list_destinations`: Call `list_destination_sessions`, map to `DestinationEntry`.
- `delete_destination`: Call `delete_destination_session`; reject primary-with-backups attempts with `IpcError::InvalidInput("cannot delete primary while backups exist")`.

**Step 11 — Wire `sharing` commands (C7, C8)**
- Path: `src-tauri/src/ui/sharing_commands.rs`.
- Use the `with_active_kek_and_stores` helper (C7).
- `export_public_key`: Read own public key via `sharing::identity::export_public_key_bytes` (no KEK needed), write to `destination_path`. Validate destination not inside vault root.
- `add_contact` / `list_contacts`: Direct `SharingStore` calls via `&SqlCipherMetadataStore`.
- `share_file`: `sharing::create_share_package(file_id, recipient_pk, expires_at, cloud_endpoint, &store, &store, &kek)` → write bytes to a generated path under `dirs::document_dir().join("arx-runa").join("shares")`, return `ShareResponse { share_id, package_path }`.
- `import_share`: Read package bytes, call `sharing::import_share_package(bytes, &store, &store, &kek)`, return `ImportShareResponse`.
- `revoke_share`: Call `sharing::revoke_share(share_id, &store, &store, &kek, &*cloud_transport.read().await)`; map `SharingError::RevocationPartial` → `IpcError::CloudError` per `.claude/rules/sharing.md` `## Revocation contract`.
- `list_shares` / `list_received_shares`: Direct `SharingStore` list calls.

**Step 12 — Update `lib.rs` to use updated `AppState`**
- Path: `src-tauri/src/lib.rs`.
- No change to the `invoke_handler` command list.
- Add `.setup(|app| { ... })` hook (Step 4). `construct_default()` now returns the new `AppState` shape (CS-003).

**Step 13 — Tests**
- Per-command unit tests under `#[cfg(test)]` in each `*_commands.rs` file, driven by `MockCloudTransport`, `MockDeviceMonitor`, and in-memory `SqlCipherMetadataStore`. Cover: locked-session rejection, argument validation, progress callback invocation, error sanitisation, primary-destination-deletion guard, 50 MiB size gate.
- Integration test `src-tauri/tests/phase_6_5_end_to_end.rs` covering: create_vault → authenticate → upload_file (asserts progress events fire) → list_directory → download_file → lock_session → authenticate re-unlock flow against a temp `MockCloudTransport`.
- Extend `src-tauri/src/ui/security_audit.rs` with a test that asserts no `AppState` field name or command body references `master_key`, `session_keys`, or `rclone_config_blob` in a plaintext log emission (string-literal level).

**Step 14 — Manual verification (Validation Checkpoint)**
- Build: `cargo build --workspace`.
- Test: `cargo test --workspace --all-targets --all-features`.
- Lint: `cargo clippy --workspace -- -D warnings`.
- Frontend: `trunk build`.
- Runtime:
  - Authenticate with a real Tier 1 and Tier 2 vault against a local rclone remote (`rclone config` pre-seeded).
  - Create a test file, upload via `DropZone`, confirm `ProgressUpdate` events fire in devtools.
  - Download, compare SHA-256 with source.
  - Insert a USB key (Tier 2), observe `KeyFileIndicator` reacts to `device-event`.
  - `/lock`, then re-authenticate.
  - Run sync, verify no credential strings appear in `RUST_LOG=debug` output.

---

## 6. Review focus areas

**6a. Rust change surface** (anticipated):
- `src-tauri/src/ui/state.rs` — `AppState` refactor (CS-003).
- `src-tauri/src/ui/mod.rs` — register new `vault_paths`, `commands_common` modules.
- `src-tauri/src/ui/auth_commands.rs` — full wire-up of 7 commands.
- `src-tauri/src/ui/file_commands.rs` — full wire-up of 6 commands + progress callbacks.
- `src-tauri/src/ui/sync_commands.rs` — full wire-up of 5 commands + progress callbacks.
- `src-tauri/src/ui/destination_commands.rs` — full wire-up of 3 commands.
- `src-tauri/src/ui/sharing_commands.rs` — full wire-up of 8 commands + KEK helper.
- `src-tauri/src/ui/vault_paths.rs` — new module (Step 5).
- `src-tauri/src/ui/commands_common.rs` — new module (Step 6).
- `src-tauri/src/auth/session/manager.rs` — `with_manifest_key`, `remaining_seconds`, `active_vault_id`, `timer_deadline` field, `vault_id` field.
- `src-tauri/src/auth/session/keys.rs` — no change (pub(crate) sealed).
- `src-tauri/src/auth/ceremonies/{create.rs, change_password.rs, rotate_key_file.rs, recover_vault.rs}` — propagate vault_id into `install_session` / `finalize_session_install` / `swap_active_session`.
- `src-tauri/src/auth/session/manager.rs` — `install_session`, `finalize_session_install`, `swap_active_session`, `authenticate` signatures extended to accept `vault_id: String`.
- `src-tauri/src/storage/vault_ops/upload_file.rs`, `download_file.rs` — progress callback (CS-006).
- `src-tauri/src/storage/pipeline/{encrypt_file.rs, decrypt_file.rs}` — progress callback threading.
- `src-tauri/src/storage/cloud/sync.rs` — progress callback on `push_vault`, `pull_vault` (CS-006).
- `src-tauri/src/storage/cloud/migration.rs` — new module for `migrate_vault` helper (if one does not already exist).
- `src-tauri/src/lib.rs` — `.setup(...)` hook (Step 4).

**6b. Security-sensitive paths** (drift-check anchor — any touched file in `src-tauri/src/{crypto,auth,storage}/` not listed below triggers a Plan Deviation):
- `src-tauri/src/auth/session/manager.rs` — new accessors must uphold sealed-keys invariant; closures receive `&[u8; 32]` only, never own the buffer.
- `src-tauri/src/auth/session/keys.rs` — MUST NOT add new public accessors; existing `pub(crate)` boundary stays.
- `src-tauri/src/auth/ceremonies/{create.rs, change_password.rs, rotate_key_file.rs, recover_vault.rs}` — `master_key` must remain `Zeroizing<[u8;32]>` inside ceremony-local scope (`.claude/rules/auth.md` line 55).
- `src-tauri/src/storage/vault_ops/upload_file.rs`, `download_file.rs` — plaintext buffers remain `Zeroizing<Vec<u8>>`; progress callback must not receive or close over plaintext.
- `src-tauri/src/storage/pipeline/{encrypt_file.rs, decrypt_file.rs}` — `VerifiedBlob` ordering preserved (`.claude/rules/storage.md` `## Pipeline`).
- `src-tauri/src/storage/cloud/sync.rs` — manifest backup still goes through `MANIFEST_BACKUP_BLOB_NAME` singleton; snapshot_counter contract unchanged.
- `src-tauri/src/ui/auth_commands.rs` — password zeroization pattern (Phase 6.4) must remain intact on every handler.
- `src-tauri/src/ui/sharing_commands.rs` — `SharingError::AuthenticationFailed` → `IpcError::AuthenticationFailed` with fixed user-safe string (`.claude/rules/sharing.md` `## HPKE error hygiene`).
- `src-tauri/src/ui/file_commands.rs` — `get_file_content` temp file must be inside `tempfile::NamedTempFile` (auto-deleted on drop) and located under `staging/` not a user-writable temp dir; the decrypted buffer must be `Zeroizing<Vec<u8>>`.

**6c. Architecture risk areas**:
- `src-tauri/src/storage/vault_ops/` and `src-tauri/src/storage/pipeline/` — progress callbacks MUST NOT pull `tauri::` into storage; check `use` statements in these files after the change.
- `src-tauri/src/ui/state.rs` — `AppState` fields must stay `pub(crate)` — no `pub` exposure. `cloud_transport` `RwLock` write access must not be held across long operations (swap is O(1)).
- `src-tauri/src/sharing/` — must continue reading `vault_identity` only (`.claude/rules/auth.md` line 60); IPC helpers must not insert/update the identity row.
- `src-tauri/src/ui/` — no direct `dyn MetadataStore` re-export from `ui` to the frontend; commands return only the sanitised response types (CS-008).
- Module visibility: `vault_paths` and `commands_common` remain `pub(crate)` under `ui`.
- Dependency direction: `ui` → `auth`, `storage`, `sharing` is allowed; any new symbol added to `ui` must not be imported by `auth`/`storage`/`sharing`.

**6d. Testing requirements**:
- Every new `thiserror` variant (if any added) and every new `From<_>` mapping must be triggered by a unit test (`.claude/rules/rust.md` `## Testing`).
- Integration test covering the Validation Checkpoint flow end-to-end.
- `session_manager` tests extended: `with_manifest_key` returns `SessionNotActive` outside `Active`; `remaining_seconds()` decreases monotonically; `active_vault_id()` cleared on `lock`.
- `vault_ops::upload_file` progress callback test: given a 10-chunk file, callback fires ≥10 times with monotonically non-decreasing `bytes_processed` and final `bytes_processed == bytes_total`.
- Device-event bridge test: using `MockDeviceMonitor` pushing `Mounted{..}`, assert the payload emitted on `"device-event"` has `kind: "mounted"`, `mountPath` set, and field names are camelCase.
- Zero-Trace audit test extended: forbid `session_keys`, `master_key`, `rclone_config_blob`, `manifest_key`, `sqlcipher_key`, `key_encryption_key` in any `tracing::info!` / `tracing::warn!` / `println!` / `eprintln!` string literal within `src-tauri/src/ui/`.
- Sync commands: `MockCloudTransport` fault-injection — every sync error variant (`NetworkUnavailable`, `CloudProviderError`, `Other`) maps through `From<SyncError>` / `From<CloudTransportError>` to a single sanitised `IpcError::CloudError("…")` string, no internal paths in the message.
- Primary-destination-delete guard test with a seeded backup destination — must return `IpcError::InvalidInput`.
- 50 MiB gate: `get_file_content` returns `IpcError::InvalidInput` on a ≥50 MiB file without decrypting it end-to-end (short-circuit after reading manifest size).

Manual verification (Validation Checkpoint carried forward from sub-phase):
- Authenticate (Tier 1 and Tier 2); browse, upload, download; lock and re-authenticate; sync to a local Rclone destination; observe `KeyFileIndicator` auto-detect on USB insert.

---

## 7. Documentation impact

| File | Required this run? | Change |
|---|---|---|
| `.claude/rules/auth.md` — `## Session` | Required | Add `with_manifest_key`, `remaining_seconds`, `active_vault_id` accessors; add `vault_id` field on `SessionManager`. Extend `install_session` / `finalize_session_install` / `swap_active_session` / `authenticate` signature list to include `vault_id: String`. |
| `.claude/rules/tauri.md` — `## IPC / UI layer` | Required | Add rule: `AppState.cloud_transport` is `Arc<RwLock<Arc<dyn CloudTransport>>>`, swapped on authenticate/lock/delete. Add rule: `"device-event"` Tauri event carries `{ kind: "mounted" \| "unmounted", mountPath: String }`. |
| `.claude/rules/storage.md` — `## Pipeline` | Required | Add rule: `upload_file` / `download_file` accept `progress: Option<&(dyn Fn(u64, u64) + Send + Sync)>`; storage must not depend on `tauri::`. Mirror for `push_vault` / `pull_vault` (callback `Fn(u32, u32, Option<&str>)`). |
| `docs/architecture/designs/tauri-ipc-and-frontend/design.md` — `## Backend Integration` | Deferred | The design doc's Backend Integration section is illustrative; this plan doesn't change the contract. Rationale: only the illustrative section mentions `NoOpCloudTransport`; the normative Canonical Command Surface is unchanged. Tracked as a future doc-sync ticket. |
| `docs/architecture/designs/tauri-ipc-and-frontend/sub-phases/6.5-backend-command-wiring.md` | Deferred | No spec changes required — all gaps are implementation-level. Rationale: the sub-phase already lists all deferred items and the deliverable list remains accurate. |

---

## 8. Governance sync actions (pre-implementation)

| Action ID | Reason / linked concern | Target files | Required edit | Verification |
|---|---|---|---|---|
| G1 | C1, C6 — new `SessionManager` surface | `.claude/rules/auth.md` | In `## Session`, extend the accessor list: "`SessionManager::with_key_encryption_key`, `with_sqlcipher_key`, `with_manifest_key` invoke a closure with the active key bytes under the session read lock; no buffer escapes." Add: "`SessionManager::remaining_seconds()` returns `None` outside `Active`; `active_vault_id()` is cleared in `lock()` after zeroization. The `vault_id` is set on `authenticate`, `install_session`, `finalize_session_install`, and `swap_active_session`." | `grep with_manifest_key .claude/rules/auth.md` returns the new entry. |
| G2 | C2 — device-event contract | `.claude/rules/tauri.md` | In `## IPC / UI layer`, add: "The `"device-event"` Tauri event carries `{ kind: "mounted" \| "unmounted", mountPath: String }` with camelCase serde; emitted from the `DeviceMonitor::watch()` stream via the `Builder::setup()` subscriber task." | `grep device-event .claude/rules/tauri.md` returns the new rule. |
| G3 | C4 — storage progress callback | `.claude/rules/storage.md` | In `## Pipeline`, add: "`upload_file` / `download_file` accept `progress: Option<&(dyn Fn(u64, u64) + Send + Sync)>`. Pipeline invokes the callback once per chunk. Storage must never depend on `tauri::`. Same contract applies to `push_vault` / `pull_vault` with `Fn(u32, u32, Option<&str>)`." | `grep -n 'progress' .claude/rules/storage.md` shows the new rule. |
| G4 | C5 — cloud-transport swap | `.claude/rules/tauri.md` | In `## IPC / UI layer`, add: "`AppState.cloud_transport` is `Arc<RwLock<Arc<dyn CloudTransport>>>`. `NoOpCloudTransport` is the default; `RcloneTransport` is installed on authenticate/create_vault and reset on lock/delete. The write lock is held only during the swap." | `grep cloud_transport .claude/rules/tauri.md` returns the new rule. |
| G5 | G1–G4 | — | Run `/copilot-sync` after rule edits. | `/copilot-sync` completes without errors; `.github/copilot-instructions.md` reflects the new rules. |

If Section 8 actions fail verification, stop implementation and report.

---

## 9. Handoff Notes for Implementer

Working directory: `C:\Users\chris\source\repos\arx-runa` (Windows primary; plan must preserve Linux and macOS behavior per `CLAUDE.md` Platform compatibility). Order of operations: execute Section 8 governance actions first and run `/copilot-sync`; then Steps 1–6 (backend primitives) in order; Steps 7–11 may run in parallel per-domain but share the new `vault_paths` / `commands_common` modules; Step 12 and Step 13 follow; Step 14 validates end-to-end. Plan is self-contained — do not re-read the sub-phase. Traps: (a) storage modules must NOT import `tauri::ipc::Channel` (reverse-layer dependency); the `Channel` is wrapped into a `dyn Fn` closure at the IPC layer. (b) `AppState.cloud_transport` refactor to `Arc<RwLock<Arc<dyn CloudTransport>>>` touches every `*_commands.rs` call site that currently does `state.cloud_transport.clone()`. (c) `.setup()` hook must be in `lib.rs::run()` *before* `.run()`. (d) The single-vault resolver returns `IpcError::InvalidInput` if zero or >1 vaults are present; document this in the ceremony path so `create_vault` tests don't leave stale sub-directories. (e) `tempfile::NamedTempFile` in `get_file_content` must be created under `staging/` (not system temp) to ensure deletion on vault cleanup. (f) The Zero-Trace audit test uses string-literal inspection — avoid introducing `master_key` / `session_keys` / `rclone_config_blob` / `manifest_key` / `sqlcipher_key` / `key_encryption_key` anywhere in `src-tauri/src/ui/` log strings. (g) Preferred test command per project memory: `cargo test --workspace --all-targets --all-features`. (h) Security review is REQUIRED for this sub-phase (roadmap) — `src-tauri/src/{auth,storage,crypto}` are all touched; hand off to `security-reviewer` after implementation. (i) No frontend changes — Phase 6.3 already wired `KeyFileIndicator`, progress channel consumers, and all command invocations; this phase only fills in the Rust side.
