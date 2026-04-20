# `/review-only` — Optimized Rust Review Command

Run a full Rust review-only flow for: $ARGUMENTS

---

## Design Principles

- **Orchestrator stays thin.** It routes, sequences, and merges structured outputs. It never performs deep reasoning on file contents or finding semantics itself.
- **Agents own their domain.** Each agent receives only the context relevant to its task — no global dumps.
- **Structured contracts, not prose.** All inter-agent I/O uses defined structured fields. Agents never return unstructured narrative that the orchestrator must parse and interpret.
- **Parallelism is the default.** Serialize only when strict data dependency requires it.
- **Summarization must be lossless for high-authority items.** Gatherer agents emit verbatim excerpts and source citations for rules, design invariants, and plan rationale — never paraphrased prose that loses nuance.
- **Context-bounded cycles.** Cycle state is persisted to disk; the orchestrator never accumulates full finding records across cycles in working memory.

---

## Agent Roster

| Agent | Role | Input | Output |
|---|---|---|---|
| `plan-context-builder` | Parses plan + handoff files into structured digest | Plan files | `PLAN_DIGEST` |
| `rules-extractor` | Extracts authority rules as structured anchors | Rules files | `RULES_INDEX` |
| `design-extractor` | Extracts design invariants as structured anchors | Design docs | `DESIGN_INDEX` |
| `shard-planner` | Resolves scope to file shards and emits digest summaries | File paths | `SHARD_MAP` + `SHARD_DIGEST_SUMMARY[]` |
| `rust-reviewer` | Rust code review per shard | Shard + digest slice | Raw findings |
| `architecture-reviewer` | Architecture review per shard | Shard + digest slice | Raw findings |
| `security-reviewer` | Security review (wave 2, conditional) | Shard + digest slice | Raw findings |
| `cross-shard-reviewer` | Cross-shard contradiction and integration risk review | Cycle findings + shard map + digest summaries | Raw findings |
| `finding-classifier` | Disposes and confidence-rates canonical findings | Canonical findings + digests | `CLASSIFIED_FINDINGS` |
| `problem-solver` | Recommendation-only per finding group | Finding group + shard slice | `SOLUTION_PACK` |
| `report-writer` | Renders final Markdown report | All structured outputs | Report file |

---

## Structured Contract Ownership (Hard)

| Artifact | Authoritative producer contract |
|---|---|
| `PLAN_DIGEST` | `.claude/agents/plan-context-builder.md` |
| `RULES_INDEX` | `.claude/agents/rules-extractor.md` |
| `DESIGN_INDEX` | `.claude/agents/design-extractor.md` |
| `SHARD_MAP` + `SHARD_DIGEST_SUMMARY[]` | `.claude/agents/shard-planner.md` |
| `CLASSIFIED_FINDINGS` | `.claude/agents/finding-classifier.md` |
| `SOLUTION_PACK` / `NO_ACTIONABLE_FIXES` / `BLOCKED_SOLUTIONS` | `.claude/agents/problem-solver.md` |
| `REPORT_WRITER_RESULT` | `.claude/agents/report-writer.md` |

This command owns orchestration and gates. Producer schema details live in agent contracts.

---

## Tool Invocation Contract (Hard)

**Every named agent in the Agent Roster MUST be invoked via the `task` tool.** The orchestrator MUST NOT perform review, classification, solution synthesis, or report assembly directly. The `task` tool runs each agent in an isolated context window — this is the core context-preservation mechanism of this command.

```
task(agent_type="plan-context-builder", model="claude-sonnet-4.6", ...)    → PLAN_DIGEST
task(agent_type="rules-extractor",      model="claude-sonnet-4.6", ...)    → RULES_INDEX
task(agent_type="design-extractor",     model="claude-sonnet-4.6", ...)    → DESIGN_INDEX
task(agent_type="shard-planner",        model="claude-sonnet-4.6", ...)    → SHARD_MAP + SHARD_DIGEST_SUMMARY[]
task(agent_type="rust-reviewer",        model="claude-sonnet-4.6",   ...)    → Raw findings
task(agent_type="architecture-reviewer",model="claude-sonnet-4.6",   ...)    → Raw findings
task(agent_type="security-reviewer",    model="claude-sonnet-4.6",   ...)    → Raw findings
task(agent_type="cross-shard-reviewer", model="claude-sonnet-4.6", ...)    → Raw findings
task(agent_type="finding-classifier",   model="claude-sonnet-4.6", ...)    → CLASSIFIED_FINDINGS
task(agent_type="problem-solver",       model="claude-sonnet-4.6",   ...)    → SOLUTION_PACK / NO_ACTIONABLE_FIXES / BLOCKED_SOLUTIONS
task(agent_type="report-writer",        model="claude-sonnet-4.6",   ...)    → REPORT_WRITER_RESULT
```

All custom agent names in the Agent Roster map directly to `agent_type` values. Skipping an agent invocation is a protocol violation, not a valid optimization.

## Model Assignments

Apply these model overrides on every `task` invocation. Never omit the `model` parameter.

