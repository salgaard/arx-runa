# `/implement-review` — Review-Driven Rust Fix Command

Run a review-driven Rust fix flow for: $ARGUMENTS

---

## Design Principles

- **Orchestrator stays thin.** Route, normalize, and merge structured outputs only. Never perform deep reasoning on file contents or finding semantics directly.
- **Agents own their domain.** Classification belongs to `finding-classifier`; remediation synthesis belongs to `problem-solver`; implementation belongs to `rust-implementer`.
- **Structured contracts, not prose.** Prefer machine-readable Appendix K payloads from `/review-only`; markdown parsing is a compatibility fallback only.
- **Parallelism is the default.** Serialize only when strict data dependency requires it.
- **Summarization must be lossless for high-authority items.** Gatherer agents emit verbatim excerpts and source citations for rules, design invariants, and plan rationale — never paraphrased prose that loses nuance.
- **Write-capable by design.** Unlike `/review-only`, this command is explicitly implementation-focused and may modify code within scope.
- **Context-bounded cycles.** Cycle state is persisted to disk; the orchestrator never accumulates full finding records across remediation cycles in working memory.

---

## Agent Roster

| Agent | Role | Input | Output |
|---|---|---|---|
| `plan-context-builder` | Plan and handoff context extraction | Plan files | `PLAN_DIGEST` |
| `rules-extractor` | Rule anchor extraction | Rules files | `RULES_INDEX` |
| `design-extractor` | Design invariant extraction | Design docs | `DESIGN_INDEX` |
| `shard-planner` | Scope-to-shard mapping and digest summaries | File paths | `SHARD_MAP` + `SHARD_DIGEST_SUMMARY[]` |
| `finding-classifier` | Disposition/confidence classification | Findings + digests | `CLASSIFIED_FINDINGS` |
| `problem-solver` | Fix synthesis per grouped findings | Finding group + shard slice | `SOLUTION_PACK` / `NO_ACTIONABLE_FIXES` / `BLOCKED_SOLUTIONS` |
| `rust-implementer` | Code implementation pass | Solution pack + shard slice | `IMPLEMENTATION_RESULT` |
| `rust-reviewer` | Rust verification loop | Changed shard + digest slice | Structured findings |
| `architecture-reviewer` | Architecture verification loop (required) | Changed shard + digest slice | Structured findings |
| `security-reviewer` | Conditional security verification | Changed shard + digest slice | Structured findings |
| `cross-shard-reviewer` | Cross-shard contradiction detection in re-review loops | Cycle findings + shard summaries | Structured findings |
| `test-writer` | Conditional test expansion | Changed files + coverage gaps | Test additions/updates |

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
| `IMPLEMENTATION_RESULT` | `.claude/agents/rust-implementer.md` |

This command owns orchestration and gates. Producer schema details live in agent contracts.

---

## Tool Invocation Contract (Hard)

**Every named agent in the Agent Roster MUST be invoked via the `task` tool.** The orchestrator MUST NOT classify findings, synthesize solutions, implement code, or write reports directly. The `task` tool runs each agent in an isolated context window — this is the core context-preservation mechanism of this command.

```
task(agent_type="plan-context-builder", model="claude-sonnet-4.6", ...)    → PLAN_DIGEST
task(agent_type="rules-extractor",      model="claude-sonnet-4.6", ...)    → RULES_INDEX
task(agent_type="design-extractor",     model="claude-sonnet-4.6", ...)    → DESIGN_INDEX
task(agent_type="shard-planner",        model="claude-sonnet-4.6", ...)    → SHARD_MAP + SHARD_DIGEST_SUMMARY[]
task(agent_type="finding-classifier",   model="claude-sonnet-4.6", ...)    → CLASSIFIED_FINDINGS
task(agent_type="problem-solver",       model="claude-sonnet-4.6",   ...)    → SOLUTION_PACK / NO_ACTIONABLE_FIXES / BLOCKED_SOLUTIONS
task(agent_type="rust-implementer",     model="claude-sonnet-4.6",   ...)    → IMPLEMENTATION_RESULT
task(agent_type="rust-reviewer",        model="claude-sonnet-4.6",   ...)    → Raw findings
task(agent_type="architecture-reviewer",model="claude-sonnet-4.6",   ...)    → Raw findings
task(agent_type="security-reviewer",    model="claude-sonnet-4.6",   ...)    → Raw findings
task(agent_type="cross-shard-reviewer", model="claude-sonnet-4.6", ...)    → Raw findings
task(agent_type="test-writer",          model="claude-sonnet-4.6",   ...)    → Test additions/updates
```

