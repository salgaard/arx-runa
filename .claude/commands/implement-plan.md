# `/implement-plan` — Plan-Driven Implementation Command

Implement the saved plan: $ARGUMENTS

---

## Design Principles

- **Plan-first implementation.** Approved plan intent is the source of truth; unexpected reality is handled as a plan deviation, not improvised execution.
- **Hard-gated execution.** Gate failures halt execution; unattended operation must not silently bypass safeguards.
- **Thin orchestrator, explicit specialists.** The invoking agent sequences and verifies; designated agents own review, classification, and solution semantics.
- **Structured context handoff.** Once digest artifacts exist, downstream agents consume structured contracts rather than raw narrative.

---

## Agent Roster

| Agent | Role | Output |
|---|---|---|
| `plan-context-builder` | Plan context extraction | `PLAN_DIGEST` |
| `rules-extractor` | Rule anchor extraction (via parallel gather) | `RULES_INDEX` |
| `design-extractor` | Design invariant extraction | `DESIGN_INDEX` |
| `shard-planner` | Scope-to-shard mapping and digest summaries | `SHARD_MAP` + `SHARD_DIGEST_SUMMARY[]` |
| `rust-implementer` | Delegated code implementation | `IMPLEMENTATION_RESULT` |
| `rust-reviewer` | Rust quality review when required by plan gates | Structured findings |
| `architecture-reviewer` | Architecture integrity review when required by plan gates | Structured findings |
| `security-reviewer` | Security review when required by plan gates | Structured findings |
| `cross-shard-reviewer` | Cross-shard contradiction detection in remediation loops | Structured findings |
| `finding-classifier` | Findings disposition/confidence quality gate | `CLASSIFIED_FINDINGS` |
| `problem-solver` | Findings-to-fix synthesis when required by plan gates | `SOLUTION_PACK` / `NO_ACTIONABLE_FIXES` / `BLOCKED_SOLUTIONS` |
| `test-writer` | Test expansion when required by plan gates | Test additions/updates |


**Implementer-agnostic**: this command runs on any CLI agent that can read `.claude/` resources. It assumes no specific model and no human in the loop beyond the hard gates below. Interactive confirmation is reserved for destructive or ambiguous actions — routine checks pass or fail, they do not prompt.

**Execution contract (hard)**: the invoking agent owns implementation end-to-end. It may delegate coding steps to sub-agents but retains orchestration, verification, and final accountability.

**Orchestrator delegation contract (hard)**: keep orchestration thin. Delegate reviewer semantics, finding classification, and solution synthesis to designated agents. When structured artifacts exist (`PLAN_DIGEST`, `RULES_INDEX`, `DESIGN_INDEX`, shard `DIGEST_SLICE`, normalized findings), pass those artifacts instead of raw prose.

## Structured contract ownership (hard)

- `PLAN_DIGEST` → `.claude/agents/plan-context-builder.md`
- `RULES_INDEX` → `.claude/agents/rules-extractor.md`
- `DESIGN_INDEX` → `.claude/agents/design-extractor.md`
- `SHARD_MAP` + `SHARD_DIGEST_SUMMARY[]` → `.claude/agents/shard-planner.md`
- `CLASSIFIED_FINDINGS` → `.claude/agents/finding-classifier.md`
- `SOLUTION_PACK` / `NO_ACTIONABLE_FIXES` / `BLOCKED_SOLUTIONS` → `.claude/agents/problem-solver.md`
- `IMPLEMENTATION_RESULT` → `.claude/agents/rust-implementer.md`

This command owns orchestration and gates. Producer schema details live in agent contracts.

---

## Step 1 — Resolve the plan file

Locate the plan from $ARGUMENTS:

- Full filename (e.g., `phase-1-cryptographic-primitives.md`) → read `.claude/plans/$ARGUMENTS`
- Filename without `.md` → append and retry
- `latest` → find the most recently created file in `.claude/plans/` by `created` frontmatter field, excluding `_template.md` and any plan with `status: blocked`
- Empty or no match → list all plans (excluding `_template.md`) with title, status, and created date; ask the user to choose

---

## Step 2 — Pre-flight checks

Any failure here halts execution. Do not attempt to auto-fix these conditions.

