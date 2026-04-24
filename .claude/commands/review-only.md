# `/review-only` — Rust Review Command

Run full Rust review for: $ARGUMENTS

---

## Agent Roster, Models & Contracts

| Agent | Model | Role | Output |
|---|---|---|---|
| `plan-context-builder` | `claude-haiku-4-5` | Parse plan/handoff files | `PLAN_DIGEST` |
| `rules-extractor` | `claude-haiku-4-5` | Extract rules verbatim | `RULES_INDEX` |
| `design-extractor` | `claude-haiku-4-5` | Extract design invariants verbatim | `DESIGN_INDEX` |
| `shard-planner` | `claude-haiku-4-5` | Map files to shards + keyword scan | `SHARD_MAP` + `SHARD_DIGEST_SUMMARY[]` |
| `rust-reviewer` | `claude-sonnet-4-6` | Deep Rust code review per shard | Raw findings |
| `architecture-reviewer` | `claude-sonnet-4-6` | Architecture integrity per shard | Raw findings |
| `security-reviewer` | `claude-sonnet-4-6` | Security review (conditional) | Raw findings |
| `cross-shard-reviewer` | `claude-haiku-4-5` | Cross-shard contradiction detection | Raw findings |
| `finding-classifier` | `claude-haiku-4-5` | Classify findings by disposition/confidence | `CLASSIFIED_FINDINGS` |
| `problem-solver` | `claude-sonnet-4-6` | Recommendations only (no code) | `SOLUTION_PACK` |
| `report-writer` | `claude-haiku-4-5` | Render final Markdown report | `REPORT_WRITER_RESULT` |

Producer schemas are authoritative in `.claude/agents/<agent-name>.md`. This command owns orchestration and gates only.

**Invocation rule (hard):** Every agent MUST be invoked via `task` tool with the model above. The orchestrator MUST NOT review code, classify findings, synthesize solutions, or write reports directly — this is a protocol violation regardless of reasoning quality. Never pass one agent's full raw output to another; extract and pass structured fields only.

---

## Design Principles

- **Orchestrator stays thin.** Route, sequence, merge structured outputs only.
- **Structured contracts, not prose.** All inter-agent I/O uses defined structured fields.
- **Parallelism is the default.** Serialize only when strict data dependency requires it.
- **Context-bounded cycles.** Persist cycle state to disk; carry forward only IDs and severities in working memory.
- **Lossless high-authority extraction.** Gatherer agents emit verbatim excerpts for rules, design invariants, and plan rationale — never paraphrased.

## Canonical Designs and Rules

1. `docs/architecture/design-invariants.md`
2. `docs/architecture/designs/*/design.md`
3. `.claude/rules/*.md`

---

## Scope Resolution

- Empty or `all` → all `src-tauri/src/**/*.rs` + `src/*.rs`
- Otherwise: resolve `$ARGUMENTS` to concrete Rust file paths
- Zero files resolved → **halt**

## Cycle Configuration

- Default: **2 cycles** (override: `cycles=<N>` or `--cycles <N>`; valid range 1–10)
- Stable identifiers: `cycle-1`, `cycle-2`, ..., `cycle-N`
- Invalid value → **halt**

## Track Selection

| Condition | Track |
|---|---|
| Security shards present (`auth/`, `crypto/`, `storage/`) OR >10 files | `full` — all agents, all waves, max configured cycles |
| 4–10 non-security files, no security shard overlap | `standard` — rust+arch reviewers; security only if keyword trigger; cross-shard if ≥2 shards |
| ≤3 non-security files, single shard | `minimal` — rust-reviewer only, 1 cycle; escalate to `standard` if any HIGH surfaces |

Track is locked after selection and recorded in the report header.

## Baseline Configuration

- **Strict** (default): `cargo check --workspace` failure = hard stop.
- `baseline=degraded` / `--degraded-baseline`: continue only for environment/toolchain failures (not source errors); classify failure with evidence.
- `--skip-check`: mark `SKIPPED`, add warning to report, continue.

## Output Parsing Protocol

After every agent invocation:
1. Locate the named output block (e.g., `PLAN_DIGEST`, `CLASSIFIED_FINDINGS`). Strip prose wrappers/fences.
2. Validate all required top-level fields per the agent's contract.
3. On failure: re-invoke once, prepending: `"Your previous output did not match the required schema. Return only the structured block specified in your agent contract — no prose preamble, no markdown fences unless part of the schema."`
4. Second failure → halt with `PARSE_ERROR` (agent name, expected schema, raw output). Do not infer missing fields.

---

## Phase 0 — Parallel Preflight

Spawn **in parallel** via `task` tool. The orchestrator does not read plan, rules, or design files directly.

