Plan the implementation of: $ARGUMENTS

**Implementer context**: plans produced by this command are typically handed off to Copilot Codex (or another agent with no conversation context) for implementation. Write the plan as a self-contained artefact: inline trait signatures, error enums, and DDL verbatim rather than pointing to them; use absolute file paths; do not assume the reader can infer intent from prior discussion.

**Execution contract (hard)**: plans produced here must be executable by `/implement-plan` without requiring any specific implementation agent. Keep execution guidance explicit, but do not couple the plan to a named agent.

## Step 1 — Detect roadmap phase and sub-phase

Check whether $ARGUMENTS references a roadmap phase or sub-phase. Match patterns like:
- **Full phase**: "Phase 1", "phase 1", "Phase 01", "phase-1", bare number (0–8)
- **Sub-phase**: "4.1", "phase-4.1", "Phase 4.1", "4.2", "phase 6.3"

**If a sub-phase is matched** (e.g., "4.1"):
1. Extract the phase number (4) and sub-phase identifier (1)
2. Look for a sub-phase roadmap in the design's `sub-phases/roadmap.md` file (e.g., `docs/architecture/designs/cloud-synchronisation/sub-phases/roadmap.md`)
3. If found:
   - Read the sub-roadmap file
   - Extract the sub-phase section matching the identifier (e.g., "Phase 4.1: ...")
   - Use the sub-phase's Design sections, Deliverables, Dependencies, and Validation checkpoint to populate the plan
   - The plan filename will be `phase-N-S-kebab-case.md` (e.g., `phase-4-1-cloud-transport.md`)
4. If no sub-roadmap found: warn the user and fall back to full phase planning

**If a full phase is matched**:
1. Read `docs/roadmap.md`
2. Extract the matching phase block: Objective, Depends on, Deliverables,
   and Parallelisable with sections
3. Check for a sub-phase roadmap at `docs/architecture/designs/<design-name>/sub-phases/roadmap.md`:
   - If found: notify the user that a sub-phase roadmap exists and suggest using `/plan <N>.<subphase>` for focused planning, or proceed with full-phase plan
   - If not found: proceed with full-phase planning
4. Find any rows in the Pending Architectural Decisions table that map to
   this phase
5. Use this extracted content to pre-populate the Context section below —
   do not ask the user to re-describe what is already in the roadmap

If no phase or sub-phase is matched, treat as an ad-hoc plan with no roadmap context.

## Step 1.5 — Detect design document (or sub-phase design sections)

**If planning a sub-phase** (from Step 1):
- The sub-phase roadmap already references specific design sections (e.g., "lines 47-135 of cloud-synchronisation.md")
- Read the parent design document and extract only the referenced sections
- Use those sections as the primary input for the Approach section
- Add both `design-document:` (full design path) and `sub-phase-roadmap:` (roadmap path) to the frontmatter

**If planning a full phase:**
Check whether a design document exists for this topic:

- **Phase-linked**: look for a `**Design document**:` line in the roadmap phase block
  extracted in Step 1, then verify the file exists at the linked path
- **Standalone**: search `docs/architecture/designs/` for a document whose title or
  Goals section matches the planning topic

**If a design document is found:**
1. Read it in full
2. It becomes the primary input for the Approach section — implementation steps
   should map directly to the design's trait definitions, schemas, wire formats,
   and flows
3. Add a `design-document:` field to the plan's YAML frontmatter with the relative path

**If no design document is found:**
- If the topic corresponds to a phase that has designs in the roadmap: note in the
  Context section that no formal design document was found — the user may want to
  run `/design <topic>` first
- For phases or topics without designs (Phase 0, Phase 1, standalone topics):
  proceed without one; note "No design document" in the Context section

## Step 1.75 — Critical review of the source spec

Before structuring the plan, read the sub-phase (or design document) **adversarially**. Do not treat it as ground truth. Check for:

1. **Invariant conflicts** — does anything in the sub-phase contradict `docs/architecture/design-invariants.md`?
2. **Contract conflicts** — does it contradict the parent design's `## Contract Surface` section? If a trait signature, error type, or data shape differs, the sub-phase is wrong by default (the Contract Surface is canonical per `CLAUDE.md`).
3. **Under-specified failure modes** — for every trait method, can you answer: what cancels it, what happens on partial failure, what happens under concurrent access, what happens at shutdown? If not, that is a gap.
4. **Missing edge cases in the test list** — do the enumerated tests cover the deliverables' implied failure modes, or only the happy path?
5. **Infeasible or handwaved APIs** — signatures that can't be implemented as stated (e.g. non-dyn-safe traits claimed as dyn, lifetimes that won't compose, async in a sync trait).
6. **Security review self-assessments** — if the sub-phase claims "Security Review: Not required", verify independently. Touching `src-tauri/src/crypto/`, `src-tauri/src/auth/`, or `src-tauri/src/storage/` usually warrants review even for "just plumbing" changes.
7. **Gaps the implementer will hit** — anything Codex would have to guess at (default values, file locations, config keys, error message wording) that isn't stated.