1. **Working directory**: confirm cwd is the repo root (directory containing `CLAUDE.md`). If not, halt with actual and expected paths.
2. **Git state**: run `git status --porcelain` and `git branch --show-current`. Record branch for the Implementation Log. If the working tree is dirty, display the dirty files and halt — re-run with `--force-dirty` to override. Do not prompt; this is a hard gate on unattended runs.
3. **Baseline build**: run `cargo check --workspace` from the repo root. If it fails, halt and report errors — the baseline must be green before implementation begins.

---

## Step 3 — Validate the plan and enforce gates

Any hard-gate failure stops execution before implementation begins.

1. Read the plan file and parse its YAML frontmatter.

2. **Status gate** (hard):
   - `approved` → proceed.
   - `draft` → halt: "Plan is still a draft. Set `status: approved` before running `/implement-plan`."
   - `blocked` → halt: "Plan is blocked." Display **Design Concerns / Open Questions**.
   - `in-progress` → halt: "Plan is already in progress. Reset `status: approved` to re-run."
   - `implemented` / `superseded` → halt: "Plan is already `<status>`. Reset to `approved` if intentional."

3. **Blocking-concerns gate** (hard): scan **Design Concerns / Open Questions** for any **Blocking** entry. If found, halt and display each blocking concern — regardless of `status`.

4. **Display Handoff Notes**: output the plan's **Handoff Notes for Implementer** section verbatim before continuing.

5. **Verify Assumptions**: for each Assumptions entry, verify it still holds against the current repo state. If any assumption is now false, halt with which assumption failed, the current state, and a suggested resolution.

6. **Sub-phase detection**: if `sub-phase` is present in frontmatter, enable sub-phase-aware implementation. Otherwise use standard flow.

7. **Sub-phase prerequisites** (sub-phase plans only): read the sub-phase roadmap from the `sub-phase-roadmap` frontmatter field. Extract the sub-phase section. If any prerequisite sub-phase is missing or not marked implemented, halt with the list.

8. **Implementation-mode gate**:
   - Read `implementation-delegation` from frontmatter.
   - If missing (legacy plan), default to `direct`; record migration note in Implementation Log.
   - Allowed values: `direct`, `delegated`. Any other value is a hard failure.
   - If `delegated`: verify Section 14 exists with explicit delegation boundaries.

9. **Rust-review gate** (hard):
   - Prefer frontmatter `rust-review-agent-required`. If missing, infer from Section 6 and emit migration warning.
   - Parse Section 6 for an explicit "Invoke rust-reviewer? YES/NO" decision. If missing or ambiguous, halt.
   - Frontmatter and Section 6 must agree; mismatch halts.

10. **Security-review gate** (hard):
    - Prefer frontmatter `security-agent-required`. If missing, infer from Section 7 and emit migration warning.
    - Parse Section 7 for an explicit "Invoke security-reviewer? YES/NO" decision. If missing or ambiguous, halt.
    - Frontmatter and Section 7 must agree.

11. **Architecture-review gate** (hard):
    - Prefer frontmatter `architecture-review-agent-required`. If missing, infer from Section 8 (or from `rust-review-agent-required`); emit migration warning.
    - Parse Section 8 for an explicit "Invoke architecture-reviewer? YES/NO" decision. If missing or ambiguous, halt.
    - Rust-touching enforcement: if `rust-review-agent-required: true`, `architecture-review-agent-required` must also be `true`.

12. **Solution-synthesis gate** (hard):
    - Prefer frontmatter `solution-agent-required`. If missing, infer from Section 9 and emit migration warning.
    - Parse Section 9 for an explicit "Invoke problem-solver? YES/NO" decision. If missing or ambiguous, halt.
    - Coupling: if any reviewer agent is required, `solution-agent-required` must be `true` unless Section 9 contains a non-empty `Solver override justification:`.

13. **Testing-agent gate** (hard):
    - Require frontmatter `test-agent-required`. If missing, halt.
    - Parse Section 10 for an explicit "Invoke test-writer? YES/NO" decision. If missing or ambiguous, halt.
    - Frontmatter and Section 10 must agree.

14. **Governance-sync gate** (hard, pre-implementation):
    - Require frontmatter `governance-sync-required`. If missing, halt.
    - If `true` but **Governance sync actions** section is missing or says "None," halt.
    - If `false` but section lists actions, halt.
    - If actions are listed: execute them in order before proceeding. Apply `.claude/rules/*.md` and `.claude/agents/*.md` updates as declared. If any action touches `.claude/rules/*.md`, run `/copilot-sync` once after rule edits. Re-read each target file and confirm the declared update is present. If any action cannot be completed or verified, invoke Plan-deviation protocol and halt.

