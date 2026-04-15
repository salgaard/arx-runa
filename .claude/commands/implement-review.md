Run a review-driven Rust fix flow for: $ARGUMENTS

Use this command to orchestrate:
1. `problem-solver`
2. `rust-implementer`
3. `rust-reviewer` (verification loop)
4. `architecture-reviewer` (required verification loop)
5. `security-reviewer` (when needed)
6. `test-writer` (when needed)

## Input resolution

`$ARGUMENTS` can be:

1. Empty:
   - Use the newest review file in `.claude/reviews/review-*.md`.
2. Path to a review file:
   - e.g. `.claude/reviews/review-auth-20260415-001212.md`
3. `<review-path> <scope-override>`:
   - use the review file but constrain fixes to the explicit scope override.

If no review file is found or readable, halt and report the missing input.

## Scope resolution

1. Default scope comes from the review report's Scope section.
2. If scope override is provided, use it as the implementation scope.
3. Resolve scope to concrete Rust files before synthesis starts.
4. If no files are resolved, halt and report unresolved scope.

## Authority order (hard)

1. `.claude/rules/*.md` (primary, normative)
2. Canonical design docs in `docs/architecture/designs/**/design.md` and `docs/architecture/design-invariants.md`
3. `.claude/reference/*.md` (secondary pattern guidance only; never overrides rules/design)
4. Exception path: architecture-originated deviations must flow through `DESIGN_CHALLENGE_LEDGER` and explicit approval decisions; do not implement silent rule/design overrides.

## Phase 0 — Implementation context discovery (required)

Before synthesis or implementation, build a plan-aware context from `.claude/plans/`.

1. Read all phase plan files:
   - `.claude/plans/phase-*.md`
2. Read handoff context files when present:
   - `.claude/plans/HANDOFF-*.md`
3. Parse frontmatter fields at minimum:
   - `status`
   - `roadmap-phase`
   - `sub-phase`
   - `title`
4. Build an implementation progress snapshot:
   - implemented plans
   - in-progress plans
   - draft/planned plans
   - highest implemented roadmap phase and sub-phase
5. Extract rationale and intentional deviations from plan body sections, especially:
   - `## Design Concerns / Open Questions`
   - `## Assumptions`
   - `## Approach`
   - `## Implementation Decisions` (if present)
   - `## Implementation Log` (if present)
   - handoff notes with trade-offs, deferrals, and known constraints
6. Use this context as a required interpretation layer for every fix decision.

## Phase 0.5 — Context budget controls (required)

1. Build a compact `CONTEXT_DIGEST` before any agent invocation:
   - highest implemented phase/sub-phase
   - in-progress phase list
   - deferred/not-yet-implemented phase list
   - top rationale and constraints (max 20 bullets total)
2. Do not pass full plan files to every agent once `CONTEXT_DIGEST` exists.
3. Reuse the same digest across all shards; regenerate only if scope changes.
4. Keep only actionable evidence in working memory:
   - finding metadata
   - rule/design/plan citations
   - remediation action
5. Drop verbose prose after converting findings into structured entries.

## Phase 1 — Review ingestion and triage

1. Parse the input review report findings into structured records:
   - `id`
   - `severity`
   - `category`
   - `confidence` (optional)
   - `disposition` (optional)
   - `location`
   - `problem`
   - `evidence`
   - `plan_context`
   - `recommended_fix`
   - `proposed_solution`
   - `risk_if_unchanged`
   - `design_challenge` (optional)
2. Apply phase-aware classification:
   - if `disposition` is present from `/review-only`, treat it as default classification input
   - **Actionable now**: in-scope for implemented/in-progress phases
   - **Intentional decision**: justified in plans/handoff; do not change
   - **Deferred by roadmap/plan**: not-yet-implemented phase; do not change
   - **Conflict with approved rationale**: actionable now
3. Build `ACTIONABLE_FINDINGS` from:
   - Actionable now
   - Conflict with approved rationale
4. Keep `DEFERRED_OR_INTENTIONAL` as report-only entries (no code edits).
5. Build `DESIGN_CHALLENGE_LEDGER` from findings that challenge current rules/design:
   - challenged constraint
   - rationale
   - proposed update
   - related findings
6. Build `APPROVED_DESIGN_CHALLENGES` from ledger entries with explicit `Resolution: Accepted for update`.

## Phase 2 — Baseline

1. Run `cargo check --workspace`.
2. If baseline fails, halt and report baseline blockers before applying fixes.

## Phase 3 — Solution synthesis (sharded)

1. Split actionable findings by path shard (at minimum):
   - `src-tauri/src/auth/**`
   - `src-tauri/src/crypto/**`
   - `src-tauri/src/storage/**`
   - `src-tauri/src/**` (remaining Rust files)
2. Invoke `problem-solver` per shard in parallel with:
   - `CONTEXT_DIGEST`
   - shard-local findings
   - `DESIGN_CHALLENGE_LEDGER`
   - `APPROVED_DESIGN_CHALLENGES`
   - explicit requirement for minimal-risk, surgical edits
