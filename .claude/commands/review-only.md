Run a full Rust review-only flow for: $ARGUMENTS

Use this command to orchestrate:
1. `rust-reviewer`
2. `security-reviewer` (when needed)
3. `problem-solver` (recommendations only; no implementation)

## Scope resolution

1. If `$ARGUMENTS` is empty (or `all`), set scope to all Rust implementation code under:
   - `src-tauri/src/**/*.rs`
2. If `$ARGUMENTS` is provided:
   - Treat it as the review scope (path, module hint, or file set expression), after extracting any cycle-count tokens.
   - Resolve it to concrete Rust files before review starts.
3. If no files are resolved, halt and report the unresolved scope.

## Review cycle configuration

1. Run multiple independent review cycles to improve issue discovery stability.
2. Default cycle count is `3`.
3. Allow an optional override via `$ARGUMENTS` tokens:
   - `cycles=<N>`
   - `--cycles <N>`
4. Validate `N` as an integer in `[1, 10]`; if invalid, halt and report invalid cycle configuration.
5. Use stable cycle identifiers: `cycle-1`, `cycle-2`, ..., `cycle-N`.
6. Keep file scope and `CONTEXT_DIGEST` identical across all cycles unless scope resolution itself changes.

## Authority order (hard)

1. `.claude/rules/*.md` (primary, normative)
2. Canonical design docs in `docs/architecture/designs/**/design.md` and `docs/architecture/design-invariants.md`
3. `.claude/reference/*.md` (secondary pattern guidance only; never overrides rules/design)

## Phase 0 — Implementation context discovery (required)

Before baseline or reviewer calls, build a plan-aware context from `.claude/plans/`.

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
   - handoff notes that explain trade-offs, deferrals, and known constraints
6. Use this context as a required interpretation layer for all findings.

## Phase 0.5 — Context budget controls (required)

1. Build a compact `CONTEXT_DIGEST` before any reviewer invocation:
   - highest implemented phase/sub-phase
   - in-progress phase list
   - deferred/not-yet-implemented phase list
   - top rationale and constraints (max 20 bullets total)
2. Do not pass full plan files to every agent once `CONTEXT_DIGEST` exists.
3. Reuse the same digest across all review shards; regenerate only if scope changes.
4. Keep only actionable evidence in working memory:
   - finding metadata
   - rule/design/plan citations
   - remediation recommendation
5. Drop verbose reviewer prose after extracting structured findings.

## Phase 1 — Baseline

1. Run `cargo check --workspace`.
2. If baseline fails, write a report file that captures baseline blockers and stop.

## Phase 2 — Review pass (multi-cycle, sharded)

1. For each cycle `cycle-1..cycle-N`:
   - split resolved files into path shards (at minimum):
     - `src-tauri/src/auth/**`
     - `src-tauri/src/crypto/**`
     - `src-tauri/src/storage/**`
     - `src-tauri/src/**` (remaining Rust files)
   - invoke `rust-reviewer` per shard in parallel with the same `CONTEXT_DIGEST`
   - invoke `security-reviewer` per shard when either is true:
     - shard includes `auth/`, `crypto/`, or `storage/`
     - corresponding `rust-reviewer` shard output indicates security-sensitive risk
2. Require compact structured output per finding from each reviewer shard:
   - `id`
   - `cycle_id`
   - `reviewer`
   - `severity`
   - `location`
   - `problem`
   - `evidence`
   - `plan_context`
   - `recommended_fix`
   - `proposed_solution`
   - `risk_if_unchanged`
3. Per-shard output limits (applies per cycle):
   - keep all CRITICAL/HIGH findings
   - include up to 20 MEDIUM findings (highest impact first)
   - include up to 10 LOW findings (deduplicated summaries)
4. Consolidate all cycles into one prioritized canonical finding list:
   - normalize severities as `CRITICAL/HIGH`, `MEDIUM`, `LOW`
   - map `security-reviewer` severities as `WARNING -> MEDIUM` and `NOTE -> LOW`
   - deduplicate same root cause across files/reviewers/cycles into one canonical finding
   - keep highest observed severity per canonical finding
5. Track recurrence metadata per canonical finding:
   - `occurrence_count` = total number of raw finding events across all cycles
   - `cycle_hits` = set of cycle IDs where the finding appeared
   - `reviewer_hits` = reviewers that reported it
   - `affected_locations` = merged deduplicated location list
