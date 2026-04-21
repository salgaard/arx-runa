---
title: "Phase 6.4 — Zero-Trace Compliance and Security Hardening"
created: "2026-04-21T13:08:16Z"
status: approved
roadmap-phase: 6
sub-phase: "6.4"
design-document: "docs/architecture/designs/tauri-ipc-and-frontend/design.md"
sub-phase-roadmap: "docs/architecture/designs/tauri-ipc-and-frontend/sub-phases/roadmap.md"
governance-sync-required: true
tags: [security, zero-trace, csp, backoff, tauri, leptos]
---

# Phase 6.4 — Zero-Trace Compliance and Security Hardening

## 1. Goal

Close the Phase 6 security boundary: populate the Tauri CSP, explicitly deny clipboard, enforce exponential backoff on failed authentication in `SessionManager`, and ship automated audits that verify Zero-Trace state clearing and password zeroization introduced in Phases 6.1–6.3.

---

## 2. Context

**Sub-phase:** 6.4 "Zero-Trace Compliance and Security Hardening" of the `tauri-ipc-and-frontend` design.

**Source spec:** `docs/architecture/designs/tauri-ipc-and-frontend/sub-phases/6.4-zero-trace-and-security-hardening.md` (9 deliverables, validation checkpoint `cargo test ui::security` + `trunk build`).

**Dependencies:** Phase 6.3 (frontend pages) — already implemented (`ce4271d feat(frontend): Phase 6.3 — Frontend Pages`). Prerequisite surface:
- `src/app.rs` Router Effect already clears `VaultActions` + `SyncActions` on unlock→lock transition.
- `src/auth.rs` `LoginPage` and `VaultCreationPage` already call `password_value.zeroize()` + `set_password.update(|s| s.zeroize())` on both success and failure branches after `invoke_command`.
- `Cargo.toml` frontend already depends on `zeroize = { version = "1", features = ["alloc"] }`.
- `src-tauri/tauri.conf.json` already has `"withGlobalTauri": true`, but `"csp": null` and no capability restrictions beyond default.
- `src-tauri/capabilities/default.json` contains: `core:default`, `opener:default`, `dialog:allow-open`, `shell:allow-execute` scoped to the rclone sidecar. No clipboard plugin is present in `src-tauri/Cargo.toml` (inherent denial).
- `SessionManager::authenticate` in `src-tauri/src/auth/session/manager.rs` carries the explicit marker `// TODO(phase-2.4/6.1): add per-vault exponential backoff on InvalidCredentials.` at line 218; the IPC path in `src-tauri/src/ui/auth_commands.rs` line 32 has a matching `// TODO(phase-6.4): backoff`.

**Design anchors consulted:** design.md §Zero-Trace Compliance, §CSP Configuration (lines 1582–1598), §State Clearing on Lock (lines 1602–1627), §Security Analysis (lines 1631–1640), §Threat Model Additions (lines 1644–1659, brute-force formula at 1654). Design invariants #5 (path validation), #6 (IPC sensitive input), #7 (Zero-Trace persistence) in `docs/architecture/design-invariants.md`.

**Pending Architectural Decisions affecting this phase:** none in `docs/roadmap.md` at the Phase 6.4 row.

---

## 3. Design Concerns / Open Questions

| Field | Content |
|---|---|
| **Concern** | "Per-vault" backoff language vs. single-vault `SessionManager` instance |
| **Source** | `design.md` §Threat Model Additions line 1654 ("per-vault in-memory exponential backoff"); `SessionManager` is a single instance stored in `AppState` |
| **Impact** | Implementer could over-engineer a `HashMap<VaultId, u32>` where one `AtomicU32` suffices; conversely, forgetting to reset across vault switches would over-throttle legitimate use |
| **Classification** | Non-blocking |
| **Resolution** | Assume single-process backoff state held in `SessionManager` itself (one `AtomicU32` attempt counter + `AtomicU64` next-allowed-instant millis). Reset on any successful `authenticate()`. This matches the "resets on process restart" guarantee in the design verbatim (line 1654). See Assumption A1. |
| **Documentation updates** | None — design language already compatible with single-counter implementation. |

