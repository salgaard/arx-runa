---
title: "Phase 6.3 — Frontend Pages"
created: "2026-04-21T00:00:00Z"
status: approved
roadmap-phase: 6
sub-phase: "6.3"
design-document: docs/architecture/designs/tauri-ipc-and-frontend/design.md
sub-phase-roadmap: docs/architecture/designs/tauri-ipc-and-frontend/sub-phases/roadmap.md
governance-sync-required: false
tags: [leptos, tauri, frontend, pages, zero-trace]
---

## 1. Goal

Build the Leptos page surface — `LoginPage`, `VaultCreationPage`, `VaultBrowser`, `DropZone`, `UploadButton`, `ProgressModal`, `AppShell`, `SessionStatus`, generic components — on top of the Phase 6.2 state contexts and `invoke_command` wrapper, with a conditional router that flips between login, vault creation, and vault browser according to `SessionState.is_unlocked`.

## 2. Context

- Sub-phase budget: ~400 production LoC + ~50 test LoC; third of four sub-phases (6.1 → 6.2 → 6.3 → 6.4). Strict dependency on 6.2 for `invoke_command<A,R>`, `SessionState`/`SessionActions`, `VaultState`/`VaultActions`, `SyncState`/`SyncActions`, and the mirrored DTOs (`FileEntry`, `SessionStatus`, `IpcError`).
- Phase 6.1 landed (`status: implemented`): 30-command `generate_handler!` surface. All long-running commands (`authenticate`, `create_vault`, `list_directory`, `upload_file`, `download_file`, `delete_file`, `get_file_content`, `sync_to_cloud`, …) currently return `IpcError::InternalError("command not yet wired")` — backend orchestration is owned by a later phase (labelled "Phase 6.5" in scaffold TODOs). Phase 6.3 is therefore expected to wire UI ⇄ IPC correctly; end-to-end success of the flows is *not* an acceptance criterion.
- Phase 6.2 landed (`status: implemented`): `src/lib.rs`, `src/invoke.rs`, `src/error.rs`, `src/ipc_types/` (`SessionStatus`, `FileEntry`, `ListDirectoryRequest`), `src/state/` (`SessionProvider` + `SessionActions`, `VaultProvider` + `VaultActions`, `SyncProvider` + `SyncActions`) all in place. `App` already mounts `SessionProvider > VaultProvider > SyncProvider` around a "Hello Arx Runa" stub.
- Frontend `Cargo.toml` deps present: `leptos = "0.8"`, `leptos_meta`, `leptos_router`, `serde`, `serde-wasm-bindgen`, `wasm-bindgen`, `wasm-bindgen-futures`, `gloo-timers`. **Missing** for 6.3: `zeroize`, `web-sys`, `js-sys`.
- Backend `Cargo.toml` deps present: `tauri`, `tauri-plugin-opener`, `tauri-plugin-shell`. **Missing** for 6.3: `tauri-plugin-dialog` (native file pickers for `UploadButton`, `KeyFileIndicator` manual select, `VaultCreationPage` key-file destination picker, `export_public_key` destination).
- Tailwind v4 brand tokens in `input.css` define `--color-iron`, `--color-stone`, `--color-steel`, `--color-rune`, `--color-bone`, plus text/surface/border scales. Design pages reference `text-danger` — no `--color-danger` token exists yet.
- Cross-phase invariants touched:
  - **#5 Vault path validation** — enforced server-side by `validate_vault_path` (`src-tauri/src/ui/validation.rs`); UI must present the raw user path to the IPC layer without pre-sanitising (server is the authority).
  - **#6 IPC sensitive-input handling** — backend `authenticate`/`create_vault`/`change_password` already move `String` into `Zeroizing<Vec<u8>>`; the *frontend* must zeroise its local `password` copy after the IPC call resolves (success or failure).
  - **#7 Zero-Trace** — lock path must clear Session, Vault, and Sync state (existing `*Actions::clear()` methods); when `SessionProvider` polling observes a transition `is_unlocked: true → false` the Vault + Sync states must also be cleared (not just Session, which `apply_status` already handles implicitly).
  - **#4 Chunk size contract** — `VaultCreationPage` must clamp `chunk_size_bytes` to [131072, 67108864] before submission; server-side `validate_chunk_size` is the final authority.
  - **#8 Epoch routing** — `VaultCreationPage` toggle must use invariant-aligned copy.
