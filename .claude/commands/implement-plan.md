# `/implement-plan` — Plan-Driven Implementation Command

Implement the saved plan: $ARGUMENTS

---

## Design Principles

- **Plan is source of truth.** Unexpected reality is handled as a plan deviation, not improvised execution.
- **Hard-gated execution.** Gate failures halt execution; unattended operation must not silently bypass safeguards.
- **Thin orchestrator, explicit specialists.** The invoking agent sequences and verifies; designated agents own review, classification, and solution semantics.
- **Structured context handoff.** Once digest artifacts exist, downstream agents consume structured contracts rather than raw narrative.
- **Scope-driven reviewers.** Which reviewers run is determined by actual changed files — not plan frontmatter flags.
- **Context-bounded cycles.** Cycle state is persisted to disk; the orchestrator never accumulates full records across cycles in working memory.

---

## Agent Roster

| Agent | Role | Output |
|---|---|---|
| `plan-context-builder` | Plan context extraction | `PLAN_DIGEST` |
| `rules-extractor` | Rule anchor extraction | `RULES_INDEX` |
| `design-extractor` | Design invariant extraction | `DESIGN_INDEX` |
| `shard-planner` | Scope-to-shard mapping and digest summaries | `SHARD_MAP` + `SHARD_DIGEST_SUMMARY[]` |
| `rust-implementer` | Delegated code and design-doc implementation | `IMPLEMENTATION_RESULT` |
| `rust-reviewer` | Rust quality review (all Rust-touching plans) | Structured findings |
| `architecture-reviewer` | Architecture integrity review (all Rust-touching plans) | Structured findings |
| `security-reviewer` | Security review (security-path-touching plans) | Structured findings |
| `cross-shard-reviewer` | Cross-shard contradiction detection | Structured findings |
| `finding-classifier` | Findings disposition/confidence quality gate | `CLASSIFIED_FINDINGS` |
| `problem-solver` | Findings-to-fix synthesis and design challenge evaluation | `SOLUTION_PACK` / `NO_ACTIONABLE_FIXES` / `BLOCKED_SOLUTIONS` |
| `test-writer` | Test expansion (all code-changing plans) | Test additions/updates |

**Execution contract (hard):** The invoking agent owns orchestration, gate enforcement, and final accountability. It **MUST NOT** write Rust code, read design docs for content reasoning, or perform review/classification work directly — all of these are delegated to named agents via the `task` tool. The only direct work the orchestrator performs is: file reads for gate verification, `cargo` commands for verification, and writing run-state/log files.

**Orchestrator delegation contract (hard — enforced):** Every named agent in the Agent Roster **MUST** be invoked via the `task` tool before the orchestrator may proceed past the step that requires it. The orchestrator MUST NOT skip an agent invocation because it believes it can do the work itself. The `task` tool runs each agent in an isolated context window — this is the core context-preservation mechanism of this command. Skipping an agent invocation is a protocol violation, not a valid optimization. If the orchestrator finds itself writing Rust code or synthesising review findings directly, it must STOP, record the violation, and restart the step using the correct agent.

**How to invoke agents:** Use the `task` tool with the agent's name as `agent_type` and the `model` override from the Model Assignments table below. Example:
```
task(agent_type="rust-implementer",    model="claude-sonnet-4.6", prompt="<step context + SOLUTION_PACK>")
task(agent_type="rust-reviewer",       model="claude-sonnet-4.6", prompt="<shard context>")
task(agent_type="finding-classifier",  model="gpt-4.1",           prompt="<canonicalized findings + PLAN_DIGEST + RULES_INDEX + DESIGN_INDEX>")
```
All custom agent names in the Agent Roster above map directly to `agent_type` values in the `task` tool.

---

## Model Assignments

Apply these model overrides on every `task` invocation. Never omit the `model` parameter — rely on defaults only when an agent is not listed here.

> **Note:** `claude-opus-4.6` and `claude-opus-4.5` return `CAPIError: 400 The requested model is not supported` for sub-agent task invocations in this environment (premium tier unavailable). Do not use Opus models.