| Field | Content |
|---|---|
| **Concern** | `cargo test ui::security` filter requires a test path literal containing the substring `ui::security` |
| **Source** | Deliverable 8 validation checkpoint (`cargo test ui::security`) |
| **Impact** | If tests are placed under `src-tauri/tests/` (integration) instead of an inline module, the filter will not match because integration tests are scoped as `<crate>::<file>::<test>`, not `ui::security::<test>`. Implementer could silently leave the filter failing. |
| **Classification** | Non-blocking |
| **Resolution** | Place the security audit tests inline as `src-tauri/src/ui/security_audit.rs` gated `#[cfg(test)]`, exposed as `mod security_audit;` in `src-tauri/src/ui/mod.rs` so the test path is `ui::security_audit::<test>`. The filter `ui::security` matches `ui::security_audit::*` by prefix. Document in tests/README that the filter is a prefix match. |
| **Documentation updates** | Extend `.claude/rules/tauri.md` with the `ui::security_audit` test-location rule (Section 8, Governance action G-2). |

| Field | Content |
|---|---|
| **Concern** | Browser-level assertions (`localStorage.length == 0`) require `wasm-bindgen-test` harness; the frontend has no such harness today |
| **Source** | Deliverable 8 bullet "After lock: `localStorage` and `sessionStorage` are empty (inspected via `web_sys::window().local_storage()`)" |
| **Impact** | Implementer could attempt to add a wasm browser test runner (new toolchain, headless browser CI, ~30–60 min build) out of scope for a hardening phase, or could silently drop the assertion |
| **Classification** | Non-blocking |
| **Resolution** | Replace the runtime-browser assertion with a **static/source audit** integration test that greps the compiled Leptos source tree for any use of `local_storage`, `session_storage`, `document.cookie`, `indexed_db`, or `service_worker` from the `web_sys` / `js-sys` surface. See Assumption A2. The design calls Zero-Trace an "application-level guarantee" (sub-phase line 72) — source audit is the mandated verification form. |
| **Documentation updates** | None — already covered by sub-phase implementation-notes line 72. |

| Field | Content |
|---|---|
| **Concern** | Backoff triggers only on `AuthenticationError::InvalidCredentials`; `KeyFileNotFound`, `MemoryLockFailed`, `SessionAlreadyActive` are not throttled |
| **Source** | `design.md` line 1654 "Delay is applied before returning `InvalidCredentials`." |
| **Impact** | If implementer reads the sub-phase without the design line, they may gate backoff on every error variant, which would cause legitimate `KeyFileNotFound` diagnostic loops (user inserting wrong USB stick) to throttle — poor UX and inconsistent with `.claude/rules/auth.md` "Never log key file contents" threat model (failure to read a key file is not adversarial evidence). |
| **Classification** | Non-blocking |
| **Resolution** | Backoff counter increments only when `Argon2id → SessionKeys` derivation completes and fails credential verification (the existing all-zero-key check path). `KeyFileNotFound`, `MemoryLockFailed`, and `SessionAlreadyActive` bypass the counter. See Assumption A3. |
| **Documentation updates** | Extend `.claude/rules/auth.md` with the "InvalidCredentials-only backoff" rule (Section 8, Governance action G-1). |

| Field | Content |
|---|---|
| **Concern** | `withGlobalTauri: true` + `script-src 'self' 'wasm-unsafe-eval'` CSP omits `'unsafe-eval'` — confirm this is sufficient for Leptos 0.8 CSR and wasm-bindgen glue |
| **Source** | `design.md` CSP block line 1591 vs. observed wasm-bindgen behavior |
| **Impact** | If the glue JS triggers CSP violations at runtime, the `trunk build` step passes but the app is broken at load time, undetected by `cargo test` |
| **Classification** | Non-blocking |
| **Resolution** | Implementer must run `trunk serve` once (manual verification step in sub-phase) and watch the dev-tools console for CSP violations before shipping. If violations occur and are intrinsic to wasm-bindgen, document as an Open Decision in `design.md` §Open Decisions. No allowlist change without explicit user approval. |
| **Documentation updates** | Conditional: only if violations are observed; out-of-band update to `design.md` §Open Decisions and `.claude/rules/tauri.md`. |