| Agent | Model | Rationale |
|---|---|---|
| `rust-reviewer` | `claude-sonnet-4.6` | Deep code review with security/rule awareness across large shards |
| `security-reviewer` | `claude-sonnet-4.6` | Crypto correctness and zero-knowledge threat model — no false negatives tolerable |
| `architecture-reviewer` | `claude-sonnet-4.6` | Broad cross-cutting structural analysis requiring deep reasoning |
| `problem-solver` | `claude-sonnet-4.6` | Complex solution synthesis across multiple classified findings and design challenges |
| `report-writer` | `claude-sonnet-4.6` | Final report assembly requires synthesizing all findings with high fidelity |
| `finding-classifier` | `claude-sonnet-4.6` | Structured disposition classification — accurate table output, no deep reasoning needed |
| `cross-shard-reviewer` | `claude-sonnet-4.6` | Pattern-based contradiction detection using structured shard digests |
| `shard-planner` | `claude-sonnet-4.6` | File-to-shard mapping and keyword classification — structured analysis |
| `plan-context-builder` | `claude-sonnet-4.6` | Document parsing and structured extraction |
| `rules-extractor` | `claude-sonnet-4.6` | Text extraction from rule files — mechanical |
| `design-extractor` | `claude-sonnet-4.6` | Design invariant extraction — mechanical |

---

## Scope Resolution

1. If `$ARGUMENTS` is empty or `all`, set scope to all Rust implementation files under `src-tauri/src/**/*.rs`.
2. If `$ARGUMENTS` is provided, treat it as the review scope after extracting any cycle-count tokens. Resolve to concrete Rust file paths before proceeding.
3. If no files resolve, **halt** and report the unresolved scope. Do not proceed.

---

## Cycle Configuration

1. Default cycle count: `3`.
2. Optional override via `$ARGUMENTS` tokens: `cycles=<N>` or `--cycles <N>`.
3. Valid range: integer in `[1, 10]`. If invalid, **halt** and report invalid cycle configuration.
4. Use stable identifiers: `cycle-1`, `cycle-2`, ..., `cycle-N`.
5. File scope and per-shard digest slices are identical across all cycles unless scope resolution itself changes.

---

## Track Selection

Evaluate scope before context build. Track is locked after selection and recorded in the report header.

| Condition | Track |
|---|---|
| Security-sensitive shards present (`auth/`, `crypto/`, `storage/`), OR > 10 files in scope | `full` |
| 4–10 non-security files, no security shard overlap | `standard` |
| ≤ 3 non-security files, single anticipated shard | `minimal` |

Track capabilities:

- **`full`** — all agents, all waves, cross-shard review, max configured cycles.
- **`standard`** — rust-reviewer + architecture-reviewer + finding-classifier + problem-solver + report-writer; cross-shard only if 2+ shards touched; security-reviewer only if keyword triggers fire.
- **`minimal`** — rust-reviewer + finding-classifier + report-writer; 1 review cycle; no architecture-reviewer; no cross-shard review. If any HIGH finding surfaces → automatically escalate to `standard` and continue.

---

## Baseline Configuration

1. Default baseline mode is **strict**.
2. Optional degraded mode token: `baseline=degraded` or `--degraded-baseline`.
3. Optional skip token: `--skip-check`.
4. In strict mode, any `cargo check --workspace` failure is a hard stop.
5. In degraded mode, continue **only** when failure is classified as environment/toolchain-related (missing linker, system package, or toolchain component); source-level compile or type errors are still a hard stop.
6. If `--skip-check` is present, baseline is marked `SKIPPED` in the report and Phase 2 proceeds with an explicit warning.

---

## Authority Order (Hard)

1. `.claude/rules/*.md` — primary, normative
2. `docs/architecture/designs/**/design.md` and `docs/architecture/design-invariants.md` — canonical design
3. `.claude/reference/*.md` — secondary pattern guidance only; never overrides rules or design
4. `architecture-reviewer` may challenge rules/designs **only** via explicit `design_challenge` entries. No silent override is permitted.

---

## Output Parsing Protocol

Apply after every agent invocation, including gatherers, reviewers, classifier, solver, and report-writer.

1. Locate the named output block by scanning for its keyword header (e.g., `PLAN_DIGEST`, `CANONICAL_FINDINGS`, `CLASSIFIED_FINDINGS`, `SOLUTION_PACK`, `REPORT_WRITER_RESULT`). Strip any prose wrapper or markdown fences.
2. Validate that all required top-level fields are present per the agent's output contract.
3. **If the block is not found or required fields are missing:**
   a. Re-invoke the agent once. Prepend the raw output to the new invocation with: `"Your previous output did not match the required schema. Return only the structured block specified in your agent contract — no prose preamble, no markdown fences unless part of the schema."`
   b. If the second attempt also fails: halt with `PARSE_ERROR`. Record the agent name, expected schema, and raw output. Surface to the user. Do not infer missing field values.
4. Do not proceed with a partially parsed output.

---

## Phase 0 — Parallel Preflight

Spawn all agents and the baseline check **in parallel via the `task` tool**. The orchestrator does not read plan, rules, or design files directly — it consumes only structured outputs.