- `plan-context-builder` → `.claude/plans/phase-*.md`, `.claude/plans/HANDOFF-*.md`
- `rules-extractor` → `.claude/rules/*.md`
- `design-extractor` → `docs/architecture/designs/**/design.md`, `docs/architecture/design-invariants.md`
- `shard-planner` → resolved file list (runs in parallel with above)
- `cargo check --workspace` (concurrent; result consumed in Phase 1)

Generate run ID: `review-<scope-slug>-<YYYYMMDD-HHMMSS>`. Write initial run state to `.claude/reviews/<run-id>/run-state.json`:

```json
{
  "run_id": "", "scope": "", "track": "", "cycle_count": 0,
  "canonical_finding_count": 0,
  "finding_summary": { "CRITICAL": 0, "HIGH": 0, "MEDIUM": 0, "LOW": 0 },
  "disposition_summary": { "ACTIONABLE_NOW": 0, "INTENTIONAL_DECISION": 0, "DEFERRED_BY_PLAN": 0, "INSUFFICIENT_EVIDENCE": 0 },
  "cycles": []
}
```

Apply output parsing protocol to each. Halt Phase 0 on any malformed output after retry.

### 0-D: Build Shard-Scoped Digest Slices (orchestrator step)

Once `PLAN_DIGEST`, `RULES_INDEX`, `DESIGN_INDEX` are available, build one `DIGEST_SLICE_<shard_id>` per shard containing only the rules, design invariants, and plan context relevant to that shard's scope. Do NOT pass full global indices to reviewer agents.

Shard-to-scope mapping:
- `shard-auth` → `src-tauri/src/auth/**` — scopes: `auth`, `global`
- `shard-crypto` → `src-tauri/src/crypto/**` — scopes: `crypto`, `global`
- `shard-storage` → `src-tauri/src/storage/**` — scopes: `storage`, `global`
- `shard-default` → remaining `src-tauri/src/**` — scope: `global`

Wait for all four structured outputs before proceeding.

---

## Phase 1 — Baseline Gate

Consume `cargo check --workspace` result per Baseline Configuration. Environment/toolchain classification must be evidence-based (error signature + message). Report in appendix.

---

## Phase 2 — Multi-Cycle Sharded Review

### Per-Cycle State

Maintain a rolling `CANONICAL_FINDINGS` list updated after each cycle. Cycles 2–N receive it as suppression input (IDs + one-line descriptions only). Persist full records to disk — orchestrator carries forward only IDs and severities in working memory.

### Wave 1 — Parallel Reviewer Invocation (mandatory, all tracks)

For each shard, spawn in parallel:
- `rust-reviewer` with `DIGEST_SLICE_<shard_id>` + shard file list + suppression list (cycles 2–N)
- `architecture-reviewer` with same inputs (`standard` and `full` tracks only)

Suppression prefix for cycles 2–N: `"The following findings are already canonical. Report only NEW findings or direct contradictions."` followed by CF-NNN list with one-line descriptions.

### Wave 2 — Conditional Security Review

Invoke `security-reviewer` per shard **only if** any of:
- `shard.is_security_sensitive == true` (always for auth/crypto/storage)
- Any Wave 1 finding for this shard has `security_flag: true`
- `shard.security_keyword_hits` is non-empty

Input: `DIGEST_SLICE_<shard_id>` + Wave 1 findings for this shard.

### Wave 3 — Cross-Shard Review (`full`/`standard` tracks, ≥2 shards in scope)

After all shards complete Waves 1+2, invoke `cross-shard-reviewer` **once** for the cycle.

Before invoking, extract boundary pub signatures:
```bash
grep -rn "^pub fn\|^pub trait\|^pub struct\|^pub enum\|^pub type" <boundary files>
```

Input: `SHARD_MAP`, structured Wave 1+2 findings (fields only — not full agent outputs), `CANONICAL_FINDINGS` suppression (IDs only, cycles 2–N), `SHARD_DIGEST_SUMMARY[]` (IDs only), `INTERFACE_SLICE`. Never pass full `DIGEST_SLICE` content.

Wave 3 is serial at cycle end — next cycle cannot start until it completes.

### Required Finding Fields

Every finding must include: `id`, `cycle_id`, `reviewer`, `shard_id`, `severity`, `category`, `location`, `problem`, `evidence`, `rule_refs[]`, `design_refs[]`, `plan_context`, `recommended_fix`, `proposed_solution`, `risk_if_unchanged`, `security_flag`, `design_challenge`. Discard findings missing required fields; log in report appendix.

Severity normalization (applied by orchestrator after collection):

| Raw | Normalized |
|---|---|
| CRITICAL | CRITICAL |
| HIGH | HIGH |
| WARNING (security-reviewer) | MEDIUM |
| NOTE (security-reviewer) | LOW |
| MEDIUM/LOW | unchanged |

