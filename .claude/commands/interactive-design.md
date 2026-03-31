# Interactive Design

Collaborative design session for: $ARGUMENTS

This command facilitates a **teamwork-driven design process** where we research together,
explore options, and reason through decisions before producing a design document.

---

## Argument Parsing

$ARGUMENTS can be:

1. **New design**: A topic or phase name
   - `"Phase 6"`, `"phase-6"`, `"6"` → new design for roadmap phase
   - `"error recovery strategy"` → new standalone design

2. **Review existing**: `review <filename>` or `review <topic>`
   - `review tauri-ipc-and-frontend` → review existing design
   - `review authentication` → finds matching design file
   
3. **List designs**: `list`
   - Shows all existing designs with status

If `review` is detected, skip to the **Review Mode** section below.

---

## CRITICAL: Multi-turn collaboration

This is a **conversation**, not an autonomous task.

**After each phase**: Present findings, ask questions, then **stop and wait** for the user
to respond before continuing. Never auto-advance to the next phase.

**Asking questions**: Use choices when options are clear, freeform when open-ended.
The goal is to make it easy for the user to respond quickly while allowing elaboration.

---

## Phase 1 — Understand the topic

1. Parse $ARGUMENTS to identify the design topic
2. Check if this relates to a roadmap phase (`docs/roadmap.md`)
3. Read existing context:
   - `CLAUDE.md` — project constraints
   - `.claude/memory/MEMORY.md` — prior decisions
   - Existing designs in `docs/architecture/designs/` — avoid contradictions
   - Relevant ADRs in `docs/architecture-decisions/`

