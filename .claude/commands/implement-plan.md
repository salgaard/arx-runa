Implement the saved plan: $ARGUMENTS

**Implementer-agnostic**: this command is designed to be run by any CLI agent that can read `.claude/` resources (Claude Code, Copilot CLI with alternate models, etc.). It assumes no specific model and no human in the loop beyond the hard gates below. Interactive confirmation is reserved for destructive or ambiguous actions — routine checks pass or fail, they do not prompt.

**Execution contract (hard)**: for Arx Runa plans, the invoking agent owns implementation end-to-end according to the approved plan. It may delegate coding steps to sub-agents, but must retain orchestration, verification, and final accountability. Do not require or assume a named implementation agent in plan frontmatter.

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

Before touching the plan, verify the working environment is sane. Any failure here halts execution — do not try to "fix" these conditions automatically, they represent work in progress that the user may not want disturbed.

1. **Working directory**: confirm cwd is the repo root (the directory containing `CLAUDE.md`). If not, halt with the actual and expected paths.
2. **Git state**: run `git status --porcelain` and `git branch --show-current`. Record the branch for the Implementation Log. If the working tree is not clean, display the dirty files and halt — the user must commit, stash, or explicitly approve proceeding by re-running with a `--force-dirty` argument (do **not** prompt; this is a hard gate on unattended runs).
3. **Baseline build**: run `cargo check --workspace` from the repo root. If it fails, halt and report the errors — the baseline must be green so that any later failure can be attributed to the implementation, not pre-existing breakage.

## Step 3 — Validate the plan and enforce gates

1. Read the plan file and parse its YAML frontmatter.
2. **Status gate** (hard):
   - `status: approved` → proceed.
   - `status: draft` → halt with: "Plan is still a draft. Review it and change `status:` to `approved` before running `/implement-plan`."
   - `status: blocked` → halt with: "Plan is blocked by unresolved Design Concerns. Resolve them (update the sub-phase and re-run `/plan`, or explicitly demote concerns to non-blocking) before running `/implement-plan`." Also display the plan's **Design Concerns / Open Questions** section so the user can see what's outstanding.
   - `status: in-progress` → halt with: "Plan is already in progress. Either resume manually or reset `status:` to `approved` to re-run."
   - `status: implemented` or `superseded` → halt with: "Plan is already `<status>`. Re-running would overwrite the Implementation Log. If intentional, reset `status:` to `approved` first."
3. **Blocking-concerns gate** (hard, belt-and-braces): scan the plan's **Design Concerns / Open Questions** section for any entry whose Classification is **Blocking**. If found, halt — regardless of `status:` — and display each one. A blocked concern with a non-blocked status usually means `/plan` was edited by hand.
4. **Display Handoff Notes**: output the plan's **Handoff Notes for Implementer** section verbatim. This is the implementer's orientation — branch expectations, order of operations, traps, gated tests. Read it before going further.
5. **Verify Assumptions**: walk the plan's **Assumptions** section. For each assumption, check it still holds against the current repo state (file exists, dependency present in `Cargo.toml`, trait/struct available at the cited path, feature flag enabled, etc.). If any assumption is now false, halt with:
   - Which assumption failed.
   - What the current state actually is.
   - Suggested resolution: re-run `/plan` to refresh, or update the assumption in the plan if the new state is acceptable.
6. **Sub-phase detection**: check if `sub-phase` field exists in frontmatter (e.g., `sub-phase: "4.1"`).
   - If present: this is a sub-phase plan; proceed with sub-phase-aware implementation.
   - If absent: this is a full-phase or ad-hoc plan; use standard implementation flow.
7. **If sub-phase plan**:
   - Read the sub-phase roadmap from `sub-phase-roadmap` frontmatter field.
   - Extract the specific sub-phase section (e.g., "Phase 4.1: ...").
   - Note the dependencies (e.g., "Depends on: Phase 4.1").
   - Check if prerequisite sub-phases are complete: look for plan files matching the prerequisite pattern (e.g., `phase-4-1-*.md`) with `status: implemented`. If prerequisite is missing or not implemented, halt with the list of missing prerequisites.
