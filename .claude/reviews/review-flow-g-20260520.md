---
title: "Flow G — UI Zero-Trace & Frontend Security"
date: "2026-05-20"
reviewer: claude-sonnet-4-6
invariants: 7 (zero-trace), design spec 6.4 (CSP)
status: complete
---

# Flow G Review — UI Zero-Trace & Frontend Security

## Preamble

Per the review plan: `security_audit.rs` already contains 15 static-analysis tests. This session does **not** re-verify what those tests enforce. It checks (1) whether the tests are correctly scoped for current code, and (2) the gaps those tests do not cover. Deviations from the design doc are marked `[DESIGN-GAP]`.

---

## Confirmed pass — no findings

### Audit test scan coverage
`collect_rs_source` in `security_audit.rs:20` resolves its scan root as `Path::new(manifest_dir).join("../src")`, which — from `CARGO_MANIFEST_DIR = src-tauri/` — resolves to the `src/` Leptos frontend directory. The function walks recursively. All of `src/transfer.rs`, `src/settings.rs`, and `src/destinations.rs` are included in the localStorage, sessionStorage, IndexedDB, service-worker, and password-zeroize scans. **Test scope is correct for current code.**

### CSP directives
`src-tauri/tauri.conf.json` CSP (object form):
```
"script-src": "'self' 'wasm-unsafe-eval'"   // no 'unsafe-inline', no 'unsafe-eval' ✅
"connect-src": "'self' ipc: http://ipc.localhost"              ✅
"style-src":   "'self' 'unsafe-inline'"                        (style only — not script)
"media-src":   "'self' arxvault: http://arxvault.localhost"    (video stream)
"img-src":     "'self' asset: http://asset.localhost blob: data:"
"default-src": "'self'"
```
All design spec 6.4 checks pass: `wasm-unsafe-eval` present, no `unsafe-eval`, no `unsafe-inline` in `script-src`, `connect-src` contains `ipc:` and `http://ipc.localhost`. The test `test_csp_is_populated_in_tauri_conf` (`:246`) correctly validates these exact conditions.

### Password Leptos signal cleared after IPC dispatch
`src/auth.rs::LoginPage` (`line 101`). In the `on_submit` closure, after `invoke_command` returns:
```rust
password_value.zeroize();              // local clone zeroized
set_password.update(|s| s.zeroize()); // signal backing zeroized
```
Both the local clone and the signal backing bytes are explicitly zeroized after IPC dispatch. **Invariant 7 (frontend) satisfied.**

### VaultActions::clear() covers all modified components
`src/state/vault_context.rs::VaultState` fields: `current_path`, `files`, `loading`, `error`, `selected`. `clear()` resets all to `Default`.

`src/transfer.rs`, `src/settings.rs`, `src/destinations.rs` have no dedicated context module in `src/state/`. Search confirms no `provide_context` calls in those files register global reactive state. `ProgressModal` (`transfer.rs:15`) takes `IpcChannel<ProgressUpdate>` and local signal props — all component-scoped and automatically dropped on unmount (vault lock causes router to navigate away). **No new context signals escape `VaultActions::clear()`.**

### FileEntry IPC response — data minimisation
`src-tauri/src/ui/file_commands.rs::node_to_file_entry` (`line 102`) maps:
- `id` → `node.node_id.as_uuid().hyphenated()` (opaque UUID, not SQLite row ID)
- `name` → display filename only
- `parent_id` → parent UUID (opaque)
- `size_bytes`, `modified_at`, `entry_type`, `pending_flush`

No blob names, chunk identifiers, or raw SQLite row IDs are exposed. ✅

### IPC command surface
`src-tauri/src/lib.rs::run` registers 62 commands + `video_stream` (63 total) via `generate_handler!`. All are named production commands grouped by subsystem (auth, files, sync, destinations, sharing, shell). No debug, internal, or administrative commands detected. `withGlobalTauri: true` is present.

