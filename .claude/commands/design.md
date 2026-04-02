Recommended model: `/model opus`

Create or manage an architecture design document: $ARGUMENTS

## Step 1 — Parse arguments and detect scope

$ARGUMENTS can be:
- A roadmap phase: "Phase 6", "phase 6", "Phase 06", "phase-6", or a bare number (0–9)
- A standalone topic: "error recovery strategy", "vault header format"
- `list` → skip to the **Listing designs** section below
- `update <filename>` → skip to the **Updating a design** section below

**If a phase is matched:**
1. Read `docs/roadmap.md`
2. Extract the matching phase block: Objective, Depends on, Deliverables, Documentation, Design document link
3. If the roadmap already links to a design document for this phase:
   - Read that design document
   - Ask the user: "A design already exists at `<path>`. Update it or create a new standalone design?"
   - If updating: follow the **Updating a design** section below
4. Follow the "Depends on" chain — read design documents for all prerequisite phases so the new design builds on established foundations

**If no phase is matched:** treat as a standalone design topic.

---

## Step 2 — Delegate to `design-architect` agent

Invoke the `design-architect` agent. The agent runs its own Research Phase (reads
CLAUDE.md, MEMORY.md, roadmap, existing designs, ADRs, and relevant source files)
then produces the complete design document following the established format.

**Filename convention:**
- Phase-linked: descriptive kebab-case matching the design topic, not the phase number
  (e.g., `tauri-ipc-and-frontend.md`, not `phase-006-tauri.md`)
- Standalone: `<kebab-case-topic>.md`

**Initial status line in the document:**
- Phase-linked: `> Status: Draft. Implementation target: Phase N.`
- Standalone: `> Status: Draft. Standalone design.`

---

## Step 3 — Generate companion artifacts

After the design document is saved, generate companion artifacts based on its content.

### Diagrams

For each flow, schema, or structure described in the design, invoke the `diagram` skill:

| Design content | Mermaid type |
|---|---|
| Multi-step authentication, encryption, or sync flow | `sequenceDiagram` |
| Module structure or architecture overview | `flowchart TD` |
| Database schema (nodes, chunks, tables) | `erDiagram` |
| Session lifecycle, vault states | `stateDiagram-v2` |
| Rust trait/type relationships | `classDiagram` |

Before creating each diagram, check `docs/architecture/diagrams/INDEX.md` — if a diagram for
this topic already exists, invoke `/diagram update <filename>` instead of creating a duplicate.

### Report-log entry

Invoke the `report-note` skill with type `decision` to capture the design creation as a
thesis-reportable event. Include:
- What design decisions were made and why
- Key alternatives considered
- Security trade-offs accepted

### Roadmap update

If the design is phase-linked and `docs/roadmap.md` does not already have a
`**Design document**:` line for this phase, add one:

```
**Design document**: `docs/architecture/designs/<design-name>/design.md`
```

Insert it after the `**Objective**:` line in the relevant phase block.

### ADR candidates

Scan the "Decisions Made" table in the new design. For each decision that does not have a
corresponding ADR in `docs/architecture-decisions/`, flag it as an ADR candidate in the
output. Do not auto-create ADRs — list them so the user can invoke
`/architecture-decision-record` when ready.

---

## Step 4 — Report to user

Output a concise summary:
- Path to the saved design document
- List of diagrams generated (or updated)
- Path to the report-log entry
- ADR candidates (list of decisions that need an ADR)
- Whether `docs/roadmap.md` was updated
- Reminder: "Design is in Draft status. Review and mark as 'Design complete' when satisfied.
  Then use `/plan <topic or phase>` to generate an implementation plan."

---

## Listing designs (`/design list`)

1. Read all `.md` files in `docs/architecture/designs/`
2. Extract the title and status blockquote from each
3. Display as a table: Design | Phase | Status | Last updated

---

## Updating a design (`/design update <filename>`)

1. Read the existing design at `docs/architecture/designs/<filename>`
2. Invoke the `design-architect` agent with the existing document — the agent re-reads
   all context via its Research Phase, updates the document, resolves Open Decisions,
   and updates citation markers
3. Update the `> Last updated:` line in the document
4. Re-run Step 3 (companion artifacts): update any existing diagrams, add a report-log
   entry for the revision if substantive changes were made
5. Report what changed
