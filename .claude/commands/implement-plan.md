# `/implement-plan` — Plan-Driven Implementation Command

Implement the saved plan: $ARGUMENTS

---

## Design Principles

- **Plan-first implementation.** Approved plan intent is the source of truth; unexpected reality is handled as a plan deviation, not improvised execution.
- **Hard-gated execution.** Gate failures halt execution; unattended operation must not silently bypass safeguards.
- **Thin orchestrator, explicit specialists.** The invoking agent sequences and verifies; designated agents own review/classification/solution semantics.
- **Structured context handoff.** Once digest artifacts exist, downstream agents consume structured contracts rather than raw narrative.
- **Step-model clarity.** `/implement-plan` remains step-oriented implementation flow; it does not collapse into review-only phase orchestration.

---

## Agent Roster

| Agent | Role | Output |
|---|---|---|
| `plan-context-builder` | Plan context extraction (via parallel gather) | `PLAN_DIGEST` |
| `rules-extractor` | Rule anchor extraction (via parallel gather) | `RULES_INDEX` |
| `design-extractor` | Design invariant extraction (via parallel gather) | `DESIGN_INDEX` |
| `shard-planner` | Scope-to-shard mapping and digest summaries | `SHARD_MAP` + `SHARD_DIGEST_SUMMARY[]` |
| `rust-implementer` | Delegated code implementation | `IMPLEMENTATION_RESULT` |
| `rust-reviewer` | Rust quality review when required by plan gates | Structured findings |
| `architecture-reviewer` | Architecture integrity review when required by plan gates | Structured findings |
| `security-reviewer` | Security review when required by plan gates | Structured findings |
| `cross-shard-reviewer` | Cross-shard contradiction detection in remediation loops | Structured findings |
| `finding-classifier` | Findings disposition/confidence quality gate during remediation | `CLASSIFIED_FINDINGS` |
| `problem-solver` | Findings-to-fix synthesis when required by plan gates | `SOLUTION_PACK` / `NO_ACTIONABLE_FIXES` / `BLOCKED_SOLUTIONS` |
| `test-writer` | Test expansion when required by plan gates | Test additions/updates |

**Implementer-agnostic**: this command is designed to be run by any CLI agent that can read `.claude/` resources. It assumes no specific model and no human in the loop beyond the hard gates below. Interactive confirmation is reserved for destructive or ambiguous actions — routine checks pass or fail, they do not prompt.

**Execution contract (hard)**: the invoking agent owns implementation end-to-end according to the approved plan. It may delegate coding steps to sub-agents, but must retain orchestration, verification, and final accountability.

**Orchestrator delegation contract (hard)**: keep orchestration thin. Delegate reviewer semantics, finding classification, and solution synthesis to the designated agents. When structured artifacts exist (`PLAN_DIGEST`, `RULES_INDEX`, `DESIGN_INDEX`, shard `DIGEST_SLICE`, normalized findings), pass those artifacts instead of raw prose.

## Structured contract ownership (hard)

- `PLAN_DIGEST` → `.claude/agents/plan-context-builder.md`
- `RULES_INDEX` → `.claude/agents/rules-extractor.md`
- `DESIGN_INDEX` → `.claude/agents/design-extractor.md`
- `SHARD_MAP` + `SHARD_DIGEST_SUMMARY[]` → `.claude/agents/shard-planner.md`
- `CLASSIFIED_FINDINGS` → `.claude/agents/finding-classifier.md`
- `SOLUTION_PACK` / `NO_ACTIONABLE_FIXES` / `BLOCKED_SOLUTIONS` → `.claude/agents/problem-solver.md`
- `IMPLEMENTATION_RESULT` → `.claude/agents/rust-implementer.md`

This command owns orchestration and gates. Producer schema details live in agent contracts.

## Step 1 — Resolve the plan file

Locate the plan file from $ARGUMENTS:
- If $ARGUMENTS is a full filename (e.g., `phase-1-cryptographic-primitives.md`),
  read `.claude/plans/$ARGUMENTS`
