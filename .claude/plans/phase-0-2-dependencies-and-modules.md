---
title: "Phase 0.2 — Dependencies and Module Skeleton"
created: "2026-04-12T00:00:00Z"
status: approved
roadmap-phase: 0
sub-phase: "0.2"
design-document: "docs/architecture/designs/project-scaffolding/design.md"
sub-phase-roadmap: "docs/architecture/designs/project-scaffolding/sub-phases/roadmap.md"
tags: [scaffolding, dependencies, module-skeleton, phase-0]
---

# Phase 0.2 — Dependencies and Module Skeleton

## 1. Goal

Populate `src-tauri/Cargo.toml` with all crate dependencies required by Phases 1–6, create empty placeholder modules for `crypto`, `auth`, `storage`, `sync`, `memory`, and `ui`, and declare them in `src-tauri/src/lib.rs` such that `cargo fmt`, `cargo clippy -- -D warnings`, and `cargo test --all-targets` all pass on a stock Tauri v2 + Leptos 0.8 scaffold.

## 2. Context

**Roadmap**: Phase 0 — Project Scaffolding (see `docs/roadmap.md` lines 35–41). Depends on Phase 0.1 (Tauri workspace initialisation) which is already complete as of commit `61cc70a` ("plan 0.1 implemented").

**Sub-phase dependency**: Phase 0.2 depends strictly on Phase 0.1 — needs a compiled workspace with `src-tauri/` as a member. The current state satisfies this:
- `src-tauri/Cargo.toml` exists (minimal template — see lines 1–25 of that file).
- `src-tauri/src/lib.rs` contains the template `greet` command and `pub fn run()` entry point.
- `src-tauri/src/main.rs` calls `arx_runa_tauri_lib::run()`.
- No `src-tauri/src/{crypto,auth,storage,sync,memory,ui}/` directories exist yet.

**Estimated scope** (from sub-phase): ~100 lines created, no test code.

**Pending Architectural Decisions**: None for Phase 0.2 itself. Decisions #1–#19 in `docs/architecture/designs/project-scaffolding/design.md` ("Decisions Made" table) are already resolved; this plan merely executes them.

**Contract Surface anchor** (from `design.md` § Contract Surface, line 24):
> "Phase 0 backend module scaffold is `mod.rs` + `error.rs` + `types/mod.rs` for `crypto`, `auth`, `storage`, `sync`, `memory`, and `ui`."

This is canonical per `CLAUDE.md`'s rule that "each phase `design.md` `## Contract Surface` section is canonical; sub-phases should reference it instead of duplicating." See Design Concern #1 below — the sub-phase and the design's own project-structure tree contradict this.

## 3. Design Concerns / Open Questions

### Concern 1 — `types/` directory for `memory/` and `ui/` (Contract Surface vs tree vs sub-phase)

- **Source**: Three conflicting statements:
  1. `design.md` line 24 (Contract Surface): "mod.rs + error.rs + types/mod.rs for crypto, auth, storage, sync, memory, and ui" → **all 6 modules get `types/`** → 18 files.
  2. `design.md` lines 54–103 (Project Structure tree): `memory/` lines 94–96 and `ui/` lines 97–99 both show only `mod.rs` + `error.rs`, with `crypto`/`auth`/`storage`/`sync` showing `types/mod.rs` → **4 modules get `types/`** → 14 files.
  3. `0.2-dependencies-and-modules.md` Deliverable 2 and Manual Verification (lines 20–27, 56): says `memory/` has no `types/` but `ui/` does → **5 modules get `types/`** (crypto/auth/storage/sync/ui) → 17 files.
