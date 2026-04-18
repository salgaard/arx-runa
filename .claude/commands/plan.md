# `/plan` — Implementation Planning Command

Plan the implementation of: $ARGUMENTS

---

## Design Principles

- **Planning output only.** Produce a plan artefact; do not begin implementation.
- **Self-contained handoff.** Plans are handed off to agents with zero conversation context. Inline trait signatures, error enums, and DDL verbatim; use absolute paths; do not assume the reader can infer intent from prior discussion.
- **Execution-agnostic.** Plans must be executable by `/implement-plan` without requiring a specific model or agent. The plan describes *what* to review and *what* to test — `/implement-plan` decides which agents to invoke.
- **Producer schema authority.** Agent files own their output schemas — reference `.claude/agents/<agent>.md` rather than duplicating schemas in plans.

---

## Step 1 — Detect phase and locate design document

### Phase detection

Match $ARGUMENTS against:

- **Sub-phase** (primary path): "4.1", "phase-4.1", "Phase 4.1", etc.
- **Full phase**: "Phase 1", "phase-1", "phase 1", bare number 0–8.
- **No match**: treat as ad-hoc with no roadmap context.

**Sub-phase (primary path):**
1. Locate `docs/architecture/designs/<design>/sub-phases/roadmap.md`.
2. Extract the matching sub-phase section: Deliverables, Dependencies, Design sections, Validation checkpoint.
3. Read the parent design document; extract only the referenced sections. These are the primary input for the Approach.
4. Plan filename: `phase-N-S-kebab-case-description.md`.
5. Set both `design-document:` and `sub-phase-roadmap:` in frontmatter.

**Full phase:**
1. Read `docs/roadmap.md`; extract Objective, Depends on, Deliverables, Parallelisable with.
2. If a sub-phase roadmap exists at `docs/architecture/designs/<design>/sub-phases/roadmap.md`, notify the user and suggest `/plan <N>.<S>` for focused planning; otherwise continue.
3. Look for a `**Design document**:` line in the phase block, or a matching doc under `docs/architecture/designs/`. Read it in full if found.
4. Find any Pending Architectural Decisions rows mapping to this phase.

**Ad-hoc:** No roadmap context; proceed directly to Step 2.

### Design document

- **Found:** read in full; it is the primary input for the Approach section. Set `design-document:` in frontmatter.
- **Not found for a design-bearing phase:** note "No formal design document found — consider `/design <topic>` first."
- **None by design:** note "No design document" in the Context section.

---

## Step 2 — Adversarial review of the source spec

Read the sub-phase (or design document) adversarially. Do not treat it as ground truth.

### 2a — Spec integrity

