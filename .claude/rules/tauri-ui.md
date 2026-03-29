---
paths:
  - "src-tauri/src/ui/**"
---

# Tauri UI / IPC layer — scoped rules

These rules apply to all files under `src-tauri/src/ui/` (Tauri commands
and the frontend bridge).

## Error sanitisation (critical)
- Errors returned to the frontend MUST be sanitised before crossing the IPC
  boundary. Never expose:
  - Partial key material or key derivation parameters
  - Plaintext file paths or filenames
  - Memory addresses or pointer values
  - Internal stack trace fragments
- Return a generic error code + user-safe message. Log internal details
  server-side only. When in doubt, return "An error occurred" and log the
  details in RUST_LOG=debug output.

## Error handling pattern
- Tauri command functions use `anyhow::Result` — collect and convert errors
  from library modules via `?`
- Library modules (`crypto/`, `auth/`, `storage/`) use `thiserror` typed
  enums — never expose these types directly to the frontend
- Map internal errors to sanitised IPC responses before returning

## Data isolation
- The UI layer must only ever know about:
  - vault_id (opaque identifier, not a key)
  - Operation status messages
  - File metadata as displayed to the user (names, sizes — never key bytes)
- Never pass raw key material, master_key, or derived keys from Rust to
  the frontend JavaScript layer

## Input sanitisation
- Sanitise all user inputs received via Tauri commands before passing to
  library modules — validate types, lengths, and acceptable ranges

## Async
- All Tauri commands that perform I/O must be `async` — never call
  synchronous blocking code from a Tauri command handler
- Use `tokio::spawn` for background work that should not block the command
  response

## IPC surface
- Keep the IPC surface minimal — expose the smallest set of commands
  needed by the frontend
- Command names should be descriptive: `encrypt_and_upload_file`,
  not `enc_upload`
