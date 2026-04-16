# `/plan` — Implementation Planning Command

Plan the implementation of: $ARGUMENTS

---

## Design Principles

- **Planning output only.** This command produces a plan artefact and does not execute implementation.
- **Self-contained handoff.** Plans produced by this command are typically handed off to an agent with no conversation context. Write the plan as a self-contained artefact: inline trait signatures, error enums, and DDL verbatim rather than pointing to them; use absolute file paths; do not assume the reader can infer intent from prior discussion.
- **Execution-agnostic plans.** Plans produced here must be executable by `/implement-plan` without requiring any specific implementation agent. Keep execution guidance explicit and keep a valid direct-execution fallback even when delegation is recommended.
- **Contract ownership clarity.** When referring to inter-agent artifacts, treat agent files as producer-schema authority and avoid duplicating full schemas in plans.

## Step 1 — Detect roadmap phase and sub-phase

Check whether $ARGUMENTS references a roadmap phase or sub-phase. Match patterns like:
- **Full phase**: "Phase 1", "phase 1", "Phase 01", "phase-1", bare number (0–8)
- **Sub-phase**: "4.1", "phase-4.1", "Phase 4.1", "4.2", "phase 6.3"

**If a sub-phase is matched** (e.g., "4.1"):
1. Extract the phase number (4) and sub-phase identifier (1)
2. Look for a sub-phase roadmap in the design's `sub-phases/roadmap.md` file
3. If found:
   - Read the sub-roadmap file
   - Extract the sub-phase section matching the identifier
   - Use the sub-phase's Design sections, Deliverables, Dependencies, and Validation checkpoint to populate the plan
   - The plan filename will be `phase-N-S-kebab-case.md`
4. If no sub-roadmap found: warn the user and fall back to full phase planning

**If a full phase is matched**:
1. Read `docs/roadmap.md`
2. Extract the matching phase block: Objective, Depends on, Deliverables, and Parallelisable with sections
3. Check for a sub-phase roadmap at `docs/architecture/designs/<design-name>/sub-phases/roadmap.md`:
   - If found: notify the user that a sub-phase roadmap exists and suggest `/plan <N>.<subphase>` for focused planning
   - If not found: proceed with full-phase planning
4. Find any rows in the Pending Architectural Decisions table that map to this phase
5. Use extracted content to pre-populate the Context section — do not ask the user to re-describe what is already in the roadmap

If no phase or sub-phase is matched, treat as an ad-hoc plan with no roadmap context.

## Step 1.5 — Detect design document (or sub-phase design sections)

**If planning a sub-phase** (from Step 1):
- The sub-phase roadmap already references specific design sections
- Read the parent design document and extract only the referenced sections
- Use those sections as the primary input for the Approach section
- Add both `design-document:` and `sub-phase-roadmap:` to the frontmatter

**If planning a full phase:**
Check whether a design document exists for this topic:
- **Phase-linked**: look for a `**Design document**:` line in the roadmap phase block
- **Standalone**: search `docs/architecture/designs/` for a document matching the planning topic

**If a design document is found:**
1. Read it in full
2. It becomes the primary input for the Approach section
3. Add `design-document:` to the plan's YAML frontmatter

**If no design document is found:**
- If the topic corresponds to a phase that has designs: note that no formal design document was found — the user may want to run `/design <topic>` first
- For phases without designs: proceed without one; note "No design document" in the Context section

## Step 1.75 — Critical review of the source spec

Before structuring the plan, read the sub-phase (or design document) **adversarially**. Do not treat it as ground truth. Check for:

1. **SRP and boundary conflicts (first)** — does any deliverable violate one-concern-per-file or one-reason-to-change? Flag mixed actors in the same file/module.
2. **Invariant conflicts** — does anything contradict `docs/architecture/design-invariants.md`?
3. **Contract conflicts** — does it contradict the parent design's `## Contract Surface` section? The Contract Surface is canonical; a differing sub-phase is wrong by default.
4. **Under-specified failure modes** — for every trait method, can you answer: what cancels it, what happens on partial failure, what happens under concurrent access, what happens at shutdown? If not, that is a gap.
5. **Missing edge cases in the test list** — do the enumerated tests cover implied failure modes, or only the happy path?
6. **Infeasible or handwaved APIs** — signatures that can't be implemented as stated (non-dyn-safe traits claimed as dyn, lifetimes that won't compose, async in a sync trait).
7. **Security review self-assessments** — if the sub-phase claims "Security Review: Not required", verify independently. Touching `src-tauri/src/crypto/`, `src-tauri/src/auth/`, or `src-tauri/src/storage/` usually warrants review.
8. **Gaps the implementer will hit** — anything an agent would have to guess at (default values, file locations, config keys, error message wording) that isn't stated.

