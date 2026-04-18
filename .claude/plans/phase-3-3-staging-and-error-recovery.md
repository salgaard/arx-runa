---
title: "Phase 3.3 — Staging Directory and Error Recovery"
created: "2026-04-18T00:00:00Z"
status: approved
roadmap-phase: 3
sub-phase: "3.3"
design-document: docs/architecture/designs/chunking-and-manifest/design.md
sub-phase-roadmap: docs/architecture/designs/chunking-and-manifest/sub-phases/roadmap.md
governance-sync-required: true
tags: [storage, staging, crash-recovery, orphan-cleanup, manifest]
---

## 1. Goal

Introduce cross-platform staging-directory management and orphan-blob cleanup, and wire a vault-open/delete-file orchestration that preserves the transactional invariants of the SQLCipher manifest across crashes.

---

## 2. Context

**Sub-phase**: 3.3 is the last slice of Phase 3. It depends on 3.1 (SQLCipher schema + `MetadataStore` trait + `SqlCipherMetadataStore`) and 3.2 (streaming `encrypt_file` / `decrypt_file` pipelines). 3.1/3.2 already shipped:
- `SqlCipherMetadataStore::delete_node` enqueues blob names into `pending_deletions` and cascades chunk rows in one transaction (`src-tauri/src/storage/sqlcipher.rs`).
- `SqlCipherMetadataStore::insert_file_with_chunks` inserts node + chunks atomically.
- `storage::vault_ops::upload_file` already performs encrypt → atomic persist; on persist failure it removes staged blobs best-effort.

**Remaining scope for 3.3**:
- Staging path resolution (Windows, macOS, Linux) in a new `storage/staging.rs` module, using `dirs::data_dir()`.
- `cleanup_orphaned_blobs` function that reads the SQLCipher `chunks.blob_name` universe directly and removes untracked `*.blob` files.
- An orchestration entry point that runs the cleanup on vault open.
- `vault_ops::delete_file` orchestrator that reads chunk blob names, calls `MetadataStore::delete_node` (transactional), then best-effort removes local staging blobs.
- Regression tests confirming transaction atomicity for the Phase 3 mutation surface.
- Crash-recovery tests for the three interruption points.

**Estimated scope**: ~120 lines production code, ~100 lines tests.

**Constraints**:
- Orphan cleanup must operate on `&SqlCipherMetadataStore` (design: enumeration is a SQLCipher-specific helper, not on the `MetadataStore` trait).
- Staging blobs are AEAD ciphertext: leaving orphans on disk does not leak plaintext. No security review required for 3.3.
- Platform compatibility invariant (CLAUDE.md): Windows, macOS, Linux all supported via `dirs::data_dir()`.

---

## 3. Design Concerns / Open Questions

