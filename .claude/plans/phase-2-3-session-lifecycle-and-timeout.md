---
title: "Phase 2.3 — Session Lifecycle and Timeout"
created: "2026-04-13T00:00:00Z"
status: approved
roadmap-phase: 2
sub-phase: "2.3"
design-document: "docs/architecture/designs/authentication-and-session-management/design.md"
sub-phase-roadmap: "docs/architecture/designs/authentication-and-session-management/sub-phases/roadmap.md"
test-agent-required: true
governance-sync-required: true
tags: [auth, phase-2, session, timeout, state-machine, tokio, zeroization]
---

# Plan: Phase 2.3 — Session Lifecycle and Timeout

## 1. Goal

Implement `SessionManager` in `src-tauri/src/auth/session.rs`: a `NoSession → Active → Expired` state machine that wraps Phase 2.2's `SessionKeys` behind `Arc<RwLock<Option<SessionKeys>>>`, enforces an activity-reset tokio timeout with a 60-second pre-warning broadcast, and delays zeroization until all in-flight operations complete via a watch-channel-backed operation gate.

## 2. Context

**Roadmap**: Phase 2 — Authentication and Session Management (`docs/roadmap.md` lines 55–61). Depends on Phase 1 (complete) and Phases 2.1, 2.2 (complete). Produces `SessionManager` consumed by Phase 2.4 (vault ceremonies) and Phase 6.1 (IPC layer — reset timer on each command, subscribe to session events).

**Sub-phase roadmap**: `docs/architecture/designs/authentication-and-session-management/sub-phases/roadmap.md`. Strict order 2.1 → 2.2 → 2.3 → 2.4. 2.3 is the third unit. Security review **required** per the roadmap's Security Review Checkpoints. Estimated scope: ~150 lines production + ~120 lines tests.

**Sub-phase doc**: `docs/architecture/designs/authentication-and-session-management/sub-phases/2.3-session-lifecycle-and-timeout.md` (deliverables 1–8).

**Parent design sections used** (absolute paths):

- `docs/architecture/designs/authentication-and-session-management/design.md` lines 182–252: Session Management — `SessionKeys`, `SharedSession = Arc<RwLock<Option<SessionKeys>>>`, ownership model, memory-lock failure messages.
- Same file lines 225–270: Session lifecycle + Session-lived `rclone.conf` handling + Timeout mechanism + Timeout UX.
- Same file lines 272–300: `AuthenticationError` enum.
- Same file lines 21–47: Contract Surface — canonical interface/data/invariant/dependency contracts (binding).
- `docs/architecture/design-invariants.md` §6 (IPC sensitive-input handling), §7 (Zero-Trace persistence — clear contexts on lock), §9 (Argon2 vault-header trust contract — not directly exercised here but informs authenticate() boundary).
- `docs/architecture/designs/cloud-synchronisation/design.md` lines 308–332 (Destination Session Storage — referenced only to identify what `lock()` defers to Phase 4).
- `docs/architecture/designs/tauri-ipc-and-frontend/design.md` lines 1587–1612 (State Clearing on Lock — informs `subscribe()` receiver shape) and line 1639 (in-memory exponential backoff, explicitly out of scope for 2.3 — see DC-11).

**Existing state** (branch `development`):

- `src-tauri/src/auth/mod.rs` already declares `pub mod session;` and re-exports `SessionKeys` (crate-visible). `SessionKeys::derive(password, key_file, salt, params)` is implemented per Phase 2.2 and unit-tested.
- `src-tauri/src/auth/session.rs` currently contains only the `SessionKeys` struct + `derive()` + unit tests. No `SessionManager`, no state machine, no timer, no config loader.
- `src-tauri/src/auth/error.rs` defines `AuthenticationError` with `InvalidCredentials`, `KeyFileNotFound`, `MemoryLockFailed(String)`, `VaultHeaderInvalid`, `KeySource(#[from] KeySourceError)`. Missing: `SessionAlreadyActive`, `InvalidRecoveryPhrase`, `NoRecoverySlot` (last two deferred to Phase 2.4).
- `src-tauri/src/auth/kdf.rs` exposes `Argon2Params { memory_cost_kib, time_cost, parallelism }` with `DEFAULT`.
- `src-tauri/src/auth/key_source.rs` exposes `KeySource::read_key() -> Result<Zeroizing<[u8; 32]>, KeySourceError>` — trait object safe.
- `src-tauri/src/memory/secure_buffer.rs` provides `SecureBytes<N>` with locked-on-construction / zero-on-drop semantics. `src-tauri/src/memory/platform` exposes `take_last_unlock_snapshot()` / `clear_last_unlock_snapshot()` under `#[cfg(test)]` — reused here for zeroization verification.
- `src-tauri/Cargo.toml` pins `tokio = { version = "1", features = ["macros", "rt-multi-thread", "fs", "io-util", "sync", "time"] }` — `sync` includes `watch`, `broadcast`, `oneshot`, `RwLock`; `time` includes `sleep`. **No new Cargo dependencies required.** `dirs = "6"` is already pinned (used here for platform config path).
- `.claude/rules/auth.md` describes session memory protection but does NOT yet mention `SessionManager`, operation gate, or timer cadence. Governance sync (Section 9) updates the rule so post-2.3 the auth rule reflects the state machine.
- `.github/instructions/auth.instructions.md` mirrors `.claude/rules/auth.md` — must stay in sync via `/copilot-sync`.

**No pending architectural decisions** in the roadmap touch Phase 2.3 directly.

## 3. Design Concerns / Open Questions

### DC-1 — Sub-phase deliverable 2 requires `authenticate()` to "open the SQLCipher DB"; Phase 3 does not exist yet

- **Concern**: Deliverable 2 says `authenticate()` "opens the SQLCipher DB with `sqlcipher_key`". SQLCipher and the `storage` module are Phase 3.1 deliverables. Phase 2.3's own "Depends on" line declares only Phase 2.2. Literally implementing this would introduce a cross-phase dependency that contradicts the sub-phase's dependency statement.
- **Source**: `docs/architecture/designs/authentication-and-session-management/sub-phases/2.3-session-lifecycle-and-timeout.md` line 13 (deliverable 2) vs line 6 ("Depends on: Phase 2.2").
- **Impact**: Without resolution, Codex would either stub SQLCipher silently, pull in Phase 3 prematurely, or drop the deliverable.
- **Classification**: Non-blocking.
- **Resolution**: Scope Phase 2.3 `authenticate()` to the in-memory state machine only — it delegates to `SessionKeys::derive(...)`, installs the keys behind the `Arc<RwLock<Option<SessionKeys>>>`, and transitions state to `Active`. It does **not** open a SQLCipher connection. Phase 3.1 will extend `SessionManager` to take a storage backend when that phase lands. A `// TODO(phase-3.1): open SQLCipher DB with session_keys.sqlcipher_key` comment marks the extension point.
- **Documentation sync required on implementation**: YES.
  - `docs/architecture/designs/authentication-and-session-management/sub-phases/2.3-session-lifecycle-and-timeout.md` deliverable 2 (line 13): append "SQLCipher DB opening is deferred to Phase 3.1 extension of this method."
  - Parent `design.md` line 243 ("SQLCipher DB is opened with `sqlcipher_key`") is aspirational for the final shape — add a note linking to Phase 3.1.

### DC-2 — Sub-phase deliverable 3 requires `lock()` to "close SQLCipher" and "overwrite/delete session-lived temp rclone.conf"; Phases 3/4 do not exist yet

