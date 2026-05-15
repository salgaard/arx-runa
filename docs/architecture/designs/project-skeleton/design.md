# Arx Runa — Project Skeleton

> Status: Living reference for the current project structure.
> Last updated: 2026-05-15
> **Sub-phase roadmap**: [`sub-phases/roadmap.md`](sub-phases/roadmap.md)

---

## Goals

This document is the canonical reference for the Arx Runa project skeleton: workspace layout, technology stack, dependency registry, module structure, and build pipeline. It reflects the actual implemented state and is updated as the project evolves.

---

## Contract Surface

### Interface contract

- Root `Cargo.toml` is the package+workspace contract: frontend crate at `src/` (Trunk) plus `src-tauri/` workspace member.
- Backend modules are `crypto`, `auth`, `storage`, `sync`, `memory`, `sharing`, and `ui` — all declared in `src-tauri/src/lib.rs`.
- Build entrypoints are `cargo tauri dev` (development) and `cargo tauri build` (production packaging).

### Data contract

- Canonical skeleton artifacts are `Cargo.toml`, `Cargo.lock`, `Trunk.toml`, `index.html`, `input.css`, `package.json`, `src-tauri/tauri.conf.json`, and `src-tauri/capabilities/*`.
- Capability configuration is intentionally permissive during development. Phase 6 tightens this to per-command capability files scoped to specific windows.

### Invariant contract

- `cargo fmt --all -- --check`, `cargo clippy --all-targets --all-features -- -D warnings`, and `cargo test --all-targets` must always pass.
- Seven module directories always exist under `src-tauri/src/`: `auth`, `crypto`, `memory`, `sharing`, `storage`, `sync`, `ui`.
- All modules are declared in `src-tauri/src/lib.rs`.
- Cross-phase invariant reference: [docs/architecture/design-invariants.md](../../design-invariants.md).

### Dependency contract

- Backend dependencies include `tauri`, `tokio`, `serde`, `thiserror`, `async-trait`, `tracing`, `reqwest`, and platform-specific crates listed in the Dependencies section.
- Frontend dependencies include `leptos`, `leptos_meta`, `leptos_router`, `wasm-bindgen`, `web-sys`, `serde-wasm-bindgen`, and `gloo-timers`.
- X25519 dependency strategy is single-stack: one crate family per major version (no parallel curve implementations for the same purpose).
- Toolchain contract is Tauri v2 + Leptos 0.8 + Trunk + Tailwind CSS v4.

---

## Project Structure

### Workspace Layout

The project uses a Cargo workspace where the root `Cargo.toml` is a **package+workspace manifest** — it defines both the frontend crate (compiled by Trunk to WASM) and declares `src-tauri/` as a named workspace member. A pure virtual workspace manifest cannot be targeted by Trunk, so the root must include a `[package]` section.

```
arx-runa/
├── Cargo.toml               # [package] (frontend WASM) + [workspace] with members = ["src-tauri"]
├── Cargo.lock
├── Trunk.toml               # Trunk build config for Leptos frontend
├── index.html               # Trunk entry point
├── input.css                # Tailwind input (imports + @theme block)
├── package.json             # Tailwind CSS CLI dependency
├── src/                     # Leptos frontend (WASM, built by Trunk)
│   ├── components/          # Reusable UI components
│   ├── ipc_types/           # Shared IPC request/response types
│   └── state/               # Leptos context providers (session, sync, vault)
├── src-tauri/               # Tauri backend (workspace member)
│   ├── Cargo.toml           # All Rust dependencies
│   ├── tauri.conf.json      # Tauri configuration
│   ├── capabilities/        # Tauri v2 capability files
│   ├── icons/               # Application icons
│   ├── build.rs             # tauri_build::build()
│   └── src/
│       ├── main.rs          # Tauri entry point
│       ├── lib.rs           # Command registration, module declarations
│       ├── auth/            # Authentication, sessions, device monitoring
│       ├── crypto/          # Cryptographic primitives
│       ├── memory/          # Secure memory (mlock/VirtualLock, zeroisation)
│       ├── sharing/         # File sharing (HPKE, packages, revocation, identity)
│       ├── storage/         # Chunking, SQLCipher manifest, blob pipeline
│       │   ├── cloud/       # Cloud sync, rclone subprocess, vault header
│       │   ├── pipeline/    # Encrypt/decrypt file pipelines
│       │   └── vault_ops/   # Upload, download, reencrypt, epoch flush
│       ├── sync/            # Sync error types
│       └── ui/              # Tauri IPC command handlers
├── bin/                     # Bundled external binaries
│   └── rclone               # Rclone binary (platform-specific, bundled by Tauri)
├── docs/
├── .github/workflows/
└── .claude/
```