| # | Concern | Source | Impact | Classification | Resolution | Documentation updates |
|---|---------|--------|--------|----------------|------------|------------------------|
| C-1 | Sub-phase Deliverable #2 lists only Windows + Linux paths, but parent design and CLAUDE.md require macOS too | `3.3-staging-and-error-recovery.md` Deliverable 1 vs `design.md` "Staging Directory / Location" | Without macOS mention the implementer might skip cross-platform check | Non-blocking | `dirs::data_dir()` already resolves macOS correctly; plan covers all three. Add sub-phase doc update to list macOS path. | Update `3.3-staging-and-error-recovery.md` Deliverable 1 to include macOS `~/Library/Application Support/arx-runa/staging/` (Section 7). |
| C-2 | Sub-phase says "File deletion flow in `src-tauri/src/storage/vault_ops.rs`" but `vault_ops` is a directory module, not a single file | Deliverable 5 | Implementer could create a stray sibling file | Non-blocking | Place at `src-tauri/src/storage/vault_ops/delete_file.rs` and re-export from `vault_ops/mod.rs`. Documented in Section 5. | None (path corrected in plan). |
| C-3 | Deliverable #4 demands "transaction wrapping confirmed for all manifest mutations" but Phase 3.1 already ships atomic `insert_file_with_chunks` and transactional `delete_node` | Deliverable 4 vs `sqlcipher.rs:522–591` and `:709–742` | Could be read as a request to re-implement existing transaction scopes | Non-blocking | Interpret Deliverable 4 as adding regression tests that prove atomicity (rollback on mid-transaction failure, no partial manifest state). | None. |
| C-4 | Implementation note says "Files with unexpected extensions or names that are not valid UUIDs should be left untouched", but Deliverable 2 says "delete files whose stem does not appear in the known set" | `3.3-staging-and-error-recovery.md` Impl-notes para 3 vs Deliverable 2 | Without resolution, implementer could delete legitimate unrelated files | Non-blocking | Authoritative rule: delete only when stem parses as UUID v4 AND stem is absent from SQLCipher known set AND extension is `.blob`. Non-matching filenames are skipped (returned count excludes them). | None (design already lists this as implementation note — leave as is). |
| C-5 | `cleanup_orphaned_blobs` must not race in-flight uploads (encrypt writes blob → later commits manifest) | Design `## Error Recovery`; Sub-phase Deliverable 3 | Cleanup during upload could delete a freshly staged blob before its chunk row commits | Non-blocking | Assumption: cleanup runs at vault open only, before any user-facing op; enforced by calling it from the orchestration entry point (Section 5, Step 3) and documenting the invariant on the public function. | Storage rules: add bullet "orphan cleanup runs at vault open only" (Section 8). |
| C-6 | Orphan scan may race other vault instances (Unix) or concurrent Delete handlers; `remove_file` may fail with `NotFound` | Parent design § Staging Directory / Lifecycle | Spurious error from cleanup aborts vault open | Non-blocking | Treat `NotFound`/ENOENT as success during blob removal; surface all other I/O errors via `StorageError::Io`. Documented in Assumptions. | None. |
| C-7 | Deliverable #3 says cleanup is called "during vault open, before any user-facing operation", but no `open_vault` orchestrator exists at the storage layer | Sub-phase Deliverable 3 vs current codebase | Without a concrete call-site the cleanup ships un-wired | Non-blocking | Introduce `storage::vault_ops::prepare_vault_storage(store, staging_directory)` which (a) auto-creates staging dir, (b) calls `cleanup_orphaned_blobs`, returns cleanup count. Phase 6.x (IPC) will wire it; 3.3 ships the orchestrator + tests. | None — documented in handoff notes. |

No blocking concerns.

---

## 4. Assumptions

1. **Default staging path** resolves via `dirs::data_dir().join("arx-runa").join("staging")` across Windows (`%APPDATA%`), Linux (`~/.local/share`), and macOS (`~/Library/Application Support`). When `dirs::data_dir()` returns `None`, return `StorageError::Io("data directory unavailable".to_owned())`.
2. **Auto-create behavior** uses `tokio::fs::create_dir_all` — idempotent, does not fail on existing directories.
3. **Orphan eligibility** requires all three: (i) extension equals `.blob`, (ii) file stem parses as UUID v4, (iii) stem not present in SQLCipher `chunks.blob_name` set. Any other file is left untouched. Count returned by `cleanup_orphaned_blobs` counts only deleted files.
4. **Concurrent-delete tolerance**: `remove_file` returning `ErrorKind::NotFound` during cleanup is silently treated as already-removed; other I/O errors surface as `StorageError::Io`.
5. **`list_all_blob_names` visibility**: added on `SqlCipherMetadataStore` as `pub(crate)` (not on the `MetadataStore` trait, per parent design anchor). Returns `HashSet<String>`.
6. **`delete_file` orchestrator signature**: `async fn delete_file(node_id: Uuid, metadata_store: &dyn MetadataStore, staging_directory: &Path) -> Result<(), StorageError>`.
7. **`prepare_vault_storage` signature**: `async fn prepare_vault_storage(store: &SqlCipherMetadataStore, staging_directory: &Path) -> Result<usize, StorageError>` returning the orphan count.
8. **Error wording** — constants reused where possible: `StorageError::Io("data directory unavailable")`, `StorageError::Io("<path>: <reason>")`.
9. **No new `StorageError` variants** are required; `Io`, `Database`, and `NotFound` cover the failure modes.
10. **Zero-byte file delete** path: `get_chunks` returns empty; `delete_node` still removes the node row (CASCADE is a no-op); staging loop iterates over zero items.
11. **Tests** use `tempfile::TempDir` for staging; SQLCipher test stores use `SqlCipherMetadataStore::create` with a fresh on-disk db under the tempdir (matching existing Phase 3.1 test style).
12. **No security-sensitive code changes** — staging blobs are AEAD ciphertext; staging path resolution does not read/write keys.

