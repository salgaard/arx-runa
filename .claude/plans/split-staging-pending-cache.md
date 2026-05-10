# Plan: Split Staging into `pending/` and `cache/`

## Context

The vault staging directory has dual, conflicting purposes:

1. **Pending upload** — blobs encrypted locally awaiting their first push to the cloud
2. **View cache** — blobs fetched from the cloud so local decryption can happen for in-app viewing or download

`push_vault` cannot distinguish these two cases. It scans staging for any blob whose name appears in `list_sync_chunks()` and is present on disk — and uploads it. When the user views a file, `fetch_missing_file_blobs` downloads encrypted blobs into staging to decrypt them; those blobs are then treated as "pending upload" by the next `sync_to_cloud`, causing a redundant re-upload of data already in the cloud.

**Fix**: Split staging into two subdirectories with clear semantics, and teach `push_vault` to scan only `pending/`.

---

## Directory structure (after)

```
<vault_root>/<vault_id>/staging/
  pending/    ← blobs encrypted locally, awaiting first cloud upload
  cache/      ← blobs fetched from cloud for viewing / decryption
```

---

## Key helpers (all in `sync.rs`)

Replace `blob_staging_path(staging_dir, blob_name)` with two named functions:

```rust
fn pending_blob_path(staging_dir: &Path, blob_name: &str) -> PathBuf {
    staging_dir.join("pending").join(format!("{blob_name}.blob"))
}
fn cache_blob_path(staging_dir: &Path, blob_name: &str) -> PathBuf {
    staging_dir.join("cache").join(format!("{blob_name}.blob"))
}
```

Add an async resolver in `decrypt_file.rs` for read operations that need to find a blob regardless of which subdir it landed in:

```rust
async fn resolve_blob_path(staging_dir: &Path, blob_name: &str) -> PathBuf {
    for subdir in &["pending", "cache"] {
        let p = staging_dir.join(subdir).join(format!("{blob_name}.blob"));
        if tokio::fs::try_exists(&p).await.unwrap_or(false) {
            return p;
        }
    }
    staging_dir.join(format!("{blob_name}.blob")) // flat fallback (tests / migration window)
}
```

---

## Files to modify

### 1. `src-tauri/src/storage/cloud/sync.rs`

- **Rename** `blob_staging_path` → `pending_blob_path` (update all 4 internal callers: `upload_blob_task`, `push_vault`, `pull_vault`, test setups).
- **Add** `cache_blob_path`.
- **`download_blob_task`**: write to `cache_blob_path` instead of `pending_blob_path`.
- **`fetch_missing_file_blobs`**: before downloading, check both `pending_blob_path` AND `cache_blob_path`; download to `cache_blob_path` (the function already passes `staging_dir` as the top-level dir, so pass it unchanged and use the two helpers internally).
- **`push_vault`**: already uses `pending_blob_path` after rename — correct behaviour falls out automatically since only blobs in `pending/` will be found.
- **`pull_vault`**: write downloaded blobs to `cache_blob_path` (update call to `download_blob_task` context or the existence check).
- **`drive_blob_uploads` / `upload_blob_task`**: delete from `pending_blob_path` after successful upload (rename only, no logic change).

### 2. `src-tauri/src/storage/staging.rs`

- **`ensure_staging_directory(path)`**: after creating `path`, also `create_dir_all` for `path/pending` and `path/cache`.
- **`cleanup_orphaned_blobs`**: scan both `path/pending` and `path/cache` (currently scans only flat `path`). Keep the same orphan detection logic (`blob_name` not in `list_all_blob_names()`), applied to each subdir in turn.
- **Add `migrate_flat_staging_blobs(staging_dir)`**: moves any `*.blob` files that sit directly in `staging_dir/` (i.e., old flat layout) into `staging_dir/pending/`. Called once on vault open, before orphan cleanup. Treat all flat blobs as pending-upload (conservative: the worst case is a no-op re-upload for already-synced blobs, which is safe).

### 3. `src-tauri/src/storage/vault_ops/prepare_vault_storage.rs`

- After `ensure_staging_directory` and before `cleanup_orphaned_blobs`, call `migrate_flat_staging_blobs(staging_dir)`.

### 4. `src-tauri/src/storage/pipeline/decrypt_file.rs`

- Replace the direct `blob_directory.join(format!("{}.blob", chunk.blob_name))` path construction with an `await`ed call to the new `resolve_blob_path` helper.
- The helper checks `pending/` then `cache/` then the flat path. This makes `decrypt_file` correct for: newly-uploaded unsynced files (in `pending/`), fetched view-cache files (in `cache/`), and existing tests (flat path fallback).

### 5. `src-tauri/src/storage/vault_ops/delete_file.rs`