> **Rate-limit fallback:** If you receive a weekly token limit error during a run, switch all non-Sonnet agents to `auto` (GitHub Copilot auto model selection). Auto model selection continues to function on premium requests even when the weekly cap is hit for specific models. Sonnet 4.6 agents may be switched to `claude-haiku-4.5` as an emergency fallback; record this in the Implementation Log.

### Tiered model strategy

Agents are assigned to one of three cost tiers based on task complexity:

| Tier | Models | Premium multiplier | When to use |
|---|---|---|---|
| **T0 — Free** | `gpt-4.1`, `gpt-5-mini` | 0× | Mechanical extraction, verbatim parsing, structured classification |
| **T1 — Low** | `claude-haiku-4.5`, `grok-code-fast-1` | 0.25–0.33× | Pattern-based analysis over structured inputs, mechanical code generation |
| **T2 — Standard** | `claude-sonnet-4.6` | 1× | Deep code review, security analysis, solution synthesis, production code writing |

### Agent → Model table

| Agent | Model | Tier | Rationale |
|---|---|---|---|
| `plan-context-builder` | `gpt-4.1` | T0 | Verbatim YAML/markdown extraction — no reasoning required |
| `rules-extractor` | `gpt-4.1` | T0 | Verbatim rule text extraction — fully mechanical |
| `design-extractor` | `gpt-4.1` | T0 | Verbatim invariant extraction — fully mechanical |
| `shard-planner` | `gpt-4.1` | T0 | File-path classification and keyword grep — rule-table driven |
| `finding-classifier` | `gpt-4.1` | T0 | Table-driven disposition classification using explicit policy rules |
| `cross-shard-reviewer` | `claude-haiku-4.5` | T1 | Pattern-based contradiction detection over structured shard digest summaries — no raw source needed |
| `test-writer` | `claude-haiku-4.5` | T1 | Mechanical test generation from spec; adversarial cases may be escalated to Sonnet |
| `architecture-reviewer` | `claude-haiku-4.5` | T1 | Structural pattern detection; escalate to Sonnet if security_flag hits appear |
| `rust-reviewer` | `claude-sonnet-4.6` | T2 | Deep code review with security/rule awareness — must be highest quality |
| `security-reviewer` | `claude-sonnet-4.6` | T2 | Crypto correctness and zero-knowledge threat model — no quality compromise |
| `problem-solver` | `claude-sonnet-4.6` | T2 | Solution synthesis across classified findings — requires full reasoning |
| `rust-implementer` | `claude-sonnet-4.6` | T2 | All production Rust code — must be highest quality |

### Architecture-reviewer escalation rule

If `architecture-reviewer` (running as `claude-haiku-4.5`) emits any finding with `security_flag: true`, re-invoke it as `claude-sonnet-4.6` for that shard only, passing the haiku output as suppression context. Record the escalation in the Implementation Log.

### Test-writer escalation rule

If the plan's Section 6d requests adversarial crypto tests or the shard is `shard-auth` or `shard-crypto`, invoke `test-writer` as `claude-sonnet-4.6` instead of `claude-haiku-4.5`. Record the override in the Implementation Log.

---

## Structured contract ownership (hard)

| Artifact | Owner |
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

## Step 1 — Resolve the plan file

Locate the plan from $ARGUMENTS:

- Full filename (e.g., `phase-1-cryptographic-primitives.md`) → read `.claude/plans/$ARGUMENTS`
- Filename without `.md` → append and retry
- `latest` → most recently created file in `.claude/plans/` by `created` frontmatter, excluding `_template.md` and `status: blocked` plans
- Empty or no match → list all plans (excluding `_template.md`) with title, status, and created date; ask the user to choose

Generate a run ID: `<plan-slug>-<YYYYMMDD-HHMMSS>`. All run-state artifacts are written under `.claude/runs/<run-id>/`.

---

## Step 2 — Pre-flight checks

Any failure halts execution. Do not attempt to auto-fix these conditions.