### Post-gate setup

After all gates pass, perform these setup actions before updating plan status:

1. **Build structured context artifacts** — spawn in parallel:
   - `plan-context-builder` → `PLAN_DIGEST`
   - `rules-extractor` → `RULES_INDEX`
   - `design-extractor` → `DESIGN_INDEX`
   - `shard-planner` → `SHARD_MAP` + `SHARD_DIGEST_SUMMARY[]`

   Required consumer fields:
   - `PLAN_DIGEST`: `highest_implemented_phase`, `in_progress_phases`, `deferred_phases`, `plans[]`, `handoffs[]`
   - `RULES_INDEX`: `rules[].{id, source_file, anchor, verbatim, scope, severity_if_violated}`
   - `DESIGN_INDEX`: `invariants[].{id, source_file, anchor, verbatim, scope, challenged}`
   - `SHARD_MAP` + `SHARD_DIGEST_SUMMARY[]`: per `.claude/agents/shard-planner.md`

   Do not pass full raw plan or design prose to reviewer or solver agents once these structures exist. If any artifact fails to build, halt and report which gatherer failed — do not update plan status.

2. **Update `status` to `in-progress`** in the plan frontmatter. (Intentionally after artifact build — a failed build leaves plan status unchanged.)

---

## Step 4 — Implement

Follow the **Approach** section of the plan step by step, in order.

### Delegation boundaries (hard)

- In `delegated` mode, coding work may be delegated to `rust-implementer`; orchestration, gate enforcement, and verification stay with the invoking agent.
- `rust-reviewer`, `security-reviewer`, and `architecture-reviewer` provide findings; remediation requires explicit synthesis and implementation flow.
- `finding-classifier` and `problem-solver` govern remediation structure; `rust-implementer` executes approved remediation payloads.
- If any required delegation contract cannot be satisfied, invoke Plan-deviation protocol and halt.

1. Determine execution mode from `implementation-delegation` (`direct` default for legacy plans).
2. If `delegated`: delegate coding-focused Approach steps to `rust-implementer` with full step context, expected outputs, and constraints. The invoking agent still owns decisions, review, and verification.
3. Execute every Approach step as written.
4. **No speculative fallback**: if a step cannot be completed as written, follow Plan-deviation protocol and halt.
5. After each Approach step, run `cargo check --workspace` as a fast fail-check. Fix compile errors before moving to the next step.

6. **Rust quality review** — driven by Section 6 and `rust-review-agent-required`:
   - YES → after implementation (or at a sensible midpoint for long runs), invoke `rust-reviewer` on touched Rust files. Pass the `DIGEST_SLICE_<shard_id>` for each touched shard; do not pass full `RULES_INDEX` or `DESIGN_INDEX`.
   - NO → skip; record Section 6 rationale in Implementation Log.

7. **Security review** — driven by Section 7 and `security-agent-required`:
   - YES → invoke `security-reviewer` on touched files under `src-tauri/src/{crypto,auth,storage}/`. Pass relevant `DIGEST_SLICE`.
   - NO → skip. The drift check (item 10) still fires if sensitive paths are touched.

8. **Architecture review** — driven by Section 8 and `architecture-review-agent-required`:
   - YES → invoke `architecture-reviewer` on touched Rust files. Pass relevant `DIGEST_SLICE`.
   - NO → skip; record Section 8 rationale in Implementation Log.