- **Concern**: Deliverable 3 says `lock()` "closes the SQLCipher connection, overwrites and deletes the session-lived temp `rclone.conf`". SQLCipher is Phase 3.1; rclone.conf management is Phase 4 (Destination Session Storage). Neither subsystem exists.
- **Source**: Same sub-phase doc line 14 (deliverable 3); cross-reference `docs/architecture/designs/cloud-synchronisation/design.md` lines 326–332 (session close flow).
- **Impact**: Same as DC-1 — silent deferral or out-of-scope dependency.
- **Classification**: Non-blocking.
- **Resolution**: Phase 2.3 `lock()` scope is:
  1. Wait for the operation counter to drop to zero (via a `tokio::sync::watch` subscription).
  2. Acquire the write lock on the `Arc<RwLock<Option<SessionKeys>>>` and call `.take()` — the dropped `Option` triggers `SessionKeys` drop → `SecureBytes<32>::drop` → zero → munlock/VirtualUnlock.
  3. Transition the lifecycle state to `Expired`.
  4. Cancel any pending timer task.
  5. Broadcast `SessionEvent::Locked` on the event channel.

  SQLCipher close and rclone.conf cleanup are explicit TODO extension points:
  - `// TODO(phase-3.1): close SQLCipher connection here before zeroizing.`
  - `// TODO(phase-4): overwrite and unlink temp rclone.conf here before zeroizing.`

  Phase 3.1 and Phase 4 will refactor `lock()` to run those steps in the correct order relative to zeroization. Phase 2.3 leaves deterministic hook points.
- **Documentation sync required on implementation**: YES.
  - Sub-phase doc line 14 (deliverable 3): append "SQLCipher close is deferred to Phase 3.1; rclone.conf cleanup is deferred to Phase 4."

### DC-3 — `authenticate()` signature is under-specified: salt/params sources not stated, KeySource mandatory-ness unclear

- **Concern**: Sub-phase text says `authenticate()` "accepts password bytes and a `KeySource` reference". It does not say how salt and Argon2 params reach the method, nor whether `KeySource` is mandatory (Tier 1 has no key file).
- **Source**: Sub-phase doc line 13 (deliverable 2).
- **Impact**: Codex would guess — either hardcode salt, rely on a vault header type that does not exist, or require a `KeySource` for Tier 1.
- **Classification**: Non-blocking.
- **Resolution**: Phase 2.3 `authenticate()` takes the inputs directly:
  ```rust
  pub async fn authenticate(
      &self,
      password_utf8_bytes: &[u8],
      key_source: Option<&(dyn KeySource + Send + Sync)>,
      salt: &[u8; 32],
      params: &Argon2Params,
  ) -> Result<(), AuthenticationError>;
  ```
  Tier 1 passes `None`; Tier 2 passes `Some(&source)`. Phase 2.4 will wrap this with vault-header loading. Keeping Phase 2.3 pure (no vault header dependency) matches "Depends on: Phase 2.2".
- **Documentation sync required on implementation**: YES.
  - Sub-phase doc line 13: replace "`KeySource` reference" with "`Option<&(dyn KeySource + Send + Sync)>` (`None` for Tier 1, `Some(&source)` for Tier 2)".
  - Add a line: "salt and Argon2 params are parameters; vault-header loading is Phase 2.4's responsibility".

### DC-4 — State machine cannot be represented by `Option<SessionKeys>` alone

- **Concern**: The canonical `SharedSession = Arc<RwLock<Option<SessionKeys>>>` conflates `NoSession` (never authenticated) and `Expired` (was active, now locked) — both are `None`. The sub-phase's tests require distinguishing them ("Session transitions from `NoSession` to `Active`", "Session transitions to `Expired` when `lock()` is called manually", "Re-authentication after `Expired` transitions back to `Active`").
- **Source**: Sub-phase doc deliverable 1 (line 12) and test list items 1, 2, 6 (lines 20–25).
- **Impact**: No resolution means one of (a) state untestable, (b) redefine `SharedSession`, (c) add parallel tracking. Design contract pins (b) as a violation.
- **Classification**: Non-blocking.
- **Resolution**: Keep `SharedSession = Arc<RwLock<Option<SessionKeys>>>` as the key-access boundary (contract-compliant). Track lifecycle as a parallel `Arc<RwLock<LifecycleState>>` where:
  ```rust
  #[derive(Debug, Clone, Copy, PartialEq, Eq)]
  pub enum LifecycleState {
      NoSession,
      Active,
      Expired,
  }
  ```
  Expose `SessionManager::state() -> LifecycleState` for tests and IPC. Every transition is atomic under the write lock on the lifecycle state.
- **Documentation sync required on implementation**: YES.
  - Parent `design.md` lines 199–210 (Session ownership and sharing): add a paragraph noting that `SessionManager` tracks a lifecycle enum alongside `SharedSession` to distinguish `NoSession` from `Expired`.

### DC-5 — `SessionWarning` event delivery mechanism not specified; auth module cannot depend on Tauri

- **Concern**: Sub-phase deliverable 7 says "the timer task sends a `SessionWarning` event to the Tauri frontend 60 seconds before the timeout fires". But the auth module must not depend on `tauri::AppHandle` — that dependency is Phase 6.1 scope and would create a circular dependency.
- **Source**: Sub-phase doc deliverable 7 (line 18).
- **Impact**: Without guidance, Codex might add a Tauri dependency to the auth module or omit the warning entirely.
- **Classification**: Non-blocking.
- **Resolution**: `SessionManager` owns a `tokio::sync::broadcast::Sender<SessionEvent>`. Shape:
  ```rust
  #[derive(Debug, Clone)]
  pub enum SessionEvent {
      TimeoutWarning { seconds_remaining: u64 },
      Locked,
      Expired,
  }
  ```
  Phase 2.3 exposes `pub fn subscribe(&self) -> tokio::sync::broadcast::Receiver<SessionEvent>`. Phase 6.1 will spawn a subscriber task that forwards events to `AppHandle::emit`. Broadcast channel capacity = 16 (small, events are rare). If no subscribers exist, emission is a no-op (`.send(...)` returns `Err(NoSubscribers)` — plan ignores this via `let _ = sender.send(event);`).
- **Documentation sync required on implementation**: YES.
  - Sub-phase doc deliverable 7 (line 18): replace "sends a `SessionWarning` event to the Tauri frontend" with "emits `SessionEvent::TimeoutWarning { seconds_remaining }` on the internal broadcast channel; Phase 6.1 forwards to Tauri events".

### DC-6 — `reset_timer()` caller not defined; IPC layer does not exist yet

- **Concern**: Deliverable 5 says the timer "is cancelled and restarted on every Tauri IPC command invocation via a `reset_timer()` call". The IPC layer is Phase 6.1 scope.
- **Source**: Sub-phase doc deliverable 5 (line 16).
- **Impact**: Phase 2.3 cannot wire IPC callers. Still must expose the public API for Phase 6.1 to call later.
- **Classification**: Non-blocking.
- **Resolution**: Expose `pub fn reset_timer(&self)` on `SessionManager`. In Phase 2.3 it is called only from tests. Phase 6.1 will call it from a Tauri IPC middleware-style wrapper. Document this in sub-phase doc.
- **Documentation sync required on implementation**: NO (already documented in Phase 6.1 design).

### DC-7 — Two distinct "wait-for-operations" mechanisms are described and must coexist without conflict