- If $ARGUMENTS is a filename without the `.md` extension, append it and try again
- If $ARGUMENTS is `latest`, find the most recently created file in `.claude/plans/`
  by the `created` frontmatter field, excluding `_template.md` **and excluding any plan with `status: blocked`** (a blocked plan is never the intended target of `latest`)
- If $ARGUMENTS is empty or no match is found, list all files in `.claude/plans/`
  (excluding `_template.md`) with their title, status, and created date, then ask
  the user to choose one

## Step 2 — Pre-flight checks

Before touching the plan, verify the working environment is sane. Any failure here halts execution — do not try to "fix" these conditions automatically.

1. **Working directory**: confirm cwd is the repo root (the directory containing `CLAUDE.md`). If not, halt with the actual and expected paths.
2. **Git state**: run `git status --porcelain` and `git branch --show-current`. Record the branch for the Implementation Log. If the working tree is not clean, display the dirty files and halt — the user must commit, stash, or explicitly approve proceeding by re-running with a `--force-dirty` argument (do **not** prompt; this is a hard gate on unattended runs).
3. **Baseline build**: run `cargo check --workspace` from the repo root. If it fails, halt and report the errors — the baseline must be green so that any later failure can be attributed to the implementation, not pre-existing breakage.

## Step 3 — Validate the plan and enforce gates

### Gate map (quick scan)

- **Plan state gates**: status, blocking concerns, assumptions, and sub-phase prerequisites.
- **Agent-contract gates**: implementation mode and rust/security/architecture/solution/test requirement consistency.
- **Governance gates**: pre-implementation governance sync and design-challenge approvals.
- **Halt semantics**: any hard-gate failure stops execution before Step 4 implementation work.

1. Read the plan file and parse its YAML frontmatter.
2. **Status gate** (hard):
   - `status: approved` → proceed.
   - `status: draft` → halt with: "Plan is still a draft. Review it and change `status:` to `approved` before running `/implement-plan`."
   - `status: blocked` → halt with: "Plan is blocked by unresolved Design Concerns. Resolve them before running `/implement-plan`." Display the **Design Concerns / Open Questions** section.
   - `status: in-progress` → halt with: "Plan is already in progress. Either resume manually or reset `status:` to `approved` to re-run."
   - `status: implemented` or `superseded` → halt with: "Plan is already `<status>`. Reset `status:` to `approved` first if intentional."
3. **Blocking-concerns gate** (hard): scan **Design Concerns / Open Questions** for any entry whose Classification is **Blocking**. If found, halt regardless of `status:` and display each blocking concern.
4. **Display Handoff Notes**: output the plan's **Handoff Notes for Implementer** section verbatim before going further.
5. **Verify Assumptions**: walk the **Assumptions** section. For each assumption, check it still holds against the current repo state. If any assumption is now false, halt with which assumption failed, what the current state is, and suggested resolution.
6. **Sub-phase detection**: check if `sub-phase` field exists in frontmatter.
   - If present: sub-phase-aware implementation.
   - If absent: standard implementation flow.
7. **If sub-phase plan**:
   - Read the sub-phase roadmap from `sub-phase-roadmap` frontmatter field.
   - Extract the specific sub-phase section.
   - Check if prerequisite sub-phases are complete. If prerequisite is missing or not implemented, halt with the list of missing prerequisites.
8. **Implementation-mode gate**:
   - Read frontmatter field `implementation-delegation`.
   - If missing (legacy plan), default to `direct` and record a migration note in the Implementation Log.
   - Allowed values: `direct`, `delegated`. Any other value is a hard failure.
   - If `delegated`, verify the plan includes Section 14 with explicit delegation boundaries.
9. **Rust-review gate** (hard):
   - Prefer frontmatter field `rust-review-agent-required` (`true`/`false`).
   - If missing (legacy plan), infer from Section 6 and emit a migration warning.
   - Parse Section 6 for an explicit "Invoke rust-reviewer agent? YES/NO" decision.
   - If the Section 6 decision is missing or ambiguous, halt.
   - If Section 6 says YES but `rust-review-agent-required` is not `true`, halt.
   - If Section 6 says NO but `rust-review-agent-required` is not `false`, halt.