Parallel launch set:
- `plan-context-builder` (Step 0-A)
- `rules-extractor` (Step 0-B)
- `design-extractor` (Step 0-C)
- `shard-planner` (Step 0-E, parallel with 0-A/B/C)
- baseline command: `cargo check --workspace` (unless `--skip-check`)

Generate a run ID: `review-<scope-slug>-<YYYYMMDD-HHMMSS>`. Write initial run state to `.claude/reviews/<run-id>/run-state.json`:

```json
{
  "run_id": "<run-id>",
  "scope": "<resolved scope>",
  "track": "<minimal|standard|full>",
  "cycle_count": 0,
  "canonical_finding_count": 0,
  "finding_summary": { "CRITICAL": 0, "HIGH": 0, "MEDIUM": 0, "LOW": 0 },
  "disposition_summary": {
    "ACTIONABLE_NOW": 0, "INTENTIONAL_DECISION": 0,
    "DEFERRED_BY_PLAN": 0, "INSUFFICIENT_EVIDENCE": 0
  },
  "cycles": []
}
```

### 0-A: `plan-context-builder`

**Input:** All files matching `.claude/plans/phase-*.md` and `.claude/plans/HANDOFF-*.md`

**Task:** Parse and extract. Do not summarize into prose. Emit structured output only.

**Output — `PLAN_DIGEST`:**

Authoritative producer contract: `.claude/agents/plan-context-builder.md`.

Orchestrator-consumed fields (required):
- `highest_implemented_phase`, `in_progress_phases`, `deferred_phases`
- `plans[].{file,status,roadmap_phase,sub_phase,title,rationale_bullets,deferred_items,known_constraints}`
- `handoffs[].{file,trade_offs,deferrals}`

Apply output parsing protocol. Halt Phase 0 if required fields are missing or malformed after retry.

---

### 0-B: `rules-extractor`

**Input:** All files matching `.claude/rules/*.md`

**Task:** Extract each distinct rule as a structured anchor. Do not interpret or summarize — extract.

**Output — `RULES_INDEX`:**

Authoritative producer contract: `.claude/agents/rules-extractor.md`.

Orchestrator-consumed fields (required):
- `rules[].{id,source_file,anchor,verbatim,scope,severity_if_violated}`

Apply output parsing protocol. Halt Phase 0 if required fields are missing or malformed after retry.

---

### 0-C: `design-extractor`

**Input:** `docs/architecture/designs/**/design.md` and `docs/architecture/design-invariants.md`

**Task:** Extract each design invariant and constraint as a structured anchor.

**Output — `DESIGN_INDEX`:**

Authoritative producer contract: `.claude/agents/design-extractor.md`.

Orchestrator-consumed fields (required):
- `invariants[].{id,source_file,anchor,verbatim,scope,challenged}`

Apply output parsing protocol. Halt Phase 0 if required fields are missing or malformed after retry.

---

### 0-D: Orchestrator — Build Shard-Scoped Digest Slices

Once `PLAN_DIGEST`, `RULES_INDEX`, and `DESIGN_INDEX` are returned, the orchestrator builds a **per-shard digest slice** for each shard. Each reviewer agent receives only its shard's slice.

**Shard-to-scope mapping (default):**

| Shard ID | Path pattern | Relevant scopes |
|---|---|---|
| `shard-auth` | `src-tauri/src/auth/**` | `auth`, `global` |
| `shard-crypto` | `src-tauri/src/crypto/**` | `crypto`, `global` |
| `shard-storage` | `src-tauri/src/storage/**` | `storage`, `global` |
| `shard-default` | remaining `src-tauri/src/**` | `global` |

**Per-shard digest slice structure:**

```
DIGEST_SLICE_<shard_id> {
  shard_id: "<shard-auth|shard-crypto|shard-storage|shard-default>"
  plan_context: {
    highest_implemented_phase: "<value>"
    in_progress_phases: [...]
    deferred_phases: [...]
    relevant_rationale: [...]    // only bullets whose scope intersects shard scopes
    relevant_deferrals: [...]    // only deferrals relevant to this shard
  }
  rules: [...]                   // RULES_INDEX entries matching shard scopes
  design_invariants: [...]       // DESIGN_INDEX entries matching shard scopes
}
```

**Guardrail:** Do not pass the full `PLAN_DIGEST`, `RULES_INDEX`, or `DESIGN_INDEX` to reviewer agents. Each agent receives only its shard's slice.

---

### 0-E: `shard-planner` (parallel with 0-A/B/C)

**Input:** Resolved file list from scope resolution

**Task:** Map each file to its shard based on the path pattern table in Step 0-D. Flag any files that don't match a named shard pattern as `shard-default`. Also emit a `SHARD_DIGEST_SUMMARY` per shard for use by `cross-shard-reviewer`.

**Output — `SHARD_MAP` + `SHARD_DIGEST_SUMMARY[]`:**

Authoritative producer contract: `.claude/agents/shard-planner.md`.