---

## 5. Approach

### CONTRACT_SNIPPETS

- **CS-001** — New staging module (`src-tauri/src/storage/staging.rs`):
  ```rust
  use std::collections::HashSet;
  use std::path::{Path, PathBuf};

  /// Resolves the default staging-directory path for the current platform.
  pub fn default_staging_directory() -> Result<PathBuf, StorageError>;

  /// Creates the staging directory if it does not already exist.
  pub async fn ensure_staging_directory(path: &Path) -> Result<(), StorageError>;

  /// Deletes staged `*.blob` files whose UUID v4 stem is not present in the
  /// manifest `chunks.blob_name` set. Returns the number of files deleted.
  /// Must not run concurrently with active uploads, downloads, or deletes.
  pub async fn cleanup_orphaned_blobs(
      staging_directory: &Path,
      sqlcipher_store: &SqlCipherMetadataStore,
  ) -> Result<usize, StorageError>;
  ```

- **CS-002** — SQLCipher-specific enumeration helper (added to `SqlCipherMetadataStore`):
  ```rust
  /// Enumerates every `chunks.blob_name` currently stored in the manifest.
  /// Intentionally not on the `MetadataStore` trait — used only by orphan cleanup.
  pub(crate) async fn list_all_blob_names(&self) -> Result<HashSet<String>, StorageError>;
  ```

- **CS-003** — Vault-open orchestrator (`src-tauri/src/storage/vault_ops/prepare_vault_storage.rs`):
  ```rust
  /// Prepares local storage for vault operations: ensures the staging directory
  /// exists and runs orphan-blob cleanup. Must be called before any encrypt/
  /// decrypt/delete operation on the manifest.
  pub async fn prepare_vault_storage(
      store: &SqlCipherMetadataStore,
      staging_directory: &Path,
  ) -> Result<usize, StorageError>;
  ```

- **CS-004** — Delete orchestrator (`src-tauri/src/storage/vault_ops/delete_file.rs`):
  ```rust
  /// Deletes a file node: reads chunk blob names, transactionally removes the
  /// node (CASCADE drops chunks, enqueues `pending_deletions`), then best-effort
  /// removes local staging blobs. Surviving local blobs on failure are cleaned
  /// up by the next `cleanup_orphaned_blobs` run.
  pub async fn delete_file(
      node_id: Uuid,
      metadata_store: &dyn MetadataStore,
      staging_directory: &Path,
  ) -> Result<(), StorageError>;
  ```

- **CS-005** — `StorageError` variants already present (used, not added):
  - `Io(String)`, `Database(String)`, `NotFound`, `ConstraintViolation(String)` — `src-tauri/src/storage/error.rs:9`.

---

### Step 1 — Add the `storage/staging` module (Deliverables 1, 2)

**File**: `src-tauri/src/storage/staging.rs` (new)

Implement the three public functions from **CS-001**:

1. `default_staging_directory()`:
   - `dirs::data_dir().ok_or_else(|| StorageError::Io("data directory unavailable".to_owned()))?.join("arx-runa").join("staging")`.
2. `ensure_staging_directory(path)`:
   - `tokio::fs::create_dir_all(path).await.map_err(|error| StorageError::Io(format!("{}: {error}", path.display())))`.
