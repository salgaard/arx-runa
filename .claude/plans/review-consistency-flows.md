---
title: "Consistency & Readability Review Plan"
created: "2026-05-19"
status: active
tags: [consistency, naming, error-types, api-surface, readability]
---

# Consistency & Readability Review Plan

Focused review sessions targeting structural consistency, naming hygiene, and API surface coherence. Each session loads only this section and its flow block, navigates via jcodemunch (no full-file reads except the file being edited), and writes findings to `.claude/reviews/review-<flow-id>-<YYYYMMDD>.md`.

## How to start a session

Tell Claude at session start:

> "Review **Flow X** from `.claude/plans/review-consistency-flows.md`. Write findings to `.claude/reviews/review-<flow-id>-YYYYMMDD.md`. Use jcodemunch for all navigation. Do not commit anything."

Claude should then:
1. `resolve_repo {"path": "."}` to get the repo identifier
2. `plan_turn` with the flow's query string (listed in each flow)
3. **Enumerate before checking** — each session lists specific discovery steps (`search_symbols`, `search_text`, `git diff`) to run first; build the complete set of relevant symbols or files across the codebase before evaluating any of them. The starting symbol / starting file lists are entry points and examples, not a fixed scope. Anything not in that list is still in scope if the enumeration finds it.
4. Navigate each enumerated symbol via `get_file_outline` / `get_symbol_source` / `search_symbols`
5. Write findings as it goes — do not accumulate and dump at end

---

## Session I — Naming Conventions & Abbreviation Audit

**Core question**: Does every symbol, parameter, and file name in the modified surface area conform to the project's hard naming rule (no abbreviations; `chunk_index` not `chunk_idx`; Rust keywords and acronyms — AEAD, KDF, HKDF — exempt)?

**plan_turn query**: `"symbol names parameter names abbreviations naming conventions file names module names consistency"`

**Scope**: All files modified on the current branch. Run `git diff --name-only main` (or `git diff --name-only origin/master` if on a feature branch) at session start — that output is the authoritative file list. The starting files below are examples from the time this plan was written; the git output takes precedence. Any file in the git output not listed below is still in scope.

**Starting files** (example list — always re-derive from `git diff --name-only`):
- `src-tauri/src/auth/ceremonies/` — all modified ceremony files
- `src-tauri/src/crypto/types/mod.rs`
- `src-tauri/src/ui/auth_commands.rs`, `src-tauri/src/ui/sync_commands.rs`, `src-tauri/src/ui/error.rs`
- `src-tauri/src/storage/cloud/sync.rs`, `src-tauri/src/storage/cloud/destination_session.rs`
- `src-tauri/src/storage/vault_ops/` — all modified vault ops files

**Enumerate first** (run before checking any symbol):
- `git diff --name-only main` (or `origin/master`) — get the exact list of modified files; this is the scope for this session
- `get_file_outline` on every file in that list — extracts all symbol names and signatures without reading full bodies; scan outlines for violations before reaching for source

**Known inconsistency to verify first** (spotted in structure scan):
- `src-tauri/src/auth/device_monitor/windows_impl.rs` — uses `_impl` suffix; sibling files are `linux.rs` and `macos.rs` with no suffix. Verify whether this is intentional (e.g. the Windows version wraps an inner `_impl` detail) or a naming drift that should be `windows.rs`.

**What to verify**:

| Check | Pass condition |
|---|---|
| No non-exempt abbreviation in any modified symbol name (from the git-derived file list, not just the examples above) | No `idx`, `cfg`, `msg`, `ctx`, `buf`, `res`, `err`, `cb`, `fn_` prefix, `_cnt`, `_sz`, `_len` used as standalone name segments (compound words like `chunk_index`, `file_key` are fine) |
| No non-exempt abbreviation in any modified function parameter name | Same rule applies at parameter scope |
| Platform file naming consistent across `device_monitor/` | Either all have suffix or none do; rationale documented if intentional |
| `get_file_outline` each modified file and scan all parameter signatures | Flag any `param` or single-letter names outside closures |
| Enum variant names are full words | No `Err` variant named `Io` (should be `IoError` or mapped to descriptive name), no `Cfg` |
| Struct field names are full words | Check all modified `types/` files |

**Finding severity guide**: Abbreviation in public API symbol = **medium** (breaks CLAUDE.md rule, hurts readability). Abbreviation in internal parameter = **low**. Inconsistent platform file naming = **low** (structural clarity). Single-letter variable in non-closure context = **low**.

**Output file**: `.claude/reviews/review-flow-i-YYYYMMDD.md`

---

## Session J — Error Type Hierarchy & From Chain Completeness

**Core question**: Do the 15+ error types form a coherent, complete conversion graph? Are there duplicate error types? Does any conversion silently drop context that would be useful for debugging?