10. **Security-review gate** (hard):
   - Prefer frontmatter field `security-agent-required` (`true`/`false`).
   - If missing (legacy plan), infer from Section 7 and emit a migration warning.
   - Parse Section 7 for an explicit "Invoke security-reviewer agent? YES/NO" decision.
   - If the Section 7 decision is missing or ambiguous, halt.
   - Consistency checks same as Rust-review gate.
11. **Architecture-review gate** (hard):
   - Prefer frontmatter field `architecture-review-agent-required` (`true`/`false`).
   - If missing: infer from Section 8 (if present) or from `rust-review-agent-required`; emit migration warning.
   - If Section 8 exists, parse for explicit "Invoke architecture-reviewer agent? YES/NO".
   - If decision missing or ambiguous, halt.
   - Rust-touching enforcement: if `rust-review-agent-required` is `true`, `architecture-review-agent-required` must be `true`.
12. **Solution-synthesis gate** (hard):
   - Prefer frontmatter field `solution-agent-required` (`true`/`false`).
   - If missing (legacy plan), infer from Section 9 and emit a migration warning.
   - Parse Section 9 for an explicit "Invoke problem-solver agent? YES/NO" decision.
   - If the decision is missing or ambiguous, halt.
   - Coupling enforcement: if any reviewer agent is required (`true`), `solution-agent-required` must be `true` unless Section 9 includes a non-empty `Solver override justification:` line and an explicit direct-handoff statement.
13. **Testing-agent gate** (hard):
   - Require frontmatter field `test-agent-required` (`true`/`false`). If missing, halt.
   - Parse Section 10 for an explicit "Invoke test-writer agent? YES/NO" decision.
   - If missing or ambiguous, halt. Consistency checks same as above gates.
14. **Governance-sync gate** (hard, pre-implementation):
   - Require frontmatter field `governance-sync-required` (`true`/`false`). If missing, halt.
   - Parse **Governance sync actions (pre-implementation)** section.
   - If `governance-sync-required: true` but section is missing or says "None", halt.
   - If `governance-sync-required: false` but section lists actions, halt.
   - If actions are listed: execute them in order before Step 4. Apply `.claude/rules/*.md`, `.claude/reference/*.md`, and `.claude/agents/*.md` updates as declared. If any action touches `.claude/rules/*.md`, run `/copilot-sync` once after rule edits. Re-read each target file and confirm the declared update is present. If any action cannot be completed or verified, invoke Plan-deviation protocol and halt.
15. **Design-challenge approvals gate** (hard):
   - Prefer frontmatter fields `design-challenge-approvals-required` (`true`/`false`) and `approved-design-challenges` (list of `DC-xxx` IDs).
   - If fields are missing (legacy plan), default to `design-challenge-approvals-required: false` and `approved-design-challenges: []`; emit migration warning.
   - Parse Section 13. Consistency checks:
     - If `design-challenge-approvals-required: true` but Section 13 is missing or "None", halt.
     - If `design-challenge-approvals-required: false` but Section 13 lists approvals, halt.
     - If `design-challenge-approvals-required: true` and `approved-design-challenges` is empty, halt.
     - Every ID in `approved-design-challenges` must exist in Section 13 with `Approval status: Approved`; otherwise halt.
   - Store the validated `APPROVED_DESIGN_CHALLENGES` list for use in Step 4. Do not re-derive it in Step 4 — read the validated list from here.

16. **Build Step 4 structured context artifacts** — spawn in parallel before setting status:
   - `plan-context-builder` for `PLAN_DIGEST`
   - `rules-extractor` for `RULES_INDEX`
   - `design-extractor` for `DESIGN_INDEX`
   - `shard-planner` for `SHARD_MAP` + `SHARD_DIGEST_SUMMARY[]`
   
   Required consumer fields:
   - `PLAN_DIGEST`: `highest_implemented_phase`, `in_progress_phases`, `deferred_phases`, `plans[]`, `handoffs[]`
   - `RULES_INDEX`: `rules[].{id,source_file,anchor,verbatim,scope,severity_if_violated}`
   - `DESIGN_INDEX`: `invariants[].{id,source_file,anchor,verbatim,scope,challenged}`
   - `SHARD_MAP` + `SHARD_DIGEST_SUMMARY[]`: per `.claude/agents/shard-planner.md`
   
   If any artifact fails to build, halt and report which gatherer failed before updating plan status.
   
   Do not pass full raw plan/rules/design prose to reviewer or solver agents once these structures exist.

