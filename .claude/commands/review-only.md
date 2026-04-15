# `/review-only` — Optimized Rust Review Command

Run a full Rust review-only flow for: $ARGUMENTS

---

## Design Principles

- **Orchestrator stays thin.** It routes, sequences, and merges structured outputs. It never performs deep reasoning on file contents or finding semantics itself.
- **Agents own their domain.** Each agent receives only the context relevant to its task — no global dumps.
- **Structured contracts, not prose.** All inter-agent I/O uses defined structured fields. Agents never return unstructured narrative that the orchestrator must parse and interpret.
- **Parallelism is the default.** Serialize only when strict data dependency requires it.
- **Summarization must be lossless for high-authority items.** Gatherer agents emit verbatim excerpts and source citations for rules, design invariants, and plan rationale — never paraphrased prose that loses nuance.

---

## Agent Roster

| Agent | Role | Input | Output |
|---|---|---|---|
| `plan-context-builder` | Parses plan + handoff files into structured digest | Plan files | `PLAN_DIGEST` |
| `rules-extractor` | Extracts authority rules as structured anchors | Rules files | `RULES_INDEX` |
| `design-extractor` | Extracts design invariants as structured anchors | Design docs | `DESIGN_INDEX` |
| `shard-planner` | Resolves scope to file shards | File paths | `SHARD_MAP` |
| `rust-reviewer` | Rust code review per shard | Shard + digest slice | Raw findings |
| `architecture-reviewer` | Architecture review per shard | Shard + digest slice | Raw findings |
| `security-reviewer` | Security review (wave 2, conditional) | Shard + digest slice | Raw findings |
| `finding-classifier` | Disposes and confidence-rates canonical findings | Canonical findings + digests | `CLASSIFIED_FINDINGS` |
| `problem-solver` | Recommendation-only per finding group | Finding group + shard slice | `SOLUTION_PACK` |
| `report-writer` | Renders final Markdown report | All structured outputs | Report file |

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

## Authority Order (Hard)

1. `.claude/rules/*.md` — primary, normative
2. `docs/architecture/designs/**/design.md` and `docs/architecture/design-invariants.md` — canonical design
3. `.claude/reference/*.md` — secondary pattern guidance only; never overrides rules or design
4. `architecture-reviewer` may challenge rules/designs **only** via explicit `design_challenge` entries. No silent override is permitted.

---

## Phase 0 — Parallel Context Gathering (required before all else)

Spawn all three gatherer agents **in parallel**. The orchestrator does not read plan, rules, or design files directly — it consumes only their structured outputs.

### 0-A: `plan-context-builder`

**Input:** All files matching `.claude/plans/phase-*.md` and `.claude/plans/HANDOFF-*.md`

**Task:** Parse and extract. Do not summarize into prose. Emit structured output only.

**Output — `PLAN_DIGEST`:**

```
PLAN_DIGEST {
  highest_implemented_phase: "<phase/sub-phase>"
  in_progress_phases: ["<phase>", ...]
  deferred_phases: ["<phase>", ...]
  plans: [
    {
      file: "<path>"
      status: "<implemented|in-progress|draft|planned>"
      roadmap_phase: "<value>"
      sub_phase: "<value>"
      title: "<value>"
      rationale_bullets: ["<verbatim excerpt>", ...]   // max 8 per plan file
      deferred_items: ["<verbatim excerpt>", ...]       // max 5 per plan file
      known_constraints: ["<verbatim excerpt>", ...]    // max 5 per plan file
    }
  ]
  handoffs: [
    {
      file: "<path>"
      trade_offs: ["<verbatim excerpt>", ...]
      deferrals: ["<verbatim excerpt>", ...]
    }
  ]
}
```

**Guardrail:** Excerpts must be verbatim (not paraphrased). Truncate long excerpts at 120 characters with `…`. Do not collapse multiple distinct rationale points into one bullet.

---

### 0-B: `rules-extractor`

**Input:** All files matching `.claude/rules/*.md`

**Task:** Extract each distinct rule as a structured anchor. Do not interpret or summarize — extract.

**Output — `RULES_INDEX`:**

```
RULES_INDEX {
  rules: [
    {
      id: "<rule-id or auto-assigned R-NNN>"
      source_file: "<path>"
      anchor: "<section heading or line range>"
      verbatim: "<exact rule statement, truncated at 200 chars>"
      scope: ["auth" | "crypto" | "storage" | "global" | ...]
      severity_if_violated: "<CRITICAL|HIGH|MEDIUM|LOW>"
    }
  ]
}
```

