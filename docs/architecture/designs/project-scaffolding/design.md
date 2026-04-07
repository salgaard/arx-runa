# Arx Runa — Project Scaffolding Design

> Status: Design complete. Implementation target: Phase 0.
> Last updated: 2026-04-07
> **Sub-phase roadmap**: [`sub-phases/roadmap.md`](sub-phases/roadmap.md)

---

## Goals

- Establish a compilable Tauri v2 + Leptos project skeleton
- Define workspace layout, dependencies, and module structure
- Integrate Tailwind CSS build pipeline via Trunk
- Ensure `cargo fmt`, `cargo clippy -- -D warnings`, `cargo test`, and `cargo build --release` all pass
- Provide a stable foundation for all subsequent implementation phases

---

## Project Structure

### Workspace Layout

The project uses a Cargo workspace with `src-tauri/` as the sole workspace member. The Leptos frontend in `src/` compiles to WASM via Trunk and is not a Cargo workspace member.

```
arx-runa/
├── Cargo.toml                  # [workspace] with members = ["src-tauri"]
├── Cargo.lock
├── Trunk.toml                  # Trunk build config for Leptos frontend
├── index.html                  # Trunk entry point
├── input.css                   # Tailwind input (imports + directives)
├── tailwind.config.js          # Arx Runa custom theme
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

**Note**: The `sharing/` module is not created in Phase 0 — it is introduced in Phase 5 per the roadmap.

### Rationale

- **Workspace**: Allows future workspace members (e.g., a shared types crate) without restructuring. Standard Tauri pattern for v2.
- **Trunk as frontend builder**: Purpose-built for WASM, handles `index.html` processing, asset bundling, and Tailwind hooks. Simpler than `cargo-leptos` for CSR-only desktop apps.
- **`src/` outside workspace**: Trunk compiles `src/` independently to WASM. It does not participate in the Cargo workspace — Tauri orchestrates both builds via `cargo tauri dev`.

---

## Technology Versions

| Component | Version | Rationale |
|-----------|---------|-----------|
| Rust edition | 2024 | Latest edition; enables modern patterns. Requires `rand >= 0.9` (`gen` keyword reserved). |
| Tauri | 2.x | Current stable. Improved security model, official Leptos template. |
| Leptos | 0.8.x | Latest stable (0.8.17 at time of writing). Template generates 0.6 code — upgrade during scaffolding. |
| Trunk | latest | WASM bundler for Leptos. Handles index.html, Tailwind hooks, hot-reload. |
| Tailwind CSS | 3.x / 4.x | Utility-first CSS. Existing custom theme (void-\*, accent-\*, status colors) preserved. |

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
| `anyhow` | `"1"` | Error context propagation |

#### Cryptography (Phase 1+)

| Crate | Version | Purpose |
|-------|---------|---------|
| `chacha20poly1305` | `"0.10"` | XChaCha20-Poly1305 AEAD |
| `argon2` | `"0.5"` | Argon2id KDF |
| `hkdf` | `"0.12"` | HKDF-SHA256 key derivation |
| `sha2` | `"0.10"` | SHA-256 (HKDF dependency) |
| `blake3` | `"1"` | BLAKE3 checksums |
| `rand` | `"0.9"` | CSPRNG (`>= 0.9` required for edition 2024 — `gen` keyword) |
| `zeroize` | `"1"` | Memory zeroisation (with `derive` feature) |
| `secrecy` | `"0.10"` | `Secret<T>` wrappers |
| `x25519-dalek` | `"2"` | X25519 key exchange (Phase 5) |

#### Storage (Phase 3+)

| Crate | Version | Features | Purpose |
|-------|---------|----------|---------|
| `rusqlite` | `"0.34"` | `bundled-sqlcipher-vendored-openssl` | SQLCipher encrypted database |
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
| `console_error_panic_hook` | `"0.1"` | — | WASM panic messages in browser console |

**Note**: The root `Cargo.toml` serves dual duty — it defines the workspace and is the frontend crate compiled by Trunk. Trunk reads the `[[bin]]` or `[lib]` target and compiles it to WASM.

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
#[derive(Debug, Error)]
pub enum <Module>Error {
    // Variants added in implementation phases.
}
```

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
| `memory` | Memory protection utilities: mlock/VirtualLock wrappers, zeroisation helpers. |
| `ui` | Tauri IPC command handlers: input validation, error sanitisation, async command dispatch. |

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
command_arguments = ["tailwindcss", "-i", "input.css", "-o", "output.css"]
```

### Tailwind Integration

The existing `tailwind.config.js` is preserved with the Arx Runa custom theme:

- **void-\***: Blue-gray scale for backgrounds and text
- **accent-\***: Muted teal for interactive elements
- **Status**: `secure` (green), `locked` (gray), `warning` (amber), `danger` (red)
- **Fonts**: Inter (sans), JetBrains Mono (mono)

The `content` path scans `*.html` and `./src/**/*.rs` for Tailwind class usage.

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

Phase 0 creates a minimal default capability. Permissions are expanded per-phase as Tauri commands are added (Phase 6).

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
| 5 | Existing files | Generate from template, merge custom theme | Preserve as-is (risk build pipeline mismatch); Regenerate (lose theme) |
| 6 | SQLCipher strategy | `bundled-sqlcipher-vendored-openssl` | `bundled-sqlcipher` (needs system OpenSSL); system sqlcipher (hardest portability) |
| 7 | Dependency versioning | Semver ranges | Exact pinning (misses patches, unusual in Rust) |
| 8 | Module placeholders | Minimal (doc comments + types + error) | Stub traits (blurs Phase 0/1 boundary) |

---

## Acceptance Criteria

1. `cargo fmt --all -- --check` passes
2. `cargo clippy --all-targets --all-features -- -D warnings` passes
3. `cargo test --all-targets` passes (no tests yet, but compilation succeeds)
4. `cargo build --release` succeeds
5. `cargo tauri dev` launches a window showing the Leptos app
6. All five module directories exist with `mod.rs`, `error.rs`, and `types/mod.rs`
7. `src-tauri/src/lib.rs` declares all modules
8. Tailwind custom theme (void-\*, accent-\*, status colors) is present in the build

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