**Background**: The codebase defines at minimum: `CryptoError`, `StorageError`, `AuthenticationError`, `KeySourceError`, `SharingError`, `CloudTransportError`, `MemoryLockError`, `VaultHeaderError`, `VaultHeaderSyncError`, `ManifestBackupSyncError`, `SqlcipherOpenError`, `DownloadTaskError`, `B2ApiError`, `GdriveApiError`, `IpcError` — plus `SyncError` which appears in **two** separate locations:
- `src-tauri/src/sync/error.rs` (the `sync/` top-level module, which is otherwise nearly empty)
- `src-tauri/src/storage/cloud/sync.rs` (inline definition at line 101)

This duplication is the primary target of this session.

**plan_turn query**: `"error type From conversion chain IpcError StorageError AuthenticationError SyncError duplicate definition"`

**Starting symbols** (entry points — enumeration finds the full error type population):
- `SyncError` in `src-tauri/src/sync/error.rs` — `get_symbol_source` (the standalone module)
- `SyncError` in `src-tauri/src/storage/cloud/sync.rs` — `search_symbols` kind=type, file_pattern `storage/cloud/sync.rs`
- `src-tauri/src/sync/mod.rs` — `get_file_outline` (what does the nearly-empty `sync/` module actually export?)
- `src-tauri/src/ui/error.rs` — `get_file_outline` to see all `From` impls on `IpcError`
- `src-tauri/src/storage/error.rs` — `get_file_outline` to see all `From` impls on `StorageError`
- `src-tauri/src/auth/error.rs` — `get_file_outline` for `AuthenticationError` and `KeySourceError`

**Enumerate first** (run before checking any From chain):
- `search_symbols {"kind": "type", "pattern": "Error$"}` across `src-tauri/src/` — build the complete list of error types in the codebase; any type not in the background section above is still in scope for From-chain and naming checks
- `search_text {"query": "impl From<"}` across `src-tauri/src/` — enumerate all From impls; map each `From<X> for Y` edge; any error type with no outgoing From edge to `IpcError` (directly or transitively) is a stranded type candidate
- `search_text {"query": "\.to_string\(\)|format!\("}` in `src-tauri/src/ui/` with `context_lines=2` — any hit inside a `From` impl or an `IpcError` construction site is a context-loss candidate

**What to verify**:

| Check | Pass condition |
|---|---|
| `SyncError` is defined exactly once — determine which definition is canonical, whether the `sync/` module is a stub or dead code | One definition used; the other either removed or justified as intentional alias |
| `src-tauri/src/sync/` module purpose is clear | Either populated with intent, or documented as a placeholder; `sync/error.rs` having 1 symbol and `sync/mod.rs` having 0 is suspicious |
| Every error type from the enumeration has a From path to `IpcError` (directly or via `StorageError` / `AuthenticationError`) — no stranded types | Complete `From` chain across all error types, not just the listed ones |
| `IpcError` has `From<StorageError>` (or equivalent) — no storage error reaching the UI as `.to_string()` or `format!()` only | Complete `From` chain |
| `IpcError` has `From<AuthenticationError>` | Complete `From` chain |
| `IpcError` has `From<SharingError>` | Complete `From` chain |
| `StorageError` has `From<CryptoError>` | Complete `From` chain |
| `StorageError` has `From<SqlcipherOpenError>` | Complete `From` chain |
| No `From` impl maps a typed error to `IpcError::Internal(e.to_string())` where a dedicated `IpcError` variant exists or should exist — checked across all From impls from enumeration | No silent context loss |
| Every error type from enumeration uses consistent variant naming — `PascalCase` variants with descriptive nouns, not `FailedToXyz` wrapper names | Naming consistency |

**Finding severity guide**: Duplicate type causing silent mismatch = **high**. Dead module containing the only definition = **high**. Missing `From` causing `.to_string()` loss of structured error = **medium**. Stranded error type with no `From` path = **medium**. Inconsistent variant naming = **low**.

**Output file**: `.claude/reviews/review-flow-j-YYYYMMDD.md`

---

## Session K — IPC Command Surface Consistency

**Core question**: Do all IPC command handlers follow the same structural contract? A consistent surface means the frontend can predict signatures, and a future developer can read any handler and understand it without cross-referencing others.

**Background**: The `ui/` module has ~40 IPC command handlers across `auth_commands.rs`, `sync_commands.rs`, `file_commands.rs`, `destination_commands.rs`, `sharing_commands.rs`, `shell_commands.rs`. Each returns via `IpcError`. There are also 24 separate response type files in `ui/types/` — one struct per file.

**plan_turn query**: `"IPC command handler tauri command signature return type AppState parameter order async event emit"`

**Starting symbols** (entry points — enumeration finds the full command surface):
- `src-tauri/src/ui/auth_commands.rs` — `get_file_outline` (auth handler signatures)
- `src-tauri/src/ui/sync_commands.rs` — `get_file_outline` (sync handler signatures)
- `src-tauri/src/ui/file_commands.rs` — `get_file_outline`
- `src-tauri/src/ui/destination_commands.rs` — `get_file_outline`
- `src-tauri/src/ui/sharing_commands.rs` — `get_file_outline`
- `src-tauri/src/ui/commands_common.rs` — `get_file_outline` (shared helpers; `sanitise_password` lives here)
- `src-tauri/src/ui/state.rs` — `get_file_outline` (what `AppState` looks like)
- `src-tauri/src/lib.rs` — confirm all handlers are registered (cross-check count)

