# `/implement-review` — Review-Driven Rust Fix Command

Run a review-driven Rust fix flow for: $ARGUMENTS

---

## Design Principles

- **Orchestrator stays thin.** Route, normalize, and merge structured outputs only.
- **Agents own semantics.** Classification belongs to `finding-classifier`; remediation synthesis belongs to `problem-solver`.
- **Structured contracts over prose.** Prefer machine-readable Appendix K payloads from `/review-only`; markdown parsing is compatibility fallback.
- **Write-capable by design.** Unlike `/review-only`, this command is explicitly implementation-focused and may modify code within scope.
---

## Agent Roster

| Agent | Role | Output |
|---|---|---|
| `plan-context-builder` | Plan and handoff context extraction | `PLAN_DIGEST` |
| `rules-extractor` | Rule anchor extraction | `RULES_INDEX` |
| `design-extractor` | Design invariant extraction | `DESIGN_INDEX` |
| `shard-planner` | Scope-to-shard mapping and digest summaries | `SHARD_MAP` + `SHARD_DIGEST_SUMMARY[]` |
| `finding-classifier` | Disposition/confidence classification | Classified findings + challenge ledger |
| `problem-solver` | Fix synthesis per grouped findings | `SOLUTION_PACK` / `NO_ACTIONABLE_FIXES` / `BLOCKED_SOLUTIONS` |
| `rust-implementer` | Code implementation pass | `IMPLEMENTATION_RESULT` |
| `rust-reviewer` | Rust verification loop | Structured findings |
| `architecture-reviewer` | Architecture verification loop (required) | Structured findings |
| `security-reviewer` | Conditional security verification | Structured findings |
| `cross-shard-reviewer` | Cross-shard contradiction detection in re-review loops | Structured findings |
| `test-writer` | Conditional test expansion | Test additions/updates |

---

## Structured contract ownership (hard)

- `PLAN_DIGEST` → `.claude/agents/plan-context-builder.md`
- `RULES_INDEX` → `.claude/agents/rules-extractor.md`
- `DESIGN_INDEX` → `.claude/agents/design-extractor.md`
- `SHARD_MAP` + `SHARD_DIGEST_SUMMARY[]` → `.claude/agents/shard-planner.md`
- `CLASSIFIED_FINDINGS` → `.claude/agents/finding-classifier.md`
- `SOLUTION_PACK` / `NO_ACTIONABLE_FIXES` / `BLOCKED_SOLUTIONS` → `.claude/agents/problem-solver.md`
- `IMPLEMENTATION_RESULT` → `.claude/agents/rust-implementer.md`

Command docs define orchestration flow and required consumer fields; agent files are the authoritative producer schema.

---

## Input Resolution

`$ARGUMENTS` can be:

1. Empty:
   - Use the newest review file in `.claude/reviews/review-*.md`.
2. Path to a review file:
   - e.g. `.claude/reviews/review-auth-20260415-001212.md`
3. Path to a machine-readable findings payload exported from `/review-only` report Appendix K:
   - JSON with `actionable_findings` and optional `design_challenge_ledger`.
4. `<review-path> <scope-override>`:
   - Use the review file but constrain fixes to the explicit scope override.

If no review file is found or readable, halt and report the missing input.

---

## Scope Resolution

1. Default scope comes from the review report's Scope section.
2. If scope override is provided, use it as the implementation scope.
3. Resolve scope to concrete Rust files before synthesis starts.
4. If no files are resolved, halt and report unresolved scope.

---

## Authority Order (Hard)

1. `.claude/rules/*.md` (primary, normative)
2. Canonical design docs in `docs/architecture/designs/**/design.md` and `docs/architecture/design-invariants.md`
3. `.claude/reference/*.md` (secondary pattern guidance only; never overrides rules/design)
4. Architecture-originated deviations must flow through `DESIGN_CHALLENGE_LEDGER` and explicit approval decisions; do not implement silent rule/design overrides.

---

