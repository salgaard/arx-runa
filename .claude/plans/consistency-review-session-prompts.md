# Consistency & Readability Review Session Prompts

Nine sessions, run in the recommended order below. Plan file: `.claude/plans/review-consistency-flows.md`.

**Date placeholder**: replace `YYYYMMDD` with today's date before running each session.

**Starting symbols are entry points, not scope boundaries.** Every prompt includes an "Enumerate before checking" block. Run those steps first — they build the complete set of relevant symbols, files, or call sites. Anything the enumeration finds is in scope, even if it is not in the listed starting symbols.

**Convention gap rule** (applies to every session): if code deviates from a project convention but is locally reasonable — e.g. an abbreviation used consistently within one module, or a structural split that makes sense even if it breaks the pattern — record it as a `[CONVENTION-GAP]` note rather than a finding. Format:
```
[CONVENTION-GAP] Short description
Convention says: …
Code does: …
Verdict: fix / accept / needs discussion
```
This keeps findings reserved for genuine inconsistencies and avoids flagging intentional local decisions as violations.

---

## Flow J — Error Type Hierarchy & From Chain Completeness

```
Review Flow J from `.claude/plans/review-consistency-flows.md`.
Write findings to `.claude/reviews/review-flow-j-YYYYMMDD.md`.
Use jcodemunch for all navigation. Do not commit anything.

Enumerate before checking:
1. search_symbols {"kind": "type", "pattern": "Error$"} across src-tauri/src/ — build the complete
   list of error types in the codebase. Any type not in the plan's background section is still in
   scope for From-chain and naming checks.
2. search_text {"query": "impl From<"} across src-tauri/src/ — enumerate all From edges; map each
   From<X> for Y relationship; any error type with no outgoing path to IpcError (directly or
   transitively) is a stranded type candidate.
3. search_text {"query": "\\.to_string\\(\\)|format!"} in src-tauri/src/ui/ with context_lines=2
   — hits inside From impls or IpcError construction sites are context-loss candidates.

Start by locating both SyncError definitions: one in src-tauri/src/sync/error.rs and one inline
in src-tauri/src/storage/cloud/sync.rs — determining which is canonical is the first task.
The src-tauri/src/sync/ module (mod.rs has 0 symbols, error.rs has 1) may be an abandoned stub;
investigate before assuming either definition is live.
```

---

## Flow M — Module Boundary & Naming Clarity

```
Review Flow M from `.claude/plans/review-consistency-flows.md`.
Write findings to `.claude/reviews/review-flow-m-YYYYMMDD.md`.
Use jcodemunch for all navigation. Do not commit anything.

Enumerate before checking:
1. get_file_tree {"path": "src-tauri/src/"} — scan the full module layout; identify any two modules
   or files with shared name prefixes not listed in the plan background. Add any found to the
   check list — the three known ambiguities are the starting point, not the complete scope.
2. search_text {"query": "pub mod |^mod "} across src-tauri/src/*/mod.rs — any module with 0
   exported symbols (confirmed via get_file_outline) is a near-empty module candidate.
3. search_text {"query": "pub use "} across all mod.rs files — enumerate re-exports; any type
   re-exported across a conceptual layer boundary is a leaking-export candidate.

Read the summary from `.claude/reviews/review-flow-j-YYYYMMDD.md` before starting — Flow J's
SyncError finding directly informs the sync/ module assessment here.
The three known ambiguities to resolve: (1) storage/sharing.rs vs sharing/, (2) sync/ vs
storage/cloud/sync.rs, (3) vault_header.rs vs vault_header_io.rs. Enumeration may surface more.
```

---

## Flow I — Naming Conventions & Abbreviation Audit

```
Review Flow I from `.claude/plans/review-consistency-flows.md`.
Write findings to `.claude/reviews/review-flow-i-YYYYMMDD.md`.
Use jcodemunch for all navigation. Do not commit anything.

Enumerate before checking:
1. Run git diff --name-only main (or git diff --name-only origin/master if on a feature branch)
   — this is the authoritative file list for this session. The files listed in the plan are
   examples from when it was written; the git output takes precedence. Any file in the git output
   not listed in the plan is still in scope.
2. get_file_outline on every file from step 1 — extract all symbol names and signatures without
   loading full bodies; scan outlines for violations before reaching for source.

Exempt from the no-abbreviations rule: Rust keywords, and acronyms AEAD, KDF, HKDF, IPC, EXIF,
BLAKE, HPKE, CTX, AAD, KEK, BIP.
Use get_symbol_source only on symbols that look suspicious from the outline scan.
```

---

## Flow K — IPC Command Surface Consistency