3. `cleanup_orphaned_blobs(staging_directory, sqlcipher_store)`:
   - Fetch known set via **CS-002** (`sqlcipher_store.list_all_blob_names().await?`).
   - Use `tokio::fs::read_dir` to iterate entries in `staging_directory`.
   - For each entry, skip if not a regular file. Get `file_name`; skip if extension ≠ `.blob`. Get stem; skip unless `Uuid::parse_str(stem).is_ok()` AND the parsed `Uuid`'s `get_version_num() == 4`.
   - If stem ∈ known set → skip. Else → `tokio::fs::remove_file(entry.path()).await`; treat `ErrorKind::NotFound` as success; count increments on successful delete.
   - Return `usize` total removed.

Register the module in `src-tauri/src/storage/mod.rs`: add `pub mod staging;` between `sqlcipher` and `types`.

---

### Step 2 — Add `list_all_blob_names` helper (Deliverable 2 support)

**File**: `src-tauri/src/storage/sqlcipher.rs`

Add method to the `impl SqlCipherMetadataStore` block (not the trait impl) — signature from **CS-002**:

```rust
pub(crate) async fn list_all_blob_names(&self) -> Result<HashSet<String>, StorageError> {
    self.with_connection_blocking(move |conn| {
        let mut statement = conn
            .prepare("SELECT blob_name FROM chunks")
            .map_err(StorageError::from_rusqlite)?;
        let rows = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(StorageError::from_rusqlite)?;
        let mut names = HashSet::new();
        for row in rows {
            names.insert(row.map_err(StorageError::from_rusqlite)?);
        }
        Ok(names)
    })
    .await
}
```

Import `std::collections::HashSet` at the top of `sqlcipher.rs`.

---

### Step 3 — Add the vault-open orchestrator (Deliverable 3)

**File**: `src-tauri/src/storage/vault_ops/prepare_vault_storage.rs` (new)

Signature from **CS-003**. Body:
1. Call `staging::ensure_staging_directory(staging_directory).await?`.
2. Call `staging::cleanup_orphaned_blobs(staging_directory, store).await?`.
3. Return the count.

Wire it in `src-tauri/src/storage/vault_ops/mod.rs`:
- Add `mod prepare_vault_storage;`
- Add `pub use prepare_vault_storage::prepare_vault_storage;`

No existing code paths need modification; Phase 6 IPC will call this after `SqlCipherMetadataStore::open`.

---

### Step 4 — Add transactional regression tests (Deliverable 4)

**File**: `src-tauri/src/storage/sqlcipher.rs` (existing `#[cfg(test)] mod tests`)

Add regression tests that confirm the existing atomic boundaries:
1. `test_insert_file_with_chunks_rejects_bad_chunk_and_leaves_no_partial_manifest_entry` — call `insert_file_with_chunks` with one valid chunk and one chunk whose `size_padded` is wrong; assert the call returns `ConstraintViolation`, and that a subsequent `get_node(node_id)` returns `NotFound` (proving the node insert rolled back).
2. `test_delete_node_transaction_commits_pending_deletions_and_cascades` — insert a file with chunks, call `delete_node`; assert `get_node` is `NotFound`, `get_chunks` returns empty, `list_pending_deletions(10)` contains every original `blob_name`.

(Both re-exercise paths already present in the production code; the tests lock in the contract.)

---

### Step 5 — Add `vault_ops::delete_file` orchestrator (Deliverable 5)

**File**: `src-tauri/src/storage/vault_ops/delete_file.rs` (new)

Signature from **CS-004**. Body:
1. `let chunks = metadata_store.get_chunks(node_id).await?;`
2. `metadata_store.delete_node(node_id).await?;` (the concrete impl already handles the transactional `pending_deletions` enqueue + CASCADE per Phase 3.1 work).
3. `for chunk in chunks { let blob_path = staging_directory.join(format!("{}.blob", chunk.blob_name)); let _ = tokio::fs::remove_file(blob_path).await; }` — best-effort; surviving files are recovered by the next orphan cleanup.
4. Return `Ok(())`.

