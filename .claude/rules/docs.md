---
paths:
  - "docs/**"
---

# Documentation — rules

## Register
- Academic, objective — no first-person, no padding
- Define terms on first use; cite standards (RFC, NIST, OWASP)

## Citations
- Flag claims needing citation: `<!-- CITE: suggested source -->`
- Security claims must reference established standards

## ADR format
- File: `NNN-kebab-case-title.md` in `docs/architecture-decisions/`
- Sections: Context, Decision, Consequences, References

## What goes where
- `docs/architecture-decisions/` — design choices with rationale
- `docs/architecture/` — diagrams, key derivation, data flow
- `docs/threat-model/` — protections and out-of-scope threats
- `docs/guides/` — setup, workflows