**Enumerate first** (run before checking any handler):
- `search_text {"query": "#\\[tauri::command\\]"}` across `src-tauri/src/ui/` — build the **complete, authoritative list** of IPC-exposed functions; cross-reference with `lib.rs` invoke_handler registration; any command not in the starting files above is still in scope for all consistency checks
- `search_text {"query": "emit\\(|emit_to\\("}` across `src-tauri/src/ui/` with `context_lines=1` — enumerate all event emission sites; extract event name strings for the naming-consistency check
- `search_text {"query": "State<'_, AppState>|State<AppState>"}` across `src-tauri/src/ui/` — enumerate all handlers that take AppState; check parameter position consistently across all hits, not just the files listed above

**What to verify**:

| Check | Pass condition |
|---|---|
| All handlers that accept `State<'_, AppState>` place it in the same parameter position — confirmed across all handlers from `search_text` enumeration, not just the listed files | Consistent position across all handlers |
| All handlers returning `Result<T, IpcError>` — confirm no handler returns bare `T` or `()` when it can fail | No swallowed errors |
| Handlers emitting Tauri events use consistent event name strings — checked across all emission sites from enumeration; no typos, mixed case, `snake_case` vs `kebab-case` inconsistency | Uniform event naming scheme |
| `ui/types/` response structs all derive the same set of traits (`Serialize`, `Deserialize`, etc.) — no struct missing a derive that siblings have | Uniform derives |
| No handler constructs `IpcError` variants inline that `commands_common.rs` already provides as helpers | DRY on error construction |
| Progress-emitting handlers all follow the same emit → complete pattern (no handler emits progress but returns before emitting a terminal event) | Consistent progress lifecycle |
| Handlers that mutate vault state all acquire the session lock in the same order — no handler acquires state locks in a different order that could deadlock | Lock ordering |
| `src-tauri/src/lib.rs` registered command count exactly matches the `#[tauri::command]` count from enumeration — no unregistered or phantom commands | No gap between annotation and registration |

**Finding severity guide**: Inconsistent lock acquisition order = **high** (deadlock risk). Handler returning `()` for a fallible operation = **medium**. Inconsistent event naming = **medium** (breaks frontend subscriptions). Missing derive on response type = **low**. Wrong parameter position = **low** (cosmetic but confusing).

**Output file**: `.claude/reviews/review-flow-k-YYYYMMDD.md`

---

## Session L — High-Complexity Function Review

**Core question**: Do the five highest-complexity functions contain logic that is correct but hard to audit, or do they hide actual gaps — missed error paths, untested branches, or invariant violations buried in complexity?

**Background** (from repo health scan, as of 2026-05-19 — treat as a starting point, not the authoritative list):

| Function | File | Cyclomatic | Nesting |
|---|---|---|---|
| `FileItem` | `src/vault.rs:285` | 78 | 13 |
| `recover_with_phrase` | `src-tauri/src/auth/ceremonies/recover_with_phrase.rs:33` | 70 | 9 |
| `VaultCreationPage` | `src/auth.rs:381` | 63 | 9 |
| `sync_backup` | `src-tauri/src/ui/sync_commands.rs:1167` | 63 | 9 |
| `create_vault` | `src-tauri/src/auth/ceremonies/create.rs:40` | 58 | 6 |

Note: `FileItem` and `VaultCreationPage` are Leptos reactive components — high CC is partially inherent to signal branching. The Rust-side functions (`recover_with_phrase`, `sync_backup`, `create_vault`) are more concerning.

**plan_turn query**: `"recover_with_phrase sync_backup create_vault complex function error branch coverage early return"`

**Enumerate first — re-derive the candidate list from current state**:
- Call `get_hotspots {"top_n": 20, "min_complexity": 30}` at session start — the table above is from 2026-05-19 and may be stale; use current hotspot data as the authoritative list
- Exclude Leptos files (`src/*.rs`, `src/components/*.rs`) from the candidate list — their CC is inherent to signal branching
- Compare current hotspot results against the table above; note any function whose complexity changed significantly (> 10 points) since the scan — that is itself a signal worth recording
- Add any new Rust-side function with CC ≥ 50 to the review set, even if not in the table above

**Starting symbols** (re-confirm line numbers against current hotspot output before reading):
- `recover_with_phrase` — `get_symbol_source` in `src-tauri/src/auth/ceremonies/recover_with_phrase.rs`
- `sync_backup` — `get_symbol_source` in `src-tauri/src/ui/sync_commands.rs`
- `create_vault` — `get_symbol_source` in `src-tauri/src/auth/ceremonies/create.rs`
- Any additional function from the refreshed hotspot list with CC ≥ 50
- Do NOT attempt to read `FileItem` or `VaultCreationPage` in full — Leptos component CC is UI branching; note complexity as accepted and move on

**What to verify** (for each Rust-side hotspot function):

