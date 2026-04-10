# Interactive Research

Collaborative research session for: $ARGUMENTS

**Conversation, not automation.** Present findings, ask questions, present options, present recommendations — find optimal solutions together.

---

## Flow

### 0. Set Up the Research File

1. Derive a kebab-case filename from the topic: `docs/research/<topic-name>.md`
2. Check `docs/research/` for related existing research documents — link them in the new doc
3. Create the file with this exact header and scaffold:

```markdown
# Arx Runa: <Title>

> **Document type**: Exploration / feasibility research
> **Status**: Draft
> **Last updated**: <YYYY-MM DD>

<One-sentence description of what this document investigates.>

For background on <related topic>, see `<related-doc>.md`.

---

## Table of Contents

1. [The Problem](#the-problem)
2. [Prior Art](#prior-art)
...
N. [Recommendation](#recommendation)
N+1. [Decisions](#decisions)
N+2. [Open Questions](#open-questions)
N+3. [Sources](#sources)

---

## The Problem

...

---

## Recommendation

...

---

## Decisions

> Choices made during this research session. Updated as the session progresses.

| Decision | Alternatives considered | Rationale |
|---|---|---|

---

## Open Questions

...

---

## Sources

| Source | Topic | URL |
|---|---|---|
```

4. Add an entry to `docs/research/README.md` before the session ends

### 1. Research

1. **Deep search**: web, codebase, standards (RFCs, NIST, FIPS, OWASP), academic papers (IACR ePrint, USENIX Security, IEEE, ACM CCS), prior art in production systems
2. **Privacy model lens**: evaluate every finding against Arx Runa's zero-knowledge threat model — does it preserve fixed-size blobs, protect metadata, avoid side channels?
3. Present findings with sources (author, venue/publisher, year, URL)
4. Ask: "What should we dig into next?" or "Ready to discuss?"
5. Update the research document continuously — do not batch all writes to the end
6. Repeat as needed

### 2. Discuss

1. Present one topic, decision, or finding at a time
2. If options exist, show them with trade-offs (a comparison table is preferred)
3. Ask for input
4. Repeat until done

### 3. Close Out

1. Write the `Recommendation` section — a clear position with rationale
2. Confirm the `Decisions` table is complete — every choice made during the session must have a row
3. Capture unresolved questions in `Open Questions`
4. Populate the `Sources` table with every reference used
5. Set `Status` to `Living document` (if further investigation expected) or `Concluded` (if a decision was reached)
6. Confirm `docs/research/README.md` has been updated

---

## Rules

1. **One thing at a time** — don't overwhelm
2. **Suggest, don't assume** — user decides which direction to take
3. **Cite everything** — every claim that isn't common knowledge needs a source entry
4. **Security claims require standards** — NIST, RFC, IACR, peer-reviewed paper
5. **Flag speculation** — mark unverified claims with `<!-- TODO: verify -->`
6. **Cross-reference** — link related research docs and design docs rather than duplicating content
7. **Use the Ask tool** — with choices when options are clear

---

## Status Values

| Status | Meaning |
|---|---|
| `Draft` | Active session, document incomplete |
| `Living document` | Session concluded, further investigation expected |
| `Concluded` | Decision reached, document is final |