Record every finding. These become the plan's **Design Concerns / Open Questions** section. Classify each as:
- **Blocking** — the sub-phase must be updated before implementation can proceed.
- **Non-blocking** — the plan can proceed with an explicit assumption, recorded in **Assumptions**.

If there are any blocking concerns, the plan's status is `blocked` (not `draft`) and the recommended next step is to revise the sub-phase via `/design` or manual edit.

## Step 1.8 — Governance drift review of operational guidance

Before structuring the final plan, review planned behavior and contract changes against:
- `.claude/rules/*.md` (primary, normative guidance)
- `.github/instructions/*.instructions.md` (rule mirrors)
- `.claude/reference/*.md` (secondary guidance; patterns only)
- `.claude/agents/*.md`

Check for:
1. **Contradictions** — guidance conflicts with the planned implementation behavior.
2. **Stale or missing guardrails** — rules/checklists/examples omit newly required constraints.
3. **Outdated execution guidance** — references/agent prompts would steer implementers to obsolete behavior.
4. **Rule-mirror drift** — `.claude/rules/*.md` and `.github/instructions/*.instructions.md` are out of sync.

Classify each finding as:
- **Blocking** — requires a design/product decision that cannot be safely automated.
- **Non-blocking** — deterministic file updates executable before implementation starts.

Handling requirements:
- Record every finding in **Design Concerns / Open Questions** with file path(s) and impact.
- Every non-blocking finding must produce an action in **Governance sync actions (pre-implementation)** with exact target files and edits.
- When a finding touches `.claude/rules/*.md`, note that `/implement-plan` will run `/copilot-sync` before coding.
- If any blocking finding remains unresolved, set plan status to `blocked`.

## Step 1.9 — Compactness and token budget controls (hard)

Apply these limits when generating the plan:

1. Keep the plan concise but self-contained; avoid repeated restatement of the same contract.
2. Use a single `CONTRACT_SNIPPETS` block in **Approach** and assign IDs (`CS-001`, `CS-002`, ...). Inline each unique signature/enum/DDL verbatim **once**, then reference by ID in steps. (See Step 2, Section 5 for required placement and usage.)
3. Include only implementation-relevant evidence; do not quote long design prose when a short citation suffices.

## Step 2 — Generate the plan

Structure the plan as follows:

1. **Goal** — what are we building or changing, in one sentence
2. **Context** — what exists today, what constraints apply
   - If roadmap-linked: include the phase objective, dependencies, deliverables list, and any pending architectural decisions
   - If sub-phase: include sub-roadmap dependencies, estimated scope, and implementation notes
3. **Design Concerns / Open Questions** — findings from Steps 1.75 and 1.8. Each entry:
    - **Concern** — one-line summary
    - **Source** — where in the sub-phase/design it appears
    - **Impact** — what breaks or gets guessed if left unresolved
    - **Classification** — Blocking or Non-blocking
    - **Resolution** — for blocking: what needs to change. For non-blocking: the explicit assumption the plan will make (also recorded in Assumptions).
    - **Documentation sync required on implementation** — list exact `docs/architecture/designs/**` files/sections that must be updated once implemented, if any.
    
    If no concerns were found, state "None — sub-phase reviewed, no gaps identified." Do not leave this section out.
