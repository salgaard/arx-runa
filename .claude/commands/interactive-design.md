# Interactive Design

Collaborative design session for: $ARGUMENTS

Multi-turn conversation — present findings, ask questions, **stop and wait** after each phase.

## Arguments
- **New**: `"Phase 6"`, `"error recovery"` → create design
- **Review**: `review <topic>` → review existing design
- **List**: `list` → show all designs

## Phases (stop after each)

### 1. Understand
Parse topic, read CLAUDE.md + existing designs + ADRs. Present summary, ask scope questions. **Wait.**

### 2. Research
Web search: RFCs, NIST, OWASP, prior art (KeePassXC, Signal, LUKS). Summarise findings with links. **Wait.**

### 3. Options (one decision at a time)
Present 2-4 options per decision: how it works, pros/cons, prior art, recommendation. **Wait.** Record choice, next decision.

### 4. Confirm
Summary table of all decisions. List open questions. **Wait.**

### 5. Write
Create `docs/architecture/designs/<topic>.md` with decisions, rationale, security analysis. **Wait.**

### 6. Artifacts
Offer diagrams, ADR, report-log entry. Create only what requested.

## Review Mode
1. Load design, present status. Ask: full review, decisions, security, or specific section. **Wait.**
2. Per section: present, analyse (still valid? new alternatives?), ask proceed/discuss. **Wait.**
3. Compile changes table. **Wait.**
4. Apply updates if confirmed.

## Rules
- Stop after each phase — never auto-continue
- One decision at a time
- Always present options with prior art
- Cite sources