1. **Working directory:** confirm cwd is the repo root (directory containing `CLAUDE.md`). If not, halt with actual and expected paths.
2. **Git state:** run `git status --porcelain` and `git branch --show-current`. Record branch for the Implementation Log. If the working tree is dirty, display the dirty files and halt — re-run with `--force-dirty` to override.
3. **Baseline build:** run `cargo check --workspace`. If it fails, halt and report errors — the baseline must be green before implementation begins.

---

## Step 3 — Validate the plan and enforce gates

Any hard-gate failure stops execution before implementation begins.

1. Read the plan file and parse its YAML frontmatter.

2. **Status gate (hard):**
   - `approved` → proceed.
   - `draft` → halt: "Plan is still a draft. Set `status: approved` before running `/implement-plan`."
   - `blocked` → halt: "Plan is blocked." Display **Design Concerns / Open Questions**.
   - `in-progress` → halt: "Plan is already in progress. Reset `status: approved` to re-run."
   - `implemented` / `superseded` → halt: "Plan is already `<status>`. Reset to `approved` if intentional."

3. **Blocking-concerns gate (hard):** scan **Design Concerns / Open Questions** for any **Blocking** entry. If found, halt and display each — regardless of `status`.

4. **Display Handoff Notes:** output the plan's **Handoff Notes for Implementer** section verbatim before continuing.

5. **Verify Assumptions:** for each Assumptions entry, verify it holds against the current repo state. If any assumption is now false, halt with: which assumption failed, the current state, and a suggested resolution.

6. **Sub-phase detection:** if `sub-phase` is present in frontmatter, enable sub-phase-aware implementation.

7. **Sub-phase prerequisites (sub-phase plans only):** read the sub-phase roadmap from `sub-phase-roadmap`. Extract the sub-phase section. If any prerequisite sub-phase is missing or not marked implemented, halt with the list.

8. **Governance-sync gate (hard, pre-implementation):**
   - Require frontmatter `governance-sync-required`.
   - If `true` but **Governance sync actions** section is missing or says "None" → halt.
   - If `false` but section lists actions → halt.
   - If actions are listed: execute them in order. Apply `.claude/rules/*.md` and `.claude/agents/*.md` edits as declared. Re-read each target file and confirm the declared edit is present — if a file edit cannot be applied or verified, invoke Plan-deviation protocol and halt.
   - If any action touches `.claude/rules/*.md`: run `/copilot-sync` with a 30-second timeout.
     - Success → record outcome in Implementation Log.
     - Failure or timeout → record `GOVERNANCE_SYNC_DEGRADED` with failure reason; continue implementation. Surface warning at Step 6: "Run `/copilot-sync` manually before next session."
     - The file edits are the hard requirement; copilot-sync propagation is best-effort.

### Post-gate setup

After all gates pass (before updating plan status):

**A. Track selection (required before context build)**

Evaluate the plan to assign an implementation track. The track is locked after selection and recorded in the Implementation Log.

| Condition | Track |
|---|---|
| Section 6b lists any security-sensitive paths, OR `governance-sync-required: true`, OR Section 6a lists > 10 files | `full` |
| Section 6a lists 4–10 non-security files, no governance sync | `standard` |
| Section 6a lists ≤ 3 non-security files, single anticipated shard, no governance sync | `minimal` |

Track capabilities:

- **`full`** — all agents, max 8 remediation cycles, security-reviewer, cross-shard review when 2+ shards touched.
- **`standard`** — rust-implementer + rust-reviewer + architecture-reviewer + finding-classifier + problem-solver + test-writer; max 3 remediation cycles; cross-shard only if 2+ shards touched; no security-reviewer unless drift check fires.
- **`minimal`** — rust-implementer + rust-reviewer + finding-classifier + test-writer; 1 review cycle; no architecture-reviewer; no cross-shard review. If any HIGH finding surfaces after the review cycle → automatically escalate to `standard` track and continue.

**B. Build structured context artifacts (mandatory — HALT if skipped)**

**MUST** invoke all four agents in parallel via the `task` tool before any implementation begins. These are not optional pre-work — they are the contract surfaces downstream agents consume. Skipping them is a hard violation that blocks all subsequent steps.