4. **Assumptions** — every non-obvious fact the plan takes for granted but which is not stated in the sub-phase (defaults, file locations, config keys, error wording, ordering). List explicitly so the user can correct them before handoff.
5. **Approach** — step-by-step implementation plan with absolute file paths.
   - Begin with a `CONTRACT_SNIPPETS` subsection (see Step 1.9) — inline each unique trait signature, error enum variant, struct field, and DDL **verbatim once**, assigning IDs `CS-001`, `CS-002`, etc. Reference snippets by ID in each relevant step rather than re-inlining the same contract.
   - Each step should map to a specific design section (trait definitions, schema, flows) when a design document is present. The design's Contract Surface is ground truth.
   - If sub-phase: use the Deliverables list from the sub-roadmap as the primary structure; each deliverable becomes an implementation step.
   - Do not write "implement the `KeySource` trait as defined in the sub-phase" — include the signature in `CONTRACT_SNIPPETS` and reference its snippet ID.
6. **Rust quality review implications** — structured decision, three parts:
   a. **Expected Rust change surface** — files or directories under `src-tauri/**/*.rs` this plan anticipates touching. If none, state "None anticipated."
   b. **Invoke rust-reviewer agent? YES / NO** with rationale.
   c. **What the reviewer should check** — if YES, list concrete focus areas. If NO, list explicit reasons.
   - Reviewer authority order must be explicit: `.claude/rules/*.md` first, canonical design docs second, `.claude/reference/*.md` only for pattern clarification.
7. **Security implications** — structured decision, three parts:
   a. **Expected sensitive path set** — files or directories under `src-tauri/src/crypto/`, `src-tauri/src/auth/`, or `src-tauri/src/storage/` this plan anticipates touching. "None anticipated" if none. This is an audit anchor: `/implement-plan` cross-checks actual touched files against this list, and unanticipated sensitive paths trigger a Plan Deviation.
   b. **Invoke security-reviewer agent? YES / NO** with rationale.
   c. **What the reviewer should check** — if YES, list specific concerns. If NO, list specific reasons.
8. **Architecture review implications** — structured decision, three parts:
   a. **Expected architecture risk surface** — files or directories where SRP, boundary, or dependency-flow risks are most likely.
   b. **Invoke architecture-reviewer agent? YES / NO** with rationale. For Rust-touching plans, YES by default.
   c. **What the reviewer should check** — if YES, list specific checks (one concern per file, module visibility discipline, dependency flow, abstraction debt, `design_challenge` handling). If NO, list explicit reasons.
9. **Findings-to-fix synthesis implications** — structured decision, three parts:
   a. **Invoke problem-solver agent? YES / NO** with rationale. Hard coupling rule: if item 6, 7, or 8 is YES, item 9 must be YES unless Section 9 contains a non-empty `Solver override justification:` with an explicit direct-handoff statement.
   b. **When the solver runs** — define trigger points explicitly.
   c. **Handoff contract to implementer** — choose one:
      - **Solver mode (default)**: require `problem-solver` output contract (`SOLUTION_PACK`, `NO_ACTIONABLE_FIXES`, or `BLOCKED_SOLUTIONS`). Reference `.claude/agents/problem-solver.md` as authoritative schema.
      - **Direct mode (override only)**: state that reviewer findings are passed directly to `rust-implementer` with explicit severity mapping and ordering.
10. **Execution and testing strategy** — what tests are needed and what boundary cases matter.
   - Explicitly decide: "Invoke test-writer agent? YES/NO" with rationale.
   - Mirror in frontmatter as `test-agent-required: true|false`.
   - If sub-phase: include the Validation checkpoint from the sub-roadmap.
   - Include edge-case tests surfaced by Step 1.75.
11. **Documentation impact** — which `docs/` files need creating or updating after implementation.
    - Include documentation updates required by planned deviations from current canonical design/sub-phase docs.
    - If no docs need updates, state why.
12. **Governance sync actions (pre-implementation)** — ordered, machine-actionable actions for `/implement-plan` to execute before coding.
    - For each action include: **Action ID**, **Reason / linked concern**, **Target files** (absolute paths), **Required edit**, **Verification**.
    - If any action touches `.claude/rules/*.md`, include "Run `/copilot-sync` after rule edits."
    - If none, state "None."
