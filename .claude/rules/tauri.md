---
paths:
  - "src-tauri/src/ui/**"
  - "src-tauri/tauri.conf.json"
  - "src-tauri/capabilities/**"
  - "src-tauri/build.rs"
---

# Tauri

> Design: `docs/architecture/designs/tauri-ipc-and-frontend/design.md`

## IPC / UI (`src/ui/`)
- Sanitise before IPC: no key material, no paths, no stack traces; return generic message; log details server-side (`RUST_LOG=debug`)
- UI knows: `vault_id`, status, display metadata — never raw keys
- All I/O commands `async`; long-running ops use `tauri::ipc::Channel<T>` for streaming
- `get_file_content` non-streaming; reject files > 50 MiB with `InvalidInput`
- Validate vault-relative paths: allowlist + explicit traversal/absolute-path rejection (not denylist-only)
- Password-bearing IPC: immediately convert `String` to `Zeroizing<Vec<u8>>`, scrub String backing bytes, drop original
- `tauri::State<T>` for config — never for keys (mlocked memory)
- `From<StorageError>` maps real variants (`Database`, `NotFound`, `ChecksumMismatch`, `Io`, `WrongKey`, `ConstraintViolation`); design §Error Sanitisation is illustrative and out of date
- `storage::SyncError` and `storage::CloudTransportError` are the canonical cloud-error inputs for IPC sanitisation
- `AppState.database` is `Arc<RwLock<Option<SqlCipherMetadataStore>>>`; no separate `DatabaseConnection` type
- `AppState.cloud_transport` is `Arc<RwLock<Arc<dyn CloudTransport>>>`; `NoOpCloudTransport` default; `RcloneTransport` installed on `authenticate`/`create_vault`, reset on `lock`/`delete_vault`; write lock held only during swap — never across long ops
- `"device-event"` Tauri event: `{ kind: "mounted" | "unmounted", mountPath: String }` (camelCase serde); emitted from `DeviceMonitor::watch()` via `Builder::setup()` subscriber; `emit` errors logged at `warn!`, don't terminate loop
- Zero-Trace audit tests: `src-tauri/src/ui/security_audit.rs` `#[cfg(test)] mod security_audit`; `cargo test ui::security`; must NOT embed real secret data

## Config / Capabilities / Build / Plugins
- CSP: `default-src 'self'` with explicit local-only directives; `withGlobalTauri: true` for Leptos WASM IPC + strict CSP; never `dangerousRemoteDomainIpcAccess`; explicit `security.capabilities`; sign release builds
- Capabilities: deny-by-default; never `"windows": ["*"]` with sensitive permissions; no remote URLs; scope `$APPDATA`, `$DOCUMENT` only
- Whitelist via `AppManifest::commands()`:
  - Auth: `authenticate`, `create_vault`, `change_password`, `rotate_key_file`, `lock_session`, `get_session_status`, `delete_vault`
  - Files: `list_directory`, `upload_file`, `download_file`, `delete_file`, `get_file_content`, `list_remote`
  - Sync: `sync_to_cloud`, `recover_from_cloud`, `get_sync_status`, `migrate_vault`, `sync_backup`
  - Destinations: `add_destination`, `list_destinations`, `delete_destination`
  - Sharing: `export_public_key`, `add_contact`, `list_contacts`, `share_file`, `import_share`, `revoke_share`, `list_shares`, `list_received_shares`
- Never expose: key material, internal paths, debug commands
- Plugins: minimal surface; never `shell` (general), `http`, `clipboard`, unrestricted FS; `tauri-plugin-shell` allowed only with scoped `shell:allow-execute` for `rclone` sidecar (`{ "name": "rclone", "sidecar": true }`); OAuth via `tauri_plugin_opener`; `tauri-plugin-dialog` for `dialog:allow-open`; `dialog:allow-save` only where command-contracted destination path is required