> `/implement-review` applies stricter normalization at ingestion: `WARNING→HIGH`, `NOTE→MEDIUM`.

### Per-Cycle Deduplication and Canonical Update

1. Deduplicate within cycle by root cause + location.
2. Merge into `CANONICAL_FINDINGS`: same root cause+location → increment `occurrence_count`; contradiction → `has_contradiction: true`; new → `occurrence_count: 1`.
3. Per-shard limits: keep all CRITICAL/HIGH; ≤20 MEDIUM (highest impact); ≤10 LOW.
4. Persist: `.claude/reviews/<run-id>/cycle-<N>.json` `{ cycle, findings[{canonical_id,severity,occurrence_count}], cross_shard_finding_count, security_reviewer_invocations, canonical_finding_count }`. Update `run-state.json`.

**Context compaction:** if context budget is insufficient for another full cycle, persist state, emit `CONTEXT_CHECKPOINT`, continue from disk.

---

## Phase 2.5 — Finding Classification

Invoke `finding-classifier` via `task` tool with: full `CANONICAL_FINDINGS`, `PLAN_DIGEST`, `RULES_INDEX`, `DESIGN_INDEX`, and previous-cycle `actionable_now` IDs (for override eligibility).

Dispositions (applied by agent per its contract):
- `ACTIONABLE_NOW`: violates rule/design invariant AND within implemented/in-progress scope
- `INTENTIONAL_DECISION`: explicitly justified by plan/handoff (must cite exact section)
- `DEFERRED_BY_PLAN`: belongs to not-yet-implemented phase
- `INSUFFICIENT_EVIDENCE`: no concrete location, no citation, or no multi-cycle reproduction

`INSUFFICIENT_EVIDENCE` findings are never passed to `problem-solver`.

---

## Phase 3 — Solution Synthesis (no code changes)

Skip if `CLASSIFIED_FINDINGS.actionable_now` is empty.

Group and spawn `problem-solver` agents in parallel:
- One per CRITICAL/HIGH finding (isolated)
- One per shard for MEDIUM findings (≤10 per agent)
- One for all LOWs

Each agent receives only: `{ findings[], relevant_files[], digest_slice, design_challenge_entries[], approved_design_challenges: [], instruction: "Produce recommendations only. No code edits." }`

Deep-dive: all CRITICAL/HIGH; MEDIUM only when `blast_radius` is CROSS-MODULE/SYSTEM or `has_contradiction: true`; LOWs concise.

---

## Phase 4 — Report Writer

Invoke `report-writer` via `task` tool. Input: `PLAN_DIGEST`, `SHARD_MAP`, `CANONICAL_FINDINGS` (full with recurrence metadata), `CLASSIFIED_FINDINGS`, all merged `SOLUTION_PACK` outputs, baseline result, cycle count + per-cycle summaries (including cross-shard finding counts), `DESIGN_CHALLENGE_LEDGER`, scope slug, timestamp.

Output path: `.claude/reviews/review-<scope-slug>-<YYYYMMDD-HHMMSS>.md`. Ensure `.claude/reviews/` exists.

The report must include:
- Appendix B cycle summary table with a `Cross-Shard Findings` column.
- Appendix K machine-readable JSON export (fenced ` ```json ``` `): `source_report`, `report_timestamp`, `scope`, `actionable_findings[]` (with all finding fields + `blast_radius`, `estimated_complexity`, `design_challenge`), `design_challenge_ledger[]`. This is the bridge to `/implement-review`.

---

## Guardrails

- **Review-only mode is absolute.** Never modify application source files.
- Allowed writes: report file and run-state artifacts under `.claude/reviews/` only.
- No commits, pushes, or destructive git commands.
- Gatherer agents must use verbatim extraction; paraphrasing rules or design invariants is not permitted.
- `cross-shard-reviewer` receives structured findings + `SHARD_DIGEST_SUMMARY` IDs + `INTERFACE_SLICE` only — never full `DIGEST_SLICE` content.
- Every `ACTIONABLE_NOW` finding must cite at least one `rule_refs` or `design_refs` entry; findings without citations must be reclassified as `INSUFFICIENT_EVIDENCE`.

## Failure Modes

| Condition | Action |
|---|---|
| Scope resolves to zero files | Halt; report unresolved scope |
| Invalid cycle count | Halt |
| Baseline gate violates policy | Per Baseline Configuration policy |
| Gatherer malformed after retry | Halt Phase 0 |
| `cross-shard-reviewer` malformed after retry | Halt cycle; do not start next |
| Any agent parse failure after retry | Halt with `PARSE_ERROR`; surface raw output |
| Reviewer finding missing required fields | Discard; log in appendix |
| All `problem-solver` return `BLOCKED_SOLUTIONS` | Include blockers in report |
| Report writer fails | Write minimal plain-text fallback to stdout |