17. **Update `status` to `in-progress`** in the plan file's frontmatter. (Intentionally after artifact build — if Step 16 fails, the plan status is not modified.)

## Step 4 — Implement

Follow the **Approach** section of the plan step by step, in order.

### Delegation boundaries (hard, quick scan)

- In `delegated` mode, coding work may be delegated to `rust-implementer`; orchestration, gate enforcement, and verification stay with the invoking agent.
- `rust-reviewer`, `security-reviewer`, and `architecture-reviewer` provide findings; remediation still requires explicit synthesis and implementation flow.
- `finding-classifier` and `problem-solver` govern remediation structure; `rust-implementer` executes approved remediation payloads.
- If any required delegation contract cannot be satisfied, invoke Plan-deviation protocol and halt.

1. Determine execution mode from `implementation-delegation` (`direct` default for legacy plans).
2. If mode is `delegated`, delegate coding-focused Approach steps to `rust-implementer` with full step context, expected outputs, and constraints. The invoking agent still owns decisions, review, and verification.
3. Execute every Approach step as written. Treat `.claude/rules/*.md` as normative; use `.claude/reference/*.md` only as secondary pattern guidance.
4. **No speculative fallback**: if a step cannot be completed as written, follow the Plan-deviation protocol and halt.
5. After each Approach step, run `cargo check --workspace` as a fast fail-check. Fix compile errors before moving to the next step.
6. **Rust quality review** — driven by Section 6 and `rust-review-agent-required`:
   - If YES → after implementation (or sensible midpoint for long runs), invoke `rust-reviewer` on touched Rust files. Pass the `DIGEST_SLICE_<shard_id>` for each touched shard; do not pass full `RULES_INDEX` or `DESIGN_INDEX`.
   - If NO → skip and record the rationale from Section 6 in the Implementation Log.
7. **Security review** — driven by Section 7 and `security-agent-required`:
   - If YES → invoke `security-reviewer` on touched files under `src-tauri/src/crypto/`, `src-tauri/src/auth/`, or `src-tauri/src/storage/`. Pass relevant `DIGEST_SLICE`.
   - If NO → skip. The drift check in item 10 still fires if sensitive paths are touched.
8. **Architecture review** — driven by Section 8 and `architecture-review-agent-required`:
   - If YES → invoke `architecture-reviewer` on touched Rust files. Pass relevant `DIGEST_SLICE`.
   - If NO → skip and record the rationale from Section 8 in the Implementation Log.