- For each chunk, attempt `remove_file_if_present` on all three candidate paths:
  - `staging_dir/pending/{blob}.blob`
  - `staging_dir/cache/{blob}.blob`
  - `staging_dir/{blob}.blob` (compat for any migration window)
- The function already tolerates missing files, so this is safe.

### 6. `src-tauri/src/ui/file_commands.rs`

- **`upload_file` command** (line ~262): pass `staging_dir.join("pending")` as `staging_directory` to the `vault_ops::upload_file` call.
- **`download_file` command** (line ~329): pass top-level `staging_dir` to `fetch_missing_file_blobs` (unchanged) and to `vault_download` — `resolve_blob_path` inside `decrypt_file` handles routing automatically. No change needed here unless currently passing a subpath.
- **`get_file_content` command** (line ~429): same as `download_file` — pass top-level `staging_dir` throughout; no additional changes needed beyond what `decrypt_file` already does.
- **`delete_file` command**: no change (delete_file handles both subdirs).

### 7. `src-tauri/src/ui/sync_commands.rs`

- **`sync_to_cloud`** (line ~247): the `flush_epoch_buffer` call passes `&staging_dir`; change to `&staging_dir.join("pending")` so epoch blobs land in `pending/`. `push_vault` continues to receive the top-level `staging_dir` — `pending_blob_path` inside it resolves correctly.
- **`import_vault_from_cloud`** (line ~307): `pull_vault` receives top-level `staging_dir`; inside `pull_vault`, `download_blob_task` now writes to `cache/` — correct, no caller change needed.

### 8. `src-tauri/src/ui/auth_commands.rs`

- **`decrypt_local_vault_files_background`** (line ~724): passes `staging_dir` to `fetch_missing_file_blobs` and `vault_download` — no change needed (routing handled by `resolve_blob_path`).
- **`on_vault_ready_for_operations`** (line ~339/505): `prepare_vault_storage` is called with the top-level staging_dir — migration and subdir creation happen inside. No change at this call site.

### 9. `src-tauri/src/ui/sharing_commands.rs`

- **`import_share` / `import_multiple_shares`** (lines ~369, ~707): these write new blobs to staging. Pass `staging_dir.join("pending")` so imported blobs are queued for upload on next sync.
- **`export_share`** (line ~279): reads blobs for re-encryption. Pass top-level `staging_dir` to `fetch_missing_file_blobs`; `decrypt_file` resolves via `resolve_blob_path`. No change needed if `fetch_missing_file_blobs` already receives the top-level dir.
- **`revoke_share`** (line ~1048): same as `export_share`.

---

## Migration behaviour

On vault open (after upgrade), `migrate_flat_staging_blobs` moves all flat `*.blob` files from `staging/` into `staging/pending/`. This means:
- Blobs that haven't been synced yet: moved to `pending/`, will be uploaded on next sync ✓
- Blobs that were already synced but re-fetched for viewing before the upgrade: also moved to `pending/`, re-uploaded once (idempotent, safe) ✓
- After migration the flat directory is empty; `cleanup_orphaned_blobs` runs on both subdirs as normal.

---

## What does NOT change

- `encrypt_file.rs` — writes to wherever `staging_directory` points; callers pass `staging_dir.join("pending")`, no internal change.
- `epoch_flush.rs` — same: callers pass `pending_staging_dir`.
- `vault_ops/upload_file.rs` — unchanged; its `staging_directory` param is now always `pending/`.
- `vault_ops/download_file.rs` — unchanged; `blob_directory` param is the top-level `staging_dir`; `resolve_blob_path` inside `decrypt_file` handles routing.
- DB schema — no changes.
- IPC API — no changes.

---

## Verification

1. **Unit tests**: `cargo test -p arx-runa-lib` — existing tests use flat staging paths that hit the `resolve_blob_path` fallback; they should continue to pass without modification.
2. **Sync round-trip**: Upload a file → verify blob appears in `staging/pending/` → press Sync → verify `pending/` is empty after sync and no re-upload occurs on second Sync.
3. **View without re-upload**: After syncing, click a filename to view it → verify blob appears in `staging/cache/` (not `pending/`) → press Sync → verify `blobs_uploaded == 0`.
4. **View before sync**: Upload a file without syncing → view the file → verify it decrypts correctly (blob resolved from `pending/`) → sync → verify blob uploaded exactly once.
5. **Migration**: Manually place a `.blob` file in the flat `staging/` directory → restart the vault → verify the blob is moved to `staging/pending/` and no data is lost.
6. **Orphan cleanup**: Place a `.blob` file with a UUID not in the DB into both `staging/pending/` and `staging/cache/` → restart vault → verify both are removed.