Orchestrator-consumed fields (required):
- `shards[].{shard_id,files,is_security_sensitive,security_keyword_hits}`
- `security_trigger_keywords`, `total_files`
- `shard_digest_summaries[].{shard_id,scopes,rule_ids,design_ids,implemented_phases,deferred_phases}`

**`SHARD_DIGEST_SUMMARY` structure** (used exclusively by `cross-shard-reviewer`; contains IDs only, not full verbatim text):

```
SHARD_DIGEST_SUMMARY {
  shard_id: "<shard-id>"
  scopes: ["auth" | "crypto" | "storage" | "global" | ...]
  rule_ids: ["<R-NNN>", ...]       // IDs of rules governing this shard
  design_ids: ["<D-NNN>", ...]     // IDs of design invariants governing this shard
  implemented_phases: ["<phase>"]
  deferred_phases: ["<phase>"]
}
```

Apply output parsing protocol. Halt Phase 0 if required fields are missing or malformed after retry.

The orchestrator waits for all four structured outputs (0-A, 0-B, 0-C, 0-E) before proceeding. Baseline result is resolved in Phase 1.

---

## Phase 1 — Baseline Gate

1. If `--skip-check` is present, set baseline status to `SKIPPED`, add warning note to the final report, and continue to Phase 2.
2. Otherwise, consume the `cargo check --workspace` result started in Phase 0 and apply the decision policy from **Baseline Configuration**.
3. Environment/toolchain classification in degraded mode must be evidence-based (error signature and message) and reported explicitly in the final report appendix.

---

## Phase 2 — Multi-Cycle Sharded Review

### Per-Cycle State

The orchestrator maintains a rolling `CANONICAL_FINDINGS` list that is updated after each cycle. Cycles 2–N receive it as a "known findings" suppression list. The full list is persisted to disk after each cycle — the orchestrator carries forward only IDs and severities in working memory.

### Cycle Execution (repeat for cycle-1 through cycle-N)

#### Step 2-A: Wave 1 — Parallel Reviewer Invocation

For each shard in `SHARD_MAP`, invoke in **parallel via `task` tool** (mandatory — HALT if skipped):

- `rust-reviewer` with `DIGEST_SLICE_<shard_id>` + shard file list + current `CANONICAL_FINDINGS` suppression list (IDs + one-line descriptions only, from cycle 2 onward)
- `architecture-reviewer` with same inputs (required for every shard, every cycle — `standard` and `full` tracks only)

Apply output parsing protocol to each reviewer result.

**Suppression instruction for cycles 2–N:**

> "The following findings are already canonical from prior cycles. Do not re-report them unless you observe a direct contradiction or significant new evidence. Report only NEW findings or contradictions."
> `<CANONICAL_FINDINGS list — IDs and one-line descriptions only>`

#### Step 2-B: Wave 2 — Conditional Security Review

After Wave 1 completes for a shard, invoke `security-reviewer` via `task` tool on that shard **only if**:

- `shard.is_security_sensitive == true` (always true for `shard-auth`, `shard-crypto`, `shard-storage`), **OR**
- any Wave 1 finding for this shard includes `security_flag: true`, **OR**
- `shard.security_keyword_hits` is non-empty (primary trigger for `shard-default`)

`security-reviewer` receives the same `DIGEST_SLICE_<shard_id>` plus the Wave 1 findings for its shard as additional context.

Apply output parsing protocol. **If no condition is met, skip `security-reviewer` for this shard entirely.**

#### Step 2-C: Wave 3 — Cross-Shard Consistency Review (`full` and `standard` tracks, when 2+ shards in scope)

After Wave 1 and Wave 2 complete for **all shards** in a cycle, invoke `cross-shard-reviewer` via `task` tool **once** for that cycle.

**Before invoking `cross-shard-reviewer`, extract boundary pub signatures** from files at the interface between changed shards:

```bash
grep -rn "^pub fn\|^pub trait\|^pub struct\|^pub enum\|^pub type" \
  <files at the boundary between each pair of shards>
```

Pass the resulting `INTERFACE_SLICE` alongside structured findings.

`cross-shard-reviewer` input:
- `SHARD_MAP`
- per-shard reviewer findings for the current cycle (Wave 1 + Wave 2, structured fields only — not full agent outputs)
- `CANONICAL_FINDINGS` suppression list (cycles 2–N, IDs only)
- `SHARD_DIGEST_SUMMARY[]` per shard (IDs only — not full `DIGEST_SLICE` content)
- `INTERFACE_SLICE` (pub boundary signatures only)

`cross-shard-reviewer` mission:
- find contradictions across shard-local recommendations
- detect boundary contract mismatches spanning shard interfaces
- emit only net-new cross-shard findings or contradiction evidence

Apply output parsing protocol.

**Note:** Wave 3 is a serial dependency at cycle end — the next cycle cannot start until `cross-shard-reviewer` returns. Since it reads only structured finding data (not source files), it should be fast.

#### Step 2-D: Required Finding Structure

Every finding returned by any reviewer agent must conform to this schema. The orchestrator rejects and discards any finding that does not include all required fields.