13. **Design challenge approvals (pre-implementation)** — explicit approval artifact for allowed rule/design deviations.

    **DC-xxx ID lifecycle:** Challenge entries first appear in reviewer `design_challenge` blocks (from `/review-only` or prior implementation runs) without IDs. When a plan author decides to formally approve a challenge, it is assigned a `DC-xxx` ID here in Section 13. The `DC-xxx` ID is then referenced in the plan's `approved-design-challenges` frontmatter and passed to `problem-solver` and `rust-implementer`. Do not reference DC-xxx IDs in frontmatter that are not listed in Section 13 with `Approval status: Approved`.

    Use deterministic IDs: `DC-001`, `DC-002`, ...
    For each approved item include:
    - **Challenge ID**
    - **Linked finding IDs** (from architecture/rust/security findings or plan concerns)
    - **Challenged constraint** (rule/design anchor)
    - **Approval status** (`Approved` only; anything else is not executable)
    - **Allowed implementation scope** (exact file/module scope)
    - **Guardrails** (what must not change as part of the deviation)
    - **Required post-implementation sync** (rules/design docs to update)
    
    If no approved deviations exist, state "None."
14. **Implementation execution mode** — select one and justify:
    - `direct` — invoking `/implement-plan` agent performs coding steps itself.
    - `delegated` — invoking `/implement-plan` agent delegates coding steps to `rust-implementer` and focuses on orchestration/verification.
    - List delegation boundaries (which Approach steps can be delegated and which must stay with the orchestrator).
    - When `delegated` is chosen, the plan must remain valid for direct execution as fallback. Section 14 must exist and explicitly state delegation boundaries when `implementation-delegation: delegated` is in frontmatter.
15. **Handoff Notes for Implementer** — one short paragraph framed for an agent with zero conversation context. State the working directory, order of operations, whether the plan is self-contained or requires re-reading the sub-phase, and any traps (platform-specific code paths, feature flags, gated tests). If status is `blocked`, write "Do not implement — resolve Design Concerns first."

## Step 3 — Save the plan to disk

After generating the plan:

1. Determine the filename:
   - **Sub-phase**: `phase-N-S-kebab-case-description.md`
   - Roadmap phase: `phase-N-kebab-case-objective.md`
   - Ad-hoc: `YYYY-MM-DD-kebab-case-description.md`
2. Write the plan to `.claude/plans/<filename>` using this frontmatter:

```yaml
---
title: "<plan title>"
created: "<ISO 8601 datetime>"
status: draft  # or "blocked" if Step 1.75 surfaced blocking Design Concerns
roadmap-phase: <number or null>
sub-phase: <"N.S" or null>
design-document: <relative path or null>
sub-phase-roadmap: <relative path or null>
implementation-delegation: <"direct"|"delegated">
rust-review-agent-required: <true|false>
security-agent-required: <true|false>
architecture-review-agent-required: <true|false>
solution-agent-required: <true|false>
test-agent-required: <true|false>
governance-sync-required: <true|false>
design-challenge-approvals-required: <true|false>
approved-design-challenges: [<DC-001, ...>]
tags: [<relevant tags>]
---
```

Frontmatter consistency rules (all hard):

- `implementation-delegation` must match Section 14.
- `rust-review-agent-required` must match Section 6 YES/NO decision.
- `security-agent-required` must match Section 7 YES/NO decision.
- `architecture-review-agent-required` must match Section 8 YES/NO decision.
- If `rust-review-agent-required: true`, then `architecture-review-agent-required` must also be `true`.
- `solution-agent-required` must match Section 9 YES/NO decision.
- If any reviewer agent is `true` and `solution-agent-required` is `false`, Section 9 must include a non-empty `Solver override justification:` and an explicit direct-handoff statement.
- `test-agent-required` must match Section 10 YES/NO decision.
- `governance-sync-required: true` when Section 12 lists actions; `false` when Section 12 is "None."
- `design-challenge-approvals-required: true` when Section 13 lists approved DC-xxx entries; `false` when Section 13 is "None."
- `approved-design-challenges` must list only DC-xxx entries with `Approval status: Approved` in Section 13. Must be non-empty when `design-challenge-approvals-required: true`; must be empty when `false`.
- Valid `status` values: `draft`, `blocked`, `approved`, `implemented`.

3. Report the saved path to the user. If status is `blocked`, explicitly surface the blocking concerns and recommend revising the sub-phase before proceeding.

Do NOT start implementing. Output the plan and the saved path only.
Wait for approval. When approved, use `/implement-plan <filename>` to execute — **unless** the plan is `blocked`, in which case resolve the Design Concerns first.
