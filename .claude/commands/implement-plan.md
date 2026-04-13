Implement the saved plan: $ARGUMENTS

**Implementer-agnostic**: this command is designed to be run by any CLI agent that can read `.claude/` resources (Claude Code, Copilot CLI with alternate models, etc.). It assumes no specific model and no human in the loop beyond the hard gates below. Interactive confirmation is reserved for destructive or ambiguous actions — routine checks pass or fail, they do not prompt.

**Execution contract (hard)**: for Arx Runa plans, code implementation is performed directly by the invoking agent according to the approved plan. Do not require or assume a named implementation agent in plan frontmatter.

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
8. **Testing-agent gate** (hard):
   - Require frontmatter field `test-agent-required` (`true`/`false`). If missing, halt with: "Plan missing `test-agent-required`. Re-run `/plan` or add the field before `/implement-plan`."
   - Parse the Testing Strategy for an explicit "Invoke test-writer agent? YES/NO" decision.
   - If the Testing Strategy decision is missing or ambiguous, halt.
   - If Testing Strategy says YES but `test-agent-required` is not `true`, halt.
   - If Testing Strategy says NO but `test-agent-required` is not `false`, halt.
9. **Governance-sync gate** (hard, pre-implementation):
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
10. Update `status` to `in-progress` in the plan file's frontmatter.

## Step 4 — Implement

Follow the **Approach** section of the plan step by step, in order.

1. Execute every Approach step directly as written, referencing `.claude/reference/rust-patterns.md` and `docs/architecture/design-invariants.md`.
2. **No speculative fallback**: if a step cannot be completed as written, follow the Plan-deviation protocol and halt rather than improvising signatures, schemas, or behavior.
3. After each Approach step, run `cargo check --workspace` as a fast fail-check. If it breaks, fix it before moving to the next step — don't let compile errors accumulate.
4. **Security review** is driven by the plan's **Security implications** section, not by an automatic path trigger. Read that section and act:
   - **If `Invoke security-reviewer agent?` is YES** → after implementation is complete (or at a sensible midpoint for long runs), invoke `security-reviewer` on the touched files under `src-tauri/src/crypto/`, `src-tauri/src/auth/`, or `src-tauri/src/storage/`. Pass the plan's "What the reviewer should check" list as focus. Fix any **CRITICAL** findings before continuing. Record **WARNING** and **NOTE** findings in the Implementation Log but do not block on them.
   - **If `Invoke security-reviewer agent?` is NO** → skip the review. The plan's rationale stands. Record the rationale in the Implementation Log.
   - **Drift check (always runs, regardless of YES/NO)**: compare the set of files actually modified under `src-tauri/src/{crypto,auth,storage}/` against the plan's **Expected sensitive path set**. If the implementation touched any sensitive file that the plan did not anticipate, this is a **Plan Deviation** — the plan under-scoped the security surface. Halt via the Plan-deviation protocol below: stash the unanticipated change, append a `## Plan Deviation` section naming the file(s), set `status: blocked`, and report. Do not silently auto-invoke `security-reviewer` to paper over the scope drift; surfacing the under-scope is the point. The user revises the plan (or the sub-phase) and re-runs.

### Plan-deviation protocol

If any Approach step cannot be executed as written — or a required governance sync action from Step 3.9 cannot be completed exactly as specified — **stop implementing and do not guess**. Signature won't compile, file state is unexpected, a cited dependency is missing, the inlined DDL doesn't match the current schema, a trait signature from the plan turns out to be infeasible, or a required test/review agent cannot be used as mandated are all Plan Deviations. Instead:

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

Read the plan's **Testing Strategy** section:
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
    - **Agent evidence** — table with `Approach step | Agent | Agent ID | Outcome`; include one record per implemented step, plus `test-writer` / `security-reviewer` entries when used
    - **Files changed** — list of modified / created files
    - **Test results** — `cargo test` summary (pass count, any skipped or failing)
    - **Clippy results** — clean / warnings introduced / pre-existing noted
    - **Security review** — agent findings if run, or "N/A" if no sensitive modules touched
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
✓ Agent evidence: [summary]
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
Report what was implemented, branch, agent evidence summary, test results, clippy results, governance-sync summary, files changed, and the verbatim Documentation impact list. Do not cross-reference or audit the doc state.