```
FINDING {
  id: "<reviewer-shard-cycle-NNN>"
  cycle_id: "<cycle-1|cycle-2|...>"
  reviewer: "<rust-reviewer|architecture-reviewer|security-reviewer|cross-shard-reviewer>"
  shard_id: "<shard-auth|shard-crypto|shard-storage|shard-default>"
  severity: "<CRITICAL|HIGH|MEDIUM|LOW|WARNING|NOTE>"
  category: "<category string>"
  location: "<file>:<line> or module path or cross-shard>"
  problem: "<what is wrong and why it matters>"
  evidence: "<specific observation with rule/design citation>"
  rule_refs: ["<R-NNN>", ...]
  design_refs: ["<D-NNN>", ...]
  plan_context: "<relevant phase or rationale, one line>"
  recommended_fix: "<clear recommendation>"
  proposed_solution: "<concrete approach, constraints, trade-offs>"
  risk_if_unchanged: "<impact>"
  security_flag: true|false
  design_challenge: {
    challenged_constraint: "<rule or design anchor>"
    rationale: "<why current constraint is suboptimal>"
    proposed_update: "<draft update direction>"
  } | null
}
```

**Severity normalization (applied by orchestrator after collection):**

| Raw severity | Normalized |
|---|---|
| CRITICAL | CRITICAL |
| HIGH | HIGH |
| WARNING (security-reviewer) | MEDIUM |
| MEDIUM | MEDIUM |
| NOTE (security-reviewer) | LOW |
| LOW | LOW |

> **Cross-command note:** `/implement-review` applies stricter normalization when ingesting this report — `WARNING → HIGH` and `NOTE → MEDIUM` — to avoid downgrading security risks at implementation time. The `disposition` field in Appendix K reflects the classification under this command's (review-phase) normalization. If plan state has changed since this report was generated, `/implement-review` will re-classify via `finding-classifier`.

#### Step 2-E: Per-Cycle Deduplication and Canonical Update (Rolling)

After all shards and Wave 3 complete for a cycle:

1. Collect all findings from Wave 1, Wave 2, and Wave 3 for this cycle.
2. Deduplicate within the cycle by root cause + location.
3. Merge new findings into `CANONICAL_FINDINGS`:
   - If a finding matches an existing canonical entry (same root cause + location), increment `occurrence_count` and add cycle to `cycle_hits`. Do not create a new entry.
   - If a finding contradicts an existing canonical entry, flag the canonical entry with `has_contradiction: true` and attach the new evidence.
   - If a finding is genuinely new, add it as a new canonical entry with `occurrence_count: 1`.
4. Persist cycle state to disk (see **Run-State Persistence**).
5. Update `CANONICAL_FINDINGS` before the next cycle starts. Pass updated list (IDs + one-line descriptions only) as the suppression input for cycle N+1.

**Per-shard output limits (applied before deduplication, per cycle):**

- Keep all CRITICAL/HIGH findings
- Include up to 20 MEDIUM findings (highest impact first)
- Include up to 10 LOW findings (deduplicated summaries)

#### Step 2-F: `CANONICAL_FINDINGS` Structure

```
CANONICAL_FINDING {
  canonical_id: "<CF-NNN>"
  severity: "<normalized severity>"
  category: "<category>"
  location: "<primary location>"
  affected_locations: ["<location>", ...]
  problem: "<canonical problem statement>"
  evidence: "<strongest evidence observed>"
  rule_refs: ["<R-NNN>", ...]
  design_refs: ["<D-NNN>", ...]
  plan_context: "<most relevant plan context>"
  recommended_fix: "<canonical recommendation>"
  proposed_solution: "<concrete approach>"
  risk_if_unchanged: "<impact>"
  occurrence_count: <N>
  cycle_hits: ["<cycle-1>", ...]
  reviewer_hits: ["<rust-reviewer>", ...]
  has_contradiction: true|false
  design_challenge: { ... } | null
}
```

---

### Run-State Persistence

After each cycle completes, write `.claude/reviews/<run-id>/cycle-<N>.json`:

```json
{
  "cycle": <N>,
  "findings": [{ "canonical_id": "CF-NNN", "severity": "...", "occurrence_count": <N> }],
  "cross_shard_finding_count": <N>,
  "security_reviewer_invocations": <N>,
  "canonical_finding_count": <N>
}
```

Update `run-state.json` with incremented `cycle_count` and cumulative summary counts.

**The orchestrator carries forward between cycles only:**
- CF-NNN → severity mapping for existing canonical findings (IDs and severities only)
- Running canonical finding count and severity summary

Full finding prose and `SHARD_DIGEST_SUMMARY` content must not accumulate across cycles. Reload from disk when a specific record is needed.

**Context compaction:** if the orchestrator estimates it cannot complete another full cycle within a safe context budget:
1. Persist current cycle state to disk.
2. Emit `CONTEXT_CHECKPOINT`: "Context compacted after cycle N — state at `.claude/reviews/<run-id>/`. Resuming from disk."
3. Continue with a fresh working context loaded from run-state only.

---

## Phase 2.5 — `finding-classifier` Agent (Quality Gate)

**Do not perform this classification in the orchestrator.** Spawn a dedicated `finding-classifier` agent via `task` tool (mandatory — HALT if skipped).

