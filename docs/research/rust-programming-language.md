# Rust for security-critical systems programming

> **Document type**: Exploration / feasibility research
> **Status**: Living document
> **Last updated**: 2026-04-13

## The Problem

Arx Runa depends on memory-safe, cross-platform systems code for cryptography, storage, and local-only security boundaries. This document evaluates whether Rust is still the right primary implementation language for those constraints, and whether the current language choice aligns with current safety guidance from standards and government security bodies.

From a zero-knowledge perspective, language choice does not change the cryptographic contract by itself. However, implementation-level memory-safety defects can still leak plaintext, keys, or metadata, so language/runtime risk remains security-relevant.

## Recommendation

Continue using Rust as the default language for security-critical Arx Runa components.

Rationale:
1. Rust's ownership and borrowing model provides compile-time memory-safety guarantees that directly reduce classes of vulnerabilities common in C/C++ codebases.
2. Independent research (e.g., RustBelt) provides formal grounding for Rust's safety claims, including disciplined handling of `unsafe`.
3. Current public guidance from NIST and U.S. cyber agencies explicitly supports migration toward memory-safe languages for high-risk software.

Privacy-model impact:
- This recommendation preserves Arx Runa's fixed-size blob and metadata-privacy model, provided implementation constraints remain enforced (e.g., no plaintext persistence, no key logging, bounded and reviewed `unsafe` usage).

## Decisions

| Decision | Alternatives considered | Rationale |
|---|---|---|
| Replace the previous ad-hoc Rust document with a standards-backed, compliance-structured research note | Keep the prior long-form draft as-is; move the draft outside `docs/research/` | The prior version was structurally non-compliant and mixed high/low-quality citations. A shorter, standards-oriented document fits project governance and keeps claims evidence-backed. |

## Open Questions

- Should Arx Runa adopt an explicit unsafe-code budget (e.g., per-module cap plus mandatory safety justification checklist)?
- Should tool-assisted formal verification (Kani/Prusti/Creusot) be required for selected cryptographic or memory-sensitive modules?
- Which Rust ecosystem maturity signals (audit status, maintainer bus factor, MSRV policy) should be mandatory before introducing new security-sensitive dependencies?

## Sources

| Source | Topic | URL |
|---|---|---|
| The Rust Programming Language — Chapter 4 (Ownership) | Ownership/borrowing model and compile-time memory-safety discipline | https://doc.rust-lang.org/book/ch04-00-understanding-ownership.html |
| RustBelt (POPL 2018, DOI) | Formal foundations for Rust safety and `unsafe` encapsulation | https://doi.org/10.1145/3158154 |
| NIST — Safer Languages | NIST guidance and references on memory-safe language adoption | https://www.nist.gov/itl/ssd/software-quality-group/safer-languages |
| CISA/NSA Joint Cybersecurity Information Sheet (2025) | Memory-safe language adoption guidance for vulnerability reduction | https://media.defense.gov/2025/Jun/23/2003742198/-1/-1/0/CSI_MEMORY_SAFE_LANGUAGES_REDUCING_VULNERABILITIES_IN_MODERN_SOFTWARE_DEVELOPMENT.PDF |
| DARPA — Eliminating Memory Safety Vulnerabilities Once and For All | U.S. government program context for systemic memory-safety risk reduction | https://www.darpa.mil/news/2024/memory-safety-vulnerabilities |
| Atlantic Council — Buying Down Risk: Memory Safety | Policy and ecosystem analysis of memory-safety-driven risk reduction | https://www.atlanticcouncil.org/content-series/buying-down-risk/memory-safety/ |