Wire in `src-tauri/src/storage/vault_ops/mod.rs`:
- Add `mod delete_file;`
- Add `pub use delete_file::delete_file;`
- Update `src-tauri/src/storage/mod.rs` to re-export the new symbol alongside `upload_file`/`download_file` (the existing line becomes `pub use vault_ops::{RouteDecision, decide, delete_file, download_file, prepare_vault_storage, upload_file};`).

---

### Step 6 — Add crash-recovery + staging tests (Deliverables 6, 7)

Colocate tests with their modules.

**`src-tauri/src/storage/staging.rs` `#[cfg(test)]`**:
1. `test_ensure_staging_directory_creates_missing_directory` — tempdir path absent; after call, exists.
2. `test_ensure_staging_directory_is_idempotent_when_present` — pre-create; call succeeds.
3. `test_cleanup_orphaned_blobs_removes_untracked_blob_and_returns_count` — create `SqlCipherMetadataStore`; stage two blobs (one with a known blob_name inserted via `insert_file_with_chunks`, one random UUID v4 with no manifest row); call cleanup → assert count = 1, only the orphan is deleted, the referenced blob survives.
4. `test_cleanup_orphaned_blobs_preserves_referenced_blob_when_manifest_lists_it` — subset of the above, asserting survival explicitly.
5. `test_cleanup_orphaned_blobs_skips_non_blob_and_non_uuid_files` — seed `readme.txt`, `not-a-uuid.blob`, `<non-v4-uuid>.blob` → cleanup returns 0; files remain.
6. `test_cleanup_orphaned_blobs_tolerates_concurrently_removed_file` — stage an orphan blob, then call cleanup after pre-removing the file to simulate a race (`ErrorKind::NotFound`); assert `Ok(0)` (no error bubbled).

**`src-tauri/src/storage/vault_ops/prepare_vault_storage.rs` `#[cfg(test)]`**:
7. `test_prepare_vault_storage_creates_missing_staging_directory_and_runs_cleanup` — tempdir path absent; stage a tracked blob file after dir exists; precondition: call once to create, then unlink manifest-referenced blob to produce an orphan; re-run → confirm count behavior.

**`src-tauri/src/storage/vault_ops/delete_file.rs` `#[cfg(test)]`**:
8. `test_delete_file_removes_node_chunks_pending_queue_and_local_blobs` — upload a multi-chunk file; call `delete_file`; assert `get_node` is `NotFound`, staged `.blob` files no longer exist, `list_pending_deletions` contains the originals.
9. `test_delete_file_zero_byte_node_only_deletes_node_row` — insert a 0-byte file node via the store directly (no chunks, `file_key_wrapped = Some([..])`); `delete_file` returns `Ok`, node is gone, no filesystem side-effects.
10. `test_delete_file_missing_local_blob_still_succeeds` — pre-remove one of the staged `.blob` files before calling `delete_file`; call still returns `Ok(())`.

**Crash simulation tests** (collocated with `prepare_vault_storage.rs`):
11. `test_crash_after_encrypt_before_commit_cleanup_removes_orphaned_blobs` — run `encrypt_file` (produces staged blobs), skip the persist call entirely, call `prepare_vault_storage` → orphans removed.
12. `test_crash_after_commit_before_blob_delete_orphan_scan_noop` — upload (commit) a file, call `prepare_vault_storage` → count = 0 because all blobs are tracked; then `delete_file` → blobs removed, pending_deletions queued.

Validation command: `cargo test --workspace --all-targets --all-features storage::`.

---

### Step 7 — Export surface (Deliverable 1, 2, 3, 5)

**`src-tauri/src/storage/mod.rs`**:
- `pub mod staging;`
- Update `pub use vault_ops::{...}` to include `delete_file` and `prepare_vault_storage`.
- Optionally re-export `staging::{cleanup_orphaned_blobs, default_staging_directory, ensure_staging_directory}` if callers outside the crate need them; otherwise keep them accessible via `crate::storage::staging::*` (see Handoff).