9. **Findings remediation loop** — driven by Section 9 and `solution-agent-required`:
   - Consolidate findings from enabled review agents.
   - Invoke `finding-classifier` as a dedicated quality gate with consolidated findings + `PLAN_DIGEST` + `RULES_INDEX` + `DESIGN_INDEX`. Require disposition outputs: `ACTIONABLE_NOW`, `INTENTIONAL_DECISION`, `DEFERRED_BY_PLAN`, `INSUFFICIENT_EVIDENCE`.
   - Build `ACTIONABLE_FINDINGS` from `ACTIONABLE_NOW` only. If empty, continue to Testing.
   - Read `APPROVED_DESIGN_CHALLENGES` from the validated list built in Step 3.15. Do not re-derive.
   - Group actionable findings before solver launch:
     - one isolated solver invocation per CRITICAL/HIGH finding
     - MEDIUM grouped by shard (max 10 per invocation)
     - one LOW batch
   - If `solution-agent-required` is `true`, invoke `problem-solver` per group with scoped files and `DIGEST_SLICE`. Require one of `SOLUTION_PACK`, `NO_ACTIONABLE_FIXES`, or `BLOCKED_SOLUTIONS` per `.claude/agents/problem-solver.md`. Pass `APPROVED_DESIGN_CHALLENGES`.
   - If `BLOCKED_SOLUTIONS` is returned, invoke the Plan-deviation protocol and halt.
   - If `NO_ACTIONABLE_FIXES` is returned, continue.
   - If any `SOLUTION_PACK` item requires a deviation outside `APPROVED_DESIGN_CHALLENGES`, invoke Plan-deviation protocol and halt.
   - In `delegated` mode, invoke `rust-implementer` with `SOLUTION_PACK` and `APPROVED_DESIGN_CHALLENGES`. Require `IMPLEMENTATION_RESULT` per `.claude/agents/rust-implementer.md`. If any item returns `BLOCKED`, invoke Plan-deviation protocol and halt.
   - In `direct` mode, implement fixes directly or delegate selectively; same `SOLUTION_PACK` contract applies if delegating.
   - **Cross-shard consistency pass**: after each remediation cycle, when two or more shards had changed files, invoke `cross-shard-reviewer` once with per-shard finding records + `SHARD_DIGEST_SUMMARY[]`. Use stable remediation-cycle labels (`remediation-cycle-1`, `remediation-cycle-2`, ...). Cross-shard findings are fed back into the next remediation cycle's `finding-classifier` invocation if they are CRITICAL/HIGH.
   - Re-run enabled reviewers on changed files after each remediation cycle.
   - Acceptance thresholds:
     - All Rust HIGH findings must be remediated before completion.
     - All Architecture HIGH findings must be remediated before completion.
     - All Security CRITICAL findings must be remediated before completion.
     - Architecture MEDIUM/LOW and Security WARNING/NOTE findings are recorded in the Implementation Log with rationale when deferred.
   - Max remediation cycles: 8. If required thresholds remain after remediation-cycle-8, invoke Plan-deviation protocol and halt.
   - If `solution-agent-required` is `true`, reviewer-only loops are forbidden: every actionable remediation cycle must invoke both `problem-solver` and a remediation step.

10. **Drift check (always runs)**: compare files actually modified under `src-tauri/src/{crypto,auth,storage}/` against the plan's **Expected sensitive path set**. If the implementation touched any sensitive file the plan did not anticipate, invoke Plan-deviation protocol and halt. Do not silently auto-invoke `security-reviewer` to paper over the scope drift.

### Plan-deviation protocol

If any Approach step cannot be executed as written — or a required governance sync action from Step 3.14 cannot be completed exactly as specified — **stop and do not guess**:

1. Revert or stash any partial work for that step so the repo is in a consistent state.
2. Append a `## Plan Deviation` section to the plan file with:
   - **Step** — which Approach step hit the problem
   - **Expected** — what the plan said to do
   - **Actual** — what's actually true in the repo
   - **Suggested resolution** — one or two options
3. Update `status:` to `blocked`.
4. Halt and report the deviation. Do not proceed with subsequent steps.

### Testing

Read Section 10 and follow its testing decision:
- If "Invoke test-writer agent? YES":
  1. Parse the reason field to understand test focus.
  2. Invoke `test-writer` with the specific focus (mandatory; no substitution).
  3. Run `cargo test` after `test-writer` completes.
  4. Report test results.
- If "Invoke test-writer agent? NO":
  - Rely on tests written during implementation.
  - Run `cargo test` and `cargo clippy -- -D warnings`.
- If decision is unchecked or ambiguous, halt at Step 3's testing-agent gate.

### Sub-phase Implementation Decisions sync (mandatory for sub-phase plans)

1. Locate the sub-phase document path (prefer explicit path in plan body).
2. Ensure the sub-phase doc contains a `## Implementation Decisions` section. Create if missing.
3. Append or update bullets for decisions made during implementation that were previously optional/ambiguous.
4. Keep entries concise: **decision + rationale + any deferred follow-up**.
5. This sync is required before Step 6 status can be set to `implemented`.

### Validation checkpoint

