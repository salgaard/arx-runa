# Tauri IPC Layer and Frontend

The IPC layer exposes backend functionality (auth, storage, sync, sharing) to the Leptos frontend through a minimal, auditable set of Tauri commands. Responses are sanitised — no key material, stack traces, or internal paths ever reach the frontend. The frontend is built with Leptos (Rust/WASM) and enforces Zero-Trace principles: no localStorage, no persistent state, UI cleared on lock.

---

## Goals

- Expose backend through Tauri commands with proper error sanitisation
- Minimal, auditable IPC surface enforced at build time via `build.rs` allowlist + capability files
- Zero-Trace: no localStorage, no persistent sensitive state, UI cleared on lock/timeout
- Stream large file transfer progress in real-time without blocking the UI

---

## Contract Surface

### Interface

Command surface: registered async Tauri commands across `auth_commands`, `file_commands`, `sync_commands`, `destination_commands`, `sharing_commands`.

Long-running commands stream progress via `tauri::ipc::Channel<T>`: `upload_file`, `download_file`, `sync_to_cloud`, `migrate_vault`, `sync_backup`.

Build-time exposure contract: `src-tauri/build.rs` `AppManifest::commands(...)` allowlist.

### Data

Error payload: `IpcError` with sanitised, user-safe messages only.

Response payload: `AuthResponse`, `SessionStatus`, `FileEntry`, `SyncResult`, `DestinationEntry`, `RemoteFileEntry`, and related progress types.

Shared runtime state: `AppState { database, cloud_transport, device_monitor, session_manager, sync_status }` — key material is excluded.

### Invariants

- IPC responses must never expose key material, passwords, stack traces, or unsanitised filesystem details.
- Zero-Trace: sensitive state is memory-only; frontend contexts are cleared on lock/timeout.
- All domain errors are mapped through explicit sanitisation boundaries (typed `From` impls).

---

## Canonical Command Surface

| Domain | Commands |
|--------|----------|
| Auth | `authenticate`, `create_vault`, `change_password`, `rotate_key_file`, `delete_vault`, `lock_session`, `get_session_status` |
| File | `list_directory`, `upload_file`, `download_file`, `delete_file`, `get_file_content`, `list_remote` |
| Sync | `sync_to_cloud`, `recover_from_cloud`, `get_sync_status`, `migrate_vault`, `sync_backup` |
| Destination | `add_destination`, `list_destinations`, `delete_destination` |
| Sharing | `export_public_key`, `add_contact`, `list_contacts`, `share_file`, `import_share`, `revoke_share`, `list_shares`, `list_received_shares` |

All 28 commands are registered in `lib.rs` and listed in the `build.rs` allowlist. Any command not in the allowlist fails at compile time.

---

## IpcError

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

Every domain error type has an explicit `From` impl into `IpcError`. Matching is exhaustive — a new domain error variant fails to compile until it gets an explicit sanitisation mapping. Internal details are logged server-side via `tracing`; the frontend receives only safe messages.

---

## AppState

```rust
pub struct AppState {
    pub database:        Mutex<Option<SqlCipherDatabase>>,
    pub cloud_transport: Mutex<Option<Box<dyn CloudTransport>>>,
    pub device_monitor:  Arc<dyn DeviceMonitor>,
    pub session_manager: Arc<SessionManager>,
    pub sync_status:     Arc<RwLock<SyncStatus>>,
}
```

Key material is not stored in `AppState`. Keys live exclusively in `SessionManager` under `SharedSession = Arc<RwLock<Option<SessionKeys>>>`. Commands that need key access borrow from `SessionManager` under a read lock for the minimum duration required.

---

## Zero-Trace Compliance

| Requirement | Implementation |
|-------------|---------------|
| No localStorage | All frontend state is Leptos reactive state in WASM memory only |
| No persistent sensitive state | No IndexedDB, sessionStorage, or cookies |
| Clear on lock | `SessionProvider` listens to `get_session_status`; on lock/timeout, clears all vault-related signal state |
| No URL-embedded vault data | Routing uses in-memory signals, not URL params with file IDs |
| Password zeroization | Passwords arrive as `String`; immediately converted to `Zeroizing<Vec<u8>>` and `String` bytes scrubbed before drop |

---

## Frontend Architecture

The frontend is a Leptos CSR app compiled to WASM by Trunk.

```
src/
├── main.rs              # Leptos mount point
├── app.rs               # Root component, router
├── auth.rs              # Login, create vault, recover
├── vault.rs             # Vault browser (directory listing)
├── settings.rs          # Vault settings, destinations, key rotation
├── shares.rs            # Share management, received shares
├── layout.rs            # Shell, nav, session status bar
└── state/
    ├── session_context.rs  # SessionProvider — tracks active session
    └── sync_context.rs     # SyncProvider — tracks sync state
```

### SessionProvider

`SessionProvider` wraps the app and polls `get_session_status` on an interval (default: every 30 seconds). On status change to `Locked` or `Expired`, it clears all vault-scoped reactive state and navigates to the login view. The 60-second timeout warning is surfaced through this same polling path.

### Progress streaming

Long-running commands use `tauri::ipc::Channel<T>`:

```rust
// Frontend creates a channel and passes it to the command:
let (tx, rx) = tauri::ipc::channel();
upload_file(source_path, vault_path, tx, state).await?;

// Backend sends updates:
progress.send(ProgressUpdate { bytes_transferred, total_bytes }).ok();
```

The Leptos frontend maps the channel receiver to a reactive signal displayed as a progress bar.

---

## Command Module Layout

```
src-tauri/src/ui/
├── mod.rs                  # Re-exports, invoke_handler registration
├── error.rs                # IpcError enum, From impls
├── auth_commands.rs        # authenticate, create_vault, change_password, ...
├── file_commands.rs        # list_directory, upload_file, download_file, ...
├── sync_commands.rs        # sync_to_cloud, recover_from_cloud, ...
├── destination_commands.rs # add_destination, list_destinations, ...
├── sharing_commands.rs     # export_public_key, share_file, import_share, ...
└── types/
    ├── mod.rs
    ├── session_status.rs
    ├── received_share_entry.rs
    └── sync_status.rs
```

---

## Security Configuration

### Content Security Policy

The Tauri window CSP is set in `tauri.conf.json`:

```json
{
  "security": {
    "csp": "default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; style-src 'self' 'unsafe-inline'; connect-src 'ipc:' asset: https://asset.localhost"
  }
}
```

`'wasm-unsafe-eval'` is required for WASM execution. No external origins are permitted. `connect-src` allows only Tauri IPC (`ipc:`) and local assets.

### Capability files

Each Tauri capability file grants the frontend access to specific commands for specific windows. The default capability (dev scaffold) grants broad access and must be replaced before release with per-window, per-command grants.

---

## Related Documents

- [Authentication and Session Management](design-authentication.md) — auth ceremonies, session lifecycle
- [Chunking and Manifest](design-chunking-and-manifest.md) — file upload/download pipeline
- [Cloud Synchronisation](design-cloud-synchronisation.md) — sync and destination commands
- [File Sharing](design-file-sharing.md) — sharing command surface
- [Project Scaffolding](design-project-scaffolding.md) — workspace layout, tech stack