```
Review Flow K from `.claude/plans/review-consistency-flows.md`.
Write findings to `.claude/reviews/review-flow-k-YYYYMMDD.md`.
Use jcodemunch for all navigation. Do not commit anything.

Enumerate before checking:
1. search_text {"query": "#\\[tauri::command\\]"} across src-tauri/src/ui/ — build the complete,
   authoritative list of IPC-exposed functions. The plan lists six handler files as examples; any
   command in a file not on that list is still in scope for all consistency checks.
2. Cross-check the count from step 1 against the lib.rs invoke_handler registration — any
   discrepancy (annotated but not registered, or registered but not annotated) is a finding.
3. search_text {"query": "emit\\(|emit_to\\("} across src-tauri/src/ui/ with context_lines=1
   — enumerate all event emission sites; extract event name strings for the naming-consistency check.
4. search_text {"query": "State<'_, AppState>|State<AppState>"} across src-tauri/src/ui/
   — enumerate all handlers that take AppState; verify parameter position is consistent across
   all hits, not just the listed files.

Use get_file_outline on all handler files found in step 1 before reading any individual symbol
— build the full picture of signatures first.
```

*If context fills before all handler files are covered: start a second session with the same prompt but add "the following handler files were already reviewed in a prior session: [list them] — start from the next unreviewed file."*

---

## Flow L — High-Complexity Function Review

```
Review Flow L from `.claude/plans/review-consistency-flows.md`.
Write findings to `.claude/reviews/review-flow-l-YYYYMMDD.md`.
Use jcodemunch for all navigation. Do not commit anything.

Enumerate first — re-derive the candidate list from current state before reading any source:
1. Call get_hotspots {"top_n": 20, "min_complexity": 30} — the table in the plan is from
   2026-05-19 and may be stale; use current hotspot data as the authoritative list. Exclude Leptos
   files (src/*.rs, src/components/*.rs — high CC there is inherent to signal branching).
2. Compare current results against the plan table; note any function whose complexity changed
   significantly (>10 points) since the scan — a rise is higher priority, a drop means it may
   already be fixed.
3. Add any Rust-side function with CC >= 50 not in the plan table to the review set.

Before starting the source review, read summary sections from:
- `.claude/reviews/review-flow-e-YYYYMMDD.md` (covers recover_with_phrase and create_vault)
- `.claude/reviews/review-flow-c-YYYYMMDD.md` (covers sync_backup's IPC context)
if those sessions have been run — do not re-derive what they already established.

FileItem (src/vault.rs) and VaultCreationPage (src/auth.rs) are Leptos components — note as
accepted and focus time on the Rust-side hotspots from the refreshed list.
```

---

## Flow N — Nesting Breadth Scan (whole repo, no source reads)

```
Review Flow N from `.claude/plans/review-consistency-flows.md`.
Write output to `.claude/reviews/review-flow-n-YYYYMMDD.md`.
Use jcodemunch for all navigation. Do not read any function source bodies in this session
— metadata only. Do not commit anything.

Skip all files under src/ (Leptos frontend). Note known Leptos hotspots as accepted in a
one-line header at the top of the output file.

Step 1: call get_hotspots with top_n=150, min_complexity=1. Filter for max_nesting >= 5 in
Rust backend files (src-tauri/src/) only. Record these.

Step 2: for stable backend files absent from the hotspot list, call get_file_outline then
get_symbol_complexity on functions whose line span (end_line - line) exceeds 60. Priority
file list is in the flow definition. Add to the candidate list if max_nesting >= 5.

Stop adding files when the context budget warning appears in _meta. List any unreached files
at the bottom of the output so Flow O can optionally extend coverage.

Output is a candidate table, not findings — Flow O reads the source.
```

---

## Flow P — Error Handling & User-Facing Error Responses

```
Review Flow P from `.claude/plans/review-consistency-flows.md`.
Write findings to `.claude/reviews/review-flow-p-YYYYMMDD.md`.
Use jcodemunch for all navigation. Do not commit anything.

Phase 1 — backend error site sweep:
Run each search_text with file_pattern "src-tauri/src/**/*.rs" and context_lines=2. Classify
each hit as OK (test code or justified), REVIEW (plausible expect message), or GAP (bare
unwrap / let _ / .ok() with no justification). Queries:
  - \.unwrap\(\)         — bare panic, no message
  - \.expect\(           — flag if message is uninformative ("should work", "ok", "TODO")
  - \bpanic!\(           — explicit panic; is it reachable from non-test code?
  - \btodo!\(            — incomplete production path
  - \bunimplemented!\(   — same
  - let _ =              — silently discarded Result or Option
  - \.ok\(\)             — Result→Option; error thrown away
  - map_err\(\|_\|       — maps error but discards original

Write results as a table. Only write full finding blocks for GAP entries.

Phase 2 — IpcError message quality:
Start with get_file_outline on src-tauri/src/ui/error.rs, then read the From impl bodies.
Cross-reference the error types found in Flow J's enumeration (if that session has been run)
to ensure all types are covered here, not just the ones listed in the plan.
Flag if more than 3 distinct source error variants collapse to the same generic IpcError message.

Phase 3 — frontend error propagation:
Read invoke_command and invoke_command_with_channel in src/invoke/ first — this determines
whether individual call sites need their own error handling. Then check src/components/toast.rs.
Then sample src/auth.rs, src/vault.rs, src/settings.rs for per-call error handling patterns.

If context fills before Phase 3: note in the summary which phase was reached and start a
follow-up session beginning from Phase 3.
```