**Input:**
- Full `CANONICAL_FINDINGS` list
- `PLAN_DIGEST` (full — classifier needs global plan context)
- `RULES_INDEX`
- `DESIGN_INDEX`

**Task:** For each canonical finding, assign:

```
CLASSIFICATION {
  canonical_id: "<CF-NNN>"
  disposition: "<ACTIONABLE_NOW|INTENTIONAL_DECISION|DEFERRED_BY_PLAN|INSUFFICIENT_EVIDENCE>"
  confidence: "<HIGH|MEDIUM|LOW>"
  confidence_rationale: "<one line>"
  disposition_citation: "<plan file + section, or rule ID, or design ID>"
}
```

**Classification rules the classifier must apply:**

- `ACTIONABLE_NOW`: violates a rule or design invariant AND falls within an implemented or in-progress phase scope.
- `INTENTIONAL_DECISION`: explicitly justified by plan/handoff rationale. Classifier must cite the exact plan section. This is not a defect.
- `DEFERRED_BY_PLAN`: belongs to a not-yet-implemented phase. Document as deferred — not as a current implementation failure.
- `INSUFFICIENT_EVIDENCE`: finding lacks concrete location anchor, has no rule/design citation, or was not reproduced in any cycle with strong evidence.

**Confidence scoring:**

- `HIGH`: appeared in 2+ cycles AND has a concrete rule/design citation AND has a precise location.
- `MEDIUM`: appeared in 1+ cycles AND has either a citation OR a precise location (not both).
- `LOW`: single cycle, weak citation, or vague location.

**Output — `CLASSIFIED_FINDINGS`:**

Authoritative producer contract: `.claude/agents/finding-classifier.md`.

Orchestrator-consumed fields (required):
- `actionable_now`, `intentional_decisions`, `deferred_by_plan`, `insufficient_evidence`
- `design_challenge_ledger`
- each classification record includes `canonical_id`, `disposition`, `confidence`, `confidence_rationale`, `disposition_citation`

Apply output parsing protocol. Halt Phase 2.5 if required fields are missing or malformed after retry.

**Guardrail:** `INSUFFICIENT_EVIDENCE` findings are **never** passed to `problem-solver`. They go directly to the report writer for the appendix.

---

## Phase 3 — Parallel Solution Synthesis (no code changes)

If `CLASSIFIED_FINDINGS.actionable_now` is empty, skip to Phase 4.

### Finding Grouping Strategy

The orchestrator groups `actionable_now` findings **before** spawning `problem-solver` agents via `task` tool (mandatory — HALT if skipped):

| Group | Contents | Agent scope |
|---|---|---|
| One agent per CRITICAL/HIGH finding | Each CRITICAL/HIGH gets its own isolated agent | Single finding |
| One agent per shard for MEDIUM findings | Group MEDIUMs by shard_id | Up to 10 per agent |
| One agent for all LOW findings | Batch across all shards | All LOWs |

For N CRITICAL/HIGH findings + M shards with MEDIUMs + any LOWs, spawn `N + M + 1` agents in parallel.

### Per-Agent Input (scoped — do not pass global state)

Each `problem-solver` agent receives **only**:

```
PROBLEM_SOLVER_INPUT {
  findings: [<assigned CANONICAL_FINDING(s) with classification>]
  relevant_files: [<file paths from finding locations only>]
  digest_slice: <DIGEST_SLICE for the finding's shard(s)>
  design_challenge_entries: [<only DESIGN_CHALLENGE_LEDGER entries related to these findings>]
  approved_design_challenges: []   // review-only default
  instruction: "Produce recommendations only. No code edits. No file modifications."
}
```

### `problem-solver` Required Output

Each agent returns one structured payload per the authoritative producer contract in `.claude/agents/problem-solver.md`:
- `SOLUTION_PACK`
- `NO_ACTIONABLE_FIXES`
- `BLOCKED_SOLUTIONS`

Orchestrator-consumed fields (required):
- `SOLUTION_PACK.finding_ids`
- `SOLUTION_PACK.solutions[].{canonical_id,recommendation,implementation_approach,blast_radius,dependencies,estimated_complexity}`
- `NO_ACTIONABLE_FIXES.reason`
- `BLOCKED_SOLUTIONS.blockers`

Apply output parsing protocol to each solver result.

### Deep-Dive Rules

- Deep-dive all CRITICAL/HIGH by default (each has its own agent).
- Deep-dive MEDIUM only when `blast_radius` is `CROSS-MODULE` or `SYSTEM`, or when `has_contradiction: true`.
- Keep LOW recommendations concise unless directly security-sensitive.

---

## Phase 4 — `report-writer` Agent

**Do not assemble the report in the orchestrator.** Spawn a dedicated `report-writer` agent via `task` tool (mandatory — HALT if skipped).

**Input (all structured — no raw prose):**
- `PLAN_DIGEST`
- `SHARD_MAP`
- `CANONICAL_FINDINGS` (full, including recurrence metadata)
- `CLASSIFIED_FINDINGS` (all dispositions)
- All `SOLUTION_PACK` outputs merged
- Baseline result
- Cycle count and per-cycle shard summary
- `DESIGN_CHALLENGE_LEDGER`
- Scope slug and timestamp

