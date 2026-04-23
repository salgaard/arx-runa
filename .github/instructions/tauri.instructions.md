---
applyTo: "src-tauri/src/ui/**,src-tauri/tauri.conf.json,src-tauri/capabilities/**,src-tauri/build.rs"
---


# Tauri — rules

**Design specification**: `docs/architecture/designs/tauri-ipc-and-frontend/design.md` — last verified against design dated 2026-04-12

## MVP Scope Note

**Single-vault per device**: The Phase 1–6 MVP assumes exactly one active vault at a time. Multi-vault support with per-device session coordination is a Phase 7+ candidate. See [Deferred Items Inventory](../../docs/architecture/deferred-items-inventory.md) §Multi-Vault Support.

**Backend-implemented, UI-deferred commands**: `recover_from_cloud`, `migrate_vault`, `sync_backup` are fully wired but have no UI consumer in Phase 6.9. These are candidates for Phase 7 UI implementation.

## IPC / UI layer (`src/ui/`)
- Sanitise before IPC: no key material, no paths, no stack traces
- Return generic message; log details server-side (`RUST_LOG=debug`)
- UI knows: `vault_id`, status, display metadata — never raw keys
- All I/O commands `async`; long-running operations use `tauri::ipc::Channel<T>` for streaming
- `get_file_content` is non-streaming and must reject files above 50 MiB with `InvalidInput`
- Validate vault-relative paths with allowlist + explicit traversal/absolute-path rejection (not denylist-only checks)
- For password-bearing IPC payloads, immediately convert `String` to `Zeroizing<Vec<u8>>`, scrub String backing bytes, and drop the original
- `tauri::State<T>` for config — never for keys (those stay in mlocked memory)
- `From<StorageError>` maps the real variants (`Database`, `NotFound`, `ChecksumMismatch`, `Io`, `WrongKey`, `ConstraintViolation`); design §Error Sanitisation code block is illustrative and out of date.
- `storage::SyncError` (re-export of `storage::cloud::sync::SyncError`) and `storage::CloudTransportError` are the canonical cloud-error inputs for IPC sanitisation.
- `AppState.database` is `Arc<RwLock<Option<SqlCipherMetadataStore>>>`; there is no separate `DatabaseConnection` type.
- `AppState.cloud_transport` is `Arc<RwLock<Arc<dyn CloudTransport>>>`. `NoOpCloudTransport` is the default; `RcloneTransport` is installed on `authenticate`/`create_vault` and reset on `lock`/`delete_vault`. The write lock is held only during the swap — never across long operations.
- The `"device-event"` Tauri event carries `{ kind: "mounted" | "unmounted", mountPath: String }` with camelCase serde; emitted from the `DeviceMonitor::watch()` stream via the `Builder::setup()` subscriber task. The task lives for the process lifetime; `emit` errors are logged at `warn!` level and do not terminate the loop.
- Zero-Trace audit tests live in `src-tauri/src/ui/security_audit.rs`as `#[cfg(test)] mod security_audit`, reachable via the `cargo test ui::security` prefix filter. Audit tests MUST NOT embed real secret data; they only reference forbidden-identifier string literals.

## Config (`tauri.conf.json`)
- CSP required: `default-src 'self'` with explicit local-only directives (`connect-src`, `script-src`, `style-src`, `img-src`) per design
- `withGlobalTauri: true` is required for Leptos WASM IPC; compensate with strict CSP and no remote script sources
- Never enable `dangerousRemoteDomainIpcAccess`
- Explicit `security.capabilities` list; sign release builds

## Capabilities (`capabilities/`)
- Deny-by-default — add only what's strictly needed
- Never `"windows": ["*"]` with sensitive permissions
- No remote URLs — Arx Runa is local-only
- Scope: `$APPDATA`, `$DOCUMENT` — never absolute paths

## Build (`build.rs`)
- Whitelist commands via `AppManifest::commands()` — compile-time safety
- Keep allowlist aligned with the design command set:
  - Auth: `authenticate`, `create_vault`, `change_password`, `rotate_key_file`, `lock_session`, `get_session_status`, `delete_vault`
  - Files: `list_directory`, `upload_file`, `download_file`, `delete_file`, `get_file_content`, `list_remote`
  - Sync: `sync_to_cloud`, `recover_from_cloud`, `get_sync_status`, `migrate_vault`, `sync_backup`
  - Destinations: `add_destination`, `list_destinations`, `delete_destination`
  - Sharing: `export_public_key`, `add_contact`, `list_contacts`, `share_file`, `import_share`, `revoke_share`, `list_shares`, `list_received_shares`
- Never expose: key material, internal paths, debug commands

## Plugins
- Keep plugin surface minimal and tightly scoped
- Never: `shell` (general), `http`, `clipboard`, or unrestricted filesystem permissions. Exception: `tauri-plugin-shell` may be enabled with a scoped `shell:allow-execute` permission targeting the bundled `rclone` sidecar only (`{ "name": "rclone", "sidecar": true }`). OAuth browser launches use `tauri_plugin_opener`, never the shell plugin.
- `tauri-plugin-dialog` is allowed for native open/save file pickers scoped to the `dialog:allow-open` permission; `dialog:allow-save` is allowed only where a command-contracted destination path is required.

