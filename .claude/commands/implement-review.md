# `/implement-review` — Review-Driven Rust Fix Command

Run a review-driven Rust fix flow for: $ARGUMENTS

---

## Agent Roster, Models & Contracts

| Agent | Model | Role | Output |
|---|---|---|---|
| `plan-context-builder` | `claude-haiku-4-5` | Parse plan/handoff files | `PLAN_DIGEST` |
| `rules-extractor` | `claude-haiku-4-5` | Extract rules verbatim | `RULES_INDEX` |
| `design-extractor` | `claude-haiku-4-5` | Extract design invariants verbatim | `DESIGN_INDEX` |
| `shard-planner` | `claude-haiku-4-5` | Map files to shards + keyword scan | `SHARD_MAP` + `SHARD_DIGEST_SUMMARY[]` |
| `finding-classifier` | `claude-haiku-4-5` | Classify findings by disposition/confidence | `CLASSIFIED_FINDINGS` |
| `cross-shard-reviewer` | `claude-haiku-4-5` | Cross-shard contradiction detection | Raw findings |
| `problem-solver` | `claude-sonnet-4-6` | Fix synthesis per grouped findings | `SOLUTION_PACK` |
| `rust-implementer` | `claude-sonnet-4-6` | Code implementation pass | `IMPLEMENTATION_RESULT` |
| `rust-reviewer` | `claude-sonnet-4-6` | Verification re-review per shard | Raw findings |
| `architecture-reviewer` | `claude-sonnet-4-6` | Architecture verification loop | Raw findings |
| `security-reviewer` | `claude-sonnet-4-6` | Conditional security verification | Raw findings |
| `test-writer` | `claude-haiku-4-5` | Test expansion (Sonnet for auth/crypto scope) | Test additions/updates |

Producer schemas are authoritative in `.claude/agents/<agent-name>.md`. This command owns orchestration and gates only.

**Invocation rule (hard):** Every agent MUST be invoked via `task` tool with the model above. The orchestrator MUST NOT classify findings, synthesize solutions, implement code, or write reports directly — this is a protocol violation regardless of reasoning quality. Never pass one agent's full raw output to another; extract and pass structured fields only.

**`test-writer` model exception:** use `claude-sonnet-4-6` when the shard is `shard-auth` or `shard-crypto`, or when the plan requests adversarial crypto tests.

---

## Design Principles

- **Orchestrator stays thin.** Route, normalize, merge structured outputs only.
- **Structured contracts, not prose.** Prefer machine-readable Appendix K payloads from `/review-only`; markdown parsing is a fallback only.
- **Parallelism is the default.** Serialize only when strict data dependency requires it.
- **Write-capable by design.** This command may modify code within scope, unlike `/review-only`.
- **Context-bounded cycles.** Persist cycle state to disk; carry forward only IDs and severities in working memory.

## Canonical Designs and Rules

1. `docs/architecture/design-invariants.md`
2. `docs/architecture/designs/*/design.md`
3. `.claude/rules/*.md`

Architecture-originated deviations must flow through `DESIGN_CHALLENGE_LEDGER` and explicit approval decisions; do not implement silent rule/design overrides.

---

## Input Resolution

`$ARGUMENTS` can be:
1. **Empty** — use the newest `.claude/reviews/review-*.md`
2. **Path to a review file** — e.g., `.claude/reviews/review-auth-20260415-001212.md`
3. **Path to an Appendix K JSON payload** — preferred; no markdown parsing required
4. **`<review-path> <scope-override>`** — use the review file but constrain fixes to the explicit scope

If no review file is found or readable → halt.

## Scope Resolution

Default scope comes from the review report's Scope field. If a scope override is provided, use it. Resolve to concrete Rust files before synthesis starts. Zero files resolved → halt.

## Track Selection

| Condition | Track |
|---|---|
| Security-sensitive findings present, OR security shards in scope, OR >10 files | `full` — all agents, security-reviewer, cross-shard if ≥2 shards, max 8 remediation cycles |
| 4–10 non-security files; CRITICAL/HIGH findings present | `standard` — rust-implementer + reviewers + test-writer; cross-shard if ≥2 shards; max 3 cycles |
| ≤3 non-security files; no CRITICAL/HIGH | `minimal` — rust-implementer + rust-reviewer + test-writer; 1 cycle; escalate to `standard` if HIGH surfaces |

