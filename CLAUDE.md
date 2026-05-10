# Arx Runa

Zero-knowledge bring-your-own-cloud file encryption. Client encrypts before upload; cloud receives opaque blobs only — no keys, names, or metadata.

Design sources: `docs/architecture/designs/*/design.md` · `docs/architecture/design-invariants.md`. Canonical contract: each `## Contract Surface` section. Changes propagate to sub-phases, diagrams, rules, skills, agents, instructions.

## Naming
No abbreviations: `chunk_index` not `chunk_idx`. Rust keywords and acronyms (AEAD, KDF, HKDF) exempt.

## Hard rules
- Never write unencrypted sensitive data to disk
- Never commit secrets or key files
- User handles git — don't commit

## Platform
Targets Win/macOS/Linux; preserve behavior across all three; platform-specific code requires equivalent or documented limitation.

## Output
- Read files before writing; don't re-read unless changed; skip > 100 KB unless required
- Concise output; no sycophantic openers, closing fluff, or preamble before tool calls
- Review feedback: `BLOCK:` / `WARN:` / `NOTE:`
- Multi-step: `step → result → next`, no narrative padding

## Lookup
- Design docs → jdocmunch-mcp (`docs/architecture/**`)
- Library APIs → context7 (Tauri, Leptos, ring, argon2, tokio, rclone)
- Source code → jcodemunch-mcp; fallback: `code-explorer` agent

## Agent routing
| Task | Agent |
|---|---|
| Symbol lookup, grep, file navigation | `code-explorer` (Haiku) |
| Trivial Rust changes (rename, fmt, docs) | `simple-implementer` (Haiku) |
| Complex Rust implementation | `rust-implementer` (Sonnet) |
| Security invariant analysis | `security-reviewer` (Sonnet) |
| Architecture decisions | `architecture-reviewer` (Sonnet) |
| Blocked path or unknown security invariant | Escalate to Opus |

## Context
- `/clear` between major task domains (UI ↔ crypto ↔ storage ↔ auth)
- `/strategic-compact` after ~30 tool calls, after exploration, or before switching domains
- Use sub-agents when token-efficient