8. **Implementation-mode gate**:
   - Read frontmatter field `implementation-delegation`.
   - If missing (legacy plan), default to `direct` and record a migration note in the Implementation Log.
   - Allowed values: `direct`, `delegated`. Any other value is a hard failure.
   - If `delegated`, verify the plan includes Section 14 (**Implementation execution mode**) with explicit delegation boundaries.
9. **Rust-review gate** (hard):
   - Prefer frontmatter field `rust-review-agent-required` (`true`/`false`).
   - If the field is missing (legacy plan), infer from Section 6 (**Rust quality review implications**) and emit a migration warning in the Implementation Log.
   - Parse Section 6 for an explicit "Invoke rust-reviewer agent? YES/NO" decision.
   - If the Section 6 decision is missing or ambiguous, halt.
   - If Section 6 says YES but `rust-review-agent-required` is not `true`, halt.
   - If Section 6 says NO but `rust-review-agent-required` is not `false`, halt.
10. **Security-review gate** (hard):
   - Prefer frontmatter field `security-agent-required` (`true`/`false`).
   - If the field is missing (legacy plan), infer from Section 7 (**Security implications**) and emit a migration warning in the Implementation Log.
   - Parse Section 7 for an explicit "Invoke security-reviewer agent? YES/NO" decision.
   - If the Section 7 decision is missing or ambiguous, halt.
   - If Section 7 says YES but `security-agent-required` is not `true`, halt.
   - If Section 7 says NO but `security-agent-required` is not `false`, halt.
11. **Architecture-review gate** (hard):
   - Prefer frontmatter field `architecture-review-agent-required` (`true`/`false`).
   - If the field is missing:
     - if Section 8 (**Architecture review implications**) exists, infer from its explicit YES/NO decision and emit a migration warning in the Implementation Log;
     - otherwise (legacy plan), infer from `rust-review-agent-required` and emit a migration warning in the Implementation Log.
   - If Section 8 exists, parse it for an explicit "Invoke architecture-reviewer agent? YES/NO" decision.
   - If Section 8 exists and the decision is missing or ambiguous, halt.
   - If Section 8 says YES but `architecture-review-agent-required` is not `true`, halt.
   - If Section 8 says NO but `architecture-review-agent-required` is not `false`, halt.
   - Rust-touching enforcement:
     - if `rust-review-agent-required` is `true`, `architecture-review-agent-required` must be `true`;
     - if Section 6 **Expected Rust change surface** is not "None anticipated", `architecture-review-agent-required` must be `true`.
12. **Solution-synthesis gate** (hard):
   - Prefer frontmatter field `solution-agent-required` (`true`/`false`).
   - If the field is missing (legacy plan), infer from Section 9 (**Findings-to-fix synthesis implications**) and emit a migration warning in the Implementation Log.
   - Parse Section 9 for an explicit "Invoke problem-solver agent? YES/NO" decision.
   - If the Section 9 decision is missing or ambiguous, halt.
   - If Section 9 says YES but `solution-agent-required` is not `true`, halt.
   - If Section 9 says NO but `solution-agent-required` is not `false`, halt.
   - Coupling enforcement:
     - If `rust-review-agent-required`, `security-agent-required`, or `architecture-review-agent-required` is `true`, then `solution-agent-required` must be `true` **unless** Section 9 includes:
       1. a non-empty `Solver override justification:` line, and
       2. an explicit handoff statement that reviewer findings are passed directly to `rust-implementer`.
     - If these override requirements are missing, halt with: "Reviewer-enabled plans must enable `problem-solver`, or include an explicit Section 9 solver override justification plus direct handoff statement."
