# Arx Runa — Project Scaffolding Design

> Status: Design complete. Implementation target: Phase 0.
> Last updated: 2026-04-08
> **Sub-phase roadmap**: [`sub-phases/roadmap.md`](sub-phases/roadmap.md)

---

## Goals

- Establish a compilable Tauri v2 + Leptos project skeleton
- Define workspace layout, dependencies, and module structure
- Integrate Tailwind CSS build pipeline via Trunk
- Ensure `cargo fmt`, `cargo clippy -- -D warnings`, `cargo test`, and `cargo build --release` all pass
- Provide a stable foundation for all subsequent implementation phases

---

## Contract Surface

### Interface contract

- Root `Cargo.toml` is the package+workspace contract: frontend crate at `src/` (Trunk) plus `src-tauri/` workspace member.
- Phase 0 backend module scaffold is `mod.rs` + `error.rs` + `types/mod.rs` for `crypto`, `auth`, `storage`, `sync`, `memory`, and `ui`.
- Build entrypoints are `cargo tauri dev` (development) and `cargo tauri build` (production packaging).

### Data contract

- Canonical scaffold artifacts are `Cargo.toml`, `Cargo.lock`, `Trunk.toml`, `index.html`, `input.css`, `package.json`, `src-tauri/tauri.conf.json`, and `src-tauri/capabilities/*`.
- Placeholder module files keep only doc comments and type/error stubs until their implementation phases.
- Capability configuration is intentionally permissive in Phase 0 and tightened in Phase 6.

### Invariant contract

- Phase 0 establishes a compilable baseline where `cargo fmt`, `cargo clippy -- -D warnings`, `cargo test`, and `cargo build --release` succeed.
- Workspace layout and module naming are stable inputs for all later design phases.
- Cross-phase invariant reference: `docs/architecture/design-invariants.md`.

### Dependency contract

- Backend baseline dependencies include `tauri`, `tokio`, `serde`, `thiserror`, `async-trait`, `tracing`, `anyhow`, plus phase-specific crypto/storage crates listed below.
- Frontend baseline dependencies include `leptos`, `leptos_meta`, `leptos_router`, `console_error_panic_hook`, `console_log`, `log`, `serde-wasm-bindgen`, and `gloo-timers`.
- Toolchain contract is Tauri v2 + Leptos 0.8 + Trunk + Tailwind CSS v4.

---

## Project Structure

### Workspace Layout

The project uses a Cargo workspace where the root `Cargo.toml` is a **package+workspace manifest** — it defines both the frontend crate (compiled by Trunk to WASM) and declares `src-tauri/` as a named workspace member. A pure virtual workspace manifest cannot be targeted by Trunk, so the root must include a `[package]` section.

```
arx-runa/
├── Cargo.toml                  # [package] (frontend) + [workspace] with members = ["src-tauri"]
├── Cargo.lock
├── Trunk.toml                  # Trunk build config for Leptos frontend
├── index.html                  # Trunk entry point
├── input.css                   # Tailwind input (imports + @theme block)
├── package.json                # Tailwind CSS dependency
├── src/                        # Leptos frontend (WASM, built by Trunk)
│   ├── main.rs                 # Leptos mount point
│   └── app.rs                  # Root component
├── src-tauri/                  # Tauri backend (workspace member)
│   ├── Cargo.toml              # All Rust dependencies
│   ├── tauri.conf.json         # Tauri configuration
│   ├── capabilities/           # Tauri v2 capability files
│   ├── icons/                  # Application icons
│   ├── build.rs                # tauri_build::build()
│   └── src/
│       ├── main.rs             # Tauri entry point
│       ├── lib.rs              # Command registration, module declarations
│       ├── crypto/             # Phase 1
│       │   ├── mod.rs
│       │   ├── error.rs
│       │   └── types/
│       │       └── mod.rs
│       ├── auth/               # Phase 2
│       │   ├── mod.rs
│       │   ├── error.rs
│       │   └── types/
│       │       └── mod.rs
│       ├── storage/            # Phase 3
│       │   ├── mod.rs
│       │   ├── error.rs
│       │   └── types/
│       │       └── mod.rs
│       ├── sync/               # Phase 4 (cloud synchronisation)
│       │   ├── mod.rs
│       │   ├── error.rs
│       │   └── types/
│       │       └── mod.rs
│       ├── memory/             # Memory protection utilities
│       │   ├── mod.rs
│       │   └── error.rs
│       └── ui/                 # Phase 6 (Tauri IPC commands)
│           ├── mod.rs
│           └── error.rs
├── docs/                       # Documentation (unchanged)
├── .github/workflows/          # CI (already exists)
└── .claude/                    # AI rules and reference
```