**Guardrail:** `verbatim` must be the literal rule text — not the extractor's interpretation of it.

---

### 0-C: `design-extractor`

**Input:** `docs/architecture/designs/**/design.md` and `docs/architecture/design-invariants.md`

**Task:** Extract each design invariant and constraint as a structured anchor.

**Output — `DESIGN_INDEX`:**

```
DESIGN_INDEX {
  invariants: [
    {
      id: "<auto-assigned D-NNN>"
      source_file: "<path>"
      anchor: "<section heading or line range>"
      verbatim: "<exact invariant statement, truncated at 200 chars>"
      scope: ["auth" | "crypto" | "storage" | "global" | ...]
      challenged: false
    }
  ]
}
```

---

### 0-D: Orchestrator — Build Shard-Scoped Digest Slices

Once `PLAN_DIGEST`, `RULES_INDEX`, and `DESIGN_INDEX` are returned, the orchestrator builds a **per-shard digest slice** for each shard (not one global digest). Each reviewer agent receives only its shard's slice.

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

## Phase 0-E: `shard-planner` (parallel with 0-A/B/C)

**Input:** Resolved file list from scope resolution

**Task:** Map each file to its shard based on path pattern. Flag any files that don't match a default shard pattern as `shard-default`.

**Output — `SHARD_MAP`:**

```
SHARD_MAP {
  shards: [
    {
      shard_id: "<shard-auth|shard-crypto|shard-storage|shard-default>"
      files: ["<path>", ...]
      is_security_sensitive: true|false   // true if shard-auth, shard-crypto, or shard-storage
    }
  ]
  total_files: <N>
}
```

This agent runs in parallel with Phase 0-A/B/C. The orchestrator waits for all four outputs before proceeding.

---

## Phase 1 — Baseline

1. Run `cargo check --workspace`.
2. If baseline **fails:** write a report file capturing baseline blockers (file, error, line) and **stop**. Do not invoke any reviewer agents.
3. If baseline **passes:** proceed to Phase 2.

---

## Phase 2 — Multi-Cycle Sharded Review

### Per-Cycle State

The orchestrator maintains a rolling `CANONICAL_FINDINGS` list that is updated after each cycle. Cycles 2–N receive it as a "known findings" suppression list.

### Cycle Execution (repeat for cycle-1 through cycle-N)

#### Step 2-A: Wave 1 — Parallel Reviewer Invocation

For each shard in `SHARD_MAP`, invoke in **parallel**:

- `rust-reviewer` with `DIGEST_SLICE_<shard_id>` + shard file list + current `CANONICAL_FINDINGS` (as suppression list from cycle 2 onward)
- `architecture-reviewer` with same inputs (required for every shard, every cycle)

**Suppression instruction for cycles 2–N:**

> "The following findings are already canonical from prior cycles. Do not re-report them unless you observe a direct contradiction or significant new evidence. Report only NEW findings or contradictions."
> `<CANONICAL_FINDINGS list — IDs and one-line descriptions only>`

This cuts redundant output in later cycles dramatically.

#### Step 2-B: Wave 2 — Conditional Security Review

After Wave 1 completes for a shard, invoke `security-reviewer` on that shard **only if**:

- `shard.is_security_sensitive == true`, **OR**
- any Wave 1 finding for this shard includes `security_flag: true`

`security-reviewer` receives the same `DIGEST_SLICE_<shard_id>` plus the Wave 1 findings for its shard as additional context.

**If neither condition is met, skip `security-reviewer` for this shard entirely.**

#### Step 2-C: Required Finding Structure

Every finding returned by any reviewer agent must conform to this schema. The orchestrator rejects and discards any finding that does not include all required fields.

```
FINDING {
  id: "<reviewer-shard-cycle-NNN>"           // e.g., rust-auth-cycle1-001
  cycle_id: "<cycle-1|cycle-2|...>"
  reviewer: "<rust-reviewer|architecture-reviewer|security-reviewer>"
  shard_id: "<shard-auth|...>"
  severity: "<CRITICAL|HIGH|MEDIUM|LOW|WARNING|NOTE>"
  category: "<category string>"
  location: "<file>:<line> or module path"
  problem: "<what is wrong and why it matters>"
  evidence: "<specific observation with rule/design citation>"
  rule_refs: ["<R-NNN>", ...]                // from RULES_INDEX
  design_refs: ["<D-NNN>", ...]              // from DESIGN_INDEX
  plan_context: "<relevant phase or rationale, one line>"
  recommended_fix: "<clear recommendation>"
  proposed_solution: "<concrete approach, constraints, trade-offs>"
  risk_if_unchanged: "<impact>"
  security_flag: true|false
  design_challenge: {                         // optional; omit if not applicable
    challenged_constraint: "<rule or design anchor>"
    rationale: "<why current constraint is suboptimal>"
    proposed_update: "<draft update direction>"
  } | null
}
```

