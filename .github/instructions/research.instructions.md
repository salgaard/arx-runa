---
applyTo: "docs/research/**"
---


# Research documents — rules

## Required sections
Every research document must contain all of these sections, in order:
- `The Problem` (or equivalent framing section)
- `Recommendation` — a clear position with rationale; not optional
- `Decisions` — table of choices made by the user during the session; not optional
- `Open Questions` — unresolved items; may be empty but must be present
- `Sources` — table format: `| Source | Topic | URL |`

## Decisions section
- Table format: `| Decision | Alternatives considered | Rationale |`
- One row per choice made during the session
- Updated in real time as decisions are made — not written retrospectively at close-out
- Records the user's choices, not the agent's recommendations (those go in `Recommendation`)

## Header block
Every document must open with this block immediately after the H1 title:

```
> **Document type**: Exploration / feasibility research  (or: Exploration / brainstorming)
> **Status**: Draft | Living document | Concluded
> **Last updated**: YYYY-MM-DD
```

## Status values
- `Draft` — active session, document incomplete
- `Living document` — session concluded, further investigation may continue
- `Concluded` — decision reached, no further changes expected

## File naming
- kebab-case: `bin-packing.md`, `padding-overhead-reduction.md`
- Descriptive of the specific topic, not generic

## README
- Every research document must have a corresponding entry in `docs/research/README.md`
- Entry format: `- **[Title](filename.md)** — one-sentence summary`

## Sources
- Every non-trivial claim must have a source entry
- Security and cryptographic claims must reference standards: NIST FIPS/SP, RFC, IACR ePrint, USENIX, IEEE, ACM
- URLs must be included — no bare author/title citations

## Source-quality rubric
- Normative technical claims (security, cryptography, architecture, performance, protocol semantics) must cite standards bodies, peer-reviewed venues, or official specification/vendor documentation
- Tertiary/news/blog sources are acceptable only for market sentiment, ecosystem context, or anecdotal background
- A tertiary source must not be the sole citation for normative technical claims; pair with a primary source or replace it
- If no higher-quality source is available, keep the claim explicitly marked unresolved and record it in `Open Questions`

## Cross-references
- Link related research docs and design docs instead of duplicating content
- Use relative paths: `compression-and-cloud-cost.md`, `docs/architecture/designs/.../design.md`

## Speculation
- Unverified claims: `<!-- TODO: verify -->`
- Speculative ideas belong in `Open Questions`, not `Recommendation`

## Privacy model
- Evaluate every proposed approach against Arx Runa's zero-knowledge threat model
- Explicitly state whether a technique preserves fixed-size blobs and metadata privacy

