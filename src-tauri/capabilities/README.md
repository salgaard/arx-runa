# Capabilities

Arx Runa follows a strict deny-by-default capability model. Only the minimum
required permissions are declared; everything else is absent by design.

## What is not permitted

### Clipboard access

`tauri-plugin-clipboard-manager` is **not** declared in `Cargo.toml` and has
no capability entry in `default.json`. Clipboard write access is intentionally
withheld to prevent decrypted content from leaking into the system clipboard
where it could be read by other applications or persisted by clipboard managers.

### Outbound HTTP

`tauri-plugin-http` is **not** declared. All cloud storage operations are
proxied through the bundled `rclone` sidecar process (`tauri-plugin-shell`
with a `shell:allow-execute` permission scoped to `rclone` only). The
frontend never makes direct HTTP requests.

### Shell (general)

`tauri-plugin-shell` is permitted **only** with
`shell:allow-execute` targeting the bundled `rclone` sidecar. General shell
execution is not enabled.

## What is permitted

| Permission                | Scope                              | Reason                                  |
|---------------------------|------------------------------------|-----------------------------------------|
| `shell:allow-execute`     | `rclone` sidecar only              | Cloud storage transport                 |
| `dialog:allow-open`       | Native file picker (read)          | Vault selection, key-file selection     |
| `dialog:allow-save`       | Native file picker (write)         | Export destinations only                |
| `opener:allow-open-url`   | `https://` scheme only             | OAuth browser launch                    |
| `fs:allow-read-file`      | `$APPDATA`, `$DOCUMENT`            | Vault metadata and staged chunks        |
| `fs:allow-write-file`     | `$APPDATA`, `$DOCUMENT`            | Vault metadata and staged chunks        |

## Design reference

See `docs/architecture/designs/tauri-ipc-and-frontend/design.md` §Capability
Model and §Threat Model for the full rationale.