All four have no dependencies on each other and **MUST** be invoked in a single parallel `task` call batch:

```
task(agent_type="plan-context-builder", model="gpt-4.1", ...)  → PLAN_DIGEST
task(agent_type="rules-extractor",      model="gpt-4.1", ...)  → RULES_INDEX
task(agent_type="design-extractor",     model="gpt-4.1", ...)  → DESIGN_INDEX
task(agent_type="shard-planner",        model="gpt-4.1", ...)  → SHARD_MAP + SHARD_DIGEST_SUMMARY[]
```

Wait for all four to complete before continuing.

Required consumer fields:
- `PLAN_DIGEST`: `highest_implemented_phase`, `in_progress_phases`, `deferred_phases`, `plans[]`, `handoffs[]`
- `RULES_INDEX`: `rules[].{id, source_file, anchor, verbatim, scope, severity_if_violated}`
- `DESIGN_INDEX`: `invariants[].{id, source_file, anchor, verbatim, scope, challenged}`
- `SHARD_MAP` + `SHARD_DIGEST_SUMMARY[]`: per `.claude/agents/shard-planner.md`

Apply output parsing protocol to each artifact. Do not pass full raw plan or design prose to reviewer or solver agents once these structures exist. If any artifact fails to build, halt and report which gatherer failed.

**C. Write initial run state to disk**

Write `.claude/runs/<run-id>/run-state.json`:

```json
{
  "run_id": "<run-id>",
  "plan_file": "<path>",
  "track": "<minimal|standard|full>",
  "branch": "<branch>",
  "cycle_count": 0,
  "finding_summary": { "CRITICAL": 0, "HIGH": 0, "MEDIUM": 0, "LOW": 0 },
  "disposition_summary": {
    "ACTIONABLE_NOW": 0, "INTENTIONAL_DECISION": 0,
    "DEFERRED_BY_PLAN": 0, "INSUFFICIENT_EVIDENCE": 0
  },
  "override_records": [],
  "governance_sync_degraded": false,
  "model_escalations": [],
  "cycles": []
}
```

**D. Update `status` to `in-progress`** in the plan frontmatter. (Intentionally after artifact build — a failed build leaves plan status unchanged.)

---

## Output parsing protocol (applies after every agent invocation)

1. Locate the named output block by scanning for its keyword header (e.g., `RUST_REVIEW`, `SOLUTION_PACK`, `CLASSIFIED_FINDINGS`, `IMPLEMENTATION_RESULT`). Strip any prose wrapper or markdown fences.
2. Validate that all required top-level fields are present per the agent's output contract.
3. **If the block is not found or required fields are missing:**
   a. Re-invoke the agent once. Prepend the raw output to the new invocation with the correction prompt: `"Your previous output did not match the required schema. Return only the structured block specified in your agent contract — no prose preamble, no markdown fences unless part of the schema."`
   b. If the second attempt also fails: halt with `PARSE_ERROR`. Record the agent name, expected schema, and raw output. Surface to the user. Do not infer missing field values.
4. Do not proceed with a partially parsed output.

---

## Step 4 — Implement

Follow the **Approach** section of the plan step by step, in order.

### Delegation model

`rust-implementer` is the **mandatory** executor for all coding steps. The orchestrator **MUST NOT** write, edit, or reason about Rust code directly — it orchestrates and verifies only.

**For every coding-focused Approach step:**
1. Invoke `rust-implementer` via the `task` tool with: full step context, the relevant `DIGEST_SLICE` from `SHARD_MAP`, expected outputs, and constraints. Apply output parsing protocol. Require `IMPLEMENTATION_RESULT`.
2. **Fallback is only available if `rust-implementer` returns `BLOCKED`** (not for convenience, context, or speed). If fallback is used, record it explicitly in the Implementation Log as a deviation with justification.
3. If any required delegation contract cannot be satisfied and direct fallback is also infeasible, invoke Plan-deviation protocol and halt.