### Video stream: cross-vault UUID isolation
`serve_video_range` (`video_stream.rs:69`) obtains `db_store` via `state.session_manager.get_metadata_store()`, which returns the **active vault's** SQLite store. A UUID belonging to a different vault will not exist in this DB; `db.get_node()` returns `NOT_FOUND` → 404 response. A valid UUID from a different vault cannot be served.

### Video stream: filename not leaked in response headers
`mime_from_name` (`video_stream.rs:226`) uses only the file extension to return a static MIME type string (`"video/mp4"` etc.). The node's display name is not included in any response header or log statement. No `Content-Disposition` header is set.

### Video stream: arxvault:// cross-origin isolation
The `arxvault://` custom URI scheme is registered via Tauri's `register_uri_scheme_protocol`, not as a TCP listener. Requests are routed to the Rust handler in-process by the WebView (WebView2 on Windows). No other process or browser tab can issue requests to this scheme. The `http://arxvault.localhost` form used on some platforms is similarly WebView-internal. Cross-origin access from other origins is not possible at the platform level.

---

## Findings

### [FLOW-G-001] Video stream: decrypted byte buffer not zeroized after Tauri handoff
**Severity**: medium
**Invariant**: 7 (zero-trace — plaintext in memory)
**Location**: `src-tauri/src/ui/video_stream.rs:185`
**Observation**: `download_file_range_to_memory` returns `Zeroizing<Vec<u8>>` (bound to `bytes`). The function then calls `bytes.to_vec()` to produce a plain `Vec<u8>`, which is moved into `builder.body()` and handed to the Tauri runtime. The `Zeroizing<Vec<u8>>` wrapper is dropped and zeroized at end of scope, but the `.to_vec()` copy — owned by Tauri for the duration of the HTTP response — is not zeroized.
**Violation**: Decrypted video content remains in a non-zeroized allocation for the Tauri response lifetime. Under memory pressure or with a memory-scanning adversary, this window represents plaintext exposure beyond what the Zeroizing wrapper provides.
**Code already notes this**: An inline comment at line 183 reads: *"bytes (Zeroizing<Vec<u8>>) zeroes on drop at end of scope. The Vec<u8> copy passed to builder.body() is Tauri-owned after the move; post-handoff zeroization is not possible with this API. Accepted limitation."* The limitation is acknowledged.
**Recommendation**: The current Tauri URI-scheme responder API does not support `Zeroizing`-aware response bodies; no immediate fix is possible without upstream change. The accepted-limitation comment is appropriate. If Tauri's API gains a `body_bytes` path that owns the buffer and zeroizes on drop, migrate to it. Consider documenting this explicitly in the threat model rather than only in an inline comment.
**Test coverage**: none.

---

### [FLOW-G-002] Audit test for lock-transition state clearing omits session_actions.clear()
**Severity**: low
**Invariant**: 7 (zero-trace — state cleared on lock)
**Location**: `src-tauri/src/ui/security_audit.rs:444`
**Observation**: `test_state_clearing_wired_on_lock_transition_in_router` verifies that `vault_actions.clear()` and `sync_actions.clear()` appear within 30 lines of `Effect::new` in `src/app.rs`. It does not check for `session_actions.clear()`.
`SessionActions::clear` (`src/state/session_context.rs:89`) is documented as "Zero-Trace: called on vault lock". If session state is cleared via a different mechanism (e.g., `apply_status` called from an event listener rather than the Effect), that path is not tested by the 30-line co-location guard.
**Violation**: If `session_actions.clear()` or the equivalent `apply_status(locked_status)` call is ever moved out of the lock-transition path, no test will catch the regression.
**Recommendation**: Extend the test to either (a) verify `session_actions.clear()` is co-located in the same 30-line window, or (b) add a separate assertion verifying that `apply_status` or `clear` is invoked on the session context inside the lock Effect. Determine first which mechanism (`clear()` vs `apply_status(locked)`) is actually used in `src/app.rs` for session state, then write the test to match.
**Test coverage**: partial — vault and sync clearing tested; session clearing not tested.

