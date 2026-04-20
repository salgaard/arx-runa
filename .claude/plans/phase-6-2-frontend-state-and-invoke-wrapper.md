---
title: "Phase 6.2 — Frontend State Contexts and Tauri Invoke Wrapper"
created: "2026-04-20T18:00:00Z"
status: approved
roadmap-phase: 6
sub-phase: "6.2"
design-document: docs/architecture/designs/tauri-ipc-and-frontend/design.md
sub-phase-roadmap: docs/architecture/designs/tauri-ipc-and-frontend/sub-phases/roadmap.md
governance-sync-required: true
tags: [leptos, tauri, frontend, state-contexts, invoke-wrapper]
---

## 1. Goal

Stand up the Leptos-side IPC bridge and in-memory state contexts so Phase 6.3 pages can invoke backend commands through a typed `invoke_command<A, R>` helper and consume `SessionState`/`VaultState`/`SyncState` via provider/hook pairs — with explicit lock-time clear semantics that satisfy Zero-Trace.

## 2. Context

- Sub-phase budget: ~250 production LoC + ~50 test LoC; second of four sub-phases (6.1 → 6.2 → 6.3 → 6.4). Strict dependency on 6.1 for backend IPC DTO shapes (`SessionStatus`, `FileEntry`, `IpcError { kind, message }` JSON shape).
- 6.1 landed (`status: implemented` in `.claude/plans/phase-6-1-ipc-core-and-error-sanitisation.md`). Concrete backend surface: `IpcError` serde shape `{"kind": "<camelCase>", "message": "..."}`, full 30-command `generate_handler!` registration, `SessionStatus { is_unlocked, vault_id, timeout_seconds }`, `FileEntry { id, name, entry_type, size_bytes, modified_at, parent_id }`.
- Current frontend state (`src/`): only `main.rs` (mounts `App`) + `app.rs` (static "Hello Arx Runa" view). No `lib.rs`, no `state/` module, no IPC wrapper, no dependency on `serde`/`wasm-bindgen` in top-level `Cargo.toml`.
- Frontend `Cargo.toml` dependencies present: `leptos = "0.8"`, `leptos_meta`, `leptos_router`, `console_error_panic_hook`, `console_log`, `log`, `serde-wasm-bindgen = "0.6"`, `gloo-timers = "0.3"`. Missing direct deps required for this sub-phase: `serde` (with `derive`) and `wasm-bindgen`.
- `tauri.conf.json` already has `withGlobalTauri: true` → the `window.__TAURI__.core.invoke` extern path is available. CSP is still `null` (Phase 6.4 concern; not in scope here).
- Cross-phase invariants touched: #7 Zero-Trace (frontend sensitive state must be cleared on lock — `VaultActions::clear()` is the enforcement point).
- Rule anchors: `.claude/rules/leptos.md` (signals, Zero-Trace, reactive props), `.claude/rules/rust.md` (one-concern-per-file, `///` docs, newtypes, testing naming), `.claude/rules/tauri.md` (frontend IPC via `invoke()`, no `localStorage`).
- Security review: sub-phase explicitly marks "Not required" — frontend state, no crypto, no key material. Verified independently: `src/state/*` holds `String`/`u64`/`bool` (no bytes), no access to `src-tauri/src/{crypto,auth,storage}`, and the `clear()` enforcement satisfies Zero-Trace at the UI surface.
- Validation gate: `trunk build` must succeed; unit tests for `VaultState::clear()` reset and `SessionState` transition paths must pass.

## 3. Design Concerns / Open Questions