## Phase Contracts

| Phase | Contract |
|---|---|
| Phase 0 | Gather context in parallel via structured outputs only; kick off baseline concurrently. |
| Phase 0.5 | Build and enforce shard-scoped digest slices and context budgets. |
| Phase 1 | Ingest/normalize findings and classify actionable scope. |
| Phase 2 | Enforce baseline compile gate before fixes. |
| Phase 3 | Synthesize solutions in grouped, scoped solver calls. |
| Phase 4 | Execute write-capable implementation within strict boundaries. |
| Phase 5 | Run budgeted remediation re-review loops with escalation limit. |
| Phase 6 | Validate with tests and targeted test-authoring when needed. |
| Phase 7 | Emit final fix report, including no-op outcomes and blockers. |

---

## Phase 0 — Parallel Preflight Context Gathering (Required)

Spawn these in parallel and consume structured outputs only. Kick off `cargo check --workspace` concurrently to overlap baseline latency with gather time; the result is consumed in Phase 2.

1. `plan-context-builder` for `PLAN_DIGEST`.
2. `rules-extractor` for `RULES_INDEX`.
3. `design-extractor` for `DESIGN_INDEX`.
4. `shard-planner` for `SHARD_MAP` + `SHARD_DIGEST_SUMMARY[]`.
5. `cargo check --workspace` (result deferred to Phase 2).

Required consumer fields from gatherers:
- `PLAN_DIGEST`: `highest_implemented_phase`, `in_progress_phases`, `deferred_phases`, `plans[]`, `handoffs[]`
- `RULES_INDEX`: `rules[].{id,source_file,anchor,verbatim,scope,severity_if_violated}`
- `DESIGN_INDEX`: `invariants[].{id,source_file,anchor,verbatim,scope,challenged}`
- `SHARD_MAP`: `shards[].{shard_id,files,is_security_sensitive,security_keyword_hits}`, `security_trigger_keywords`, `total_files`
- `SHARD_DIGEST_SUMMARY[]`: `[].{shard_id,scopes,rule_ids,design_ids,implemented_phases,deferred_phases}`

If required fields are missing or malformed, halt before Phase 1.

Do not parse raw plan/rules/design prose in the orchestrator once these outputs exist.

## Phase 0.5 — Context Budget Controls and Digest Slices (Required)

1. Build `DIGEST_SLICE_<shard_id>` for each shard using `PLAN_DIGEST`, `RULES_INDEX`, and `DESIGN_INDEX`.
2. Pass shard slices to agents; do not pass full global indices except where a global classifier explicitly requires them.
3. Reuse slices across all subsequent phases and remediation loops unless scope changes.
4. Keep working memory to structured evidence:
   - finding metadata
   - citation anchors
   - remediation status
5. Drop verbose prose after converting findings into structured entries.

## Phase 1 — Review Ingestion and Classification Gate

1. Parse findings from one of:
   - Appendix K machine-readable payload (`actionable_findings` / `design_challenge_ledger`) — preferred, requires no markdown parsing,
   - or review report `Detailed Findings and Recommended Fixes` section — markdown fallback.
2. Normalize findings to this required shape:
   - `id`, `severity`, `category`, `location`, `problem`, `evidence`
   - `rule_refs`, `design_refs`
   - `plan_context`, `recommended_fix`, `proposed_solution`
   - `blast_radius`, `estimated_complexity`
   - `confidence` (may be present from Appendix K)
   - `disposition` (may be present from Appendix K)
   - `design_challenge` (optional)
3. Classify dispositions using `finding-classifier` when:
   - dispositions are absent from the parsed payload, **or**
   - the source review report predates the current plan state (i.e., any plan file has a newer `created` or `status` timestamp than the review report timestamp).
   
   Otherwise, use the dispositions as-is from the Appendix K payload.
   
   `finding-classifier` input: normalized findings + `PLAN_DIGEST` + `RULES_INDEX` + `DESIGN_INDEX`. Classifier output must conform to `.claude/agents/finding-classifier.md` and include per-record `canonical_id`, `disposition`, `confidence`, `confidence_rationale`, `disposition_citation`.

