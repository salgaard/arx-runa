# Design Review

Critical review session for: $ARGUMENTS

**Conversation, not automation.** Read the design, research alternatives, discuss findings one at a time, write a comprehensive standalone document, apply confirmed changes back to the design.

This command produces a research document in the same style as `docs/research/compression-and-cloud-cost.md` and `docs/research/bin-packing.md` — topic-driven sections with full prose, comparison tables, and inline position statements. It is not a findings report. Someone should be able to read the output document without ever having seen this conversation.

For open-ended topic research on a new subject, use `/research` instead.

---

## Argument Parsing

`$ARGUMENTS` can be:

- **Design name**: `cryptographic-primitives`, `auth`, `chunking` → fuzzy-match in `docs/architecture/designs/`
- **Path**: `docs/architecture/designs/cryptographic-primitives/design.md` → read directly
- **Phase number**: `1`, `phase-1` → look up design doc from `docs/roadmap.md`

If the argument is ambiguous, list candidate matches and ask before proceeding.

---

## Flow

### 0. Load Context

1. Read the target `design.md` in full
2. Read `docs/roadmap.md` — find the phase entry to understand deliverables and dependencies
3. Check `docs/research/` for any prior research on this topic — link if found
4. Check `docs/architecture-decisions/` for ADRs that constrain this design
5. Note the design's existing **Decisions Made** table — these are the decisions to re-examine
6. Check for `sub-phases/` directory — if present, read `sub-phases/roadmap.md` and each `sub-phases/*.md` file. Note anywhere a sub-phase file reproduces a spec verbatim from design.md (dep versions, code blocks, config values) — these are candidates for conversion to references.

---

### 1. Set Up the Research File

Derive a filename: `docs/research/<design-name>-review.md`

Create with this scaffold — topics are placeholders, replace with the actual design decisions found in step 0:

```markdown
# Arx Runa: <Design Name> — Critical Review

> **Document type**: Exploration / feasibility research
> **Status**: Draft
> **Last updated**: YYYY-MM-DD

Critical review of `docs/architecture/designs/<design-name>/design.md` against
academic literature, production systems, and implementation correctness.
Each design decision is re-examined for correctness, completeness, and
missed opportunities.

For the canonical design, see `docs/architecture/designs/<design-name>/design.md`.

---

## Table of Contents

1. [The Problem](#the-problem)
2. [<Topic from design>](#topic)
3. [<Topic from design>](#topic)
...
N. [Recommendation](#recommendation)
N+1. [Decisions](#decisions)
N+2. [Open Questions](#open-questions)
N+3. [Sources](#sources)

---

## The Problem

...

---

<!-- One section per design decision topic. Written during the session. -->

---

## Recommendation

...

---

## Decisions

| Decision | Alternatives considered | Rationale |
|---|---|---|

---

## Open Questions

---

## Sources

| Source | Topic | URL |
|---|---|---|
```

---

### 2. Research

Search for critique across these lenses. Run web searches in parallel where possible.

**Alternative lens** — is there a better approach?
- For each key design decision: what are the alternatives? Is the design's choice still the best one?
- Note newer standards or algorithms that post-date the design

**Correctness lens** — will the implementation actually work?
- API version mismatches, renamed functions, deprecated methods
- Edition/compiler compatibility issues
- Wire format or encoding bugs

**Security lens** — are there known attacks or weaknesses?
- IACR ePrint, USENIX Security, IEEE S&P, ACM CCS for the primitives used
- CVEs or security advisories for the crates listed in the design
- Compare with how production systems (Signal, age, KeePassXC, Cryptomator, fscrypt) solve the same problem
- NIST, RFC, OWASP guidance for the algorithms or patterns used
- Evaluate against Arx Runa's zero-knowledge threat model specifically

**Completeness lens** — what's missing?
- Are there cases the design doesn't handle?
- Are there enforcement mechanisms missing (type-level, runtime)?
- Are there cross-module interactions not accounted for?

As research findings are discovered, **write them into the research document immediately** — draft the relevant topic section, even if incomplete. Do not hold all writing until the end.

---

### 3. Triage

Once all findings are identified, present a **complete triage table** to the user:

```
| # | Topic | Finding | Severity |
|---|-------|---------|----------|
| 1 | Nonces | rand = "0.8" compile error | Bug |
| 2 | Integrity | No structural check-before-decrypt | Gap |
| 3 | KDF | Empty HKDF salt | Improvement |
| 4 | Cipher | AEGIS-256 upgrade path exists | Note |
```

> **Severity**: Bug (will fail) · Gap (missing protection) · Improvement (better approach) · Note (document only)

Then say: "Ready to discuss, starting with #1?"