**Note**: The `sharing/` module is not created in Phase 0 — it is introduced in Phase 5 per the roadmap, following the same `mod.rs` + `error.rs` + `types/mod.rs` placeholder pattern used here.

### Rationale

- **Workspace**: Allows future workspace members (e.g., a shared types crate) without restructuring. Standard Tauri pattern for v2.
- **Trunk as frontend builder**: Purpose-built for WASM, handles `index.html` processing, asset bundling, and Tailwind hooks. Simpler than `cargo-leptos` for CSR-only desktop apps.
- **`src/` outside workspace**: Trunk compiles `src/` independently to WASM. It does not participate in the Cargo workspace — Tauri orchestrates both builds via `cargo tauri dev`.
- **Package+workspace manifest**: The root `Cargo.toml` follows the `create-tauri-app` Leptos template convention. A pure virtual workspace manifest (containing only `[workspace]`) is also valid — Trunk can target any directory with a `[package]` section — but would require placing the frontend in a `frontend/` subdirectory and pointing Trunk at it. The package+workspace approach is used here for consistency with the official template and to keep the frontend at the root.

---

## Technology Versions

| Component | Version | Rationale |
|-----------|---------|-----------|
| Rust edition | 2024 | Latest edition; enables modern patterns. Requires `rand >= 0.9` (`gen` keyword reserved). Current dep: `"0.10"`. |
| Tauri | 2.x | Current stable. Improved security model, official Leptos template. |
| Leptos | 0.8.x | Latest stable (0.8.17 at time of writing). Template generates 0.6 code — upgrade during scaffolding. |
| Trunk | latest | WASM bundler for Leptos. Handles index.html, Tailwind hooks, hot-reload. |
| Tailwind CSS | 4.x | Utility-first CSS. Brand token palette (iron, stone, steel, rune, bone) defined in `input.css` `@theme` block, sourced from `docs/arx-runa-brand.css`. |

---

## Dependencies

### Backend (`src-tauri/Cargo.toml`)

All dependencies use semver ranges. `Cargo.lock` pins exact versions.

#### Core dependencies

| Crate | Version | Purpose |
|-------|---------|---------|
| `tauri` | `"2"` | Desktop application framework |
| `tauri-build` | `"2"` | Build script (build-dependency) |
| `serde` | `"1"` | Serialisation (with `derive` feature) |
| `serde_json` | `"1"` | JSON serialisation |
| `tokio` | `"1"` | Async runtime (with `full` feature) |
| `thiserror` | `"2"` | Error type derivation |
| `async-trait` | `"0.1"` | Dyn-safe async traits (`MetadataStore` in Phase 3, `CloudTransport` in Phase 4) |
| `tracing` | `"0.1"` | Structured logging (Phase 6 error logging before IPC sanitisation) |
| `anyhow` | `"1"` | Error context propagation (dev/test use; production code uses typed `thiserror` enums) |

#### Cryptography (Phase 1+)