---

## 4. Assumptions

- **A1** — Backoff state is a pair of atomics on `SessionManager` (`failed_attempts: AtomicU32`, `next_allowed_unix_millis: AtomicU64`), reset atomically on any successful `authenticate()`. Not persisted. Not per-vault (single-vault-per-process matches the current `AppState` shape).
- **A2** — "Zero-Trace browser-state empty after lock" is verified by a compile-time source-grep test over `src/**/*.rs` asserting no call sites to `web_sys::Window::local_storage`, `::session_storage`, `js_sys::Reflect::get` patterns that reach `localStorage`, or `web_sys::HtmlDocument::cookie`. The test fails if any such call site is introduced later.
- **A3** — Backoff increments only on the `InvalidCredentials` terminal branch of `authenticate()` that follows successful KDF derivation, not on `KeyFileNotFound`, `MemoryLockFailed`, `SessionAlreadyActive`, or `authenticate_gate` acquisition failure.
- **A4** — `tokio::time::pause()` + `tokio::time::advance()` is the test mechanism for asserting backoff delays 1s, 2s, 4s, 8s, 16s, 30s without wall-clock sleeps. This requires `tokio::time::sleep` inside `authenticate()` (not `std::thread::sleep` or a loop on `Instant::now()`).
- **A5** — Tests are named `test_authenticate_backoff_*` and live in `src-tauri/src/auth/session/manager.rs #[cfg(test)] mod` (backoff unit tests) + `src-tauri/src/ui/security_audit.rs` (audit tests). The `cargo test ui::security` invocation prefix-matches `ui::security_audit`.
- **A6** — Clipboard denial is expressed by (a) the absence of `tauri-plugin-clipboard-manager` in `src-tauri/Cargo.toml` (already true) and (b) a comment block in `src-tauri/capabilities/default.json` documenting the deliberate omission. No positive permission `clipboard-manager:deny-*` is available because the plugin is not installed; the absence is the denial.
- **A7** — The "verified password zeroize" deliverable (6.4 item 5) is discharged by a source-audit test asserting the token sequence `password.zeroize()` appears after every `invoke_command("authenticate", ...)` call site in `src/**/*.rs`. No new zeroize logic is added if the Phase 6.3 implementation already satisfies this.
- **A8** — The "no console logging of sensitive data" audit (item 7) is discharged by a grep-based test over `src-tauri/src/ui/**/*.rs` and `src/**/*.rs` asserting no `tracing::info!`, `tracing::debug!`, `console::log_*` call site interpolates password, key, or decrypted-content variables (heuristic: forbidden identifier list `password, passphrase, master_key, key_file, decrypted, plaintext, mnemonic, recovery_phrase` used as format arguments).

---

## 5. Approach

### CONTRACT_SNIPPETS

**CS-001** — CSP JSON block for `src-tauri/tauri.conf.json > app.security.csp` (verbatim from `design.md` §CSP Configuration lines 1588–1595; keys are CSP directive names, values are space-separated source lists):

```json
{
  "default-src": "'self'",
  "connect-src": "ipc: http://ipc.localhost",
  "script-src": "'self' 'wasm-unsafe-eval'",
  "style-src": "'self' 'unsafe-inline'",
  "img-src": "'self' asset: http://asset.localhost blob: data:"
}
```

**CS-002** — Backoff formula (verbatim from `design.md` line 1654):

```
delay = min(30 seconds, 2^(attempt-1) seconds)
```

