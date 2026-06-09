# Arx Runa

Zero-knowledge bring-your-own-cloud file encryption. Client encrypts before upload; cloud receives opaque blobs only — no keys, names, or metadata.

Design sources: `docs/architecture/designs/*/design.md` · `docs/architecture/design-invariants.md`. Canonical contract: each `## Contract Surface` section. Changes propagate to sub-phases, diagrams, rules, skills, agents, instructions.

## Naming
No abbreviations: `chunk_index` not `chunk_idx`. Rust keywords and acronyms (AEAD, KDF, HKDF) exempt.

## Hard rules
- Never write unencrypted sensitive data to disk
- User handles git — don't commit
- [!IMPORTANT!] Everything should be implemented now. All Deffered items should be implemented or discarded. No deferred tags are allowed anymore and should be handled immediately. We are in final stage of fixing errors and hardening/improving only.

## Platform
Targets Win/macOS/Linux; preserve behavior across all three; platform-specific code requires equivalent or documented limitation.

## Output
- Concise output; no sycophantic openers, closing fluff, or preamble before tool calls
- Multi-step: `step → result → next`, no narrative padding

## Lookup
- Markdown → jdocmunch-mcp
- Library APIs → context7
- Source code → jcodemunch-mcp
- RTK (token optimization) → See `~/.claude/RTK.md`

## Report
- when user report, rapport, rapporten, the report, bachelor rapporten. refer to docs/report/arx-runa-bachelorrapport.md.

## Token Efficiency
- Dont use any unnecessary filler words/sentences in replies to me.
- Dont write any unnecessary summaries to me.
- Be simple and precise in all output.
- use jdocmunch-mcp to navigate .md files
- when i say `index jdocmunch`, you should index this repo with `index_local` with `path: "C:\\Users\\chris\\source\\repos\\arx-runa"`
- after we have made changes to .md files you should `index jdocmunch` automatically