- **Impact**: The output file count differs by up to 4 files; implementer will guess or pick one source.
- **Classification**: **Non-blocking** — `CLAUDE.md` establishes that the Contract Surface wins over sub-phases and diagrams by default, so the resolution is deterministic even without user input. Flagged so the user can correct the two non-canonical sources in the same sitting.
- **Resolution**: Follow the Contract Surface → **all six modules get `types/mod.rs`, 18 files total**. Also recommended (not part of this plan's implementation scope):
  - Edit `design.md` Project Structure tree (lines 94–99) to add `types/mod.rs` under both `memory/` and `ui/`.
  - Edit `0.2-dependencies-and-modules.md` Deliverable 2 (lines 20–27) and Manual Verification (line 56) to state "all six modules, 18 files".
  - Update Acceptance Criteria in `design.md` line 401 if it implicitly counts directories.

### Concern 2 — Package+workspace manifest at project root

- **Source**: `design.md` lines 50–52 and Rationale lines 108–112 state the root `Cargo.toml` must be a "package+workspace manifest" containing `[package]` (the Leptos frontend crate) **plus** `[workspace] members = ["src-tauri"]`. The Phase 0.2 sub-phase does not mention touching the root `Cargo.toml` at all.
- **Impact**: If the root `Cargo.toml` is already configured correctly by Phase 0.1, this is a no-op. If Phase 0.1 left the root as a virtual-only or single-package manifest, then `cargo clippy --all-targets --all-features -- -D warnings` will not lint `src-tauri/` and Phase 0.2's acceptance criteria are satisfied by accident.
- **Classification**: **Non-blocking** — verified at plan start, assumed already-correct from Phase 0.1 commit `61cc70a`.
- **Resolution**: Step 0 of the Approach below verifies the root manifest structure before proceeding; plan aborts if it is wrong and surfaces the gap back to Phase 0.1.

### Concern 3 — `greet` template command in `src-tauri/src/lib.rs`

- **Source**: `src-tauri/src/lib.rs` lines 2–18 contain the `greet` command and `pub fn run()` template from `cargo create-tauri-app`. The sub-phase says "Declare all six modules in `src-tauri/src/lib.rs`" but does not say to remove `greet`.
- **Impact**: Ambiguity on whether `greet` should be deleted as part of 0.2 or carried through to Phase 6 (where IPC commands get their permanent home).
- **Classification**: **Non-blocking**.
- **Resolution (assumption)**: Keep `greet` and `pub fn run()` as-is; only **add** the six `pub mod` declarations. Removing template commands is Phase 0.3 or 6 territory. Explicit assumption recorded in §4.

### Concern 4 — `rand = "0.10"` availability check

- **Source**: `design.md` Decision #12 (line 385) and sub-phase Notes line 118 both assert `rand = "0.10"` is the current stable, released 2026-02-08. `docs/research/` has no direct crates.io snapshot.
- **Impact**: If `rand 0.10` does not resolve, `cargo build` will fail at dependency resolution and the plan is stuck until the correct version is identified.
- **Classification**: **Non-blocking** — the design already resolved this with a dated stability claim (2026-02-08), which predates today (2026-04-12) by two months. Trust the design.
- **Resolution**: If `cargo build` fails with `rand 0.10` unresolvable at implementation time, fall back to `rand = "0.9"` (also satisfies the edition 2024 `gen`-keyword constraint per design Decision #12) and log a Plan Deviation.

### Concern 5 — `#[non_exhaustive]` + empty `thiserror` enum clippy interaction

- **Source**: Sub-phase Implementation Notes line 111 asserts `#[non_exhaustive]` on an empty `thiserror` enum "satisfies clippy". Modern clippy (`-D warnings`) can emit `clippy::empty_enum` or `dead_code` on an empty enum depending on toolchain.
- **Impact**: If any of the six empty error enums triggers a clippy lint under `-D warnings`, the sub-phase acceptance criteria fail.
- **Classification**: **Non-blocking** — the design explicitly chose `#[non_exhaustive]` (Decision #15) to survive the empty-enum case. If clippy still complains, the canonical escape hatch is an `#[allow(clippy::empty_enum)]` attribute on the enum with a comment pointing to the design decision.
- **Resolution**: If `-D warnings` fails on empty `thiserror` enums, add `#[allow(clippy::empty_enum)]` above each enum with comment `// Variants added in later phases — see design Decision #15.` Log as Plan Deviation.

### Concern 6 — `async-trait` + `tracing` baseline inclusion with no callers

- **Source**: `design.md` Core dependencies table (lines 137–145) includes `async-trait = "0.1"` and `tracing = "0.1"` as baseline. These are unused in Phase 0 but required by the dependency contract.
- **Impact**: Unused dependencies could trigger `cargo udeps` or a manual clippy audit flag. No lint in stable clippy fires on unused `[dependencies]`, so this is purely a style question.
- **Classification**: **Non-blocking** — baseline declaration is the design's explicit choice.
- **Resolution**: Declare in Cargo.toml. No code references. Acceptance criteria only require clippy/fmt/test to pass, not `cargo udeps`.

## 4. Assumptions

1. **File count is 18**, not 17 or 14 (resolution to Concern 1 via Contract Surface canonicality).
2. **`memory/types/mod.rs` and `ui/types/mod.rs` both exist** after this plan, each containing only the domain-types doc comment from the design's `types/mod.rs` pattern (lines 231–235).
3. **The `greet` command stays in `src-tauri/src/lib.rs`** through Phase 0.2. Only new content added: six `pub mod` declarations at the top of the file.
4. **Root `Cargo.toml` is already a correct package+workspace manifest** from Phase 0.1. Step 0 verifies this and aborts otherwise.
5. **No changes to `src-tauri/Cargo.toml` `[package]`, `[lib]`, or `[build-dependencies]` sections.** Only `[dependencies]` and `[dev-dependencies]` are populated. The existing `tauri-plugin-opener = "2"` line stays (template default, not contested by design).
6. **`tracing` is imported with no subscriber or features in Phase 0** — the design line 142 mentions it is for "Phase 6 error logging". Baseline declaration only; no `[features]` added.
7. **`tokio` explicit feature set is exactly**: `["macros", "rt-multi-thread", "fs", "io-util", "sync", "time"]` (design Decision #16).
8. **Module doc-comment wording** for each `mod.rs` comes from the design's "Module descriptions" table (lines 239–247). See Step 2 below for verbatim strings.
9. **`cargo test --all-targets` passes vacuously** — no `#[test]` functions are added in 0.2. The criterion is that the test target compiles.
10. **Placeholder files are tracked in git** — the plan creates all 18 files with non-empty content (each has at least a doc comment), so git tracks them without needing `.gitkeep`.
11. **No `#[allow(dead_code)]`** is added anywhere. Per design Decision #15, `#[non_exhaustive]` alone must suffice. If clippy disagrees, fall back to Concern 5 resolution.
12. **The plan does not touch** `src-tauri/build.rs`, `src-tauri/capabilities/`, `src-tauri/tauri.conf.json`, `src-tauri/gen/`, `src-tauri/icons/`, or any frontend files (`src/`, `index.html`, `Trunk.toml`, `input.css`, `package.json`). Those belong to 0.1 or 0.3.

## 5. Approach

### Step 0 — Pre-flight verification

Absolute paths are Windows-style under `C:\Users\chris\source\repos\arx-runa\`.

1. Read `C:\Users\chris\source\repos\arx-runa\Cargo.toml` and confirm it contains **both** a `[package]` section (the Leptos frontend crate) and a `[workspace]` section with `members = ["src-tauri"]`. If either is missing, **stop** — Phase 0.1 is incomplete; surface the gap and exit without touching 0.2 files.
2. Read `C:\Users\chris\source\repos\arx-runa\src-tauri\Cargo.toml` and confirm the existing `[dependencies]` section matches the current state (lines 20–24: `tauri`, `tauri-plugin-opener`, `serde`, `serde_json`) and `[build-dependencies]` contains `tauri-build` (line 17–18). No edits needed to `[package]`, `[lib]`, or `[build-dependencies]`.
3. Confirm `src-tauri/src/lib.rs` and `src-tauri/src/main.rs` exist and that no `crypto/`, `auth/`, `storage/`, `sync/`, `memory/`, or `ui/` subdirectories exist yet under `src-tauri/src/`.

### Step 1 — Populate `src-tauri/Cargo.toml` dependencies

Edit `C:\Users\chris\source\repos\arx-runa\src-tauri\Cargo.toml`. Replace the current `[dependencies]` section (lines 20–24) with the following exact block, preserving `[package]`, `[lib]`, and `[build-dependencies]` untouched:

```toml
[dependencies]
# --- Core (design.md Backend Core table) ---
tauri = { version = "2", features = [] }
tauri-plugin-opener = "2"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tokio = { version = "1", features = ["macros", "rt-multi-thread", "fs", "io-util", "sync", "time"] }
thiserror = "2"
async-trait = "0.1"
tracing = "0.1"

# --- Cryptography (design.md Backend Cryptography table — Phase 1+) ---
chacha20poly1305 = "0.10"
argon2 = "0.5"
bip39 = "2"
hkdf = "0.13"
sha2 = "0.11"
blake3 = "1"
rand = "0.10"
zeroize = { version = "1", features = ["derive"] }
secrecy = "0.10"
x25519-dalek = "2"

# --- Storage (design.md Backend Storage table — Phase 3+) ---
rusqlite = { version = "0.39", features = ["bundled-sqlcipher-vendored-openssl"] }
uuid = { version = "1", features = ["v4", "serde"] }

[dev-dependencies]
proptest = "1"
tempfile = "3"
assert_matches = "1"
anyhow = "1"
```

Notes embedded in the diff:
- `zeroize` uses `features = ["derive"]` — Phase 1 will `#[derive(ZeroizeOnDrop)]` on key types.
- `rusqlite` uses `bundled-sqlcipher-vendored-openssl` — first `cargo build` after this edit compiles OpenSSL from source and takes several minutes (sub-phase Note line 112).
- `rand = "0.10"` is the edition-2024-compatible stable per design Decision #12. If unresolvable at implementation time, see Concern 4 resolution.
- `anyhow` is **dev-only** per design Decision #17 — not in `[dependencies]`.

### Step 2 — Create six module directories with 18 placeholder files

Module doc-comment strings, verbatim from `design.md` Module descriptions table (lines 239–247):

| Module | Short name | Doc comment |
|---|---|---|
| `crypto` | Crypto | Cryptographic primitives: key derivation, chunk encryption, file key management, BLAKE3 checksums. |
| `auth` | Auth | Authentication and session management: Argon2id KDF, USB key file, session lifecycle, memory locking. |
| `storage` | Storage | Storage layer: fixed-size chunking, SQLCipher manifest database, file-to-blob pipeline. |
| `sync` | Sync | Cloud synchronisation: provider-agnostic blob transport, push/pull flows, vault header management. |
| `memory` | Memory | Memory protection utilities: mlock/VirtualLock wrappers, zeroisation helpers. Primary consumer: `auth` module (Phase 2) for session key memory locking. |
| `ui` | Ui | Tauri IPC command handlers: input validation, error sanitisation, async command dispatch. |

For **each** module `<m>` in `{crypto, auth, storage, sync, memory, ui}`, create these three files:

#### 2a. `C:\Users\chris\source\repos\arx-runa\src-tauri\src\<m>\mod.rs`

```rust
//! Arx Runa <m> module.
//!
//! <doc comment from table above — single line or wrapped at ~100 cols>

pub mod error;
pub mod types;
```

Use the `Module descriptions` table doc comment verbatim for the second `//!` line. Example for `crypto`:

```rust
//! Arx Runa crypto module.
//!
//! Cryptographic primitives: key derivation, chunk encryption, file key
//! management, BLAKE3 checksums.

pub mod error;
pub mod types;
```

#### 2b. `C:\Users\chris\source\repos\arx-runa\src-tauri\src\<m>\error.rs`

```rust
//! Error types for the <m> module.

use thiserror::Error;

/// Errors produced by the <m> module.
#[non_exhaustive]
#[derive(Debug, Error)]
pub enum <Short>Error {
    // Variants added in implementation phases.
}
```

Example for `crypto`:

```rust
//! Error types for the crypto module.

use thiserror::Error;

/// Errors produced by the crypto module.
#[non_exhaustive]
#[derive(Debug, Error)]
pub enum CryptoError {
    // Variants added in implementation phases.
}
```

Enum names (exact):
- `crypto` → `CryptoError`
- `auth` → `AuthError`
- `storage` → `StorageError`
- `sync` → `SyncError`
- `memory` → `MemoryError`
- `ui` → `UiError`

#### 2c. `C:\Users\chris\source\repos\arx-runa\src-tauri\src\<m>\types\mod.rs`

```rust
//! Domain types for the <m> module.
//!
//! Newtypes added in implementation phases.
```

**Per Assumption 1 and Concern 1 resolution, `types/mod.rs` is created for ALL SIX modules including `memory/` and `ui/`.** Total files created in Step 2: 6 × 3 = **18 files**.

### Step 3 — Declare modules in `src-tauri/src/lib.rs`

Edit `C:\Users\chris\source\repos\arx-runa\src-tauri\src\lib.rs`. **Prepend** the following block at the top of the file, **above** the existing `#[tauri::command]` line:

```rust
//! Arx Runa backend library.

pub mod auth;
pub mod crypto;
pub mod memory;
pub mod storage;
pub mod sync;
pub mod ui;

```

Keep the existing `greet` function (lines 2–6) and `run` function (lines 8–18) unchanged. Per Assumption 3, template commands are not removed in 0.2.

Final `src-tauri/src/lib.rs` shape:

```rust
//! Arx Runa backend library.

pub mod auth;
pub mod crypto;
pub mod memory;
pub mod storage;
pub mod sync;
pub mod ui;

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
/// Returns a greeting message from the backend.
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
/// Starts the Arx Runa Tauri runtime.
pub fn run() {
    if let Err(error) = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![greet])
        .run(tauri::generate_context!())
    {
        eprintln!("error while running tauri application: {error}");
    }
}
```

### Step 4 — Validate

Run from `C:\Users\chris\source\repos\arx-runa\`:

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets
```

Expected outcomes:
1. `cargo fmt --all -- --check` → zero diffs.
2. `cargo clippy --all-targets --all-features -- -D warnings` → zero warnings. If any empty `thiserror` enum triggers `clippy::empty_enum`, apply Concern 5 resolution and re-run.
3. `cargo test --all-targets` → compiles; passes vacuously (0 tests run in the new modules).

First clippy/build run will be slow (multiple minutes) — `rusqlite` with `bundled-sqlcipher-vendored-openssl` compiles OpenSSL from source.

### Step 5 — Manual verification (per sub-phase Validation Checkpoint lines 47–66, adjusted for Concern 1 resolution)

1. Confirm 18 placeholder files exist: 6 modules × (`mod.rs` + `error.rs` + `types/mod.rs`). Sub-phase line 56 says "17 files" — that number is wrong per Concern 1; actual count should be 18.
2. Open `src-tauri/src/lib.rs` — confirm all six `pub mod` declarations.
3. Run `cargo tree` and check:
   - No duplicate major versions of `rand`, `zeroize`, `chacha20poly1305`.
   - `x25519-dalek` resolves to a single major version (Phase 5 `hpke` integration will re-validate).
4. Run `cargo tree | grep "rand v"` — should show `rand v0.10.x` (or `v0.9.x` if Concern 4 fallback applied).

### Step 6 — Commit

Single commit titled `Phase 0.2 — dependencies and module skeleton` touching:
- `src-tauri/Cargo.toml` (modified)
- `src-tauri/src/lib.rs` (modified)
- `src-tauri/src/{crypto,auth,storage,sync,memory,ui}/mod.rs` (new, 6 files)
- `src-tauri/src/{crypto,auth,storage,sync,memory,ui}/error.rs` (new, 6 files)
- `src-tauri/src/{crypto,auth,storage,sync,memory,ui}/types/mod.rs` (new, 6 files)
- `Cargo.lock` (modified — dependency resolution side-effect)

## 6. Security implications

### 6a. Expected sensitive path set

All six placeholder modules under `src-tauri/src/` are touched. Of those, three fall under sensitive-path policy per `CLAUDE.md` and the rust rules:

- `src-tauri/src/crypto/{mod.rs, error.rs, types/mod.rs}` — **placeholder only**, no crypto logic.
- `src-tauri/src/auth/{mod.rs, error.rs, types/mod.rs}` — **placeholder only**, no auth logic.
- `src-tauri/src/storage/{mod.rs, error.rs, types/mod.rs}` — **placeholder only**, no storage logic.

The `memory/` module is security-adjacent (primary consumer: `auth` Phase 2) but is not under any of the three canonical sensitive paths. It is a placeholder.

No existing sensitive files are modified (none exist yet). No cryptographic, key-handling, or storage logic is added.

### 6b. Invoke `security-reviewer` agent? **NO**

**Rationale**: All deliverables are empty placeholder stubs:
- `mod.rs` files contain only doc comments and `pub mod` declarations.
- `error.rs` files contain an empty `thiserror` enum with no variants.
- `types/mod.rs` files contain only doc comments.

No cryptographic operations, no key material, no persistence, no IPC surface, no memory locking, no network I/O, no trait boundaries that accept secret inputs. The sub-phase's own "Security Review: Not required" statement (line 120) is independently verified to be correct.

The plan's Pre-flight verification (Step 0) explicitly cross-checks that no unexpected files under `crypto/`, `auth/`, or `storage/` get touched beyond the 9 placeholder files listed in 6a. Any drift — e.g., a variant sneaking into an error enum, or a newtype being added to `types/mod.rs` — must be flagged as a Plan Deviation and trigger a `security-reviewer` pass before merge.

### 6c. What the reviewer should check — N/A (NO decision above)

## 7. Testing strategy

Phase 0.2 ships **no test code**. Validation is compiler-driven.

**Test types — applicable (from sub-phase Validation Checkpoint)**:
- [x] **Compilation**: `cargo build` / `cargo test --all-targets` must succeed.
- [x] **Lint**: `cargo clippy --all-targets --all-features -- -D warnings` must produce zero warnings.
- [x] **Format**: `cargo fmt --all -- --check` must pass.
- [ ] Unit tests — not applicable, no logic.
- [ ] Integration tests — not applicable, no logic.
- [ ] Property tests — not applicable, no logic.
- [ ] Adversarial crypto tests — not applicable, no crypto logic.
- [ ] Smoke test — deferred to Phase 0.3 (`cargo tauri dev`).

**Invoke `test-writer` agent? NO.** Rationale: there is no production logic to test. Every `thiserror` variant test (required by the rust rules) is satisfied vacuously because no variants exist yet. Phase 1 will add variants and, at that point, per-variant tests.

**Edge cases from Step 1.75 review**:
- Empty-enum clippy interaction (Concern 5) — covered by Step 4 fallback, not a test.
- `rand 0.10` resolution (Concern 4) — covered by Step 4 fallback, not a test.

**Acceptance criteria** (from sub-phase lines 60–66 and `design.md` lines 394–403, filtered to 0.2 scope):
- [ ] Zero clippy warnings.
- [ ] `cargo fmt` reports no diffs.
- [ ] All six modules compile and are reachable via `src-tauri/src/lib.rs`.
- [ ] `rand` resolves to `>= 0.9` (target: `0.10`).
- [ ] `cargo test --all-targets` compiles and passes.
- [ ] 18 placeholder files exist (per Concern 1 resolution).
- [ ] `cargo tree` shows no duplicate majors of `rand`, `zeroize`, `chacha20poly1305`.

## 8. Documentation impact

- **In-scope**: none. Sub-phase line 110–112 explicitly says "Phase 0.2: No documentation updates."
- **Out-of-scope for implementer, but recommended as a follow-up edit** (from Concern 1): update `design.md` Project Structure tree (lines 94–99) and `0.2-dependencies-and-modules.md` Deliverable 2 + Manual Verification to match the Contract Surface (18 files, not 17 or 14). **Do not** make these edits during implementation — they belong to a separate design-document change, reviewed separately.

## 9. Handoff Notes for Implementer

Working directory: `C:\Users\chris\source\repos\arx-runa\`. Plan is **self-contained** — every file path, trait, enum name, doc comment, and dependency line is inlined above; you do not need to re-read the sub-phase or `design.md` unless a fallback (Concern 4 or 5) fires.

Order of operations: Step 0 (pre-flight) → Step 1 (Cargo.toml) → Step 2 (18 placeholder files) → Step 3 (lib.rs mod declarations) → Step 4 (validate) → Step 5 (manual check) → Step 6 (commit).

**Traps**:
1. The first `cargo build`/`cargo clippy` run after Step 1 compiles OpenSSL from source and takes **several minutes** — do not assume a hang.
2. File count is **18, not 17**. The sub-phase says 17 but contradicts the canonical Contract Surface. Create `types/mod.rs` under `memory/` AND `ui/`. See Concern 1.
3. Keep the template `greet` function in `lib.rs` — only **prepend** the six `pub mod` declarations. Do not delete or reorder template code.
4. Do **not** add `#[allow(dead_code)]` to the empty error enums — `#[non_exhaustive]` is the design-sanctioned approach. Only fall back to `#[allow(clippy::empty_enum)]` if clippy rejects the `#[non_exhaustive]` approach (Concern 5).
5. Do **not** touch the root `C:\Users\chris\source\repos\arx-runa\Cargo.toml` — it is Phase 0.1's responsibility. Step 0 only reads it to verify.
6. Platform: the build runs on Windows; all commands must use forward-slash paths inside bash (already the case in Step 4).
7. Any unplanned file under `src-tauri/src/crypto/`, `src-tauri/src/auth/`, or `src-tauri/src/storage/` must be treated as a Plan Deviation and triggers a `security-reviewer` pass before the commit in Step 6 (overrides the NO decision in §6b).