Canonical delays for attempts 1..=6: `1s, 2s, 4s, 8s, 16s, 30s (capped)`. Attempt 7+ stays at `30s`. Counter resets on successful `authenticate()`.

**CS-003** — `SessionManager` backoff-state field additions (new fields in `src-tauri/src/auth/session/manager.rs` struct `SessionManager`):

```rust
/// Count of consecutive failed authenticate() calls returning InvalidCredentials
/// after successful KDF derivation. Reset to 0 on any successful authenticate().
/// Not persisted; resets to 0 on process restart.
failed_attempts: Arc<AtomicU32>,

/// Unix-millis timestamp after which the next authenticate() attempt is allowed
/// to begin KDF work. Stored as atomic for lock-free read from concurrent
/// authenticate() callers. 0 means "no delay required".
next_allowed_unix_millis: Arc<AtomicU64>,
```

**CS-004** — Authenticate-path pseudocode (applied inside `SessionManager::authenticate`, after the `authenticate_gate` acquisition at manager.rs:156 and before the `state().await == Active` check at 166):

```rust
// 1. Compute delay-until from next_allowed_unix_millis.
// 2. If now < next_allowed, sleep via tokio::time::sleep for the remainder.
// 3. Proceed with existing KDF + key_file read + derivation logic unchanged.
// 4. On the terminal InvalidCredentials branch following successful derivation:
//    - attempts = failed_attempts.fetch_add(1, Ordering::SeqCst) + 1
//    - delay_seconds = min(30, 1u64 << (attempts.saturating_sub(1)).min(5))
//    - next_allowed_unix_millis.store(now_millis + delay_seconds * 1000, SeqCst)
// 5. On successful return (Ok(())): failed_attempts.store(0, SeqCst);
//                                   next_allowed_unix_millis.store(0, SeqCst).
```

**CS-005** — Security-audit module declaration (new file `src-tauri/src/ui/security_audit.rs`, referenced from `src-tauri/src/ui/mod.rs` with `#[cfg(test)] mod security_audit;`):

```rust
#[cfg(test)]
mod security_audit {
    // test_no_localstorage_usage_in_frontend
    // test_no_sessionstorage_usage_in_frontend
    // test_no_cookie_access_in_frontend
    // test_no_indexeddb_usage_in_frontend
    // test_no_service_worker_registration
    // test_password_zeroize_after_every_authenticate_invoke
    // test_no_sensitive_identifiers_in_tracing_macros
    // test_csp_block_present_in_tauri_conf
    // test_clipboard_plugin_absent_from_cargo_manifest
}
```

**CS-006** — Capability file comment block (to be inserted as a leading JSON-comment-alternative — either JSON-with-comments if Tauri permits, or a sibling `capabilities/README.md` because strict JSON forbids comments). Since `src-tauri/capabilities/default.json` is strict JSON per Tauri schema, the rationale lives in `src-tauri/capabilities/README.md`:

```
# Denied capabilities (Phase 6.4)
- Clipboard: `tauri-plugin-clipboard-manager` is not declared in Cargo.toml; no
  clipboard permission is present in default.json. This is the Zero-Trace
  clipboard-exfiltration mitigation documented in
  docs/architecture/designs/tauri-ipc-and-frontend/design.md §Threat Model
  Additions "Clipboard attack".
- HTTP: `tauri-plugin-http` is not declared. All network I/O goes through the
  rclone sidecar (shell:allow-execute scoped).
```

---

### Step-by-step implementation

**Step 1 — Populate the CSP in `src-tauri/tauri.conf.json`.**
- Replace `"csp": null` in `app.security` with the JSON object from `CS-001`.
- Verify `"withGlobalTauri": true` is still present at `app.withGlobalTauri` (precondition).
- Do not touch `capabilities` list.
- Governance anchor: `.claude/rules/tauri.md` lines 27–30 (CSP required) — rule already present, no sync needed.