1. **SRP and boundary conflicts** — does any deliverable mix concerns in a single file or module?
2. **Invariant conflicts** — contradictions with `docs/architecture/design-invariants.md`.
3. **Contract conflicts** — contradictions with the parent design's `## Contract Surface`. The Contract Surface is canonical; a differing sub-phase is wrong by default.
4. **Under-specified failure modes** — for every trait method: what cancels it, what happens on partial failure, concurrent access, shutdown? Unresolved = a gap.
5. **Missing edge cases** — do enumerated tests cover implied failure modes or only the happy path?
6. **Infeasible APIs** — signatures that cannot be implemented as stated (non-dyn-safe traits claimed as `dyn`, lifetimes that won't compose, `async` in sync traits).
7. **Unwarranted security self-assessments** — if the sub-phase claims "Security Review: Not required", verify independently. Touching `src-tauri/src/crypto/`, `src-tauri/src/auth/`, or `src-tauri/src/storage/` always warrants security review regardless of what the sub-phase claims.
8. **Implementer gaps** — anything an agent would have to guess: default values, file locations, config keys, error wording.

### 2b — Governance drift

Check planned behavior against `.claude/rules/*.md`:

1. **Contradictions** — planned behavior conflicts with existing rules.
2. **Stale guardrails** — rules or checklists omit newly required constraints.

### Classification and output

Classify every finding from 2a and 2b as:

- **Blocking** — the sub-phase must be updated before implementation can proceed. If any blocking finding exists, set plan `status: blocked`.
- **Non-blocking** — the plan can proceed with an explicit assumption, recorded in **Assumptions** (Section 4).

Governance non-blocking findings also produce an action in **Governance sync actions** (Section 8).

---

## Step 3 — Structure the plan

**Token budget (hard):** Keep the plan concise but self-contained. Use `CONTRACT_SNIPPETS` IDs (defined in Section 5) to reference each contract after its first appearance — never re-inline the same signature or enum. Include only implementation-relevant evidence.

---

**1. Goal** — what is being built or changed, in one sentence.

---

**2. Context** — what exists today and what constraints apply.
- Sub-phase: sub-roadmap dependencies, estimated scope, implementation notes.
- Roadmap phase: phase objective, dependencies, deliverables list, any pending architectural decisions.

---

**3. Design Concerns / Open Questions** — all findings from Step 2. For each:
- **Concern** — one-line summary
- **Source** — location in the sub-phase or design
- **Impact** — what breaks or gets guessed if left unresolved
- **Classification** — Blocking or Non-blocking
- **Resolution** — for Blocking: what must change. For Non-blocking: the explicit assumption the plan makes (also copy to Section 4).

If no concerns: "None — spec reviewed, no gaps identified." Do not omit this section.

---

**4. Assumptions** — every non-obvious fact the plan takes for granted that is not stated in the sub-phase (defaults, file locations, config keys, error wording, ordering). List explicitly so the user can correct before handoff.

---

**5. Approach** — step-by-step implementation plan with absolute file paths.

Begin with a **`CONTRACT_SNIPPETS`** subsection. Inline each unique trait signature, error enum variant, struct field, and DDL **verbatim once**, assigning IDs `CS-001`, `CS-002`, etc. Reference snippets by ID in each implementation step. The design's `## Contract Surface` is ground truth.

If sub-phase: use the Deliverables list as the primary structure; each deliverable becomes an implementation step. Map each step to a specific design section when a design document is present.

---

**6. Review focus areas** — guidance for `/implement-plan`. Does not determine which agents run; that is `implement-plan`'s responsibility based on actual changed files. Four subsections, all required:

**6a. Rust change surface** — list anticipated files under `src-tauri/**/*.rs`. "None anticipated" if none.

**6b. Security-sensitive paths** — list anticipated files under `src-tauri/src/crypto/`, `src-tauri/src/auth/`, or `src-tauri/src/storage/`. "None anticipated" if none.
   - This is the drift-check anchor for `/implement-plan`: any sensitive file touched that does not appear here triggers a Plan Deviation.
   - For each listed path, note specific security concerns (e.g., key zeroization, nonce handling, IPC sanitization).

**6c. Architecture risk areas** — list files or modules where SRP, boundary, or dependency-flow risks are most likely. Note specific checks: concern isolation, module visibility discipline, dependency direction, abstraction debt.

**6d. Testing requirements** — what tests are needed and which boundary cases matter. Include the Validation checkpoint from the sub-roadmap if sub-phase. Include edge cases surfaced by Step 2.

---

**7. Documentation impact** — which `docs/architecture` files need updating after implementation. "None" if none.

---

**8. Governance sync actions (pre-implementation)** — ordered, machine-actionable actions for `/implement-plan` to execute before coding begins.
- For each action: **Action ID**, **Reason / linked concern**, **Target files** (absolute paths), **Required edit**, **Verification**.
- If any action touches `.claude/rules/*.md`: include "Run `/copilot-sync` after rule edits."
- "None" if no governance sync is required.

---

**9. Implementation execution mode** — select one and justify:
- `direct` — the invoking agent performs coding steps itself.
- `delegated` — the invoking agent delegates coding steps to `rust-implementer` and focuses on orchestration and verification.

When `delegated`: list delegation boundaries — which Approach steps may be delegated and which must stay with the orchestrator. The plan must remain valid for direct execution as a fallback.

---

**10. Handoff Notes for Implementer** — one short paragraph for an agent with zero conversation context. State: working directory, order of operations, whether the plan is self-contained or requires re-reading the sub-phase, and any traps (platform-specific paths, feature flags, gated tests). If `status: blocked`: "Do not implement — resolve Design Concerns first."

---

## Step 4 — Save the plan to disk

1. Determine filename:
   - Sub-phase: `phase-N-S-kebab-case-description.md`
   - Full phase: `phase-N-kebab-case-objective.md`
   - Ad-hoc: `YYYY-MM-DD-kebab-case-description.md`

2. Write to `.claude/plans/<filename>` with this frontmatter:

```yaml
---
title: "<plan title>"
created: "<ISO 8601 datetime>"
status: draft  # or "blocked" if Step 2 surfaced any Blocking concerns
roadmap-phase: <number or null>
sub-phase: <"N.S" or null>
design-document: <relative path or null>
sub-phase-roadmap: <relative path or null>
implementation-delegation: <"direct"|"delegated">
governance-sync-required: <true|false>
tags: [<relevant tags>]
---
```

**Frontmatter consistency rules (hard):**
- `implementation-delegation` must match Section 9.
- `governance-sync-required: true` when Section 8 lists actions; `false` when "None."
- Valid `status` values: `draft`, `blocked`, `approved`, `in-progress`, `implemented`.

3. Report the saved path. If `status: blocked`, surface each blocking concern explicitly and recommend resolving the sub-phase before proceeding.

**Do NOT begin implementing.** Output the plan and the saved path only.