13. **Testing-agent gate** (hard):
   - Require frontmatter field `test-agent-required` (`true`/`false`). If missing, halt with: "Plan missing `test-agent-required`. Re-run `/plan` or add the field before `/implement-plan`."
   - Parse Section 10 (**Execution and testing strategy**) for an explicit "Invoke test-writer agent? YES/NO" decision.
   - If the testing decision is missing or ambiguous, halt.
   - If testing says YES but `test-agent-required` is not `true`, halt.
   - If testing says NO but `test-agent-required` is not `false`, halt.
14. **Governance-sync gate** (hard, pre-implementation):
   - Require frontmatter field `governance-sync-required` (`true`/`false`). If missing, halt with: "Plan missing `governance-sync-required`. Re-run `/plan` or add the field before `/implement-plan`."
   - Parse the plan's **Governance sync actions (pre-implementation)** section.
   - Consistency checks:
     - If `governance-sync-required: true` but the section is missing or says "None", halt.
     - If `governance-sync-required: false` but the section lists one or more actions, halt.
   - If actions are listed:
     1. Execute them in order **before** Step 4 implementation work.
     2. Apply `.claude/rules/*.md`, `.claude/reference/*.md`, and `.claude/agents/*.md` updates exactly as declared in the action list.
     3. If any action touches `.claude/rules/*.md`, run `/copilot-sync` once after rule edits so `.github/instructions/*.instructions.md` is regenerated from the updated rules.
     4. Do not manually edit mirrored `.github/instructions/*.instructions.md` files when a corresponding `.claude/rules/*.md` source exists, unless the plan explicitly marks an exception and rationale.
     5. Re-read each target file and confirm the declared update is present.
    - If any governance sync action cannot be completed or verified, invoke the Plan-deviation protocol and halt before Step 4.
15. **Design-challenge approvals gate** (hard):
   - Prefer frontmatter fields:
     - `design-challenge-approvals-required` (`true`/`false`)
     - `approved-design-challenges` (list of `DC-xxx` IDs)
   - If fields are missing (legacy plan), default to:
     - `design-challenge-approvals-required: false`
     - `approved-design-challenges: []`
     and emit a migration warning in the Implementation Log.
   - Parse Section 13 (**Design challenge approvals (pre-implementation)**).
   - Consistency checks:
     - If `design-challenge-approvals-required: true` but Section 13 is missing or "None", halt.
     - If `design-challenge-approvals-required: false` but Section 13 lists approvals, halt.
     - If `design-challenge-approvals-required: true` and `approved-design-challenges` is empty, halt.
     - Every ID in `approved-design-challenges` must exist in Section 13 with `Approval status: Approved`; otherwise halt.
16. Update `status` to `in-progress` in the plan file's frontmatter.

## Step 4 — Implement

Follow the **Approach** section of the plan step by step, in order.

1. Determine execution mode from `implementation-delegation` (`direct` default for legacy plans).
2. If mode is `delegated`, delegate coding-focused Approach steps to `rust-implementer` with full step context, expected outputs, and constraints. The invoking agent still owns decisions, review, and verification.
3. Execute every Approach step as written. Treat `.claude/rules/*.md` as normative; use `.claude/reference/*.md` only as secondary pattern guidance.
4. **No speculative fallback**: if a step cannot be completed as written, follow the Plan-deviation protocol and halt rather than improvising signatures, schemas, or behavior.
5. After each Approach step, run `cargo check --workspace` as a fast fail-check. If it breaks, fix it before moving to the next step — don't let compile errors accumulate.
6. **Rust quality review** is driven by the plan's Section 6 (**Rust quality review implications**) and `rust-review-agent-required`.
   - **If `Invoke rust-reviewer agent?` is YES** → after implementation is complete (or at a sensible midpoint for long runs), invoke `rust-reviewer` on touched Rust files and pass the section's "What the reviewer should check" list as focus.
   - **If `Invoke rust-reviewer agent?` is NO** → skip the review and record the rationale from Section 6 in the Implementation Log.
