# Zero-Trace Automated Verification

## What was done

Added automated verification of the Zero-Trace principle across four layers, plus fixed a real violation discovered during the process.

---

## The violation that was found and fixed

`get_file_content` (IPC handler in `src-tauri/src/ui/file_commands.rs`) was writing decrypted plaintext to the OS temp directory (`%LOCALAPPDATA%\Temp`) via `tempfile::tempdir()` before reading it back and returning it over IPC as base64. The RAII guard deleted the file on drop, but between write and delete Windows Search or AV could read plaintext.

**Fix:** Added `decrypt_file_to_memory` / `decrypt_epoch_file_to_memory` in the pipeline and `download_file_to_memory` in vault_ops. `get_file_content` now decrypts entirely in RAM into a `Zeroizing<Vec<u8>>` — no temp file is created at any point.

---

## Four verification layers

### Layer 1 — Source-scanning tests
**File:** `src-tauri/src/ui/security_audit.rs`

Compile-time structural checks that run with `cargo test ui::security`. They scan source files for patterns that would violate Zero-Trace and fail the build if found:

- No `localStorage` / `sessionStorage` / `IdbDatabase` / `ServiceWorker` API calls in the frontend
- No `tempfile::` calls (excluding `use` import lines) in the decrypt pipeline or `file_commands.rs`
- No key material names (`kek`, `file_key`, `master_key`, etc.) passed to `tracing::` macros in `src/storage/`
- No password values logged in `src/auth/`
- CSP is set in `tauri.conf.json`
- The `clipboard` plugin is not in `Cargo.toml`
- `vault_actions.clear()` and `sync_actions.clear()` are both wired in the lock-transition `Effect::new` block in `src/app.rs`
- Password strings are wrapped with `Zeroizing::new()` before IPC dispatch

### Layer 2 — Runtime filesystem monitoring tests
**File:** `src-tauri/src/tests/scenarios_backup.rs`

Tokio async tests that run the real storage pipeline and assert no plaintext escapes to disk:

- `test_no_plaintext_file_in_staging_after_upload_and_download_to_memory` — staging dir contains only `.blob` files throughout upload + download
- `test_atomic_temp_file_not_left_on_decrypt_error` — tampered blob causes decrypt to fail; no `.tmp` files remain in the destination dir
- `test_download_to_memory_leaves_no_os_temp_files` — `std::env::temp_dir()` has no new `arx-runa` files after `download_file_to_memory`

The `FileSystemMonitor` helper diffs the directory against a baseline snapshot so pre-existing files (e.g. `arx-runa-oauth-*.conf`) don't cause false failures.

### Layer 3 — Memory zeroization test
**File:** `src-tauri/src/storage/pipeline/decrypt_file.rs`

`test_zeroizing_vec_zeroes_chunk_buffer_on_drop` — allocates a `Zeroizing<Vec<u8>>` with known bytes, drops it, reads the raw pointer, asserts the byte is zero. Confirms the `zeroize` crate is working as expected in this dependency chain.

A corresponding session-key zeroization test (`test_session_keys_buffers_are_zeroed_after_lock`) already existed in `src-tauri/src/auth/session/manager.rs`.

### Layer 4 — Frontend e2e tests (WebDriver)
**Directory:** `src-tauri/tests/e2e/`

WebdriverIO tests that drive the real built app via `tauri-driver` + `msedgedriver` and verify the WebView2 frontend layer:

- `localStorage is empty after lock`
- `sessionStorage is empty after lock`
- `file list is cleared after lock` — `[data-testid="file-list"]` must not exist after locking
- `no vault UUID in URL after lock`

#### data-testid attributes added to the Leptos frontend

| Attribute | File | Element |
|---|---|---|
| `data-testid="lock-button"` | `src/layout.rs` | Lock `<button>` in `SessionStatusBar` |
| `data-testid="file-list"` | `src/vault.rs` | File list `<div>` in `FileList` |
| `data-testid="password-input"` | `src/auth.rs` | Password `<Input>` in `LoginPage` |
| `data-testid="login-submit"` | `src/auth.rs` | Submit `<Button>` in `LoginPage` |
| `data-testid="vault-card"` | `src/vault_picker.rs` | Vault card `<button>` in `VaultPicker` |
| `data-testid="recovery-remind-later"` | `src/auth.rs` | "Remind Me Later" `<button>` in the recovery setup modal |

`Button` and `Input` components were extended with an optional `testid: Option<&'static str>` prop that renders as `data-testid` on the inner HTML element.

#### CSP requirements

Two Content Security Policy directives in `tauri.conf.json` are required for the WASM frontend to load correctly in the embedded binary:

- `script-src` must include `'unsafe-inline'` — Trunk's generated `dist/index.html` uses an inline `<script type="module">` bootstrapper to load the WASM module. Without it the WebView2 console shows a CSP block and the app renders a blank page.
- `connect-src` must include `'self'` — the bootstrapper calls `fetch()` to load the `.wasm` file from `http://tauri.localhost/`. Without it the fetch is blocked by CSP and WASM never initialises.

These are already set in `tauri.conf.json`; this note records why they are required.

#### Recovery modal gate

After Tier 1 vault creation, a "Set up recovery phrase?" modal fires before `session_actions.complete_success()` is called. The session does **not** reach the unlocked state until the user either sets up the recovery phrase or dismisses the modal with "Remind Me Later". The e2e helper clicks `[data-testid="recovery-remind-later"]` to complete the unlock flow; without this step the `lock-button` never appears.

#### Running the e2e tests

```powershell
# First run — builds the app automatically (cargo tauri build --debug --no-bundle)
cd src-tauri/tests/e2e && npm test

# Skip the build if you already ran `cargo tauri build --debug --no-bundle`:
$env:E2E_SKIP_BUILD=1; npm test
# WARNING: do NOT set E2E_SKIP_BUILD=1 after a plain `cargo build` (the VS Code debug
# workflow). That binary uses devUrl (http://localhost:1420) and shows a blank page when
# Trunk isn't serving. Only `cargo tauri build` embeds the frontend.
```

See `docs/guides/development.md` for the full one-time setup (tauri-driver, msedgedriver).

---

## Why plain `cargo build` is not enough for e2e

`cargo build --manifest-path src-tauri/Cargo.toml` builds the Rust backend only. The Leptos frontend (WASM) is not embedded, so the app crashes on startup with `cannot access imported statics on non-wasm targets` from `js-sys`. The e2e tests require `cargo tauri build [--debug] --no-bundle`, which runs Trunk first to compile and bundle the frontend before linking the Tauri binary. `npm test` handles this automatically via the `onPrepare` hook in `wdio.conf.js`.

---

## Running all non-e2e verification

```powershell
cargo test ui::security            # Layer 1: source-scanning
cargo test scenarios_backup        # Layer 2: runtime filesystem
cargo test decrypt_file            # Layer 3: zeroization
```
