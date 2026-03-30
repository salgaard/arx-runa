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

| Type | When to use |
|------|-------------|
| `decision` | A design or architectural choice was made, with alternatives considered |
| `discovery` | A non-obvious fact about the system, a library, or a constraint was found |
| `pivot` | The approach changed direction and the reason is worth documenting |
| `implementation` | A complex or non-obvious implementation detail was completed |
| `security-trade-off` | A security property was gained, limited, or traded against usability |
| `limitation` | A known constraint, out-of-scope item, or accepted risk was identified |

### Step 2 — Map to report sections

Assign one or more values to `report-sections`:

| Value | Maps to bachelor report section |
|-------|--------------------------------|
| `problem` | Problem formulation |
| `method` | Method and scientific foundation |
| `analysis` | Analysis and application |
| `discussion` | Discussion and recommendations |
| `conclusion` | Conclusion |

Guidance: `decision` → `method`+`discussion` · `discovery` → `analysis` · `pivot` → `method`+`discussion` · `implementation` → `analysis` · `security-trade-off` → `discussion` · `limitation` → `discussion`+`conclusion`

### Step 3 — Metadata

- `source`: `manual` (user invoked) or `agent` (proactive)
- `commit`: run `git rev-parse --short HEAD` (use `""` if no commit yet)
- Timestamp: run `date '+%Y-%m-%dT%H:%M:%S%z'` and `date '+%Y-%m-%d-%H%M%S'` for filename prefix
- Filename: `YYYY-MM-DD-HHMMSS-<kebab-case-title>.md` (3-6 words, save to `docs/report-log/`)

### Step 4 — Write the entry

Use `docs/report-log/_template.md`. Follow docs.md register rules: objective, no first-person, cite all claims with `<!-- CITE: suggested source -->`.

- 200–600 words — raw material, not final report prose
- Include **Alternatives considered** whenever alternatives were evaluated

### Step 5 — Update INDEX.md and confirm

Append to `docs/report-log/INDEX.md`:
`| YYYY-MM-DD | <type> | <Title> | <sections> | [filename](filename.md) |`

Output one line: `Logged report note: <title>`

### Step 6 — Optionally diagram

For `decision` or `implementation` entries: if the topic maps to a visualisable flow, structure, or schema, invoke the `diagram` skill. Check `docs/architecture/diagrams/INDEX.md` first to avoid duplicates.

---

## Listing entries (`/report-note list`)

1. Read all `.md` files in `docs/report-log/` (exclude `_template.md`, `INDEX.md`, `_compilation.md`)
2. Parse frontmatter, group by `type`, list titles with `report-sections` values
3. Show summary count per report-section

---

## Resolving sources (`/report-note resolve-sources`)

1. Read all report-log `.md` files, find all `<!-- CITE: ... -->` markers
2. For each: WebSearch → WebFetch to verify it supports the surrounding claim
3. If verified: replace with `<!-- SOURCE: Title — URL — "quote" -->`
4. If paywalled: `<!-- SOURCE: Title — URL — abstract verified, full text requires library access -->`
5. If unverifiable: leave `<!-- CITE: -->` unchanged
6. Report: N resolved, N paywalled, N unresolvable

Do not fabricate URLs. Only write `<!-- SOURCE: -->` after a successful WebFetch.

---

## Compiling entries (`/report-note compile`)

Use the `documentation-writer` agent to:

1. Read all report-log `.md` files; separate stubs from full entries
2. Group full entries by `report-sections`, listed chronologically under each report section:
   Problem formulation · Method · Analysis · Discussion · Conclusion
3. Flag: sections with zero entries (gaps), entries missing citations, stubs (candidates for deletion/expansion)
4. Estimate total character count (report limit: 72,000 chars)
5. Write result to `docs/report-log/_compilation.md`