**Output path:** `.claude/reviews/review-<scope-slug>-<YYYYMMDD-HHMMSS>.md`

Ensure directory `.claude/reviews/` exists before writing.

Apply output parsing protocol. Expected completion contract: `REPORT_WRITER_RESULT` (authoritative in `.claude/agents/report-writer.md`) with `status`, `path`, `summary`, and `error`.

### Report Structure

````markdown
# Review Report — <scope>

> Generated by `/review-only`
> Timestamp (UTC): <YYYY-MM-DD HH:MM:SS>
> Scope: <resolved scope>
> Track: <minimal|standard|full>
> Agents used: plan-context-builder, rules-extractor, design-extractor,
>              shard-planner, rust-reviewer, architecture-reviewer,
>              security-reviewer (conditional), cross-shard-reviewer,
>              finding-classifier, problem-solver (×<N>), report-writer

---

## Implementation Context Snapshot

- Highest implemented phase: <phase/sub-phase>
- In-progress phases: <list>
- Planned/draft phases: <list>
- Key plan files consulted: <list with status>
- Key handoff files consulted: <list>

---

## Executive Summary

- Review cycles run: <N>
- Track: <minimal|standard|full>
- Raw finding events (all cycles, all shards): <N>
- Unique canonical findings after rolling deduplication: <N>
- Repeated findings (seen in >1 cycle): <N>
- Critical/High: <N> | Medium: <N> | Low: <N>
- Problem-solver agents spawned: <N>
- Security-reviewer shards skipped (clean wave 1): <N>
- Cross-shard review invocations: <N>
- Filtered as insufficient evidence: <N>
- Status: <No actionable findings | Action required>

---

## Findings by Severity

| Severity | Count |
|---|---:|
| CRITICAL/HIGH | <N> |
| MEDIUM | <N> |
| LOW | <N> |

## Architectural Risk Overview

| Category | Count |
|---|---:|
| SRP / boundary integrity | <N> |
| Dependency flow | <N> |
| Abstraction / design debt | <N> |
| Rule / design tension | <N> |

## Repeated Findings (seen in >1 cycle)

| Finding | Occurrences | Cycles Seen | Confidence |
|---|---:|---|---|
| <short title> | <N> | <cycle-1, cycle-3> | <HIGH/MEDIUM/LOW> |

## Finding Quality Gate Results

| Disposition | Count |
|---|---:|
| ACTIONABLE_NOW | <N> |
| INTENTIONAL_DECISION | <N> |
| DEFERRED_BY_PLAN | <N> |
| INSUFFICIENT_EVIDENCE | <N> |

---

## Detailed Findings and Recommended Fixes

### <Finding title> — <CF-NNN>

- **Severity**: <CRITICAL/HIGH/MEDIUM/LOW>
- **Occurrences**: <N> (out of <total cycles> cycles)
- **Cycles Seen**: <cycle list>
- **Reviewers**: <list>
- **Category**: <category>
- **Confidence**: <HIGH|MEDIUM|LOW> — <confidence rationale>
- **Disposition**: <ACTIONABLE_NOW>
- **Location**: `<file>:<line>` (additional: `<file>:<line>`, ...)
- **Rule Refs**: <R-NNN list>
- **Design Refs**: <D-NNN list>
- **Problem**: <what is wrong and why it matters>
- **Evidence**: <strongest observed evidence>
- **Plan Context**: <relevant plan phase and rationale with citation>
- **Recommended Fix**: <clear recommendation>
- **Proposed Solution**: <concrete approach, constraints, trade-offs>
- **Blast Radius**: <ISOLATED|MODULE|CROSS-MODULE|SYSTEM>
- **Estimated Complexity**: <LOW|MEDIUM|HIGH>
- **Risk if Unchanged**: <impact>

---

## Design Challenge Ledger

### <Challenge title>

- **Challenged constraint**: <rule/design anchor with ID>
- **Rationale**: <why current constraint is suboptimal>
- **Proposed update**: <draft direction>
- **Related findings**: <CF-NNN list>
- **Status**: Requires decision | Deferred | Accepted for update

---

## Recommended Remediation Order

1. <highest-priority item — CF-NNN>
2. <next item>
...

## Blocked Solutions

- <blocker description, or "None">

---

## Appendix

### A. Files Reviewed
<full list by shard>

### B. Cycle Summary
| Cycle | Shards | Raw Findings | Critical/High | Medium | Low | Security Invocations | Cross-Shard Findings |
|---|---:|---:|---:|---:|---:|---:|---:|
| cycle-1 | <N> | <N> | <N> | <N> | <N> | <N> | <N> |
...

### C. Shard Summary
| Shard | Files | Raw Findings | Security Review Triggered |
|---|---:|---:|---|
| shard-auth | <N> | <N> | Yes / No |
...

### D. Deduplication Criteria
<rolling deduplication criteria used — root cause + location matching, contradiction handling, Wave 3 CSR finding merger>