| # | Concern | Source | Impact | Classification | Resolution | Documentation updates |
|---|---|---|---|---|---|---|
| 1 | Design `wasm_bindgen` extern at `design.md` line 1440 declares `async fn invoke(cmd: &str, args: JsValue) -> JsValue;`. This signature cannot distinguish Tauri promise resolution from rejection — the backend's sanitised `IpcError` arrives on the rejected branch, which would surface as a `JsValue` panic in `async` bindings, not a deserialisable error. | `docs/architecture/designs/tauri-ipc-and-frontend/design.md:1437-1441` | Implementer cannot map rejection → `IpcError`; every backend error would look like a generic serialisation failure. | Non-blocking | Declare the extern with `#[wasm_bindgen(catch)]`: `async fn invoke(cmd: &str, args: JsValue) -> Result<JsValue, JsValue>;`. On `Err(js_err)` branch, `serde_wasm_bindgen::from_value::<IpcError>(js_err)` yields the sanitised error; on parse failure fall back to `IpcError { kind: "internalError", message: "Unknown error".into() }`. | None required — design block is "Illustrative" per §How to Read This Design; reality matches the implementation here. |
| 2 | Sub-phase deliverable 1 signature reads `invoke_command<A, R>(cmd: &str, args: A)` (owned `A`); design §Tauri IPC Integration example uses `args: &A` and every call site (`invoke::<_, SessionStatus>("get_session_status", &()).await`) passes a reference. | `6.2-frontend-state-and-invoke-wrapper.md:12` vs. `design.md:1444`, `design.md:1055` | If taken literally the helper would consume request payloads, breaking the reference-borrowing call pattern the design demonstrates. | Non-blocking | Adopt `args: &A` — matches the design's call-site ergonomics and `serde_wasm_bindgen::to_value(&A)` signature. Document in assumption 1. | None. |
| 3 | Sub-phase deliverable 1+2 places both `invoke_command` and the frontend `IpcError` struct in `src/lib.rs`. `.claude/rules/rust.md` mandates "one concern per file" and `mod.rs`-style re-exports-only roots. Single-file placement would collide with project structure rules. | `6.2-frontend-state-and-invoke-wrapper.md:12-13` vs. `.claude/rules/rust.md` §Structure | Either violate the Rust rule or deviate from the literal sub-phase path. | Non-blocking | Create `src/lib.rs` as a re-exports-only root and place bodies in focused files: `src/invoke.rs` (extern + `invoke_command`), `src/error.rs` (frontend `IpcError`), `src/ipc_types/` (mirror DTOs). `src/lib.rs` re-exports all public types so `use arx_runa::{invoke_command, IpcError, SessionState, …}` resolves as the sub-phase intends. | None — internal structure choice. |
| 4 | Frontend-side `Cargo.toml` lacks direct deps on `serde` and `wasm-bindgen`; only transitive availability via Leptos. Deriving `Deserialize` on `IpcError` and `SessionStatus`, plus the `#[wasm_bindgen(catch)]` extern block, require direct declarations. | `Cargo.toml:15-23` | Plan as-written would not compile without new dependencies. | Non-blocking | Add to `[dependencies]`: `serde = { version = "1", features = ["derive"] }` and `wasm-bindgen = "0.2"`. Keep the version pins in sync with the transitive versions already compiled in `Cargo.lock` to avoid duplicate crates. | None. |
| 5 | The `SessionState` struct in the sub-phase adds `authenticating: bool` and `error: Option<String>` beyond `SessionStatus`. Polling only updates the three `SessionStatus` fields — `authenticating` and `error` are set exclusively by `LoginPage` (Phase 6.3). No conflict, but the polling loop must **not** clobber those fields. | `6.2-frontend-state-and-invoke-wrapper.md:15` vs. `design.md:1042-1065` | Incorrect `set_state.set(SessionState { … })` replacement would reset UI feedback on every 5-second tick. | Non-blocking | Polling loop updates only the three status fields via `set_state.update(|s| { s.is_unlocked = …; s.vault_id = …; s.timeout_seconds = …; })` — matches `design.md:1056-1060`. `authenticating` and `error` remain under Phase 6.3 control. | None. |
| 6 | Sub-phase deliverable 9 enumerates tests that must run, but Leptos signals (`signal()`, `WriteSignal::update`) require a reactive runtime that only exists inside `wasm-bindgen-test` or `leptos::mount` contexts — plain `cargo test` cannot drive them. The sub-phase's Validation Checkpoint lists `trunk build` as automated and leaves unit tests to "`wasm-pack test` or equivalent". No wasm test harness is currently configured. | `6.2-frontend-state-and-invoke-wrapper.md:20, 26-33` | Signal-driven tests would either not run or require a full `wasm-bindgen-test` scaffold (out of the 50-LoC budget). | Non-blocking | Split the logic: give `SessionState` a pure `apply_status(&mut self, status: SessionStatus)` method (polling-update), `begin_authenticating(&mut self)`, `complete_success(&mut self, vault_id: String)`, `complete_failure(&mut self, message: String)` transitions; give `VaultState` a pure `clear(&mut self)` method. `VaultActions`/`SessionProvider` delegate to these methods inside `update(|s| …)`. Unit tests exercise the pure methods under regular `cargo test` (host target) — no wasm harness needed. | None. |
| 7 | Sub-phase deliverable 3 says "registers a stop signal on `on_cleanup` to prevent stale intervals after unmount"; Implementation Notes §2 specifies `Rc<Cell<bool>>`. Design `design.md:1044-1068` shows a different pattern using a `ReadSignal<bool>/WriteSignal<bool>` pair. Both work, but mixing patterns creates ambiguity. | `6.2-frontend-state-and-invoke-wrapper.md:14, 52` vs. `design.md:1044-1068` | Implementer might pick either; minor code-shape drift. | Non-blocking | Follow the sub-phase's explicit guidance: `Rc<Cell<bool>>` shared between the polling `async move` block and the `on_cleanup` closure. Reason: signals are cheaper semantically, but `Cell<bool>` keeps the stop flag out of the reactive graph (it is not UI-observable state). | None. |
| 8 | `FileEntry` field `entry_type: String` is a free-form `"file" | "directory"` discriminator on the backend. The frontend currently has no enum mirror; consumers would string-compare. Sub-phase Implementation Notes §4 explicitly says "keep the two in sync manually until a shared type generation step is introduced". | `src-tauri/src/ui/types/file_entry.rs:14` + `6.2-frontend-state-and-invoke-wrapper.md:54` | String comparisons in 6.3 without a centralised helper invite typos and drift. | Non-blocking | Mirror `FileEntry` verbatim in `src/ipc_types/file_entry.rs` with `entry_type: String`; add a `FileEntry::is_directory(&self) -> bool` helper (`self.entry_type == "directory"`) so Phase 6.3 pages do not spread the string comparison. | None. |
| 9 | Sub-phase deliverable 8 names `SyncContext` and a `SyncStatus` signal; `SyncStatus` is already the backend IPC DTO name (`src-tauri/src/ui/types/sync_status.rs`). Re-using the same name for the frontend signal state causes import collisions. | `6.2-frontend-state-and-invoke-wrapper.md:19` vs. `src-tauri/src/ui/types/sync_status.rs` | Name shadowing in Phase 6.3 imports. | Non-blocking | Backend wire type is mirrored into `src/ipc_types/sync_status.rs` as `SyncStatus` (Deserialize). Frontend state struct is named `SyncState` and lives in `src/state/sync_context.rs`. The polling/sync hook converts `SyncStatus` → `SyncState` updates. | None. |
| 10 | `VaultActions::navigate` issues `list_directory` which requires `path: String` on the IPC boundary. The backend `ui::file_commands::list_directory(path: String, …)` with Tauri serde deserialises from `{"path": "..."}` by default (Tauri v2 camelCases param names). Passing a raw `&String` via `invoke_command` without wrapping in a request struct produces the wrong JSON shape. | `design.md:1120` (shows `invoke(&path)` — illustrative) vs. Tauri v2 command argument convention | Runtime `list_directory` call would reject with `InvalidInput`. | Non-blocking | Define per-command request structs in `src/ipc_types/requests.rs` — for 6.2 scope only `ListDirectoryRequest { path: String }` is needed. `VaultActions::navigate(path)` serialises `&ListDirectoryRequest { path }`. Pattern extends in 6.3 for other commands. | Note in §4 Assumption 5 and §7 (Phase 6.3 will extend this file). |
| 11 | `use_session()` and `use_session_actions()` hook panic strings. Sub-phase deliverable 5 says "panicking with a clear message if called outside a `SessionProvider`". `.expect("SessionProvider must wrap the app")` (design.md:1076) is suitable but should match across both hooks. | `design.md:1076, 1082` | Inconsistent panic strings make debug reports noisier. | Non-blocking | Both hooks use `"SessionProvider must wrap the component tree — did you forget to mount it in src/app.rs?"`. Apply the same convention to `use_vault`/`use_vault_actions` and `use_sync`. | None. |
| 12 | `src/app.rs` currently renders only a "Hello Arx Runa" stub. If the three providers do not wrap `App`'s body, none of the hooks are usable by Phase 6.3. | `src/app.rs:1-11` | 6.3 pages would panic on mount. | Non-blocking | In Phase 6.2, wrap the current static body inside `SessionProvider > VaultProvider > SyncProvider` (no pages yet; keep the stub heading so `trunk build` still renders something). Providers are idempotent scaffolding; 6.3 replaces the stub with the router. | None. |