**Severity normalization (applied by orchestrator after collection):**

| Raw severity | Normalized |
|---|---|
| CRITICAL | CRITICAL/HIGH |
| HIGH | CRITICAL/HIGH |
| WARNING (security-reviewer) | MEDIUM |
| MEDIUM | MEDIUM |
| NOTE (security-reviewer) | LOW |
| LOW | LOW |

#### Step 2-D: Per-Cycle Deduplication and Canonical Update (Rolling)

After all shards complete for a cycle:

1. Deduplicate within the cycle by root cause + location.
2. Merge new findings into `CANONICAL_FINDINGS`:
   - If a finding matches an existing canonical entry (same root cause + location), increment `occurrence_count` and add cycle to `cycle_hits`. Do not create a new entry.
   - If a finding contradicts an existing canonical entry, flag the canonical entry with `has_contradiction: true` and attach the new evidence.
   - If a finding is genuinely new, add it as a new canonical entry with `occurrence_count: 1`.
3. Update `CANONICAL_FINDINGS` before the next cycle starts.

**Per-shard output limits (applied before deduplication, per cycle):**

- Keep all CRITICAL/HIGH findings
- Include up to 20 MEDIUM findings (highest impact first)
- Include up to 10 LOW findings (deduplicated summaries)

#### Step 2-E: `CANONICAL_FINDINGS` Structure

```
CANONICAL_FINDING {
  canonical_id: "<CF-NNN>"
  severity: "<normalized severity>"
  category: "<category>"
  location: "<primary location>"
  affected_locations: ["<location>", ...]    // merged across all occurrences
  problem: "<canonical problem statement>"
  evidence: "<strongest evidence observed>"
  rule_refs: ["<R-NNN>", ...]               // merged
  design_refs: ["<D-NNN>", ...]             // merged
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

## Phase 2.5 — `finding-classifier` Agent (Quality Gate)

**Do not perform this classification in the orchestrator.** Spawn a dedicated `finding-classifier` agent.

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

```
CLASSIFIED_FINDINGS {
  actionable_now: [<CANONICAL_FINDING + CLASSIFICATION>, ...]
  intentional_decisions: [<CANONICAL_FINDING + CLASSIFICATION>, ...]
  deferred_by_plan: [<CANONICAL_FINDING + CLASSIFICATION>, ...]
  insufficient_evidence: [<CANONICAL_FINDING + CLASSIFICATION>, ...]
  design_challenge_ledger: [
    {
      challenged_constraint: "<rule/design anchor>"
      rationale: "<why suboptimal>"
      proposed_update: "<direction>"
      related_finding_ids: ["<CF-NNN>", ...]
      status: "Requires decision"
    }
  ]
}
```

**Guardrail:** `INSUFFICIENT_EVIDENCE` findings are **never** passed to `problem-solver`. They are passed directly to the report writer for the appendix.

---

## Phase 3 — Parallel Solution Synthesis (no code changes)

If `CLASSIFIED_FINDINGS.actionable_now` is empty, skip to Phase 4.

### Finding Grouping Strategy

The orchestrator groups `actionable_now` findings **before** spawning `problem-solver` agents:

| Group | Contents | Agent scope |
|---|---|---|
| One agent per CRITICAL/HIGH finding | Each CRITICAL/HIGH gets its own isolated agent | Single finding |
| One agent per shard for MEDIUM findings | Group MEDIUMs by shard_id | Up to 10 per agent |
| One agent for all LOW findings | Batch across all shards | All LOWs |

This means for N CRITICAL/HIGH findings + M shards with MEDIUMs + any LOWs, you spawn `N + M + 1` agents in parallel.

### Per-Agent Input (scoped — do not pass global state)

Each `problem-solver` agent receives **only**:

```
PROBLEM_SOLVER_INPUT {
  findings: [<assigned CANONICAL_FINDING(s) with classification>]
  relevant_files: [<file paths from finding locations only>]
  digest_slice: <DIGEST_SLICE for the finding's shard(s)>
  design_challenge_entries: [<only DESIGN_CHALLENGE_LEDGER entries related to these findings>]
  instruction: "Produce recommendations only. No code edits. No file modifications."
}
```

### `problem-solver` Required Output

Each agent returns one of:

```
SOLUTION_PACK {
  finding_ids: ["<CF-NNN>", ...]
  solutions: [
    {
      canonical_id: "<CF-NNN>"
      recommendation: "<clear human-readable recommendation>"
      implementation_approach: "<concrete steps, constraints, trade-offs>"
      blast_radius: "<ISOLATED|MODULE|CROSS-MODULE|SYSTEM>"
      dependencies: ["<prerequisite fixes if any>"]
      estimated_complexity: "<LOW|MEDIUM|HIGH>"
    }
  ]
}
```

or `NO_ACTIONABLE_FIXES` or `BLOCKED_SOLUTIONS { blockers: ["..."] }`.

### Deep-Dive Rules

- Deep-dive all CRITICAL/HIGH by default (each has its own agent).
- Deep-dive MEDIUM only when `blast_radius` is `CROSS-MODULE` or `SYSTEM`, or when `has_contradiction: true`.
- Keep LOW recommendations concise unless directly security-sensitive.

---

## Phase 4 — `report-writer` Agent

**Do not assemble the report in the orchestrator.** Spawn a dedicated `report-writer` agent.

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

### Report Structure

```markdown
# Review Report — <scope>

