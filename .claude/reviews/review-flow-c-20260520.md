---
flow: C
title: "IPC Boundary & Zero-Trace"
invariants: [6, 7, 15]
date: 2026-05-20
reviewer: claude-sonnet-4-6
status: complete
---

# Flow C Review — IPC Boundary & Zero-Trace

## Scope

Files reviewed:
- `src-tauri/src/ui/auth_commands.rs` — all 9 password-bearing IPC handlers
- `src-tauri/src/ui/commands_common.rs` — `sanitise_password`, `extract_kek`
- `src-tauri/src/ui/sync_commands.rs` — outline check (no password parameters)
- `src-tauri/src/ui/error.rs` — `IpcError` enum + all six `From` impls
- `src-tauri/src/ui/shell_commands.rs` — `validate_reveal_path`, `validate_url_scheme`, `validate_email_address`, `compose_email_with_attachment`, `reveal_in_explorer`, `open_url`
- `src-tauri/src/lib.rs` — IPC command registration surface
- `src-tauri/tauri.conf.json` — capabilities and CSP configuration

---

## Findings

### [FLOW-C-001] `format_cloud_error` catch-all passes raw `Display` of unrecognised `CloudTransportError` to the frontend
**Severity**: medium
**Invariant**: 7 (zero-trace — frontend-facing error strings contain no internal detail)
**Location**: `src-tauri/src/ui/auth_commands.rs:128`
**Observation**: `format_cloud_error` handles known `CloudTransportError` variants explicitly and formats sanitised messages. The catch-all arm is:
```rust
_ => format!(
    "Failed to validate cloud storage '{}': {}",
    config.label, err
)
```
`err` is formatted via its `Display` impl. For `CloudTransportError::Other(msg)`, `Display` includes the inner `msg` string verbatim. If `msg` carries rclone output (which can include OAuth tokens, API keys, or bucket credentials in error traces), those strings reach the Tauri frontend as part of `IpcError::InvalidInput`.

`format_cloud_error` is called only from `validate_storage_destination`, which returns `Err(IpcError::InvalidInput(format_cloud_error(e, config)))` — a value serialised over the IPC bridge to the UI.
**Violation**: Invariant 7 — frontend-facing error strings must not leak implementation detail. Raw rclone error output may contain cloud credentials.
**Recommendation**: Replace the catch-all with a hardcoded generic string: `_ => format!("Failed to validate cloud storage '{}'", config.label)`. Log `err` at `tracing::warn!` level for diagnostics, but do not forward it to the frontend.
**Test coverage**: none

---

### [FLOW-C-002] Full `Debug` representation of errors logged via `tracing::error!`
**Severity**: low
**Invariant**: 7 (zero-trace — no internal detail in logs)
**Location**: `src-tauri/src/ui/error.rs:79` (StorageError), `src-tauri/src/ui/error.rs:100` (SharingError), `src-tauri/src/ui/error.rs:140` (SyncError), `src-tauri/src/ui/error.rs:192` (CloudTransportError)
**Observation**: All four `From` impls open with a log call such as:
```rust
tracing::error!("storage error: {:?}", error);
```
The `{:?}` format uses the `Debug` derive, which recursively formats inner fields. `StorageError::Database(_)` and `StorageError::ConstraintViolation(_)` carry rusqlite error objects that may include SQL statement fragments and column/table names. `CloudTransportError::RcloneProcessFailed { stderr_sanitised, .. }` logs the sanitised stderr text at `tracing::error!` level regardless of the variant matched below.

These are backend logs, not IPC responses, so this does not violate the "no sensitive data in IPC response" rule. However, logging SQL text and rclone output at `error!` level creates forensic risk if log files are readable by other processes or included in crash reports.
**Violation**: Partial Invariant 7 concern — internal implementation detail (SQL fragments, rclone stderr) in persistent log output.
**Recommendation**: Log a sanitised `kind_name()` or equivalent at `error!` and reserve `{:?}` for `debug!`/`trace!` or a dedicated structured field. Pattern used correctly for `AuthenticationError` (`kind_name()`); apply the same pattern in the other four `From` impls.
**Test coverage**: none

---

### [FLOW-C-003] No Tauri v2 capabilities file exists
**Severity**: medium
**Invariant**: attack surface
**Location**: `src-tauri/tauri.conf.json` (no `capabilities` key); `src-tauri/capabilities/` directory absent
**Observation**: Tauri v2 uses a capability system (`src-tauri/capabilities/*.json`) to declare which frontend-initiated IPC calls are permitted and which plugin APIs the WebView may access. No capability files are present and `tauri.conf.json` has no `capabilities` section. In a Tauri v2 production build this means:
- Either all frontend IPC invocations are denied (if Tauri enforces deny-by-default), or
- Tauri is running with an undocumented blanket grant (possible in development configuration).

