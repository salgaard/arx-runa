---
paths:
  - "docs/research/**"
---

# Research documents

Required sections (in order): `The Problem`, `Recommendation` (clear position with rationale; mandatory), `Decisions` (user choices table; mandatory), `Open Questions` (may be empty; mandatory), `Sources` (table: `| Source | Topic | URL |`)

Header block immediately after H1:
```
> **Document type**: Exploration / feasibility research  (or: Exploration / brainstorming)
> **Status**: Draft | Living document | Concluded
> **Last updated**: YYYY-MM-DD
```
Status: `Draft` = active/incomplete · `Living document` = concluded, further investigation possible · `Concluded` = decision reached

Decisions table: `| Decision | Alternatives considered | Rationale |`; one row per session choice; updated in real time (not retrospectively); records user's choices (agent recommendations go in `Recommendation`)

- File naming: kebab-case, descriptive (`bin-packing.md`); add entry to `docs/research/README.md`: `- **[Title](filename.md)** — one-sentence summary`
- Every non-trivial claim needs a source entry; security/crypto claims cite NIST FIPS/SP, RFC, IACR ePrint, USENIX, IEEE, or ACM; include URLs — no bare citations
- Normative claims: cite standards, peer-reviewed venues, or official vendor docs; tertiary/blog = background only, never sole citation; no qualifying source → mark unresolved in `Open Questions`
- Link related docs instead of duplicating; use relative paths
- Unverified claims: `<!-- TODO: verify -->`; speculative ideas → `Open Questions`, not `Recommendation`
- Evaluate every approach against zero-knowledge threat model; state explicitly whether technique preserves fixed-size blobs and metadata privacy