9. **Findings remediation loop** — driven by Section 9 and `solution-agent-required`:
   - Consolidate findings from all enabled review agents.
   - **Severity normalization (required before classifier invocation)**: map security-reviewer severities to the common scale — WARNING → MEDIUM, NOTE → LOW, CRITICAL stays CRITICAL.
   - Invoke `finding-classifier` with the consolidated normalized findings + `PLAN_DIGEST` + `RULES_INDEX` + `DESIGN_INDEX`. Require disposition outputs: `ACTIONABLE_NOW`, `INTENTIONAL_DECISION`, `DEFERRED_BY_PLAN`, `INSUFFICIENT_EVIDENCE`. Downstream remediation draws exclusively from `ACTIONABLE_NOW`.
   - If `ACTIONABLE_NOW` is empty, continue to Testing.
   - Group actionable findings before solver launch:
     - one isolated solver invocation per CRITICAL/HIGH finding
     - MEDIUM findings grouped by shard (max 10 per invocation)
     - one LOW batch
   - If `solution-agent-required: true`: invoke `problem-solver` per group with scoped files and `DIGEST_SLICE`. Require one of `SOLUTION_PACK`, `NO_ACTIONABLE_FIXES`, or `BLOCKED_SOLUTIONS` per `.claude/agents/problem-solver.md`.
   - If `BLOCKED_SOLUTIONS` returned: invoke Plan-deviation protocol and halt.
   - If `NO_ACTIONABLE_FIXES` returned: continue.
   - In `delegated` mode: invoke `rust-implementer` with `SOLUTION_PACK`. Require `IMPLEMENTATION_RESULT` per `.claude/agents/rust-implementer.md`. If any item returns `BLOCKED`, invoke Plan-deviation protocol and halt.
   - In `direct` mode: implement fixes directly or delegate selectively; same `SOLUTION_PACK` contract applies if delegating.
   - **Cross-shard consistency pass**: after each remediation cycle, when two or more shards had changed files, invoke `cross-shard-reviewer` once with per-shard finding records + `SHARD_DIGEST_SUMMARY[]`. Use stable labels (`remediation-cycle-1`, `remediation-cycle-2`, ...). CRITICAL/HIGH cross-shard findings feed back into the next `finding-classifier` invocation.
   - Re-run enabled reviewers on changed files after each remediation cycle.
   - **Acceptance thresholds**:
     - All Rust HIGH → must remediate before completion.
     - All Architecture HIGH → must remediate before completion.
     - All Security CRITICAL → must remediate before completion.
     - Architecture MEDIUM/LOW and Security WARNING/NOTE (normalized to MEDIUM/LOW) → record in Implementation Log with rationale when deferred.
   - Max remediation cycles: 8. If required thresholds are not met after cycle 8, invoke Plan-deviation protocol and halt.
   - If `solution-agent-required: true`: reviewer-only loops are forbidden — every actionable remediation cycle must invoke both `problem-solver` and a remediation step.

10. **Drift check (always runs)**: compare files actually modified under `src-tauri/src/{crypto,auth,storage}/` against the plan's **Expected sensitive path set** (Section 7a). If any sensitive file was touched that the plan did not anticipate, invoke Plan-deviation protocol and halt. Do not auto-invoke `security-reviewer` to paper over scope drift. Note: this check is directory-scoped; security keyword hits in `shard-default` files outside these directories are flagged by the shard-planner but do not trigger this check.

### Plan-deviation protocol

If any Approach step cannot be executed as written, or a governance sync action cannot be completed as specified:

1. Revert or stash any partial work for that step so the repo is in a consistent state.
2. Append a `## Plan Deviation` section to the plan file with:
   - **Step** — which step hit the problem
   - **Expected** — what the plan said to do
   - **Actual** — what is true in the repo
   - **Suggested resolution** — one or two options
3. Update `status: blocked`.
4. Halt and report the deviation. Do not proceed with subsequent steps.

### Testing

Read Section 10 and follow its decision:
- **Invoke test-writer? YES**: invoke `test-writer` with the specific focus from Section 10. Run `cargo test` after completion. Report results.
- **Invoke test-writer? NO**: rely on tests written during implementation. Run `cargo test` and `cargo clippy -- -D warnings`.
- If decision is missing or ambiguous: halt (caught at gate 13, but enforce here as a second-check).

### Sub-phase implementation decisions sync (mandatory for sub-phase plans)

1. Locate the sub-phase document path (prefer explicit path in plan body).
2. Ensure the sub-phase doc has an `## Implementation Decisions` section; create if missing.
3. Append or update bullets for decisions made during implementation that were previously optional or ambiguous.
4. Keep entries concise: **decision + rationale + deferred follow-up if any**.
5. This sync is required before Step 6 status can be set to `implemented`.

### Validation checkpoint

If sub-phase plan: after implementation, read the Validation checkpoint from the sub-phase roadmap. Run automated tests. Display manual verification steps and acceptance criteria to the user — do not mark `implemented` if the automated portion fails.

---

## Step 5 — Verify