The three shell commands (`reveal_in_explorer`, `open_url`, `compose_email_with_attachment`) open the OS shell and are the highest-privilege IPC commands in the surface. There is no capability entry scoping or documenting their access.

`withGlobalTauri: true` is set, exposing the Tauri invoke bridge to the WebView — this is an additional reason capabilities should be explicit.
**Violation**: The plan checklist item "registered in tauri.conf.json capabilities with appropriately scoped permissions — not unguarded" cannot be confirmed. Whether the app builds and ships without capabilities is unverified from static analysis.
**Recommendation**: Create `src-tauri/capabilities/default.json` listing all IPC commands, with shell commands in a restricted capability that requires the application window's origin. Audit whether `tauri-plugin-opener`'s `allow-open-url` / `allow-reveal-item-in-dir` permissions are required and present.
**Test coverage**: none

---

### [FLOW-C-004] `%`-encoded sequences in `recipient_email` can inject mailto query parameters
**Severity**: low
**Invariant**: command injection (mailto parameter injection, not OS)
**Location**: `src-tauri/src/ui/shell_commands.rs:62` (`validate_email_address`)
**Observation**: `validate_email_address` permits `%` in the email address string (for percent-encoded characters such as `user%2Bname@example.com`). The email is embedded into the mailto URL as the recipient field:
```rust
let mailto = format!("mailto:{recipient_email}?subject={SUBJECT}&body={body}");
```
A crafted input such as `user@example.com%3Fbcc%3Devil@evil.com` decodes to `user@example.com?bcc=evil@evil.com` when the mail client processes the mailto URL, potentially injecting a BCC or additional header. This is not OS command injection — arguments to `xdg-email` are separate `arg()` entries and there is no shell interpolation. The risk is limited to manipulating the pre-filled mail compose window, which the user would see before sending.
**Violation**: Minor: `validate_email_address` does not explicitly prevent percent-encoded delimiter injection into the mailto URL.
**Recommendation**: Either (a) percent-encode the `recipient_email` before embedding in the mailto URL using a proper URL encoder, or (b) strip `%` from the allowlist and document that percent-encoded email addresses are not supported. Option (a) is preferred.
**Test coverage**: none (existing tests cover raw injection chars like `?`, `&`, `#` but not their percent-encoded equivalents)

---

### [FLOW-C-005] `validate_reveal_path` logs user-supplied (pre-canonical) path string
**Severity**: low
**Invariant**: 7 (zero-trace)
**Location**: `src-tauri/src/ui/shell_commands.rs:30`
**Observation**:
```rust
tracing::warn!(path = %path, "reveal path outside allowed set");
```
`path` here is the raw user-supplied string argument, not the canonicalized path. Since `canonicalize()` has already succeeded at this point (otherwise the function returned earlier), the raw string contains whatever the frontend sent — potentially a path chosen to cause log noise or obscure a traversal attempt in log triage.
**Violation**: Minor Invariant 7 — user-supplied strings in backend logs create noise and may include sensitive filesystem paths the user did not intend to expose.
**Recommendation**: Log `canonical.display()` instead of the raw `path` argument, or drop the log entirely (the `IpcError::InvalidInput` return is the meaningful signal).
**Test coverage**: none

---

### [FLOW-C-006] `SharingError::NotSupported(msg)` threads inner message directly to frontend
**Severity**: low
**Invariant**: 7 (zero-trace — frontend-facing error strings)
**Location**: `src-tauri/src/ui/error.rs:118`
**Observation**:
```rust
Sh::NotSupported(msg) => IpcError::SharingNotSupported(msg),
```
This is the only `From` impl arm that forwards an inner error string to the frontend without sanitisation. All other arms produce hardcoded literal strings. The `NotSupported` message originates in sharing/destination business logic (currently a static developer-written string), but if callers ever populate this with a runtime value (e.g., a destination name or cloud error fragment), it would reach the UI.
**Violation**: Low risk now, but a structural exception to the otherwise consistent policy of hardcoded IPC error strings. Represents a future regression risk.
**Recommendation**: Either (a) define a fixed set of `NotSupported` sub-reasons and map them to hardcoded strings, or (b) add a comment at the call sites of `SharingError::NotSupported` explicitly restricting its content to user-safe static strings, enforced by review.
**Test coverage**: none

