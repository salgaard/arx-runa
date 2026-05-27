---
applyTo: "docs/architecture/designs/**/diagrams/**,docs/architecture/diagrams/**,docs/report/**"
---

# Mermaid Diagrams

> Full syntax reference: `.claude/reference/mermaid.md`

| Type | Use for |
|------|---------|
| `flowchart TD` | Process flows, pipelines, key derivation trees |
| `sequenceDiagram` | IPC flows, auth flows, sync sequences |
| `erDiagram` | SQLCipher schema, manifest structure |
| `classDiagram` | Rust traits, type hierarchies |
| `stateDiagram-v2` | Session lifecycle, vault state machine |
| `graph TD` | Information flows, SSOT diagrams |

Stick to these six types only. Experimental types (`architecture-beta`, `C4`, `timeline`) not used for core docs.

see https://mermaid.js.org/syntax/examples.html for multiple types and how to use. there are many subpages in https://mermaid.js.org/syntax/ like https://mermaid.js.org/syntax/architecture.html.

## Critical Syntax Constraints

**Flowchart node labels**
- Line breaks: `<br/>` — never `\n`
- Arrows: `#45;#62;` — never `->` or `-&gt;` (cause empty squares in v11)
- Pipe char: `#124;` for `|`, `#124;#124;` for `||` — never literal (parsed as edge-label delimiter)
- Unicode/emoji: ASCII only — `>=` not `≥`, no emoji in labels
- Reserved word: never use `end` as node ID; use `END` or `["end"]`
- Node IDs: never start with `o-` or `x-` (creates special edges)
- Edge labels: `-->|text|` only — never `-->|"quoted text"|`

**Sequence diagram**
- Line breaks: never `\n` — split into separate arrows; shorten notes to single line
- Arrows in text: `#45;#62;`; semicolons: `#59;`; pipe in notes: `#124;`; no emoji
- Never nest `par`/`end` inside `alt`/`else`/`end` — use `break`/`end` then `par` at top level

**ER diagram**
- FK arrows: `#45;#62;` — never `->` or `-&gt;`

## Layout
- Max 15 nodes; split on trust/layer boundaries if larger
- Descriptive node IDs (`AUTH`, `CRYPTO`) — never `a`, `b`, `c`; label every edge; intermediate nodes must be explicit

## Color Palette

```
classDef secret   fill:#dc2626,stroke:#991b1b,color:#fff
classDef crypto   fill:#2563eb,stroke:#1e40af,color:#fff
classDef storage  fill:#16a34a,stroke:#166534,color:#fff
classDef user     fill:#9333ea,stroke:#6b21a8,color:#fff
classDef boundary fill:#f59e0b,stroke:#d97706,color:#000
classDef infra    fill:#6b7280,stroke:#374151,color:#fff
```
