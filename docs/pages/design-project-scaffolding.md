# Project Scaffolding

Arx Runa is a Tauri v2 + Leptos desktop application. The project uses a Cargo workspace with a package+workspace root manifest, Trunk as the WASM build tool, and Tailwind CSS v4 for styling.

---

## Goals

- Compilable Tauri v2 + Leptos skeleton with defined workspace layout, dependencies, and module structure
- Tailwind CSS v4 build pipeline via Trunk
- `cargo fmt`, `cargo clippy -- -D warnings`, `cargo test`, and `cargo build --release` all pass
- Stable foundation for all subsequent implementation phases

---

## Contract Surface

### Interface

- Root `Cargo.toml` is the package+workspace contract: frontend crate at `src/` (Trunk) plus `src-tauri/` workspace member
- Backend module scaffold: `mod.rs` + `error.rs` + `types/mod.rs` for `crypto`, `auth`, `storage`, `sync`, `memory`, and `ui`
- Build entrypoints: `cargo tauri dev` (development) and `cargo tauri build` (production packaging)

### Data

Canonical scaffold artifacts: `Cargo.toml`, `Cargo.lock`, `Trunk.toml`, `index.html`, `input.css`, `package.json`, `src-tauri/tauri.conf.json`, `src-tauri/capabilities/*`.

### Invariants

- Phase 0 establishes a compilable baseline; all clippy warnings are errors
- Workspace layout and module naming are stable inputs for all later design phases

### Dependencies

- Backend: `tauri`, `tokio`, `serde`, `thiserror`, `async-trait`, `tracing` + phase-specific crypto/storage crates
- Frontend: `leptos`, `leptos_meta`, `leptos_router`, `console_error_panic_hook`, `serde-wasm-bindgen`, `gloo-timers`
- Toolchain: Tauri v2 + Leptos 0.8 + Trunk + Tailwind CSS v4

---

## Workspace Layout

```
arx-runa/
├── Cargo.toml              # [package] (frontend) + [workspace] { members = ["src-tauri"] }
├── Cargo.lock
├── Trunk.toml              # Trunk build config for Leptos frontend
├── index.html              # Trunk entry point
├── input.css               # Tailwind input (@import + @theme)
├── package.json            # Tailwind CSS CLI dependency
├── src/                    # Leptos frontend (WASM, built by Trunk)
│   ├── main.rs
│   └── app.rs
└── src-tauri/              # Tauri backend (Cargo workspace member)
    ├── Cargo.toml
    ├── tauri.conf.json
    ├── capabilities/
    ├── build.rs
    └── src/
        ├── main.rs
        ├── lib.rs
        ├── crypto/         # Phase 1
        ├── auth/           # Phase 2
        ├── storage/        # Phase 3
        ├── sync/           # Phase 4
        ├── memory/         # Memory protection utilities
        └── ui/             # Phase 6 — Tauri IPC commands
```

The root `Cargo.toml` follows the `create-tauri-app` Leptos template convention — a package+workspace manifest keeps the frontend at the root while declaring `src-tauri/` as a workspace member. Trunk compiles `src/` independently to WASM; Tauri orchestrates both builds via `cargo tauri dev`.

---

## Technology Stack

| Component | Version |
|-----------|---------|
| Rust edition | 2024 |
| Tauri | 2.x |
| Leptos | 0.8.x |
| Trunk | latest |
| Tailwind CSS | 4.x |

---

## Backend Dependencies

