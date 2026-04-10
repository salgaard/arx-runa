# Arx Runa: Project Scaffolding — Critical Review

> **Document type**: Exploration / feasibility research
> **Status**: Concluded
> **Last updated**: 2026-04-08

Critical review of `docs/architecture/designs/project-scaffolding/design.md` against
current crate versions, Rust edition 2024 constraints, ecosystem compatibility, and
completeness of the declared dependency set.

For the canonical design, see `docs/architecture/designs/project-scaffolding/design.md`.

---

## Table of Contents

1. [The Problem](#the-problem)
2. [Tailwind CSS Version](#tailwind-css-version)
3. [Frontend Dependency Completeness](#frontend-dependency-completeness)
4. [Workspace Layout and Alternatives](#workspace-layout-and-alternatives)
5. [Tauri v2 Capability Model](#tauri-v2-capability-model)
6. [Rust Edition 2024 — Resolver v3](#rust-edition-2024--resolver-v3)
7. [Stale Dependency Versions](#stale-dependency-versions)
8. [RustCrypto Digest Suite](#rustcrypto-digest-suite)
9. [Recommendation](#recommendation)
10. [Decisions](#decisions)
11. [Open Questions](#open-questions)
12. [Sources](#sources)

---

## The Problem

Phase 0 is the foundation for all subsequent implementation phases. A scaffolding design with stale crate versions, ambiguous version ranges, or missing dependencies will cause compile failures the moment Phase 1 implementation begins. The crypto and auth reviews already found and fixed stale `rand` and `secrecy` versions in those design documents — this review applies the same correctness lens to the scaffolding design itself, which is the single authoritative list of declared dependencies.

---

## Tailwind CSS Version

### What the design chose

The design listed "3.x / 4.x" for Tailwind CSS without committing to either version. The Trunk pre-build hook and `tailwind.config.js` were written in the v3 style, while the version table claimed dual compatibility.

### Why the ambiguity is a build failure

Tailwind v4 split the CLI into a separate package. Running the v3 command against a v4 install fails immediately:

| Property | v3 | v4 |
|---|---|---|
| CLI package | `tailwindcss` (includes CLI) | `@tailwindcss/cli` (separate install) |
| Trunk hook command | `npx tailwindcss -i input.css -o output.css` | `npx @tailwindcss/cli -i input.css -o output.css` |
| Config file | `tailwind.config.js` (JavaScript) | `@theme { }` block in CSS; JS config is opt-in via `@config` |
| Import syntax | `@tailwind base; @tailwind utilities;` | `@import "tailwindcss";` |
| Dark mode default | Class-based (`dark:` attribute on `<html>`) | Media query (`prefers-color-scheme: dark`) |
| Build engine | Node.js / PostCSS | Rust (Lightning CSS) — 3–8× faster cold builds |

Running `npx tailwindcss -i ...` on a v4 install outputs `Error: Unknown argument: -i` and exits non-zero, which fails the Trunk build before any WASM compilation begins.

### The brand design system changes the calculus

The Arx Runa brand tokens in `docs/arx-runa-brand.css` are already written as CSS custom properties — exactly the format v4's `@theme` block expects. The existing design described a different palette (`void-*`, `accent-*`, Inter, JetBrains Mono) that does not match the actual brand. Both need to change.

**v4 `@theme` is a near-verbatim transcription of the brand CSS:**

```css
/* input.css */
@import "tailwindcss";

@theme {
  /* Surfaces */
  --color-iron:  #09090B;   /* bg-iron, darkest page fill */
  --color-stone: #0C0E14;   /* bg-stone, card surfaces */
  --color-steel: #222736;   /* border-steel */
  --color-rune:  #5C7090;   /* text-rune, accent / logomark */
  --color-bone:  #DBD7CD;   /* text-bone, primary text */

  /* Text scale */
  --color-text-primary:   #DBD7CD;
  --color-text-secondary: #9AA3B0;
  --color-text-muted:     #636D7E;
  --color-text-ghost:     #3E4A5E;

  /* Surface scale */
  --color-surface-base:    #09090B;
  --color-surface-raised:  #0C0E14;
  --color-surface-overlay: #111420;

  /* Border scale */
  --color-border-subtle:  #181C26;
  --color-border-default: #1C2030;
  --color-border-strong:  #222736;

  /* Typography */
  --font-display: 'Helvetica Neue', Helvetica, Arial, sans-serif;
  --font-body:    Georgia, 'Times New Roman', serif;
  --font-mono:    'Courier New', Courier, monospace;
}
```

These become `bg-iron`, `text-bone`, `border-steel`, `text-rune`, `font-mono` as Tailwind utilities — no JS translation layer.

With v3, the same tokens require a `tailwind.config.js` that re-declares every hex value in an `extend.colors` block — a manual duplication of the brand CSS that must be kept in sync.

### Verdict

**Status: Fixed. Committed to Tailwind CSS v4.**

- Technology versions table updated: "4.x"
- Trunk hook updated: `@tailwindcss/cli`
- `tailwind.config.js` replaced with `input.css` using `@import "tailwindcss"` + `@theme {}`
- Color palette replaced: `void-*` / `accent-*` → `iron` / `stone` / `steel` / `rune` / `bone` from `docs/arx-runa-brand.css`
- Font stack replaced: Inter / JetBrains Mono → Helvetica Neue (display) / Georgia (body) / Courier New (mono)

---

## Frontend Dependency Completeness

### What the design declared

Two frontend crates: `leptos = "0.8"` (csr feature) and `console_error_panic_hook = "0.1"`.

### What was missing

The official Leptos CSR template and the tauri-ipc design both require additional crates not listed in the scaffolding dep table:

| Crate | Version | Why needed | Design reference |
|---|---|---|---|
| `leptos_meta` | `"0.8"` | Manages `<title>`, `<meta>` tags from within Leptos components | Every official Leptos CSR template |
| `leptos_router` | `"0.8"` | Client-side routing between vault browser views (login, vault, file list) | Standard in all CSR templates; required for Phase 6 UI |
| `console_log` | `"1"` | Routes `log::*` macros to the browser console in WASM builds | Standard Leptos template setup |
| `log` | `"0.4"` | Logging facade providing `log::info!`, `log::error!` etc. | Pair with `console_log` |
| `serde-wasm-bindgen` | `"0.6"` | `to_value`/`from_value` for Tauri IPC bridge — used directly in the tauri-ipc design's `invoke_command` function | `docs/architecture/designs/tauri-ipc-and-frontend/design.md` Phase 6 IPC code |

`wasm-bindgen` and `wasm-bindgen-futures` are transitive dependencies of `leptos` and do not need explicit declarations.

Without these, the Leptos template generated by `create-tauri-app` will not compile as-is (it imports `leptos_meta` and `leptos_router`), and Phase 6 IPC code will fail to compile with a `serde_wasm_bindgen` unresolved import.

**Status: Fixed. All five crates added to the frontend dep table.**

---

## Workspace Layout and Alternatives

### What the design chose

A package+workspace manifest at the root — `Cargo.toml` contains both `[package]` (the Leptos frontend crate, Trunk's build target) and `[workspace]` (declaring `src-tauri` as a member). The stated rationale was that Trunk requires a `[package]` section and cannot target a virtual manifest.

### Why the rationale was wrong

Trunk's only requirement is that its working directory contains a `Cargo.toml` with a `[package]` section identifying the crate to compile to WASM. It does not require that same `Cargo.toml` to also be the workspace root. A virtual manifest layout is equally valid:

| Pattern | Root Cargo.toml | Frontend location | Trunk invocation |
|---|---|---|---|
| **Package+workspace** (current) | `[package]` + `[workspace]` | `src/` (root package) | Run Trunk from root |
| **Virtual manifest** | `[workspace]` only | `frontend/Cargo.toml` (workspace member) | Run Trunk from `frontend/` |

The package+workspace choice is correct — it matches the `create-tauri-app` Leptos template convention and keeps the frontend at the project root. The rationale, however, was inaccurate: the real reason is template convention and developer ergonomics, not a Trunk constraint.

**Status: Design choice unchanged; rationale corrected.**

---

## Tauri v2 Capability Model

### What the design says

"Phase 0 creates a minimal default capability. Permissions are expanded per-phase as Tauri commands are added (Phase 6)."

### What was missing

In Tauri v2, the default capability grants the frontend access to **all registered commands** — not a subset. This means a Phase 0 scaffold with the default capability, if deployed as-is, would allow the frontend to call every Tauri command without restriction. This is acceptable during development but must not reach production.

Phase 6 must replace the default capability with per-command capability files scoped to the window that needs each command. The tauri-ipc design covers the command surface; a note was missing in the scaffolding design that the default capability is a development scaffold only.

**Status: Note added to the Capabilities section of design.md.**

---

## Rust Edition 2024 — Resolver v3

The design's Edition 2024 considerations table states: "Resolver v3 (default) — Workspace inherits resolver from edition; compatible with all target deps."

This claim is **correct**. Rust edition 2024 implies `resolver = "3"` in `Cargo.toml`, which enables the MSRV-aware (Minimum Supported Rust Version) dependency resolver. Key behaviour: when a dependency version is incompatible with the project's declared MSRV, the resolver falls back to an older compatible version rather than erroring. This requires Rust ≥ 1.84 (edition 2024 itself requires Rust ≥ 1.85).

**Verdict: No change needed.** The design is accurate.

---

## Stale Dependency Versions (`rand` and `rusqlite`)

### What the design specified

`rand = "0.9"` and `rusqlite = "0.34"`.

### `rand`

`rand` 0.10.0 was released 2026-02-08 as stable. The `"0.9"` semver range is exclusive at the major boundary and will not resolve 0.10. The crypto review documented the 0.8→0.9 API change (`thread_rng().gen()` → `rng().random()`). Whether `rand` 0.10 introduced further API changes should be verified when Phase 1 implementation begins, but the design dep should declare the current major so Phase 1 code is written and compiled against 0.10 docs.

The edition 2024 `gen` keyword constraint established in the crypto review requires `rand >= 0.9`; `"0.10"` satisfies this.

### `rusqlite`

`rusqlite` 0.39.0 is current. Breaking changes between 0.34 and 0.39 include:

- **0.35**: `Connection::execute` now rejects queries with trailing content; `prepare` rejects multiple statements in one call
- **0.38**: `u64` and `usize` `ToSql`/`FromSql` implementations removed by default (opt-in via feature flag); statement cache made optional; minimum SQLite version raised to 3.34.1

Phase 0 does not use `rusqlite` — the dep is declared upfront so Phase 3 picks up the right version from the start. Updating the range to `"0.39"` now means Phase 3 code is authored against the current API and docs.

**Status: Fixed. `rand = "0.10"`, `rusqlite = "0.39"`.**

---

## RustCrypto Digest Suite (`hkdf` and `sha2`)

### What the design chose

`hkdf = "0.12"` and `sha2 = "0.10"` — the correct pairing for the `0.10`/`0.12` generation of the RustCrypto digest suite.

### The problem

Both crates bumped to new major versions: `hkdf 0.13.0` and `sha2 0.11.0`. Cargo's semver ranges are exclusive at the major boundary — `"0.12"` resolves `>= 0.12.0, < 0.13.0` and will never pick up 0.13. A developer reading docs.rs (which shows the latest — 0.13 and 0.11) will write code against those API signatures, then get compile errors because Cargo resolved 0.12 and 0.10. This is the same class of bug as `rand 0.8` found in the cryptographic primitives review.

### Why they must be updated together

`hkdf` depends on `sha2` via the `digest` crate's HMAC trait hierarchy. The correct pairings are:

| Pairing | Resolves from design spec | Current docs.rs |
|---|---|---|
| `hkdf = "0.12"` + `sha2 = "0.10"` | Yes | No — shows 0.13 / 0.11 |
| `hkdf = "0.13"` + `sha2 = "0.11"` | No | Yes ✓ |

Mixing versions forces Cargo to link two copies of the underlying digest trait objects. Passing a `sha2::Sha256` (from the version in your code) to `Hkdf::<Sha256>::new` (from a different version) produces a type mismatch compile error, because Rust treats `sha2_0_10::Sha256` and `sha2_0_11::Sha256` as distinct types.

### Compatibility with the rest of the suite

`argon2 = "0.5"` and `chacha20poly1305 = "0.10"` depend on `sha2` internally, but their transitive dependencies are managed by Cargo independently. Providing an explicit `sha2 = "0.11"` in `Cargo.toml` does not force those crates to upgrade — they resolve their own dependency in isolation. The conflict only arises when types cross the version boundary at the call site, which doesn't happen between `sha2` in user code and `sha2` inside `argon2`'s internals.

**Status: Fixed. `hkdf = "0.13"`, `sha2 = "0.11"`.**

---

## Recommendation

The project scaffolding design was structurally sound — workspace layout, Tauri v2, Leptos 0.8, Trunk, and SQLCipher bundling decisions all hold. The review found no architectural problems and no security issues. What it did find was a cluster of stale dependency versions and one omitted compatibility detail (Tailwind v3/v4) that would each cause compile or build failures at the start of the relevant implementation phase.

Nine changes were made:

| # | Finding | Severity | Resolution |
|---|---------|----------|------------|
| 1 | Tailwind "3.x / 4.x" ambiguity; Trunk hook fails with v4 | Bug | Committed to v4; updated hook to `@tailwindcss/cli`; replaced `void-*`/`accent-*` palette with brand tokens (iron/stone/steel/rune/bone) from `docs/arx-runa-brand.css` |
| 2 | `hkdf = "0.12"` — major bump to 0.13 | Bug | Updated to `"0.13"` |
| 3 | `sha2 = "0.10"` — major bump to 0.11 | Bug | Updated to `"0.11"` |
| 4 | Frontend dep table missing `leptos_meta`, `leptos_router`, `console_log`, `log`, `serde-wasm-bindgen` | Gap | All five added to root `Cargo.toml` dep table |
| 5 | `rand = "0.9"` — 0.10.0 stable | Improvement | Updated to `"0.10"` |
| 6 | `rusqlite = "0.34"` — 0.39.0 stable, breaking changes | Improvement | Updated to `"0.39"` |
| 7 | Workspace layout rationale inaccurate | Improvement | Corrected: package+workspace is template convention, not a Trunk constraint. Choice unchanged. |
| 8 | Tauri v2 default capability scope undocumented | Gap | Note added: default capability is dev-only; Phase 6 must tighten to per-command granularity |
| 9 | Resolver v3 claim | Note | Verified correct — no change |

The design is now ready for Phase 0 implementation.

---

## Decisions

| Decision | Alternatives considered | Rationale |
|---|---|---|
| Tailwind CSS v4 | v3 (existing hook and config work as-is) | Brand CSS (`arx-runa-brand.css`) is already in CSS custom property format — maps directly to v4 `@theme` with no JS translation layer. v3 would require duplicating hex values into `tailwind.config.js`. |
| `hkdf = "0.13"`, `sha2 = "0.11"` | Keep `"0.12"` / `"0.10"` | Major version bumps in both; docs.rs shows 0.13/0.11 API. Mismatch causes Phase 1 compile errors when user code follows current docs against design-specified older version. |
| All five frontend deps declared in Phase 0 | Defer `serde-wasm-bindgen` to Phase 6 | Consistent with upfront dep declaration pattern used for backend crypto/storage crates; prevents Phase 6 surprise compile failure. |
| `rand = "0.10"` | Keep `"0.9"` | rand 0.10 stable since 2026-02-08; 0.9 won't resolve 0.10; Phase 1 should be written against current docs. |
| `rusqlite = "0.39"` | Keep `"0.34"` | Breaking changes in 0.35 and 0.38; Phase 3 code should target current API and not require post-hoc fixes. |
| `tokio = { version = "1", features = ["full"] }` | `rt-multi-thread, macros` only | `full` is standard Tauri practice; eliminates any risk of missing feature mid-development. Attack surface difference is minimal for a desktop app. |
| Package+workspace rationale corrected | — | Design choice unchanged; stated rationale (Trunk requires [package]) was inaccurate. Real reason: template convention and root-level ergonomics. |
| Tauri v2 default capability note added | — | Documented that Phase 0 default capability is dev-only and must be tightened per-command in Phase 6. |

---

## Open Questions

---

## Sources

| Source | Topic | URL |
|---|---|---|
| Tailwind CSS Upgrade Guide | v3 → v4 migration, CLI separation, @theme syntax | https://tailwindcss.com/docs/upgrade-guide |
| Tailwind CSS v4.0 Release Blog | v4 feature overview, Oxide engine, production readiness | https://tailwindcss.com/blog/tailwindcss-v4 |
| Tailwind CSS CLI Docs | @tailwindcss/cli install and usage | https://tailwindcss.com/docs/installation/tailwind-cli |
| hkdf crates.io | Latest version (0.13.0) | https://crates.io/crates/hkdf |
| sha2 crates.io | Latest version (0.11.0) | https://crates.io/crates/sha2 |
| Leptos start-trunk template | Canonical frontend dep list for CSR Leptos | https://github.com/leptos-rs/start-trunk |
| serde-wasm-bindgen crates.io | Latest version (0.6.5) | https://crates.io/crates/serde-wasm-bindgen |
| rand crates.io | Latest version (0.10.0, stable 2026-02-08) | https://crates.io/crates/rand |
| rusqlite crates.io | Latest version (0.39.0), changelog | https://crates.io/crates/rusqlite |
| rusqlite changelog | Breaking changes 0.35 and 0.38 | https://github.com/rusqlite/rusqlite/blob/master/Changelog.md |
| Rust Edition Guide — Cargo resolver | Resolver v3 default for edition 2024, MSRV behaviour | https://doc.rust-lang.org/edition-guide/rust-2024/cargo-resolver.html |
| Announcing Rust 1.85.0 and Rust 2024 | Edition 2024 release, resolver v3 activation | https://blog.rust-lang.org/2025/02/20/Rust-1.85.0/ |
| Tauri v2 — Capabilities | Capability model, per-command permissions, default capability scope | https://v2.tauri.app/security/capabilities/ |
| Leptos start-trunk template — Cargo.toml | Virtual manifest alternative, canonical frontend dep list | https://github.com/leptos-rs/start-trunk/blob/main/Cargo.stable.toml |
