# Handoff — Phase 2.4 Vault Ceremonies

**Date**: 2026-04-15
**Branch**: `development`
**Plan**: `.claude/plans/phase-2-4-vault-ceremonies.md` (`status: in-progress`)
**Working dir**: `C:\Users\chris\source\repos\arx-runa\src-tauri`

## State

Implementation is complete and all tests pass. What remains is finalisation (clippy + sub-phase decision sync + plan Implementation Log). The user requested this run be wrapped up without spending more credits on review/test agents, so the remaining steps are inline work only.

### Done
- Phase 2.4 Approach steps 1–12 implemented (six ceremonies + staging + forward-declared `CloudTransport` / `VaultHeader` / `manifest_backup`).
- Governance sync G-1 (`.claude/rules/auth.md` Ceremonies + Recovery slots subsections, mirrored to `.github/instructions/auth.instructions.md`) done in an earlier run.
- Test suite: 29 ceremony tests + baseline suite. Full `cargo test --workspace --all-targets --all-features` = **220 passed, 0 failed, 1 ignored**. Last green run: task `br3ihz7ny`.
- Ceremony tests serialise through a static `tokio::sync::Mutex<()>` (`CEREMONY_TEST_LOCK` / `ceremony_lock()`) because they all write to the singleton `dirs::config_dir()/arx-runa/pending-vault-header.json` staging path. Every async ceremony test starts with `let _lock = ceremony_lock().await;`.
- A duplicate-lock bug in `test_change_password_old_kek_cannot_unwrap_file_keys_after_change` was fixed (line ~1410); do not reintroduce.

### Not done — remaining checklist
1. **Clippy** — run `cargo clippy --workspace --all-targets --all-features -- -D warnings` from `src-tauri/`. Fix any warnings introduced by Phase 2.4 (ceremonies / recovery_wrap / storage::cloud forward-declares / staging). Pre-existing warnings outside the Phase 2.4 file set are recorded, not fixed.
2. **Agent skips (document in Implementation Log, do NOT spawn)**:
   - `test-writer` — plan Section 7 says YES. **Skip.** User directive 2026-04-15: credits are tight, do not spawn. Log rationale: "Deferred — user directive 2026-04-15 (credit budget). Baseline 29 ceremony tests + 220 workspace tests pass. Follow-up issue: schedule adversarial/proptest coverage pass separately."
   - `security-reviewer` — plan Section 6.b says YES. **Skip with same rationale.** Record the plan's Section 6.c checklist (10 items) in the Implementation Log as manual spot-check evidence, noting which items are already covered by existing tests vs. which need a future manual review.
   - `rust-reviewer` / `problem-solver` — plan is legacy format, no explicit Section 6/8 decision. Default to NO; record the legacy-format migration warning.
3. **Sub-phase Implementation Decisions sync** — append `## Implementation Decisions` to `docs/architecture/designs/authentication-and-session-management/sub-phases/2.4-vault-ceremonies.md`. Must cover at minimum:
   - Stub schema for DC-1 (vault DB minimal table set used by ceremonies; full Phase 3.1 schema deferred)
   - DC-2 forward declarations scope (CloudTransport trait surface = `upload_blob`/`download_blob` only; VaultHeader fields implemented; `manifest_backup::{encrypt,decrypt}` helpers with 24-byte nonce + 16-byte tag, no AAD — `encrypt_manifest_backup` currently marked `#[allow(dead_code)]`)
   - DC-12 credential verification — `change_password` / `setup_recovery` verify current credentials via `vault_identity` unwrap (identity-unwrap pattern) rather than a dedicated verify call; reason: avoids a second Argon2id pass
   - Test serialisation decision — static `tokio::sync::Mutex` gate around the singleton staging path; reason: tests all write `pending-vault-header.json` in `dirs::config_dir()/arx-runa/`
   - `x25519-dalek` feature flag — enabled `static_secrets` in `Cargo.toml` for `StaticSecret::from([u8; 32])`
   - `rand = "0.10"` call pattern — `rand::rng().fill_bytes(&mut slice)` with `use rand::Rng;`