Record every finding. These become the plan's **Design Concerns / Open Questions** section. Classify each as:
- **Blocking** — the sub-phase must be updated before implementation can proceed.
- **Non-blocking** — the plan can proceed with an explicit assumption, recorded in the **Assumptions** section.

If there are any blocking concerns, the plan's status is `blocked` (not `draft`) and the recommended next step is to revise the sub-phase via `/design` or manual edit — **not** `/implement-plan`.

## Step 1.8 — Governance drift review of operational guidance

Before structuring the final plan, review planned behavior and contract changes against:
- `.claude/rules/*.md`
- `.github/instructions/*.instructions.md` (rule mirrors)
- `.claude/reference/*.md`
- `.claude/agents/*.md`

Check for:
1. **Contradictions** — guidance now conflicts with the planned implementation behavior.
2. **Stale or missing guardrails** — rules/checklists/examples omit newly required constraints.
3. **Outdated execution guidance** — references/agent prompts would steer implementers to obsolete behavior.
4. **Rule-mirror drift** — `.claude/rules/*.md` and `.github/instructions/*.instructions.md` are out of sync.

Classify each finding as:
- **Blocking** — requires a design/product decision or ambiguous rewrite that cannot be safely automated.
- **Non-blocking** — deterministic file updates that can be executed automatically before implementation starts.

Handling requirements:
- Record every finding in **Design Concerns / Open Questions** with file path(s) and impact.
- Every non-blocking governance finding must also produce an action in **Governance sync actions (pre-implementation)** with exact target files and edits.
- When a finding touches `.claude/rules/*.md`, treat the rule file as source-of-truth and note that `/implement-plan` will run `/copilot-sync` before coding to regenerate `.github/instructions/*.instructions.md`.
- If any blocking governance finding remains unresolved, set plan status to `blocked`.

## Step 2 — Generate the plan

Structure the plan as follows:

1. **Goal** — what are we building or changing, in one sentence
2. **Context** — what exists today, what constraints apply
   - If roadmap-linked: include the phase objective, dependencies,
     deliverables list, and any pending architectural decisions from the
     roadmap
   - **If sub-phase**: include dependencies from the sub-roadmap (e.g., "Depends on Phase 4.1"), estimated scope, and any implementation notes
3. **Design Concerns / Open Questions** — findings from Steps 1.75 and 1.8. Each entry:
    - **Concern** — one-line summary of the issue
    - **Source** — where in the sub-phase / design it appears (line numbers or section)
    - **Impact** — what breaks or gets guessed if left unresolved
    - **Classification** — Blocking or Non-blocking
    - **Resolution** — for blocking: what needs to change in the sub-phase. For non-blocking: the explicit assumption the plan will make (also recorded below).
    - **Documentation sync required on implementation** — if the resolution deviates from canonical docs, list exact `docs/architecture/designs/**` files/sections that must be updated once implemented.
    If no concerns were found, state "None — sub-phase reviewed, no gaps identified." Do not leave this section out.
4. **Assumptions** — every non-obvious fact the plan takes for granted but which is not stated in the sub-phase (defaults, file locations, config keys, error wording, ordering). If the assumption is wrong, the implementation is wrong — so list them explicitly so the user can correct them before handoff.
5. **Approach** — step-by-step implementation plan with absolute file paths.
   - Each step should inline the relevant trait signatures, error enum variants, struct fields, and DDL **verbatim** from the sub-phase / design. Do not write "implement the `KeySource` trait as defined in the sub-phase" — write the signature out. This forces the planner to notice errors and gives Codex a self-contained spec.
   - If a design document was found in Step 1.5: each step should map to a specific
     section of the design (trait definitions, schema, flows). The design's Contract Surface is ground truth — if the sub-phase diverges, flag it in Design Concerns and use the Contract Surface.
   - **If sub-phase**: use the Deliverables list from the sub-roadmap as the primary structure; each deliverable becomes an implementation step
   - If no design document exists: derive steps from the roadmap deliverables and
     CLAUDE.md architectural constraints
