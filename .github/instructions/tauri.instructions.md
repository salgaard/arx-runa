---
applyTo: "src-tauri/src/ui/**,src-tauri/tauri.conf.json,src-tauri/capabilities/**,src-tauri/build.rs"
---

# Tauri — rules

## IPC / UI layer (`src/ui/`)
- Sanitise before IPC: no key material, no paths, no stack traces
- Return generic message; log details server-side (`RUST_LOG=debug`)
- UI knows: `vault_id`, status, display metadata — never raw keys
- All I/O commands `async`; use `tauri::ipc::Channel<T>` for streaming
- `tauri::State<T>` for config — never for keys (those stay in mlocked memory)

## Config (`tauri.conf.json`)
- CSP required: `default-src 'self'` — no `'unsafe-inline'`, no `'unsafe-eval'`
- Never: `dangerousRemoteDomainIpcAccess`, `withGlobalTauri` in production
- Explicit `security.capabilities` list; sign release builds

## Capabilities (`capabilities/`)
- Deny-by-default — add only what's strictly needed
- Never `"windows": ["*"]` with sensitive permissions
- No remote URLs — VoidGate is local-only
- Scope: `$APPDATA`, `$DOCUMENT` — never absolute paths

## Build (`build.rs`)
- Whitelist commands via `AppManifest::commands()` — compile-time safety
- Required: `unlock_vault`, `lock_vault`, `get_vault_status`, `list_directory`, `encrypt_and_upload_file`, `download_and_decrypt_file`, `delete_file`
- Never expose: key material, internal paths, debug commands

## Plugins
- Allowed: `dialog`, `fs` (scoped), `process`
- Never: `shell`, `http`, `clipboard`, `fs:write-all`
