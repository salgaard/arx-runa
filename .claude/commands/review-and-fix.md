Run a full Rust review-and-fix flow for: $ARGUMENTS

Use this command to orchestrate:
1. `rust-reviewer`
2. `security-reviewer` (when needed)
3. `problem-solver`
4. `rust-implementer`
5. `test-writer` (when needed)

## Scope resolution

1. If `$ARGUMENTS` is empty (or `all`), set scope to all Rust implementation code under:
   - `src-tauri/src/**/*.rs`
2. If `$ARGUMENTS` is provided:
   - Treat it as the review scope (path, module hint, or file set expression).
   - Resolve it to concrete Rust files before review starts.
3. If no files are resolved, halt and report the unresolved scope.

## Authority order (hard)

1. `.claude/rules/*.md` (primary, normative)
2. Canonical design docs in `docs/architecture/designs/**/design.md` and `docs/architecture/design-invariants.md`
3. `.claude/reference/*.md` (secondary pattern guidance only; never overrides rules/design)

## Phase 1 — Baseline

1. Run `cargo check --workspace`.
2. If baseline fails, halt and report baseline issues before running reviewers.

## Phase 2 — Review pass

1. Invoke `rust-reviewer` on the resolved scope.
2. Invoke `security-reviewer` when either is true:
   - scope includes files under `src-tauri/src/auth/`, `src-tauri/src/crypto/`, or `src-tauri/src/storage/`
   - `rust-reviewer` findings indicate security-sensitive risk
3. Consolidate findings into a single prioritized list:
   - CRITICAL / HIGH first
   - MEDIUM next
   - LOW last
   - map `security-reviewer` severities as `WARNING -> MEDIUM` and `NOTE -> LOW`

## Phase 3 — Solution synthesis pass

1. If there are no actionable findings, skip to Phase 6.
2. Invoke `problem-solver` with:
   - consolidated reviewer findings
   - exact file scope
   - current round identifier (`round-1` for first pass)
3. Require `problem-solver` to return one of:
   - `IMPLEMENTATION_PACK`
   - `NO_ACTIONABLE_FIXES`
   - `BLOCKED_SOLUTIONS`
4. If `BLOCKED_SOLUTIONS`, halt and report blockers explicitly.

## Phase 4 — Implementation pass

1. If `problem-solver` returned `NO_ACTIONABLE_FIXES`, skip to Phase 6.
2. Invoke `rust-implementer` with:
   - the `IMPLEMENTATION_PACK` output verbatim
   - exact file scope
   - requirement for surgical edits only
3. Require `rust-implementer` to return `IMPLEMENTATION_RESULT`.

## Phase 5 — Re-review remediation loop

1. Re-run `rust-reviewer` on changed files.
2. Re-run `security-reviewer` if sensitive paths were changed.
3. If actionable CRITICAL/HIGH findings remain:
   - invoke `problem-solver` on the new findings (`round-N`)
   - if `BLOCKED_SOLUTIONS`, halt and report unresolved blockers
   - invoke `rust-implementer` with the new `IMPLEMENTATION_PACK`
4. Max iterations: 10 implementation rounds per command run.
5. **Reviewer-only loops are not allowed**: any iteration with actionable CRITICAL/HIGH findings must include both `problem-solver` and `rust-implementer`.
6. If CRITICAL/HIGH still remain after max rounds, stop and report unresolved findings explicitly.

## Phase 6 — Test pass

Invoke `test-writer` when needed:
- behavior changed in implementation, or
- reviewers identified missing tests, or
- sensitive modules (`auth/crypto/storage`) were modified.

If tests fail, fix failures inside scope and re-run once.

## Phase 7 — Final report

Report:
1. Resolved scope and files reviewed
2. Findings summary by severity (before vs after)
3. Per-round agent chain used (`reviewers -> problem-solver -> rust-implementer`)
4. Files changed
5. Test outcomes
6. Any unresolved CRITICAL/HIGH findings (must be explicit)

## Guardrails

- No commits, pushes, or PR actions.
- No destructive git commands.
- Do not broaden scope unless required by a direct dependency; when broadened, report why.