4. Present a summary of what you understood, then ask clarifying questions:
   - Scope boundaries (what's in/out)
   - Key constraints or requirements
   - Any specific concerns to address

5. **Stop and wait.**

---

## Phase 2 — Research together

After user confirms scope:

1. **Web research**: Search for prior art, standards, and established patterns
   - Relevant RFCs, NIST publications, OWASP guidance
   - How comparable systems solve this (KeePassXC, age, Signal, LUKS, etc.)
   - Crate documentation and security audits

2. Present findings:
   - Summarise each source (2-3 paragraphs max)
   - Include links for verification
   - List the key decision areas identified

3. Ask if they want to proceed to options or research something specific.

4. **Stop and wait.**

---

## Phase 3 — Explore options (one decision at a time)

For **each** major design decision:

1. Present 2-4 concrete options:

   ```
   ### Decision: <What we're deciding>
   
   #### Option A: <Name>
   **How it works**: <brief description>
   **Pros**: ...
   **Cons**: ...
   **Used by**: <prior art>
   
   #### Option B: <Name>
   ...
   
   **My recommendation**: Option X because <reason>
   ```

2. Ask which option they prefer, or if they want to discuss further.

3. **Stop and wait.**

4. Based on response:
   - Choice made → record it, move to next decision
   - Want to discuss → explore together, then stop again
   - New idea → explore it, then stop again

5. Repeat for each major decision.

---

## Phase 4 — Confirm design direction

After all decisions are made:

1. Present a compact summary table of all decisions made
2. List any remaining open questions
3. Ask if they're ready for the design document, or want to revisit anything.

4. **Stop and wait.**

---

## Phase 5 — Write the design document

Only after user confirms:

1. Write the design document to `docs/architecture/designs/<topic>.md`
2. Follow the standard VoidGate design document format
3. Include:
   - All decisions made with rationale
   - The options considered (in "Decisions Made" table)
   - Security analysis
   - Open decisions (if any remain)

4. Summarise the key sections for review.
5. Ask if any changes are needed before companion artifacts.

6. **Stop and wait.**

---

## Phase 6 — Companion artifacts

After user approves the document:

1. Offer to create:
   - Diagrams (sequence, flow, ER diagrams)
   - ADR for key decisions
   - Report-log entry

2. **Stop and wait.**

3. Create only what user requests.

---

## Rules

1. **Stop after each phase** — never auto-continue
2. **One decision at a time** — don't overwhelm
3. **Always present options** — even if one seems obviously better
4. **Cite sources** — link to RFCs, docs, or prior art
5. **Be patient** — the goal is collaboration, not speed

---

## Example flow

```
Agent: "I'll design X. Here's what I understand: [summary]. 
       Questions: 1) What's the scope? 2) Any constraints?"
       [choices: "Scope is...", "Add constraint...", freeform]

User:  "Scope is A and B, not C."

Agent: "Researched prior art. Found: [summaries with links].
       Key decisions needed: auth method, storage format, sync strategy.
       Ready to explore options?"
       [choices: "Yes, proceed", "Research more on...", freeform]

User:  "Yes, proceed"

Agent: "Decision 1 - Auth method:
       Option A: Password + TOTP ...
       Option B: Password + USB key file ...
       I recommend B because [reason]. Which do you prefer?"
       [choices: "Option A", "Option B", "Discuss further", freeform]

User:  "Option B"

Agent: "Recorded. Decision 2 - Storage format: ..."
```

---

## Differences from `/design`

| Aspect | `/design` | `/interactive-design` |
|--------|-----------|----------------------|
| Interaction | Fully autonomous | Multi-turn conversation |
| Research | Agent does it silently | User sees findings, can request more |
| Decisions | Agent decides based on context | User chooses from presented options |
| Pace | Fast — one turn | Deliberate — many turns |
| Best for | Clear scope, trusted defaults | Complex decisions, learning together |

---

## Review Mode

When $ARGUMENTS starts with `review`:

### Review Phase 1 — Load and Analyse

1. Find the design document:
   - Check `docs/architecture/designs/` for matching filename
   - Fuzzy match on topic keywords if exact match not found

2. Read the full document and related context:
   - The design document itself
   - Related ADRs referenced in the document
   - Current implementation (if any exists in `src-tauri/`)
   - Any newer designs that might affect this one

3. Present:
   - Document title and current status (Draft/Complete)
   - Last updated date
   - Brief summary of what the design covers

4. Ask what aspects to review:
   - Full review — go through everything
   - Decisions — revisit the choices made
   - Security — focus on security analysis
   - Implementation gaps — what's missing for implementation
   - Specific section

5. **Stop and wait.**

### Review Phase 2 — Section-by-Section Analysis

Based on user's choice, go through the document:

**For each section or decision:**

1. Present the current state (quote or summarise)

2. Provide analysis:
   - Does this still align with project constraints?
   - Have standards/best practices changed?
   - Are there new alternatives worth considering?
   - Any implementation concerns?
   - Security implications?

3. Ask:
   - Looks good — move on?
   - Concerns about X — discuss?
   - Potential improvement — interested?

4. **Stop and wait.**

5. Based on response:
   - "Looks good" → move to next section
   - "Discuss" → explore together, stop again
   - "Change it" → note the change, move on

### Review Phase 3 — Compile Changes

After reviewing all requested sections:

1. Present:
   - Summary table of proposed changes
   - Sections that remain unchanged
   - Any new open decisions identified

2. Ask if you should update the design document with these changes.

3. **Stop and wait.**

### Review Phase 4 — Apply Updates

Only after user confirms:

1. Update the design document
2. Update `> Last updated:` date
3. Add any new decisions to "Decisions Made" table
4. If decisions changed significantly, note in "Open Decisions" what was reconsidered

5. Summarise the changes made.
6. Ask if companion artifacts (diagrams, ADRs) need updating.

7. **Stop and wait.**

---

## List Mode

When $ARGUMENTS is `list`:

1. Read all `.md` files in `docs/architecture/designs/`
2. Extract from each: title, status, last updated, implementation phase

3. Present as a table:

   | Design | Status | Phase | Last Updated |
   |--------|--------|-------|--------------|
   | Authentication and Session Management | Complete | Phase 2 | 2026-03-29 |
   | Chunking and Manifest | Complete | Phase 3 | 2026-03-XX |
   | ... | ... | ... | ... |

4. Ask which design to review, or if they want to create a new one.

5. **Stop and wait.**