All custom agent names in the Agent Roster map directly to `agent_type` values. Skipping an agent invocation is a protocol violation, not a valid optimization.

## Model Assignments

Apply these model overrides on every `task` invocation. Never omit the `model` parameter.

| Agent | Model | Rationale |
|---|---|---|
| `rust-implementer` | `claude-sonnet-4.6` | Writes all production Rust code — maximum code quality and rule compliance |
| `rust-reviewer` | `claude-sonnet-4.6` | Deep code review with security/rule awareness across large shards |
| `security-reviewer` | `claude-sonnet-4.6` | Crypto correctness and zero-knowledge threat model — no false negatives tolerable |
| `architecture-reviewer` | `claude-sonnet-4.6` | Broad cross-cutting structural analysis requiring deep reasoning |
| `problem-solver` | `claude-sonnet-4.6` | Complex solution synthesis across multiple classified findings and design challenges |
| `test-writer` | `claude-sonnet-4.6` | Adversarial crypto tests and full coverage planning require domain depth |
| `finding-classifier` | `claude-sonnet-4.6` | Structured disposition classification — accurate table output, no deep reasoning needed |
| `cross-shard-reviewer` | `claude-sonnet-4.6` | Pattern-based contradiction detection using structured shard digests |
| `shard-planner` | `claude-sonnet-4.6` | File-to-shard mapping and keyword classification — structured analysis |
| `plan-context-builder` | `claude-sonnet-4.6` | Document parsing and structured extraction |
| `rules-extractor` | `claude-sonnet-4.6` | Text extraction from rule files — mechanical |
| `design-extractor` | `claude-sonnet-4.6` | Design invariant extraction — mechanical |

---

## Input Resolution

`$ARGUMENTS` can be:

1. **Empty** — use the newest review file in `.claude/reviews/review-*.md`.
2. **Path to a review file** — e.g. `.claude/reviews/review-auth-20260415-001212.md`
3. **Path to an Appendix K payload** — JSON file with `actionable_findings` and optional `design_challenge_ledger`. Preferred: no markdown parsing required.
4. **`<review-path> <scope-override>`** — use the review file but constrain fixes to the explicit scope override.

If no review file is found or readable, halt and report the missing input.

---

## Scope Resolution

1. Default scope comes from the review report's Scope section.
2. If scope override is provided, use it as the implementation scope.
3. Resolve scope to concrete Rust files before synthesis starts.
4. If no files are resolved, halt and report unresolved scope.

---

## Authority Order (Hard)

1. `.claude/rules/*.md` — primary, normative
2. `docs/architecture/designs/**/design.md` and `docs/architecture/design-invariants.md` — canonical design
3. `.claude/reference/*.md` — secondary pattern guidance only; never overrides rules or design
4. `architecture-reviewer` may challenge rules/designs **only** via explicit `design_challenge` entries. Architecture-originated deviations must flow through `DESIGN_CHALLENGE_LEDGER` and explicit approval decisions; do not implement silent rule/design overrides.

---

## Track Selection

Evaluate scope and finding severity before context build. Track is locked after selection and recorded in the fix report header.

| Condition | Track |
|---|---|
| Security-sensitive findings present, OR security shards in scope (`auth/`, `crypto/`, `storage/`), OR > 10 files in scope | `full` |
| 4–10 non-security files; CRITICAL/HIGH findings present | `standard` |
| ≤ 3 non-security files; no CRITICAL/HIGH findings | `minimal` |

Track capabilities:

- **`full`** — all agents, security-reviewer, cross-shard review when 2+ shards touched, max 8 remediation cycles.
- **`standard`** — rust-implementer + rust-reviewer + architecture-reviewer + finding-classifier + problem-solver + test-writer; cross-shard only if 2+ shards touched; max 3 remediation cycles; security-reviewer only if risk indicators appear.
- **`minimal`** — rust-implementer + rust-reviewer + finding-classifier + test-writer; 1 remediation cycle; no architecture-reviewer; no cross-shard review. If any HIGH finding surfaces after the cycle → automatically escalate to `standard` and continue.

