---
applyTo: "src/**/*.rs"
---

# Leptos Frontend

> Design: `docs/architecture/designs/tauri-ipc-and-frontend/design.md`; patterns: `.claude/reference/leptos-patterns.md`

- Pass signals (not `.get()` values) for reactivity; derived signals (`move || ...`) for computed — not Effects
- `.get()` clones, `.read()` borrows — prefer `.read()` for complex types
- `///` on every component/prop; `#[prop(into)]` with `Signal<T>` for flexible props; component body runs ONCE — reactive logic in closures
- `LocalResource` for fetching, `Action` for mutations; wrap fallible async in `ErrorBoundary`
- Zero-Trace: no localStorage/sessionStorage/IndexedDB/service workers for sensitive data
- Zero-Trace: never log keys, passwords, or decrypted content; zeroize password strings before clearing UI state; clear UI state when vault locks
- All Tauri IPC via `invoke()` — backend handles crypto
- Hook pairs: `use_session/use_session_actions`, `use_vault/use_vault_actions`, `use_sync/use_sync_actions` — panic with `"<Provider> must wrap the component tree"` if provider missing
- Actions pattern (`VaultActions`, `SessionActions`, `SyncActions`) is the Zero-Trace enforcement point — every new field added to any state struct must be cleared in the corresponding `Actions::clear()`
- Provider hierarchy: `SessionProvider > VaultProvider > SyncProvider` in `src/app.rs`