Track is locked after selection and recorded in the fix report header.

## Output Parsing Protocol

After every agent invocation:
1. Locate the named output block. Strip prose wrappers/fences.
2. Validate all required top-level fields per the agent's contract.
3. On failure: re-invoke once, prepending: `"Your previous output did not match the required schema. Return only the structured block specified in your agent contract — no prose preamble, no markdown fences unless part of the schema."`
4. Second failure → halt with `PARSE_ERROR`. Do not infer missing fields.

---

## Phase 0 — Parallel Preflight

Spawn **in parallel** via `task` tool. The orchestrator does not read plan, rules, or design files directly.

- `plan-context-builder` → `.claude/plans/phase-*.md`, `.claude/plans/HANDOFF-*.md`
- `rules-extractor` → `.claude/rules/*.md`
- `design-extractor` → `docs/architecture/designs/**/design.md`, `docs/architecture/design-invariants.md`
- `shard-planner` → resolved file list (runs in parallel with above)
- `cargo check --workspace` (concurrent; result consumed in Phase 2)

Generate run ID: `fix-<scope-slug>-<YYYYMMDD-HHMMSS>`. Write to `.claude/reviews/<run-id>/run-state.json`:

```json
{
  "run_id": "", "source_review": "", "scope": "", "track": "", "cycle_count": 0,
  "finding_summary": { "CRITICAL": 0, "HIGH": 0, "MEDIUM": 0, "LOW": 0 },
  "disposition_summary": { "ACTIONABLE_NOW": 0, "INTENTIONAL_DECISION": 0, "DEFERRED_BY_PLAN": 0, "INSUFFICIENT_EVIDENCE": 0 },
  "override_records": [], "cycles": []
}
```

### 0-D: Build Shard-Scoped Digest Slices (orchestrator step)

Once `PLAN_DIGEST`, `RULES_INDEX`, `DESIGN_INDEX` are available, build one `DIGEST_SLICE_<shard_id>` per shard (scope-filtered). Reuse across all subsequent phases and remediation loops. Do NOT pass full global indices to agents.

Shard-to-scope mapping: `shard-auth` → `auth/global`; `shard-crypto` → `crypto/global`; `shard-storage` → `storage/global`; `shard-default` → `global`.

Apply output parsing protocol to all Phase 0 agents. Halt on any malformed output after retry.

---

## Phase 1 — Review Ingestion and Classification Gate

1. Parse findings from Appendix K payload (preferred) or the `Detailed Findings and Recommended Fixes` section of the review report (markdown fallback).

2. Normalize each finding to include: `id`, `severity`, `category`, `location`, `problem`, `evidence`, `rule_refs[]`, `design_refs[]`, `plan_context`, `recommended_fix`, `proposed_solution`, `blast_radius`, `estimated_complexity`, `confidence` (if present), `disposition` (if present), `design_challenge` (optional).

3. **Severity normalization at ingestion** (stricter than review-phase):

   | Raw | Normalized |
   |---|---|
   | CRITICAL | CRITICAL |
   | HIGH | HIGH |
   | WARNING (security-reviewer) | HIGH |
   | MEDIUM | MEDIUM |
   | NOTE (security-reviewer) | MEDIUM |
   | LOW | LOW |

4. **Invoke `finding-classifier` when:** dispositions are absent, OR any plan file has a newer timestamp than the review report. Otherwise use dispositions from Appendix K as-is.

5. Build: `ACTIONABLE_FINDINGS` (from `ACTIONABLE_NOW` only), `DEFERRED_OR_INTENTIONAL` (report-only), `DESIGN_CHALLENGE_LEDGER`.

6. Build `APPROVED_DESIGN_CHALLENGES` from ledger entries with `status: Accepted for update`.

7. If `ACTIONABLE_FINDINGS` is empty → skip to Phase 7 (emit no-op report).

---

## Phase 2 — Baseline Gate

Consume `cargo check --workspace` result from Phase 0. Failure → halt and report baseline blockers before any fixes.