> **If you are writing Rust code directly without a `BLOCKED` return from `rust-implementer`: STOP. You are in violation of the delegation contract. Restart the step using the `task` tool.**

1. Delegate coding-focused Approach steps to `rust-implementer`.
2. Execute every Approach step as written — via delegation or direct fallback.
3. **No speculative fallback:** if a step cannot be completed as written by either path, follow Plan-deviation protocol and halt.
4. After each Approach step, run `cargo check --workspace`. Fix compile errors before moving to the next step.

### Review invocation (scope-driven, mandatory)

Reviewers **MUST** be invoked via the `task` tool. The orchestrator **MUST NOT** perform review, classification, or finding synthesis directly. Read plan **Section 6** for reviewer guidance.

5. **Rust quality review:** if any `src-tauri/**/*.rs` files were changed, invoke `rust-reviewer` via `task` tool with model `claude-sonnet-4.6`. Pass `DIGEST_SLICE_<shard_id>` for each touched shard. Apply output parsing protocol. Skip and record if no Rust files changed.

6. **Security review:** if any files under `src-tauri/src/{crypto,auth,storage}/` were changed, invoke `security-reviewer` via `task` tool with model `claude-sonnet-4.6`. Pass relevant `DIGEST_SLICE` and security concerns from plan Section 6b. Apply output parsing protocol. Skip and record if no security-path files changed.
   - **Drift check (always runs):** if any sensitive file was touched that plan Section 6b did not anticipate, invoke Plan-deviation protocol and halt.

7. **Architecture review:** if any `src-tauri/**/*.rs` files were changed, invoke `architecture-reviewer` via `task` tool with model `claude-haiku-4.5`. Pass `DIGEST_SLICE_<shard_id>`. Apply output parsing protocol.
   - **Escalation:** if any returned finding has `security_flag: true`, re-invoke `architecture-reviewer` with model `claude-sonnet-4.6` for that shard, passing haiku output as suppression context. Record escalation in Implementation Log.

8. **INTERFACE_SLICE extraction (when 2+ shards have changed files):** before invoking `cross-shard-reviewer`, extract boundary pub signatures:

   ```bash
   grep -rn "^pub fn\|^pub trait\|^pub struct\|^pub enum\|^pub type" \
     <files at the boundary between each pair of changed shards>
   ```

   Pass the resulting `INTERFACE_SLICE` to `cross-shard-reviewer` alongside structured findings. This gives the reviewer the contract surface at shard boundaries without passing full implementation content.

### Findings remediation loop

9. **Finding canonicalization:** assign stable `CF-NNN` IDs in arrival order (rust-reviewer → architecture-reviewer → security-reviewer). Preserve original IDs in `source_id`. Mapping is fixed for the entire loop.

10. **Severity normalization:** security-reviewer: `CRITICAL` → `CRITICAL`, `WARNING` → `HIGH`, `NOTE` → `MEDIUM`. Rust and architecture findings use `HIGH|MEDIUM|LOW` directly.

11. **Finding classification:** invoke `finding-classifier` via `task` tool with model `gpt-4.1` and canonicalized normalized findings + `PLAN_DIGEST` + `RULES_INDEX` + `DESIGN_INDEX`. Apply output parsing protocol. Require `CLASSIFIED_FINDINGS`. The orchestrator **MUST NOT** classify findings itself.

