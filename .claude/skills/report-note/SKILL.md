---
name: report-note
description: >
  Capture a noteworthy moment for the bachelor report log. Invoke proactively
  when a design decision, technical discovery, pivot, security trade-off,
  complex implementation, or limitation occurs during a session — especially
  when a threat is declared out-of-scope, a weaker-but-simpler approach is
  chosen, or a known limitation is accepted. Also invoke proactively after a
  meaningful git commit (implementation milestone, design decision, or
  security-relevant change) — but NOT for routine fixes or formatting commits.
  Invokable manually by the user via /report-note <topic>.
---

Capture or manage bachelor report log entries in `docs/report-log/`.

## Arguments

- No argument or a topic string → capture a new report log entry
- `list` → show all entries grouped by type with counts per report-section
- `compile` → aggregate all entries into a structured outline mapped to report sections
- `resolve-sources` → find and verify real sources for all unresolved `<!-- CITE: -->` markers across all entries

---

## Capturing a new entry

### Step 1 — Classify the entry

Determine the type based on what happened:

| Type | When to use |
|------|-------------|
| `decision` | A design or architectural choice was made, with alternatives considered |
| `discovery` | A non-obvious fact about the system, a library, or a constraint was found |
| `pivot` | The approach changed direction and the reason is worth documenting |
| `implementation` | A complex or non-obvious implementation detail was completed |
| `security-trade-off` | A security property was gained, limited, or traded against usability |
| `limitation` | A known constraint, out-of-scope item, or accepted risk was identified |

### Step 2 — Map to report sections

Assign one or more of these values to `report-sections`:

| Value | Maps to bachelor report section |
|-------|--------------------------------|
| `problem` | Problem formulation |
| `method` | Method and scientific foundation (architecture choices, process, validation) |
| `analysis` | Analysis and application (theory applied to practice, sub-conclusions) |
| `discussion` | Discussion and recommendations (alternatives, trade-offs, limitations) |
| `conclusion` | Conclusion |

Guidance:
- `decision` → usually `method` + `discussion`
- `discovery` → usually `analysis`
- `pivot` → usually `method` + `discussion`
- `implementation` → usually `analysis`
- `security-trade-off` → usually `discussion`
- `limitation` → usually `discussion` + `conclusion`

### Step 3 — Determine source and get git context

Set `source` in the frontmatter based on how this skill was invoked:
- `source: manual` — user explicitly invoked `/report-note <topic>`
- `source: agent` — Claude invoked this skill proactively during a session

Run: `git rev-parse --short HEAD`

Use the result as the `commit` field. If no commit exists yet, use `""`.

### Step 4 — Get the current timestamp

Run: `date '+%Y-%m-%dT%H:%M:%S%z'`

Also capture `date '+%Y-%m-%d-%H%M%S'` for the filename prefix.

### Step 5 — Generate the filename

Format: `YYYY-MM-DD-HHMMSS-<kebab-case-title>.md`

Example: `2026-03-28-223045-xchacha20-nonce-strategy-decision.md`

Keep the kebab title short (3-6 words). Use the `docs/report-log/` directory.

### Step 6 — Write the entry 

Use the structure from `docs/report-log/_template.md`. Rules:

- Objective academic register — no first person ("I", "we", "the team")
- No subjective qualifiers ("interesting", "smart", "nice")
- Flag every factual claim that needs a source: `<!-- CITE: suggested source -->`
- Reference relevant standards where applicable: RFC 8439, RFC 5869, NIST SP 800-63,
  OWASP ASVS, draft-irtf-cfrg-xchacha
- Keep entries 200–600 words — this is raw material, not final report prose
- The **Alternatives considered** section is optional but highly valuable for the
  Discussion section of the report — include it whenever alternatives were evaluated

### Step 6.5 — Resolve sources

For each `<!-- CITE: ... -->` marker in the entry:

1. WebSearch for the suggested source to find the best URL
2. WebFetch the result to verify it directly supports the claim made in the entry
3. Replace the marker with a verified source block:
   ```
   <!-- SOURCE: Title — URL — "relevant quote or section that supports the claim" -->
   ```
4. If the source exists but full text is paywalled, write:
   ```
   <!-- SOURCE: Title — URL — abstract verified, full text requires library access -->
   ```
5. If no suitable source can be found or verified, leave the original `<!-- CITE: -->` marker unchanged

Do not fabricate URLs. Only replace a `<!-- CITE: -->` with `<!-- SOURCE: -->` after successfully fetching and verifying the page.

### Step 7 — Update INDEX.md

Append one row to `docs/report-log/INDEX.md`:

```
| YYYY-MM-DD | <type> | <Title> | <section1>, <section2> | [filename](filename.md) |
```

### Step 8 — Confirm briefly

At the end of the response, add one line:
`Logged report note: <title>`

Do not interrupt the user's workflow. Keep the mention short.

### Step 9 — Auto-generate diagram (optional)

If the entry type is `decision` or `implementation`, check whether the topic would benefit from a diagram. If yes, invoke the `diagram` skill:

| Topic involves | Diagram to generate |
|---|---|
| A flow (auth, encryption, sync, session) | `sequenceDiagram` |
| Module structure or architecture | `flowchart TD` |
| Data model or schema | `erDiagram` |
| State transitions | `stateDiagram-v2` |

Before creating, check `docs/architecture/diagrams/INDEX.md` — if a diagram for this topic already exists, run `/diagram update <filename>` instead of creating a duplicate.

Skip this step if the entry is abstract, minor, or does not map to a visualisable structure.

---

## Listing entries (`/report-note list`)

1. Read all `.md` files in `docs/report-log/` (exclude `_template.md`, `INDEX.md`, `_compilation.md`)
2. Parse the YAML frontmatter of each file
3. Group by `type`, then list titles with their `report-sections` values
4. Show a summary count per report-section

---

## Resolving sources (`/report-note resolve-sources`)

1. Read all `.md` files in `docs/report-log/` (exclude `_template.md`, `INDEX.md`, `_compilation.md`)
2. Find all `<!-- CITE: ... -->` markers (skip lines already using `<!-- SOURCE: -->`)
3. For each unresolved marker:
   - WebSearch for the suggested source
   - WebFetch the best result to verify it supports the surrounding claim
   - Replace with `<!-- SOURCE: Title — URL — "quote or section summary" -->` if verified
   - Replace with `<!-- SOURCE: Title — URL — abstract verified, full text requires library access -->` if paywalled
   - Leave as `<!-- CITE: ... -->` if nothing verifiable is found
4. Save the updated file
5. Report a summary: N resolved, N paywalled, N unresolvable, listing each

Do not fabricate URLs. Only write `<!-- SOURCE: -->` after a successful WebFetch confirms the page exists and is relevant.

---

## Compiling entries (`/report-note compile`)

Use the documentation-writer agent to:

1. Read all `.md` files in `docs/report-log/` (exclude `_template.md`, `INDEX.md`, `_compilation.md`)
2. Separate stubs (files containing `<!-- Auto-captured from commit`) from full entries
3. Group full entries by `report-sections` value
4. For each bachelor report section, list the relevant entries chronologically:
   - Problem formulation
   - Method and scientific foundation
   - Analysis and application
   - Discussion and recommendations
   - Conclusion
5. Flag:
   - Sections with zero entries as **gaps**
   - Entries missing citations (no `<!-- CITE:` markers in the body)
   - Stub files as **candidates for deletion or expansion**
6. Estimate total character count across all full entries
7. Write the result to `docs/report-log/_compilation.md`