7. **Security review** is driven by Section 7 (**Security implications**) and `security-agent-required`, not by an automatic path trigger. Read that section and act:
   - **If `Invoke security-reviewer agent?` is YES** → after implementation is complete (or at a sensible midpoint for long runs), invoke `security-reviewer` on the touched files under `src-tauri/src/crypto/`, `src-tauri/src/auth/`, or `src-tauri/src/storage/`. Pass the plan's "What the reviewer should check" list as focus.
   - **If `Invoke security-reviewer agent?` is NO** → skip the review. The plan's rationale stands. Record the rationale in the Implementation Log.
8. **Architecture review** is driven by Section 8 (**Architecture review implications**) and `architecture-review-agent-required`.
   - **If `Invoke architecture-reviewer agent?` is YES** → after implementation is complete (or at a sensible midpoint for long runs), invoke `architecture-reviewer` on touched Rust files and pass the section's "What the reviewer should check" list as focus.
   - **If `Invoke architecture-reviewer agent?` is NO** → skip the review and record the rationale from Section 8 in the Implementation Log.
9. **Findings remediation loop** is driven by Section 9 (**Findings-to-fix synthesis implications**) and `solution-agent-required`.
   - Consolidate findings from enabled review agents (rust/architecture/security).
   - Run a lightweight findings quality gate before remediation:
     1. **Evidence check** — every finding needs location anchors + rule/design citation.
     2. **False-positive check** — remove findings already explained by intentional or deferred plan context.
     3. **Actionability check** — keep only findings with a concrete in-scope fix path.
     4. **Confidence check** — label each finding `HIGH|MEDIUM|LOW` confidence based on evidence quality and recurrence.
   - Tag each finding disposition as `ACTIONABLE_NOW|INTENTIONAL_DECISION|DEFERRED_BY_PLAN|INSUFFICIENT_EVIDENCE`.
   - Build `ACTIONABLE_FINDINGS` from `ACTIONABLE_NOW` only.
   - If no `ACTIONABLE_FINDINGS` exist, continue to Testing.
   - Build `APPROVED_DESIGN_CHALLENGES` from frontmatter `approved-design-challenges` and Section 13 approval entries (ID + allowed scope + guardrails).
   - If `solution-agent-required` is `true`, invoke `problem-solver` with `ACTIONABLE_FINDINGS`, touched files, and current remediation round (`round-1`, `round-2`, ...). Require one of:
       - `IMPLEMENTATION_PACK`
       - `NO_ACTIONABLE_FIXES`
       - `BLOCKED_SOLUTIONS`
       - Pass `APPROVED_DESIGN_CHALLENGES` to `problem-solver` and require explicit challenge metadata per item.
   - If `BLOCKED_SOLUTIONS` is returned, invoke the Plan-deviation protocol and halt.
   - If `NO_ACTIONABLE_FIXES` is returned, continue.
   - If any `IMPLEMENTATION_PACK` item carries `Design challenge.status=PROPOSED`, or `status=APPROVED` with a `challenge_id` not present in `APPROVED_DESIGN_CHALLENGES`, invoke the Plan-deviation protocol and halt.
   - If `solution-agent-required` is `false` and direct `ACTIONABLE_FINDINGS` include unresolved/unapproved `design_challenge` entries, invoke the Plan-deviation protocol and halt.
   - If remediation is needed:
      - In `delegated` execution mode, invoke `rust-implementer` with the `IMPLEMENTATION_PACK` (or direct findings when `solution-agent-required` is `false`) and require an implementation result mapping.
      - In `direct` execution mode, implement fixes directly or delegate selectively to `rust-implementer`; if delegating, pass through the same solver output contract.
      - Always pass `APPROVED_DESIGN_CHALLENGES` to `rust-implementer`.
      - If `rust-implementer` returns any `ITEM ... — BLOCKED`, invoke the Plan-deviation protocol and halt.
   - Re-run enabled reviewers on changed files after each remediation round.
   - Acceptance thresholds:
      - All Rust `HIGH` findings must be remediated before completion.
      - All Architecture `HIGH` findings must be remediated before completion.
      - All Security `CRITICAL` findings must be remediated before completion.
      - Architecture `MEDIUM`/`LOW` findings are recorded in the Implementation Log with rationale when deferred.
      - Security `WARNING` and `NOTE` findings are recorded in the Implementation Log with rationale when deferred.
   - Max remediation rounds: 5. If required Rust/Architecture `HIGH` or Security `CRITICAL` findings remain after round 5, invoke the Plan-deviation protocol and halt.
   - If `solution-agent-required` is `true`, reviewer-only loops are forbidden: every actionable remediation round must invoke both `problem-solver` and a remediation step.