- **Concern**: The parent design (lines 204–210) relies on `RwLock` read-guard lifetime to delay zeroing: "the write lock cannot be acquired while any reader holds a guard". The sub-phase (deliverable 6) describes a different mechanism: "an atomic counter incremented on operation start and decremented on completion; when the timeout fires, the session manager checks the counter and waits (via `tokio` notify) until it reaches zero before calling `lock()`". These are two different mechanisms.
- **Source**: Design.md lines 204–210; sub-phase doc deliverable 6 (line 17).
- **Impact**: Implementing only one misses the other's guarantee. Implementing both is required because the design explicitly says "File operations acquire a read lock, borrow the keys for the duration of the key lookup only (not for the entire I/O), then release the lock" — so the RwLock does NOT span the full operation, and the counter is required.
- **Classification**: Non-blocking.
- **Resolution**: Phase 2.3 implements **both**, because they guard different windows:
  - `RwLock` guards the key-access window (cannot read a zeroed key). Enforced by Rust's borrow checker on the lock guard.
  - Operation counter guards the operation lifecycle (cannot zero while a multi-step operation is mid-flight even though no read guard is held right now).

  Use `tokio::sync::watch::channel::<u32>(0)` for the counter. `begin_operation()` calls `sender.send_modify(|c| *c += 1)` and returns an `OperationGuard` whose `Drop` calls `sender.send_modify(|c| *c -= 1)`. The timer task awaits `receiver.wait_for(|c| *c == 0)` before acquiring the write lock. `watch` handles the register-before-check race that `Notify` would introduce.
- **Documentation sync required on implementation**: YES.
  - Parent `design.md` lines 225–270 (Timeout mechanism): add a paragraph clarifying both guards are used, with links to the sub-phase rationale.

### DC-8 — Local config file schema and macOS path unspecified

- **Concern**: Sub-phase deliverable 4 says the timeout is stored in local config at `%APPDATA%/arx-runa/config.json` (Windows) or `~/.local/share/arx-runa/config.json` (Linux). macOS path is absent. JSON schema is absent.
- **Source**: Sub-phase doc deliverable 4 (line 15) and Implementation Notes (line 68).
- **Impact**: Codex would pick a schema arbitrarily or hard-code the default.
- **Classification**: Non-blocking.
- **Resolution**: Use `dirs::config_dir()` joined with `arx-runa/config.json`. This yields:
  - Windows: `%APPDATA%\arx-runa\config.json`
  - Linux: `$XDG_CONFIG_HOME/arx-runa/config.json` (fallback `~/.config/arx-runa/config.json`) — **note: sub-phase says `~/.local/share` which is `dirs::data_dir()`; treat this as advisory and use `dirs::config_dir()` for the `config.json` name, which is the canonical convention**. Keep this consistent with Phase 2.1's `dirs` usage.
  - macOS: `~/Library/Application Support/arx-runa/config.json`

  Schema (JSON):
  ```json
  {
    "schema_version": 1,
    "session_timeout_secs": 900
  }
  ```
  - Missing file → use default 900 seconds. Not an error.
  - Malformed JSON → use default 900 seconds + `tracing::warn!`. Not an error.
  - `session_timeout_secs` below 60 → clamp to 60 and warn.
  - `session_timeout_secs` above 86_400 → clamp to 86_400 and warn.
- **Documentation sync required on implementation**: YES.
  - Sub-phase doc line 68: replace the OS-specific path list with "use `dirs::config_dir()` joined with `arx-runa/config.json`; all three platforms derive their location from this crate".
  - Parent `design.md` lines 260–265 (Timeout mechanism): add the JSON schema and path.

### DC-9 — Re-authentication while `Active`: behavior undefined

- **Concern**: Sub-phase's test "Re-authentication after `Expired` transitions back to `Active`" implies the `Expired → Active` path. `Active → Active` (caller re-invokes authenticate without locking first) is undefined.
- **Source**: Sub-phase doc test list item 6 (line 25).
- **Impact**: Codex might silently replace keys (leaking the prior session) or panic.
- **Classification**: Non-blocking.
- **Resolution**: `authenticate()` returns `AuthenticationError::SessionAlreadyActive` if `state() == Active`. Add the variant to the enum (safe under `#[non_exhaustive]`). Add a test for the rejection.
- **Documentation sync required on implementation**: YES.
  - Parent `design.md` lines 276–296 (`AuthenticationError` enum): add the new variant.
  - `.claude/rules/auth.md` error list (governance sync — see Section 9).

### DC-10 — `KeySourceError` → `AuthenticationError` mapping inside `authenticate()` is oracle-sensitive

- **Concern**: The current blanket `From<KeySourceError> for AuthenticationError` maps into `KeySource(KeySourceError::…)` — which preserves the underlying variant and could leak path/IO details to the IPC boundary. `authenticate()` specifically should not distinguish "file disappeared" from "wrong password" to the caller.
- **Source**: `src-tauri/src/auth/error.rs` lines 31–34 and design.md lines 293–296.
- **Impact**: Ambiguous error semantics; possible information leakage.
- **Classification**: Non-blocking.
- **Resolution**: Do not rely on the blanket `From` inside `authenticate()`. Use a local match that maps:
  - `KeySourceError::NotFound` → `AuthenticationError::KeyFileNotFound` (explicitly allowed by design as "does not reveal password status").
  - `KeySourceError::InvalidSize { .. }` → `AuthenticationError::KeyFileNotFound` (same semantics — no 32-byte content usable).
  - `KeySourceError::IoFailed(_)` → `AuthenticationError::InvalidCredentials` + `tracing::warn!` with the underlying IO detail logged server-side only.

  Do not modify the existing `From` impl — other call sites (Phase 2.4 recovery flows) may want the preserved variant.
- **Documentation sync required on implementation**: NO (internal decision, not a design-level contract).

### DC-11 — Exponential backoff on auth failure: Phase 6.1 says `SessionManager` owns it; Phase 2.3 sub-phase does not list it

- **Concern**: `docs/architecture/designs/tauri-ipc-and-frontend/design.md` line 1639 says "backend applies per-vault in-memory exponential backoff in `SessionManager` (`delay = min(30s, 2^(attempt-1)s)`)". Phase 2.3 sub-phase deliverables do not mention this.
- **Source**: Cross-design drift between 2.3 sub-phase doc and tauri-ipc design.md.
- **Impact**: Two designs disagree on which phase owns backoff.
- **Classification**: Non-blocking.
- **Resolution**: **Defer backoff to Phase 2.4 or Phase 6.1**. Phase 2.3 implements only the lifecycle + timer. This is called out so the reader understands the omission is deliberate. The hook: `authenticate()`'s `Err` path is where a future per-vault backoff gate can be inserted — leave a `// TODO(phase-2.4/6.1): exponential backoff on InvalidCredentials` comment.
- **Documentation sync required on implementation**: YES.
  - Sub-phase 2.3 doc: add a line under Notes stating "Per-vault exponential backoff is deferred to a later sub-phase (Phase 2.4 or Phase 6.1); see `tauri-ipc-and-frontend/design.md` line 1639".

### DC-12 — Governance drift: `.claude/rules/auth.md` and `.github/instructions/auth.instructions.md` do not yet describe `SessionManager`, operation gate, or timer