---

## Flow O — Nesting Source Review

```
Review Flow O from `.claude/plans/review-consistency-flows.md`.
Write findings to `.claude/reviews/review-flow-o-YYYYMMDD.md`.
Use jcodemunch for all navigation. Do not commit anything.

Start by reading `.claude/reviews/review-flow-n-YYYYMMDD.md` to get the candidate list
— do not re-run the full scan.

Validate the candidate list before reading source:
For each function in Flow N's candidate list, call get_symbol_complexity before get_symbol_source.
Compare current nesting against N's recorded values:
  - Nesting dropped significantly → note as "resolved since Flow N", skip source read
  - Nesting rose → flag as higher priority, move to front of queue
  - Any function from N's "Files not reached" section with estimated nesting >= 7 → add to queue

Process validated candidates in descending current-nesting order. Call get_symbol_source for each.
Stop when context budget warning appears — list remaining candidates in the summary so a follow-up
session can continue.

If Flow L has already been run, read its summary from `.claude/reviews/review-flow-l-YYYYMMDD.md`
— focus here on structural readability only, not correctness.
```


---

## Flow Q — Dead Code & Deferred Work Sweep

`
Review Flow Q from .claude/plans/review-consistency-flows.md.
Update docs/notes/dead-code-audit.md in place. Do not commit anything.

Confirm the current phase with the user before proceeding. If all phases are complete,
every deferred-work marker and every unsuppressed #[allow(dead_code)] is real debt
requiring a verdict. If a phase is still in progress, record those markers as intentional
and exclude them from the action list.

Enumerate before checking (run all four before evaluating any individual item):
1. search_text {"query": "#[allow(dead_code)]", "file_pattern": "src-tauri/src/**/*.rs",
   "context_lines": 2} — build the complete list of suppressed symbols; each is a candidate.
2. search_text {"query": "#[allow(unused", "file_pattern": "src-tauri/src/**/*.rs",
   "context_lines": 2} — catches unused_imports, unused_variables, unused_mut suppressions.
3. search_text {"query": "// Phase |// TODO(phase", "file_pattern": "src-tauri/src/**/*.rs"}
   — enumerate every phase-tagged deferred comment; check each against current phase state.
4. search_text {"query": "#[cfg_attr.*allow(dead_code)", "file_pattern": "src-tauri/src/**/*.rs",
   "context_lines": 2} — cfg-gated suppressions; evaluate separately for architectural correctness.

For every item found, verify with grep before assigning a verdict:
  search_text {"query": "FUNCTION_NAME", "file_pattern": "src-tauri/src/**/*.rs"}
Zero grep hits + zero import-graph edges = confirmed dead. Any grep hit = live code with a
stale suppression (the suppression is the only problem, not the code).

WARNING: Do NOT rely solely on find_dead_code or get_dead_code_v2. These tools use the import
graph only — they miss method calls within the same crate (e.g. self.method()). Grep is
the authoritative check for call sites.

Classify every item into exactly one category:
  A — True dead: zero grep call sites + zero import edges → DELETE or WIRE UP
  B — Stale suppression: grep finds call sites, but #[allow(dead_code)] is present → REMOVE attribute
  C — Cfg-gated false positive: cfg(test)/cfg(not(test)) split makes one branch look unused → KEEP
  D — Deferred-work marker: phase/TODO comment, phase still active → KEEP (note in summary)
  E — Test module import noise: #![allow(unused_imports)] in cfg(test) → LOW PRIORITY

For every Category A item, assign a verdict — no item may be left without one:
  DELETE — vestigial code, no future use, cross-reference design docs to confirm
  WIRE UP — complete implementation missing only a caller or Tauri command registration;
             specify exactly what wiring is needed

For any item whose name suggests a design feature (revoke, share, sync, auth, vault, key),
look up the relevant design doc in docs/architecture/designs/ before assigning DELETE.
If a design doc lists the feature as a deliverable and the implementation is complete,
the verdict is WIRE UP, not DELETE.

Output format for .claude/reviews/review-flow-q-YYYYMMDD.md:
  Section A: True Dead Code — one entry per item, verdict, location, rationale
  Section B: Stale Suppressions — one entry per item, "remove #[allow(dead_code)] at line N"
  Section C: Correct Suppressions — brief list with justification for each
  Section D: Deferred Work Markers — one entry per phase tag still active
  Section E: Test Import Noise — list only; low priority
  Priority action table at end: ordered DELETE/WIRE UP/REMOVE tasks for the next fix session
`

---
