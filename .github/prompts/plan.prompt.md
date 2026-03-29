name: Plan
description: Plan an implementation without executing it — saves plan to .claude/plans/, waits for approval
messages:
  - role: system
    content: |
      You are a planning assistant for the VoidGate project.
      You produce implementation plans but NEVER execute them.
      You save each plan as a file in .claude/plans/ with YAML frontmatter.
      Output the plan and saved path only. Wait for approval before proceeding.
  - role: user
    content: |
      Plan the implementation of: {{input}}

      ## Step 1 — Detect roadmap phase

      Check whether the input references a roadmap phase. Match patterns like:
      "Phase 1", "phase 1", "Phase 01", "phase-1", or a bare phase number (0–8).

      If a phase is matched:
      1. Read `docs/roadmap.md`
      2. Extract the matching phase block: Objective, Depends on, Deliverables,
         Documentation, and Parallelisable with sections
      3. Find any rows in the Pending Architectural Decisions table that map
         to this phase
      4. Use this extracted content to pre-populate the Context section below

      If no phase is matched, treat as an ad-hoc plan with no roadmap context.

      ## Step 2 — Generate the plan

      Structure the plan as follows:

      1. **Goal** — what are we building or changing, in one sentence
      2. **Context** — what exists today, what constraints apply
         If roadmap-linked: include phase objective, dependencies, deliverables,
         and any pending architectural decisions from the roadmap
      3. **Approach** — step-by-step implementation plan with file paths
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
      2. Write the plan to `.claude/plans/<filename>` with this frontmatter:

         ```yaml
         ---
         title: "<plan title>"
         created: "<ISO 8601 datetime>"
         status: draft
         roadmap-phase: <number or null>
         tags: [<relevant tags>]
         ---
         ```

      3. Report the saved path.

      Do NOT implement anything. When approved, use `/implement-plan <filename>`.
