# Decision-002: Frontend Stack Selection

**Date:** 2026-03-30  
**Status:** Accepted

## Context

Arx Runa requires a frontend for Phase 6 (Tauri IPC + Frontend). The UI needs
are modest: login screen, vault browser, upload/download controls, and session
status indicator.

Key constraints:
- **Zero-Trace pillar**: Minimise forensic artifacts on host (RAM-based UI)
- **Developer context**: Familiar with Blazor/C#, learning Rust for backend
- **Bachelor project**: Time-constrained, demonstrable architectural decisions
- **Desktop-only**: No web deployment, offline operation

Options evaluated:
1. Svelte + Tailwind + DaisyUI
2. React + Tailwind + Radix UI
3. Leptos (Rust + WASM)
4. Vanilla HTML + CSS + TypeScript
5. Blazor (rejected: GC makes Zero-Trace difficult)

## Decision

**Leptos** — Rust-based frontend framework compiled to WebAssembly.

Stack:
```
Framework:    Leptos 0.8+
Styling:      Tailwind CSS (via Trunk build)
Components:   Thaw UI (if needed) or hand-rolled
Build:        Trunk or cargo-leptos
Tauri:        Official leptos template
```

## Rationale

### Single-language architecture

The entire Arx Runa codebase — crypto, auth, storage, and UI — uses Rust.
This provides:

- **No context-switching**: One language, one mental model, one toolchain
- **Type safety end-to-end**: Compiler catches errors across the full stack
- **Shared types**: Domain types (`FileId`, `VaultId`) usable in both backend
  and frontend without serialisation boundaries
- **Unified tooling**: `cargo fmt`, `cargo clippy`, `cargo test` for everything

### Zero-Trace superiority

Leptos compiles to WASM running in Tauri's webview. Sensitive data handling:

- `zeroize` crate works on frontend variables — keys can be zeroed in WASM
- No JavaScript runtime to audit for memory retention
- No garbage collector — deterministic memory management
- State stays in Rust; only rendered HTML crosses to the DOM

Comparison with JS frameworks:
- JavaScript strings are immutable and GC'd — cannot reliably zero
- JS frameworks may retain state in closures unpredictably
- Auditing JS bundle for data leaks is harder than auditing Rust

### Official Tauri support

Leptos is a first-class citizen in Tauri's `create-tauri-app`:
```bash
cargo create-tauri-app --template leptos
```

This provides:
- Pre-configured Trunk/cargo-leptos build integration
- Correct Tauri IPC bindings for WASM
- Working development workflow out of the box

### Fine-grained reactivity

Leptos uses SolidJS-inspired signals, not a virtual DOM:
- DOM updates are surgical — only changed elements re-render
- Better performance than React/Vue for complex UIs
- `Copy + 'static` signals provide ergonomic Rust code

### Bachelor report narrative

Choosing Leptos supports a coherent thesis:

> "Arx Runa demonstrates that a security-critical application can be built
> entirely in Rust, from cryptographic primitives to user interface, achieving
> type safety and memory control throughout the stack."

This is a stronger architectural statement than "we used React because it's
popular."

## Consequences

### Accepted trade-offs

1. **Larger bundle size**: ~200-250KB WASM overhead vs ~50KB for Svelte.
   Acceptable for a desktop app where startup time is less critical than web.

2. **Smaller ecosystem**: Thaw UI (574 ★) vs DaisyUI (32K ★). Arx Runa's UI
   is simple enough that hand-rolling components is feasible.

3. **Harder debugging**: WASM stack traces are less readable than JS. Mitigated
   by Rust's compile-time checks catching most errors before runtime.

4. **Slower iteration**: WASM compile times (~5-10s) vs Svelte hot reload (~1s).
   Acceptable for a focused development period.

5. **Learning curve**: Leptos patterns on top of Rust. Mitigated by existing
   Rust investment and official documentation quality.

### Risks

- **Leptos API stability**: Framework is pre-1.0, APIs may change. Mitigated by
  pinning dependency version and limited UI surface area.

- **Component library gaps**: May need to build custom components. Mitigated by
  simple UI requirements (login, file list, buttons).

### What we gain

- Single language across entire codebase
- Provable Zero-Trace compliance (no JS GC)
- Type-safe IPC between UI and backend
- Compelling architectural narrative for bachelor report

## Implementation notes

### Project setup
```bash
cargo create-tauri-app voidgate --template leptos
```

### Tailwind integration
```toml
# Trunk.toml
[build]
target = "index.html"

[[hooks]]
stage = "pre_build"
command = "npx"
command_arguments = ["tailwindcss", "-i", "input.css", "-o", "output.css"]
```

### Tauri command invocation from Leptos
```rust
use leptos::*;
use tauri_wasm::api::core::invoke;

#[component]
fn LoginForm() -> impl IntoView {
    let (password, set_password) = signal(String::new());
    
    let authenticate = move |_| {
        spawn_local(async move {
            let result = invoke::<_, ()>("authenticate", &password.get()).await;
            // Handle result
        });
    };
    
    view! {
        <input
            type="password"
            on:input=move |ev| set_password.set(event_target_value(&ev))
        />
        <button on:click=authenticate>"Unlock"</button>
    }
}
```

### Zero-Trace patterns
```rust
use zeroize::Zeroize;

// Sensitive data in signals can be zeroed
let (secret, set_secret) = signal(String::new());

// On session lock:
set_secret.update(|s| s.zeroize());
```

## References

- Leptos documentation: https://leptos.dev/
- Leptos GitHub: https://github.com/leptos-rs/leptos
- Tauri + Leptos template: https://github.com/tauri-apps/create-tauri-app
- Thaw UI (Leptos components): https://github.com/thaw-ui/thaw
- Fine-grained reactivity explanation: https://leptos-rs.github.io/leptos/
- Frontend stack research: `docs/report-log/2026-03-30-frontend-stack-research.md`
- Design system: `docs/architecture/design-system.md`

## Styling Decision (Addendum)

**Date:** 2026-03-30

### Decision

Tailwind CSS with a custom dark theme designed for privacy/security context.

### Rationale

1. **Utility-first**: Rapid prototyping, classes defined inline in Rust code
2. **Purging**: Unused CSS removed at build time — minimal bundle size
3. **Dark mode support**: Built-in class-based dark mode switching
4. **Rust integration**: Tailwind CLI scans `.rs` files for class names
5. **Leptos examples**: Official Leptos examples use Tailwind

### Theme Design

The Arx Runa theme emphasises:
- **Dark backgrounds**: Professional, privacy-focused aesthetic
- **Muted palette**: No bright/playful colors — calm, trustworthy
- **Clear security states**: Locked (gray), unlocked (green), warning (amber)
- **Minimal animations**: Function over form

Custom colors:
- `void-*`: Blue-gray scale for backgrounds and text
- `accent-*`: Muted teal for interactive elements (trust/security connotation)
- Status colors: `secure`, `locked`, `warning`, `danger`

### Implementation Files

- `tailwind.config.js`: Custom color palette and font configuration
- `src/styles.css`: Base styles and component classes
- `docs/architecture/design-system.md`: Full design system documentation