1. Run `cargo test` (full workspace). Fix related failures before marking implemented. Record pre-existing unrelated failures in the Implementation Log.
2. Run `cargo clippy --workspace -- -D warnings`. Fix new warnings introduced by this implementation; note pre-existing warnings in the Implementation Log.

---

## Step 6 — Mark complete and report

1. **Sub-phase decision-sync gate (hard)**: if sub-phase plan, verify the target sub-phase doc has an `## Implementation Decisions` section reflecting this run. If missing or stale, return to the decision-sync subsection in Step 4.

2. Update `status: implemented` in the plan frontmatter.

3. Append an **Implementation Log** section to the plan file:
   - **Date** — ISO 8601 datetime
   - **Branch** — recorded in Step 2
   - **Execution mode** — `direct` or `delegated` (plus brief delegation summary)
   - **Agent evidence** — table: `Approach step | Agent | Agent ID | Outcome`
   - **Files changed** — list of modified/created files
   - **Test results** — `cargo test` summary
   - **Clippy results** — clean / warnings introduced / pre-existing noted
   - **Rust review** — `rust-reviewer` findings summary or "N/A"
   - **Architecture review** — `architecture-reviewer` findings summary or "N/A"
   - **Security review** — `security-reviewer` findings summary or "N/A"
   - **Cross-shard review** — invocation count + any findings or "N/A"
   - **Findings quality gate** — counts by disposition
   - **Governance sync** — action count, files updated, `/copilot-sync` outcome when applicable
   - **Sub-phase decisions sync** — target doc path + decisions added/updated (or "N/A")
   - **Deviations from plan** — small adjustments (large deviations should have halted at Step 4)
   - **Documentation flagged** — verbatim list from the plan's **Documentation impact** section

4. **Do not commit, push, or open a pull request.** Leave the working tree dirty.

5. **Report to the user**:

**Sub-phase plan**:
```
✓ Phase [X.Y] implementation complete — status: implemented
✓ Branch: [branch]
✓ Execution mode: [direct|delegated]
✓ Agent evidence: [summary]
✓ Rust review: [N/A or findings summary]
✓ Architecture review: [N/A or findings summary]
✓ Security review: [N/A or findings summary]
✓ Cross-shard review: [N/A or invocation count + findings summary]
✓ Findings quality gate: [counts by disposition]
✓ Solution synthesis: [N/A or problem-solver summary]
✓ Tests: [summary]
✓ Clippy: [clean / N warnings]
→ Validation checkpoint (manual): [checkpoint description from sub-roadmap]
→ Acceptance criteria (manual): [list from sub-roadmap]
→ Files changed: [list]
→ Governance sync: [summary]
→ Sub-phase decisions sync: [doc path + decisions count]
→ Documentation flagged: [list from Documentation impact]
→ Next sub-phase: [X.Y+1 title, or "end of roadmap"]
```

**Full-phase or ad-hoc plan**: report implemented work, branch, agent evidence summary, review summaries, cross-shard summary, findings-quality-gate counts, problem-solver summary, test results, clippy results, governance-sync summary, files changed, and the Documentation impact list.

---

## Guardrails

- Preserve hard-gate semantics in Step 3; do not silently downgrade failures.
- Do not bypass required reviewers, classifier, solver, or testing decisions declared by plan gates.
- Do not broaden implementation scope outside the approved plan without triggering Plan-deviation protocol.
- Do not auto-chain `/review-only` and `/implement-review`; this command is a separate entrypoint.
- Do not commit, push, or open pull requests.
- `cross-shard-reviewer` receives `SHARD_DIGEST_SUMMARY[]` (IDs only) — never full `DIGEST_SLICE` content.

---

## Failure Modes and Halt Conditions

| Condition | Action |
|---|---|
| Plan file cannot be resolved | Halt and report candidate plan files |
| Any Step 3 gate fails | Halt before implementation begins |
| Structured context artifact build fails (post-gate setup) | Halt before updating plan status |
| Governance sync action fails verification | Plan-deviation protocol, then halt |
| Required reviewer/solver/test agent decision is ambiguous | Halt and report missing/ambiguous gate decision |
| `BLOCKED_SOLUTIONS` returned during required remediation | Plan-deviation protocol, then halt |
| Required findings thresholds not met after remediation-cycle-8 | Plan-deviation protocol, then halt |
| Sensitive path drift detected outside expected path set | Plan-deviation protocol, then halt |