10. **Drift check (always runs, regardless of YES/NO reviews)**: compare the set of files actually modified under `src-tauri/src/{crypto,auth,storage}/` against the plan's **Expected sensitive path set**. If the implementation touched any sensitive file that the plan did not anticipate, this is a **Plan Deviation** — the plan under-scoped the security surface. Halt via the Plan-deviation protocol below: stash the unanticipated change, append a `## Plan Deviation` section naming the file(s), set `status: blocked`, and report. Do not silently auto-invoke `security-reviewer` to paper over the scope drift; surfacing the under-scope is the point. The user revises the plan (or the sub-phase) and re-runs.

### Plan-deviation protocol

If any Approach step cannot be executed as written — or a required governance sync action from Step 3.14 cannot be completed exactly as specified — **stop implementing and do not guess**. Signature won't compile, file state is unexpected, a cited dependency is missing, the inlined DDL doesn't match the current schema, a trait signature from the plan turns out to be infeasible, or a required test/review/solution agent cannot be used as mandated are all Plan Deviations. Instead:

1. Revert or stash any partial work for that step so the repo is in a consistent state.
2. Append a `## Plan Deviation` section to the plan file with:
   - **Step** — which Approach step hit the problem
   - **Expected** — what the plan said to do
   - **Actual** — what's actually true in the repo
   - **Suggested resolution** — one or two options (update the sub-phase, revise the plan, change the approach)
3. Update `status:` to `blocked`.
4. Halt. Report the deviation to the user and exit. Do not proceed with subsequent steps.

A plan deviation is not a failure — it means the plan was wrong, and the correct action is to surface it rather than paper over it.

### Testing

Read the plan's Section 10 (**Execution and testing strategy**) and follow its testing decision:
- If "Invoke test-writer agent?" is checked **YES**:
  1. Parse the reason field to understand test focus (adversarial, property-based, coverage).
  2. Invoke the `test-writer` agent with the specific focus (mandatory; no substitution):
      - For adversarial tests: `/test adversarial` or direct `test-writer` invocation with the relevant module paths.
      - For property-based tests: `test-writer` with the proptest requirement.
      - For coverage gaps: `/test coverage` first, then `test-writer` if below target.
  3. Run `cargo test` after `test-writer` completes.
  4. Report test results and any new failures.
- If "Invoke test-writer agent?" is checked **NO**:
  - Do not invoke `test-writer`.
  - Rely on tests written during implementation.
  - Proceed to `cargo test` and `cargo clippy -- -D warnings` verification.
- If the decision is unchecked or ambiguous, halt at Step 3's testing-agent gate.

### Sub-phase Implementation Decisions sync (mandatory for sub-phase plans)

If `sub-phase` is present in frontmatter, `/implement-plan` must update the corresponding sub-phase document with concrete implementation choices made during Step 4.

1. Locate the sub-phase document path:
   - Prefer an explicit path in the plan body (e.g., `Sub-phase doc: docs/architecture/designs/.../sub-phases/2.1-....md`).
   - If ambiguous or missing, halt and report the path-resolution failure (do not guess among multiple candidates).
2. Ensure the sub-phase doc contains a `## Implementation Decisions` section.
   - Create it if missing.
3. Append or update bullets for decisions that were actually made during implementation and were previously optional/ambiguous in the sub-phase (crate selection, limits/caps, platform filtering strategy, stub scope, etc.).
4. Keep entries concise and factual: **decision + rationale + any deferred follow-up**.
5. This sync is required before Step 6 status can be set to `implemented`.