12. **Problem-solver invocation:** if `ACTIONABLE_NOW` is empty, continue to Testing. Otherwise:

    **Security-scoped challenge checkpoint (hard):** check the `design_challenge_ledger` for entries where `requires_human_review: true`. If any exist, halt and display each to the user — the challenged constraint, the reviewer's rationale, and the proposed update. Require explicit `accept` or `reject` per challenge before proceeding. Record decisions in `run-state.json`. Resume with user decisions injected into the `problem-solver` invocation.

    Group remaining actionable findings:
    - One isolated solver invocation per CRITICAL finding
    - HIGH findings: one per finding or grouped by root cause
    - MEDIUM findings grouped by shard (max 10 per invocation)
    - One LOW batch

    Invoke `problem-solver` via `task` tool with model `claude-sonnet-4.6` per group with scoped files, `DIGEST_SLICE`, and `design_challenge_entries`. Apply output parsing protocol. Require `SOLUTION_PACK`, `NO_ACTIONABLE_FIXES`, or `BLOCKED_SOLUTIONS`. The orchestrator **MUST NOT** synthesise solutions itself.

    - If `BLOCKED_SOLUTIONS`: invoke Plan-deviation protocol and halt.
    - If `NO_ACTIONABLE_FIXES`: continue.
    - Invoke `rust-implementer` with model `claude-sonnet-4.6` and `SOLUTION_PACK`. Apply output parsing protocol. If any item returns `BLOCKED`, orchestrator implements directly; if also infeasible, invoke Plan-deviation protocol and halt.

    **Cross-shard consistency pass (`full` and `standard` tracks, when 2+ shards changed):** invoke `cross-shard-reviewer` with model `claude-haiku-4.5` and per-shard finding records + `SHARD_DIGEST_SUMMARY[]` + `INTERFACE_SLICE`. Apply output parsing protocol. Use stable labels (`remediation-cycle-1`, ...). CRITICAL/HIGH cross-shard findings feed into the next `finding-classifier` invocation.

    **Persist cycle state to disk** (see Run-state persistence).

    **Re-review:** re-run enabled reviewers on changed files. Repeat steps 9–12.

    **Acceptance thresholds:**
    - CRITICAL → must remediate before completion.
    - HIGH → must remediate or carry an approved Override Record before completion.
    - MEDIUM and LOW → record in Implementation Log with rationale when deferred.

    **Max cycles:** `full`: 8. `standard`: 3. `minimal`: 1 (then escalate to `standard` or accept with rationale).

### Orchestrator override for persistent HIGH findings

Available from cycle 3 (`full`) or cycle 2 (`standard`). **Never available for CRITICAL findings.**

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

All Override Records appear in the Implementation Log under "Finding overrides."

### Run-state persistence

After each remediation cycle, write `.claude/runs/<run-id>/cycle-<N>.json`:

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

Full finding prose, SOLUTION_PACK content, and IMPLEMENTATION_RESULT records must not accumulate across cycles. Reload from disk when a specific record is needed.

**Context compaction:** if the orchestrator estimates it cannot complete another full cycle within a safe context budget:
1. Persist current cycle state to disk.
2. Emit `CONTEXT_CHECKPOINT`: "Context compacted after cycle N — state at `.claude/runs/<run-id>/`. Resuming from disk."
3. Continue with a fresh working context loaded from run-state only.

### Plan-deviation protocol

If any Approach step cannot be executed as written, or a governance sync file edit cannot be applied:

1. Revert or stash any partial work so the repo is in a consistent state.
2. Append `## Plan Deviation` to the plan file: **Step**, **Expected**, **Actual**, **Suggested resolution**.
3. Update `status: blocked`.
4. Halt. Do not proceed.

### Testing

Invoke `test-writer` with model `claude-haiku-4.5` (or `claude-sonnet-4.6` for crypto/auth shards — see Test-writer escalation rule above) with the focus from plan Section 6d. Apply output parsing protocol. Run `cargo test` after completion and report results.

If Section 6d says no tests are needed and no Rust files changed: skip `test-writer` and record rationale.

### Sub-phase implementation decisions sync (mandatory for sub-phase plans)

1. Locate the sub-phase document (prefer explicit path in plan body).
2. Ensure the sub-phase doc has `## Implementation Decisions`; create if missing.
3. Append bullets: **decision + rationale + deferred follow-up if any**.
4. Required before Step 6 can set status to `implemented`.

### Validation checkpoint

If sub-phase plan: read the Validation checkpoint from the sub-phase roadmap. Run the full CI-equivalent local checks from Step 5. Display manual verification steps and acceptance criteria — do not mark `implemented` if the automated portion fails.

---

## Step 5 — Verify