---

## Confirmed Passing Checks

| Check | Invariant | Result |
|---|---|---|
| All 9 password-bearing handlers (`authenticate`, `create_vault`, `change_password`, `rotate_key_file`, `setup_recovery`, `recover_vault_with_phrase`, `recover_vault_from_cloud`, `recover_vault_from_cloud_with_phrase`, `retry_pending_vault_operation`) call `sanitise_password` as the first action on every `mut password: String` parameter | 6 | ✅ PASS |
| `change_password` sanitises both `current_password` and `new_password`; optional `recovery_phrase` sanitised via `recovery_phrase.as_mut().map(sanitise_password)` | 6 | ✅ PASS |
| `rotate_key_file` sanitises password and optional `recovery_phrase` via the same `Option::map` pattern | 6 | ✅ PASS |
| `retry_pending_vault_operation` accepts `password` at the IPC boundary and sanitises it (`let _password_bytes = sanitise_password(&mut password)`) even though the value is not consumed downstream; `_password_bytes` binding keeps `Zeroizing` alive to end of scope | 6 | ✅ PASS |
| `sanitise_password` implementation: zeroes the original `String` backing bytes via `write_bytes` before returning the `Zeroizing<Vec<u8>>` | 6 | ✅ PASS |
| No password bytes, key material, or raw key identifiers appear in any `tracing!` call across the reviewed handlers | 7 | ✅ PASS |
| `From<AuthenticationError>`: `InvalidCredentials`, `KeyFileNotFound`, and `KeySource(_)` all map to the same `IpcError::AuthenticationFailed("Invalid credentials")` — no oracle distinguishing wrong-password from wrong-key-file | 15 | ✅ PASS |
| `IpcError` serialises via serde tag+content with hardcoded literal strings in all `From` impls (excluding the `NotSupported` exception noted in FLOW-C-006) | 7 | ✅ PASS |
| `AuthResponse` carries only `vault_id` and `vault_name` — no session keys, wrapping keys, or SQLite row IDs | 7 | ✅ PASS |
| `validate_reveal_path` calls `canonicalize()` before `starts_with` comparison — symlink and `..` traversal is blocked | path disclosure | ✅ PASS |
| `validate_url_scheme` allows only `https://` and `http://127.0.0.1` (rclone OAuth callback) — `file:`, `javascript:`, `data:` and non-loopback `http:` are rejected | SSRF / scheme abuse | ✅ PASS |
| `compose_email_with_attachment` on Linux: `xdg-email` called via `std::process::Command::new(...).arg(...).arg(...).arg(...)` — no shell string interpolation; `package_path` and `mailto:` URL are separate argument-list entries | command injection | ✅ PASS |
| `validate_email_address` allowlists `a-z A-Z 0-9 . _ % + - @` — raw `?`, `&`, `#`, `;`, space, and shell metacharacters are rejected | command injection | ✅ PASS |
| CSP `script-src` uses `'wasm-unsafe-eval'`, not `'unsafe-eval'`; no `'unsafe-inline'` in `script-src`; `connect-src` limited to `ipc:` and `http://ipc.localhost` | design spec 6.4 | ✅ PASS |
| `withGlobalTauri: true` is present | attack surface | ✅ noted (see FLOW-C-003) |
| `sync_commands.rs` handlers take no password parameters — no Zeroizing conversion needed | 6 | ✅ PASS |

---

## Summary

**Findings by severity**

| Severity | Count |
|---|---|
| Critical | 0 |
| High | 0 |
| Medium | 2 (FLOW-C-001, FLOW-C-003) |
| Low | 4 (FLOW-C-002, FLOW-C-004, FLOW-C-005, FLOW-C-006) |

**Invariants fully confirmed with no findings**

- **Invariant 6** — Zeroizing conversion is centralised in `sanitise_password` and called immediately at the IPC boundary in every handler. No inline conversion found.
- **Invariant 15** — Auth error responses are non-oracular: wrong-password and wrong-key-file both return `AuthenticationFailed("Invalid credentials")`.
- **Zero-trace on IPC responses** — The `IpcError` `From` impls are consistently hardcoded, with the single structural exception documented in FLOW-C-006.

**Fix session recommended**: Yes — FLOW-C-001 (medium) and FLOW-C-003 (medium) should be addressed before the next release. FLOW-C-001 is a one-line fix; FLOW-C-003 requires creating a capabilities file. The four low findings can be batched.