### Rationale

- **Workspace**: Allows future workspace members (e.g., a shared types crate) without restructuring. Standard Tauri pattern for v2.
- **Trunk as frontend builder**: Purpose-built for WASM, handles `index.html` processing, asset bundling, and Tailwind hooks. Simpler than `cargo-leptos` for CSR-only desktop apps.
- **`src/` outside workspace**: Trunk compiles `src/` independently to WASM. It does not participate in the Cargo workspace — Tauri orchestrates both builds via `cargo tauri dev`.
- **Package+workspace manifest**: The root `Cargo.toml` follows the `create-tauri-app` Leptos template convention. A pure virtual workspace manifest (containing only `[workspace]`) is also valid — Trunk can target any directory with a `[package]` section — but would require placing the frontend in a `frontend/` subdirectory and pointing Trunk at it. The package+workspace approach is used here for consistency with the official template and to keep the frontend at the root.
- **`sync/` vs `storage/cloud/`**: Cloud synchronisation logic lives under `storage/cloud/` because it is tightly coupled to the storage layer (blob pipeline, vault header management). The `sync/` module holds shared error types consumed across the storage boundary.

---

## Technology Versions

| Component | Version | Rationale |
|-----------|---------|-----------|
| Rust edition | 2024 | Latest edition; enables modern patterns. Requires `rand >= 0.9` (`gen` keyword reserved). Current dep: `"0.10"`. |
| Tauri | 2.x | Current stable. Improved security model, official Leptos template. |
| Leptos | 0.8.x | Latest stable. CSR-only for desktop app. |
| Trunk | latest | WASM bundler for Leptos. Handles index.html, Tailwind hooks, hot-reload. |
| Tailwind CSS | 4.x | Utility-first CSS. Brand token palette (iron, stone, steel, rune, bone) defined in `input.css` `@theme` block, sourced from `docs/arx-runa-brand.css`. |

---

## Dependencies

### Backend (`src-tauri/Cargo.toml`)

All dependencies use semver ranges. `Cargo.lock` pins exact versions.

#### Core

| Crate | Version | Purpose |
|-------|---------|---------|
| `tauri` | `"2"` | Desktop application framework |
| `tauri-build` | `"2"` | Build script (build-dependency) |
| `tauri-plugin-dialog` | `"2"` | Native file open/save dialogs |
| `tauri-plugin-opener` | `"2"` | Open files and URLs with default system handler |
| `tauri-plugin-shell` | `"2"` | Shell command execution (rclone subprocess) |
| `serde` | `"1"` | Serialisation (with `derive` feature) |
| `serde_json` | `"1"` | JSON serialisation |
| `tokio` | `"1"` | Async runtime (`macros`, `rt-multi-thread`, `fs`, `io-util`, `sync`, `time`, `process`) |
| `futures-util` | `"0.3"` | Async stream and future utilities |
| `thiserror` | `"2"` | Error type derivation |
| `async-trait` | `"0.1"` | Dyn-safe async traits |
| `tracing` | `"0.1"` | Structured logging |
| `tracing-subscriber` | `"0.3"` | Log subscriber (stderr output in dev) |
| `regex` | `"1"` | Regex matching (rclone stderr sanitisation) |

#### Cryptography

| Crate | Version | Purpose |
|-------|---------|---------|
| `chacha20poly1305` | `"0.10"` | XChaCha20-Poly1305 AEAD |
| `chacha20` | `"0.9"` | ChaCha20 stream cipher (used directly for nonce generation) |
| `argon2` | `"0.5"` | Argon2id KDF |
| `bip39` | `"2"` | Mnemonic recovery phrase generation |
| `hkdf` | `"0.13"` | HKDF-SHA256 key derivation |
| `sha2` | `"0.11"` | SHA-256 |
| `blake3` | `"1"` | BLAKE3 checksums |
| `rand` | `"0.10"` | CSPRNG |
| `subtle` | `"2"` | Constant-time comparisons |
| `zeroize` | `"1"` | Memory zeroisation (with `derive` feature) |
| `secrecy` | `"0.10"` | `SecretBox<T>` wrappers |
| `x25519-dalek` | `"2"` | X25519 for HPKE key exchange; version aligned with sharing transitive deps |

#### Storage

| Crate | Version | Features | Purpose |
|-------|---------|----------|---------|
| `rusqlite` | `"0.39"` | `bundled-sqlcipher-vendored-openssl` | SQLCipher encrypted database |
| `uuid` | `"1"` | `v4`, `serde` | Blob naming |
| `base64` | `"0.22"` | — | Blob encoding |
| `hex` | `"0.4"` | — | Key hex encoding |