**Summary:** 12 non-blocking concerns, 0 blocking. Proceed.

## 4. Assumptions

1. `invoke_command<A, R>(cmd: &str, args: &A) -> Result<R, IpcError>` takes a borrowed `A` (resolves Concern 2). Callers pass `&()` for no-arg commands, or `&MyRequest { … }` otherwise.
2. Frontend `IpcError` is deserialised both from Tauri success payloads that already contain `{"kind", "message"}` shapes **and** from the rejected-promise `JsValue` branch. Unknown/parse-failure falls back to `IpcError { kind: "internalError".into(), message: "Unknown error".into() }`.
3. `src/lib.rs` is created anew as a pure re-export root; `Cargo.toml` gains a `[lib]` entry (`name = "arx_runa"`, `path = "src/lib.rs"`, `crate-type = ["rlib"]`) alongside the existing `[[bin]]` so `main.rs` can `use arx_runa::App`.
4. `src/main.rs` is updated to `use arx_runa::app::App;` (or equivalent re-export) rather than `mod app;` — needed because `app` now lives in the library crate for re-use by tests.
5. For 6.2 only one request struct is needed on the wire: `ListDirectoryRequest { path: String }` in `src/ipc_types/requests.rs`. Other commands' request structs are deferred to 6.3.
6. `SessionProvider` polls `get_session_status` every 5 seconds via `gloo_timers::future::TimeoutFuture::new(5_000).await` inside `spawn_local`; the stop flag is `Rc<Cell<bool>>` captured by both the polling body and the `on_cleanup` closure (resolves Concern 7).
7. Signal APIs follow Leptos 0.8 conventions already compiled in: `leptos::prelude::*`, `signal()` returns `(ReadSignal<T>, WriteSignal<T>)`, `provide_context(T)` / `use_context::<T>() -> Option<T>`, `spawn_local` via `leptos::task::spawn_local`, `on_cleanup(FnOnce())`.
8. `VaultActions` and `SessionActions` are plain `#[derive(Clone, Copy)]` structs holding the `WriteSignal<…>`. `WriteSignal` is already `Copy` in Leptos 0.8, so the wrapper is trivially copyable and can be cloned into async closures without boxing.
9. Test strategy: signal-free pure methods (`SessionState::apply_status`, `begin_authenticating`, `complete_success`, `complete_failure`; `VaultState::clear`) live on the plain structs; tests exercise these methods only. Acceptance test for `IpcError` kind/message deserialisation uses `serde_json::from_str` (works on host target). This avoids pulling in `wasm-bindgen-test` for 6.2 (resolves Concern 6).
10. Dependencies to add in the top-level `Cargo.toml`: `serde = { version = "1", features = ["derive"] }`, `wasm-bindgen = "0.2"`. A dev-dep on `serde_json = "1"` is added for host-target deserialisation tests.
11. Frontend `IpcError` implements `Debug + Clone + Deserialize`; it does **not** implement `std::error::Error` (keeps the type minimal; Leptos error boundaries accept anything `Into<Cow<str>>`-ish in 0.8 through the `message` field).
12. `use_sync()` hook is provided for parity with `use_session()` and `use_vault()` even though the sub-phase deliverable 8 only mentions a context. A hook is a one-liner and the cost of omitting it is a breaking change in 6.3.
13. No frontend polling for `get_sync_status` in 6.2 — sync polling ownership sits in 6.3's upload/download UI. The `SyncState` context in 6.2 is initialised to `SyncState::default()` only.

