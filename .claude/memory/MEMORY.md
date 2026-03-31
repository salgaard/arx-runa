# VoidGate — Agent Memory

- [Bachelor report requirement](project_bachelor_report.md) — PBA report: 30 pages max, English, objective register, auto-captured via /report-note and commit hook
- [Architecture rationale](architecture_rationale.md) — Why each major design decision was made: rejected alternatives, trade-offs, references (XChaCha20, AAD, HKDF tree, USB auth, manifest, etc.)
- [Pending decisions](pending_decisions.md) — Open design questions: USB key file format
- [Known gotchas](known_gotchas.md) — Implementation pitfalls: crate tag doubling, AAD serialisation, vault header upload order, BLAKE3 scope

## Key ADRs

- **ADR-001**: Code structure and patterns — see `docs/architecture-decisions/001-code-structure-and-patterns.md`
- **ADR-002**: Frontend stack (Leptos + Tailwind) — see `docs/architecture-decisions/002-frontend-stack-selection.md`
