# Interactive Design

Collaborative design session for: $ARGUMENTS

This is a **conversation**, not an autonomous task. Present findings, ask questions, **stop and wait** after each phase. Never auto-advance.

---

## Argument Parsing

$ARGUMENTS can be:

1. **New design**: Topic or phase name
   - `"Phase 6"`, `"phase-6"`, `"6"` → match to `docs/roadmap.md`
   - `"error recovery strategy"` → standalone design

2. **Review**: `review <filename>` or `review <topic>`
   - `review tauri-ipc` → exact match in `docs/architecture/designs/`
   - `review authentication` → fuzzy match on keywords

3. **List**: `list` → show all designs with status

---

## Phase 1 — Understand

1. Parse $ARGUMENTS, check if it maps to a roadmap phase
2. Read context:
   - `CLAUDE.md` — project constraints
   - Existing designs in `docs/architecture/designs/`
   - Relevant ADRs in `docs/architecture-decisions/`
3. Present summary of understanding
4. Ask clarifying questions (scope, constraints, concerns)
5. **Stop and wait.**

---

## Phase 2 — Research

1. Web search for prior art and standards:
   - RFCs, NIST publications, OWASP guidance
   - How comparable systems solve this (KeePassXC, age, Signal, LUKS)
   - Crate docs and security audits
2. Present findings (2-3 paragraphs per source, include links)
3. List key decision areas identified
4. Ask: proceed to options, or research something specific?
5. **Stop and wait.**

---

## Phase 3 — Options (one decision at a time)

For **each** major decision:

1. Present 2-4 options using this format:

   ```
   ### Decision: <What we're deciding>

   #### Option A: <Name>
   **How it works**: <brief description>
   **Pros**: ...
   **Cons**: ...
   **Used by**: <prior art>

   #### Option B: <Name>
   ...

   **Recommendation**: Option X because <reason>
   ```

2. Ask which option they prefer (use `ask_user` with choices)
3. **Stop and wait.**
4. Record choice, move to next decision. Repeat until all decisions made.

---

## Phase 4 — Confirm

1. Present summary table of all decisions made
2. List any remaining open questions
3. Ask: ready for design document, or revisit anything?
4. **Stop and wait.**

---

## Phase 5 — Write

1. Create `docs/architecture/designs/<design-name>/design.md`
2. Include: decisions with rationale, options considered, security analysis
3. Summarise key sections
4. Ask if changes needed before artifacts
5. **Stop and wait.**

---

## Phase 6 — Artifacts

1. Offer: diagrams, ADR, report-log entry
2. **Stop and wait.**
3. Create only what requested.

---

## Review Mode

When $ARGUMENTS starts with `review`:

### Load and Analyse
1. Find design in `docs/architecture/designs/` (exact or fuzzy match)
2. Read document + related ADRs + current implementation
3. Present: title, status, last updated, summary
4. Ask what to review: full, decisions, security, implementation gaps, specific section
5. **Stop and wait.**

### Section-by-Section
For each section:
1. Present current state
2. Analyse: still valid? new alternatives? implementation concerns? security implications?
3. Ask: looks good, discuss, or change?
4. **Stop and wait.**
5. Record changes, move to next section.

### Apply Updates
1. Present summary table of proposed changes
2. Ask: apply updates?
3. **Stop and wait.**
4. If confirmed: update document, update date, note reconsidered decisions

---

## List Mode

When $ARGUMENTS is `list`:

1. Read all `.md` files in `docs/architecture/designs/`
2. Present table: Design | Status | Phase | Last Updated
3. Ask: which to review, or create new?
4. **Stop and wait.**

---

## Example Flow

```
Agent: "I'll design X. Here's what I understand: [summary].
       Questions: 1) What's the scope? 2) Any constraints?"
       [ask_user with choices + freeform]

User:  "Scope is A and B, not C."

Agent: "Researched prior art. Found: [summaries with links].
       Key decisions: auth method, storage format, sync strategy.
       Ready to explore options?"
       [ask_user: "Yes, proceed", "Research more on...", freeform]

User:  "Yes, proceed"

Agent: "Decision 1 - Auth method:
       Option A: Password + TOTP ...
       Option B: Password + USB key file ...
       Recommendation: B because [reason]. Which do you prefer?"
       [ask_user: "Option A", "Option B", "Discuss further", freeform]

User:  "Option B"

Agent: "Recorded. Decision 2 - Storage format: ..."
```

---

## vs `/design`

| Aspect | `/design` | `/interactive-design` |
|--------|-----------|----------------------|
| Interaction | Autonomous | Multi-turn conversation |
| Research | Silent | User sees findings |
| Decisions | Agent decides | User chooses from options |
| Pace | Fast (one turn) | Deliberate (many turns) |
| Best for | Clear scope | Complex decisions, learning together |

---

## Rules

1. **Stop after each phase** — never auto-continue
2. **One decision at a time** — don't overwhelm
3. **Always present options** — even if one seems obviously better
4. **Use the Ask tool** — with choices when options are clear
5. **Cite sources** — link to RFCs, docs, prior art