1. Run `cargo fmt --all` (**Fix formatting**).
2. Run `cargo clippy --workspace --all-targets --all-features -- -D warnings` (**Run Clippy (warnings as errors)**). Fix related failures; note pre-existing unrelated issues.
3. Run `cargo test --workspace --all-targets --all-features` (**Run tests**). Fix related failures; note pre-existing unrelated issues.
4. Run `cargo build --workspace --release` (**Release build**). Fix related failures; note pre-existing unrelated issues.

---

## Step 6 — Mark complete and report

1. **Sub-phase decision-sync gate (hard):** verify the sub-phase doc has `## Implementation Decisions` reflecting this run. If missing or stale, return to the sync step.

2. **Design-doc sync gate (hard):** if `SOLUTION_PACK` contained any accepted challenges (including user-approved security-scoped ones), verify each referenced design doc was updated. If any accepted challenge has no corresponding update, halt.

3. **Documentation-impact gate (hard):** execute plan Section 7 (`Documentation impact`) before setting `status: implemented`.
   - If Section 7 lists concrete doc updates, apply them in this run and include them in **Files changed**.
   - If an item is explicitly marked deferred/optional in Section 7, keep it deferred but record rationale in **Deviations from plan** and **Documentation flagged**.
   - If applying a listed update would require a large canonical design rewrite (cross-phase contract refactor or broad semantic redesign), invoke Plan-deviation protocol and halt for user decision.

4. Update `status: implemented` in plan frontmatter.

5. Append **Implementation Log** to the plan file:
   - **Date** — ISO 8601 datetime
   - **Run ID** — generated in Step 1
   - **Track** — `minimal` / `standard` / `full`; note any mid-run escalation
   - **Branch** — from Step 2
   - **Execution mode** — rust-implementer (default) or orchestrator (fallback); note which steps fell back
   - **Agent evidence** — table: `Approach step | Agent | Model Requested | Model Reported | Agent ID | Outcome | Escalated?`. Parse `Model Reported` from the `model_self_reported:` field in each agent's output block header; use `—` if the field is absent. Record `true` in `Escalated?` for any agent invoked at a higher tier than its default.
   - **Files changed** — including any updated design docs
   - **Formatting check** — `cargo fmt --all -- --check` summary
   - **Clippy results** — `cargo clippy --workspace --all-targets --all-features -- -D warnings` summary
   - **Test results** — `cargo test --workspace --all-targets --all-features` summary
   - **Release build** — `cargo build --workspace --release` summary
   - **Rust review** — findings summary or "Skipped"
   - **Architecture review** — findings summary or "Skipped"; note model used
   - **Security review** — findings summary or "Skipped"
   - **Cross-shard review** — invocation count + findings or "N/A"
   - **Findings quality gate** — counts by disposition across all cycles
   - **Finding overrides** — for each Override Record: CF-NNN, rationale, confidence, decision
   - **Design challenge outcomes** — finding ID, summary, decision, rationale, design doc path or "N/A"; flag user-approved security challenges
   - **Governance sync** — action count, files updated, copilot-sync outcome or `GOVERNANCE_SYNC_DEGRADED`
   - **Sub-phase decisions sync** — doc path + decisions added/updated (or "N/A")
   - **Deviations from plan** — small adjustments only
   - **Documentation flagged** — verbatim from plan Section 7
   - **Run state path** — `.claude/runs/<run-id>/`
   - **Model escalations** — list of agents that ran at a higher tier than default, with reason

6. **Do not commit, push, or open a pull request.** Leave the working tree dirty.

7. **Report to the user:**