6. Mark findings that appeared in more than one cycle as repeated findings for confidence reporting.

## Phase 3 — Solution synthesis (no code changes)

1. If there are no actionable findings, skip to Phase 4.
2. Invoke `problem-solver` with:
   - consolidated canonical findings including `occurrence_count`, `cycle_hits`, and merged evidence
   - exact file scope
   - instruction to produce recommendations only (no edits)
3. Require `problem-solver` to return one of:
   - `REMEDIATION_REPORT`
   - `NO_ACTIONABLE_FIXES`
   - `BLOCKED_SOLUTIONS`
4. If `BLOCKED_SOLUTIONS`, include blockers explicitly in the report.
5. Progressive deepening:
   - deep-dive all unresolved CRITICAL/HIGH
   - deep-dive MEDIUM only when ambiguity or high blast radius exists
   - keep LOW recommendations concise unless directly security-sensitive

## Phase 4 — Write report file

1. Ensure directory exists: `.claude/reviews/`.
2. Derive output filename:
   - `.claude/reviews/review-<scope-slug>-<YYYYMMDD-HHMMSS>.md`
3. Write a complete, nicely formatted Markdown report with this structure:

```markdown
# Review Report — <scope>

> Generated by `/review-only`
> Timestamp (UTC): <YYYY-MM-DD HH:MM:SS>
> Scope: <resolved scope>

## Implementation Context Snapshot

- Highest implemented phase: <phase/sub-phase>
- In-progress phases: <list>
- Planned/draft phases: <list>
- Key plan files consulted:
  - `.claude/plans/<file>.md` (status: <status>)
  - `.claude/plans/HANDOFF-<file>.md` (if used)

## Executive Summary

- Review cycles run: <N>
- Raw finding events (all cycles): <N>
- Unique canonical findings: <N>
- Repeated findings (seen in >1 cycle): <N>
- Critical/High: <N>
- Medium: <N>
- Low: <N>
- Status: <No actionable findings | Action required>

## Findings by Severity

| Severity | Count |
|---|---:|
| CRITICAL/HIGH | <N> |
| MEDIUM | <N> |
| LOW | <N> |

## Repeated Findings Frequency

| Finding | Occurrences | Cycles Seen |
|---|---:|---|
| <short finding title> | <N> | <cycle-1, cycle-3> |

## Detailed Findings and Recommended Fixes

### <Finding title>
- **Severity**: <CRITICAL/HIGH/MEDIUM/LOW>
- **Occurrences**: <N> (out of <total cycles> cycles)
- **Cycles Seen**: <cycle list>
- **Reviewers Seen**: <rust-reviewer/security-reviewer>
- **Location**: `<file>:<line>` (or module path; include affected locations when multiple)
- **Problem**: <what is wrong and why it matters>
- **Evidence**: <reviewer observation or rule/design mismatch>
- **Plan Context**: <implemented-phase context and rationale from `.claude/plans/*`>
- **Recommended Fix**: <clear recommendation>
- **Proposed Solution**: <concrete implementation approach, constraints, trade-offs>
- **Risk if Unchanged**: <impact>

### Finding classification rules (must apply)

- **Actionable now**: violates rules/design and is in-scope for already implemented or in-progress phases.
- **Intentional decision**: explicitly justified by plan/handoff rationale; document it with citation, do not flag as defect.
- **Deferred by roadmap/plan**: belongs to a not-yet-implemented phase; document as deferred, not as current implementation failure.
- **Conflict with approved rationale**: current code contradicts recorded plan rationale; flag as actionable with high clarity and citations.

## Recommended Remediation Order

1. <highest-priority item>
2. <next item>
3. <next item>

## Blockers / Open Questions

- <item or "None">

## Appendix

- Files reviewed
- Cycle summary (per cycle: shards reviewed, raw finding count, severity breakdown)
- Shards reviewed and per-shard finding counts
- Deduplication criteria used for canonical findings
- Reviewer agents used
- Rule/design references consulted
- Plan files and handoff files cited
```

4. If there are no actionable findings, still create the report and clearly state that no fixes are required.

## Guardrails

- **Review-only mode**: do not modify application source files.
- Allowed write output is the report file under `.claude/reviews/` only.
- No commits, pushes, or PR actions.
- No destructive git commands.
- Do not broaden scope unless required by direct dependency; if broadened, document why in the report.
- Every major finding must cite relevant plan file(s) when phase context or rationale influenced the judgment.
