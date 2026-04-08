# Project Scaffolding — Sub-Phase Roadmap

**Parent design**: [`design.md`](../design.md)
**Created**: 2026-04-07
**Status**: Draft
**Implementation order**: 0.1 → 0.2 → 0.3 (strict dependencies)

---

## Overview

This sub-phase roadmap decomposes Phase 0 (Project Scaffolding) into 3 independently testable units. The goal is a compilable Tauri v2 + Leptos 0.8 project skeleton that serves as the foundation for all subsequent phases. Each sub-phase ends with a green `cargo build` so the project is never left in a broken state.

**Total sub-phases**: 3

**Rationale for decomposition**:
- Bootstrapping the workspace before adding dependencies prevents confusing multi-step failures
- Dependency declaration and module skeleton are independent of the Tauri template generation and can be verified separately
- The build pipeline (Trunk, Tailwind) involves external tooling that is easier to validate once the Rust side compiles cleanly

**Implementation strategy**: Generate the Tauri workspace first → populate dependencies and module placeholders → wire up the frontend build pipeline and verify all acceptance criteria

---

## Dependency Graph

```
0.1 (Tauri workspace initialisation)
 ↓
0.2 (Dependencies and module skeleton)
 ↓
0.3 (Frontend build pipeline and verification)
```

**Legend**:
- `→` strict dependency (must complete predecessor before starting)

---

## Sub-Phases

1. **[Phase 0.1: Tauri Workspace Initialisation](0.1-tauri-workspace-init.md)**
   - Run `cargo create-tauri-app` with the Leptos template
   - Convert root `Cargo.toml` to a `[workspace]` with `src-tauri/` as member
   - Remove the bare `src/main.rs` Rust binary
   - Verify `cargo build` succeeds on the generated skeleton
   - **Estimated**: ~20 lines modified/created, no test code

2. **[Phase 0.2: Dependencies and Module Skeleton](0.2-dependencies-and-modules.md)**
   - Populate `src-tauri/Cargo.toml` with all required crates (crypto, storage, dev-deps)
   - Create `src-tauri/src/{crypto,auth,storage,memory,ui}/mod.rs`, `error.rs`, `types/mod.rs`
   - Declare all modules in `src-tauri/src/lib.rs`
   - Verify `cargo clippy -- -D warnings` and `cargo test` pass
   - **Estimated**: ~80 lines created, no test code (empty skeleton)

3. **[Phase 0.3: Frontend Build Pipeline and Verification](0.3-frontend-build-pipeline.md)**
   - Write `Trunk.toml` with Tailwind v4 pre-build hook (`@tailwindcss/cli`)
   - Create `input.css` with `@import "tailwindcss"` and `@theme {}` brand token block (iron/stone/steel/rune/bone from `docs/arx-runa-brand.css`)
   - Update `index.html` as Trunk entry point
   - Verify `cargo tauri dev` launches a window and `cargo build --release` succeeds
   - Update `docs/guides/development.md` with Tauri build instructions
   - **Estimated**: ~60 lines created/modified, no test code

---

## Testing Strategy

### Per-Sub-Phase Testing

Phase 0 has no application logic to unit test. Validation is compiler-driven.

**Test types**:
- **Compilation**: `cargo build` must succeed at the end of each sub-phase
- **Lint**: `cargo clippy -- -D warnings` must produce zero warnings by end of 0.2
- **Format**: `cargo fmt --all -- --check` must pass by end of 0.2
- **Smoke test**: `cargo tauri dev` must launch a visible window at end of 0.3

### Regression Testing

After completing each sub-phase:
```bash
cargo build
cargo fmt --all -- --check
```

After completing 0.2 and 0.3:
```bash
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets
cargo build --release
```

### Manual Testing Checklist

- Phase 0.1: `cargo metadata` shows `src-tauri` as a workspace member; no top-level `[[bin]]` target
- Phase 0.2: All five module paths exist; `cargo check` reports no errors or unresolved modules
- Phase 0.3: Tauri window appears with default Leptos content; Tailwind brand token classes (`bg-iron`, `text-bone`, `text-rune`) compile without warnings

---

## Security Review Checkpoints

- **Phases 0.1–0.3**: No security review required. Phase 0 contains no cryptographic code, no key handling, and no data access. The first security-sensitive code appears in Phase 1 (crypto module) and Phase 2 (auth module).

---

## Documentation Impact

- **Phase 0.1**: No documentation updates
- **Phase 0.2**: No documentation updates
- **Phase 0.3**: Update `docs/guides/development.md` with Tauri build and run instructions; update `docs/roadmap.md` to mark Phase 0 complete

---

## Notes

- **`gen` keyword**: Edition 2024 reserves `gen`. `rand = "0.10"` satisfies the `>= 0.9` requirement and is the current stable. This is declared in 0.2 but validated once the project compiles.
- **Leptos template version**: `cargo create-tauri-app` generates Leptos 0.6 code. During 0.1, update the generated `Cargo.toml` deps to `leptos = "0.8"` and add `leptos_meta`, `leptos_router`, `console_log`, `log`, `serde-wasm-bindgen` before first compilation.
- **Tailwind v4 — no config file**: Delete any `tailwind.config.js` the template generates. Tailwind v4 uses CSS `@theme {}` in `input.css` and auto-detects source files. No `content` paths required.
- **Trunk dev port**: Default Trunk dev server is `:1420`. Confirm `tauri.conf.json`'s `devUrl` matches.
- **Windows CI**: The CI workflow runs on `ubuntu-latest`. The `bundled-sqlcipher-vendored-openssl` feature compiles OpenSSL from source — confirm the Tauri system dep install step (already in the workflow) includes all required headers.

---

## References

- **Parent design**: `docs/architecture/designs/project-scaffolding/design.md`
- **Roadmap entry**: `docs/roadmap.md` Phase 0
- **ADR-002**: Frontend stack selection (Leptos rationale)
- **ADR-004**: Project scaffolding decisions (workspace, Tauri v2, SQLCipher strategy)
- **Related phases**: Phase 1 (first code in `crypto/`), Phase 2 (first code in `auth/`)
