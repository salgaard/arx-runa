---
applyTo: "src/**/*.rs"
---

# Leptos Frontend — Scoped Rules

These rules apply to all Leptos frontend code under `src/` (the Tauri webview UI).

For detailed patterns and syntax examples, see `.claude/reference/leptos-patterns.md`.

## Reactivity rules

- Pass signals (not `.get()` values) to views for reactivity
- Use derived signals (`move || ...`) for computed values — not Effects
- Use `Memo` for expensive computations that should be memoized
- `.get()` clones, `.read()` borrows — prefer `.read()` for complex types

## Component rules

- Document every component and prop with `///` doc comments
- Use `#[prop(into)]` with `Signal<T>` for flexible reactive props
- Use `#[prop(default = ...)]` for optional props with defaults
- Component body runs ONCE (setup) — put reactive logic in closures

## Async rules

- Use `LocalResource` for data fetching (CSR context)
- Use `Action` for mutations with state tracking (pending, value)
- Use `spawn_local` for one-off async work without tracking
- Wrap fallible async in `ErrorBoundary` for graceful degradation

## State management

- Use `provide_context` / `use_context` for shared state
- Keep state as close to usage as possible — avoid global state sprawl
- For complex state, use `Store` derive macro with field-level reactivity

## VoidGate-specific constraints

- **Zero-Trace**: Never use `localStorage`/`sessionStorage` for sensitive data
- **Zero-Trace**: Never log keys, passwords, or decrypted content
- **Zero-Trace**: Clear UI state (file lists, previews) when vault locks
- Use `zeroize` crate for sensitive data in WASM — it works correctly
- All Tauri IPC via `invoke()` — backend handles crypto, frontend only displays

## Styling rules

VoidGate uses Tailwind CSS. See `docs/architecture/design-system.md` for the full system.

- **Dark theme always**: `bg-void-*` backgrounds, `text-void-*` text
- **Security states must be obvious**: locked (gray), unlocked (green), warning (amber)
- **Focus rings required**: Always include for accessibility
- **Minimal animations**: `transition-colors` only — no bouncy effects
- **Monospace for paths**: Use `font-mono` for file names and technical data

## Required tests

- Extract business logic to plain Rust structs — test logic without components
- Test error states and loading states explicitly
- Verify sensitive data is cleared on vault lock