## 5. Approach

### `CONTRACT_SNIPPETS`

**CS-001 — Frontend `IpcError`** (`src/error.rs`)

```rust
use serde::Deserialize;

/// Frontend representation of backend-sanitised IPC errors.
///
/// Deserialised from the JSON shape `{"kind": "<camelCase>", "message": "..."}`
/// produced by `src-tauri/src/ui/error.rs::IpcError`.
#[derive(Debug, Clone, Deserialize)]
pub struct IpcError {
    /// Machine-readable discriminator: `"vaultLocked"`, `"authenticationFailed"`,
    /// `"notFound"`, `"alreadyExists"`, `"cloudError"`, `"invalidInput"`,
    /// `"internalError"`.
    pub kind: String,
    /// User-safe, displayable message (no paths, keys, or internals).
    pub message: String,
}

impl IpcError {
    /// Build a synthetic error for client-side serialisation / parse failures
    /// where no backend error was produced.
    pub(crate) fn internal(message: impl Into<String>) -> Self {
        Self { kind: "internalError".into(), message: message.into() }
    }
}
```

**CS-002 — Invoke extern + typed wrapper** (`src/invoke.rs`)

```rust
use serde::{Serialize, de::DeserializeOwned};
use wasm_bindgen::prelude::*;

use crate::error::IpcError;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "core"], catch)]
    async fn invoke(cmd: &str, args: JsValue) -> Result<JsValue, JsValue>;
}

/// Type-safe wrapper around `window.__TAURI__.core.invoke`.
///
/// Serialises `args` via `serde_wasm_bindgen`, invokes the Tauri command,
/// and deserialises either the success payload into `R` or the rejected
/// `IpcError` JSON payload.
pub async fn invoke_command<A, R>(cmd: &str, args: &A) -> Result<R, IpcError>
where
    A: Serialize,
    R: DeserializeOwned,
{
    let args_js = serde_wasm_bindgen::to_value(args)
        .map_err(|_| IpcError::internal("Failed to serialise command arguments"))?;

    match invoke(cmd, args_js).await {
        Ok(result_js) => serde_wasm_bindgen::from_value(result_js)
            .map_err(|_| IpcError::internal("Failed to deserialise command response")),
        Err(error_js) => Err(serde_wasm_bindgen::from_value::<IpcError>(error_js)
            .unwrap_or_else(|_| IpcError::internal("Unknown error"))),
    }
}
```

**CS-003 — `SessionStatus` mirror** (`src/ipc_types/session_status.rs`)

```rust
use serde::Deserialize;

/// Mirror of `src-tauri/src/ui/types/session_status.rs::SessionStatus`.
/// Kept in sync manually until shared type generation is introduced.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionStatus {
    pub is_unlocked: bool,
    pub vault_id: Option<String>,
    pub timeout_seconds: Option<u64>,
}
```

**CS-004 — `FileEntry` mirror** (`src/ipc_types/file_entry.rs`)

```rust
use serde::Deserialize;

/// Mirror of `src-tauri/src/ui/types/file_entry.rs::FileEntry`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileEntry {
    pub id: String,
    pub name: String,
    pub entry_type: String,
    pub size_bytes: u64,
    pub modified_at: String,
    pub parent_id: Option<String>,
}

impl FileEntry {
    /// Whether this entry is a directory (versus a file).
    pub fn is_directory(&self) -> bool {
        self.entry_type == "directory"
    }
}
```

**CS-005 — `ListDirectoryRequest`** (`src/ipc_types/requests.rs`)

```rust
use serde::Serialize;

/// Argument payload for the `list_directory` Tauri command.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListDirectoryRequest {
    pub path: String,
}
```

**CS-006 — `SessionState` + transitions** (`src/state/session_context.rs`)

