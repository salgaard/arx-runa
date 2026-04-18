# `/plan` — Implementation Planning Command

Plan the implementation of: $ARGUMENTS

---

## Design Principles

- **Plan only.** Output a plan artefact; do not begin implementation.
- **Self-contained handoff.** Plans are consumed by agents with zero conversation context. Inline all trait signatures, error enums, and DDL verbatim; use absolute paths.
- **Execution-agnostic.** Plans describe *what* to review and *what* to test — `/implement-plan` decides which agents to invoke.
- **Producer schema authority.** Output schemas live in `.claude/agents/<agent>.md`; reference them, do not duplicate.

---

## Step 1 — Detect phase and locate design document

Match $ARGUMENTS against:

| Pattern | Type | Plan filename |
|---|---|---|
| `N.S`, `phase-N.S`, `Phase N.S` | Sub-phase (primary) | `phase-N-S-kebab-case-description.md` |
| `Phase N`, `phase-N`, bare `0–8` | Full phase | `phase-N-kebab-case-objective.md` |
| No match | Ad-hoc | `YYYY-MM-DD-kebab-case-description.md` |

**Sub-phase:** Read `docs/architecture/designs/<design>/sub-phases/roadmap.md`. Extract: Deliverables, Dependencies, Design sections, Validation checkpoint. Read the parent design doc; extract only referenced sections. Set both `design-document:` and `sub-phase-roadmap:` in frontmatter.

**Full phase:** Read `docs/roadmap.md`; extract Objective, Depends on, Deliverables, Parallelisable with. If a sub-phase roadmap exists, notify the user and suggest `/plan <N>.<S>` for focused planning; otherwise continue. Look for a `**Design document**:` line or a matching doc under `docs/architecture/designs/`. Find any Pending Architectural Decisions rows for this phase.

**Design document:** Read in full — it is the primary input for the Approach section. Set `design-document:` in frontmatter. If absent for a design-bearing phase: note "No formal design document found — consider `/design <topic>` first." If none by design: note "No design document" in Section 2.

---

## Step 2 — Adversarial review of the source spec

Read the sub-phase (or design document) adversarially. Do not treat it as ground truth.

### 2a — Spec integrity

1. **SRP and boundary conflicts** — any deliverable mixing concerns in a single file or module?
2. **Invariant conflicts** — contradictions with `docs/architecture/design-invariants.md`?
3. **Contract conflicts** — contradictions with the parent design's `## Contract Surface`? The Contract Surface is canonical; a differing sub-phase is wrong by default.
4. **Under-specified failure modes** — for every trait method: cancellation, partial failure, concurrent access, shutdown. Unresolved = a gap.
5. **Missing edge cases** — do enumerated tests cover implied failure modes or only the happy path?
6. **Infeasible APIs** — non-dyn-safe traits claimed as `dyn`, lifetimes that won't compose, `async` in sync traits.
7. **Unwarranted security self-assessments** — if the sub-phase claims "Security Review: Not required," verify independently. Touching `src-tauri/src/crypto/`, `src-tauri/src/auth/`, or `src-tauri/src/storage/` always warrants security review.
8. **Implementer gaps** — anything an agent must guess: defaults, file locations, config keys, error wording.

### 2b — Governance drift

Check planned behavior against `.claude/rules/*.md`:

1. **Contradictions** — planned behavior conflicts with existing rules.
2. **Stale guardrails** — rules or checklists omit newly required constraints.

### Classification

Classify every finding from 2a and 2b:

- **Blocking** — the sub-phase must be updated before implementation. Any blocking finding sets plan `status: blocked`.
- **Non-blocking** — proceed with an explicit assumption recorded in **Assumptions** (Section 4). Governance non-blocking findings also produce an action in **Governance sync actions** (Section 8).

---

## Step 3 — Write the plan

**Token budget (hard):** Concise but self-contained. Use `CONTRACT_SNIPPETS` IDs (defined in Section 5) to reference each contract after its first appearance — never re-inline the same signature or enum. Include only implementation-relevant evidence.