| Crate | Version | Purpose |
|-------|---------|---------|
| `chacha20poly1305` | `"0.10"` | XChaCha20-Poly1305 AEAD |
| `argon2` | `"0.5"` | Argon2id KDF |
| `bip39` | `"2"` | Mnemonic recovery phrase generation (Phase 2) |
| `hkdf` | `"0.13"` | HKDF-SHA256 key derivation |
| `sha2` | `"0.11"` | SHA-256 (HKDF dependency) |
| `blake3` | `"1"` | BLAKE3 checksums |
| `rand` | `"0.10"` | CSPRNG (`>= 0.9` required for edition 2024 — `gen` keyword; 0.10 is current stable) |
| `zeroize` | `"1"` | Memory zeroisation (with `derive` feature) |
| `secrecy` | `"0.10"` | `Secret<T>` wrappers |
| `x25519-dalek` | `"2"` | X25519 key exchange (Phase 5) |

#### Storage (Phase 3+)

| Crate | Version | Features | Purpose |
|-------|---------|----------|---------|
| `rusqlite` | `"0.39"` | `bundled-sqlcipher-vendored-openssl` | SQLCipher encrypted database |
| `uuid` | `"1"` | `v4`, `serde` | Blob naming |

#### Dev dependencies

| Crate | Version | Purpose |
|-------|---------|---------|
| `proptest` | `"1"` | Property-based testing |
| `tempfile` | `"3"` | Temporary directories in tests |
| `assert_matches` | `"1"` | Pattern matching assertions |

### Frontend (`src/` — compiled by Trunk)

The frontend Leptos code requires its own `Cargo.toml` at the project root (or Trunk configuration pointing to it). Dependencies:

| Crate | Version | Features | Purpose |
|-------|---------|----------|---------|
| `leptos` | `"0.8"` | `csr` | Leptos framework (client-side rendering) |
| `leptos_meta` | `"0.8"` | — | `<title>`, `<meta>` tag management in Leptos components |
| `leptos_router` | `"0.8"` | — | Client-side routing between vault browser views |
| `console_error_panic_hook` | `"0.1"` | — | WASM panic messages in browser console |
| `console_log` | `"1"` | — | Routes `log::*` macros to browser console in WASM |
| `log` | `"0.4"` | — | Logging facade (`log::info!`, `log::error!`, etc.) |
| `serde-wasm-bindgen` | `"0.6"` | — | `to_value`/`from_value` for Tauri IPC bridge (Phase 6) |
| `gloo-timers` | `"0.3"` | — | WASM timer utilities for polling intervals (session timeout, sync status — Phase 6) |

**Note**: The root `Cargo.toml` is a **package+workspace manifest** — it contains both `[package]` (the frontend crate, compiled by Trunk to WASM) and `[workspace]` (declaring `src-tauri` as a member). See Workspace Layout rationale for the choice of this pattern over a virtual manifest.

---

## Module Placeholders

Each module follows the structure defined in ADR-001. Phase 0 creates minimal placeholders:

### `mod.rs` pattern

```rust
//! Arx Runa <module_name> module.
//!
//! <Brief description of what this module will contain.>

pub mod error;
pub mod types;
```

### `error.rs` pattern

```rust
//! Error types for the <module_name> module.

use thiserror::Error;

/// Errors produced by the <module_name> module.
#[non_exhaustive]
#[derive(Debug, Error)]
pub enum <Module>Error {
    // Variants added in implementation phases.
}
```

`#[non_exhaustive]` is chosen over `#[allow(dead_code)]` because all later designs mark their error enums `#[non_exhaustive]`. Committing to it here avoids a blanket edit when Phase 1 starts adding variants.

### `types/mod.rs` pattern

```rust
//! Domain types for the <module_name> module.

// Newtypes added in implementation phases.
```

### Module descriptions