---

## Output Parsing Protocol

Apply after every agent invocation throughout all phases.

1. Locate the named output block by scanning for its keyword header (e.g., `PLAN_DIGEST`, `CLASSIFIED_FINDINGS`, `SOLUTION_PACK`, `IMPLEMENTATION_RESULT`). Strip any prose wrapper or markdown fences.
2. Validate that all required top-level fields are present per the agent's output contract.
3. **If the block is not found or required fields are missing:**
   a. Re-invoke the agent once. Prepend the raw output to the new invocation with: `"Your previous output did not match the required schema. Return only the structured block specified in your agent contract — no prose preamble, no markdown fences unless part of the schema."`
   b. If the second attempt also fails: halt with `PARSE_ERROR`. Record the agent name, expected schema, and raw output. Surface to the user. Do not infer missing field values.
4. Do not proceed with a partially parsed output.

---

## Phase Contracts

| Phase | Contract |
|---|---|
| Phase 0 | Gather context in parallel via structured outputs only; kick off baseline concurrently. |
| Phase 1 | Ingest/normalize findings and classify actionable scope. |
| Phase 2 | Enforce baseline compile gate before fixes. |
| Phase 3 | Synthesize solutions in grouped, scoped solver calls. |
| Phase 4 | Execute write-capable implementation within strict boundaries. |
| Phase 5 | Run budgeted remediation re-review loops with escalation limit. |
| Phase 6 | Validate with tests and targeted test-authoring when needed. |
| Phase 7 | Emit final fix report, including no-op outcomes and blockers. |

---

## Phase 0 — Parallel Preflight

Spawn all agents and the baseline check **in parallel via `task` tool** (mandatory — HALT if any are skipped). The orchestrator does not read plan, rules, or design files directly — it consumes only structured outputs. Kick off `cargo check --workspace` concurrently to overlap baseline latency with gather time; the result is consumed in Phase 2.

Parallel launch set:
- `plan-context-builder` (Step 0-A)
- `rules-extractor` (Step 0-B)
- `design-extractor` (Step 0-C)
- `shard-planner` (Step 0-E, parallel with 0-A/B/C)
- baseline command: `cargo check --workspace`

Generate a run ID: `fix-<scope-slug>-<YYYYMMDD-HHMMSS>`. Write initial run state to `.claude/reviews/<run-id>/run-state.json`:

```json
{
  "run_id": "<run-id>",
  "source_review": "<review-file-path>",
  "scope": "<resolved scope>",
  "track": "<minimal|standard|full>",
  "cycle_count": 0,
  "finding_summary": { "CRITICAL": 0, "HIGH": 0, "MEDIUM": 0, "LOW": 0 },
  "disposition_summary": {
    "ACTIONABLE_NOW": 0, "INTENTIONAL_DECISION": 0,
    "DEFERRED_BY_PLAN": 0, "INSUFFICIENT_EVIDENCE": 0
  },
  "override_records": [],
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

Once `PLAN_DIGEST`, `RULES_INDEX`, and `DESIGN_INDEX` are returned, the orchestrator builds a **per-shard digest slice** for each shard. Each agent receives only its shard's slice. Reuse slices across all subsequent phases and remediation loops unless scope changes.

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

**Working memory discipline:** Keep working memory to structured evidence only — finding metadata, citation anchors, remediation status. Drop verbose prose after converting findings into structured entries.

**Guardrail:** Do not pass the full `PLAN_DIGEST`, `RULES_INDEX`, or `DESIGN_INDEX` to agents. Each agent receives only its shard's slice.

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
  rule_ids: ["<R-NNN>", ...]
  design_ids: ["<D-NNN>", ...]
  implemented_phases: ["<phase>"]
  deferred_phases: ["<phase>"]
}
```

Apply output parsing protocol. Halt Phase 0 if required fields are missing or malformed after retry.

The orchestrator waits for all four structured outputs (0-A, 0-B, 0-C, 0-E) before proceeding. Baseline result is resolved in Phase 2.

---

## Phase 1 — Review Ingestion and Classification Gate

1. Parse findings from one of:
   - **Appendix K machine-readable payload** (`actionable_findings` / `design_challenge_ledger`) — preferred; no markdown parsing required.
   - **Review report `Detailed Findings and Recommended Fixes` section** — markdown fallback when Appendix K is absent.

