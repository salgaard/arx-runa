---
name: plan-management
description: >
  Manage the lifecycle of saved plan files in .claude/plans/.
  Use to discover existing plans, approve drafts before implementation,
  or get a status overview of all plans. Invokable manually via
  /plan-management <subcommand>.
---

Manage plan file lifecycle in `.claude/plans/`.

## Arguments

- `list` → show all plans with status, phase, and created date
- `approve <filename>` → change a plan's status from `draft` to `approved`
- `status` → dashboard showing counts per status value

---

## Listing plans (`/plan-management list`)

1. Read all `.md` files in `.claude/plans/` (exclude `_template.md`)
2. Parse the YAML frontmatter of each file
3. Sort by `created` descending (most recent first)
4. Output a table:

```
| Filename | Title | Status | Phase | Created |
|----------|-------|--------|-------|---------|
| phase-001-cryptographic-primitives.md | Cryptographic Primitives | approved | 1 | 2026-03-29 |
| 2026-03-29-refactor-plan-command.md   | Refactor Plan Command    | completed | — | 2026-03-29 |
```

5. Below the table, print a one-line summary count per status value:
   `draft: N  approved: N  in-progress: N  completed: N  superseded: N`

---

## Approving a plan (`/plan-management approve <filename>`)

1. Resolve the plan file:
   - If `<filename>` includes `.md`, read `.claude/plans/<filename>` directly
   - If `<filename>` omits `.md`, try `.claude/plans/<filename>.md`
   - If no match, list all draft plans and ask the user to choose
2. Read the file and parse its YAML frontmatter
3. If `status` is already `approved`, `in-progress`, or `completed`, report
   the current status and ask the user to confirm they still want to change it
4. Update the `status` field to `approved` in the frontmatter
5. Write the updated file back to disk
6. Confirm: `Approved: .claude/plans/<filename>`

---

## Status dashboard (`/plan-management status`)

1. Read all `.md` files in `.claude/plans/` (exclude `_template.md`)
2. Parse the YAML frontmatter of each file
3. Group by `status` and output counts with filenames:

```
DRAFT (N):
  - phase-001-cryptographic-primitives.md — Cryptographic Primitives
  ...

APPROVED (N):
  - ...

IN-PROGRESS (N):
  - ...

COMPLETED (N):
  - ...

SUPERSEDED (N):
  - ...
```

4. Flag any plans in `draft` status that are more than 7 days old (by `created`)
   as potentially stale.