| Module | Doc comment |
|--------|-------------|
| `crypto` | Cryptographic primitives: key derivation, chunk encryption, file key management, BLAKE3 checksums. |
| `auth` | Authentication and session management: Argon2id KDF, USB key file, session lifecycle, memory locking. |
| `storage` | Storage layer: fixed-size chunking, SQLCipher manifest database, file-to-blob pipeline. |
| `sync` | Cloud synchronisation: provider-agnostic blob transport, push/pull flows, vault header management. |
| `memory` | Memory protection utilities: mlock/VirtualLock wrappers, zeroisation helpers. Primary consumer: `auth` module (Phase 2) for session key memory locking. |
| `ui` | Tauri IPC command handlers: input validation, error sanitisation, async command dispatch. |

**Note**: The `sharing/` module (Phase 5) is not created in Phase 0. It will be added using the same placeholder pattern (`mod.rs`, `error.rs`, `types/mod.rs`).

---

## Build Pipeline

### Development

```bash
cargo tauri dev
```

This runs Trunk (frontend) and Cargo (backend) in parallel with hot-reload.

### Production

```bash
cargo tauri build
```

Trunk compiles Leptos to optimised WASM, then Tauri packages the desktop application.

### Trunk Configuration

```toml
# Trunk.toml
[build]
target = "index.html"

[[hooks]]
stage = "pre_build"
command = "npx"
command_arguments = ["@tailwindcss/cli", "-i", "input.css", "-o", "output.css"]
```

**Note**: Tailwind v4 separates the CLI from the main `tailwindcss` package. Install both:

```json
{ "devDependencies": { "tailwindcss": "^4", "@tailwindcss/cli": "^4" } }
```

### Tailwind Integration

Tailwind v4 uses a CSS-first configuration. There is no `tailwind.config.js`. The brand token palette is declared in `input.css` using an `@theme` block, sourced from `docs/arx-runa-brand.css`:

