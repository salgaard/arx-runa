# Interactive Research

Collaborative research session for: $ARGUMENTS

**Conversation, not automation.** Present findings, ask questions, stop and wait. Never auto-advance.

---

## Flow

### 1. Orient

1. Read relevant existing docs (`CLAUDE.md`, related files in `docs/`)
2. Present: "Here's what I found. What are we trying to figure out?"
3. **Stop and wait.**

### 2. Research

1. Search: web, codebase, standards (RFCs, NIST, OWASP), prior art
2. Present findings with links
3. Ask: "What should we dig into next?" or "Ready to discuss?"
4. **Stop and wait.**
5. Repeat as needed.

### 3. Discuss

1. Present one topic, decision, or finding at a time
2. If options exist, show them with trade-offs
3. Ask for input
4. **Stop and wait.**
5. Repeat until done.

### 4. Document (optional)

When research leads to something worth capturing, suggest appropriate output:

| Research about... | Suggest |
|-------------------|---------|
| Architecture, design choices | `docs/architecture/designs/<design-name>/design.md` |
| Threats, attacks, mitigations | `docs/threat-model/<topic>.md` |
| A significant decision | ADR in `docs/architecture-decisions/` |
| Process, how-to | `docs/guides/<topic>.md` |
| Implementation approach | `/plan` for todos |

Ask before creating anything. User drives what gets documented.

---

## Rules

1. **Stop after each step** — never auto-continue
2. **One thing at a time** — don't overwhelm
3. **Suggest, don't assume** — user decides what to document
4. **Cite sources** — include links
5. **Use `ask_user`** — with choices when options are clear