2. Normalize findings to this required shape:
   - `id`, `severity`, `category`, `location`, `problem`, `evidence`
   - `rule_refs`, `design_refs`
   - `plan_context`, `recommended_fix`, `proposed_solution`
   - `blast_radius`, `estimated_complexity`
   - `confidence` (present when sourced from Appendix K)
   - `disposition` (present when sourced from Appendix K — use as-is unless plan has changed; see step 4)
   - `design_challenge` (optional)

3. **Severity normalization (applied after ingestion):**

   | Raw severity | Normalized |
   |---|---|
   | CRITICAL | CRITICAL |
   | HIGH | HIGH |
   | WARNING (security-reviewer) | HIGH |
   | MEDIUM | MEDIUM |
   | NOTE (security-reviewer) | MEDIUM |
   | LOW | LOW |

   > **Intentional difference from `/review-only`:** This command promotes `WARNING → HIGH` and `NOTE → MEDIUM` (versus `MEDIUM` and `LOW` in review-phase normalization). The rationale: when implementing changes, security signals must not be downgraded — a finding that warranted a `WARNING` from `security-reviewer` represents a risk that should block or gate implementation. The `disposition` field in Appendix K reflects review-phase normalization; if severity has escalated after ingestion, re-classification via `finding-classifier` applies the updated severity.

