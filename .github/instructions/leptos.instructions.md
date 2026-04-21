---
applyTo: "src/**/*.rs"
---

# Leptos Frontend — rules

**Design specification**: `docs/architecture/designs/tauri-ipc-and-frontend/design.md` — last verified against design dated 2026-04-11

For patterns and examples, see `.claude/reference/leptos-patterns.md`.

## Reactivity
- Pass signals (not `.get()` values) for reactivity
- Derived signals (`move || ...`) for computed values — not Effects
- `.get()` clones, `.read()` borrows — prefer `.read()` for complex types

## Components
- Document every component/prop with `///`
- `#[prop(into)]` with `Signal<T>` for flexible props
- Component body runs ONCE — reactive logic goes in closures

## Async
- `LocalResource` for fetching, `Action` for mutations
- Wrap fallible async in `ErrorBoundary`

## Arx Runa constraints
- **Zero-Trace**: No localStorage/sessionStorage for sensitive data
- **Zero-Trace**: No IndexedDB and no service workers
- **Zero-Trace**: Never log keys, passwords, decrypted content
- **Zero-Trace**: Zeroize password strings before clearing UI state
- **Zero-Trace**: Clear UI state when vault locks
- All Tauri IPC via `invoke()` — backend handles crypto

## State contexts
- Hook pairs: use_session/use_session_actions, use_vault/use_vault_actions, use_sync/use_sync_actions — panic with "<Provider> must wrap the component tree" if the provider is missing.
- The Actions pattern (`VaultActions`, `SessionActions`, `SyncActions`) is the Zero-Trace enforcement point for each state context — every new field added to any state struct must be cleared in the corresponding `Actions::clear()` method
- Provider hierarchy: SessionProvider > VaultProvider > SyncProvider in src/app.rs.