---

## 6. Review focus areas

### 6a. Rust change surface
- `src-tauri/src/storage/mod.rs` (module registration + re-exports).
- `src-tauri/src/storage/staging.rs` (new).
- `src-tauri/src/storage/sqlcipher.rs` (add `list_all_blob_names`, add 2 regression tests, add `HashSet` import).
- `src-tauri/src/storage/vault_ops/mod.rs` (module registration + re-exports).
- `src-tauri/src/storage/vault_ops/prepare_vault_storage.rs` (new).
- `src-tauri/src/storage/vault_ops/delete_file.rs` (new).

### 6b. Security-sensitive paths
None. Sub-phase explicitly documents "No security review required" (3.3 operates solely on AEAD-ciphertext blobs and manifest metadata). If during implementation any change drifts into `src-tauri/src/crypto/` or `src-tauri/src/auth/`, that triggers a plan deviation.

### 6c. Architecture risk areas
- `src-tauri/src/storage/staging.rs` — must stay concern-isolated (path + cleanup only; no manifest schema or crypto). Keep `list_all_blob_names` as the only inbound call into `SqlCipherMetadataStore`.
- `src-tauri/src/storage/sqlcipher.rs` — `list_all_blob_names` is `pub(crate)` and must not leak through the `MetadataStore` trait. Watch for accidental trait-method addition.
- `src-tauri/src/storage/vault_ops/{prepare_vault_storage,delete_file}.rs` — orchestration only: no direct SQL, no FFI. Depend on the `MetadataStore` trait plus the concrete store for cleanup.
- Dependency direction: `vault_ops::prepare_vault_storage` → `staging` + `SqlCipherMetadataStore`; `staging` → `SqlCipherMetadataStore` (one-way). Verify no cycles introduced.
- `storage/mod.rs` re-exports should remain a thin surface (rule: `mod.rs` re-exports only).

### 6d. Testing requirements
**Validation checkpoint from sub-phase**:
- `cargo test storage::staging` passes.
- Manual verification: stage fake `<uuid>.blob` files → open vault → orphans removed, tracked blobs untouched; process-kill during encrypt → next startup clean.

**Edge cases**:
- Missing staging directory on first call (auto-create).
- Non-`.blob` files, non-UUID stems, non-v4 UUID stems (skip untouched).
- Concurrent removal during cleanup (`ErrorKind::NotFound` → success).
- Zero-byte file deletion (no chunks, no blob files).
- `data_dir()` unavailable → `StorageError::Io`.
- `delete_file` with already-missing local blob → success.
- Mid-transaction failure in `insert_file_with_chunks` (bad chunk) leaves no partial manifest row.

Required command: `cargo test --workspace --all-targets --all-features` (project-wide gate, per feedback memory).

---

## 7. Documentation impact

- **Required this run**:
  1. `.claude/rules/storage.md` — add rules covering staging directory, orphan cleanup invariants, and the `vault_ops::delete_file` / `prepare_vault_storage` orchestration boundary.
  2. `docs/architecture/designs/chunking-and-manifest/sub-phases/3.3-staging-and-error-recovery.md` Deliverable 1 — expand path list to include macOS (`~/Library/Application Support/arx-runa/staging/`). Cross-platform covered today via `dirs::data_dir()`; sub-phase should reflect that.

- **Deferred / optional**:
  - `docs/roadmap.md` Phase 3 status update to "Complete" — deferred to the phase sign-off commit rather than 3.3 merge (matches existing pattern in prior 3.x plans).
  - `docs/architecture/diagrams/` chunk-pipeline diagram — the sub-phase "Completion" section lists this as a nice-to-have; deferred (no blocker for Phase 4.1).
  - `.github/instructions/` — only if `.claude/rules/` content changed (see Governance sync actions).

---

## 8. Governance sync actions (pre-implementation)

