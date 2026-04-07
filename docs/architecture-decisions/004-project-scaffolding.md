# ADR 004: Project Scaffolding — Workspace, Tauri v2, and Leptos 0.8

**Status**: Accepted
**Date**: 2026-04-07
**Decision makers**: Project lead

---

## Context

Arx Runa Phase 0 establishes the project skeleton. The existing codebase is a bare Rust binary (edition 2024) with design documents, ADRs, and a Tailwind theme, but no Tauri scaffolding. Several decisions were needed to define the project foundation.

This ADR records the scaffolding-level decisions. Code-level patterns (newtypes, module structure, file granularity) are in ADR-001. Frontend stack rationale (Leptos over Svelte/React) is in ADR-002.

---

## Decisions

### 1. Cargo Workspace with `src-tauri/` as Member

**Choice**: Top-level `Cargo.toml` defines `[workspace]` with `members = ["src-tauri"]`.

**Alternatives considered**:
- *Standalone `src-tauri/`* — remove top-level Cargo.toml entirely. Simpler but prevents future workspace members (e.g., shared types crate).

**Rationale**: Standard Tauri v2 pattern. Workspace provides flexibility at no cost — `Cargo.lock` is shared, and the top-level `Cargo.toml` also serves as the Trunk build target for the Leptos frontend.

### 2. Tauri v2

**Choice**: Target Tauri v2 (stable).

**Alternatives considered**:
- *Tauri v1* — older, simpler API, more community examples, but being phased out.

**Rationale**: v2 is current stable with an improved security model (capabilities system), official Leptos template support, and active development. v1 is in maintenance mode.

### 3. Leptos 0.8 (Upgrade from Template Default)

**Choice**: Use Leptos 0.8.x despite the `create-tauri-app` template generating 0.6 code.

**Alternatives considered**:
- *Leptos 0.6 (template default)* — known-good with Tauri v2 docs, zero template friction.
- *Generate then assess* — generate with defaults, evaluate upgrade feasibility.

**Rationale**: 0.8 is the latest stable with improved APIs and active community. Upgrading later would require migrating through 0.7's breaking reactivity changes. The Arx Runa UI is minimal — any template incompatibilities are straightforward to fix.

### 4. Trunk as Frontend Build Tool

**Choice**: Trunk WASM bundler.

**Alternatives considered**:
- *cargo-leptos* — more tightly integrated with Leptos (SSR, islands), but designed for full-stack web apps. Overkill for CSR-only desktop.

**Rationale**: Trunk is purpose-built for WASM, handles index.html processing, asset bundling, and Tailwind pre-build hooks. It's the default in the Tauri Leptos template. CSR-only desktop apps don't benefit from cargo-leptos's SSR features.

### 5. SQLCipher via `bundled-sqlcipher-vendored-openssl`

**Choice**: `rusqlite` with the `bundled-sqlcipher-vendored-openssl` feature.

**Alternatives considered**:
- *`bundled-sqlcipher`* — bundles SQLCipher but requires system OpenSSL dev headers.
- *System sqlcipher* — link against pre-installed SQLCipher. Fastest compile but worst portability.

**Rationale**: Fully self-contained — bundles both SQLCipher and OpenSSL from source. Zero system dependencies. Builds on Windows, Linux, and macOS without extra packages. Longer initial compile (~2-3 min) is an acceptable trade-off for a security-critical desktop app.

### 6. Semver Ranges for Dependency Versions

**Choice**: Use `"0.8"` style semver ranges. `Cargo.lock` pins exact versions.

**Alternatives considered**:
- *Exact pinning (`=0.8.17`)* — prevents any version drift but misses security patches unless manually bumped. Unusual in the Rust ecosystem.

**Rationale**: Standard Rust practice. Semver ranges pick up compatible patches on `cargo update`. The lockfile provides reproducibility. Security patches for cryptographic crates are critical to receive promptly.

### 7. Minimal Module Placeholders

**Choice**: `mod.rs` with doc comments, `pub mod types;`, and empty `error.rs`. No trait stubs.

**Alternatives considered**:
- *Stub traits from designs* — include skeleton trait definitions (`KeySource`, `CloudTransport`, etc.). More upfront structure but blurs the Phase 0 / Phase 1 boundary.

**Rationale**: Phase 0 is scaffolding, not implementation. Stub traits would create compile targets that change immediately in Phase 1. Minimal placeholders verify the module structure compiles without prescribing internals.

---

## Consequences

### Positive

- Self-contained build — no system library requirements beyond a Rust toolchain and Node.js (for Tailwind)
- Workspace allows future expansion without restructuring
- Leptos 0.8 avoids a migration tax later
- All decisions align with existing ADRs (001 for structure, 002 for stack)

### Negative

- Template code needs manual upgrade from Leptos 0.6 to 0.8 — some API changes to fix
- `bundled-sqlcipher-vendored-openssl` adds ~2-3 min to first compile
- Edition 2024 requires `rand >= 0.9` — must verify all crypto crates are compatible

### Risks

- Leptos 0.8 is pre-1.0; APIs may change. Mitigated by minimal UI surface and version pinning.
- Tauri v2 Leptos docs reference 0.6; some Tauri-specific patterns may need adaptation for 0.8.

---

## References

- Design document: `docs/architecture/designs/project-scaffolding/design.md`
- ADR-001: Code Structure and Patterns
- ADR-002: Frontend Stack Selection
- [Tauri v2 — Leptos Frontend Guide](https://v2.tauri.app/start/frontend/leptos/)
- [Leptos 0.8.17 on crates.io](https://crates.io/crates/leptos)
- [Rust 2024 Edition Guide](https://doc.rust-lang.org/edition-guide/)