> Generated by `/review-only`
> Timestamp (UTC): <YYYY-MM-DD HH:MM:SS>
> Scope: <resolved scope>
> Agents used: plan-context-builder, rules-extractor, design-extractor,
>              shard-planner, rust-reviewer, architecture-reviewer,
>              security-reviewer (conditional), finding-classifier,
>              problem-solver (×<N>), report-writer

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
- Raw finding events (all cycles, all shards): <N>
- Unique canonical findings after rolling deduplication: <N>
- Repeated findings (seen in >1 cycle): <N>
- Critical/High: <N> | Medium: <N> | Low: <N>
- Problem-solver agents spawned: <N>
- Security-reviewer shards skipped (clean wave 1): <N>
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
- **Status**: Requires decision | Deferred | Accepted for future update

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
| Cycle | Shards | Raw Findings | Critical/High | Medium | Low | Security Invocations |
|---|---:|---:|---:|---:|---:|---:|
| cycle-1 | <N> | <N> | <N> | <N> | <N> | <N> |
...

### C. Shard Summary
| Shard | Files | Raw Findings | Security Review Triggered |
|---|---:|---:|---|
| shard-auth | <N> | <N> | Yes / No |
...

### D. Deduplication Criteria
<explain rolling deduplication criteria used — root cause + location matching, contradiction handling>

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
```

---

## Guardrails

- **Review-only mode is absolute.** Do not modify any application source file under any circumstance.
- Allowed write output: **the report file under `.claude/reviews/` only.**
- No commits, pushes, branch operations, or destructive git commands.
- No scope broadening without explicit documentation in the report of why.
- **Orchestrator must not perform deep reasoning on file content.** If the orchestrator finds itself reading and interpreting source files or plan files directly, it must delegate to the appropriate gatherer or reviewer agent instead.
- Every `ACTIONABLE_NOW` finding must cite at least one `rule_refs` or `design_refs` entry. Findings without citations must be reclassified as `INSUFFICIENT_EVIDENCE`.
- Agents must never receive another agent's full raw output as context — only the extracted, structured fields they need.
- Gatherer agents (`plan-context-builder`, `rules-extractor`, `design-extractor`) must use verbatim extraction for high-authority content. Paraphrasing of rules or design invariants is not permitted.

---

## Failure Modes and Halt Conditions

| Condition | Action |
|---|---|
| Scope resolves to zero files | Halt; report unresolved scope |
| Invalid cycle count | Halt; report invalid configuration |
| `cargo check` fails | Write baseline-failure report; stop |
| Gatherer agent returns malformed output | Halt Phase 0; report which gatherer failed and why |
| Reviewer agent returns finding missing required fields | Discard finding; log discard in report appendix |
| All `problem-solver` agents return `BLOCKED_SOLUTIONS` | Include blockers in report; do not suppress |
| Report writer fails to write output file | Orchestrator writes minimal plain-text fallback to stdout |