```rust
use leptos::prelude::*;
use std::cell::Cell;
use std::rc::Rc;

use crate::invoke::invoke_command;
use crate::ipc_types::SessionStatus;

/// Authentication and session state held in Leptos signals (RAM only).
#[derive(Clone, Debug, Default)]
pub struct SessionState {
    pub is_unlocked: bool,
    pub vault_id: Option<String>,
    pub timeout_seconds: Option<u64>,
    pub authenticating: bool,
    pub error: Option<String>,
}

impl SessionState {
    /// Applies a polled `SessionStatus` without disturbing UI-owned fields.
    pub fn apply_status(&mut self, status: SessionStatus) {
        self.is_unlocked = status.is_unlocked;
        self.vault_id = status.vault_id;
        self.timeout_seconds = status.timeout_seconds;
    }

    /// Marks the beginning of an `authenticate` IPC round trip.
    pub fn begin_authenticating(&mut self) {
        self.authenticating = true;
        self.error = None;
    }

    /// Marks a successful unlock; sets `is_unlocked = true` and stores `vault_id`.
    pub fn complete_success(&mut self, vault_id: String) {
        self.authenticating = false;
        self.is_unlocked = true;
        self.vault_id = Some(vault_id);
        self.error = None;
    }

    /// Marks a failed unlock; records the sanitised error message.
    pub fn complete_failure(&mut self, message: String) {
        self.authenticating = false;
        self.is_unlocked = false;
        self.error = Some(message);
    }
}

/// Accessor for `SessionState`. Panics if no `SessionProvider` is mounted.
pub fn use_session() -> ReadSignal<SessionState> {
    use_context::<ReadSignal<SessionState>>()
        .expect("SessionProvider must wrap the component tree — did you forget to mount it in src/app.rs?")
}

/// Accessor for `SessionState` write side. Panics if no `SessionProvider` is mounted.
pub fn use_session_actions() -> WriteSignal<SessionState> {
    use_context::<WriteSignal<SessionState>>()
        .expect("SessionProvider must wrap the component tree — did you forget to mount it in src/app.rs?")
}

/// Provides `ReadSignal<SessionState>` + `WriteSignal<SessionState>` to descendants
/// and polls `get_session_status` every 5 seconds until unmount.
#[component]
pub fn SessionProvider(children: Children) -> impl IntoView {
    let (state, set_state) = signal(SessionState::default());
    provide_context(state);
    provide_context(set_state);

    let stop = Rc::new(Cell::new(false));
    let stop_poll = Rc::clone(&stop);
    leptos::task::spawn_local(async move {
        while !stop_poll.get() {
            if let Ok(status) = invoke_command::<(), SessionStatus>("get_session_status", &()).await {
                set_state.update(|s| s.apply_status(status));
            }
            gloo_timers::future::TimeoutFuture::new(5_000).await;
        }
    });

    on_cleanup(move || stop.set(true));

    children()
}
```

**CS-007 — `VaultState` + `VaultActions`** (`src/state/vault_context.rs`)

```rust
use leptos::prelude::*;

use crate::invoke::invoke_command;
use crate::ipc_types::{FileEntry, ListDirectoryRequest};

/// Current vault-browser state held in Leptos signals.
#[derive(Clone, Debug, Default)]
pub struct VaultState {
    pub current_path: String,
    pub files: Vec<FileEntry>,
    pub loading: bool,
    pub error: Option<String>,
    pub selected: Vec<String>,
}

impl VaultState {
    /// Wipes all fields to their defaults. Called on lock to satisfy Zero-Trace.
    pub fn clear(&mut self) {
        self.current_path.clear();
        self.files.clear();
        self.loading = false;
        self.error = None;
        self.selected.clear();
    }
}

/// Write-side handle to `VaultState` exposing intent-level operations.
#[derive(Clone, Copy)]
pub struct VaultActions {
    set_state: WriteSignal<VaultState>,
}

impl VaultActions {
    /// Navigates to `path` by invoking `list_directory` and replacing `files`.
    pub fn navigate(self, path: String) {
        let set_state = self.set_state;
        leptos::task::spawn_local(async move {
            set_state.update(|s| { s.loading = true; s.error = None; });
            match invoke_command::<ListDirectoryRequest, Vec<FileEntry>>(
                "list_directory",
                &ListDirectoryRequest { path: path.clone() },
            ).await {
                Ok(files) => set_state.update(|s| {
                    s.current_path = path;
                    s.files = files;
                    s.loading = false;
                }),
                Err(err) => set_state.update(|s| {
                    s.loading = false;
                    s.error = Some(err.message);
                }),
            }
        });
    }

    /// Clears all vault state fields to defaults (used on session lock).
    pub fn clear(self) {
        self.set_state.update(|s| s.clear());
    }
}

/// Accessor for `VaultState` read side.
pub fn use_vault() -> ReadSignal<VaultState> {
    use_context::<ReadSignal<VaultState>>()
        .expect("VaultProvider must wrap the component tree")
}

/// Accessor for `VaultActions`.
pub fn use_vault_actions() -> VaultActions {
    use_context::<VaultActions>()
        .expect("VaultProvider must wrap the component tree")
}

/// Provides `ReadSignal<VaultState>` and `VaultActions` to descendants.
#[component]
pub fn VaultProvider(children: Children) -> impl IntoView {
    let (state, set_state) = signal(VaultState::default());
    provide_context(state);
    provide_context(VaultActions { set_state });
    children()
}
```