```css
/* input.css */
@import "tailwindcss";

@theme {
  /* Core palette */
  --color-iron:  #09090B;   /* darkest — page fill */
  --color-stone: #0C0E14;   /* card surfaces */
  --color-steel: #222736;   /* borders, dividers */
  --color-rune:  #5C7090;   /* primary accent, logomark */
  --color-bone:  #DBD7CD;   /* primary text */

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

Generated utility classes: `bg-iron`, `bg-stone`, `text-bone`, `text-rune`, `border-steel`, `font-mono`, etc. Tailwind v4 auto-detects class usage in `*.html` and `./src/**/*.rs` — no `content` path configuration required.

---

## Tauri Configuration

### `tauri.conf.json` key settings

| Setting | Value | Rationale |
|---------|-------|-----------|
| `identifier` | `com.arxruna.app` | Reverse-domain app identifier |
| `productName` | `Arx Runa` | Display name |
| `version` | `0.1.0` | Initial version |
| `frontendDist` | `"../dist"` | Trunk output directory |
| `devUrl` | `"http://localhost:1420"` | Trunk dev server |
| `beforeDevCommand` | `"trunk serve"` | Start Trunk dev server |
| `beforeBuildCommand` | `"trunk build"` | Build frontend before packaging |

### Capabilities

Phase 0 creates a minimal default capability. In Tauri v2 the default capability grants the frontend access to all registered commands — this is intentionally permissive for development.

**Phase 6 must tighten this.** Each Tauri command should be explicitly listed in a named capability file scoped to the window that needs it. The tauri-ipc design covers per-command capability configuration. The Phase 0 default capability is a dev scaffold only and must not ship as-is.

---

## Edition 2024 Considerations

| Issue | Mitigation |
|-------|-----------|
| `gen` keyword reserved | Use `rand >= 0.9` which renamed `.gen()` to avoid conflicts |
| Lifetime capture changes | Monitor in later phases; no closures returning `impl Trait` in Phase 0 |
| Resolver v3 (default) | Workspace inherits resolver from edition; compatible with all target deps |

---

## Decisions Made

| # | Decision | Choice | Alternatives Considered |
|---|----------|--------|------------------------|
| 1 | Workspace structure | Cargo workspace with `src-tauri/` member | Standalone `src-tauri/` (simpler but less flexible) |
| 2 | Tauri version | v2 (stable) | v1 (older, being phased out) |
| 3 | Leptos version | 0.8.x (latest) | 0.6 (template default, would need migration later) |
| 4 | Frontend build tool | Trunk | cargo-leptos (overkill for CSR-only) |
| 5 | Tailwind CSS version | v4 | v3 (hook worked as-is but requires a JS config that duplicates brand CSS tokens) |
| 6 | Tailwind theme | Brand tokens from `docs/arx-runa-brand.css` (iron/stone/steel/rune/bone) in `input.css` `@theme` block | Previous `void-*`/`accent-*` palette in `tailwind.config.js` (did not match actual brand) |
| 7 | SQLCipher strategy | `bundled-sqlcipher-vendored-openssl` | `bundled-sqlcipher` (needs system OpenSSL); system sqlcipher (hardest portability) |
| 8 | Dependency versioning | Semver ranges | Exact pinning (misses patches, unusual in Rust) |
| 9 | Module placeholders | Minimal (doc comments + types + error) | Stub traits (blurs Phase 0/1 boundary) |
| 14 | `sync/` module in Phase 0 | Included as placeholder — Phase 4 and Phase 6 both reference `sync::SyncError` | Add in Phase 4 (late creation breaks "all modules from Phase 0" convention) |
| 15 | `#[non_exhaustive]` on error enums | Chosen over `#[allow(dead_code)]` | All Phase 1-6 designs use `#[non_exhaustive]`; consistent from the start avoids bulk edits |
| 10 | `hkdf` + `sha2` versions | `"0.13"` + `"0.11"` | `"0.12"` + `"0.10"` (stale majors; docs.rs shows 0.13/0.11, causing Phase 1 compile errors) |
| 11 | Frontend dep completeness | All five deps declared in Phase 0 (`leptos_meta`, `leptos_router`, `console_log`, `log`, `serde-wasm-bindgen`) | Defer `serde-wasm-bindgen` to Phase 6 — consistent with upfront backend dep declaration pattern |
| 12 | `rand` version | `"0.10"` | `"0.9"` (stale major; 0.10 stable since 2026-02-08) |
| 13 | `rusqlite` version | `"0.39"` | `"0.34"` (stale; breaking changes in 0.35 and 0.38; Phase 3 code should target current API) |

---

## Acceptance Criteria

1. `cargo fmt --all -- --check` passes
2. `cargo clippy --all-targets --all-features -- -D warnings` passes
3. `cargo test --all-targets` passes (no tests yet, but compilation succeeds)
4. `cargo build --release` succeeds
5. `cargo tauri dev` launches a window showing the Leptos app
6. All six module directories exist with `mod.rs`, `error.rs`, and `types/mod.rs` (`crypto`, `auth`, `storage`, `sync`, `memory`, `ui`)
7. `src-tauri/src/lib.rs` declares all modules
8. Tailwind brand theme (iron, stone, steel, rune, bone palette) is present in the build output

---

## References

- ADR-001: Code Structure and Patterns — module layout and newtype conventions
- ADR-002: Frontend Stack Selection — Leptos rationale, Tailwind theme design
- [Tauri v2 — Create a Project](https://v2.tauri.app/start/create-project/)
- [Tauri v2 — Leptos Frontend Guide](https://v2.tauri.app/start/frontend/leptos/)
- [Tauri v2 — Project Structure](https://v2.tauri.app/start/project-structure/)
- [Leptos on crates.io](https://crates.io/crates/leptos) (v0.8.17)
- [Rust 2024 Edition Migration Guide](https://reintech.io/blog/rust-2024-edition-migration-guide)
- [rusqlite — bundled-sqlcipher features](https://crates.io/crates/rusqlite/)