4. **Flip plan status** — change `.claude/plans/phase-2-4-vault-ceremonies.md` frontmatter `status: in-progress` → `status: implemented`.
5. **Append Implementation Log** to the plan file. Fields (per `/implement-plan` Step 6.3):
   - Date (ISO 8601)
   - Branch: `development`
   - Execution mode: `direct` (legacy plan, no `implementation-delegation` frontmatter — record the migration warning)
   - Agent evidence table — one row per Approach step; for Steps 1–11 mark `Agent: (direct)`; for Step 12 (tests) same. Add rows for test-writer / security-reviewer / rust-reviewer / problem-solver with Outcome = "Skipped per user directive 2026-04-15"
   - Files changed — see list below
   - Test results — `220 passed, 0 failed, 1 ignored, 0 measured` (cargo test --workspace --all-targets --all-features; task id `br3ihz7ny`, exit code 0)
   - Clippy results — fill in after Step 1 of this checklist
   - Rust review — N/A (skipped, legacy plan)
   - Security review — Skipped per user directive 2026-04-15; manual spot-check against plan Section 6.c checklist
   - Governance sync — G-1 applied (`.claude/rules/auth.md` + `.github/instructions/auth.instructions.md` via `/copilot-sync`)
   - Sub-phase decisions sync — doc path + count of decisions added
   - Deviations from plan — record: (a) duplicate-lock fix in test file, (b) test serialisation via static mutex (not in plan), (c) `x25519-dalek` feature flag added, (d) agent skips per user directive, (e) legacy-plan format (no `implementation-delegation`, no `rust-review-agent-required`, no `security-agent-required`, no `solution-agent-required` frontmatter)
   - Documentation flagged — copy Section 8 verbatim, do not audit
6. **Do not commit.** Leave the working tree dirty; the user inspects and commits.

## Files changed (relative to `src-tauri/`)
- `Cargo.toml` — added `static_secrets` feature to `x25519-dalek`; added `base64`, `hex`
- `src/auth/mod.rs` — re-exports for `ceremonies` module
- `src/auth/ceremonies.rs` — **new**, ~2100 lines incl. test module
- `src/auth/staging.rs` — **new**, staging-file writer for `pending-vault-header.json`
- `src/auth/error.rs` — added `InvalidRecoveryPhrase`, `NoRecoverySlot`, `SessionNotActive` variants
- `src/auth/session.rs` — added `SessionKeys::from_master_key_bytes`, `SessionManager::install_session`, `SessionManager::swap_active_session`
- `src/crypto/recovery_wrap.rs` — **new**, `wrap_master_key_for_recovery` / `unwrap_master_key_from_recovery` (AAD = `b"arx-runa recovery v1" || vault_id_bytes`)
- `src/crypto/types/mod.rs` — added `MasterKey`, `RecoveryKey`, `WrappedMasterKey`, `VaultId`
- `src/crypto/mod.rs` — declares `recovery_wrap` module
- `src/storage/mod.rs` — declares `cloud` submodule
- `src/storage/cloud/mod.rs` — **new**, `CloudTransport` trait forward declaration
- `src/storage/cloud/vault_header.rs` — **new**, `VaultHeader` / `RecoverySlot` / `Argon2ParamsJson` structs + validation
- `src/storage/cloud/manifest_backup.rs` — **new**, `encrypt_manifest_backup` / `decrypt_manifest_backup` helpers (24-byte nonce, no AAD; `encrypt_manifest_backup` is `#[allow(dead_code)]`)
- `src/storage/cloud/mock.rs` — **new** (test-only `MockCloudTransport`)
- `.claude/rules/auth.md` — G-1 subsections
- `.github/instructions/auth.instructions.md` — mirrored via `/copilot-sync`

## Gotchas for the next agent
- **Do not spawn agents** on this plan — user directive 2026-04-15 on credit budget. Record skips in the Implementation Log instead.
- Test parallelism — do not remove `let _lock = ceremony_lock().await;` from any async ceremony test. They will deadlock through the singleton staging path if the lock is dropped.
- Do not introduce a second `let _lock = ceremony_lock().await;` in the same function — that is self-deadlock. A previous run had a duplicate on line ~1411 and hung the binary.
- Test run command: `cargo test --workspace --all-targets --all-features` from `src-tauri/`. Last green task id: `br3ihz7ny`, exit code 0. Output file is at `C:\Users\chris\AppData\Local\Temp\claude\C--Users-chris-source-repos-arx-runa\c46a4439-a818-467d-bf52-4e82beb5a031\tasks\br3ihz7ny.output`.
- Plan is legacy — frontmatter is missing `implementation-delegation`, `rust-review-agent-required`, `security-agent-required`, `solution-agent-required`. The `/implement-plan` skill requires a migration warning in the Implementation Log. Do not attempt to re-run gate validation; proceed to finalisation.
- Windows platform only for this session — no `nix`/Unix-specific calls were touched this run, but `staging.rs` already handles both.