#### Cloud

| Crate | Version | Features | Purpose |
|-------|---------|----------|---------|
| `reqwest` | `"0.12"` | `rustls-tls`, `json` | HTTP client for cloud APIs (B2, GDrive) |

#### Auth / Runtime

| Crate | Version | Purpose |
|-------|---------|---------|
| `tokio-stream` | `"0.1"` | Async streams over tokio channels |
| `walkdir` | `"2"` | Recursive directory traversal (vault discovery) |
| `dirs` | `"6"` | Platform-standard directory paths |
| `tempfile` | `"3"` | Temporary files and directories (auth staging) |

#### Platform-specific

| Crate | Version | Platform | Purpose |
|-------|---------|---------|---------|
| `udev` | `"0.9"` | Linux | USB device monitoring |
| `wmi` | `"0.14"` | Windows | WMI device events |
| `windows` | `"0.59"` | Windows | Win32 API (memory locking, filesystem, security) |
| `core-foundation` | `"0.10"` | macOS | Core Foundation bindings |
| `core-foundation-sys` | `"0.8"` | macOS | Low-level Core Foundation FFI |
| `libc` | `"0.2"` | Unix | POSIX bindings (mlock) |

#### Dev dependencies

| Crate | Version | Purpose |
|-------|---------|---------|
| `proptest` | `"1"` | Property-based testing |
| `assert_matches` | `"1"` | Pattern matching assertions |
| `anyhow` | `"1"` | Error context helpers in tests; production paths remain typed `thiserror` |

### Frontend (`src/` — compiled by Trunk)

| Crate | Version | Features | Purpose |
|-------|---------|----------|---------|
| `leptos` | `"0.8"` | `csr` | Leptos framework (client-side rendering) |
| `leptos_meta` | `"0.8"` | — | `<title>`, `<meta>` tag management |
| `leptos_router` | `"0.8"` | — | Client-side routing |
| `wasm-bindgen` | `"0.2"` | — | Rust↔JS FFI |
| `wasm-bindgen-futures` | `"0.4"` | — | Async Rust↔JS promise bridge |
| `web-sys` | `"0.3"` | Window, HtmlInputElement, Event, DragEvent, DataTransfer, FileList, File | DOM APIs |
| `js-sys` | `"0.3"` | — | JavaScript built-in types |
| `console_error_panic_hook` | `"0.1"` | — | WASM panic messages in browser console |
| `console_log` | `"1"` | — | Routes `log::*` macros to browser console |
| `log` | `"0.4"` | — | Logging facade |
| `serde` | `"1"` | `derive` | Serialisation |
| `serde_json` | `"1"` | — | JSON (IPC payload encoding) |
| `serde-wasm-bindgen` | `"0.6"` | — | `to_value`/`from_value` for Tauri IPC bridge |
| `zeroize` | `"1"` | `alloc`, `derive` | Sensitive field zeroisation in WASM |
| `sha2` | `"0.10"` | — | SHA-256 in WASM (separate instance from backend `0.11`) |
| `base64` | `"0.22"` | — | Encoding utilities |
| `gloo-timers` | `"0.3"` | `futures` | WASM timer utilities (session timeout polling) |

---

## Module Structure

Each backend module follows the pattern from ADR-001. The table below describes each module's responsibility:

| Module | Description |
|--------|-------------|
| `crypto` | Cryptographic primitives: key derivation, chunk encryption, file key management, BLAKE3 checksums. |
| `auth` | Authentication and session management: Argon2id KDF, key file, session lifecycle, device monitoring, memory locking. |
| `storage` | Storage layer: fixed-size chunking, SQLCipher manifest database, file-to-blob pipeline, cloud sync via rclone. |
| `sync` | Shared sync error types consumed across the storage/cloud boundary. |
| `memory` | Memory protection utilities: mlock/VirtualLock wrappers, zeroisation helpers. |
| `sharing` | File sharing: HPKE key encapsulation, package creation, revocation, identity management. |
| `ui` | Tauri IPC command handlers: input validation, error sanitisation, async command dispatch. |

---

## Build Pipeline

### Development

```bash
cargo tauri dev
```

