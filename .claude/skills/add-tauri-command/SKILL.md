---
name: add-tauri-command
description: Add a new Tauri IPC command to VoidGate. Use when exposing new backend functionality to the frontend via invoke().
---

Follow this procedure for every new Tauri command. The IPC boundary is a security boundary — treat all frontend inputs as untrusted and all errors as potentially leaky.

**Step 1: Define the command in `src-tauri/src/ui/`.** It must be `async`, return `Result<T, String>`, and delegate all logic to a private inner function:
```rust
#[tauri::command]
pub async fn encrypt_and_upload_file(
    file_path: String,
    vault_id: String,
    state: State<'_, AppState>,
) -> Result<String, String> {
    validate_inputs(&file_path, &vault_id)?;
    internal_encrypt_and_upload(file_path, vault_id, &state)
        .await
        .map_err(sanitise_error)
}
```

**Step 2: Write the inner function using `anyhow::Result`.** Do not include file paths or user-controlled values in `.context()` strings — they will appear in the error chain and may reach logs:
```rust
async fn internal_encrypt_and_upload(file_path: String, vault_id: String, state: &AppState) -> anyhow::Result<String> {
    let keys = state.session_keys.read().await.clone()
        .context("no active session")?;
    let blob_name = state.storage.encrypt_and_upload(&file_path, &vault_id, &keys)
        .await
        .context("storage layer: encrypt_and_upload failed")?; // no file_path here
    Ok(blob_name.to_string())
}
```

**Step 3: Sanitise errors with `{}` (top level only) — not `{:?}` (full chain):**
```rust
fn sanitise_error(error: anyhow::Error) -> String {
    tracing::debug!("IPC command failed: {}", error); // {} not {:?}
    "Operation failed. Please try again or contact support.".to_string()
}
```

**Step 4: Validate all inputs before they reach library modules:**
```rust
fn validate_inputs(file_path: &str, vault_id: &str) -> Result<(), String> {
    if file_path.is_empty() { return Err("file path must not be empty".to_string()); }
    if vault_id.parse::<uuid::Uuid>().is_err() { return Err("vault_id must be a valid UUID".to_string()); }
    Ok(())
}
```

**Step 5: Register in `src-tauri/src/main.rs`:**
```rust
.invoke_handler(tauri::generate_handler![
    ui::commands::existing_command,
    ui::commands::encrypt_and_upload_file,  // add here
])
```

**Security checklist before finishing:**
- [ ] Return value contains no key material, no raw bytes, no derived key values
- [ ] Return value contains no server-side file paths
- [ ] `sanitise_error` uses `{}` not `{:?}`
- [ ] No `.context()` string contains a user-supplied value or file path
- [ ] Command is `async`; all I/O goes through `tokio`
- [ ] Inputs validated before reaching `crypto/`, `auth/`, or `storage/`
- [ ] Command name is descriptive full words (`encrypt_and_upload_file`, not `enc_upload`)

**Testing:** Unit-test `validate_inputs` and the inner function directly. Do not test the `#[tauri::command]` wrapper — test the logic it delegates to.