**Step 2 — Document clipboard denial.**
- Verify `tauri-plugin-clipboard-manager` is absent from `src-tauri/Cargo.toml` `[dependencies]` (grep; abort if present).
- Create `src-tauri/capabilities/README.md` with the body of `CS-006`.
- Do not add any positive permission.

**Step 3 — Add backoff state to `SessionManager` (`src-tauri/src/auth/session/manager.rs`).**
- Add imports `std::sync::atomic::{AtomicU32, AtomicU64}` and `std::time::SystemTime` (or use `tokio::time::Instant` if the rest of the module already does — verify first).
- Add the two fields from `CS-003` to the struct near `authenticate_gate` (lines 61 area). Use `Arc<AtomicU32>` / `Arc<AtomicU64>` to preserve cheap cloning if the manager ever needs to surface state.
- Initialize to 0 in all `SessionManager::new*` constructors (locate via grep `impl SessionManager` → `fn new`).
- Do not remove the `// TODO(phase-2.4/6.1):` comment at line 218 yet — leave it as documentation of the historical marker until Step 4 supersedes it.

**Step 4 — Insert backoff logic in `authenticate()` per `CS-004`.**
- The backoff sleep belongs **after** `authenticate_gate` is acquired and **before** the `state().await == Active` check — so holders of the gate still serialize, but an already-locked session can short-circuit without waiting. (Rationale: `SessionAlreadyActive` is not a credential failure.)
- Use `tokio::time::sleep(Duration::from_millis(remainder))`, not `std::thread::sleep`, so that `tokio::time::pause()` in tests can advance virtual time.
- On the terminal `InvalidCredentials` branch that follows successful KDF derivation (locate the existing all-zero-key check; the sub-phase calls this out explicitly), update `failed_attempts` and `next_allowed_unix_millis` atomically.
- On successful return, reset both atomics to 0.
- Do **not** apply backoff on `KeyFileNotFound`, `MemoryLockFailed`, `SessionAlreadyActive`, or `authenticate_gate` closure paths (Assumption A3).
- Delete the `// TODO(phase-2.4/6.1):` marker at line 218.
- Delete the `// TODO(phase-6.4): backoff` marker at `src-tauri/src/ui/auth_commands.rs` line 32 (no code change needed there — backoff is owned by `SessionManager`).

**Step 5 — Unit tests for backoff in `src-tauri/src/auth/session/manager.rs #[cfg(test)] mod`.**
- `test_authenticate_backoff_first_failure_sets_one_second_delay` — `tokio::time::pause()`, call `authenticate` with wrong password, assert next call at `+500ms` still returns before starting KDF by mocking `KeySource` call counter (or instrument via a test-only hook that captures the sleep duration).
- `test_authenticate_backoff_doubling_sequence` — iterate 6 failures, assert delays `1,2,4,8,16,30` seconds.
- `test_authenticate_backoff_cap_at_thirty_seconds` — 7th, 8th, 9th attempts all clamped at `30s`.
- `test_authenticate_backoff_resets_on_success` — after 3 failures, one success, verify `failed_attempts == 0` and the next authenticate begins immediately.
- `test_authenticate_backoff_skipped_on_key_file_not_found` — wrong key file (`MockKeySource` returning `NotFound`), verify counter unchanged.
- `test_authenticate_backoff_skipped_on_session_already_active` — transition to `Active`, second `authenticate` returns `SessionAlreadyActive` immediately, counter unchanged.
- All tests use `tokio::time::pause()` and `tokio::time::advance()`; none may call `std::thread::sleep` or `tokio::time::sleep` without pause (would make CI flaky).
- Every `thiserror` variant already has coverage (rust.md testing rule); new variants are none, so no new variant-trigger tests needed.

