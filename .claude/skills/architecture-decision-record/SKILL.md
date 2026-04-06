---
name: architecture-decision-record
description: >
  Create an Architecture Decision Record (ADR) for Arx Runa. Invoke proactively
  when an architectural decision is made — selecting a technology, choosing
  between design alternatives, defining a security boundary, or establishing a
  convention that constrains future work. Also invokable manually via
  /architecture-decision-record <topic>.
---

Create or update an Architecture Decision Record in `docs/architecture-decisions/`.

## Capturing a new ADR

### Step 1 — Auto-number the record

Read the filenames in `docs/architecture-decisions/` to find the highest existing
number. Assign the next integer, zero-padded to three digits (e.g., `001`, `012`).
If the directory is empty, start at `001`.

### Step 2 — Generate the filename

Format: `NNN-kebab-case-title.md`

Example: `003-xchacha20-poly1305-cipher-selection.md`

Keep the kebab title descriptive but concise (4-8 words). Save to
`docs/architecture-decisions/`.

### Step 3 — Write the ADR

Use the `documentation-writer` agent to produce the record. Structure:

```markdown
# NNN — Title

**Date:** YYYY-MM-DD
**Status:** Draft

## Context

The problem, constraints, and forces at play. What made a decision necessary?
Reference relevant standards (RFC 8439, RFC 5869, NIST SP 800-63, OWASP ASVS)
and project requirements from CLAUDE.md.

## Decision

What was chosen and why. Be specific — name the alternative(s) considered and
explain why this option was selected over them.

## Consequences

**Positive:** What this enables or improves.
**Negative / trade-offs:** What is harder, riskier, or constrained as a result.
**To monitor:** What could go wrong or need revisiting.

## References

- Standards, RFCs, crate docs, or prior art that informed this decision.
```

Follow docs.md register, citation, and naming rules. 300–800 words total.

### Step 4 — Cross-reference report-log entries

Check `docs/report-log/INDEX.md` for entries with matching tags or titles.
If found, add a **Related report-log entries** section at the bottom of the ADR
listing the filenames with links.

### Step 5 — Invoke report-note

If this ADR documents a decision not already captured in the report log,
invoke the `report-note` skill with:
- Type: `decision`
- The ADR title and number as context

### Step 6 — Confirm

Output one line: `ADR created: NNN — <title>`

---

## Updating an existing ADR

If invoked with `update <filename>`:
1. Read the existing file
2. Change **Status** from `Draft` to `Accepted` (or as appropriate)
3. Add or expand sections based on new context
4. Do not delete existing content — append or revise with a note

---

## Listing ADRs

If invoked with `list`:
1. Read all `.md` files in `docs/architecture-decisions/` (exclude `.gitkeep`)
2. Parse the title and status from each
3. Display as a table: Number | Title | Status | Date
