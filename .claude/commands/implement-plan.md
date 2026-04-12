Implement the saved plan: $ARGUMENTS

## Step 1 — Resolve the plan file

Locate the plan file from $ARGUMENTS:
- If $ARGUMENTS is a full filename (e.g., `phase-001-cryptographic-primitives.md`),
  read `.claude/plans/$ARGUMENTS`
- If $ARGUMENTS is a filename without the `.md` extension, append it and try again
- If $ARGUMENTS is `latest`, find the most recently created file in `.claude/plans/`
  (by the `created` frontmatter field, excluding `_template.md`)
- If $ARGUMENTS is empty or no match is found, list all files in `.claude/plans/`
  (excluding `_template.md`) with their title, status, and created date, then ask
  the user to choose one

## Step 2 — Validate the plan and detect sub-phase

1. Read the plan file and parse its YAML frontmatter
2. **Sub-phase detection**: Check if `sub-phase` field exists in frontmatter (e.g., `sub-phase: "4.1"`)
   - If present: this is a sub-phase plan; proceed with sub-phase-aware implementation
   - If absent: this is a full-phase or ad-hoc plan; use standard implementation flow
3. **If sub-phase plan**:
   - Read the sub-phase roadmap from `sub-phase-roadmap` frontmatter field
   - Extract the specific sub-phase section (e.g., "Phase 4.1: ...")
   - Note the dependencies (e.g., "Depends on: Phase 4.1")
   - Check if prerequisite sub-phases are complete:
     * Look for completed plan files matching the prerequisite pattern (e.g., `phase-004-1-*.md` with `status: completed`)
     * If prerequisite missing or not completed: warn the user with message: "Prerequisite sub-phase [X.Y] is not complete. Proceed anyway?" and wait for confirmation
4. If `status` is `draft`, warn the user: "This plan has not been approved. Proceed
   anyway?" and wait for confirmation before continuing
5. If `status` is `completed` or `superseded`, warn the user and ask for confirmation
6. Update `status` to `in-progress` in the plan file's frontmatter

## Step 3 — Implement

Follow the **Approach** section of the plan step by step:
1. Use the `rust-implementer` agent to implement each step following Arx Runa
   coding standards
2. If any modified files are in `src-tauri/src/crypto/`, `src-tauri/src/auth/`,
   or `src-tauri/src/storage/`, automatically invoke the `security-reviewer`
   agent on them
3. Fix any CRITICAL findings before continuing
4. Run `cargo test` and `cargo clippy -- -D warnings` to verify

**After rust-implementer completes implementation:**

Read the plan's **Testing Strategy** section:
- If "Invoke test-writer agent?" is checked **YES**:
  1. Parse the reason field to understand test focus (adversarial, property-based, coverage)
  2. Invoke the `test-writer` agent with the specific focus:
     - For adversarial tests: `/test adversarial` or direct test-writer invocation with crypto module paths
     - For property-based tests: test-writer agent with proptest requirement
     - For coverage gaps: `/test coverage` first, then test-writer if below target
  3. Run `cargo test` after test-writer completes
  4. Report test results and any new failures
- If "Invoke test-writer agent?" is checked **NO** or unchecked:
  - Rely on rust-implementer's inline tests
  - Proceed to `cargo test` and `cargo clippy -- -D warnings` verification

**If this is a sub-phase plan** (detected in Step 2):
- After implementation, read the Validation checkpoint from the sub-phase roadmap
- Display the validation checkpoint to the user with clear instructions for manual verification

## Step 4 — Flag documentation

Check the plan's **Documentation impact** section. List any `docs/` files that
need creating or updating. Do not auto-update docs; just report what is needed.

## Step 5 — Mark complete and flag documentation work

1. Update `status` to `completed` in the plan file's frontmatter

2. **Flag documentation requirements:**
   - Read the plan's **Documentation impact** section
   - If the plan has a `roadmap-phase` value (not null):
     a. Read `docs/roadmap.md` and find the phase's **Documentation** section
     b. Extract expected ADRs, report-log entries, and diagrams
     c. Cross-reference against existing files:
        - Glob `docs/architecture-decisions/` for existing ADRs
        - Read `docs/report-log/INDEX.md` for existing entries
        - Read `docs/architecture/diagrams/INDEX.md` for existing diagrams
     d. Output a documentation gap report:
        ```
        📄 Documentation required for Phase [N]:
        
        ADRs to create:
        - [ ] NNN-topic-name.md — [description from roadmap]
        - [ ] NNN-topic-name.md — [description from roadmap]
        
        Report-log entries to create:
        - [ ] [type]: [topic] — maps to report sections: [sections]
        
        Diagrams to create:
        - [ ] [diagram-name].md — [type: sequenceDiagram / flowchart / erDiagram]
        
        Command: /document phase-[N]
        ```
   - If the plan is ad-hoc (roadmap-phase: null):
     a. List files from **Documentation impact** section only
     b. Output: "Documentation flagged: [list]. No roadmap cross-reference available."

3. **Report next steps:**

**If this is a sub-phase plan**:
1. Read the sub-phase roadmap to determine the next sub-phase (if any)
2. Check if prerequisites for the next sub-phase are met (e.g., if next is 4.3, check that 4.1 and 4.2 are complete)
3. Display a structured completion report:

```
✓ Phase [X.Y] implementation complete
✓ Tests passed: [test command from validation checkpoint]
→ Validation checkpoint: [checkpoint description from sub-roadmap]
→ Next: /plan [X.Y+1] ([title of next sub-phase])
→ Dependencies: [list of prerequisites for next sub-phase with completion status]
```

**If this is a full-phase or ad-hoc plan**:
Report what was implemented, test results, and documentation work flagged above.