**CS-008 — `SyncState`** (`src/state/sync_context.rs`)

```rust
use leptos::prelude::*;

/// Frontend-side sync status. Distinct from the wire DTO `ipc_types::SyncStatus`.
#[derive(Clone, Debug, Default)]
pub struct SyncState {
    pub syncing: bool,
    pub last_synced_at: Option<String>,
    pub pending_changes: u32,
    pub conflict: Option<String>,
    pub error: Option<String>,
}

/// Accessor for `SyncState` read side.
pub fn use_sync() -> ReadSignal<SyncState> {
    use_context::<ReadSignal<SyncState>>()
        .expect("SyncProvider must wrap the component tree")
}

/// Accessor for `SyncState` write side.
pub fn use_sync_actions() -> WriteSignal<SyncState> {
    use_context::<WriteSignal<SyncState>>()
        .expect("SyncProvider must wrap the component tree")
}

/// Provides `ReadSignal<SyncState>` + `WriteSignal<SyncState>` to descendants.
#[component]
pub fn SyncProvider(children: Children) -> impl IntoView {
    let (state, set_state) = signal(SyncState::default());
    provide_context(state);
    provide_context(set_state);
    children()
}
```

**CS-009 — Library root** (`src/lib.rs`)

```rust
//! Arx Runa Leptos frontend — library crate.
//!
//! Re-exports the IPC wrapper, error type, mirrored DTOs, and state providers
//! used by the binary at `src/main.rs` and by Phase 6.3 page components.

pub mod app;
pub mod error;
pub mod invoke;
pub mod ipc_types;
pub mod state;

pub use app::App;
pub use error::IpcError;
pub use invoke::invoke_command;
```

### Implementation Steps

All paths absolute.

1. **`C:\Users\chris\source\repos\arx-runa\Cargo.toml`** — dependency + library target update.
   - Under `[dependencies]`, append: `serde = { version = "1", features = ["derive"] }` and `wasm-bindgen = "0.2"`.
   - Add `[dev-dependencies]` block with `serde_json = "1"` (host-target deserialisation tests for `IpcError`).
   - Add `[lib]` block: `name = "arx_runa"`, `path = "src/lib.rs"`, `crate-type = ["rlib"]`. Keep `[[bin]]` unchanged.

2. **Create `C:\Users\chris\source\repos\arx-runa\src\error.rs`** with **CS-001**. Include a `#[cfg(test)] mod tests` asserting `serde_json::from_str::<IpcError>(r#"{"kind":"notFound","message":"File not found"}"#)` succeeds with matching fields.

3. **Create `C:\Users\chris\source\repos\arx-runa\src\invoke.rs`** with **CS-002**. No unit tests — the extern requires a browser. Hand off runtime verification to the `trunk build` gate + manual smoke test in 6.3.

4. **Create `C:\Users\chris\source\repos\arx-runa\src\ipc_types\mod.rs`** with `mod session_status; mod file_entry; mod requests; pub use session_status::SessionStatus; pub use file_entry::FileEntry; pub use requests::ListDirectoryRequest;`.

5. **Create `C:\Users\chris\source\repos\arx-runa\src\ipc_types\session_status.rs`** with **CS-003**.

6. **Create `C:\Users\chris\source\repos\arx-runa\src\ipc_types\file_entry.rs`** with **CS-004**.

7. **Create `C:\Users\chris\source\repos\arx-runa\src\ipc_types\requests.rs`** with **CS-005**.

8. **Create `C:\Users\chris\source\repos\arx-runa\src\state\mod.rs`** with `mod session_context; mod sync_context; mod vault_context; pub use session_context::{SessionProvider, SessionState, use_session, use_session_actions}; pub use sync_context::{SyncProvider, SyncState, use_sync, use_sync_actions}; pub use vault_context::{VaultActions, VaultProvider, VaultState, use_vault, use_vault_actions};`.

9. **Create `C:\Users\chris\source\repos\arx-runa\src\state\session_context.rs`** with **CS-006**. Unit tests (host target): `test_session_state_apply_status_updates_three_fields_only`, `test_session_state_begin_authenticating_sets_flag_and_clears_error`, `test_session_state_complete_success_sets_unlocked_and_vault_id`, `test_session_state_complete_failure_records_message_and_clears_authenticating`.

10. **Create `C:\Users\chris\source\repos\arx-runa\src\state\vault_context.rs`** with **CS-007**. Unit tests (host target): `test_vault_state_clear_zeroes_all_fields`, `test_vault_state_clear_resets_selected_and_path_and_error`.

11. **Create `C:\Users\chris\source\repos\arx-runa\src\state\sync_context.rs`** with **CS-008**. No unit tests required by the sub-phase — provider is structural glue only.

12. **Create `C:\Users\chris\source\repos\arx-runa\src\lib.rs`** with **CS-009**.

