Plan the implementation of: $ARGUMENTS

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
   - The plan filename will be `phase-NNN-S-kebab-case.md` (e.g., `phase-004-1-cloud-transport.md`)
4. If no sub-roadmap found: warn the user and fall back to full phase planning

**If a full phase is matched**:
1. Read `docs/roadmap.md`
2. Extract the matching phase block: Objective, Depends on, Deliverables,
   Documentation, and Parallelisable with sections
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

## Step 2 — Generate the plan

Structure the plan as follows:

1. **Goal** — what are we building or changing, in one sentence
2. **Context** — what exists today, what constraints apply
   - If roadmap-linked: include the phase objective, dependencies,
     deliverables list, and any pending architectural decisions from the
     roadmap
   - **If sub-phase**: include dependencies from the sub-roadmap (e.g., "Depends on Phase 4.1"), estimated scope, and any implementation notes
3. **Approach** — step-by-step implementation plan with file paths
   - If a design document was found in Step 1.5: each step should map to a specific
     section of the design (trait definitions, schema, flows). Reference the design's
     trait signatures and DDL by name — the design is the ground truth for API shapes.
   - **If sub-phase**: use the Deliverables list from the sub-roadmap as the primary structure; each deliverable becomes an implementation step
   - If no design document exists: derive steps from the roadmap deliverables and
     CLAUDE.md architectural constraints
4. **Security implications** — does this touch `src-tauri/src/crypto/`,
   `src-tauri/src/auth/`, or `src-tauri/src/storage/`?
   If yes, note what the `security-reviewer` agent should check afterward.
   If no, state "None."
   - **If sub-phase**: check the sub-roadmap's Security Review Checkpoints section
5. **Testing strategy** — what tests are needed, what boundary cases matter
   - Use the template's structured format with checkboxes for test types
   - **Explicitly decide**: check "Invoke test-writer agent? YES/NO" with rationale
   - **If sub-phase**: include the Validation checkpoint from the sub-roadmap (automated tests, manual verification, acceptance criteria)
6. **Documentation impact** — which `docs/` files need creating or updating
   after implementation

## Step 3 — Save the plan to disk

After generating the plan:

1. Determine the filename:
   - **Sub-phase**: `phase-NNN-S-kebab-case-description.md`
     (e.g., `phase-004-1-cloud-transport.md`, where N=phase number, S=sub-phase identifier, zero-padded to 3 digits)
   - Roadmap phase: `phase-NNN-kebab-case-objective.md`
     (e.g., `phase-001-cryptographic-primitives.md`, zero-padded to 3 digits)
   - Ad-hoc: `YYYY-MM-DD-kebab-case-description.md`
     (e.g., `2026-03-29-refactor-error-handling.md`)
2. Write the plan to project folder `.claude/plans/<filename>` using this frontmatter:

```yaml
---
title: "<plan title>"
created: "<ISO 8601 datetime>"
status: draft
roadmap-phase: <number or null>
sub-phase: <"N.S" or null>
design-document: <relative path or null>
sub-phase-roadmap: <relative path or null>
tags: [<relevant tags>]
---
```

3. Report the saved path to the user.

Do NOT start implementing. Output the plan and the saved path only.
Wait for approval. When approved, use `/implement-plan <filename>` to execute.
