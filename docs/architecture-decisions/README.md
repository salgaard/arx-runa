# Architecture Decision Records

This directory contains Architecture Decision Records (ADRs) documenting
significant design choices made during VoidGate development.

## What is an ADR?

An ADR captures the context, decision, and consequences of an architectural
choice. They serve as:

- Historical record of why decisions were made
- Onboarding documentation for new contributors
- Reference for future decision-making

## Format

Each ADR follows this structure:

```markdown
## Decision-NNN: Title
**Date / Status**

### Context
What problem are we solving? What constraints apply?

### Decision
What did we choose and why?

### Consequences
What trade-offs did we accept? What risks exist?

### References
- Relevant standards, RFCs, prior art
```

## Records

| ID | Title | Date | Status |
|----|-------|------|--------|
| [001](001-code-structure-and-patterns.md) | Code Structure and Patterns | 2026-03-29 | Accepted |
| [002](002-frontend-stack-selection.md) | Frontend Stack Selection | 2026-03-30 | Accepted |

## Creating New ADRs

Use the `/architecture-decision-record` skill to create new ADRs following
the established format.