**Step 6 — Security-audit tests in `src-tauri/src/ui/security_audit.rs`.**
- Create the file with module body per `CS-005`.
- Declare in `src-tauri/src/ui/mod.rs` with `#[cfg(test)] mod security_audit;` (placement: alphabetically after existing modules).
- Each test uses `std::fs::read_to_string` + `include_dir!` (or `std::process::Command` invoking `rg` — prefer `std::fs` walks to avoid external tooling dependency).
- `test_no_localstorage_usage_in_frontend` — walk `src/` (frontend), assert no `local_storage` / `localStorage` textual occurrence in `.rs` files.
- `test_no_sessionstorage_usage_in_frontend` — same for `session_storage` / `sessionStorage`.
- `test_no_cookie_access_in_frontend` — same for `document.cookie` and `web_sys::HtmlDocument::cookie` / `.cookie()`.
- `test_no_indexeddb_usage_in_frontend` — same for `indexed_db` / `IdbDatabase`.
- `test_no_service_worker_registration` — same for `service_worker`, `ServiceWorker`.
- `test_password_zeroize_after_every_authenticate_invoke` — find every `invoke_command("authenticate"` call in `src/**/*.rs`, assert the same scope contains `.zeroize()` within N=30 lines (heuristic; tune to existing Phase 6.3 shape).
- `test_no_sensitive_identifiers_in_tracing_macros` — find `tracing::{info,debug,warn,error}!(...)` and `console::log*!(...)` invocations, fail if any of `password, passphrase, master_key, key_file_bytes, decrypted, plaintext, mnemonic, recovery_phrase` appear inside the macro argument span.
- `test_csp_block_present_in_tauri_conf` — parse `src-tauri/tauri.conf.json`, assert `app.security.csp` is an object containing the 5 required directives from `CS-001`.
- `test_clipboard_plugin_absent_from_cargo_manifest` — parse `src-tauri/Cargo.toml`, assert `tauri-plugin-clipboard-manager` is not in `[dependencies]`.
- Each test must match the `cargo test ui::security` prefix filter (module path `ui::security_audit::<fn>`).

**Step 7 — Frontend state-clearing audit test.**
- Add a test inside `src-tauri/src/ui/security_audit.rs` (kept backend-side because the test harness for Leptos WASM is not available): `test_router_clears_state_on_lock_transition` — source-grep `src/app.rs` asserting the Effect with conditions `prev_unlocked && !current_unlocked` invokes both `vault_actions.clear()` and `sync_actions.clear()`.
- This supplements — does not replace — the existing implementation in `src/app.rs`.

**Step 8 — Threat-model documentation additions.**
- `design.md` §Threat Model Additions already documents compromised WebView, clipboard attack, and brute-force (lines 1648–1654). Confirm no edits needed; note in Section 7 as "no documentation change required" if verified.

**Step 9 — Run validation.**
- `cargo test --workspace --all-targets --all-features` (project-wide cargo invocation per feedback memory).
- `cargo test ui::security` (validation-checkpoint-specific filter).
- `trunk build` from repository root — must succeed with no warnings.
- Manual: `trunk serve`, open app in Tauri dev shell, lock vault, confirm DevTools → Application → Storage shows empty `localStorage` / `sessionStorage` / `IndexedDB` / `Cookies`. Observe console: no CSP violations. Attempt 6 bad-password authentications, confirm latency progression matches `CS-002`.

---

## 6. Review focus areas

### 6a. Rust change surface (anticipated files under `src-tauri/**/*.rs`)

- `src-tauri/src/auth/session/manager.rs` — add backoff state fields, apply backoff in `authenticate()`, add `#[cfg(test)]` backoff tests.
- `src-tauri/src/ui/auth_commands.rs` — remove stale `// TODO(phase-6.4): backoff` marker; no logic change.
- `src-tauri/src/ui/mod.rs` — register `#[cfg(test)] mod security_audit;`.
- `src-tauri/src/ui/security_audit.rs` — new file, test-only module per `CS-005`.

### 6b. Security-sensitive paths