---

## Phase 3 — Solution Synthesis (Sharded, Grouped)

**Security-scoped challenge checkpoint (hard):** before spawning any `problem-solver`, check `DESIGN_CHALLENGE_LEDGER` for `requires_human_review: true` entries. If any exist, halt and display to the user (challenged constraint, rationale, proposed update). Require explicit `accept`/`reject` per challenge. Record decisions and resume.

Group `ACTIONABLE_FINDINGS` and spawn `problem-solver` agents in parallel:
- One per CRITICAL finding (isolated)
- One per HIGH finding (or root-cause group)
- One per shard for MEDIUM findings (≤10 per agent)
- One for all LOWs

Each agent receives only: `{ findings[], relevant_files[], digest_slice, design_challenge_entries[], approved_design_challenges: [<from APPROVED_DESIGN_CHALLENGES>] }`

Deep-dive: all CRITICAL/HIGH; MEDIUM only when ambiguity or `blast_radius` is CROSS-MODULE/SYSTEM; LOWs concise.

Deduplicate cross-group overlaps before implementation (same root cause → one canonical step). If any group returns `BLOCKED_SOLUTIONS`, continue with unblocked groups; include blockers in final report.

---

## Phase 4 — Implementation Pass

For each shard with a `SOLUTION_PACK`, invoke `rust-implementer` via `task` tool. **Orchestrator MUST NOT write code directly.**

Run shard implementations in parallel only when file sets are disjoint; otherwise sequential.

After each shard implementation, run `cargo check --workspace`. Fix compile errors before proceeding.

Implementation boundaries:
- Do not modify files outside resolved scope unless direct dependency requires it; if broadened, report why.
- Do not implement deferred-phase features or override intentional decisions.
- Pass `APPROVED_DESIGN_CHALLENGES`; mark blocked if deviation is outside that allowlist.
- When a `SOLUTION_PACK` includes `design_doc_update`, apply it in the same pass.

---

## Phase 5 — Re-Review Remediation Loop (Budgeted)

Cycle identifiers: `remediation-cycle-1`, `remediation-cycle-2`, ..., `remediation-cycle-N`.

Assign stable `CF-NNN` IDs in arrival order (rust-reviewer → architecture-reviewer → security-reviewer). Mapping is fixed for the entire loop.

### Per-Cycle Execution

1. `rust-reviewer` on changed files only, sharded by path. Pass existing `DIGEST_SLICE` artifacts — do not re-read full indices.
2. `architecture-reviewer` on changed files only (`full` and `standard` tracks).
3. `security-reviewer` on changed auth/crypto/storage shards or when risk indicators appear (`full` track; `standard` only if drift check fires).
4. Invoke `finding-classifier` with canonicalized findings + `PLAN_DIGEST` + `RULES_INDEX` + `DESIGN_INDEX`. Require `CLASSIFIED_FINDINGS`.
5. Invoke `cross-shard-reviewer` when ≥2 shards had changed files (`full` and `standard` tracks). Before invoking, extract `INTERFACE_SLICE`:
   ```bash
   grep -rn "^pub fn\|^pub trait\|^pub struct\|^pub enum\|^pub type" <boundary files>
   ```
   Input: per-shard findings (structured fields), `SHARD_DIGEST_SUMMARY[]` (IDs only), suppression list, `INTERFACE_SLICE`. Never full `DIGEST_SLICE` content.
6. If actionable CRITICAL/HIGH remain: invoke `problem-solver` → `rust-implementer` for affected shards.

### Orchestrator Override (Persistent HIGH Findings)

Available from `remediation-cycle-3` (`full`) or `remediation-cycle-2` (`standard`). Never for CRITICAL.

If a HIGH finding has been `ACTIONABLE_NOW` in two consecutive cycles without resolution, file an Override Record: `{ finding_id, cycles_unresolved, override_rationale, confidence: "CERTAIN"|"LIKELY"|"UNCERTAIN", supporting_evidence }`.
- `CERTAIN` or `LIKELY` → reclassify as `INTENTIONAL_DECISION`
- `UNCERTAIN` → halt and surface to user; resume on input

### Acceptance Thresholds