3. Require one output per shard:
   - `IMPLEMENTATION_PACK`
   - `NO_ACTIONABLE_FIXES`
   - `BLOCKED_SOLUTIONS`
4. Per-shard output limits:
   - all CRITICAL/HIGH
   - up to 20 MEDIUM
   - up to 10 LOW
5. Deduplicate cross-shard overlaps before implementation:
   - same root cause -> one canonical implementation step with all locations
6. If any shard returns `BLOCKED_SOLUTIONS`, keep blockers in the final report and continue with other shards that are unblocked.

## Phase 4 — Implementation pass

1. For each shard with `IMPLEMENTATION_PACK`, invoke `rust-implementer`.
2. Run shard implementations in parallel only when file sets are disjoint; otherwise run sequentially.
3. Require `rust-implementer` to return `IMPLEMENTATION_RESULT` including changed file list.
4. Implementation boundaries:
   - do not modify files outside resolved scope unless direct dependency requires it
   - do not implement deferred-phase features
   - do not override intentional decisions documented in plans/handoff
   - if a design/rule challenge remains unresolved, mark the item as blocked rather than silently overriding constraints
   - pass `APPROVED_DESIGN_CHALLENGES` to `rust-implementer`; if an item requires deviation outside that allowlist, mark blocked

## Phase 5 — Re-review remediation loop (budgeted)

1. Re-run `rust-reviewer` on changed files only, sharded by path.
2. Re-run `architecture-reviewer` on changed files only, sharded by path (required every round).
3. Re-run `security-reviewer` on changed shards under `auth/`, `crypto/`, `storage/` or when risk indicators appear.
4. Use compact structured findings only (same schema as Phase 1).
5. If actionable CRITICAL/HIGH remain in a shard:
   - invoke `problem-solver` for that shard (`round-N`)
   - invoke `rust-implementer` with the new `IMPLEMENTATION_PACK`
6. Progressive deepening:
   - deep-dive all unresolved CRITICAL/HIGH
   - deep-dive MEDIUM only when ambiguity or high blast radius exists
   - keep LOW concise unless security-sensitive
7. Max iterations: 8 remediation rounds.
8. Reviewer-only loops are not allowed when actionable CRITICAL/HIGH remain.

## Phase 6 — Test pass

1. Run relevant tests for changed scope.
2. Run `cargo test --workspace --all-targets --all-features` when behavior changed materially or sensitive modules were edited.
3. Invoke `test-writer` when:
   - reviewers identify missing tests, or
   - behavior changed without adequate coverage, or
   - sensitive modules were modified and adversarial coverage is missing
4. If tests fail, fix failures within scope and re-run once.

## Phase 7 — Final implementation report

1. Ensure directory exists: `.claude/reviews/`.
2. Derive output filename:
   - `.claude/reviews/fix-<scope-slug>-<YYYYMMDD-HHMMSS>.md`
3. Write a complete report:

```markdown
# Review Fix Report — <scope>

> Generated by `/implement-review`
> Timestamp (UTC): <YYYY-MM-DD HH:MM:SS>
> Source review: `.claude/reviews/<review-file>.md`
> Scope: <resolved scope>

## Implementation Context Snapshot

- Highest implemented phase: <phase/sub-phase>
- In-progress phases: <list>
- Planned/draft phases: <list>
- Key plan/handoff files consulted: <list>

## Triage Summary

- Actionable now: <N>
- Conflict with approved rationale: <N>
- Deferred by roadmap/plan: <N>
- Intentional decisions preserved: <N>

## Fixes Applied

### <Fix item>
- **Finding IDs**: <list>
- **Files changed**: <list>
- **Change summary**: <what changed>
- **Why now**: <phase-aware rationale>
- **Risk reduced**: <impact>

## Architecture Outcome

- Structural findings before: <N>
- Structural findings after: <N>
- Remaining structural blockers: <list or None>

## Design Challenge Ledger

### <Challenge item>
- **Challenged constraint**: <rule/design anchor>
- **Resolution**: <Accepted for update | Deferred | Rejected>
- **Implementation effect**: <how fix scope was impacted>
- **Follow-up owner**: <agent/command or human gate>

## Validation Summary

- Re-review result (before vs after severities)
- Test outcomes
- Remaining blockers

## Deferred / Not Applied (by design)

### <Finding title>
- **Reason**: <deferred phase or intentional decision>
- **Plan citation**: `<plan-file>:<section>`
- **Follow-up phase**: <phase/sub-phase or "N/A">

## Appendix

- Files reviewed
- Shards reviewed and per-shard finding counts
- Agent chain by round
- Rule/design references cited
```

4. If no actionable fixes are applied, still create the report and state why.

## Guardrails

- No commits, pushes, or PR actions.
- No destructive git commands.
- Do not broaden scope unless required by direct dependency; if broadened, report why.
- Do not implement deferred-phase functionality as part of this command.
- Every major implementation decision must cite relevant plan file(s) when phase context influenced the choice.