- **`src-tauri/src/auth/session/manager.rs`** — backoff logic must:
  - Use `tokio::time::sleep` (not blocking sleep) so it cannot stall the Tauri runtime.
  - Not leak timing information about *why* the delay is applied (same delay shape regardless of whether `failed_attempts` was 1 or 5 — the post-delay error type is unchanged, only timing differs).
  - Never log the attempt count or the delay duration (would reveal brute-force progress to a compromised WebView via timing side-channel is already the threat; adding logs amplifies it).
  - Reset counters on success *before* handing the session to the caller so a caller-timing side-channel does not observe reset order.
  - Leave the `authenticate_gate` semaphore semantics unchanged — the gate serializes attempts, backoff adds a delay *inside* the serialized critical section, which prevents concurrent-attempt bypass (design §Threat Model Additions line 1654 "cannot be bypassed via concurrent requests").
- **`src-tauri/src/ui/security_audit.rs`** — audit tests must themselves not embed real passwords, keys, or secret data; use only the *names* of forbidden identifiers as `&str` literals.
- **`src-tauri/tauri.conf.json`** — CSP directive values must match `CS-001` exactly (no permissive additions like `unsafe-eval`, `*`, or any remote origin).
- **`src-tauri/capabilities/default.json`** — no new permissions added; only README sidecar documentation.

### 6c. Architecture risk areas

- `src-tauri/src/auth/session/manager.rs` — adding 2 atomic fields to `SessionManager` grows the struct and touches its core invariant. Verify:
  - SRP: backoff tracking belongs to the session lifecycle manager (yes — authenticate() already lives there).
  - No leakage: the atomics are private and not exposed via any accessor.
  - Dependency flow: no new downstream dependency; `auth::session` stays below `ui`.
- `src-tauri/src/ui/security_audit.rs` — source-grep tests introduce a dependency from the `ui` module on filesystem walks of both `src-tauri/src/` and the sibling `src/` (frontend). Verify the path resolution uses `CARGO_MANIFEST_DIR` anchors so tests are location-independent.
- Module visibility discipline: `security_audit` must be `#[cfg(test)] mod`, never `pub mod`.

### 6d. Testing requirements

- **From sub-phase validation checkpoint:**
  - `cargo test ui::security` passes (all audit tests + backoff tests whose module path matches the filter; note backoff tests live under `auth::session::manager`, run via broader `cargo test --workspace`).
  - `trunk build` succeeds.
- **Edge cases surfaced in Step 2:**
  - Backoff applied only on credential failure (not `KeyFileNotFound`, `SessionAlreadyActive`, `MemoryLockFailed`, gate-closed).
  - Backoff delay uses virtual time — tests must not depend on wall-clock sleep.
  - CSP violations detected only at runtime — manual `trunk serve` verification required; cannot be automated in CI without a headless browser.
  - Source-grep tests must be robust to comments containing forbidden tokens (either tolerate comments by asserting on tokens inside `//`-stripped source, or require that no comment reference forbidden identifiers either — choose the stricter form for determinism).
- **Validation anchors:**
  - Delay sequence: `1s, 2s, 4s, 8s, 16s, 30s` for attempts 1..=6; `30s` for attempts 7+.
  - Counter reset on success: verifiable by attempting 3 failures then 1 success, then asserting immediate re-authentication.

---

## 7. Documentation impact

| Path | Change | When |
|---|---|---|
| `docs/architecture/designs/tauri-ipc-and-frontend/design.md` | No change required — §CSP Configuration, §Threat Model Additions, §Security Analysis, §Zero-Trace Compliance all already reflect Phase 6.4 final state. | n/a |
| `docs/architecture/designs/tauri-ipc-and-frontend/sub-phases/6.4-zero-trace-and-security-hardening.md` | Update "last verified" timestamp if the project convention applies. | Deferred — rationale: the sub-phase doc is a planning artefact; no canonical change. Implementer can skip without flagging a silent skip. |
| `src-tauri/capabilities/README.md` | **New file** per `CS-006`. Documents clipboard/http absence as explicit denials. | Required this run |
| `docs/architecture/design-invariants.md` | Confirm invariants #6 (IPC sensitive input) and #7 (Zero-Trace persistence) still match Phase 6.4 enforcement. | Required this run — read and verify; edit only if drift is found. |

