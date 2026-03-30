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
- Register all commands in a single `invoke_handler` call — never scatter
  registration across multiple files

## Streaming large data
- Use `tauri::ipc::Channel<T>` for streaming large data to the frontend:
  - File encryption/decryption progress
  - Chunk upload/download status
  - Large file content transfers
- Never return entire file contents as a single JSON response — this blocks
  the IPC channel and consumes excessive memory
- Example pattern:
  ```rust
  #[tauri::command]
  async fn stream_file_progress(
      file_id: FileId,
      progress: tauri::ipc::Channel<ProgressUpdate>,
  ) -> Result<(), Error> {
      // Send progress updates via channel
      progress.send(&ProgressUpdate { percent: 50 })?;
      Ok(())
  }
  ```

## State management
- Use `tauri::State<T>` for shared application state (database connections,
  configuration, sync status)
- `tauri::State<T>` must NEVER contain:
  - Raw key material (`master_key`, `file_key`, derived keys)
  - Session secrets
  - Decrypted file contents
- Session keys live in a separate, mlocked memory region — access via
  controlled APIs, not via State

## Structured error responses
- Define error codes as an enum for frontend consumption:
  ```rust
  #[derive(serde::Serialize)]
  #[serde(tag = "kind", content = "message")]
  #[serde(rename_all = "camelCase")]
  enum IpcError {
      VaultLocked(String),
      FileNotFound(String),
      AuthenticationFailed(String),
      NetworkError(String),
      InternalError(String),
  }
  ```
- Frontend receives `{ kind: 'vaultLocked', message: '...' }` — enables
  typed error handling in TypeScript
- Never expose internal error details in the `message` field — use generic
  user-safe text

## Command registration
- All commands must be registered in `lib.rs` via a single `invoke_handler`:
  ```rust
  tauri::Builder::default()
      .invoke_handler(tauri::generate_handler![
          unlock_vault,
          lock_vault,
          list_directory,
          // ... all commands in one place
      ])
  ```
- This makes the IPC surface auditable — one file to review