4. **Classify dispositions using `finding-classifier` when:**
   - Dispositions are absent from the parsed payload, **or**
   - The source review report predates the current plan state (any plan file has a newer `created` or `status` timestamp than the review report's `report_timestamp`).

   Otherwise, use dispositions as-is from the Appendix K payload.

   `finding-classifier` input: normalized findings + `PLAN_DIGEST` + `RULES_INDEX` + `DESIGN_INDEX`. Apply output parsing protocol. Classifier output must conform to `.claude/agents/finding-classifier.md` and include per-record `canonical_id`, `disposition`, `confidence`, `confidence_rationale`, `disposition_citation`.

5. Build:
   - `ACTIONABLE_FINDINGS` from `ACTIONABLE_NOW` only
   - `DEFERRED_OR_INTENTIONAL` for report-only
   - `DESIGN_CHALLENGE_LEDGER` from classified challenge entries

6. Build `APPROVED_DESIGN_CHALLENGES` from ledger entries explicitly marked accepted (`status: Accepted for update`). In most cases this list is empty for pure review-driven fixes — approvals require a plan.

7. If `ACTIONABLE_FINDINGS` is empty, skip to Phase 7 and emit a no-op fix report.

---

## Phase 2 — Baseline Gate

1. Consume the `cargo check --workspace` result from Phase 0.
2. If baseline fails, halt and report baseline blockers. Do not apply any fixes until baseline passes.

---

## Phase 3 — Solution Synthesis (Sharded, Grouped)

### Finding Grouping Strategy

Group `ACTIONABLE_FINDINGS` before spawning `problem-solver` agents via `task` tool (mandatory — HALT if skipped):

| Group | Contents | Agent scope |
|---|---|---|
| One agent per CRITICAL finding | Each CRITICAL gets its own isolated agent | Single finding |
| One agent per HIGH finding | Each HIGH gets its own isolated agent (or grouped by identical root cause) | Single finding or root-cause group |
| One agent per shard for MEDIUM findings | Group MEDIUMs by shard_id | Up to 10 per agent |
| One agent for all LOW findings | Batch across all shards | All LOWs |

For N CRITICAL/HIGH findings + M shards with MEDIUMs + any LOWs, spawn `N + M + 1` agents in parallel.

### Security-Scoped Challenge Checkpoint (Hard)

Before invoking any `problem-solver`, check `DESIGN_CHALLENGE_LEDGER` for entries where `requires_human_review: true`. If any exist, halt and display each to the user — the challenged constraint, the reviewer's rationale, and the proposed update. Require explicit `accept` or `reject` per challenge before proceeding. Record decisions in `run-state.json`. Resume only with user decisions injected into the `problem-solver` invocation.

### Per-Agent Input (scoped — do not pass global state)

Each `problem-solver` agent receives **only**:

```
PROBLEM_SOLVER_INPUT {
  findings: [<assigned ACTIONABLE_FINDING(s) with classification>]
  relevant_files: [<file paths from finding locations only>]
  digest_slice: <DIGEST_SLICE for the finding's shard(s)>
  design_challenge_entries: [<only DESIGN_CHALLENGE_LEDGER entries related to these findings>]
  approved_design_challenges: [<from APPROVED_DESIGN_CHALLENGES>]
}
```

### `problem-solver` Required Output

Each agent returns one structured payload per `.claude/agents/problem-solver.md`:
- `SOLUTION_PACK`
- `NO_ACTIONABLE_FIXES`
- `BLOCKED_SOLUTIONS`

Orchestrator-consumed fields (required):
- `SOLUTION_PACK.finding_ids`
- `SOLUTION_PACK.solutions[].{canonical_id,recommendation,implementation_approach,blast_radius,dependencies,estimated_complexity}`
- `NO_ACTIONABLE_FIXES.reason`
- `BLOCKED_SOLUTIONS.blockers`

Apply output parsing protocol to each solver result.

Deduplicate cross-group overlaps before implementation: same root cause across groups → one canonical implementation step covering all locations.

If any group returns `BLOCKED_SOLUTIONS`, keep blockers in the final report and continue with unblocked groups.

### Deep-Dive Rules

- Deep-dive all CRITICAL/HIGH by default (each has its own agent).
- Deep-dive MEDIUM only when `blast_radius` is `CROSS-MODULE` or `SYSTEM`, or ambiguity is high.
- Keep LOW recommendations concise unless directly security-sensitive.

---

## Phase 4 — Implementation Pass

1. For each shard with `SOLUTION_PACK`, invoke `rust-implementer` via `task` tool (mandatory — HALT if skipped). **Orchestrator MUST NOT write code directly.**
2. Run shard implementations in **parallel only when file sets are disjoint**; otherwise run sequentially.
3. Apply output parsing protocol. Require `IMPLEMENTATION_RESULT`; parse items for `DONE|BLOCKED` status and per-item file/summary or reason/needed fields.
4. After each shard implementation, run `cargo check --workspace`. Fix compile errors before proceeding.

**Implementation boundaries:**
- Do not modify files outside resolved scope unless direct dependency requires it; if broadened, report why.
- Do not implement deferred-phase features.
- Do not override intentional decisions documented in plans/handoffs.
- If a design/rule challenge remains unresolved, mark the item as blocked rather than silently overriding constraints.
- Pass `APPROVED_DESIGN_CHALLENGES` to `rust-implementer`; if an item requires deviation outside that allowlist, mark blocked.

---

## Phase 5 — Re-Review Remediation Loop (Budgeted)

Use stable remediation cycle identifiers: `remediation-cycle-1`, `remediation-cycle-2`, ..., `remediation-cycle-N` (distinct from `/review-only` review cycle labels).

### Finding Canonicalization

Assign stable `CF-NNN` IDs in arrival order (rust-reviewer → architecture-reviewer → security-reviewer). Preserve original IDs in `source_id`. Mapping is fixed for the entire loop.

**`CANONICAL_FINDING` structure:**

```
CANONICAL_FINDING {
  canonical_id: "<CF-NNN>"
  source_id: "<original finding ID from review report>"
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
  cycle_hits: ["<remediation-cycle-1>", ...]
  reviewer_hits: ["<rust-reviewer>", ...]
  has_contradiction: true|false
  disposition: "<ACTIONABLE_NOW|INTENTIONAL_DECISION|DEFERRED_BY_PLAN|INSUFFICIENT_EVIDENCE>"
  design_challenge: { ... } | null
}
```

### Per-Cycle Execution

1. Re-run `rust-reviewer` via `task` tool on changed files only, sharded by path. Pass existing `DIGEST_SLICE_<shard_id>` artifacts from Phase 0 — do not re-read full indices. Apply output parsing protocol.
2. Re-run `architecture-reviewer` via `task` tool on changed files only, sharded by path (`full` and `standard` tracks only). Pass same digest slices. Apply output parsing protocol.
3. Re-run `security-reviewer` via `task` tool on changed shards under `auth/`, `crypto/`, `storage/`, or when risk indicators appear in reviewer findings (`full` track; `standard` only if drift check fires). Apply output parsing protocol.
4. **Finding classification:** invoke `finding-classifier` via `task` tool with canonicalized normalized findings + `PLAN_DIGEST` + `RULES_INDEX` + `DESIGN_INDEX`. Apply output parsing protocol. Require `CLASSIFIED_FINDINGS`.
5. After all shard reviewers complete, invoke `cross-shard-reviewer` via `task` tool **when two or more shards had changed files in this cycle** (`full` and `standard` tracks only):

   **Before invoking `cross-shard-reviewer`, extract boundary pub signatures:**

   ```bash
   grep -rn "^pub fn\|^pub trait\|^pub struct\|^pub enum\|^pub type" \
     <files at the boundary between each pair of changed shards>
   ```

   `cross-shard-reviewer` receives:
   - per-shard reviewer findings for this remediation cycle (structured fields only)
   - `SHARD_DIGEST_SUMMARY[]` from Phase 0 (IDs only — not full slice content)
   - suppression list of already-resolved findings
   - `INTERFACE_SLICE` (pub boundary signatures only)

   Apply output parsing protocol.

6. If actionable CRITICAL/HIGH remain in a shard:
   - invoke `problem-solver` via `task` tool for that shard with the relevant `DIGEST_SLICE` and findings
   - apply output parsing protocol
   - invoke `rust-implementer` via `task` tool with the new `SOLUTION_PACK`
   - apply output parsing protocol

### Orchestrator Override for Persistent HIGH Findings

Available from remediation-cycle-3 (`full`) or remediation-cycle-2 (`standard`). **Never available for CRITICAL findings.**

If a HIGH finding has been `ACTIONABLE_NOW` in two consecutive cycles without resolution, the orchestrator may file an Override Record:

```
OVERRIDE_RECORD {
  finding_id: "<CF-NNN>"
  cycles_unresolved: <N>
  override_rationale: "<why this is a false positive or intentional exception>"
  confidence: "CERTAIN" | "LIKELY" | "UNCERTAIN"
  supporting_evidence: "<plan section, design doc anchor, or rule reference>"
}
```

- `CERTAIN` or `LIKELY` → reclassify as `INTENTIONAL_DECISION`. Does not block completion.
- `UNCERTAIN` → halt and surface to the user for manual decision. Resume on input.

All Override Records appear in the final fix report under "Finding overrides."

### Progressive Deepening

- Deep-dive all unresolved CRITICAL/HIGH.
- Deep-dive MEDIUM only when ambiguity or high blast radius exists.
- Keep LOW concise unless security-sensitive.

### Acceptance Thresholds

- CRITICAL → must remediate before completion.
- HIGH → must remediate or carry an approved Override Record before completion.
- MEDIUM and LOW → record in fix report with rationale when deferred.

### Run-State Persistence

After each remediation cycle, write `.claude/reviews/<run-id>/cycle-<N>.json`:

```json
{
  "cycle": <N>,
  "findings": [{ "id": "CF-NNN", "severity": "...", "disposition": "...", "source_id": "..." }],
  "override_records": [],
  "cross_shard_finding_count": <N>,
  "actionable_remaining": <N>
}
```

Update `run-state.json` with incremented `cycle_count` and cumulative summary counts.

**The orchestrator carries forward between cycles only:**
- CF-NNN → severity mapping for ACTIONABLE_NOW items (IDs and severities only)
- Running disposition and severity summary counts
- Override Records filed so far

Full finding prose, `SOLUTION_PACK` content, and `IMPLEMENTATION_RESULT` records must not accumulate across cycles. Reload from disk when a specific record is needed.

**Context compaction:** if the orchestrator estimates it cannot complete another full cycle within a safe context budget:
1. Persist current cycle state to disk.
2. Emit `CONTEXT_CHECKPOINT`: "Context compacted after remediation-cycle-N — state at `.claude/reviews/<run-id>/`. Resuming from disk."
3. Continue with a fresh working context loaded from run-state only.

### Cycle Limits

- Max cycles: `full`: 8 | `standard`: 3 | `minimal`: 1 (then escalate to `standard` or accept with rationale).
- Reviewer-only loops are not allowed when actionable CRITICAL/HIGH remain.
- If required thresholds remain unmet after the max cycle, **halt and report unresolved findings**. Recommend the user create a formal plan via `/plan` to address the remaining findings with explicit approved scope and design-challenge handling before re-attempting implementation.

---

## Agent I/O Boundaries (Hard)

1. Agents must receive structured fields only; do not pass full raw outputs from other agents.
2. Gatherers and extractors provide verbatim anchors; reviewers/solvers consume structured anchors, not full rule/design prose.
3. `rust-implementer` receives only: scoped files and grouped findings, approved challenge allowlist, relevant digest slice.
4. `cross-shard-reviewer` receives only: structured per-shard finding records, `SHARD_DIGEST_SUMMARY[]` (IDs only), `INTERFACE_SLICE` (pub boundary signatures only). Never full `DIGEST_SLICE` content.
5. Any requested deviation outside `APPROVED_DESIGN_CHALLENGES` is blocked and reported.

---

## Phase 6 — Test + Verify

1. Run `cargo fmt --all` (**Fix formatting**).
2. Run `cargo clippy --workspace --all-targets --all-features -- -D warnings` (**Run Clippy (warnings as errors)**). Fix related failures; note pre-existing unrelated issues.
3. Run `cargo test --workspace --all-targets --all-features` (**Run tests**). Fix related failures; note pre-existing unrelated issues.
4. Run `cargo build --workspace --release` (**Release build**). Fix related failures; note pre-existing unrelated issues.
5. Invoke `test-writer` when:
   - reviewers identify missing tests, or
   - behavior changed without adequate coverage, or
   - sensitive modules were modified and adversarial coverage is missing.
6. Apply output parsing protocol to `test-writer` result.
7. If tests fail, fix failures within scope and re-run. If a second run fails, record the failure as a blocker in the final report rather than looping indefinitely.

---

## Phase 7 — Final Implementation Report

1. Ensure directory exists: `.claude/reviews/`.
2. Derive output filename: `.claude/reviews/fix-<scope-slug>-<YYYYMMDD-HHMMSS>.md`
3. Write a complete report:

```markdown
# Review Fix Report — <scope>

> Generated by `/implement-review`
> Timestamp (UTC): <YYYY-MM-DD HH:MM:SS>
> Source review: `.claude/reviews/<review-file>.md`
> Scope: <resolved scope>
> Track: <minimal|standard|full>

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

## Finding Overrides

| CF-NNN | Cycles Unresolved | Confidence | Rationale | Decision |
|---|---:|---|---|---|
| <CF-NNN> | <N> | <CERTAIN|LIKELY|UNCERTAIN> | <rationale> | <INTENTIONAL_DECISION|user-approved> |

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
- Run state path: `.claude/reviews/<run-id>/`
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
- `cross-shard-reviewer` receives `SHARD_DIGEST_SUMMARY[]` + `INTERFACE_SLICE` only — never full `DIGEST_SLICE` content.
- Override Records are prohibited for CRITICAL findings.
- Security-scoped design challenge decisions require explicit user input before `problem-solver` proceeds.
- Do not mark a fix as complete unless `cargo check --workspace` passes after implementation.
- **Orchestrator must not perform deep reasoning on file content.** Delegate to the appropriate gatherer, reviewer, or implementer agent instead.
- **NEVER** read source files and implement fixes yourself — invoke `rust-implementer` via `task` tool.
- **NEVER** classify findings directly — invoke `finding-classifier` via `task` tool.
- **NEVER** synthesize solutions without invoking `problem-solver` via `task` tool.
- **NEVER** skip a delegated agent invocation because the orchestrator believes it can reason over the inputs directly — this is a protocol violation regardless of reasoning quality.

---

## Failure Modes and Halt Conditions

| Condition | Action |
|---|---|
| No review file found/readable from input resolution | Halt and report missing input |
| Scope resolves to zero files | Halt and report unresolved scope |
| Baseline `cargo check --workspace` fails | Halt and report baseline blockers before fixes |
| Gatherer agent returns malformed output after retry | Halt Phase 0; report which gatherer failed and why |
| Any agent parse failure after retry | Halt with `PARSE_ERROR`; surface raw output to user |
| Security-scoped design challenge awaits user decision | Hard pause; resume on user input |
| HIGH Override Record confidence `UNCERTAIN` | Hard pause; resume on user input |
| `BLOCKED_SOLUTIONS` returned with no unblocked groups remaining | Halt; include blockers in report |
| `rust-implementer` BLOCKED and direct fallback also infeasible | Halt; report unresolved item |
| Remediation thresholds unmet after max cycles | Halt; report unresolved findings; recommend `/plan` for formal resolution |
