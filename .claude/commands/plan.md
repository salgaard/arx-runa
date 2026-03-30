Recommended model: `/model opusplan`

Plan the implementation of: $ARGUMENTS

## Step 1 — Detect roadmap phase

Check whether $ARGUMENTS references a roadmap phase. Match patterns like:
"Phase 1", "phase 1", "Phase 01", "phase-1", or a bare phase number (0–8).

If a phase is matched:
1. Read `docs/roadmap.md`
2. Extract the matching phase block: Objective, Depends on, Deliverables,
   Documentation, and Parallelisable with sections
3. Find any rows in the Pending Architectural Decisions table that map to
   this phase
4. Use this extracted content to pre-populate the Context section below —
   do not ask the user to re-describe what is already in the roadmap

If no phase is matched, treat as an ad-hoc plan with no roadmap context.

## Step 1.5 — Detect design document

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

## Step 2 — Generate the plan

Structure the plan as follows:

1. **Goal** — what are we building or changing, in one sentence
2. **Context** — what exists today, what constraints apply
   - If roadmap-linked: include the phase objective, dependencies,
     deliverables list, and any pending architectural decisions from the
     roadmap
3. **Approach** — step-by-step implementation plan with file paths
   - If a design document was found in Step 1.5: each step should map to a specific
     section of the design (trait definitions, schema, flows). Reference the design's
     trait signatures and DDL by name — the design is the ground truth for API shapes.
   - If no design document exists: derive steps from the roadmap deliverables and
     CLAUDE.md architectural constraints
4. **Security implications** — does this touch `src-tauri/src/crypto/`,
   `src-tauri/src/auth/`, or `src-tauri/src/storage/`?
   If yes, note what the `security-reviewer` agent should check afterward.
   If no, state "None."
5. **Testing strategy** — what tests are needed, what boundary cases matter
6. **Documentation impact** — which `docs/` files need creating or updating
   after implementation

## Step 3 — Save the plan to disk

After generating the plan:

1. Determine the filename:
   - Roadmap phase: `phase-NNN-kebab-case-objective.md`
     (e.g., `phase-001-cryptographic-primitives.md`, zero-padded to 3 digits)
   - Ad-hoc: `YYYY-MM-DD-kebab-case-description.md`
     (e.g., `2026-03-29-refactor-error-handling.md`)
2. Write the plan to `.claude/plans/<filename>` using this frontmatter:

```yaml
---
title: "<plan title>"
created: "<ISO 8601 datetime>"
status: draft
roadmap-phase: <number or null>
design-document: <relative path or null>
tags: [<relevant tags>]
---
```

3. Report the saved path to the user.

Do NOT start implementing. Output the plan and the saved path only.
Wait for approval. When approved, use `/implement-plan <filename>` to execute.