### Validation checkpoint

**If this is a sub-phase plan** (detected in Step 3):
- After implementation, read the Validation checkpoint from the sub-phase roadmap.
- Run the automated tests listed in the checkpoint.
- Display the manual verification steps and acceptance criteria to the user as part of the final report — do not mark the plan `implemented` if the automated portion fails.

## Step 5 — Verify

1. Run `cargo test` (full workspace). If any tests fail, determine whether they're related to the implementation or pre-existing:
   - Related → fix before marking the plan implemented.
   - Pre-existing and unrelated → record in the Implementation Log and continue. Do not touch them as part of this plan.
2. Run `cargo clippy --workspace -- -D warnings`. Fix any warnings introduced by this implementation; leave pre-existing warnings alone and note them in the Implementation Log.

## Step 6 — Mark complete and report

1. **Sub-phase decision-sync gate (hard)**:
   - If this is a sub-phase plan, verify the target sub-phase doc has an `## Implementation Decisions` section reflecting this implementation run.
   - If missing or stale, halt and return to Step 4's decision-sync subsection before marking complete.

2. Update `status:` to `implemented` in the plan file's frontmatter.

3. Append an **Implementation Log** section to the plan file with:
    - **Date** — ISO 8601 datetime
    - **Branch** — the branch recorded in Step 2
    - **Execution mode** — `direct` or `delegated` (plus brief delegation summary)
    - **Agent evidence** — table with `Approach step | Agent | Agent ID | Outcome`; include one record per implemented step, plus `rust-reviewer` / `architecture-reviewer` / `security-reviewer` / `problem-solver` / `rust-implementer` / `test-writer` entries when used
    - **Files changed** — list of modified / created files
    - **Test results** — `cargo test` summary (pass count, any skipped or failing)
    - **Clippy results** — clean / warnings introduced / pre-existing noted
    - **Rust review** — `rust-reviewer` findings if run, or "N/A" if skipped
    - **Architecture review** — `architecture-reviewer` findings if run, or "N/A" if skipped
    - **Security review** — `security-reviewer` findings if run, or "N/A" if no sensitive modules touched
    - **Findings quality gate** — counts by disposition (`ACTIONABLE_NOW`, `INTENTIONAL_DECISION`, `DEFERRED_BY_PLAN`, `INSUFFICIENT_EVIDENCE`)
    - **Design challenge approvals used** — list `DC-xxx` IDs used in implemented items, or "None"
    - **Governance sync** — action count, files updated, `/copilot-sync` outcome when applicable
    - **Sub-phase decisions sync** — target doc path + count of Implementation Decisions added/updated (or "N/A" for non-sub-phase plans)
    - **Deviations from plan** — any small adjustments made (large deviations should have halted at Step 4's deviation protocol)
    - **Documentation flagged** — verbatim list from the plan's **Documentation impact** section (do **not** cross-reference roadmap docs, diagrams, or ADRs here — that's the job of a separate documentation pass)

4. **Do not commit, push, or open a pull request.** Leave the working tree dirty. The user inspects the diff and decides what to commit. If the CLI has autonomous commit behaviour, it must be suppressed here.

5. **Report to the user**. Use this structure:

**If this is a sub-phase plan**:
```
✓ Phase [X.Y] implementation complete — status: implemented
✓ Branch: [branch]
✓ Execution mode: [direct|delegated]
✓ Agent evidence: [summary]
✓ Rust review: [N/A or findings summary]
✓ Architecture review: [N/A or findings summary]
✓ Security review: [N/A or findings summary]
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

**If this is a full-phase or ad-hoc plan**:
Report what was implemented, branch, agent evidence summary, rust-review summary, architecture-review summary, security-review summary, findings-quality-gate summary, design-challenge approvals used summary, problem-solver summary, test results, clippy results, governance-sync summary, files changed, and the verbatim Documentation impact list. Do not cross-reference or audit the doc state.