---

## 8. Governance sync actions (pre-implementation)

| Action ID | Reason / linked concern | Target files | Required edit | Verification |
|---|---|---|---|---|
| **G-1** | Section 3 concern: backoff applied only on `InvalidCredentials` after successful KDF; `KeyFileNotFound`/`MemoryLockFailed`/`SessionAlreadyActive` paths are not throttled. Rule file currently has no such guidance. | `C:\Users\chris\source\repos\arx-runa\.claude\rules\auth.md` | Under `## Session`, append bullet: `Authentication backoff: SessionManager applies delay = min(30s, 2^(attempts-1) s) before returning InvalidCredentials, where attempts counts consecutive failures of credential verification after KDF derivation. KeyFileNotFound, MemoryLockFailed, and SessionAlreadyActive do not increment the counter. Counter resets on any successful authenticate(); it is not persisted across process restarts.` | Grep `.claude/rules/auth.md` for `min(30s, 2^(attempts-1)`; ensure it appears exactly once under `## Session`. |
| **G-2** | Section 3 concern: `cargo test ui::security` requires test path prefix `ui::security`; rule currently does not specify audit-test location. | `C:\Users\chris\source\repos\arx-runa\.claude\rules\tauri.md` | Under `## IPC / UI layer (src/ui/)`, append bullet: `Zero-Trace audit tests live in src-tauri/src/ui/security_audit.rs as #[cfg(test)] mod security_audit, reachable via the cargo test ui::security prefix filter. Audit tests MUST NOT embed real secret data; they only reference forbidden-identifier string literals.` | Grep `.claude/rules/tauri.md` for `security_audit`; ensure it appears exactly once under `## IPC / UI layer`. |
| **G-3** | Sub-phase deliverable 6: confirm that the "last verified against design dated" markers in the rule files still match the current design revision (2026-04-12 for auth, 2026-04-11 for leptos, 2026-04-12 for tauri). | `C:\Users\chris\source\repos\arx-runa\.claude\rules\auth.md`, `C:\Users\chris\source\repos\arx-runa\.claude\rules\tauri.md`, `C:\Users\chris\source\repos\arx-runa\.claude\rules\leptos.md` | Read only. If the design has been updated after those dates, bump the "last verified" line in each rule; otherwise no change. | Grep each file for `last verified against design dated`; compare to the `design.md` modification date or front-matter revision. |
| **G-4** | Governance hygiene after rule edits. | n/a | Run `/copilot-sync` (or the project-specific sync command) after any of G-1, G-2, G-3 modifies a rule file. | Synchronisation agent reports no drift. |

---

## 9. Handoff Notes for Implementer

Working directory: `C:\Users\chris\source\repos\arx-runa`. Order of operations: (1) execute the four Governance sync actions in Section 8 *before* any code changes; (2) Steps 1–2 (CSP + clipboard README) are standalone config edits; (3) Steps 3–4 (backoff logic in `SessionManager`) must be implemented together with Step 5 (backoff tests) because the test names pin the exact delay sequence; (4) Step 6 (`security_audit.rs`) is independent and can run in parallel with backoff work; (5) Step 9 validation runs last. The plan is self-contained — all contract snippets, delay formula, file paths, and test names are inlined under Section 5. Traps: (a) `tokio::time::pause()` requires the `test-util` feature on `tokio`; check `src-tauri/Cargo.toml` `[dev-dependencies]` and add `tokio = { version = "...", features = ["test-util"] }` if missing. (b) `trunk build` requires `trunk` on PATH; if CI lacks it, document manual verification. (c) The source-grep audit tests will fire false positives on their own string literals — use `concat!` or hex-escaped tokens to keep the forbidden identifiers invisible to their own scanners. (d) The frontend `src/app.rs` Router Effect is the authoritative lock hook; do not add a second clear-on-lock path. Status is `draft`; proceed after user approval.