| Action ID | Reason / linked concern | Target files | Required edit | Verification |
|-----------|------------------------|---------------|---------------|--------------|
| G-1 | Storage rules currently omit staging & orphan-cleanup guardrails (linked: C-5, C-6, Deliverables 1–3, 5) | `C:\Users\chris\source\repos\arx-runa\.claude\rules\storage.md` | Add a new `## Staging directory` section with bullets: (a) default path resolved via `dirs::data_dir().join("arx-runa/staging")` on all three targets; (b) orphan cleanup must only run at vault open; (c) `cleanup_orphaned_blobs` deletes a file only if extension is `.blob` AND stem is UUID v4 AND stem ∉ `chunks.blob_name`; (d) `StorageError::Io("NotFound")` from a concurrent remove is swallowed; (e) `list_all_blob_names` is SQLCipher-specific and not on `MetadataStore`. Also extend the `## Deletion` section: "use `vault_ops::delete_file` as the orchestration entry point — it reads chunk names, calls `MetadataStore::delete_node` (transactional), then best-effort removes staging blobs." | `rg "cleanup_orphaned_blobs" .claude/rules/storage.md` returns a match. |
| G-2 | Copilot instructions must mirror updated storage rules | `C:\Users\chris\source\repos\arx-runa\.github\instructions\` (run `/copilot-sync` after G-1) | Regenerate synced instructions from `.claude/rules/storage.md`. | `git diff .github/instructions/` shows the matching copy. |
| G-3 | Sub-phase spec omits macOS staging path; add macOS line (linked: C-1) | `C:\Users\chris\source\repos\arx-runa\docs\architecture\designs\chunking-and-manifest\sub-phases\3.3-staging-and-error-recovery.md` | Deliverable 1: append `; ~/Library/Application Support/arx-runa/staging/` on macOS` (or equivalent bullet), matching parent `design.md` Staging Directory / Location. | `rg "Library/Application Support" docs/architecture/designs/chunking-and-manifest/sub-phases/3.3-staging-and-error-recovery.md` returns a match. |

Run `/copilot-sync` after G-1 to fulfil G-2.

---

## 9. Handoff Notes for Implementer

Working directory: `C:\Users\chris\source\repos\arx-runa`. Run all cargo commands as `cargo test --workspace --all-targets --all-features` (per project feedback memory). This plan is self-contained — all trait signatures, error enums, and DDL impacts are inlined via `CS-xxx` snippets in Section 5.

**Order of operations**: execute Section 8 governance actions first (G-1 → G-3 → then `/copilot-sync` for G-2), then follow Section 5 Steps 1 → 7 sequentially. Steps 1 and 2 have no dependencies and could run in parallel, but Step 3 depends on both. Step 5 (`delete_file`) depends on Step 2's new helper only indirectly via tests; Step 4 regression tests are independent.

**Traps**:
- Do **not** add `list_all_blob_names` or any cleanup function to the `MetadataStore` trait — it must remain a SQLCipher-specific helper (parent design anchor).
- Do **not** add a new `StorageError` variant. Reuse `Io`, `Database`, `NotFound`, `ConstraintViolation`.
- Platform compatibility: resist the urge to guard staging paths with `cfg(target_os = ...)` — `dirs::data_dir()` handles all three targets. Any `cfg`-gated path is a plan deviation.
- `pub(crate)` discipline on `list_all_blob_names`; if Phase 4 needs global enumeration for a different reason, that is a new contract — do not broaden visibility pre-emptively.
- Staging module I/O uses `tokio::fs` only — no sync `std::fs` (rule: all I/O async).
- Watch for the "`src-tauri/src/storage/vault_ops.rs`" path in the sub-phase — it is a stale reference to the pre-split module; the correct location is `src-tauri/src/storage/vault_ops/delete_file.rs` (C-2).
- No security-sensitive changes expected. If implementation drifts into `src-tauri/src/crypto/` or `src-tauri/src/auth/`, pause and log a plan deviation.