Runs Trunk (frontend) and Cargo (backend) in parallel with hot-reload.

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
command = "node"
command_arguments = ["./node_modules/@tailwindcss/cli/dist/index.mjs", "-i", "input.css", "-o", "output.css"]
```

**Note**: Tailwind v4 separates the CLI from the main `tailwindcss` package. Install both:

```json
{ "devDependencies": { "tailwindcss": "^4", "@tailwindcss/cli": "^4" } }
```

**Windows compatibility note**: Trunk hooks can fail to spawn `npx` reliably on Windows environments. Calling the CLI entrypoint through `node` is the portable default for this project.

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

`@theme` variables are always emitted to generated CSS. Utility classes such as `bg-iron` or `text-rune` are emitted on demand. Tailwind v4 auto-detects usage in `*.html` and `./src/**/*.rs` — no `content` path configuration required.

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
| Window size | 800×600px | Default window dimensions |
| CSP | ipc: enabled; connect-src http://ipc.localhost; script-src 'wasm-unsafe-eval'; asset: blob: data: | WASM requires `wasm-unsafe-eval`; IPC requires `ipc.localhost` |
| External binary | `bin/rclone` | Rclone bundled for cloud sync; path is platform-specific (Tauri resolves the correct binary at runtime) |

### Capabilities

Tauri v2 capability files are under `src-tauri/capabilities/`. The default capability grants the frontend access to all registered commands — intentionally permissive during development.

**Phase 6 must tighten this.** Each Tauri command should be explicitly listed in a named capability file scoped to the window that needs it. The tauri-ipc design covers per-command capability configuration.

**Enforcement mechanism**: release readiness requires:
1. `build.rs` command allowlist matches the canonical command surface (compile-time contract)
2. Capability audit check fails if a default capability grants unrestricted command access in release configuration

---

## Edition 2024 Considerations

| Issue | Mitigation |
|-------|-----------|
| `gen` keyword reserved | Use `rand >= 0.9` which renamed `.gen()` to avoid conflicts |
| Lifetime capture changes | Monitor in later phases; no closures returning `impl Trait` in the skeleton |
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
| 9 | Module placeholders | Minimal (doc comments + types + error) | Stub traits (blurs scaffold/implementation boundary) |
| 10 | `hkdf` + `sha2` versions | `"0.13"` + `"0.11"` | `"0.12"` + `"0.10"` (stale majors) |
| 11 | Frontend dep completeness | All deps declared upfront | Defer `serde-wasm-bindgen` to Phase 6 — consistent with upfront backend dep declaration pattern |
| 12 | `rand` version | `"0.10"` | `"0.9"` (stale major) |
| 13 | `rusqlite` version | `"0.39"` | `"0.34"` (stale; breaking changes in 0.35 and 0.38) |
| 14 | `sync/` vs `storage/cloud/` | Cloud sync logic under `storage/cloud/`; `sync/` holds shared error types | Putting all sync under a top-level `sync/` module (tighter coupling than warranted by the actual code boundary) |
| 15 | `#[non_exhaustive]` on error enums | Applied from the start | `#[allow(dead_code)]` — all Phase 1–6 designs use `#[non_exhaustive]`; consistent from the start avoids bulk edits |
| 16 | `tokio` feature policy | Explicit minimal runtime features | `full` feature flag (broader surface than needed) |
| 17 | `anyhow` placement | `dev-dependencies` only | Runtime dependency in `[dependencies]` (unnecessary in typed-error production modules) |
| 18 | X25519 crate strategy | Single-stack version alignment (`x25519-dalek` aligned with sharing transitive deps) | Parallel X25519 implementations with independent version drift risk |
| 19 | Capability tightening enforcement | Release gate requires command allowlist + capability audit check | Rely on manual reminder in Phase 6 only |

---

## Structural Invariants

These must hold at all times as the project evolves:

1. `cargo fmt --all -- --check` passes
2. `cargo clippy --all-targets --all-features -- -D warnings` passes
3. `cargo test --all-targets` passes
4. All seven module directories exist under `src-tauri/src/`: `auth`, `crypto`, `memory`, `sharing`, `storage`, `sync`, `ui`
5. All modules declared in `src-tauri/src/lib.rs`

---

## References

- ADR-001: Code Structure and Patterns — module layout and newtype conventions
- ADR-002: Frontend Stack Selection — Leptos rationale, Tailwind theme design
- [Tauri v2 — Create a Project](https://v2.tauri.app/start/create-project/)
- [Tauri v2 — Leptos Frontend Guide](https://v2.tauri.app/start/frontend/leptos/)
- [Tauri v2 — Project Structure](https://v2.tauri.app/start/project-structure/)
- [Leptos on crates.io](https://crates.io/crates/leptos)
- [Rust 2024 Edition Migration Guide](https://reintech.io/blog/rust-2024-edition-migration-guide)
- [rusqlite — bundled-sqlcipher features](https://crates.io/crates/rusqlite/)