13. **Edit `C:\Users\chris\source\repos\arx-runa\src\app.rs`** — wrap the current `view!` body in `SessionProvider > VaultProvider > SyncProvider`. Keep the `<h1>"Hello Arx Runa"</h1>` stub inside the innermost provider so `trunk build` still renders a page; 6.3 replaces the stub with the router. Imports: `use crate::state::{SessionProvider, SyncProvider, VaultProvider};`.

14. **Edit `C:\Users\chris\source\repos\arx-runa\src\main.rs`** — replace `mod app;` and `use app::App;` with `use arx_runa::App;`. Keep the rest verbatim.

15. **Run validation:** `trunk build` (target-wasm) and `cargo test --workspace --all-targets --all-features` (the latter exercises the host-target state unit tests). Both must pass.

### File Surface Summary

**New files:**
- `src/error.rs`
- `src/invoke.rs`
- `src/ipc_types/mod.rs`
- `src/ipc_types/session_status.rs`
- `src/ipc_types/file_entry.rs`
- `src/ipc_types/requests.rs`
- `src/state/mod.rs`
- `src/state/session_context.rs`
- `src/state/vault_context.rs`
- `src/state/sync_context.rs`
- `src/lib.rs`

**Modified files:**
- `Cargo.toml` (dependencies + lib target)
- `src/main.rs` (two-line switch to library import)
- `src/app.rs` (wrap in providers)

**Governance edits (pre-implementation, see §8):**
- `.claude/rules/leptos.md` (state-context surface note)
- `.claude/rules/rust.md` (none expected — structure already allows the chosen layout)

## 6. Review focus areas

### 6a. Rust change surface

Backend (`src-tauri/**/*.rs`): **None anticipated.**
Frontend (`src/**/*.rs`, treated as Rust under the workspace root):
- `src/lib.rs` (new)
- `src/error.rs` (new)
- `src/invoke.rs` (new)
- `src/ipc_types/**` (new)
- `src/state/**` (new)
- `src/main.rs` (modified — 2 lines)
- `src/app.rs` (modified — provider wrap)

### 6b. Security-sensitive paths

**None anticipated** under `src-tauri/src/{crypto,auth,storage}/`. The sub-phase touches frontend state scaffolding only. Sensitivity surface in 6.2 consists of two Zero-Trace touchpoints:

- `src/state/vault_context.rs::VaultState::clear` — must fully zero `files`, `current_path`, `selected`, `error` and `loading`. Any field added later and left out of `clear()` is a Zero-Trace regression. This is the enforcement anchor.
- `src/state/session_context.rs::SessionProvider` polling loop — must not log or expose `vault_id` / `timeout_seconds` to `console_log` or persistent storage.

`/implement-plan` drift rule: if any change lands under `src-tauri/src/{crypto,auth,storage}/` during Phase 6.2, flag a Plan Deviation.

### 6c. Architecture risk areas

- **Module boundaries** (`src/lib.rs`, `src/ipc_types/mod.rs`, `src/state/mod.rs`): enforce "mod.rs = re-exports only" per `.claude/rules/rust.md`. `mod.rs` files must not carry `struct`/`fn` bodies.
- **Concern isolation** (`src/invoke.rs`): wasm extern + typed helper only. No DTO structs, no error types defined here.
- **Dependency direction**: `state/*` depends on `invoke` and `ipc_types`. `ipc_types` depends only on `serde`. `invoke` depends on `error` and `serde_wasm_bindgen`. No back-edges from `invoke`/`error` into `state`.
- **Abstraction debt**: `FileEntry` mirror drift against the backend DTO. Mitigated by keeping the mirror file-colocated and documented as a mirror; Phase 6.3+ call sites go through the `is_directory()` helper rather than string compares.
- **Context hierarchy**: `App` must mount `SessionProvider > VaultProvider > SyncProvider` so the three hooks cannot panic in any descendant.

### 6d. Testing requirements

**Unit tests (host target, `cargo test`):**
- `error.rs`: `test_ipc_error_deserialises_from_backend_json_shape` — round-trip `{"kind":"notFound","message":"File or directory not found"}`.
- `error.rs`: `test_ipc_error_internal_constructor_produces_internal_error_kind`.
- `session_context.rs`: the four transition tests listed in Step 9.
- `vault_context.rs`: the two clear tests listed in Step 10.

**Target-wasm validation (`trunk build`):**
- Compiles with new direct deps.
- No `wasm-bindgen` signature errors on the `invoke` extern.

**Edge cases enforced via tests:**
- `SessionState::apply_status` leaves `authenticating` and `error` untouched (Concern 5).
- `VaultState::clear` wipes every field — test asserts field-by-field equality against `VaultState::default()`.
- `SessionState::complete_failure` leaves `vault_id` and `timeout_seconds` unchanged (they may already be populated from a prior polling tick).

**Validation checkpoint acceptance (from sub-phase):**
- `trunk build` completes without errors or warnings.
- State contexts provide/consume without panic (verified manually in 6.3 smoke test; out of scope for automated 6.2 tests per Concern 6).
- `invoke_command` correctly dispatches success and error branches (runtime-verified in 6.3 via a real `get_session_status` call).