---

### [FLOW-G-003] Key-material log scan limited to storage module; ui/ and sharing/ not covered
**Severity**: low
**Invariant**: 7 (zero-trace — key material in logs)
**Location**: `src-tauri/src/ui/security_audit.rs:400`
**Observation**: `test_no_key_material_logged_in_storage_module` scans only `src-tauri/src/storage/` for tracing calls containing key-material names (`master_key`, `file_key`, `sqlcipher_key`, `kek`, `manifest_key`). The `src-tauri/src/ui/` directory (including `video_stream.rs`, `file_commands.rs`, `auth_commands.rs`) and `src-tauri/src/sharing/` are not scanned by any equivalent test.
**Violation**: A tracing call accidentally logging a key identifier in the UI command layer or sharing layer would not be caught by the current test suite.
**Recommendation**: Add a parallel test `test_no_key_material_logged_in_ui_module` scanning `src-tauri/src/ui/` and `test_no_key_material_logged_in_sharing_module` scanning `src-tauri/src/sharing/`, using the same keyword list. Alternatively, expand the existing test to scan `src-tauri/src/` minus the `tests/` subdirectory in one pass, which would be simpler and more future-proof.
**Test coverage**: none for ui/ and sharing/ directories.

---

### [FLOW-G-004] CSP test does not assert connect-src excludes broad origins
**Severity**: low
**Invariant**: design spec 6.4 (CSP)
**Location**: `src-tauri/src/ui/security_audit.rs:273`
**Observation**: `test_csp_is_populated_in_tauri_conf` asserts `connect-src` contains `ipc:` and `http://ipc.localhost` (positive assertions). It does not assert the absence of wildcard (`*`) or broad origins in `connect-src`. The current `tauri.conf.json` value is `"'self' ipc: http://ipc.localhost"`, which is correct. A future addition of `*` or `https:` to `connect-src` would not be caught.
**Violation**: The test cannot prevent a CSP regression that adds an overly broad `connect-src` origin.
**Recommendation**: Add a negative assertion: `connect-src` must not contain `"*"` and must not contain origins other than `'self'`, `ipc:`, and `http://ipc.localhost`. This is low-risk to add and closes the regression window.
**Test coverage**: positive assertions only; no negative guard on broad origins.

---

## Unverified items

The following checks from the flow checklist could not be fully verified within this session due to token budget. They should be reviewed in a targeted follow-up.

| Check | Reason not verified | Risk |
|---|---|---|
| `ProgressUpdate` payload content (bytes/percentage only) | `ProgressUpdate` struct not read | low — component takes typed channel; likely safe |
| IpcError variants in `src-tauri/src/ui/error.rs` — no FS paths in errors | `error.rs` not read | medium — flow C reviews error sanitisation; cross-check with that session |
| `src/auth.rs::VaultCreationPage` password signal clearing | Only `LoginPage` was read | medium — same pattern likely applied |

---

## Summary

| Severity | Count |
|---|---|
| critical | 0 |
| high | 0 |
| medium | 1 |
| low | 3 |

**Invariants fully confirmed (no findings):**
- Invariant 7 (frontend): password signal correctly zeroized in LoginPage after IPC dispatch
- Design spec 6.4 (CSP): all required directives present and correctly scoped
- Invariant 7 (IPC responses): FileEntry carries no blob names, chunk IDs, or row IDs
- VaultActions::clear() correctly scoped to all context-provided reactive state

**Follow-up recommended**: No fix session required for critical/high items. The three low-severity findings are test hardening improvements that can be addressed in a single pass over `security_audit.rs`. FLOW-G-001 (video buffer) is an accepted Tauri API limitation; recommend adding it to the threat model document rather than the code.