---

### 1. Goal
One sentence: what is being built or changed.

---

### 2. Context
What exists today and what constraints apply.
- Sub-phase: sub-roadmap dependencies, estimated scope, implementation notes.
- Full phase: phase objective, dependencies, deliverables list, pending architectural decisions.

---

### 3. Design Concerns / Open Questions
All findings from Step 2. For each:

| Field | Content |
|---|---|
| **Concern** | One-line summary |
| **Source** | Location in the sub-phase or design |
| **Impact** | What breaks or gets guessed if unresolved |
| **Classification** | Blocking or Non-blocking |
| **Resolution** | Blocking: what must change. Non-blocking: the assumption the plan makes (copy to Section 4). |

If no concerns: "None — spec reviewed, no gaps identified."

---

### 4. Assumptions
Every non-obvious fact the plan takes for granted that is not stated in the sub-phase (defaults, file locations, config keys, error wording, ordering). List explicitly so the user can correct before handoff.

---

### 5. Approach
Step-by-step implementation plan with absolute file paths.

Begin with **`CONTRACT_SNIPPETS`**: inline each unique trait signature, error enum variant, struct field, and DDL **verbatim once**, assigning IDs `CS-001`, `CS-002`, etc. Reference snippets by ID in each step. The design's `## Contract Surface` is ground truth.

If sub-phase: use the Deliverables list as the primary structure — each deliverable becomes an implementation step, mapped to a specific design section when a design document is present.

---

### 6. Review focus areas
Guidance for `/implement-plan`. Does not determine which agents run — that is decided by actual changed files.

**6a. Rust change surface** — anticipated files under `src-tauri/**/*.rs`. "None anticipated" if none.

**6b. Security-sensitive paths** — anticipated files under `src-tauri/src/{crypto,auth,storage}/`. "None anticipated" if none. For each listed path, note specific security concerns (key zeroization, nonce handling, IPC sanitization). This is the drift-check anchor for `/implement-plan`: any sensitive file touched that does not appear here triggers a Plan Deviation.

**6c. Architecture risk areas** — files or modules where SRP, boundary, or dependency-flow risks are most likely. Note specific checks: concern isolation, module visibility discipline, dependency direction, abstraction debt.

**6d. Testing requirements** — tests needed and boundary cases that matter. Include the Validation checkpoint from the sub-roadmap if sub-phase. Include edge cases from Step 2.

---

### 7. Documentation impact
Which `docs/architecture` files need updating post-implementation. "None" if none.

---

### 8. Governance sync actions (pre-implementation)
Ordered, machine-actionable actions for `/implement-plan` to execute before coding begins.

Per action: **Action ID** | **Reason / linked concern** | **Target files** (absolute paths) | **Required edit** | **Verification**.

If any action touches `.claude/rules/*.md`: include "Run `/copilot-sync` after rule edits." "None" if no governance sync required.

---

### 9. Handoff Notes for Implementer
One short paragraph for an agent with zero conversation context. State: working directory, order of operations, whether the plan is self-contained or requires re-reading the sub-phase, and any traps (platform-specific paths, feature flags, gated tests). If `status: blocked`: "Do not implement — resolve Design Concerns first."

---

## Step 4 — Save the plan

Write to `.claude/plans/<filename>` with frontmatter:

```yaml
---
title: "<plan title>"
created: "<ISO 8601 datetime>"
status: draft  # or "blocked" if Step 2 surfaced any Blocking concerns
roadmap-phase: <number or null>
sub-phase: <"N.S" or null>
design-document: <relative path or null>
sub-phase-roadmap: <relative path or null>
governance-sync-required: <true|false>
tags: [<relevant tags>]
---
```

**Frontmatter consistency (hard):**
- `governance-sync-required: true` if Section 8 lists actions; `false` when "None."
- Valid `status` values: `draft`, `blocked`, `approved`, `in-progress`, `implemented`.

Report the saved path. If `status: blocked`, surface each blocking concern and recommend resolving the sub-phase before proceeding.
