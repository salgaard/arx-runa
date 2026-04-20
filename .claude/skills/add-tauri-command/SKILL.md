---
name: add-tauri-command
description: Add a new Tauri IPC command to Arx Runa. Use when exposing new backend functionality to the frontend via invoke().
---

## When to update this skill

Review this skill when the tauri-ipc-and-frontend design changes in these areas:
- **Canonical Command Surface** — a command is added, removed, or renamed
- **`IpcError` variants** — new error categories that need `From` impl guidance
- **`AppState` structure** — new fields that commands should or should not access
- **Security invariants** — new Zero-Trace requirements at the IPC boundary

---

Follow this procedure for every new Tauri command. The IPC boundary is a security boundary — treat all frontend inputs as untrusted and all errors as potentially leaky.

**Reference:** See `docs/architecture/designs/tauri-ipc-and-frontend/design.md` for the canonical command surface, error types, and response types.

## Step 1: Choose the correct domain submodule

Commands are organised by domain under `src-tauri/src/ui/`:

| Domain | File | Examples |
|--------|------|----------|
| Auth | `auth_commands.rs` | `authenticate`, `lock_session`, `create_vault` |
| File | `file_commands.rs` | `upload_file`, `download_file`, `list_directory` |
| Sync | `sync_commands.rs` | `sync_to_cloud`, `recover_from_cloud` |
| Destination | `destination_commands.rs` | `add_destination`, `list_destinations` |
| Sharing | `sharing_commands.rs` | `share_file`, `import_share` |

Place the new command in the appropriate domain file.

## Step 2: Define the command handler

It must be `async`, return `Result<T, IpcError>`, and use the `?` operator with `From` impls for error conversion.

For long-running operations that need progress streaming, accept a `tauri::ipc::Channel<T>`:

```rust
#[tauri::command]
pub async fn upload_file(
    source_path: PathBuf,
    vault_path: String,
    progress: tauri::ipc::Channel<ProgressUpdate>,
    state: tauri::State<'_, AppState>,
) -> Result<FileEntry, IpcError> {
    validate_path(&source_path)?;
    let session = state.session_manager.require_unlocked().await?;
    progress.send(ProgressUpdate { percent: 0, message: "Starting upload".into() })?;
    let entry = state.storage.encrypt_and_upload(&source_path, &vault_path, &session, &progress)
        .await?;  // From<StorageError> for IpcError handles conversion
    Ok(entry)
}
```

## Step 3: Validate all inputs before they reach library modules

```rust
fn validate_inputs(file_path: &str, vault_id: &str) -> Result<(), IpcError> {
    if file_path.is_empty() {
        return Err(IpcError::InvalidInput("file path must not be empty".into()));
    }
    if vault_id.parse::<uuid::Uuid>().is_err() {
        return Err(IpcError::InvalidInput("vault_id must be a valid UUID".into()));
    }
    Ok(())
}
```

For vault-relative paths, use an allowlist character policy plus explicit traversal/absolute-path rejection (do not rely on denylist-only checks).

## Step 4: Add `From` impls for any new domain error types

If the command introduces a new domain error type, add a `From` impl in `src-tauri/src/ui/error.rs`. Never expose internal details:

```rust
impl From<NewDomainError> for IpcError {
    fn from(err: NewDomainError) -> Self {
        tracing::error!("Domain error: {:?}", err);
        match err {
            NewDomainError::NotFound { .. } => IpcError::NotFound("Resource not found".into()),
            _ => IpcError::InternalError("An error occurred".into()),
        }
    }
}
```

## Step 5: Re-export and register

1. Re-export the command in `src-tauri/src/ui/mod.rs`
2. Register in `src-tauri/src/lib.rs` invoke handler:

```rust
.invoke_handler(tauri::generate_handler![
    ui::existing_command,
    ui::encrypt_and_upload_file,  // add here
])
```

3. Add to the `build.rs` allowlist (alphabetical order):

```rust
tauri_build::AppManifest::new()
    .commands(&[
        // ... existing commands ...
        "encrypt_and_upload_file",  // add here, alphabetical
        // ... existing commands ...
    ])
```

## Security checklist before finishing

- [ ] If command accepts `password: String` or similar sensitive string, convert to `Zeroizing<Vec<u8>>`, scrub the original `String` backing bytes, and drop it immediately (Zero-Trace invariant)
- [ ] Return value contains no key material, no raw bytes, no derived key values
- [ ] Return value contains no server-side file paths
- [ ] No `From` impl string contains a user-supplied value or file path
- [ ] `tracing::error!` in `From` impls uses `{:?}` for debugging, but the `IpcError` message is generic
- [ ] Command is `async`; all I/O goes through `tokio`
- [ ] Inputs validated before reaching `crypto/`, `auth/`, or `storage/`
- [ ] Command name is descriptive full words (`encrypt_and_upload_file`, not `enc_upload`)
- [ ] Command added to `build.rs` allowlist
- [ ] Command re-exported in `ui/mod.rs`

## Testing

Unit-test `validate_inputs` and the domain logic directly. Do not test the `#[tauri::command]` wrapper — test the logic it delegates to.
