# Staging Directory Split: `pending/` and `cache/`

## Problem

The flat `staging/` directory conflated two distinct roles:

- **Locally-encrypted blobs awaiting upload** (produced by `encrypt_file`, `flush_epoch_buffer`, etc.)
- **Blobs fetched from cloud for local decryption/viewing** (produced by `fetch_missing_file_blobs`, `pull_vault`)

`push_vault` scans staging for blobs whose names appear in `list_sync_chunks()`. After a view-cache fetch, those downloaded blobs were re-uploaded on the next sync — wasting bandwidth and potentially overwriting cloud state.

## Fix

Split staging into two typed subdirectories with clear ownership:

| Path | Purpose |
|---|---|
| `staging/<vault-id>/pending/` | Locally-encrypted blobs queued for upload |
| `staging/<vault-id>/cache/` | Blobs fetched from cloud for local read/decrypt |

`push_vault` now only scans `pending/`; download paths write exclusively to `cache/`.

## Migration

`migrate_flat_staging_blobs` runs on vault open (inside `prepare_vault_storage`, between `ensure_staging_directory` and `cleanup_orphaned_blobs`). It moves any pre-existing flat UUID-v4 `.blob` files into `pending/`. Worst case: a blob already synced gets re-uploaded once — idempotent and safe.

## Files Changed

| File | Change |
|---|---|
| `src-tauri/src/storage/staging.rs` | `ensure_staging_directory` creates subdirs; `cleanup_orphaned_blobs` scans subdirs only; `migrate_flat_staging_blobs` added |
| `src-tauri/src/storage/vault_ops/prepare_vault_storage.rs` | Calls migrate between ensure and cleanup |
| `src-tauri/src/storage/cloud/sync.rs` | `pending_blob_path` / `cache_blob_path` helpers; all 5 call sites updated |
| `src-tauri/src/storage/pipeline/decrypt_file.rs` | `resolve_blob_path` checks `pending/` → `cache/` → flat fallback |
| `src-tauri/src/storage/vault_ops/delete_file.rs` | Collects 3 candidate paths per chunk (pending, cache, flat) |
| `src-tauri/src/ui/file_commands.rs` | `vault_upload` → `staging_dir.join("pending")` |
| `src-tauri/src/ui/sync_commands.rs` | Both `flush_epoch_buffer` calls → `staging_dir.join("pending")` |
| `src-tauri/src/ui/auth_commands.rs` | `try_flush_on_lock` → `staging_dir.join("pending")` |

## What Was Not Changed

- `sharing_commands.rs`: receipt blobs (`receipt-*.blob`, `rcpt-*.conf`) are temporary, use prefixed non-UUID-v4 names, self-upload directly, and are deleted immediately. They are unaffected by the orphan scan and `push_vault`.

## Test Result

750 lib tests passed, 0 failed.
