---
applyTo: "src/**/*.rs"
---

# Leptos Frontend — rules

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

## VoidGate constraints
- **Zero-Trace**: No localStorage/sessionStorage for sensitive data
- **Zero-Trace**: Never log keys, passwords, decrypted content
- **Zero-Trace**: Clear UI state when vault locks
- All Tauri IPC via `invoke()` — backend handles crypto
