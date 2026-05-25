# Test Strategy

This document captures the decisions, conventions, and constraints that shape how Arx Runa is tested. Read alongside `test-taxonomy.md` for the layer definitions.

---

## Guiding principle

Cryptographic correctness is owned by the Rust test layers (unit, scenario, integration). The E2E layer focuses exclusively on what the user sees — UI state, browser storage cleanup, and loading behaviour. Each layer has a clear, non-overlapping concern.

---

## CI runs

`cargo test -p arx-runa-tauri --all-targets` runs all Rust tests (unit, scenario, and integration) together. The test matrix is **ubuntu-24.04, windows-latest, macos-latest** — all three platforms on every push.

- `cargo fmt --check` runs on Linux only (single formatting gate is sufficient).
- `cargo clippy -- -D warnings` runs on all platforms.
- A release build (`cargo build --release`) is verified on every run.

E2E runs in a separate CI job, Linux only, using `xvfb-run` for a virtual display. The app is built with `cargo tauri build --no-bundle` (release profile via `E2E_RELEASE=1`) inside the WebdriverIO `onPrepare` hook.

---

## Gated / opt-in tests

Some tests require external infrastructure and are skipped in normal CI by env var:

| Test | Gate | What it needs |
|---|---|---|
| `rclone_integration.rs` | `ARX_RCLONE_INTEGRATION=1` | Real rclone binary + local filesystem remote |
| `scenarios_real_cloud.rs` | `ARX_TEST_B2_KEY_ID`, `ARX_TEST_B2_APP_KEY`, `ARX_TEST_B2_BUCKET` | Live Backblaze B2 bucket |

The B2 scenarios use a unique path prefix per run so concurrent CI runs never collide, and blobs are deleted before assertions so the bucket stays clean on failure.

---

## Rclone sidecar stub in CI

Tauri requires the rclone sidecar binary to exist at build time. In CI (where rclone is not installed) the `Prepare CI rclone sidecar stub` step creates an empty executable at `src-tauri/bin/rclone-<target-triple>[.exe]`. This satisfies the Tauri bundler without enabling real rclone functionality.

---

## Argon2 parameters in tests

Production vaults use `Argon2Params::DEFAULT` (high memory/time cost). Tests use intentionally weak parameters via `test_parameters()`:

```rust
Argon2Params { memory_cost_kib: 1024, time_cost: 1, parallelism: 1 }
```

This keeps test suites fast. `create_tier_one_vault()` / `create_tier_two_vault()` use `Argon2Params::DEFAULT` to exercise the real creation path, but subsequent ceremony calls (change password, recover, rotate) use `test_parameters()`.

---

## `ceremony_lock` — serialising ceremony tests

A global `tokio::sync::Mutex` (`CEREMONY_TEST_LOCK`) serialises all ceremony tests that touch shared process-level state. Any test that calls multiple ceremonies in sequence acquires this lock first:

```rust
let _lock = ceremony_lock().await;
```

This prevents races between tests that share temp directories or session manager state when the async runtime schedules them concurrently.

---

## `#[tokio::test(flavor = "multi_thread")]`

Ceremony and scenario tests use the multi-thread flavour. The single-thread flavour deadlocks when a ceremony internally spawns `tokio::task::spawn_blocking` and then `.await`s it — the blocking thread parks on the single scheduler thread. Multi-thread is the safe default for any async test that touches ceremonies or storage.

---

## Shared fixtures (`test_support.rs`)

Scenario and unit tests share fixtures from `src-tauri/src/auth/ceremonies/test_support.rs`:

- `TierOneVault` / `TierTwoVault` — fully constructed vault state (temp dir, DB path, `MockCloudTransport`, `SessionManager`, `VaultId`, `VaultHeader`).
- `create_tier_one_vault()` / `create_tier_two_vault()` — call through real `create_vault()` so fixtures exercise the actual creation path.
- `add_recovery_slot_and_return_phrase()` — sets up a recovery slot and returns the phrase for downstream recovery tests.
- `upload_manifest_backup_for()` / `upload_corrupted_manifest_backup_for()` — drives manifest backup upload, including a bit-flip corruption helper for tamper-detection tests.
- `DerivedVaultKeys` / `derive_vault_keys_tier_one()` / `derive_vault_keys_from_header()` — re-derives all vault keys for assertions that need to inspect the key material directly.

Constants: `TEST_PASSWORD`, `TEST_NEW_PASSWORD`, `TEST_WRONG_PASSWORD`.

---

## MockCloudTransport

Scenario tests use `MockCloudTransport` (an in-memory blob store) instead of a real cloud transport. This:
- Keeps tests hermetic and fast (no network, no rclone process).
- Still exercises the full ceremony logic — upload/download/delete calls go through the same `CloudTransport` trait as production.
- Allows corruption injection (see `upload_corrupted_manifest_backup_for`).

---

## E2E constraints

- **Tier 1 only.** Vault creation in E2E always uses Tier 1 (password only). The key-file file-picker dialog cannot be automated via WebDriver, so Tier 2 flows are not covered at the E2E layer. They are covered in scenario tests.
- **`E2E_SKIP_BUILD=1`** skips the `cargo tauri build` step. Only safe after a prior `cargo tauri build --debug --no-bundle` — not after a plain `cargo build`, which produces a dev-server binary that shows a blank page.
- **`E2E_RELEASE=1`** selects the release profile (default in CI).
- On slow CI runners, Argon2 + cold WASM startup can take 60–90 seconds. Unlock timeouts in helpers account for this.

---

## Agile Testing Quadrants

Tests are mapped to the four quadrants of Brian Marick's Agile Testing Quadrant model. The axes are:

- **Business-facing vs Technology-facing** — does the test speak the language of use cases and users, or of code and systems?
- **Support the team vs Critique the product** — does the test guide development (prevent regressions, drive design), or find problems in a finished product?

```
                  Business-facing
                        │
         Q2             │             Q3
  Scenario tests        │   Exploratory testing (informal)
  E2E tests             │   Usability testing
  ──────────────────────┼──────────────────────
  Unit tests            │   cargo audit (CVE deps)
  Integration tests     │   gitleaks (secret scan)
         Q1             │   zero_trace E2E spec          Q4
                        │
                  Technology-facing
          Support the team     Critique the product
```

| Quadrant | Description | Arx Runa coverage |
|---|---|---|
| **Q1** — Technology-facing, support the team | Unit and component tests. Automated. Fast feedback during development. | ✅ In-file unit tests; integration tests (`src-tauri/tests/*.rs`) |
| **Q2** — Business-facing, support the team | Scenario, functional, and story tests. Automated. Verify use cases are implemented correctly. | ✅ Scenario tests (`src/tests/`) by UC; E2E tests (Tier 1 UI flows) |
| **Q3** — Business-facing, critique the product | Exploratory testing, usability testing, UAT. Human-driven and unscripted. Discovers problems no script anticipated. | ⚠️ Informal only — developers manually exercise the app before release. No documented checklist or formal UAT process. |
| **Q4** — Technology-facing, critique the product | Performance, security, load, and "ility" testing. Tool-assisted. Finds non-functional problems. | ⚠️ Partial — `cargo audit` (dependency CVEs), `gitleaks` (secret scan), `zero_trace.spec.js` (browser storage after lock). No benchmarks, no fuzzing, no dynamic security testing. |

### Q4 gaps and ideas

For a zero-knowledge encryption product, Q4 is the most consequential gap. Below are concrete options, ordered roughly by effort and value.

#### Performance benchmarks (`cargo bench` + Criterion)

Criterion.rs integrates with Cargo and produces stable, comparable measurements across runs.

| Benchmark | What to measure | Why |
|---|---|---|
| Argon2 derivation at `DEFAULT` params | Wall time on CI hardware | Catch accidental param weakening; document expected cost per platform |
| Chunk encrypt/decrypt throughput | MB/s at representative file sizes (1 MB, 50 MB, 200 MB) | Baseline for pipeline performance regressions |
| Manifest serialise/deserialise | Ops/sec | Detect regressions in backup/restore hot path |

Criterion results can be uploaded as CI artefacts and compared across runs with `critcmp`.

#### Fuzzing (`cargo-fuzz`)

`cargo-fuzz` wraps libFuzzer and requires no external tooling beyond a nightly Rust compiler.

Priority targets:

| Fuzz target | Input | Risk if not fuzzed |
|---|---|---|
| Manifest deserialisation | Arbitrary bytes → manifest struct | Malformed backup causes panic or silent data loss |
| Vault header decoding | Arbitrary bytes → header struct | Attacker-supplied header triggers unexpected code path |
| Chunk reassembly | Out-of-order / duplicate / truncated chunk sequences | Corrupted download produces wrong plaintext silently |

Fuzzing does not need to run in CI on every push — a periodic job (weekly or pre-release) or a local corpus maintained in the repo is sufficient.

#### Static security analysis

| Tool | What it catches | Effort |
|---|---|---|
| `cargo geiger` | Counts `unsafe` blocks; tracks increases over time | Low — one CI step |
| `cargo clippy --deny clippy::unwrap_used` | Forces explicit error handling in production paths | Low — lint flag |
| `semgrep` with Rust rules | Pattern-based security anti-patterns | Medium — requires rule selection |

#### Zeroize / key material audit

Arx Runa likely uses the `zeroize` crate for key material. A targeted test or CI check that verifies:

- All key-holding types implement `ZeroizeOnDrop`
- No key material is cloned into types that don't zeroize

This can be done as a code review checklist item or a `grep`-based CI assertion rather than a full test.

#### Dynamic security

A full penetration test or professional cryptographic audit is beyond automated CI but is worth planning before any public release. The highest-value targets:

- Vault header and manifest format (attacker-controlled input parsed by the client)
- Key derivation parameter storage (can an attacker downgrade Argon2 cost?)
- Cloud transport authentication (are upload/download operations authenticated end-to-end?)

### Q3 — Informal / developer-driven exploratory testing

Q3 is human-driven and unscripted by nature. For Arx Runa it currently means:

- Developers manually running the built app before a release and noting anything that feels wrong
- Early users (or developers acting as users) attempting flows they haven't scripted, surfacing confusion or edge cases
- No formal process, checklist, or documented UAT sign-off exists

**Is a formal process needed?**

For the current stage: not urgently. Developer-driven exploratory testing is sufficient as long as the team is small and the developers are also the primary users.

Two flows warrant closer attention before any public release, because they are **not covered by any automated layer**:

| Flow | Why it matters |
|---|---|
| Tier 2 vault creation and unlock (key file) | File-picker cannot be driven by WebDriver; only tested in scenario tests at the Rust level, never as a real UI interaction |
| Recovery phrase restore | High-stakes, infrequent path; a usability mistake here causes permanent data loss |

A lightweight pre-release checklist covering these two flows would close the most meaningful Q3 gap without requiring a formal QA process. See [`pre-release-checklist.md`](pre-release-checklist.md).

---

## What each layer does NOT cover

| Layer | Explicitly out of scope |
|---|---|
| Unit | Cross-ceremony flows; storage/transport |
| Scenario | Private internals; real network I/O |
| Integration | Crate internals; Tauri IPC; UI |
| E2E | Cryptographic correctness; Tier 2 auth flows |