6. **Security implications** — structured decision, three parts:
   a. **Expected sensitive path set** — list the files or directories under `src-tauri/src/crypto/`, `src-tauri/src/auth/`, or `src-tauri/src/storage/` that this plan anticipates touching. If the plan does not anticipate touching any, state "None anticipated." This is an audit anchor: `/implement-plan` will cross-check actual touched files against this list at verify time, and anything *unanticipated* under a sensitive path triggers a Plan Deviation.
   b. **Invoke security-reviewer agent? YES / NO** with rationale — mirrors the test-writer decision in item 7. YES means `/implement-plan` will invoke `security-reviewer` on the touched files regardless of path. NO means the plan takes responsibility for the decision — `/implement-plan` will skip the review, **but** the drift check in (a) still fires if sensitive paths get touched anyway.
   c. **What the reviewer should check** — if YES, list the specific concerns (trait boundaries, zeroization, nonce generation, AAD scope, etc.). If NO, list the specific reasons the review is unnecessary (e.g., "module performs no cryptographic operations; BLAKE3 is used only as a preimage-resistant fingerprint comparator").
   - **If sub-phase**: check the sub-roadmap's Security Review Checkpoints section. If the sub-phase self-asserts "Security Review: Not required", verify independently in Step 1.75 and either confirm (set YES/NO explicitly with rationale) or flag as a Design Concern — do not mirror the self-assessment blindly. Opus's independent decision, recorded here, overrides the sub-phase's self-assessment.
7. **Execution and testing strategy** — what tests are needed and what boundary cases matter
   - Use the template's structured format with checkboxes for test types
   - **Explicitly decide**: check "Invoke test-writer agent? YES/NO" with rationale
   - Mirror this decision in frontmatter as:
     - `test-agent-required: true|false`
     Value must match the prose in this section.
   - **If sub-phase**: include the Validation checkpoint from the sub-roadmap (automated tests, manual verification, acceptance criteria)
   - Include any additional edge-case tests surfaced by the Step 1.75 review
8. **Documentation impact** — which `docs/` files need creating or updating
   after implementation.
   - This section must include documentation updates required by any planned deviations from current canonical design/sub-phase docs.
   - Treat any sub-phase-roadmap `## Documentation Impact` text as advisory only. Never suppress required doc sync updates just because a roadmap says "No documentation updates."
   - If no docs need updates, state why no deviation or new contract surface was introduced.
9. **Governance sync actions (pre-implementation)** — ordered, machine-actionable actions that `/implement-plan` must execute before Step 4 coding.
   - For each action include:
     - **Action ID**
     - **Reason / linked concern**
     - **Target files** (absolute paths)
     - **Required edit** (specific add/remove/replace instruction)
     - **Verification** (what to re-read/check after editing)
   - If any action touches `.claude/rules/*.md`, include "Run `/copilot-sync` after rule edits."
   - If no governance sync is required, state "None."
10. **Handoff Notes for Implementer** — one short paragraph framed for an agent with zero conversation context (typically Copilot Codex). State the working directory, the order of operations, whether the plan is self-contained or requires re-reading the sub-phase, and any traps (platform-specific code paths, feature flags, gated tests). If the plan status is `blocked`, instead write "Do not implement — resolve Design Concerns first."

## Step 3 — Save the plan to disk

After generating the plan:

1. Determine the filename:
   - **Sub-phase**: `phase-N-S-kebab-case-description.md`
     (e.g., `phase-4-1-cloud-transport.md`, where N=phase number, S=sub-phase identifier)
   - Roadmap phase: `phase-N-kebab-case-objective.md`
     (e.g., `phase-1-cryptographic-primitives.md`)
   - Ad-hoc: `YYYY-MM-DD-kebab-case-description.md`
     (e.g., `2026-03-29-refactor-error-handling.md`)
2. Write the plan to project folder `.claude/plans/<filename>` using this frontmatter:

```yaml
---
title: "<plan title>"
created: "<ISO 8601 datetime>"
status: draft  # or "blocked" if Step 1.75 surfaced blocking Design Concerns
roadmap-phase: <number or null>
sub-phase: <"N.S" or null>
design-document: <relative path or null>
sub-phase-roadmap: <relative path or null>
test-agent-required: <true|false>
governance-sync-required: <true|false>
tags: [<relevant tags>]
---
```

`governance-sync-required` must match Section 9:
- `true` when one or more governance sync actions are listed
- `false` when Section 9 is explicitly "None."

Valid `status` values: `draft` (ready for review / implementation), `blocked` (blocking Design Concerns must be resolved first), `approved` (user-approved, ready for `/implement-plan`), `implemented`.

3. Report the saved path to the user. If status is `blocked`, explicitly surface the blocking concerns in the report and recommend revising the sub-phase (via `/design` or manual edit) before proceeding.

Do NOT start implementing. Output the plan and the saved path only.
Wait for approval. When approved, use `/implement-plan <filename>` to execute — **unless** the plan is `blocked`, in which case resolve the Design Concerns first.