- CRITICAL → must remediate before completion
- HIGH → must remediate or carry an approved Override Record
- MEDIUM/LOW → record in fix report with rationale when deferred

### Run-State Persistence

After each remediation cycle, write `.claude/reviews/<run-id>/cycle-<N>.json`: `{ cycle, findings[{id,severity,disposition,source_id}], override_records[], cross_shard_finding_count, actionable_remaining }`. Update `run-state.json`.

Orchestrator carries forward between cycles only: CF-NNN → severity mapping (IDs + severities), disposition/severity summary counts, Override Records filed. Drop all verbose prose and solution content.

**Context compaction:** if context budget is insufficient, persist state, emit `CONTEXT_CHECKPOINT`, continue from disk.

**Cycle limits:** `full`: 8 | `standard`: 3 | `minimal`: 1 (then escalate or accept with rationale). If thresholds unmet after max cycles → halt, report unresolved findings, recommend formal `/plan` resolution.

---

## Phase 6 — Test + Verify

1. `cargo fmt --all`
2. `cargo clippy --workspace --all-targets --all-features -- -D warnings` — fix related failures; note pre-existing unrelated issues.
3. `cargo test --workspace --all-targets --all-features` — fix related failures; note pre-existing issues.
4. `cargo build --workspace --release` — fix related failures; note pre-existing issues.
5. Invoke `test-writer` when: reviewers identify missing tests, behavior changed without adequate coverage, or sensitive modules were modified and adversarial coverage is missing.
6. Apply output parsing protocol to `test-writer` result.
7. If tests fail, fix within scope and re-run once. Second failure → record as blocker in final report.

---

## Phase 7 — Final Implementation Report

Write `.claude/reviews/fix-<scope-slug>-<YYYYMMDD-HHMMSS>.md`. Ensure `.claude/reviews/` exists. Include even if no actionable fixes were applied (state why).

Report sections:
- Implementation Context Snapshot (phase status, plan/handoff files consulted)
- Triage Summary (actionable, deferred, intentional, conflict counts)
- Fixes Applied (finding IDs, files changed, change summary, rationale, risk reduced)
- Architecture Outcome (structural findings before/after, cross-shard issues, remaining blockers)
- Design Challenge Ledger (challenged constraint, resolution, implementation effect, follow-up owner)
- Finding Overrides table (CF-NNN, cycles unresolved, confidence, rationale, decision)
- Validation Summary (re-review severities before/after, cross-shard invocations, test outcomes, remaining blockers)
- Deferred / Not Applied (reason, plan citation, follow-up phase)
- Appendix (files reviewed, shard/finding counts, agent chain by cycle, rule/design refs cited, run state path)

---

## Guardrails

- No commits, pushes, or PR actions. No destructive git commands.
- Do not broaden scope unless required by direct dependency; if broadened, report why.
- Do not implement deferred-phase functionality.
- Every major implementation decision must cite relevant plan files when phase context influenced the choice.
- Do not run automatically from `/review-only`; implementation requires explicit operator invocation.
- `cross-shard-reviewer` receives `SHARD_DIGEST_SUMMARY[]` + `INTERFACE_SLICE` only — never full `DIGEST_SLICE` content.
- Override Records are prohibited for CRITICAL findings.
- Security-scoped design challenge decisions require explicit user input before `problem-solver` proceeds.
- Do not mark a fix complete unless `cargo check --workspace` passes after implementation.

## Failure Modes

| Condition | Action |
|---|---|
| No review file found/readable | Halt |
| Scope resolves to zero files | Halt |
| Baseline `cargo check` fails | Halt before any fixes |
| Gatherer malformed after retry | Halt Phase 0 |
| Any agent parse failure after retry | Halt with `PARSE_ERROR`; surface raw output |
| Security-scoped design challenge awaits user decision | Hard pause; resume on input |
| HIGH Override Record confidence `UNCERTAIN` | Hard pause; resume on input |
| `BLOCKED_SOLUTIONS` with no unblocked groups | Halt; include blockers in report |
| `rust-implementer` BLOCKED and fallback infeasible | Halt; report unresolved item |
| Remediation thresholds unmet after max cycles | Halt; recommend `/plan` for formal resolution |