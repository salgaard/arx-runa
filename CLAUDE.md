# Arx Runa

Zero-knowledge bring-your-own-cloud file encryption. Client encrypts before upload; cloud receives opaque blobs only — no keys, names, or metadata.

Design sources: `docs/architecture/designs/*/design.md` · `docs/architecture/design-invariants.md`. Canonical contract: each `## Contract Surface` section. Changes propagate to sub-phases, diagrams, rules, skills, agents, instructions.

## Naming
No abbreviations: `chunk_index` not `chunk_idx`. Rust keywords and acronyms (AEAD, KDF, HKDF) exempt.

## Hard rules
- Never write unencrypted sensitive data to disk
- User handles git — don't commit

## Platform
Targets Win/macOS/Linux; preserve behavior across all three; platform-specific code requires equivalent or documented limitation.

## Output
- Concise output; no sycophantic openers, closing fluff, or preamble before tool calls
- Multi-step: `step → result → next`, no narrative padding

## Lookup
- Design docs → jdocmunch-mcp (`docs/architecture/**`)
- Library APIs → context7
- Source code → jcodemunch-mcp