If this is a sub-phase plan: after implementation, read the Validation checkpoint from the sub-phase roadmap. Run the automated tests. Display the manual verification steps and acceptance criteria to the user — do not mark `implemented` if the automated portion fails.

## Step 5 — Verify

1. Run `cargo test` (full workspace). Fix related failures before marking implemented. Record pre-existing unrelated failures in the Implementation Log.
2. Run `cargo clippy --workspace -- -D warnings`. Fix new warnings introduced by this implementation; note pre-existing warnings in the Implementation Log.

## Step 6 — Mark complete and report

1. **Sub-phase decision-sync gate (hard)**: if this is a sub-phase plan, verify the target sub-phase doc has an `## Implementation Decisions` section reflecting this run. If missing or stale, halt and return to Step 4's decision-sync subsection.

2. Update `status:` to `implemented` in the plan file's frontmatter.

3. Append an **Implementation Log** section to the plan file with:
    - **Date** — ISO 8601 datetime
    - **Branch** — the branch recorded in Step 2
    - **Execution mode** — `direct` or `delegated` (plus brief delegation summary)
    - **Agent evidence** — table with `Approach step | Agent | Agent ID | Outcome`
    - **Files changed** — list of modified/created files
    - **Test results** — `cargo test` summary
    - **Clippy results** — clean / warnings introduced / pre-existing noted
    - **Rust review** — `rust-reviewer` findings summary or "N/A"
    - **Architecture review** — `architecture-reviewer` findings summary or "N/A"
    - **Security review** — `security-reviewer` findings summary or "N/A"
    - **Cross-shard review** — number of cross-shard reviewer invocations, any findings or "N/A"
    - **Findings quality gate** — counts by disposition
    - **Design challenge approvals used** — `DC-xxx` IDs used or "None"
    - **Governance sync** — action count, files updated, `/copilot-sync` outcome when applicable
    - **Sub-phase decisions sync** — target doc path + count of decisions added/updated (or "N/A")
    - **Deviations from plan** — small adjustments (large deviations should have halted at Step 4)
    - **Documentation flagged** — verbatim list from the plan's **Documentation impact** section

4. **Do not commit, push, or open a pull request.** Leave the working tree dirty.

5. **Report to the user** using this structure:

**If this is a sub-phase plan**:
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
✓ Design challenge approvals used: [DC list or None]
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

**If this is a full-phase or ad-hoc plan**: report implemented work, branch, agent evidence summary, review summaries, cross-shard review summary, findings-quality-gate summary, design-challenge approvals used, problem-solver summary, test results, clippy results, governance-sync summary, files changed, and the verbatim Documentation impact list.

## Guardrails

- Preserve hard-gate semantics in Step 3 and do not silently downgrade failures.
- Do not bypass required reviewers, classifier, solver, or testing decisions declared by plan gates.
- Do not broaden implementation scope outside approved plan/Approach without triggering Plan-deviation protocol.
- Do not auto-chain `/review-only` and `/implement-review`; this command remains a separate entrypoint.
- Do not commit, push, or open pull requests from this command.
- `cross-shard-reviewer` receives `SHARD_DIGEST_SUMMARY[]` (IDs only) — never full `DIGEST_SLICE` content.
- `APPROVED_DESIGN_CHALLENGES` is set once in Step 3.15 and read in Step 4. Do not re-derive.

## Failure Modes and Halt Conditions

| Condition | Action |
|---|---|
| Plan file cannot be resolved | Halt and report candidate plan files |
| Any Step 3 hard gate fails | Halt before Step 4 |
| Step 16 gatherer artifact build fails | Halt before updating plan status |
| Governance sync action fails verification | Plan-deviation protocol, then halt |
| Required reviewer/solver/test agent decision is ambiguous | Halt and report missing/ambiguous gate decision |
| `BLOCKED_SOLUTIONS` returned during required remediation | Plan-deviation protocol, then halt |
| Required findings thresholds not met after remediation-cycle-8 | Plan-deviation protocol, then halt |
| Sensitive path drift detected outside expected path set | Plan-deviation protocol, then halt |