**Stop and wait.**

---

### 4. Discuss (one finding at a time, severity order)

For each finding:

1. Present it — but frame it as **the topic section it belongs to**, not as a numbered finding
2. Show the options with a **comparison table** where alternatives exist
3. Give a clear recommendation with rationale
4. **Stop and wait for the user's decision**
5. Once decided:
   - Write or expand the topic section in the research document with full prose (not bullet points)
   - Record the decision in the `## Decisions` table
   - Update the Sources table with anything cited

**What "full prose" means** — look at `docs/research/compression-and-cloud-cost.md` as the standard. Each section should:
- Open with a clear statement of what the design chose
- Present alternatives in a comparison table
- Analyse trade-offs with evidence (numbers, cited papers, production system examples)
- Close with a clear position: "**Verdict: X is correct. No change.**" or "**Status: Fixed. Y applied.**"

Do not apply changes to `design.md` during discussion — apply only after user confirms.

---

### 5. Apply Changes

For each confirmed finding:

1. Edit `design.md` directly
2. Update the design's **Decisions Made** table if the decision changed
3. Update the design's `Last updated` date
4. Mark the finding `Accepted` in the triage table (kept in the Recommendation section)
5. **Propagate to sub-phases** — if `sub-phases/` exists, check whether any sub-phase reproduces the changed content verbatim:
   - If a sub-phase duplicates the spec: update it to match design.md OR convert it to a reference (`See [Section Name](../design.md#anchor)`)
   - If a sub-phase already references design.md: no change needed
   - Prefer references over duplication — sub-phases own the implementation steps; design.md owns the spec

For **Bugs**: fix immediately after confirmation — no design changes needed, just correct the error.
For **Improvements/Gaps**: edit the relevant section of `design.md`.
For **Notes**: add to Security Considerations or a note callout in `design.md` — no structural change.
For **Deferred/Won't fix**: record rationale in `## Open Questions`.

---

### 6. Close Out

1. Write the `## Recommendation` section:
   - One paragraph verdict on the overall design quality
   - The triage summary table (# / Finding / Severity / Resolution)
2. Confirm `## Decisions` table has a row for every choice made
3. Confirm `## Sources` has an entry for every cited paper, RFC, or standard
4. Set `Status` to `Concluded` (all actionable findings resolved) or `Living document` (open questions remain)
5. Add an entry to `docs/research/README.md`
6. State: "Review complete. N changes applied to design.md, M changes applied to sub-phases."

---

## Document Quality Standard

The output document must be **readable as a standalone reference** — someone should be able to understand every decision without reading this conversation.

For each topic section, write at minimum:
- What the design chose (one sentence)
- A comparison table of the main alternatives (if 2+ alternatives exist)
- Analysis of the trade-offs with evidence
- A clear verdict or status line at the end

Poor (findings report style — do not write this):
> **Finding 3**: The HKDF salt is empty. RFC 5869 recommends a fixed salt. Fixed: changed to `b"arx-runa-v1"`.

Good (research document style — write this):
> ### Key Derivation
> **What the design chose**: HKDF-SHA256 with an empty salt...
> | Property | HKDF-SHA256 | BLAKE3 KDF |
> |...|...|...|
> The empty salt problem: RFC 5869 section 3.1 states...
> **Status: Fixed. Salt changed to `b"arx-runa-v1"`.**

---

## Severity Definitions

| Severity | Definition | Examples |
|----------|------------|---------|
| **Bug** | Will cause a compile error, runtime failure, or security break | Wrong crate version, renamed API, missing error variant |
| **Gap** | A protection or constraint that should exist but doesn't | Missing type-level enforcement, unhandled edge case, missing AAD |
| **Improvement** | A better approach exists; current approach is not wrong | RFC-recommended parameter, cleaner enforcement mechanism |
| **Note** | Worth documenting for the report or future reference; no change required | Known limitation, future upgrade path, related prior art |

---

## Rules

1. **Topics drive structure, not findings** — organize sections by design decision, not severity triage
2. **Full prose per section** — comparison tables, evidence, position statement; match `compression-and-cloud-cost.md` standard
3. **One finding at a time** — present, discuss, decide, write, advance
4. **Give a recommendation** — don't just present options; state which you'd choose and why
5. **Bugs first** — severity order for discussion: Bug → Gap → Improvement → Note
6. **Cite everything** — security claims require NIST, RFC, IACR ePrint, or peer-reviewed source
7. **Changes go in design.md** — the research doc captures reasoning; the design doc captures the outcome
8. **Don't over-fix** — Notes do not require design changes
9. **Standalone readable** — the finished document needs no conversation context to make sense