- Rule anchors:
  - `.claude/rules/leptos.md` (reactivity, component docs, Zero-Trace, hook pairs, Actions as Zero-Trace enforcement point, provider hierarchy)
  - `.claude/rules/rust.md` (one concern per file, `///` docs, no `unwrap()` outside `#[cfg(test)]`, testing naming `test_<unit>_<scenario>_<expected_outcome>`)
  - `.claude/rules/tauri.md` (plugin surface — `tauri-plugin-dialog` is not currently in the allowlist; adding it falls inside the design's explicit plugin guidance; clipboard/http/generic-shell remain denied)
- Security review: sub-phase explicitly states "Not required — page and layout components only. The password `zeroize` call introduced here is verified for correctness in Phase 6.4." Independently verified: Phase 6.3 does not touch `src-tauri/src/{crypto,auth,storage}/`. The one sensitivity touchpoint (password zeroise-after-use inside `LoginPage`/`VaultCreationPage`) is landed here but *audited* in 6.4.
- Validation gate: `trunk build` must succeed; pure-function unit tests cover `Breadcrumbs` path-splitting and `SessionStatus` countdown formatting.

## 3. Design Concerns / Open Questions

| # | Concern | Source | Impact | Classification | Resolution | Documentation updates |
|---|---|---|---|---|---|---|
| 1 | Sub-phase deliverables 1/3/8 require native file pickers (USB key file selection, key-file destination, upload picker); no `tauri-plugin-dialog` dependency or capability is wired. | `6.3-frontend-pages.md:14,16,18,22` vs. `src-tauri/Cargo.toml` + `src-tauri/capabilities/default.json` | Implementer would either ship without a native picker or improvise `web_sys::HtmlInputElement` — but `authenticate`/`create_vault`/`upload_file` IPC signatures take `PathBuf`, which a browser `<input type="file">` cannot provide. | Non-blocking | Add `tauri-plugin-dialog = "2"` to `src-tauri/Cargo.toml`, register the plugin in `src-tauri/src/lib.rs` (`.plugin(tauri_plugin_dialog::init())`), grant `dialog:allow-open` in `src-tauri/capabilities/default.json`, and use the plugin's JS surface (`window.__TAURI__.plugin.dialog.open({ multiple: false })`) via a thin `src/dialog.rs` wasm-bindgen extern. | Append `tauri-plugin-dialog` to the §Plugins allowlist in `.claude/rules/tauri.md`. |
| 2 | `KeyFileIndicator` is specified to "auto-detect key file path from `DeviceMonitor` events" and Implementation Notes §4 says "use `tauri::event::listen` in the WASM layer". The `DeviceMonitor` trait (`src-tauri/src/auth/device_monitor/mod.rs`) is consumed internally by `SessionManager` and does not currently emit Tauri `Event`s. | `6.3-frontend-pages.md:15,74` vs. `src-tauri/src/auth/device_monitor/*.rs` | Full auto-detect cannot be wired without a backend bridge that forwards `DeviceEvent`s onto `AppHandle::emit("device-event", …)`. | Non-blocking | Ship the manual-select fallback path as the production path in 6.3; add a small `device_event.rs` wasm subscriber that listens on the stable event name `"device-event"` and tolerates zero emissions. Note in Assumption 6: backend emission bridge is Phase 6.5 work; 6.3 proves the subscriber shape and writes `set_key_file_path.set(Some(path))` when the event arrives. | Log in §7 as deferred: backend `DeviceMonitor → Emitter` bridge. |
| 3 | Sub-phase validation checkpoint lists manual flows (authenticate → vault browser, drag-drop → ProgressModal, lock → return to LoginPage) that require the 6.5 backend wiring; all IPC commands currently return `InternalError("command not yet wired")`. | `6.3-frontend-pages.md:42-49` vs. `src-tauri/src/ui/{auth,file,sync}_commands.rs` | A literal reading blocks acceptance; in practice Phase 6.3 cannot prove end-to-end correctness. | Non-blocking | Narrow the Phase 6.3 acceptance contract to: (a) `trunk build` clean, (b) pages compile and mount, (c) IPC calls produce the sanitised error in the UI without panics, (d) lock button flips routing back to `LoginPage` and clears Vault+Sync state. The end-to-end flows listed in the sub-phase's "Manual verification" bullets run under Phase 6.5 regression, not here. | Document the narrowed scope in the sub-phase `## Implementation Decisions` section (see GS-001 fallback). |
| 4 | `SessionStatus` is both the name of the Phase 6.3 layout component (sub-phase deliverable 11) and the mirrored DTO (`src/ipc_types/session_status.rs::SessionStatus`). An `use crate::ipc_types::SessionStatus` inside `src/layout/session_status.rs` collides with `pub fn SessionStatus()`. | `6.3-frontend-pages.md:24` vs. `src/ipc_types/session_status.rs` | Name shadowing — imports compile only with aliases; easy to get wrong. | Non-blocking | Name the component `SessionStatusBar` in `src/layout/session_status.rs`. `AppShell` mounts `<SessionStatusBar/>`. DTO import path `crate::ipc_types::SessionStatus` stays canonical. | None — internal rename. |
| 5 | Sub-phase deliverable 13 leaves routing scheme unspecified. `leptos_router = "0.8"` is already in `Cargo.toml` but Phase 6.2 did not use it. The three "routes" (login, vault creation, vault browser) are session-state transitions, not URLs. | `6.3-frontend-pages.md:26` vs. `Cargo.toml:23`, `src/app.rs:1-17` | Two reasonable implementations (leptos_router vs. nested `Show`) diverge significantly in LoC. | Non-blocking | Use conditional view in `App`: no `leptos_router` routes. A `create_vault_intent` `RwSignal<bool>` local to `App` drives the `VaultCreationPage` branch; `is_unlocked` drives the `VaultBrowser` branch; otherwise `LoginPage`. Fits the ~400-LoC budget; leptos_router dep can be removed later in 6.4 cleanup if still unused. | Note the `create_vault_intent` decision in §4 Assumption 5. |
| 6 | Sub-phase deliverable 1 says the `LoginPage` `password` string is "zeroed via `zeroize` immediately after the IPC call completes". `zeroize` is not a frontend dependency, and `String::zeroize()` requires the `Zeroize` trait impl (behind `zeroize`'s `std` feature, on by default). | `6.3-frontend-pages.md:14` vs. `Cargo.toml:15-23` | Without the crate, the "zeroise" step either silently no-ops or does not compile. | Non-blocking | Add `zeroize = { version = "1", features = ["alloc"] }` to top-level `Cargo.toml`. Zeroise pattern: capture `password` into a local `String`, pass to `invoke_command`, on both success/failure branches call `password.zeroize()` *and* `set_password.update(|s| s.zeroize())` before any other `set_state`. The write-back to the signal is the Zero-Trace-critical step; the local copy is best-effort (stack lifetime is transient). | None — frontend dependency change only. |
| 7 | `ProgressModal` consumes `ProgressUpdate` via `tauri::ipc::Channel`. Tauri v2 exposes this in JS as `new window.__TAURI__.core.Channel<T>()` — a constructor that the frontend must create, pass as an argument to `invoke_command`, and subscribe to `onmessage`. No wasm-bindgen extern for `Channel` exists yet. | Implementation Notes §2 of sub-phase + `design.md:189-196` | Without a typed wrapper the implementer would have to spread untyped `JsValue` handling across `DropZone`, `UploadButton`, `ProgressModal`. | Non-blocking | Introduce `src/ipc_channel.rs` with a `IpcChannel<T>` newtype around a `js_sys::Object` bound to `window.__TAURI__.core.Channel`. Exposes `new() -> Self`, `inner(&self) -> &JsValue` (serialised as the `progress` arg), and `on_message<F: Fn(T)>(&self, cb: F)`. Callers pass `channel.inner()` inside the request struct by serialising the channel object via `serde_wasm_bindgen::preserve::serialize`. Defer richer typing to Phase 6.5. | None — internal helper. |
| 8 | Sub-phase deliverable 5 (`DropZone`) requires reading dropped file paths from the WebView drop event. Tauri v2 routes file drops through a webview event (`tauri://drag-drop` on Tauri 2), which requires subscribing via `window.__TAURI__.webview.getCurrentWebviewWindow().onDragDropEvent(cb)`. Current `tauri.conf.json` has `dragDropEnabled` implicit-on (default); verify. | `6.3-frontend-pages.md:18`, `src-tauri/tauri.conf.json:14-20` | If drag-drop events are not surfaced to the JS API, `DropZone` silently receives nothing. | Non-blocking | Add a thin `src/drag_drop.rs` wasm-bindgen extern to the webview `onDragDropEvent` handler. `DropZone` subscribes on mount (via `Effect::new`), unsubscribes in `on_cleanup`. Fallback UX: `UploadButton` remains the primary path if a user cannot drop for any reason. No config change to `tauri.conf.json` — default drag-drop wiring is sufficient. | None. |
| 9 | Sub-phase deliverable 3 enumerates `create_vault` IPC args (`vault_name`, `password`, `tier`, `key_file_destination`, `primary_destination`, `chunk_size_bytes`, `epoch_buffer_enabled`) but no request DTO exists in `src/ipc_types/`. Phase 6.2 only shipped `ListDirectoryRequest`. | `6.3-frontend-pages.md:16` vs. `src/ipc_types/requests.rs` | Without a typed request struct the call would ship a tuple or a handmade `JsValue`; camelCase mismatches would silently cause `IpcError::InvalidInput`. | Non-blocking | Extend `src/ipc_types/requests.rs` with `AuthenticateRequest`, `CreateVaultRequest` (including `DestinationSessionConfig` mirror), `UploadFileRequest`, `DeleteFileRequest`, `LockSessionRequest` (empty), `GetFileContentRequest`. Add a `src/ipc_types/destination_session_config.rs` mirror (camelCase, `Serialize` only — frontend sends, does not receive). | Extend §7 note: the mirrored `DestinationSessionConfig` pairs with backend `src-tauri/src/ui/types/destination_session_config.rs`. |
| 10 | Design code examples reference `bg-iron`, `text-bone`, `text-danger`, `bg-stone`, `border-steel`. `text-danger` has no corresponding `--color-danger` token in `input.css`. | `design.md:1245-1266` vs. `input.css:4-36` | Missing token → Tailwind emits no utility class → error text is invisible. | Non-blocking | Extend the `@theme` block in `input.css` with `--color-danger: #E26A6A;` (matches the existing desaturated palette; no design doc binding on the exact hex). Optional: also add `--color-muted: #636D7E;` alias if any page uses `text-muted`. | None — token selection is a UI decision. |
| 11 | Sub-phase acceptance criterion "Click the lock button in `SessionStatus`; verify the UI returns to `LoginPage` and no file data is visible" requires that *both* a user-triggered lock (via IPC) *and* a polling-observed lock (session timeout) clear Vault + Sync state. `SessionActions::clear()` alone touches `SessionState` only. | `6.3-frontend-pages.md:49` + invariant #7 | Server-side session timeout followed by a poll would update `SessionState.is_unlocked = false` but leave file names in `VaultState.files` until user manually clears. | Non-blocking | Add a `leptos::prelude::Effect` inside `App` that watches `session.read().is_unlocked`; on the true→false transition, call `use_vault_actions().clear()` and `use_sync_actions().clear()`. The lock button's click handler also invokes these before calling the `lock_session` IPC (defence-in-depth against IPC failure). Document as the canonical locked-transition hook; future phases may centralise this. | Note in Assumption 11. |
| 12 | Sub-phase deliverable 13 branches routing on `SessionState.is_unlocked` "when locked and no vault creation is in progress". Without a disambiguating signal, the initial render of a freshly-installed system (no vault header on disk) would show `LoginPage` with no way to reach `VaultCreationPage`. | `6.3-frontend-pages.md:26` | UX dead-end on first launch. | Non-blocking | `LoginPage` carries a "Create new vault" secondary button that sets `create_vault_intent.set(true)`; `VaultCreationPage` carries a "Back to login" link that sets it back to false. This is the minimal router control surface for 6.3. No backend ping required. | None — UX decision. |
| 13 | In-App File Viewing is documented as a Phase 6 scope item (`design.md#in-app-file-viewing-zero-trace`) but sub-phase deliverables do not list a viewer component. | `design.md:1530-1566` vs. `6.3-frontend-pages.md:12-28` | Implementer might build a viewer within budget or mistake its absence for a gap. | Non-blocking | Out of Phase 6.3 scope. `get_file_content` remains reachable but has no consumer in this phase. Phase 6.4 is free to add it; more likely it lands as a separate follow-up. | Log in §7 as deferred. |
| 14 | Test surface. Phase 6.2 established a host-target pure-function test regime (no `wasm-bindgen-test` harness). Sub-phase deliverable lists two Leptos component tests ("`Breadcrumbs` renders one segment per path component", "`SessionStatus` lock button triggers `VaultActions::clear()`"). Neither runs under plain `cargo test`. | `6.3-frontend-pages.md:37-39` vs. Phase 6.2 test strategy | Leptos component rendering requires a WASM runtime; adding that scaffold blows the 50-LoC test budget. | Non-blocking | Factor the testable logic into pure helpers — `fn split_path_segments(path: &str) -> Vec<(String, String)>` for `Breadcrumbs`, `fn format_countdown_seconds(remaining: u64) -> String` for `SessionStatus` — and test those on the host target. The click-handler → `clear()` assertion becomes a manual acceptance step documented in §6d. | Note in Assumption 9. |
| 15 | `authenticate` IPC takes `password: String + key_file_path: Option<PathBuf>`. In Rust 2024 + `serde-wasm-bindgen`, `PathBuf` serialises to a JSON string; Tauri v2 deserialises `String → PathBuf` successfully on the command boundary, but the camelCase conversion is `keyFilePath` (Phase 6.2 used `camelCase` on `ListDirectoryRequest` — same convention). | Phase 6.2 precedent `src/ipc_types/requests.rs` | Implementer may forget `#[serde(rename_all = "camelCase")]` and ship `key_file_path` which the Tauri command rejects. | Non-blocking | Every new request DTO carries `#[serde(rename_all = "camelCase")]`. Encode `Option<PathBuf>` as `Option<String>` on the wire (paths are user-selected opaque strings; the backend re-derives `PathBuf`). | None — convention already established in 6.2. |
| 16 | Sub-phase mentions `use_session()` / `use_vault_actions()` etc. Provider mount location (`App`) must remain `SessionProvider > VaultProvider > SyncProvider`; any 6.3 component that calls hooks at depth is safe — but the conditional router pattern (§5) must render pages *inside* all three providers, not outside. | `6.3-frontend-pages.md:26` + `.claude/rules/leptos.md:37` | Rendering `LoginPage` outside the providers panics. | Non-blocking | The conditional render block lives inside `App`'s `children()` position of `SyncProvider`, preserving hierarchy for every page. No provider moves. | None. |

**Summary:** 16 non-blocking concerns, 0 blocking. Proceed.

## 4. Assumptions

1. `tauri-plugin-dialog` is the chosen dialog surface (resolves Concern 1). The plugin is initialised in `src-tauri/src/lib.rs` with `.plugin(tauri_plugin_dialog::init())`; `src-tauri/capabilities/default.json` gains `"dialog:allow-open"` alongside the existing permission set. The frontend calls it via the `window.__TAURI__.plugin.dialog.open` JS surface through a `src/dialog.rs` extern; no custom IPC command is added.
2. `KeyFileIndicator` operates in manual-select mode in Phase 6.3 (resolves Concern 2). The `tauri::event::listen("device-event", …)` subscriber is wired but will receive no payloads until Phase 6.5 adds `AppHandle::emit` bridging inside `SessionManager`/`DeviceMonitor` glue. No backend edits under `src-tauri/src/auth/` in this phase.
3. Routing uses a conditional render inside `App` (resolves Concern 5), driven by `session.read().is_unlocked` and a local `create_vault_intent: RwSignal<bool>`. `leptos_router` stays as a dependency (already present) but is not used by 6.3 — removal is deferred.
4. Frontend password zeroisation (resolves Concern 6) is considered best-effort within the WASM/JS boundary. The Rust-side copy in the Leptos signal *is* zeroised; the temporary `String` argument crossed to `invoke_command` cannot be zeroised after `serde_wasm_bindgen::to_value` has copied it into JS memory. Phase 6.4 audits and documents this limitation.
5. `CreateVaultRequest` carries `chunk_size_bytes: u64` clamped by the UI to `131_072..=67_108_864` before submission; the backend `validate_chunk_size` (`src-tauri/src/ui/validation.rs:100`) is the final authority. Preset names on the UI are **Standard** (4 MiB, 4_194_304), **Documents** (512 KiB, 524_288), **Media** (16 MiB, 16_777_216), **Paranoid** (64 MiB, 67_108_864).
6. Epoch buffer toggle copy in `VaultCreationPage` reads exactly: *"Opt-in: files smaller than the chunk size are packed before upload; larger files upload immediately."* Aligns with invariant #8; matches the sub-phase's sentence verbatim.
7. `DropZone` drag-drop events arrive via the Tauri webview JS API (`webview.onDragDropEvent`). Subscribing on mount via `Effect::new` and unsubscribing via `on_cleanup` is the only wasm interface used. No `tauri.conf.json` changes.
8. `ProgressModal` treats the received `ProgressUpdate` as authoritative state — no client-side percent derivation. An untouched `ProgressModal` is idle/hidden; receiving any `ProgressUpdate` shows it and updates the bar. Final 100% update or any IPC error closes it.
9. Unit tests are pure-helper based (resolves Concern 14). Required tests: `test_split_path_segments_root_returns_single_root_segment`, `test_split_path_segments_nested_path_splits_into_ordered_pairs`, `test_split_path_segments_strips_trailing_slash`, `test_format_countdown_seconds_zero_returns_zero_zero`, `test_format_countdown_seconds_sub_minute_returns_mm_ss`, `test_format_countdown_seconds_over_hour_returns_hhmmss`. Component rendering tests documented as manual verification in §6d.
10. Generic components (`Button`, `Input`, `Modal`, `Spinner`) surface the minimum props needed by 6.3 pages — `Button { variant, loading, on_click, children }`, `Input { input_type, label, value, on_input, placeholder }`, `Modal { open, on_close, children }`, `Spinner { size: &str }`. No design system abstraction beyond that.
11. Locked-transition hook (resolves Concern 11): an `Effect::new` in `App` watches `session.read().is_unlocked`; on `true → false` it calls `vault_actions.clear()` and `sync_actions.clear()`. The `SessionStatusBar` lock button calls these first, then invokes `lock_session` — the effect is the secondary safety net for polling-observed timeouts.
12. Added frontend dependencies: `zeroize = { version = "1", features = ["alloc"] }`, `web-sys = { version = "0.3", features = ["Window", "HtmlInputElement", "Event", "DragEvent", "DataTransfer", "FileList", "File"] }`, `js-sys = "0.3"`.
13. Added backend dependency: `tauri-plugin-dialog = "2"`. Added capability permission: `"dialog:allow-open"`.
14. No changes to `tauri.conf.json` security block — CSP stays `null` for Phase 6.3 and is hardened in Phase 6.4 along with clipboard/backoff/Zero-Trace audit.
15. Brand-token extension: `input.css` `@theme` gains `--color-danger: #E26A6A;`. No other new tokens.
16. File organisation follows the design's `## Project Structure` block: `src/auth/`, `src/vault/`, `src/transfer/`, `src/layout/`, `src/components/`. The `src/remote/` and `src/destinations/` feature trees listed in the design are **not** created in 6.3 (`RemoteBrowser`, `DestinationList` are Phase 6 deliverables but not part of the sub-phase's enumerated deliverables; defer to a later sub-phase).

## 5. Approach

### `CONTRACT_SNIPPETS`

**CS-001 — New request DTOs** (`src/ipc_types/requests.rs` — extend existing file)

```rust
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListDirectoryRequest {
    pub path: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthenticateRequest {
    pub password: String,
    pub key_file_path: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateVaultRequest {
    pub vault_name: String,
    pub password: String,
    pub tier: u8,
    pub key_file_destination: Option<String>,
    pub primary_destination: DestinationSessionConfig,
    pub chunk_size_bytes: u64,
    pub epoch_buffer_enabled: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UploadFileRequest {
    pub source_path: String,
    pub vault_path: String,
    pub progress: wasm_bindgen::JsValue,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteFileRequest {
    pub file_id: String,
}
```

**CS-002 — `DestinationSessionConfig` frontend mirror** (`src/ipc_types/destination_session_config.rs`)

```rust
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DestinationSessionConfig {
    pub label: String,
    pub destination_type: String,
    pub provider: String,
    pub bucket: String,
    pub region: String,
    pub endpoint: String,
    pub path_prefix: String,
    pub rclone_config_blob: String,
    pub is_primary: bool,
    pub backup_mode: Option<String>,
}
```

**CS-003 — File-open dialog extern** (`src/dialog.rs`)

```rust
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "plugin", "dialog"], catch)]
    async fn open(options: JsValue) -> Result<JsValue, JsValue>;
}

/// Opens a single-file open dialog. Returns `Some(path)` or `None` if cancelled.
pub async fn open_file_dialog() -> Option<String> {
    let opts = serde_wasm_bindgen::to_value(&serde_json::json!({
        "multiple": false,
        "directory": false,
    })).ok()?;
    let result = open(opts).await.ok()?;
    if result.is_null() || result.is_undefined() { return None; }
    result.as_string()
}

/// Opens a save-file dialog. Returns `Some(path)` or `None` if cancelled.
pub async fn save_file_dialog() -> Option<String> { /* mirror of open_file_dialog, calls `save` extern */ todo!("see implementer notes") }
```

**CS-004 — IPC Channel extern** (`src/ipc_channel.rs`)

```rust
use serde::de::DeserializeOwned;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "core"])]
    #[derive(Clone)]
    pub type Channel;

    #[wasm_bindgen(constructor, js_namespace = ["window", "__TAURI__", "core"])]
    pub fn new() -> Channel;

    #[wasm_bindgen(method, setter, js_name = onmessage)]
    pub fn set_onmessage(this: &Channel, cb: &js_sys::Function);
}

/// Typed wrapper around `window.__TAURI__.core.Channel`.
///
/// Pass `inner()` to IPC requests that accept a progress channel; register `on_message`
/// to receive deserialised payloads.
pub struct IpcChannel<T: DeserializeOwned + 'static> {
    inner: Channel,
    _marker: std::marker::PhantomData<T>,
}

impl<T: DeserializeOwned + 'static> IpcChannel<T> {
    pub fn new() -> Self { Self { inner: Channel::new(), _marker: std::marker::PhantomData } }
    pub fn inner(&self) -> &JsValue { self.inner.unchecked_ref() }
    pub fn on_message<F: Fn(T) + 'static>(&self, handler: F) {
        let cb = Closure::wrap(Box::new(move |msg: JsValue| {
            if let Ok(payload) = serde_wasm_bindgen::from_value::<T>(msg) {
                handler(payload);
            }
        }) as Box<dyn Fn(JsValue)>);
        self.inner.set_onmessage(cb.as_ref().unchecked_ref());
        cb.forget();
    }
}
```

**CS-005 — Drag-drop event extern** (`src/drag_drop.rs`)

```rust
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "webview"])]
    fn getCurrentWebviewWindow() -> JsValue;
}

/// Subscribes to `onDragDropEvent`. Returns an unsubscribe closure.
/// Invokes `handler` with the list of dropped file paths on a successful drop;
/// ignores "enter"/"over"/"leave" variants.
pub fn on_file_drop<F: Fn(Vec<String>) + 'static>(handler: F) -> impl FnOnce() { /* … see implementer notes … */ todo!() }
```

**CS-006 — Generic components** (`src/components/*.rs`)

```rust
// src/components/button.rs
use leptos::prelude::*;

#[component]
pub fn Button(
    #[prop(optional)] variant: &'static str,
    #[prop(into, optional)] loading: Signal<bool>,
    on_click: impl Fn(leptos::ev::MouseEvent) + 'static + Clone,
    children: Children,
) -> impl IntoView { /* Tailwind variant classes; disabled when loading.get() */ todo!() }

// src/components/input.rs
#[component]
pub fn Input(
    #[prop(optional)] input_type: &'static str,
    #[prop(into)] label: String,
    #[prop(optional)] placeholder: &'static str,
    value: ReadSignal<String>,
    on_input: impl Fn(String) + 'static + Clone,
) -> impl IntoView { /* labelled input; emits current value on `oninput` */ todo!() }

// src/components/modal.rs
#[component]
pub fn Modal(
    #[prop(into)] open: Signal<bool>,
    on_close: impl Fn() + 'static + Clone,
    children: Children,
) -> impl IntoView { /* portal-ed overlay; Escape key → on_close() */ todo!() }

// src/components/spinner.rs
#[component]
pub fn Spinner(#[prop(optional)] size: &'static str) -> impl IntoView { /* tailwind animate-spin */ todo!() }
```

**CS-007 — `LoginPage`** (`src/auth/login_page.rs`)

```rust
use leptos::prelude::*;
use zeroize::Zeroize;

use crate::auth::KeyFileIndicator;
use crate::components::{Button, Input};
use crate::dialog::open_file_dialog;
use crate::error::IpcError;
use crate::invoke::invoke_command;
use crate::ipc_types::{AuthenticateRequest, SessionStatus as _};
use crate::state::{use_session, use_session_actions, use_vault_actions};

#[component]
pub fn LoginPage(
    on_request_create_vault: impl Fn() + 'static + Clone,
) -> impl IntoView {
    let (password, set_password) = signal(String::new());
    let (key_file_path, set_key_file_path) = signal::<Option<String>>(None);
    let (loading, set_loading) = signal(false);
    let session_actions = use_session_actions();
    let session = use_session();

    let on_submit = {
        let on_create = on_request_create_vault.clone();
        move |_| {
            let mut pwd = password.get();
            let key_file = key_file_path.get();
            if pwd.is_empty() {
                session_actions.complete_failure("Password is required".into());
                return;
            }
            session_actions.begin_authenticating();
            set_loading.set(true);

            leptos::task::spawn_local(async move {
                let result = invoke_command::<AuthenticateRequest, crate::ipc_types::AuthResponse>(
                    "authenticate",
                    &AuthenticateRequest { password: pwd.clone(), key_file_path: key_file },
                ).await;
                pwd.zeroize();
                set_password.update(|s| s.zeroize());
                match result {
                    Ok(resp) => session_actions.complete_success(resp.vault_id),
                    Err(err) => session_actions.complete_failure(err.message),
                }
                set_loading.set(false);
            });
            let _ = on_create;
        }
    };

    view! {
        <div class="min-h-screen bg-iron flex items-center justify-center p-4">
            <div class="w-full max-w-md bg-stone border border-steel rounded-xl p-6 shadow-xl">
                <h1 class="text-2xl text-bone text-center mb-6">"Unlock Vault"</h1>
                <Input input_type="password" label="Password".into() value=password
                       on_input=move |v| set_password.set(v) />
                <KeyFileIndicator detected_path=key_file_path
                                   on_manual_select=move |p| set_key_file_path.set(Some(p)) />
                {move || session.read().error.clone().map(|e| view! {
                    <p class="text-danger text-sm mt-2">{e}</p>
                })}
                <Button loading=loading on_click=on_submit>"Unlock"</Button>
                <button class="mt-4 text-rune text-sm"
                        on:click=move |_| on_request_create_vault()>
                    "Create new vault"
                </button>
            </div>
        </div>
    }
}
```

**CS-008 — `VaultCreationPage`** (`src/auth/vault_creation_page.rs`)

```rust
const CHUNK_MIN: u64 = 131_072;
const CHUNK_MAX: u64 = 67_108_864;
const PRESETS: &[(&str, u64)] = &[
    ("Documents (512 KiB)", 524_288),
    ("Standard (4 MiB)", 4_194_304),
    ("Media (16 MiB)", 16_777_216),
    ("Paranoid (64 MiB)", 67_108_864),
];

fn clamp_chunk_size(bytes: u64) -> u64 { bytes.clamp(CHUNK_MIN, CHUNK_MAX) }

#[component]
pub fn VaultCreationPage(on_back_to_login: impl Fn() + 'static + Clone) -> impl IntoView {
    /* signals: vault_name, password, tier (u8 default 2),
       key_file_destination, chunk_size_bytes (default 4_194_304),
       epoch_buffer_enabled (default false), primary_destination fields.
       Submit handler builds a CreateVaultRequest, clamps chunk_size_bytes,
       invokes "create_vault", zeroes password on both branches.
       Epoch toggle label text from Assumption 6. */
    todo!()
}
```

**CS-009 — `VaultBrowser` + DropZone + FileList + Breadcrumbs + UploadButton** (`src/vault/*.rs`)

```rust
// src/vault/breadcrumbs.rs
/// Splits a vault-relative path into (label, cumulative_path) pairs.
/// Root is rendered as "Vault".
pub fn split_path_segments(path: &str) -> Vec<(String, String)> {
    let trimmed = path.trim_start_matches('/').trim_end_matches('/');
    let mut out = vec![("Vault".into(), "/".into())];
    if trimmed.is_empty() { return out; }
    let mut acc = String::new();
    for seg in trimmed.split('/') {
        acc.push('/'); acc.push_str(seg);
        out.push((seg.to_string(), acc.clone()));
    }
    out
}

#[component]
pub fn Breadcrumbs(#[prop(into)] path: Signal<String>) -> impl IntoView {
    let actions = crate::state::use_vault_actions();
    view! {
        <nav class="flex gap-1 text-sm text-bone">
            <For each=move || split_path_segments(&path.get())
                 key=|(_, full)| full.clone()
                 children=move |(label, full)| {
                     let full_click = full.clone();
                     view! {
                         <button class="hover:text-rune"
                                 on:click=move |_| actions.navigate(full_click.clone())>
                             {label}
                         </button>
                         <span>"/"</span>
                     }
                 } />
        </nav>
    }
}

// src/vault/file_list.rs — renders FileItem per FileEntry
// src/vault/file_item.rs — type icon (directory vs. file), name, size, modified date
// src/vault/drop_zone.rs — on_mount subscribes via crate::drag_drop::on_file_drop;
//     on drop: for each path → invoke "upload_file" with a fresh IpcChannel<ProgressUpdate>.
// src/vault/upload_button.rs — click → open_file_dialog().await → same invoke.
// src/vault/vault_browser.rs — composes the above; on mount calls vault_actions.navigate("/".into()).
```

**CS-010 — `ProgressModal`** (`src/transfer/progress_modal.rs`)

```rust
use leptos::prelude::*;
use crate::components::Modal;
use crate::ipc_channel::IpcChannel;
use crate::ipc_types::ProgressUpdate;

#[component]
pub fn ProgressModal(
    channel: IpcChannel<ProgressUpdate>,
    #[prop(into)] title: String,
    on_close: impl Fn() + 'static + Clone,
) -> impl IntoView {
    let (progress, set_progress) = signal::<Option<ProgressUpdate>>(None);
    channel.on_message(move |update| set_progress.set(Some(update)));
    view! {
        <Modal open=Signal::derive(move || progress.read().is_some()) on_close=on_close>
            /* title, bytes_processed/bytes_total, percent bar, status */
        </Modal>
    }
}
```

**CS-011 — `ProgressUpdate` DTO mirror** (`src/ipc_types/progress_update.rs`)

```rust
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgressUpdate {
    pub percent: u8,
    pub bytes_processed: u64,
    pub bytes_total: u64,
    pub status: String,
}
```

**CS-012 — `AuthResponse` DTO mirror** (`src/ipc_types/auth_response.rs`)

```rust
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthResponse {
    pub vault_id: String,
    pub vault_name: String,
}
```

**CS-013 — `AppShell` + `SessionStatusBar`** (`src/layout/*.rs`)

```rust
// src/layout/app_shell.rs
#[component]
pub fn AppShell(children: Children) -> impl IntoView {
    view! {
        <div class="min-h-screen bg-iron text-bone flex flex-col">
            <crate::layout::Header />
            <main class="flex-1 p-6">{children()}</main>
            <crate::layout::SessionStatusBar />
        </div>
    }
}

// src/layout/header.rs — logo/title bar.
// src/layout/session_status.rs
pub fn format_countdown_seconds(remaining: u64) -> String {
    let h = remaining / 3600;
    let m = (remaining % 3600) / 60;
    let s = remaining % 60;
    if h > 0 { format!("{h:02}:{m:02}:{s:02}") } else { format!("{m:02}:{s:02}") }
}

#[component]
pub fn SessionStatusBar() -> impl IntoView {
    let session = use_session();
    let session_actions = use_session_actions();
    let vault_actions = use_vault_actions();
    let sync_actions = use_sync_actions();
    let on_lock = move |_| {
        vault_actions.clear();
        sync_actions.clear();
        session_actions.clear();
        leptos::task::spawn_local(async move {
            let _ = invoke_command::<(), ()>("lock_session", &()).await;
        });
    };
    view! {
        <footer class="flex justify-between p-4 bg-stone border-t border-steel text-sm">
            <span>{move || session.read().timeout_seconds.map(format_countdown_seconds).unwrap_or_default()}</span>
            <button class="text-rune hover:text-bone" on:click=on_lock>"Lock"</button>
        </footer>
    }
}
```

**CS-014 — `App` routing** (`src/app.rs` — replace the stub body)

```rust
use leptos::prelude::*;
use crate::auth::{LoginPage, VaultCreationPage};
use crate::layout::AppShell;
use crate::state::{SessionProvider, SyncProvider, VaultProvider, use_session, use_sync_actions, use_vault_actions};
use crate::vault::VaultBrowser;

#[component]
pub fn App() -> impl IntoView {
    view! {
        <SessionProvider>
            <VaultProvider>
                <SyncProvider>
                    <Router />
                </SyncProvider>
            </VaultProvider>
        </SessionProvider>
    }
}

#[component]
fn Router() -> impl IntoView {
    let session = use_session();
    let vault_actions = use_vault_actions();
    let sync_actions = use_sync_actions();
    let create_vault_intent = RwSignal::new(false);

    Effect::new(move |prev: Option<bool>| {
        let now = session.read().is_unlocked;
        if prev == Some(true) && !now {
            vault_actions.clear();
            sync_actions.clear();
        }
        now
    });

    move || {
        let is_unlocked = session.read().is_unlocked;
        if is_unlocked {
            view! { <AppShell><VaultBrowser/></AppShell> }.into_any()
        } else if create_vault_intent.get() {
            view! { <VaultCreationPage on_back_to_login=move || create_vault_intent.set(false) /> }.into_any()
        } else {
            view! { <LoginPage on_request_create_vault=move || create_vault_intent.set(true) /> }.into_any()
        }
    }
}
```

**CS-015 — `input.css` `@theme` addition**

```css
--color-danger: #E26A6A;
```

**CS-016 — Backend plugin wiring** (`src-tauri/src/lib.rs`)

```rust
// inside tauri::Builder chain, after .manage(AppState::default()) and before .invoke_handler(...)
.plugin(tauri_plugin_dialog::init())
```

**CS-017 — Backend capability** (`src-tauri/capabilities/default.json` `permissions` array addition)

```json
"dialog:allow-open"
```

### Implementation Steps

All paths absolute; `C:\Users\chris\source\repos\arx-runa` elided in step titles.

1. **Extend top-level `Cargo.toml`** — under `[dependencies]` append `zeroize = { version = "1", features = ["alloc"] }`, `web-sys = { version = "0.3", features = ["Window", "HtmlInputElement", "Event", "DragEvent", "DataTransfer", "FileList", "File"] }`, `js-sys = "0.3"`. Keep `[dev-dependencies]` unchanged.
2. **Extend `src-tauri/Cargo.toml`** — under `[dependencies]` append `tauri-plugin-dialog = "2"`.
3. **Wire the dialog plugin** — edit `src-tauri/src/lib.rs` per **CS-016** inside the `tauri::Builder` chain.
4. **Grant dialog capability** — edit `src-tauri/capabilities/default.json` per **CS-017**.
5. **Extend Tailwind tokens** — edit `input.css` and append **CS-015** inside the `@theme {}` block.
6. **Create `src/dialog.rs`** with **CS-003**. Module-level `//!` doc explains the plugin binding.
7. **Create `src/ipc_channel.rs`** with **CS-004**.
8. **Create `src/drag_drop.rs`** with **CS-005**.
9. **Extend `src/ipc_types/requests.rs`** with **CS-001**.
10. **Create `src/ipc_types/destination_session_config.rs`** with **CS-002**; add `pub mod destination_session_config; pub use destination_session_config::DestinationSessionConfig;` to `src/ipc_types/mod.rs`.
11. **Create `src/ipc_types/progress_update.rs`** with **CS-011**; re-export in `mod.rs`.
12. **Create `src/ipc_types/auth_response.rs`** with **CS-012**; re-export in `mod.rs`.
13. **Create `src/components/` tree**: `mod.rs` (re-exports), `button.rs`, `input.rs`, `modal.rs`, `spinner.rs`, each with **CS-006**-shape bodies. Tailwind classes use brand tokens only.
14. **Create `src/auth/` tree**: `mod.rs` (re-exports `LoginPage`, `VaultCreationPage`, `KeyFileIndicator`), `login_page.rs` (**CS-007**), `vault_creation_page.rs` (**CS-008**), `key_file_indicator.rs` (manual-select + `tauri::event::listen("device-event", …)` wasm subscriber that tolerates no payloads).
15. **Create `src/vault/` tree**: `mod.rs`, `vault_browser.rs`, `breadcrumbs.rs`, `file_list.rs`, `file_item.rs`, `drop_zone.rs`, `upload_button.rs` (**CS-009**). `Breadcrumbs` uses the pure helper `split_path_segments` from `breadcrumbs.rs`.
16. **Create `src/transfer/` tree**: `mod.rs`, `progress_modal.rs` (**CS-010**).
17. **Create `src/layout/` tree**: `mod.rs`, `app_shell.rs`, `header.rs`, `session_status.rs` (**CS-013**). `SessionStatusBar` uses the pure helper `format_countdown_seconds`.
18. **Edit `src/lib.rs`** — add `pub mod auth; pub mod vault; pub mod transfer; pub mod layout; pub mod components; pub mod dialog; pub mod ipc_channel; pub mod drag_drop;` alongside existing exports.
19. **Edit `src/app.rs`** — replace the stub body with **CS-014**. `Router` is a sibling component.
20. **Unit tests** (inline `#[cfg(test)] mod tests` in the two host-target-testable helper files):
    - `src/vault/breadcrumbs.rs`: `test_split_path_segments_root_returns_single_root_segment`, `test_split_path_segments_nested_path_splits_into_ordered_pairs`, `test_split_path_segments_strips_trailing_slash`.
    - `src/layout/session_status.rs`: `test_format_countdown_seconds_zero_returns_zero_zero`, `test_format_countdown_seconds_sub_minute_returns_mm_ss`, `test_format_countdown_seconds_over_hour_returns_hhmmss`.
    - `src/auth/vault_creation_page.rs`: `test_clamp_chunk_size_below_min_returns_min`, `test_clamp_chunk_size_above_max_returns_max`, `test_clamp_chunk_size_default_preset_unchanged`.
21. **Run validation** — `trunk build` (warning-free) and `cargo test --workspace --all-targets --all-features` (all new tests green; no backend regressions).

### File Surface Summary

**New files (frontend):**
- `src/dialog.rs`, `src/ipc_channel.rs`, `src/drag_drop.rs`
- `src/ipc_types/destination_session_config.rs`, `src/ipc_types/progress_update.rs`, `src/ipc_types/auth_response.rs`
- `src/components/{mod.rs,button.rs,input.rs,modal.rs,spinner.rs}`
- `src/auth/{mod.rs,login_page.rs,vault_creation_page.rs,key_file_indicator.rs}`
- `src/vault/{mod.rs,vault_browser.rs,breadcrumbs.rs,file_list.rs,file_item.rs,drop_zone.rs,upload_button.rs}`
- `src/transfer/{mod.rs,progress_modal.rs}`
- `src/layout/{mod.rs,app_shell.rs,header.rs,session_status.rs}`

**Modified files (frontend):**
- `Cargo.toml` (zeroize, web-sys, js-sys)
- `src/lib.rs` (module re-exports)
- `src/app.rs` (Router body)
- `src/ipc_types/{mod.rs,requests.rs}` (new DTOs)
- `input.css` (danger token)

**Modified files (backend):**
- `src-tauri/Cargo.toml` (tauri-plugin-dialog)
- `src-tauri/src/lib.rs` (plugin init)
- `src-tauri/capabilities/default.json` (dialog:allow-open)

**Governance edits (pre-implementation):**
- None required. (See §8.)

## 6. Review focus areas

### 6a. Rust change surface

Backend (`src-tauri/**/*.rs`):
- `src-tauri/src/lib.rs` — one-line plugin registration.

Frontend (treated as Rust under the workspace root):
- All files under `src/auth/`, `src/vault/`, `src/transfer/`, `src/layout/`, `src/components/` (new).
- `src/dialog.rs`, `src/ipc_channel.rs`, `src/drag_drop.rs` (new).
- `src/ipc_types/{destination_session_config.rs,progress_update.rs,auth_response.rs}` (new), `src/ipc_types/mod.rs` and `src/ipc_types/requests.rs` (modified).
- `src/lib.rs`, `src/app.rs` (modified).

### 6b. Security-sensitive paths

**None anticipated** under `src-tauri/src/{crypto,auth,storage}/`. The backend change is limited to `src-tauri/src/lib.rs` (plugin init one-liner) and `src-tauri/capabilities/default.json` (new permission string) — neither is in the sensitive set. `/implement-plan` drift rule: any change landing in `src-tauri/src/{crypto,auth,storage}/` during Phase 6.3 is a Plan Deviation.

Sensitivity touchpoints on the frontend (non-crypto):
- `src/auth/login_page.rs` and `src/auth/vault_creation_page.rs` — password zeroisation after IPC resolution on both branches. Must zeroise the *signal* (`set_password.update(|s| s.zeroize())`) and not only the local capture.
- `src/app.rs::Router` Effect — Zero-Trace lock propagation. Any new state context added to the app must also be cleared here.

### 6c. Architecture risk areas

- **Module boundaries**: every new subtree has a `mod.rs` that is *re-exports only* — no `struct`/`fn` bodies. Enforce `.claude/rules/rust.md` §Structure.
- **Concern isolation**: `src/dialog.rs` = plugin extern only; `src/ipc_channel.rs` = channel extern only; `src/drag_drop.rs` = webview-event extern only. No mixing of externs, DTOs, and page logic in the same file.
- **Dependency direction**: `auth`/`vault`/`transfer`/`layout` depend on `components`, `ipc_types`, `state`, `invoke`, `dialog`, `ipc_channel`, `drag_drop`. `components` depends on nothing project-internal (leaf). `state` does **not** gain a back-edge to any page (it stays the pure context layer).
- **Abstraction debt**: generic components carry minimal prop surface — do not grow them beyond what the 6.3 deliverables need. `Button::variant` may be a `&'static str` for now, not an enum; bumping to an enum is a follow-up.
- **Context hierarchy**: `App` still mounts `SessionProvider > VaultProvider > SyncProvider`, and every page renders inside the innermost provider. Moving any provider out of `App` breaks all hooks — reject at review.
- **Effect discipline**: the `App::Router` `Effect` is the *only* place that observes `session.is_unlocked` to fan out `vault_actions.clear()` / `sync_actions.clear()`. Pages must not duplicate this logic.

### 6d. Testing requirements

**Unit tests (host target, `cargo test`):**
- `src/vault/breadcrumbs.rs` — 3 tests on `split_path_segments` (root, nested, trailing slash).
- `src/layout/session_status.rs` — 3 tests on `format_countdown_seconds` (0, sub-minute, over-hour).
- `src/auth/vault_creation_page.rs` — 3 tests on `clamp_chunk_size` (below-min clamps up, above-max clamps down, valid preset unchanged).

**Target-wasm validation (`trunk build`):**
- Compiles without errors or warnings.
- `wasm-bindgen` externs for `window.__TAURI__.plugin.dialog.open`, `window.__TAURI__.core.Channel`, `window.__TAURI__.webview.getCurrentWebviewWindow` resolve at link time.
- All page components mount without panic (manual smoke via `trunk serve`).

**Manual verification (documented, not automated):**
- Open app; verify `LoginPage` renders.
- Click "Create new vault" button; verify `VaultCreationPage` renders with all controls.
- Submit vault creation (IPC returns `InternalError("command not yet wired")` in 6.3; UI should display the message in the `text-danger` slot without crashing).
- Enter password and click "Unlock"; verify the same sanitised-error path.
- Click lock in `SessionStatusBar` while unlocked (requires a forced unlocked state for manual testing); verify routing returns to `LoginPage` and `VaultState`/`SyncState` are cleared.

**Edge cases enforced via tests:**
- `split_path_segments("/")` returns exactly one entry `("Vault", "/")` — no duplicate root.
- `format_countdown_seconds(3600)` returns `"01:00:00"` — hour boundary.
- `clamp_chunk_size(0)` returns `131_072` — zero input is clamped, not rejected (validation happens server-side).

**Validation checkpoint acceptance (narrowed per Concern 3):**
- `trunk build` clean.
- All new unit tests green.
- No panics on UI mount or during IPC error flow.
- Lock flow clears `VaultState` and `SyncState` and routes to `LoginPage`.

## 7. Documentation impact

| Item | Type | Required this run? | Rationale |
|---|---|---|---|
| `docs/architecture/designs/tauri-ipc-and-frontend/sub-phases/6.3-frontend-pages.md` — add `## Implementation Decisions` section capturing (a) narrowed validation scope (Concern 3), (b) manual-select-only `KeyFileIndicator` (Concern 2), (c) conditional-router decision (Concern 5). | Sub-phase doc annotation | **Required this run** | Phase 6.2 precedent: deviations from the sub-phase literal scope live in an `## Implementation Decisions` section. Keeps Phase 6.5 planners informed about what to pick up. |
| `.claude/rules/tauri.md` — append `tauri-plugin-dialog` (scoped to `dialog:allow-open`) to the §Plugins allowlist exception. | Rule update | **Required this run** | Rule currently lists only `tauri-plugin-shell` and `tauri_plugin_opener` as allowed plugins; adding `dialog` without updating the rule invites a future reviewer to flag it as a violation. |
| `.github/instructions/tauri.instructions.md` — sync from `.claude/rules/tauri.md`. | Copilot mirror | **Required this run** | `/copilot-sync` propagates rule-source edits. |
| `docs/architecture/designs/tauri-ipc-and-frontend/design.md` — §In-App File Viewing (Zero-Trace) implementation target annotation (currently labels Phase 6 without sub-phase attribution). | Design doc tweak | **Deferred/optional** | Feature not landed in 6.3; attribution is a later-phase housekeeping item. Log here so `/implement-plan` records the skip. |
| `docs/architecture/designs/tauri-ipc-and-frontend/design.md` — §Project Structure `src/remote/` and `src/destinations/` tree annotation (not implemented in 6.3). | Design doc tweak | **Deferred/optional** | Same rationale: scope outside 6.3; later sub-phase will add. |
| `docs/architecture/designs/tauri-ipc-and-frontend/sub-phases/roadmap.md` — status tick post-merge. | Roadmap bookkeeping | **Deferred/optional** | Handled by roadmap convention after merge. |

Backend `design.md` §Contract Surface is **not** updated — contracts do not change in Phase 6.3. The IPC command set, error enum, and response types are already canonical from Phase 6.1.

## 8. Governance sync actions (pre-implementation)

| Action ID | Reason / linked concern | Target files (absolute) | Required edit | Verification |
|---|---|---|---|---|
| **GS-001** | Concern 1 — `tauri-plugin-dialog` added to backend; rule must allow it. | `C:\Users\chris\source\repos\arx-runa\.claude\rules\tauri.md` | In the §Plugins bullet that currently lists `tauri-plugin-shell`/`tauri_plugin_opener` exceptions, append: *"`tauri-plugin-dialog` is allowed for native open/save file pickers scoped to the `dialog:allow-open` permission; `dialog:allow-save` is allowed only where a command-contracted destination path is required."* | `grep -n "tauri-plugin-dialog" .claude/rules/tauri.md` returns the new line inside §Plugins. |
| **GS-002** | Copilot mirror of GS-001. | `C:\Users\chris\source\repos\arx-runa\.github\instructions\tauri.instructions.md` | Run `/copilot-sync` after GS-001 lands. | `diff` between the two files shows only the formatting/frontmatter difference expected by the mirror convention. |

If `.github/instructions/tauri.instructions.md` already exists (verified: it does), `/copilot-sync` overwrites; no manual edit is required beyond invoking the skill.

## 9. Handoff Notes for Implementer

Working directory is `C:\Users\chris\source\repos\arx-runa`. This plan is self-contained — re-reading the sub-phase file is not required, but the **contract snippets CS-001…CS-017** are the source of truth for bodies and wire shapes; do not improvise. Execute **§8 governance-sync actions first** (GS-001, GS-002) before code edits. Then follow **§5 Implementation Steps 1 → 21 in order**: dependency changes first (they gate compilation), then backend plugin wiring, then CSS token, then new frontend modules bottom-up (externs → DTOs → components → auth → vault → transfer → layout), then `app.rs` last because it closes the circuit. Traps: (a) Every new request DTO needs `#[serde(rename_all = "camelCase")]` — Phase 6.2 convention; (b) The `wasm-bindgen` externs for the dialog/channel/webview surfaces require the fully qualified `js_namespace` path per the JS API — do not guess shorter paths; (c) Password zeroisation must touch *both* the local `String` and the Leptos signal (`set_password.update(|s| s.zeroize())`); missing the signal is a Zero-Trace regression; (d) Routing `Effect` must compute `prev == Some(true) && !now` before `vault_actions.clear()`/`sync_actions.clear()` — a naive re-fire on every tick would thrash state; (e) Do **not** add `tauri-plugin-dialog` to the `capabilities/default.json` by itself — it must pair with the plugin registration in `src-tauri/src/lib.rs` or Tauri emits a runtime warning; (f) `LoginPage`/`VaultCreationPage` call IPCs that currently return `InternalError("command not yet wired")` — verify the sanitised error surfaces in the UI without panicking; this is the 6.3 acceptance state; (g) Any touch of `src-tauri/src/{crypto,auth,storage}/` is a Plan Deviation — stop and escalate. Validation closure: `trunk build` must succeed warning-free and `cargo test --workspace --all-targets --all-features` must pass all new tests with zero backend regressions.