4. Build:
   - `ACTIONABLE_FINDINGS` from `ACTIONABLE_NOW` only
   - `DEFERRED_OR_INTENTIONAL` for report-only
   - `DESIGN_CHALLENGE_LEDGER` from classified challenge entries
5. Build `APPROVED_DESIGN_CHALLENGES` from ledger entries explicitly marked accepted (`status: Accepted for update`). In most cases this list is empty for pure review-driven fixes — approvals require a plan.
6. If `ACTIONABLE_FINDINGS` is empty, skip to Phase 7 and emit a no-op fix report.

## Phase 2 — Baseline

1. Consume the `cargo check --workspace` result from Phase 0.
2. If baseline fails, halt and report baseline blockers before applying fixes.

## Phase 3 — Solution Synthesis (Sharded, Grouped)

1. Group `ACTIONABLE_FINDINGS` before solver launch:
   - one isolated `problem-solver` agent per CRITICAL/HIGH finding
   - MEDIUM findings grouped by shard (max 10 per solver invocation)
   - one LOW batch across shards
2. Pass only scoped data per invocation:
   - grouped findings
   - relevant file list
   - `DIGEST_SLICE_<shard_id>` (or minimal multi-shard union slice when needed)
   - related `DESIGN_CHALLENGE_LEDGER` entries
   - `APPROVED_DESIGN_CHALLENGES`
3. Require one output per invocation per `.claude/agents/problem-solver.md`:
   - `SOLUTION_PACK`
   - `NO_ACTIONABLE_FIXES`
   - `BLOCKED_SOLUTIONS`

   Required consumer fields:
   - `SOLUTION_PACK.finding_ids`
   - `SOLUTION_PACK.solutions[].{canonical_id,recommendation,implementation_approach,blast_radius,dependencies,estimated_complexity}`
   - `NO_ACTIONABLE_FIXES.reason`
   - `BLOCKED_SOLUTIONS.blockers`
4. Deduplicate cross-group overlaps before implementation:
   - same root cause across groups → one canonical implementation step covering all locations
5. If any group returns `BLOCKED_SOLUTIONS`, keep blockers in the final report and continue with unblocked groups.

## Phase 4 — Implementation Pass

1. For each shard with `SOLUTION_PACK`, invoke `rust-implementer`.
2. Run shard implementations in parallel only when file sets are disjoint; otherwise run sequentially.
3. Require `rust-implementer` output contract from `.claude/agents/rust-implementer.md`; parse `IMPLEMENTATION_RESULT` items for `DONE|BLOCKED` status and per-item file/summary or reason/needed fields.
4. Implementation boundaries:
   - do not modify files outside resolved scope unless direct dependency requires it
   - do not implement deferred-phase features
   - do not override intentional decisions documented in plans/handoff
   - if a design/rule challenge remains unresolved, mark the item as blocked rather than silently overriding constraints
   - pass `APPROVED_DESIGN_CHALLENGES` to `rust-implementer`; if an item requires deviation outside that allowlist, mark blocked

## Phase 5 — Re-Review Remediation Loop (Budgeted)

1. Use stable remediation cycle identifiers: `remediation-cycle-1`, `remediation-cycle-2`, ..., `remediation-cycle-N` (distinct from `/review-only` review cycle labels).
2. Re-run `rust-reviewer` on changed files only, sharded by path. Pass existing `DIGEST_SLICE_<shard_id>` artifacts from Phase 0.5 — do not re-read full indices.
3. Re-run `architecture-reviewer` on changed files only, sharded by path (required every cycle). Pass same digest slices.
4. Re-run `security-reviewer` on changed shards under `auth/`, `crypto/`, `storage/`, or when risk indicators appear in reviewer findings.
5. After all shard reviewers complete for a remediation cycle, invoke `cross-shard-reviewer` **when two or more shards had changed files in this cycle**. Pass:
   - per-shard reviewer findings for this remediation cycle
   - `SHARD_DIGEST_SUMMARY[]` from Phase 0 (IDs only — not full slice content)
   - suppression list of already-resolved findings
