# VoidGate — Agent Memory

- [Bachelor report requirement](project_bachelor_report.md) — PBA report: 30 pages max, English, objective register, auto-captured via /report-note and commit hook
- [Architecture rationale](architecture_rationale.md) — Why each major design decision was made: rejected alternatives, trade-offs, references (XChaCha20, AAD, HKDF tree, USB auth, manifest, etc.)
- [Pending decisions](pending_decisions.md) — Open design questions: USB key file format
- [Known gotchas](known_gotchas.md) — Implementation pitfalls: crate tag doubling, AAD serialisation, vault header upload order, BLAKE3 scope

## Frontend Stack

**Decision (ADR-002):** Leptos (Rust + WASM) for frontend + Tailwind CSS for styling.

Key references:
- Rules: `.claude/rules/leptos.md` and `.github/instructions/leptos.instructions.md`
- Patterns: `.claude/reference/leptos-patterns.md`
- Design system: `docs/architecture/design-system.md`
- ADR: `docs/architecture-decisions/002-frontend-stack-selection.md`
- Research: `docs/architecture/frontend-stack-research.md`
- Tailwind config: `tailwind.config.js`
- Base styles: `src/styles.css`

### Leptos reactivity essentials
- Signals: `let (value, set_value) = signal(0);`
- Derived: `let doubled = move || value.get() * 2;`
- `.get()` clones, `.read()` borrows, `.with()` access via callback
- Pass signal (not `.get()`) to views for reactivity
- `LocalResource` for async data, `Action` for mutations
- `ErrorBoundary` for graceful error handling
- `provide_context` / `use_context` for global state

### Styling essentials
- Dark theme: `bg-void-950` (base), `bg-void-900` (cards), `bg-void-800` (elevated)
- Text: `text-void-50` (primary), `text-void-200` (secondary), `text-void-400` (muted)
- Accent: `accent-500` (teal for trust/security)
- Status: `text-secure` (green), `text-locked` (gray), `text-warning` (amber), `text-danger` (red)
- Component classes: `.btn-primary`, `.btn-secondary`, `.card`, `.input`, `.status-locked`