**Sub-phase plan:**
```
✓ Phase [X.Y] implementation complete — status: implemented
✓ Run ID: [run-id]
✓ Track: [minimal|standard|full] [escalated? note it]
✓ Branch: [branch]
✓ Execution mode: [rust-implementer delegated | fallback steps noted]
✓ Rust review: [Skipped | summary]
✓ Architecture review: [Skipped | summary + model used]
✓ Security review: [Skipped | summary]
✓ Cross-shard review: [N/A | count + summary]
✓ Findings quality gate: [counts by disposition]
✓ Finding overrides: [None | CF-NNN list with confidence]
✓ Design challenge outcomes: [None | summary; security-scoped user decisions noted]
✓ Formatting: [clean | failures fixed]
✓ Clippy: [clean | failures fixed | pre-existing unrelated issues]
✓ Tests: [summary]
✓ Release build: [success | failures fixed | pre-existing unrelated issues]
✓ Model escalations: [None | agent list with reason]
⚠ Governance sync: [OK | DEGRADED — run /copilot-sync manually]
→ Validation checkpoint (manual): [from sub-roadmap]
→ Acceptance criteria (manual): [from sub-roadmap]
→ Files changed: [list]
→ Sub-phase decisions sync: [doc path + count]
→ Documentation flagged: [from Section 7]
→ Run state: [.claude/runs/<run-id>/]
→ Next sub-phase: [X.Y+1 title, or "end of roadmap"]
```

**Full-phase or ad-hoc plan:** report the same fields, omitting sub-phase-specific items.

---

## Guardrails

- **NEVER write Rust code directly** — always delegate to `rust-implementer` via `task` tool.
- **NEVER synthesise review findings directly** — always delegate to `rust-reviewer`, `security-reviewer`, `architecture-reviewer`.
- **NEVER classify findings directly** — always delegate to `finding-classifier`.
- **NEVER synthesise solutions directly** — always delegate to `problem-solver`.
- **NEVER skip Step 3B context artifact build** — all four agents must be invoked before Step 4.
- Preserve hard-gate semantics in Step 3; do not silently downgrade failures.
- Do not skip the design-doc sync gate in Step 6 when accepted challenges exist.
- Do not skip plan Section 7 documentation-impact items when they are implementable in-run.
- Do not mark `status: implemented` unless all Step 5 CI-equivalent checks pass locally.
- Do not broaden implementation scope outside the approved plan without triggering Plan-deviation protocol.
- Do not auto-chain `/review-only` and `/implement-review`; this command is a separate entrypoint.
- Do not commit, push, or open pull requests.
- `cross-shard-reviewer` receives `SHARD_DIGEST_SUMMARY[]` + `INTERFACE_SLICE` — never full `DIGEST_SLICE` content.
- Override Records are prohibited for CRITICAL findings.
- Security-scoped design challenge decisions require explicit user input before `problem-solver` proceeds.

---

## Failure Modes and Halt Conditions

| Condition | Action |
|---|---|
| Plan file cannot be resolved | Halt and report candidate plan files |
| Any Step 3 gate fails | Halt before implementation begins |
| Structured context artifact build fails | Halt before updating plan status |
| **Step 3B context agents not invoked** | **Halt — context artifact build is mandatory before Step 4** |
| **Orchestrator writes Rust code without BLOCKED return** | **Record delegation violation in log; restart step via `task` tool** |
| Governance sync file edit cannot be applied or verified | Plan-deviation protocol, then halt |
| `/copilot-sync` fails or times out | Record `GOVERNANCE_SYNC_DEGRADED`; continue |
| Agent output fails to parse after one retry | Halt with `PARSE_ERROR`; surface raw output to user |
| `rust-implementer` BLOCKED and direct fallback infeasible | Plan-deviation protocol, then halt |
| `BLOCKED_SOLUTIONS` returned during remediation | Plan-deviation protocol, then halt |
| Security-scoped design challenge awaits user decision | Hard pause; resume on user input |
| HIGH Override Record confidence `UNCERTAIN` | Hard pause; resume on user input |
| Required findings thresholds not met after max cycles | Plan-deviation protocol, then halt |
| Sensitive path drift detected outside Section 6b | Plan-deviation protocol, then halt |
| Any Step 5 CI-equivalent local check fails | Halt — do not set `status: implemented` |
| Accepted design challenge missing design-doc update at Step 6 | Halt — require update before marking implemented |
| Section 7 documentation updates left unapplied without explicit deferred/optional rationale | Halt before `status: implemented` |
| Weekly token limit hit mid-run | Switch non-Sonnet agents to `auto`; switch Sonnet agents to `claude-haiku-4.5`; record in Implementation Log |