6. Use compact structured findings only (same schema as Phase 1 normalized shape).
7. If actionable CRITICAL/HIGH remain in a shard:
   - invoke `problem-solver` for that shard (`remediation-cycle-N`) with the relevant `DIGEST_SLICE`
   - invoke `rust-implementer` with the new `SOLUTION_PACK`
8. Progressive deepening:
   - deep-dive all unresolved CRITICAL/HIGH
   - deep-dive MEDIUM only when ambiguity or high blast radius exists
   - keep LOW concise unless security-sensitive
9. Max iterations: 8 remediation cycles.
10. Reviewer-only loops are not allowed when actionable CRITICAL/HIGH remain.
11. If required thresholds remain unmet after remediation-cycle-8, **halt and report unresolved findings**. Recommend the user create a formal plan via `/plan` to address the remaining findings with explicit approved scope and design-challenge handling before re-attempting implementation.

## Agent I/O Boundaries (Hard)

1. Agents must receive structured fields only; do not pass full raw outputs from other agents.
2. Gatherers and extractors provide verbatim anchors; reviewers/solvers consume structured anchors, not full rule/design prose.
3. `rust-implementer` receives only:
   - scoped files and grouped findings
   - approved challenge allowlist
   - relevant digest slice
4. `cross-shard-reviewer` receives only:
   - structured per-shard finding records
   - `SHARD_DIGEST_SUMMARY[]` (IDs only)
5. Any requested deviation outside `APPROVED_DESIGN_CHALLENGES` is blocked and reported.

## Phase 6 — Test Pass

1. Run relevant tests for changed scope.
2. Run `cargo test --workspace --all-targets --all-features` when behavior changed materially or sensitive modules were edited.
3. Invoke `test-writer` when:
   - reviewers identify missing tests, or
   - behavior changed without adequate coverage, or
   - sensitive modules were modified and adversarial coverage is missing
4. If tests fail, fix failures within scope and re-run. If a second run fails, record the failure as a blocker in the final report rather than looping indefinitely.

## Phase 7 — Final Implementation Report

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
- Cross-shard issues found in remediation loops: <N>
- Remaining structural blockers: <list or None>

## Design Challenge Ledger

### <Challenge item>
- **Challenged constraint**: <rule/design anchor>
- **Resolution**: <Accepted for update | Deferred | Rejected>
- **Implementation effect**: <how fix scope was impacted>
- **Follow-up owner**: <agent/command or human gate>

## Validation Summary

- Re-review result (before vs after severities)
- Cross-shard review invocations: <N>
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
- Agent chain by remediation cycle (including cross-shard invocations)
- Rule/design references cited
```

4. If no actionable fixes are applied, still create the report and state why.

---

## Guardrails

- No commits, pushes, or PR actions.
- No destructive git commands.
- Do not broaden scope unless required by direct dependency; if broadened, report why.
- Do not implement deferred-phase functionality as part of this command.
- Every major implementation decision must cite relevant plan file(s) when phase context influenced the choice.
- Do not run automatically from `/review-only`; implementation requires an explicit operator invocation of `/implement-review`.
- `cross-shard-reviewer` receives `SHARD_DIGEST_SUMMARY[]` only — never full `DIGEST_SLICE` content.

---

## Failure Modes and Halt Conditions

| Condition | Action |
|---|---|
| No review file found/readable from input resolution | Halt and report missing input |
| Scope resolves to zero files | Halt and report unresolved scope |
| Baseline `cargo check --workspace` fails | Halt and report baseline blockers before fixes |
| Remediation thresholds unmet after `remediation-cycle-8` | Halt; report unresolved findings; recommend `/plan` for formal resolution |