- **Concern**: Both rule files describe `SessionKeys` and mlock, but are silent on `SessionManager`, `LifecycleState`, `OperationGuard`, `reset_timer`, and `SessionEvent`. After Phase 2.3, implementers editing `src-tauri/src/auth/**` would see stale rules missing the new surface.
- **Source**: `.claude/rules/auth.md` (Session section lines 28–32) and `.github/instructions/auth.instructions.md` (mirror).
- **Impact**: Future work in the auth module would miss lifecycle guidance.
- **Classification**: Non-blocking. Deterministic file update — handled by Section 9 pre-implementation governance sync.
- **Resolution**: Governance sync action GS-1 (Section 9) adds a "Session manager" bullet list to `.claude/rules/auth.md` then runs `/copilot-sync` to regenerate `.github/instructions/auth.instructions.md`. Also extends the Errors bullet list to include `SessionAlreadyActive`.

## 4. Assumptions

The following assumptions are made because the sub-phase / design does not explicitly specify them. If any is wrong, the implementation is wrong — correct before handoff.

- **A-1**: `authenticate()` is async (uses `.await`). Argon2id is a heavy CPU operation; run it inside `tokio::task::spawn_blocking` so it does not starve the runtime. The `SessionKeys::derive` call happens inside `spawn_blocking`; the returned `SessionKeys` is moved back to the caller task.
- **A-2**: The timer task spawns on `tokio::runtime::Handle::current()`. `SessionManager::new()` requires a tokio runtime to be active when the first timer is spawned (i.e. from inside an async context). Tests use `#[tokio::test]`.
- **A-3**: Timeout config file path uses `dirs::config_dir()` not `dirs::data_dir()`, overriding the sub-phase's Linux path (`~/.local/share/arx-runa/config.json`) in favour of the canonical `$XDG_CONFIG_HOME/arx-runa/config.json`. Phase 2.1 already uses `dirs::config_dir()` for the key-file path hint — 2.3 follows the same convention.
- **A-4**: The timer task, when scheduled, splits the interval into `sleep(total - 60s)` → emit `TimeoutWarning { seconds_remaining: 60 }` → `sleep(60s)` → acquire write lock + zeroize. If `total_secs < 60`, skip the warning phase and sleep once for `total_secs`.
- **A-5**: `SessionEvent::TimeoutWarning.seconds_remaining` is always 60 in Phase 2.3. Future phases may reuse the field for partial warnings.
- **A-6**: `SessionEvent::Locked` is emitted when `lock()` is called by the user or by the timeout task. They are indistinguishable on the wire; frontend does not need to distinguish.
- **A-7**: The broadcast channel has capacity 16. If the channel fills (subscriber is slow), older events are dropped silently; this is acceptable because `Locked` supersedes any stale warning.
- **A-8**: `OperationGuard` is `#[must_use]`. Drop is the decrement path. Panics inside the operation still drop the guard (Rust's unwinding) — counter returns to zero. Tests cover the panic path.
- **A-9**: `lock()` is idempotent when state is already `Expired` (returns `Ok(())` without re-firing events). Calling `lock()` on `NoSession` is also `Ok(())` (no state change, no event).
- **A-10**: `reset_timer()` is a no-op unless state is `Active`. Calling it from `NoSession` or `Expired` returns silently.
- **A-11**: Tests may shorten the timeout via a new constructor `SessionManager::with_timeout_secs(config: u64)` (bypassing config file load). Production code calls `SessionManager::from_config()` which reads the JSON file.
- **A-12**: The `KeySource` read happens **inside** `authenticate()`, before Argon2id. If the key source fails, no memory is allocated for `SessionKeys`.
- **A-13**: `SessionKeys::derive` signature is unchanged from Phase 2.2. No modifications to `src-tauri/src/auth/session.rs` outside adding the new module items below the existing `SessionKeys` struct.
- **A-14**: No new Cargo dependencies. All required types come from already-pinned `tokio`, `zeroize`, `thiserror`, `tracing`, `serde`, `serde_json`, `dirs`.

## 5. Approach

### Step 1 — Extend `AuthenticationError` with `SessionAlreadyActive`

**File**: `C:\Users\chris\source\repos\arx-runa\src-tauri\src\auth\error.rs`

Add a new variant to the existing `#[non_exhaustive]` enum (after `VaultHeaderInvalid`):

```rust
/// `authenticate()` was called while a session was already `Active`.
#[error("session is already active; call lock() before re-authenticating")]
SessionAlreadyActive,
```

Add a corresponding display test in the existing `#[cfg(test)] mod tests`:

```rust
#[test]
fn test_authentication_error_session_already_active_display_matches_design() {
    assert_eq!(
        AuthenticationError::SessionAlreadyActive.to_string(),
        "session is already active; call lock() before re-authenticating",
    );
}
```

### Step 2 — Create timeout config loader

**File**: `C:\Users\chris\source\repos\arx-runa\src-tauri\src\auth\config.rs` (new)

Signature (inline here verbatim):

```rust
//! Session timeout configuration loaded from the platform local config file.
//!
//! Path: `dirs::config_dir()` joined with `arx-runa/config.json`.
//! Default if file is missing, unreadable, or malformed: 900 seconds.

use std::fs;
use std::path::PathBuf;
use std::time::Duration;

use serde::Deserialize;

const CONFIG_SUBDIRECTORY: &str = "arx-runa";
const CONFIG_FILE_NAME: &str = "config.json";
const DEFAULT_SESSION_TIMEOUT_SECONDS: u64 = 900;
const MINIMUM_SESSION_TIMEOUT_SECONDS: u64 = 60;
const MAXIMUM_SESSION_TIMEOUT_SECONDS: u64 = 86_400;
const CURRENT_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Deserialize)]
struct SessionConfigFile {
    schema_version: u32,
    session_timeout_secs: u64,
}

/// Returns the session timeout duration clamped to
/// `[MINIMUM_SESSION_TIMEOUT_SECONDS, MAXIMUM_SESSION_TIMEOUT_SECONDS]`.
/// Reads `dirs::config_dir() / "arx-runa/config.json"`. On any failure
/// (missing file, IO error, invalid JSON, unknown schema version) logs a
/// warning and returns the default of 900 seconds.
pub fn load_session_timeout() -> Duration {
    let path = match config_file_path() {
        Some(path) => path,
        None => {
            tracing::warn!("no platform config directory available; using default session timeout");
            return Duration::from_secs(DEFAULT_SESSION_TIMEOUT_SECONDS);
        }
    };

    let raw = match fs::read_to_string(&path) {
        Ok(raw) => raw,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Duration::from_secs(DEFAULT_SESSION_TIMEOUT_SECONDS);
        }
        Err(error) => {
            tracing::warn!(?error, "failed to read session config; using default");
            return Duration::from_secs(DEFAULT_SESSION_TIMEOUT_SECONDS);
        }
    };

    let parsed: SessionConfigFile = match serde_json::from_str(&raw) {
        Ok(parsed) => parsed,
        Err(error) => {
            tracing::warn!(?error, "invalid session config JSON; using default");
            return Duration::from_secs(DEFAULT_SESSION_TIMEOUT_SECONDS);
        }
    };

    if parsed.schema_version != CURRENT_SCHEMA_VERSION {
        tracing::warn!(
            version = parsed.schema_version,
            "unknown session config schema version; using default",
        );
        return Duration::from_secs(DEFAULT_SESSION_TIMEOUT_SECONDS);
    }

    let clamped = parsed
        .session_timeout_secs
        .clamp(MINIMUM_SESSION_TIMEOUT_SECONDS, MAXIMUM_SESSION_TIMEOUT_SECONDS);
    Duration::from_secs(clamped)
}

fn config_file_path() -> Option<PathBuf> {
    let mut path = dirs::config_dir()?;
    path.push(CONFIG_SUBDIRECTORY);
    path.push(CONFIG_FILE_NAME);
    Some(path)
}

#[cfg(test)]
mod tests {
    // Tests go here — see Section 7 test list.
}
```

Tests (in the same file under `#[cfg(test)] mod tests`):
- `test_load_session_timeout_returns_default_when_file_is_missing`: point `config_file_path` at a temp directory with no file. Cannot directly override `dirs::config_dir()`, so tests use a helper `parse_config_bytes` refactor: extract the string-parsing logic into a private `fn parse_config_bytes(raw: &str) -> Duration` and unit-test that helper. Integration coverage of the filesystem path is covered by the sub-phase's manual-verification step.
- `test_parse_config_bytes_returns_default_for_invalid_json`
- `test_parse_config_bytes_returns_default_for_unknown_schema_version`
- `test_parse_config_bytes_clamps_below_minimum_to_60s`
- `test_parse_config_bytes_clamps_above_maximum_to_86400s`
- `test_parse_config_bytes_returns_exact_value_when_in_range`

### Step 3 — Register `config` module in `auth::mod`

**File**: `C:\Users\chris\source\repos\arx-runa\src-tauri\src\auth\mod.rs`

Add between existing module declarations:

```rust
pub mod config;
```

No re-export from the top of `mod.rs` is required — `config::load_session_timeout()` is accessed as `crate::auth::config::load_session_timeout()` from `session.rs`.

### Step 4 — Define `SessionEvent` and `LifecycleState` in `session.rs`

**File**: `C:\Users\chris\source\repos\arx-runa\src-tauri\src\auth\session.rs`

Append after the existing `SessionKeys` struct (leave `SessionKeys` and its tests untouched):

```rust
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{RwLock, broadcast, oneshot, watch};
use tokio::task::JoinHandle;

use crate::auth::KeySource;
use crate::auth::config;
use crate::auth::error::{AuthenticationError, KeySourceError};
use crate::auth::kdf::Argon2Params;

const PRE_WARNING_SECONDS: u64 = 60;
const BROADCAST_CHANNEL_CAPACITY: usize = 16;

/// Session lifecycle state. Tracks the transitions
/// `NoSession → Active → Expired → Active`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecycleState {
    /// No session has ever been established.
    NoSession,
    /// A session is active; keys are live in `SharedSession`.
    Active,
    /// A prior session expired (manual `lock()` or timeout);
    /// keys are zeroized.
    Expired,
}

/// Events broadcast by the session manager. Consumers subscribe via
/// [`SessionManager::subscribe`]. Phase 6.1 forwards these to Tauri events.
#[derive(Debug, Clone)]
pub enum SessionEvent {
    /// Emitted `PRE_WARNING_SECONDS` before the timeout fires.
    TimeoutWarning { seconds_remaining: u64 },
    /// Emitted after `lock()` completes — manual or timeout-triggered.
    Locked,
}

type SharedSession = Arc<RwLock<Option<SessionKeys>>>;

/// Shared timer-cancellation sender. Dropping the sender cancels the
/// in-flight timer task.
struct TimerHandle {
    cancel: oneshot::Sender<()>,
    join: JoinHandle<()>,
}

/// Owns the session state machine, timeout timer, operation gate, and
/// event broadcast channel.
pub struct SessionManager {
    session: SharedSession,
    lifecycle: Arc<RwLock<LifecycleState>>,
    timeout: Duration,
    timer: Arc<tokio::sync::Mutex<Option<TimerHandle>>>,
    operation_counter_sender: watch::Sender<u32>,
    operation_counter_receiver: watch::Receiver<u32>,
    event_sender: broadcast::Sender<SessionEvent>,
}
```

### Step 5 — Implement `SessionManager` constructors

Still in `session.rs`, append:

```rust
impl SessionManager {
    /// Constructs a manager whose timeout is loaded from the platform
    /// config file (`dirs::config_dir() / "arx-runa/config.json"`).
    /// Missing file ⇒ 900 seconds.
    pub fn from_config() -> Self {
        Self::with_timeout(config::load_session_timeout())
    }

    /// Constructs a manager with an explicit timeout. Primarily for tests
    /// that need short durations (e.g. 100 ms).
    pub fn with_timeout(timeout: Duration) -> Self {
        let (operation_counter_sender, operation_counter_receiver) = watch::channel(0u32);
        let (event_sender, _) = broadcast::channel(BROADCAST_CHANNEL_CAPACITY);
        Self {
            session: Arc::new(RwLock::new(None)),
            lifecycle: Arc::new(RwLock::new(LifecycleState::NoSession)),
            timeout,
            timer: Arc::new(tokio::sync::Mutex::new(None)),
            operation_counter_sender,
            operation_counter_receiver,
            event_sender,
        }
    }

    /// Returns the current lifecycle state.
    pub async fn state(&self) -> LifecycleState {
        *self.lifecycle.read().await
    }

    /// Returns a subscriber handle for `SessionEvent`s.
    pub fn subscribe(&self) -> broadcast::Receiver<SessionEvent> {
        self.event_sender.subscribe()
    }
}
```

### Step 6 — Implement `authenticate()`

Append:

```rust
impl SessionManager {
    /// Reads the (optional) key file, runs Argon2id + HKDF via
    /// [`SessionKeys::derive`], installs the keys, transitions to
    /// `Active`, and spawns the timeout timer.
    ///
    /// Returns `SessionAlreadyActive` if called while the state is
    /// `Active`. On `KeySource` failure maps to `KeyFileNotFound`
    /// (NotFound / InvalidSize) or `InvalidCredentials` (IoFailed, with
    /// the underlying error logged server-side).
    pub async fn authenticate(
        &self,
        password_utf8_bytes: &[u8],
        key_source: Option<&(dyn KeySource + Send + Sync)>,
        salt: &[u8; 32],
        params: &Argon2Params,
    ) -> Result<(), AuthenticationError> {
        {
            let current = *self.lifecycle.read().await;
            if current == LifecycleState::Active {
                return Err(AuthenticationError::SessionAlreadyActive);
            }
        }

        let key_file_bytes = match key_source {
            Some(source) => match source.read_key() {
                Ok(bytes) => Some(bytes),
                Err(KeySourceError::NotFound) => return Err(AuthenticationError::KeyFileNotFound),
                Err(KeySourceError::InvalidSize { .. }) => {
                    return Err(AuthenticationError::KeyFileNotFound);
                }
                Err(KeySourceError::IoFailed(error)) => {
                    tracing::warn!(?error, "key source IO failed during authenticate");
                    return Err(AuthenticationError::InvalidCredentials);
                }
            },
            None => None,
        };

        let password_owned = password_utf8_bytes.to_vec();
        let salt_owned = *salt;
        let params_owned = *params;
        let key_file_owned: Option<[u8; 32]> = key_file_bytes.as_deref().map(|bytes| *bytes);

        let derived = tokio::task::spawn_blocking(move || {
            let key_file_ref = key_file_owned.as_ref();
            SessionKeys::derive(&password_owned, key_file_ref, &salt_owned, &params_owned)
        })
        .await
        .map_err(|join_error| {
            tracing::error!(?join_error, "spawn_blocking for SessionKeys::derive panicked");
            AuthenticationError::InvalidCredentials
        })??;

        // TODO(phase-3.1): open SQLCipher DB with derived.sqlcipher_key here.

        {
            let mut session_guard = self.session.write().await;
            *session_guard = Some(derived);
        }
        {
            let mut lifecycle_guard = self.lifecycle.write().await;
            *lifecycle_guard = LifecycleState::Active;
        }

        self.restart_timer().await;
        // TODO(phase-2.4/6.1): per-vault exponential backoff on InvalidCredentials
        // should be inserted in the error path above.
        Ok(())
    }
}
```

**Note on lifetime**: `password_utf8_bytes: &[u8]` is copied into a `Vec<u8>` before crossing the `spawn_blocking` boundary. The `Vec<u8>` is not zeroized — callers are expected to have already converted the sensitive `String` into `Zeroizing<Vec<u8>>` per cross-phase invariant #6. Phase 6.1 IPC handlers perform that conversion. Phase 2.3 does not wrap the inner `Vec` in `Zeroizing` to avoid double-work; this is consistent with Phase 2.2's signature.

### Step 7 — Implement `lock()`

Append:

```rust
impl SessionManager {
    /// Cancels the timer, waits for all in-flight operations to complete,
    /// acquires the session write lock, zeroizes and drops the keys, and
    /// transitions to `Expired`. Idempotent for `NoSession` and `Expired`.
    pub async fn lock(&self) {
        {
            let current = *self.lifecycle.read().await;
            if current != LifecycleState::Active {
                return;
            }
        }

        self.cancel_timer().await;

        let mut waiter = self.operation_counter_receiver.clone();
        if let Err(error) = waiter.wait_for(|count| *count == 0).await {
            tracing::error!(?error, "operation counter channel closed during lock");
        }

        // TODO(phase-3.1): close SQLCipher connection here before zeroizing.
        // TODO(phase-4): overwrite and unlink temp rclone.conf here before zeroizing.

        {
            let mut session_guard = self.session.write().await;
            *session_guard = None;
        }
        {
            let mut lifecycle_guard = self.lifecycle.write().await;
            *lifecycle_guard = LifecycleState::Expired;
        }

        let _ = self.event_sender.send(SessionEvent::Locked);
    }
}
```

### Step 8 — Implement `reset_timer()` and internal timer helpers

Append:

```rust
impl SessionManager {
    /// Cancels the existing timer task (if any) and spawns a new one
    /// with the configured duration. No-op unless state is `Active`.
    pub async fn reset_timer(&self) {
        {
            let current = *self.lifecycle.read().await;
            if current != LifecycleState::Active {
                return;
            }
        }
        self.restart_timer().await;
    }

    async fn cancel_timer(&self) {
        let mut slot = self.timer.lock().await;
        if let Some(handle) = slot.take() {
            let _ = handle.cancel.send(());
            handle.join.abort();
        }
    }

    async fn restart_timer(&self) {
        self.cancel_timer().await;

        let (cancel_tx, cancel_rx) = oneshot::channel();
        let event_sender = self.event_sender.clone();
        let session = Arc::clone(&self.session);
        let lifecycle = Arc::clone(&self.lifecycle);
        let counter = self.operation_counter_receiver.clone();
        let timeout = self.timeout;

        let join = tokio::spawn(Self::run_timer(
            timeout,
            cancel_rx,
            event_sender,
            session,
            lifecycle,
            counter,
        ));

        let mut slot = self.timer.lock().await;
        *slot = Some(TimerHandle { cancel: cancel_tx, join });
    }

    async fn run_timer(
        timeout: Duration,
        cancel: oneshot::Receiver<()>,
        event_sender: broadcast::Sender<SessionEvent>,
        session: SharedSession,
        lifecycle: Arc<RwLock<LifecycleState>>,
        mut counter: watch::Receiver<u32>,
    ) {
        let pre_warning = Duration::from_secs(PRE_WARNING_SECONDS);
        let total = timeout;

        tokio::pin!(cancel);

        if total > pre_warning {
            let before_warning = total - pre_warning;
            tokio::select! {
                _ = tokio::time::sleep(before_warning) => {}
                _ = &mut cancel => return,
            }
            let _ = event_sender.send(SessionEvent::TimeoutWarning {
                seconds_remaining: PRE_WARNING_SECONDS,
            });
            tokio::select! {
                _ = tokio::time::sleep(pre_warning) => {}
                _ = &mut cancel => return,
            }
        } else {
            tokio::select! {
                _ = tokio::time::sleep(total) => {}
                _ = &mut cancel => return,
            }
        }

        if let Err(error) = counter.wait_for(|count| *count == 0).await {
            tracing::error!(?error, "operation counter closed before timeout zeroize");
            return;
        }

        {
            let current = *lifecycle.read().await;
            if current != LifecycleState::Active {
                return;
            }
        }

        {
            let mut session_guard = session.write().await;
            *session_guard = None;
        }
        {
            let mut lifecycle_guard = lifecycle.write().await;
            *lifecycle_guard = LifecycleState::Expired;
        }

        let _ = event_sender.send(SessionEvent::Locked);
    }
}
```

### Step 9 — Implement operation gate

Append:

```rust
/// RAII guard that decrements the operation counter on drop.
/// Obtained via [`SessionManager::begin_operation`].
#[must_use = "dropping the guard decrements the operation counter"]
pub struct OperationGuard {
    sender: watch::Sender<u32>,
}

impl Drop for OperationGuard {
    fn drop(&mut self) {
        self.sender.send_modify(|count| {
            *count = count.saturating_sub(1);
        });
    }
}

impl SessionManager {
    /// Increments the operation counter and returns a guard. Drop the
    /// guard when the operation completes (including panic paths).
    pub fn begin_operation(&self) -> OperationGuard {
        self.operation_counter_sender
            .send_modify(|count| *count = count.saturating_add(1));
        OperationGuard {
            sender: self.operation_counter_sender.clone(),
        }
    }
}
```

### Step 10 — Re-export from `auth::mod`

**File**: `C:\Users\chris\source\repos\arx-runa\src-tauri\src\auth\mod.rs`

Add public re-exports (remove `#[allow(dead_code)]` attributes on `session` and `kdf`):

```rust
pub use session::{LifecycleState, OperationGuard, SessionEvent, SessionManager};
```

Also remove the `#[allow(unused_imports)]` on `pub(crate) use session::SessionKeys;` — consumers still reach `SessionKeys` through `crate::auth::session::SessionKeys` where needed.

### Step 11 — Tests

All tests under `#[cfg(test)] mod tests` in `src-tauri/src/auth/session.rs` (reuse the existing module or add a sibling `mod session_manager_tests`). Uses `#[tokio::test]`.

Test cases required by the sub-phase + additions from Step 1.75 (DC-8, DC-9, DC-10):

1. `test_session_manager_new_starts_in_no_session_state`
2. `test_authenticate_tier1_transitions_no_session_to_active`
3. `test_authenticate_tier2_transitions_no_session_to_active_with_mock_key_source`
4. `test_authenticate_rejects_when_state_is_active`
5. `test_authenticate_returns_key_file_not_found_when_key_source_reports_not_found`
6. `test_authenticate_returns_key_file_not_found_when_key_source_reports_invalid_size`
7. `test_authenticate_returns_invalid_credentials_when_key_source_io_fails`
8. `test_lock_transitions_active_to_expired`
9. `test_lock_is_idempotent_when_state_is_no_session`
10. `test_lock_is_idempotent_when_state_is_expired`
11. `test_timeout_fires_after_configured_duration_and_transitions_to_expired` — uses `with_timeout(Duration::from_millis(100))`; subscribes to `SessionEvent`; awaits `Locked` via the receiver.
12. `test_reset_timer_extends_timeout_when_called_before_deadline` — short timeout (200 ms), `reset_timer()` at 100 ms, assert session still `Active` at 250 ms.
13. `test_reset_timer_is_noop_when_state_is_no_session`
14. `test_operation_counter_delays_lock_until_guard_dropped` — `begin_operation()`, spawn `lock()` in background, sleep 50 ms, assert still `Active`, drop guard, await `lock()` completion.
15. `test_operation_counter_delays_timeout_until_guard_dropped` — same shape but timeout-triggered.
16. `test_operation_counter_drops_guard_on_panic` — spawn task that panics while holding a guard; assert counter returns to zero.
17. `test_re_authentication_from_expired_transitions_to_active`
18. `test_session_event_timeout_warning_emitted_before_expiry` — with timeout > 60s (e.g. 61s is too slow; use `with_timeout_and_warning(Duration::from_millis(200), Duration::from_millis(100))` only if the `PRE_WARNING_SECONDS` constant is adjusted for tests; otherwise use `Duration::from_secs(61)` and `#[ignore = "slow"]`). Pragmatic choice: **introduce a private `with_timeout_and_warning` constructor for tests only** that accepts a custom `pre_warning: Duration`, allowing sub-second tests.
19. `test_session_event_locked_emitted_on_manual_lock`
20. `test_session_event_locked_emitted_on_timeout_expiry`
21. `test_session_keys_buffers_are_zeroed_after_lock` — uses `crate::memory::platform::take_last_unlock_snapshot` after `lock()`; asserts the snapshot is all zeros. Verifies Phase 2.2's `SecureBytes::drop` runs through `SessionKeys::drop` chain.
22. `test_timeout_shorter_than_pre_warning_does_not_emit_warning` — `with_timeout(Duration::from_millis(50))`; subscriber receives only `Locked`, never `TimeoutWarning`.
23. `test_parse_config_bytes_*` (5 tests) — see Step 2.
24. `test_authentication_error_session_already_active_display_matches_design` — see Step 1.

Test helpers:
- Short Argon2 params: `Argon2Params { memory_cost_kib: 1024, time_cost: 1, parallelism: 1 }` (same as existing tests).
- Fixed salt `[0x44u8; 32]`.
- `MockKeySource` for Tier 2 tests; custom minimal `struct` implementing `KeySource` that returns each of the three `KeySourceError` variants for DC-10 coverage.
- Private `with_timeout_and_warning` constructor to avoid `#[ignore]`-gating the warning test.

### Step 12 — Leave hook comments for deferred integrations

Already covered in Steps 6/7 — the TODO comments mark:
- Phase 3.1: SQLCipher open in `authenticate()` post-derive.
- Phase 3.1: SQLCipher close in `lock()` pre-zeroize.
- Phase 4: rclone.conf cleanup in `lock()` pre-zeroize.
- Phase 2.4 / 6.1: exponential backoff on `InvalidCredentials`.

No runtime code is required for these in Phase 2.3.

## 6. Security implications

### a. Expected sensitive path set

Files/directories under `src-tauri/src/{crypto,auth,storage}/` this plan anticipates touching:

- `src-tauri/src/auth/session.rs` — extend with `SessionManager`, timer, operation gate, events.
- `src-tauri/src/auth/error.rs` — add `SessionAlreadyActive` variant.
- `src-tauri/src/auth/mod.rs` — re-export new symbols.
- `src-tauri/src/auth/config.rs` — new file for timeout config loader.

**No expected changes** to `src-tauri/src/crypto/**`, `src-tauri/src/storage/**` (does not exist yet), `src-tauri/src/memory/**`, `src-tauri/src/auth/kdf.rs`, `src-tauri/src/auth/key_source.rs`, `src-tauri/src/auth/device_monitor/**`, or `src-tauri/src/auth/autodetect.rs`. Any unanticipated touch under these paths during `/implement-plan` must be flagged as a Plan Deviation.

### b. Invoke security-reviewer agent? **YES**

Matches the sub-phase roadmap's Security Review Checkpoints ("Phase 2.3: Required — Invoke `security-reviewer` agent after implementation"). Independently confirmed — the module adds a new zeroization sequencing surface and a cancellation/ordering surface around key lifetime.

### c. What the reviewer should check

- **Zeroize sequencing**: confirm that `lock()` and the timer task both: (1) wait for `operation_counter == 0` *before* acquiring the session write lock, (2) drop the `Option<SessionKeys>` which must drop through `SessionKeys::Drop` → `SecureBytes::Drop` → `Zeroize::zeroize` → `munlock`/`VirtualUnlock` → free. No intermediate copies of the key bytes may escape the scope.
- **Timer cancellation races**: confirm `cancel_timer()` correctly aborts pending timer tasks and that a `reset_timer()` call during a running timer cannot leave two concurrent tasks capable of zeroing the session. The `oneshot` cancel signal + `JoinHandle::abort()` combination must be race-free.
- **Operation gate correctness**: confirm `OperationGuard::Drop` is panic-safe — a panicking operation must still decrement the counter. Confirm `saturating_sub` prevents underflow if the guard is constructed without a matching `begin_operation()` (shouldn't happen but belt-and-braces).
- **Event broadcast leakage**: confirm no `SessionEvent` variant carries secret material. Current shape (`TimeoutWarning { seconds_remaining }`, `Locked`) is clean — verify no future additions leak via this channel.
- **Authenticate error mapping**: confirm `KeySourceError::IoFailed` is never surfaced into `AuthenticationError::KeySource(...)` from inside `authenticate()` — it must collapse to `InvalidCredentials` with the underlying error logged server-side only (DC-10 resolution).
- **State-machine atomicity**: confirm no observable "half-authenticated" state — between installing keys and transitioning to `Active`, a concurrent `lock()` or `state()` reader must see consistent data. Review for lock ordering / TOCTOU.
- **Double-lock safety**: confirm calling `lock()` from two tasks concurrently does not double-zeroize or fire two `Locked` events.

## 7. Execution and testing strategy

### Test types

- [x] **Unit tests** — state-machine transitions, config parser, error mapping.
- [x] **Async tests** — `#[tokio::test]` for timer, `reset_timer`, operation gate, broadcast channel.
- [x] **Mock-based tests** — `MockKeySource` + custom in-file failing `KeySource` impls for each variant.
- [x] **Zeroization verification** — uses existing `memory::platform::take_last_unlock_snapshot` test hook.
- [ ] **Property-based tests** — none needed for 2.3; state space is small and deterministic.
- [ ] **Integration tests** — deferred to Phase 2.4 (full auth → operate → timeout round-trip).
- [ ] **Benchmarks** — none.

### Invoke test-writer agent? **YES**

**Rationale**: The test matrix spans 24 test cases covering async timing, cancellation races, panic paths, and broadcast channel semantics. Several tests (operation gate panic, timeout vs reset race, zeroization snapshot) benefit from the test-writer agent's adversarial perspective. Timing-sensitive async tests are a known flake vector — the test-writer agent should review for deterministic timing and proper `tokio::time::advance` / `tokio::time::pause` usage where applicable.

### Validation checkpoint (copied from sub-phase doc lines 30–50)

Automated tests:
```
cargo test auth::session
cargo test auth::config
cargo clippy -- -D warnings
```

Manual verification (recorded in report log, not automated):
- Set timeout to 5 seconds in local config; authenticate; wait; confirm `state()` returns `Expired`.
- Subscribe to session events; confirm the 60-second pre-warning (use `with_timeout_and_warning(Duration::from_secs(3), Duration::from_secs(2))` for test convenience) reaches the subscriber.

Security checks:
- After `lock()`, call `take_last_unlock_snapshot()` and assert `== vec![0u8; 32]`.
- Confirm the session-lived temp `rclone.conf` check is marked TODO for Phase 4 (not executed in 2.3).

Acceptance criteria:
- State machine rejects key access (via `state() != Active`) after `Expired`.
- Timer reset correctly extends the timeout on each `reset_timer()` call.
- Operation-in-progress gate prevents partial-operation zeroing.
- Key buffers are demonstrably zeroed (via unlock snapshot) after `lock()`.

### Additional edge-case tests from Step 1.75 review

- Test 4 (DC-9): re-auth rejection while `Active`.
- Tests 5–7 (DC-10): oracle-free `KeySource` error mapping.
- Test 16: `OperationGuard` panic-drop safety.
- Test 22: sub-pre-warning timeout skips the warning phase.
- Tests 23: config parser edge cases.

## 8. Documentation impact

Files to update **after implementation completes** (deviation-driven — required):

- `docs/architecture/designs/authentication-and-session-management/sub-phases/2.3-session-lifecycle-and-timeout.md`:
  - Deliverable 2 (line 13) — update per DC-1, DC-3.
  - Deliverable 3 (line 14) — update per DC-2.
  - Deliverable 4 (line 15) and Implementation Notes (line 68) — update per DC-8.
  - Deliverable 7 (line 18) — update per DC-5.
  - Notes (new line) — cross-reference DC-11 (backoff deferral).
- `docs/architecture/designs/authentication-and-session-management/design.md`:
  - Lines 199–210 (Session ownership and sharing) — add lifecycle enum note per DC-4.
  - Lines 225–270 (Timeout mechanism) — add two-guard clarification per DC-7 and config schema per DC-8.
  - Lines 276–296 (`AuthenticationError`) — add `SessionAlreadyActive` variant per DC-9.
- `.claude/rules/auth.md`:
  - Session section — extend with `SessionManager` lifecycle, state enum, operation gate, timer cadence, event channel. Error list — append `SessionAlreadyActive`. This is Governance Sync Action GS-1 (see Section 9).
- `.github/instructions/auth.instructions.md`:
  - Regenerated automatically by `/copilot-sync` after GS-1.

No new diagrams required for Phase 2.3 — the state machine is trivial enough to describe in prose. A Mermaid state diagram *may* be added under `docs/architecture/designs/authentication-and-session-management/diagrams/` as a follow-up but is not in scope.

Report log: capture a short entry at `docs/report-log/` summarising the zeroization verification approach for the security review checkpoint.

## 9. Governance sync actions (pre-implementation)

### GS-1 — Extend `.claude/rules/auth.md` with session-manager lifecycle guidance

- **Action ID**: GS-1
- **Reason / linked concern**: DC-12.
- **Target files**:
  - `C:\Users\chris\source\repos\arx-runa\.claude\rules\auth.md`
  - `C:\Users\chris\source\repos\arx-runa\.github\instructions\auth.instructions.md` (regenerated)
- **Required edit**: Under the existing `## Session` heading in `.claude/rules/auth.md`, append the following bullets verbatim (do not replace existing content — append):
  ```
  - `SessionManager` (src-tauri/src/auth/session.rs) owns the `NoSession → Active → Expired` state machine; check `state().await` before any session-scoped work.
  - `SessionManager::authenticate(password, key_source, salt, params)` is the only entry to `Active`; re-auth while `Active` returns `SessionAlreadyActive` — call `lock()` first.
  - `reset_timer()` must be called by the IPC dispatcher on every Tauri command invocation while the session is `Active` (Phase 6.1 wires this).
  - Long-running operations must bracket their work with `let _guard = session_manager.begin_operation();` — `lock()` and the timeout task wait for the counter to reach zero before zeroizing.
  - The session timeout is loaded from `dirs::config_dir() / "arx-runa/config.json"` (schema `{ "schema_version": 1, "session_timeout_secs": u64 }`); default 900 s; clamp to `[60, 86400]`.
  - Session events are broadcast on an internal `tokio::sync::broadcast::Sender<SessionEvent>` (`TimeoutWarning { seconds_remaining }` 60 s before expiry, `Locked` after zeroize). Never add secret material to this enum.
  ```
  Under the existing `## Errors` heading, append `SessionAlreadyActive` to the "Other variants" bullet:
  ```
  - Other variants: `MemoryLockFailed`, `VaultHeaderInvalid`, `InvalidRecoveryPhrase`, `NoRecoverySlot`, `SessionAlreadyActive`
  ```
- **Verification**:
  1. Re-read `.claude/rules/auth.md` and confirm the new bullets appear under `## Session`.
  2. Run `/copilot-sync` to regenerate `.github/instructions/auth.instructions.md`.
  3. Re-read `.github/instructions/auth.instructions.md` and confirm the new bullets appear in sync with the source rule.
- **Note**: Run `/copilot-sync` after the `.claude/rules/auth.md` edit.

No other governance files require pre-implementation changes. Post-implementation doc updates (Section 8) are the design-document sync, run after code lands.

## 10. Handoff Notes for Implementer

Working directory: `C:\Users\chris\source\repos\arx-runa`. The implementer is expected to be a zero-context agent (Copilot Codex). This plan is self-contained — every trait signature, error variant, field declaration, and DDL-equivalent shape is inlined above. The implementer should not need to re-read the sub-phase, though the sub-phase (`docs/architecture/designs/authentication-and-session-management/sub-phases/2.3-session-lifecycle-and-timeout.md`) is the final authoritative reference if any inline snippet appears ambiguous.

Order of operations: (1) run GS-1 governance sync + `/copilot-sync`, (2) Step 1 (error variant), (3) Step 2+3 (config module), (4) Steps 4–10 (session module additions), (5) Step 11 (tests), (6) `cargo test auth::session && cargo test auth::config && cargo clippy -- -D warnings`, (7) invoke `security-reviewer` agent with the touched files, (8) invoke `test-writer` agent for adversarial coverage review, (9) apply deferred documentation updates from Section 8.

Traps to watch:
- Timer timing tests are flake-prone. Use short but realistic durations (≥ 50 ms); prefer the dedicated `with_timeout_and_warning` test constructor over hand-rolling `tokio::time::pause`.
- `tokio::sync::watch::Receiver::wait_for` uses `tokio::sync::watch::error::RecvError` — import path matters.
- `broadcast::Sender::send` returns `Result<usize, SendError<T>>` — ignore via `let _` (no subscribers is not an error).
- `spawn_blocking` for Argon2id requires `Send` bounds on everything crossing the boundary — that's why `password_utf8_bytes` is copied to `Vec<u8>`.
- `dirs::config_dir()` can return `None` on minimal containers — the config loader must tolerate this by returning the default, already handled in the inline snippet.
- Do NOT touch `src-tauri/src/crypto/**`, `src-tauri/src/memory/**` (Phase 2.2 territory), or `src-tauri/src/auth/device_monitor/**` (Phase 2.1 territory). Any touch outside Section 6a's expected path set is a Plan Deviation.
- Do NOT add exponential backoff, vault-header loading, SQLCipher open, or rclone.conf cleanup — all four are deferred (see DCs 1, 2, 3, 11).
- Platform parity: the plan is platform-agnostic; `dirs`, `tokio`, and `zeroize` behave identically across Windows/macOS/Linux. Run `cargo test` on at least one platform; CI covers the rest.