| Check | Pass condition |
|---|---|
| Every `?` early-return in `recover_with_phrase` leaves no partially-committed state (no dangling DB rows, no mlock'd memory not freed) | Clean early-exit invariant |
| `recover_with_phrase` — does complexity come from error handling or from actual branching logic? If branching: are all branches covered by the existing ceremony tests in `src-tauri/src/auth/ceremonies/`? | Branches tested or documented |
| `sync_backup` — identify the top 3 branching points (largest `match` / `if-let` chains). Are any branches reachable only by cloud error injection, with no test? | Note uncovered paths |
| `sync_backup` — does it hold any lock across an `await` on a cloud operation? | Async + lock = deadlock risk |
| `create_vault` — each early-return path: confirm the vault is left in a consistent state (no partial SQLite schema, no orphaned key material) | Atomicity on failure paths |
| For any function: if complexity is > 30 and no `#[cfg(test)]` module exists in that file, flag as untested complexity | Test gap signal |
| Note whether complexity is essential (inherent to the operation) or accidental (nested `match` that could be a helper) — record as **observation**, not a required fix | Readability signal only |

**Finding severity guide**: Lock held across network `await` = **high** (deadlock risk). Early return leaving partial state = **high** (data integrity). Uncovered error-injection branch = **medium**. Accidental complexity with no tests = **medium**. Essential complexity fully tested = **observation** (no finding, just note).

**Output file**: `.claude/reviews/review-flow-l-YYYYMMDD.md`

---

## Session M — Module Boundary & Naming Clarity

**Core question**: Are module names and file placement unambiguous? Can a new contributor navigate from a concept to the right file without confusion?

**Background**: Three specific structural ambiguities spotted in the codebase scan — but the session should not stop at just these three. Enumeration may surface additional ones.

1. **`storage/sharing.rs` vs `sharing/`**: There is `src-tauri/src/storage/sharing.rs` (58 symbols — the largest single file in `storage/`) alongside a top-level `sharing/` module. A contributor looking for "sharing code" might check either location and find something completely different in each.

2. **Near-empty `sync/` module**: `src-tauri/src/sync/` contains only `error.rs` (1 symbol) and `mod.rs` (0 symbols). This alongside `storage/cloud/sync.rs` (49 symbols) creates the same ambiguity for "sync code."

3. **`vault_header.rs` vs `vault_header_io.rs`**: Two files in `storage/cloud/` share a prefix. The split is meaningful but not self-evident from names alone.

**plan_turn query**: `"module organization storage sharing sync cloud vault_header file placement naming clarity"`

**Enumerate first** (run before checking any specific ambiguity):
- `get_file_tree {"path": "src-tauri/src/"}` — get the full module layout; scan for any two modules or files with shared name prefixes not listed in the background above; add any found to the check list
- `search_text {"query": "pub mod |mod "}` across `src-tauri/src/*/mod.rs` — find all module declarations; any `mod` that is declared but has 0 exported symbols (via `get_file_outline`) is a near-empty module candidate
- `search_text {"query": "pub use "}` across all `mod.rs` files — enumerate all re-exports; any type re-exported from a module boundary that is defined in a different conceptual layer is a leaking-export candidate

**Starting symbols** (known ambiguities — enumeration may find more):
- `src-tauri/src/storage/sharing.rs` — `get_file_outline` (what does this file actually do vs. the `sharing/` module?)
- `src-tauri/src/sharing/mod.rs` — `get_file_outline` (what does top-level `sharing/` export?)
- `src-tauri/src/sync/mod.rs` — `get_file_outline` (is this module intentional or abandoned?)
- `src-tauri/src/sync/error.rs` — `get_symbol_source` on its `SyncError` (what does it define that `cloud/sync.rs` doesn't?)
- `src-tauri/src/storage/cloud/vault_header.rs` — `get_file_outline` (model/types vs. I/O operations)
- `src-tauri/src/storage/cloud/vault_header_io.rs` — `get_file_outline`

**What to verify**:

| Check | Pass condition |
|---|---|
| `storage/sharing.rs` and `sharing/` are semantically distinct and non-overlapping — document what each owns | Each owns a clearly different concern (e.g. DB persistence vs. package crypto); a one-line comment in each `mod.rs` states the boundary |
| `src-tauri/src/sync/` module: determine if it is (a) a future placeholder, (b) dead code, (c) an intentional thin wrapper | Module has a doc comment in `mod.rs` stating its role, or is identified for removal |
| `vault_header.rs` vs `vault_header_io.rs` split follows a consistent `types / IO` pattern used elsewhere in the codebase | Pattern is either consistent (same split appears in other pairs) or the files should be renamed to make the distinction explicit |
| All near-empty modules found by enumeration have a doc comment in `mod.rs` stating their role — or are flagged for removal | No silent placeholder modules |
| `storage/cloud/mod.rs` re-exports match what callers actually import — no leaking internal types through `pub use` that should be crate-private | Exports are minimal and intentional; same check for any other `mod.rs` with unexpectedly broad `pub use` from enumeration |
| Each top-level module under `src-tauri/src/` owns exactly one concept — no module whose name could equally describe two different things (check enumeration results, not just the three known ones) | Module names are unambiguous given a fresh-eyes read |
| `src-tauri/src/auth/device_monitor/windows_impl.rs` naming vs `linux.rs` / `macos.rs` — record the reason for `_impl` suffix or flag for rename | Either intentional and explained, or renamed to `windows.rs` |

**Finding severity guide**: Two modules with overlapping conceptual scope and no boundary comment = **medium** (contributor confusion). Near-empty module with no doc comment = **low** (structural noise). Inconsistent file naming within a directory = **low**. Internal type leaked through `pub use` = **low**.

**Output file**: `.claude/reviews/review-flow-m-YYYYMMDD.md`

---

## Session N — Nesting Breadth Scan (whole repo, no source reads)

**Core question**: Which functions across the entire Rust codebase have nesting deep enough to warrant a closer look? This pass produces a ranked candidate list — it reads no source bodies, only metadata.

**Why two passes**: Reading full function bodies fills context quickly. Pass 1 (this session) stays fast by using complexity metadata only, covering all 228 Rust files. Pass 2 (Session O) then reads source for the flagged candidates only.

**Leptos files — exclude entirely**: Any file under `src/` (not `src-tauri/src/`) is a Leptos frontend file where nesting is inherent to signal branching. Skip all symbols in: `src/*.rs`, `src/components/*.rs`. Note the known Leptos hotspots as accepted in a one-line header and move on.

**plan_turn query**: `"nesting depth cyclomatic complexity all functions rust backend symbol complexity scan"`

**How to run the scan**:

1. Call `get_hotspots` with `top_n=150, min_complexity=1` — this returns nesting data for the most actively-changed 150 functions. Filter results for `max_nesting ≥ 5` and exclude Leptos files. Record these.

2. For Rust files in `src-tauri/src/` that are **absent from the hotspot list** (stable files with low churn), call `get_file_outline` on the files most likely to contain long functions — prioritise by symbol count:
   - `src-tauri/src/storage/sqlcipher.rs` (121 symbols)
   - `src-tauri/src/storage/sharing.rs` (58 symbols)
   - `src-tauri/src/sharing/packages.rs` (56 symbols)
   - `src-tauri/src/sharing/revocation.rs` (30 symbols)
   - `src-tauri/src/sharing/cloud.rs` (28 symbols)
   - `src-tauri/src/storage/cloud/wizard.rs` (43 symbols)
   - `src-tauri/src/storage/cloud/rclone.rs` (37 symbols)
   - `src-tauri/src/storage/vault_ops/download_file.rs` (32 symbols)
   - `src-tauri/src/storage/vault_ops/delete_file.rs` (33 symbols)
   For any function whose line span (`end_line − line`) exceeds 60, call `get_symbol_complexity` to get its nesting score. Add to the candidate list if `max_nesting ≥ 5`.

3. Stop adding files when context budget warning appears in `_meta`.

**Output format** — write to `.claude/reviews/review-flow-n-YYYYMMDD.md` as a table, not findings:

```
## Accepted (Leptos — skip in Pass 2)
DestinationItem (nesting=16), ReceivedShareItem (15), FileItem (13), LoginPage (13), ...

## Candidates for Pass 2 (nesting ≥ 5, Rust backend only)
| Function | File | Line | CC | Nesting | Source: hotspot / outline scan |
|---|---|---|---|---|---|
| insert_chunks | src-tauri/src/storage/sqlcipher.rs | 1440 | 23 | 9 | hotspot |
| ...

## Files not reached (context limit)
List any files from step 2 not reached so Pass 2 can optionally extend coverage.
```

**Output file**: `.claude/reviews/review-flow-n-YYYYMMDD.md`

---

## Session O — Nesting Depth & Loop Structure (source review)

**Core question**: For every candidate from Session N, does the nesting or loop structure make the code hard to follow — and is there a concrete, minimal fix?

**Prerequisite**: Session N must be complete. Start by reading `.claude/reviews/review-flow-n-YYYYMMDD.md` to get the candidate list. Do not re-run the full scan.

**Distinction from Flow L**: Flow L asks "do correctness issues hide in complexity?" This flow asks "does the structure make correct code hard to read?" If Flow L has already been run, read its summary and focus here on structure only — do not re-derive correctness findings.

**plan_turn query**: `"nested if let match loop early return arrow anti-pattern extract helper flatten control flow"`

**Enumerate first — validate the candidate list before reading source**:
- For each function in Session N's candidate list, call `get_symbol_complexity` before calling `get_symbol_source`; compare current complexity against N's recorded values. If a function's nesting has dropped significantly (e.g. it was already refactored), note this as "resolved since N" and skip reading its source. If complexity increased, flag it as higher priority.
- Any function from N's "Files not reached" section that had estimated nesting ≥ 7 should be added to the start of this session's queue.

**How to run**: Process validated candidates from Session N in descending current-nesting order (per `get_symbol_complexity`). For each, call `get_symbol_source`. Stop when context budget warning appears — list remaining candidates in the summary so a follow-up session can continue.

**What to look for** (for each function):

| Pattern | What to record |
|---|---|
| Nested `if let` chains (3+ levels) where `?` with `.ok_or(...)` would flatten them | Location, the chain, and the `?`-based replacement |
| `loop { match ... }` or `while let` where a standard iterator (`map`, `filter_map`, `try_for_each`) would be clearer | Location and the combinator that applies |
| A `match` arm whose body > 10 lines and contains its own `if`/`match` — extraction candidate | Function name, arm pattern, suggested helper name |
| A function where negating a condition enables an early `return Err(...)` that removes one full nesting level | Location and the inverted condition |
| Nested loops (`for` inside `for`) where the inner loop body is non-trivial | Whether the invariant is locally obvious without reading both levels |
| `unwrap()` or `expect()` inside a nested block, outside test code, without a documented panic justification | Flag as **medium** — robustness gap, not just style |

**Finding severity guide**: `unwrap()` in non-test non-panic-documented context = **medium**. Arrow anti-pattern (3+ nested `if let` where `?` applies) = **low**. Heavy match arm extractable to helper = **low**. Nested loop with unclear invariant = **low**. Nesting that reads fine despite depth = **observation** only.

**Output file**: `.claude/reviews/review-flow-o-YYYYMMDD.md`

*If context fills before all candidates are covered: start a follow-up session with "Continue Flow O from `.claude/reviews/review-flow-n-YYYYMMDD.md`. The following candidates were already reviewed: [list them]. Start from the next unreviewed entry."*

---

## Session P — Error Handling & User-Facing Error Responses

**Core question**: Two related questions in one session. (1) In backend non-test code, is every error propagated, panicked, or silently dropped with clear justification? (2) When errors reach the frontend, does the user receive a useful, actionable response — or does the error disappear silently or surface as a generic message?

**Distinction from existing flows**:
- Flow J covers error *type structure* and `From` chain completeness — not individual error sites.
- Flow O flags `unwrap()` only as a side-effect of reading high-nesting functions — not a systematic sweep.
- Security flows C and G cover what error strings must *not* contain (no internal detail). This flow covers what they *should* contain.

**Background from pre-scan**:
- `IpcError` serializes to `{"kind": "camelCaseVariant", "message": "user-friendly string"}` — shape is correct and tested.
- Multiple `From` impls fall through to `IpcError::InternalError("An error occurred".into())` — a catch-all that is safe but not actionable.
- Frontend uses `invoke_command` and `invoke_command_with_channel` wrappers in `src/invoke/` — if these don't centrally show a toast on error, every `if let Ok(...) = invoke_command(...)` call site silently drops failures.

**plan_turn query**: `"error handling unwrap expect panic swallow invoke_command error response toast user feedback IpcError message"`

---

### Phase 1 — Backend error site sweep (text search, no source reads)

Run each `search_text` call with `context_lines=2`, `file_pattern="src-tauri/src/**/*.rs"`, and `max_results=50`. For each hit, classify using the context lines:

| Label | Meaning |
|---|---|
| `OK` | Inside `#[cfg(test)]`, `#[test]` fn, or has a comment explaining why panic/drop is safe |
| `REVIEW` | `.expect("meaningful invariant message")` — plausible but unverified |
| `GAP` | Bare `.unwrap()`, `let _ =`, `.ok()`, or `map_err(\|_\|` in production code with no justification |

Searches to run:

| Query | Regex | Concern |
|---|---|---|
| `\.unwrap\(\)` | yes | Bare panic, no message |
| `\.expect\(` | yes | Panic with message — flag if message is uninformative (`"should work"`, `"ok"`, `"TODO"`) |
| `\bpanic!\(` | yes | Explicit panic — is it reachable from a non-test path? |
| `\btodo!\(` | yes | Incomplete code in production path |
| `\bunimplemented!\(` | yes | Same |
| `let _ =` | yes | Silently discarded `Result` or `Option` |
| `\.ok\(\)` | yes | `Result → Option` — error thrown away; flag if the original `Result` could propagate |
| `map_err(\|_\|` | yes | Maps error but discards original — losing debugging context |

Write results as a table in the output file. Only write full finding blocks for clear `GAP` entries.

---

### Phase 2 — IpcError message quality

**Starting symbol**: `src-tauri/src/ui/error.rs` — `get_file_outline` then read the `From` impl bodies.

| Check | Pass condition |
|---|---|
| Every `From` impl maps each named error variant to a specific `IpcError` variant — no entire error type collapses to a single catch-all | Named variants get named responses |
| `IpcError::InternalError("An error occurred".into())` catch-alls: count how many distinct source error variants collapse to this single message | Flag if > 3 distinct causes produce the same generic message — user cannot distinguish a DB corruption from an IO failure |
| `IpcError::CloudError("Cloud operation failed".into())` — same check: do distinct failure modes (timeout vs auth failure vs not-found) collapse to one message? | User needs enough information to know whether to retry, check credentials, or contact support |
| `IpcError::InternalError` messages do not contain filesystem paths, Rust type names, or error chain detail | Security (cross-check with Flow C) — note as `[CROSS-REF: FLOW-C]` not a new finding |
| Every `IpcError` variant has a user-readable `message` string — none are empty strings or single characters | Minimum quality bar |

---

### Phase 3 — Frontend error propagation

**Starting symbols** (read in order):
1. `src/invoke/` — `get_file_outline` then `get_symbol_source` for `invoke_command` and `invoke_command_with_channel`. **This is the critical read**: if these wrappers show a toast on error automatically, individual call sites don't need to handle errors. If they don't, every `if let Ok(...) = invoke_command(...)` is a silent drop.
2. `src/components/toast.rs` — `get_file_outline` to understand how toasts are triggered (signal, global store, event?)
3. Sample 3 Leptos pages for error handling patterns — `src/auth.rs` (line ~216 `invoke_command` result), `src/vault.rs`, `src/settings.rs` — use `get_symbol_source` on the specific component functions, not full-file reads

| Check | Pass condition |
|---|---|
| `invoke_command` shows a toast or sets an error signal on `Err` response — or documents why callers must handle errors themselves | Centralized or documented pattern; not both missing |
| `invoke_command_with_channel` surfaces errors when the channel command fails — not just when progress events are malformed | Failure path reaches the UI |
| No call site uses `if let Ok(...) = invoke_command(...)` for an operation that can fail in ways the user needs to know about (upload failure, auth failure, etc.) | Silent drops only acceptable for fire-and-forget operations — document which those are |
| Every command that emits progress events also emits a terminal state (success or error) — the UI cannot be left in a "spinner forever" state | Progress lifecycle completeness |
| Error messages shown in toasts or error labels match the `IpcError.message` field — not a raw `Debug` or `serde_json` dump | Message display quality |
| Commands in `sync_commands.rs` and `file_commands.rs` that can fail mid-operation surface partial-failure state to the user (e.g. "2 of 5 files uploaded before failure") rather than a generic error | User can understand what was and wasn't completed |

**Finding severity guide**:
- `unwrap()` / `panic!()` reachable from non-test production path = **medium**
- `let _ = result` on a user-triggered operation = **medium** (user action silently fails)
- `invoke_command` with no central or per-call error handling = **high** (entire error category invisible to user)
- Progress command leaving UI in spinner state on failure = **medium**
- All distinct failure causes collapsing to "An error occurred" = **low** (bad UX, not a bug)
- Uninformative `.expect("TODO")` in production = **medium**

**Output file**: `.claude/reviews/review-flow-p-YYYYMMDD.md`

*If context fills before Phase 3 is complete: start a follow-up session with "Continue Flow P Phase 3 from `.claude/plans/review-consistency-flows.md`. Phases 1 and 2 are complete in `.claude/reviews/review-flow-p-YYYYMMDD.md`. Start from the frontend propagation checks."*

---

## Session Q — Dead Code & Deferred Work Sweep

**Core question**: Does the codebase contain any dead code, unreachable functions, or deferred work markers (`// Phase N`, `// TODO(phase-N)`, `#[allow(dead_code)]`) that should no longer exist given the current implementation state? This session produces a classified audit with concrete verdicts — not a list of candidates.

**Context prerequisite**: Before running this session, confirm the current phase with the user. The action for every deferred-work marker depends on whether that phase is complete. If all phases are complete, every marker is real debt and requires a verdict.

**Why `cargo check` alone is insufficient**: All dead code is suppressed via explicit `#[allow(dead_code)]` attributes, so the Rust compiler emits zero warnings. The only evidence of dead code is in the suppression attributes themselves and confirmed by grep.

**Critical limitation of import-graph analysis**: `find_dead_code` and `get_dead_code_v2` use the import graph to detect unreachable code, but they miss method calls within the same crate — `.method()` calls do not appear as imports. This causes false positives for any function that is called as a method rather than via `use`. **Always verify every candidate with `grep` before concluding it is dead.**

**plan_turn query**: `"dead code allow dead_code unused unreachable suppression phase TODO deferred"`

**Enumerate first** (build the complete suppression inventory before evaluating any item):
1. `search_text {"query": "#\\[allow(dead_code)\\]"}` across `src-tauri/src/` with `context_lines=2` — enumerate every explicit dead-code suppression; the function or type immediately after each is the candidate.
2. `search_text {"query": "#\\[allow(unused"}` across `src-tauri/src/` with `context_lines=2` — catches `unused_imports`, `unused_variables`, `unused_mut` suppressions; module-level `#![allow(unused_imports)]` in test modules is a separate category.
3. `search_text {"query": "// Phase |// TODO(phase"}` across `src-tauri/src/` — enumerate every deferred-work comment; check each against the current implementation state.
4. `search_text {"query": "#\\[cfg_attr.*allow(dead_code)\\]"}` across `src-tauri/src/` — cfg-gated suppressions; these may be architecturally correct (see Section C below) and require separate evaluation.

**Verify every candidate with grep** (never skip this step):
- For each item from the enumeration, run `search_text {"query": "function_name"}` across `src-tauri/src/` to find actual call sites.
- A function with zero call sites AND zero import-graph edges is confirmed dead.
- A function with call sites found by grep is a **false positive** — the suppression is stale and should be removed, but the code is live.
- Common false-positive triggers: methods called via `self.method()`, functions called only from the same file, functions re-exported via `pub use` and called through the re-export path.

**Classify every item into exactly one category**:

| Category | Definition | Action |
|----------|------------|--------|
| **A — True dead code** | Zero grep call sites + zero import-graph edges | DELETE or WIRE UP (require explicit decision) |
| **B — Stale suppression** | Has grep call sites but carries `#[allow(dead_code)]` | REMOVE the `#[allow]` attribute — no logic change |
| **C — Cfg-gated false positive** | `#[cfg(test)]` / `#[cfg(not(test))]` split causes one branch to look unused per compilation mode | KEEP — suppression is architecturally correct |
| **D — Deferred-work marker** | Phase/TODO comment; phase is still active | KEEP — record in summary as "phase N still active" |
| **E — Module-level test import noise** | `#![allow(unused_imports)]` in `#[cfg(test)]` block | LOW PRIORITY — fix on next touch of that file |

**For Category A items — require a verdict**:
Every true dead item must receive one of:
- `DELETE` — code is vestigial, has no future use, remove it
- `WIRE UP` — implementation exists and is complete; missing only a caller or command registration; specify exactly what wiring is needed

Do not leave a true dead item without a verdict. "Needs investigation" is not a verdict.

**Design doc cross-reference** (for any item whose docstring or comments reference a design concept):
- Look up the relevant design doc in `docs/architecture/designs/` before assigning DELETE to an item that sounds purposeful.
- If a design doc specifies the feature as a deliverable and the implementation is complete but unwired, the verdict is WIRE UP, not DELETE.
- If the design doc says the feature is out of scope or deferred beyond current phase, DELETE is correct.

**What to verify**:

| Check | Pass condition |
|---|---|
| Every `#[allow(dead_code)]` is classified into A, B, C, D, or E — no unclassified suppressions remain | Complete classification |
| Every Category A item has a DELETE or WIRE UP verdict with rationale | No verdict-less dead items |
| Every Category B item has a concrete "remove this line" instruction | Stale suppressions enumerated for mechanical cleanup |
| Category C cfg-gated items are explicitly confirmed as architecturally correct — not just assumed | Each cfg-split suppression justified |
| Items with `// Phase N` or `// TODO(phase-N)` comments: if that phase is complete, they are reclassified as Category A | No stale phase markers left as "intentional" |
| The output report lists: confirmed dead (with verdicts), stale suppressions (mechanical removals), and confirmed false positives | Audit is actionable, not a list of maybes |

**Finding severity guide**: Complete implementation never wired to a Tauri command = **high** (feature gap). Stale phase marker on dead code after phase completion = **medium**. Stale `#[allow(dead_code)]` on live code = **low** (misleading signal). Module-level test import noise = **low** (hygiene).

**Output file**: `.claude/reviews/review-flow-q-YYYYMMDD.md` (persistent — update in place, not a dated review file)

*If context fills before all suppressions are verified: record which items were classified, list unverified items in a "Not yet reached" section, and start a follow-up session from where enumeration left off.*

---

## Finding format (all sessions)

Each finding in the output file:

```
### [FLOW-X-NNN] Short title
**Severity**: high / medium / low / observation
**Location**: `path/to/file.rs:line`
**Observation**: What the code does.
**Issue**: How it violates the convention or creates ambiguity.
**Recommendation**: Concrete, minimal fix.
**Test coverage**: none / partial / covered by <test name>
```

End each session file with a `## Summary` section: counts by severity, any checks fully passed (no findings), and whether a follow-up fix session is recommended.

---

## Sequencing recommendation

These sessions are largely independent. Suggested order:

1. **J** (error type hierarchy) — resolves the `SyncError` duplication; informs M
2. **M** (module boundaries) — fast structural read, no deep symbol traversal
3. **I** (naming) — scan modified files only; fast with `get_file_outline`
4. **K** (IPC surface) — requires understanding from I (naming) to assess consistency
5. **L** (complexity hotspots) — deepest reads focused on correctness; save for last
6. **N** (nesting breadth scan) — no source reads; produces the candidate list for O
7. **O** (nesting source review) — reads source for N's candidates; run after L so its summary can be reused
8. **P** (error handling & user responses) — independent; can run at any point after J
9. **Q** (dead code & deferred work) — run when a phase milestone is reached or at hardening time; independent of the other sessions but informs cleanup priorities

Sessions I, J, M can run in parallel if you have separate Claude sessions available.

---

## Relationship to security review flows

These sessions are deliberately **not** checking security invariants — that is the scope of `review-security-flows.md`. Where a finding here has a security dimension (e.g. a missing `From` impl causing `.to_string()` that strips error context), note the overlap but do not duplicate the security flow's verdict. Cross-reference with `[FLOW-X-NNN]` notation.