### E. Findings Excluded as Insufficient Evidence
| CF-NNN | Reason | Single-cycle? | Missing citation? |
|---|---|---|---|
...

### F. Intentional Decisions Documented
| CF-NNN | Plan Citation | Rationale |
|---|---|---|
...

### G. Deferred Findings
| CF-NNN | Deferred to Phase | Citation |
|---|---|---|
...

### H. Rule Index Consulted
| ID | Source | Scope | Severity |
|---|---|---|---|
...

### I. Design Invariants Consulted
| ID | Source | Scope |
|---|---|---|
...

### J. Plan Files and Handoffs Cited
<list with status labels>

### K. Machine-Readable Actionable Findings Export (bridge to `/implement-review`)

Schema mirrors `/implement-review` Phase 1 normalized finding fields. The `disposition` field is included so `/implement-review` can skip re-classification when the plan has not changed since this report was generated.

```json
{
  "source_report": ".claude/reviews/review-<scope-slug>-<YYYYMMDD-HHMMSS>.md",
  "report_timestamp": "<YYYY-MM-DD HH:MM:SS UTC>",
  "scope": "<resolved scope>",
  "actionable_findings": [
    {
      "id": "<CF-NNN>",
      "severity": "<CRITICAL|HIGH|MEDIUM|LOW>",
      "category": "<category>",
      "confidence": "<HIGH|MEDIUM|LOW>",
      "disposition": "ACTIONABLE_NOW",
      "location": "<file>:<line>",
      "rule_refs": ["<R-NNN>"],
      "design_refs": ["<D-NNN>"],
      "problem": "<summary>",
      "evidence": "<summary>",
      "plan_context": "<citation>",
      "recommended_fix": "<summary>",
      "proposed_solution": "<summary>",
      "blast_radius": "<ISOLATED|MODULE|CROSS-MODULE|SYSTEM>",
      "estimated_complexity": "<LOW|MEDIUM|HIGH>",
      "design_challenge": null
    }
  ],
  "design_challenge_ledger": [
    {
      "challenged_constraint": "<rule/design anchor>",
      "rationale": "<why suboptimal>",
      "proposed_update": "<direction>",
      "related_finding_ids": ["<CF-NNN>"],
      "status": "Requires decision"
    }
  ]
}
```

> **Severity note for `/implement-review` consumers:** Severities in this export reflect review-phase normalization (`WARNING → MEDIUM`, `NOTE → LOW`). `/implement-review` applies stricter normalization at ingestion (`WARNING → HIGH`, `NOTE → MEDIUM`). If re-classification occurs, the updated severity takes precedence for implementation prioritization.
````

---

## Guardrails

- **Review-only mode is absolute.** Do not modify any application source file under any circumstance.
- Allowed write output: **the report file and run-state artifacts under `.claude/reviews/` only.**
- No commits, pushes, branch operations, or destructive git commands.
- No scope broadening without explicit documentation in the report of why.
- **Orchestrator must not perform deep reasoning on file content.** If the orchestrator finds itself reading and interpreting source files or plan files directly, it must delegate to the appropriate gatherer or reviewer agent instead.
- **NEVER** invoke a reviewer agent by reading source files yourself and summarizing findings — use `task` tool.
- **NEVER** classify findings directly — invoke `finding-classifier` via `task` tool.
- **NEVER** assemble the report yourself — invoke `report-writer` via `task` tool.
- **NEVER** skip a delegated agent invocation because the orchestrator believes it can reason over the inputs directly — this is a protocol violation regardless of reasoning quality.
- Every `ACTIONABLE_NOW` finding must cite at least one `rule_refs` or `design_refs` entry. Findings without citations must be reclassified as `INSUFFICIENT_EVIDENCE`.
- Agents must never receive another agent's full raw output as context — only the extracted, structured fields they need.
- Gatherer agents (`plan-context-builder`, `rules-extractor`, `design-extractor`) must use verbatim extraction for high-authority content. Paraphrasing of rules or design invariants is not permitted.
- `cross-shard-reviewer` must reason over structured cycle outputs, `SHARD_DIGEST_SUMMARY` entries, and `INTERFACE_SLICE` only; do not feed full source files or full `DIGEST_SLICE` content into this pass.

---

## Failure Modes and Halt Conditions

| Condition | Action |
|---|---|
| Scope resolves to zero files | Halt; report unresolved scope |
| Invalid cycle count | Halt; report invalid configuration |
| Baseline gate result violates Baseline Configuration policy | Write baseline-failure report or continue with degraded warning per policy |
| Gatherer agent returns malformed output after retry | Halt Phase 0; report which gatherer failed and why |
| `cross-shard-reviewer` output malformed after retry | Halt cycle; report malformed cross-shard output; do not start next cycle |
| Any agent parse failure after retry | Halt with `PARSE_ERROR`; surface raw output to user |
| Reviewer agent returns finding missing required fields | Discard finding; log discard in report appendix |
| All `problem-solver` agents return `BLOCKED_SOLUTIONS` | Include blockers in report; do not suppress |
| Report writer fails to write output file | Orchestrator writes minimal plain-text fallback to stdout |