## 7. Documentation impact

| Item | Type | Required this run? | Rationale |
|---|---|---|---|
| `docs/architecture/designs/tauri-ipc-and-frontend/design.md` — §Tauri IPC Integration `invoke` extern snippet (line 1440) | Design doc correction | **Deferred/optional** | Block is marked "Illustrative" per §How to Read This Design; the normative §Contract Surface does not bind the extern signature. Raising to a clarifying note would be housekeeping, not a contract change. Logged here so `/implement-plan` can record the skip rather than treat it as silent. |
| `docs/architecture/designs/tauri-ipc-and-frontend/design.md` — §Vault Context snippet (`invoke::<_, Vec<FileEntry>>("list_directory", &path)`) | Design doc correction | **Deferred/optional** | Same rationale — illustrative block. Real contract enforces the JSON request shape through Tauri's command deserialisation, which requires `{"path": "..."}`. Phase 6.3 plan should capture the request-struct convention for the broader command set. |
| `docs/architecture/designs/tauri-ipc-and-frontend/sub-phases/roadmap.md` — "Phase 6.2 done" status tick | Roadmap bookkeeping | **Deferred/optional** | Handled by the roadmap's existing Status column convention post-6.2 merge, not during implementation. |
| `.claude/rules/leptos.md` — state-context hook surface note | Rule update | **Required this run** | Keeps the Leptos rule set aligned with the canonical hook names (`use_session`/`use_session_actions`/`use_vault`/`use_vault_actions`/`use_sync`/`use_sync_actions`) so 6.3+ agents do not invent alternatives. Handled by §8 action **GS-001**. |
| `.github/instructions/leptos.instructions.md` | Copilot mirror | **Required this run** | `/copilot-sync` propagates `.claude/rules/leptos.md` changes into the Copilot mirror — handled automatically by §8 action **GS-002**. |

Backend design documents under `docs/architecture/designs/tauri-ipc-and-frontend/design.md` §Contract Surface are **not** updated — the contract has not changed.

## 8. Governance sync actions (pre-implementation)

| Action ID | Reason / linked concern | Target files (absolute) | Required edit | Verification |
|---|---|---|---|---|
| **GS-001** | Concern 11 + Concern 12 — fix the canonical Leptos hook surface so 6.3 pages consume it consistently. | `C:\Users\chris\source\repos\arx-runa\.claude\rules\leptos.md` | Append a new subsection under the existing `## Arx Runa constraints` block (before the trailing blank line), titled `## State contexts`, with three bullet lines: (1) `Hook pairs: use_session/use_session_actions, use_vault/use_vault_actions, use_sync/use_sync_actions — panic with "<Provider> must wrap the component tree" if the provider is missing.` (2) `VaultActions::clear is the Zero-Trace enforcement point — every new field added to VaultState must also be cleared there.` (3) `Provider hierarchy: SessionProvider > VaultProvider > SyncProvider in src/app.rs.` | `grep -n "use_session_actions" .claude/rules/leptos.md` returns a line under the new subsection. |
| **GS-002** | Sync Copilot mirror with `.claude/rules/leptos.md`. | `C:\Users\chris\source\repos\arx-runa\.github\instructions\leptos.instructions.md` (if it exists) | Run `/copilot-sync` after GS-001 — it rewrites `.github/instructions/leptos.instructions.md` from the rule source. | `/copilot-sync` reports success; `diff` between the two files shows only the formatting/frontmatter differences expected by the mirror convention. |

Note: if `.github/instructions/leptos.instructions.md` does not currently exist, `/copilot-sync` will create it; no manual edit is required beyond invoking the skill.

## 9. Handoff Notes for Implementer

Working directory is `C:\Users\chris\source\repos\arx-runa`. This plan is self-contained — re-reading the sub-phase is not required, but the **contract snippets (CS-001…CS-009)** are the source of truth for this phase's bodies; do not improvise signatures. Execute **§8 governance-sync actions first** (GS-001, GS-002) before touching any `src/**` files. Then follow **§5 Implementation Steps 1 → 15 in order**: Cargo.toml first (it gates compilation), then library/DTO/state files in the order listed, then `main.rs` + `app.rs` last (they close the circuit). Traps: (a) Tauri v2 command arguments are deserialised as a camelCase JSON object — pass a request struct such as `ListDirectoryRequest`, not a bare `String`; (b) the `invoke` extern must carry `#[wasm_bindgen(catch)]` (design illustration is wrong); (c) the three providers must wrap `App`'s body in `Session > Vault > Sync` order or 6.3 hooks panic; (d) `VaultState::clear` is the Zero-Trace enforcement point — tests must assert field-by-field parity with `VaultState::default()`; (e) the top-level `Cargo.toml` gains a `[lib]` entry alongside the existing `[[bin]]`, so both targets compile from the same tree. Validation closure: `trunk build` must succeed with zero warnings and `cargo test --workspace --all-targets --all-features` must pass all new tests. Security review is **not required** for this sub-phase per the verified scope check in §2; if any file under `src-tauri/src/{crypto,auth,storage}/` is touched, stop and escalate — that is a Plan Deviation.