| Crate | Version | Purpose |
|-------|---------|---------|
| `tauri` | `"2"` | Desktop application framework |
| `tokio` | `"1"` | Async runtime |
| `serde` / `serde_json` | `"1"` | Serialisation |
| `thiserror` | `"2"` | Error type derivation |
| `async-trait` | `"0.1"` | Dyn-safe async traits |
| `tracing` | `"0.1"` | Structured logging |
| `chacha20poly1305` | `"0.10"` | XChaCha20-Poly1305 AEAD |
| `argon2` | `"0.5"` | Argon2id KDF |
| `bip39` | `"2"` | Recovery phrase encoding |
| `hkdf` | `"0.13"` | HKDF-SHA256 |
| `sha2` | `"0.11"` | SHA-256 |
| `blake3` | `"1"` | BLAKE3 checksums |
| `rand` | `"0.10"` | CSPRNG |
| `zeroize` | `"1"` | Memory zeroisation |
| `secrecy` | `"0.10"` | `SecretBox<T>` wrappers |
| `x25519-dalek` | `"2"` | X25519 identity key operations |
| `rusqlite` | `"0.39"` | SQLCipher (`bundled-sqlcipher-vendored-openssl`) |
| `uuid` | `"1"` | UUID v4 blob naming |
| `hpke` | `"0.13"` | DHKEM(X25519, HKDF-SHA256) for file sharing |

---

## Frontend Dependencies

| Crate | Version | Purpose |
|-------|---------|---------|
| `leptos` | `"0.8"` | CSR frontend framework |
| `leptos_meta` | `"0.8"` | `<title>`, `<meta>` management |
| `leptos_router` | `"0.8"` | Client-side routing |
| `console_error_panic_hook` | `"0.1"` | WASM panic messages |
| `console_log` | `"1"` | Routes `log::*` to browser console |
| `serde-wasm-bindgen` | `"0.6"` | Tauri IPC bridge |
| `gloo-timers` | `"0.3"` | WASM timer utilities |

---

## Tailwind Theme

Tailwind v4 uses a CSS-first configuration. Brand tokens are declared in `input.css` via an `@theme` block:

```css
@import "tailwindcss";

@theme {
  --color-iron:  #09090B;   /* darkest — page fill */
  --color-stone: #0C0E14;   /* card surfaces */
  --color-steel: #222736;   /* borders, dividers */
  --color-rune:  #5C7090;   /* primary accent, logomark */
  --color-bone:  #DBD7CD;   /* primary text */

  --color-text-primary:   #DBD7CD;
  --color-text-secondary: #9AA3B0;
  --color-text-muted:     #636D7E;

  --color-surface-base:    #09090B;
  --color-surface-raised:  #0C0E14;
  --color-surface-overlay: #111420;

  --color-border-subtle:  #181C26;
  --color-border-default: #1C2030;
  --color-border-strong:  #222736;

  --font-display: 'Helvetica Neue', Helvetica, Arial, sans-serif;
  --font-body:    Georgia, 'Times New Roman', serif;
  --font-mono:    'Courier New', Courier, monospace;
}
```

Utility classes (`bg-iron`, `text-rune`, etc.) are emitted on demand when referenced in `*.html` or `./src/**/*.rs`. No `tailwind.config.js` required.

Trunk runs Tailwind before each build via a pre-build hook calling the Tailwind CLI through Node.

---

## Module Placeholder Pattern

Each module uses the same `mod.rs` + `error.rs` + `types/mod.rs` structure. Phase 0 creates minimal placeholders; implementation phases fill in the variants.

```rust
// error.rs pattern
#[non_exhaustive]
#[derive(Debug, Error)]
pub enum ModuleError {
    // Variants added in implementation phases.
}
```

`#[non_exhaustive]` is chosen over `#[allow(dead_code)]` — consistent with all phase designs from the start.

---

## Related Documents

- [Cryptographic Primitives](design-cryptographic-primitives.md) — Phase 1 module
- [Authentication and Session Management](design-authentication.md) — Phase 2 module
- [Chunking and Manifest](design-chunking-and-manifest.md) — Phase 3 module
- [Cloud Synchronisation](design-cloud-synchronisation.md) — Phase 4 module
- [File Sharing](design-file-sharing.md) — Phase 5 module
- [Tauri IPC and Frontend](design-tauri-ipc-and-frontend.md) — Phase 6 module
