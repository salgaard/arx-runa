# Event-Driven Mirror Sync via `pending_backup` Table

## Problem

`sync_backup` determined what a mirror destination was missing by comparing local staging
blobs against all DB-referenced blobs, then calling `list_blobs` on the mirror remote to
find gaps. This had two fundamental flaws:

1. **Missed old blobs.** Blobs uploaded to the primary are deleted from local staging
   immediately after upload (`upload_blob_task` in `sync.rs`). Any blob not in staging
   and without a prior failure record was silently skipped — a new mirror never received
   the vault's history.

2. **O(n) remote listing on every run.** Even when the mirror was fully up to date, every
   `sync_backup` call called `list_blobs` on the mirror to diff against `all_blob_names`.
   Expensive for large vaults or slow remotes.

## Fix

Replace the reconciliation loop with an event-driven queue: `pending_backup (blob_name,
destination_id)`. A row means "this blob still needs to reach this destination." No row
means it's already there.

### When rows are written

| Trigger | Action |
|---|---|
| `sync_to_cloud` completes a primary push | `bulk_insert_pending_backup` for every successfully uploaded blob × every active mirror destination |
| `add_destination` registers a new mirror | `bulk_insert_pending_backup` for all blobs currently in `list_sync_chunks()` — seeds the full vault history |

### When rows are cleared

| Event | Action |
|---|---|
| `sync_backup` successfully uploads a blob to the mirror | `clear_pending_backup` + `clear_backup_failure` |
| Blob is no longer in `all_blob_names` (vault deletion) | Stale row discarded in the per-blob loop |
| Destination deleted | `clear_pending_backups_for_destination` |

### `sync_backup` per-destination loop (new)

1. `list_pending_backups(dest_id)` — the work queue
2. For each pending blob:
   - If not in `all_blob_names` → discard stale record, skip
   - If in `staging/<vault-id>/pending/` → upload directly
   - Otherwise → pull from primary into `staging/<vault-id>/mirror-temp/{blob}.blob`,
     upload, delete temp file immediately after (success or failure)
3. Mirror orphan deletion still calls `list_blobs("vault/")` — this is the only correct
   source of truth for what is physically on the remote

### Staging directory note

Blobs pulled from primary for mirror upload go into `mirror-temp/`, not `pending/`.
`pending/` is owned by `vault_ops` (blobs produced locally awaiting primary upload).
Mixing foreign blobs into `pending/` would confuse `push_vault`'s scan and orphan cleanup.
`mirror-temp/` files are ephemeral: deleted per-blob after each attempt; no cleanup pass
needed on startup (stale files from a crash are simply overwritten on the next retry).

## Schema

Schema version bumped from 5 → 6.

```sql
CREATE TABLE pending_backup (
    blob_name      TEXT NOT NULL,
    destination_id TEXT NOT NULL,
    PRIMARY KEY (blob_name, destination_id)
);
```

`backup_upload_failures` is retained for `get_backup_health` display (failure counts and
retry metadata). `list_backup_failures` was removed — the pending queue supersedes it as
the retry driver.

## Files Changed

| File | Change |
|---|---|
| `src-tauri/src/storage/schema.rs` | `apply_pending_backup_v6_migration()`; idempotency guards in v2–v5 updated; `validate_manifest_meta` accepts 1–6 |
| `src-tauri/src/storage/sqlcipher.rs` | Migration called in `open()` and `create()`; 4 new methods: `bulk_insert_pending_backup`, `list_pending_backups`, `clear_pending_backup`, `clear_pending_backups_for_destination` |
| `src-tauri/src/storage/cloud/sync.rs` | `PushReport` gains `uploaded_blob_names: Vec<String>` |
| `src-tauri/src/ui/sync_commands.rs` | `sync_to_cloud`: seeds pending after push; `sync_backup`: pending-driven loop replaces staged-blobs + known-failures + mirror pre-fetch |
| `src-tauri/src/ui/destination_commands.rs` | `add_destination`: seeds pending for new mirrors; `delete_destination`: clears pending for deleted destination |

## Test Result

753 tests passed, 0 failed (1 pre-existing flaky failure in isolation due to shared
`staging_directory()` across parallel test modules — unrelated to this change; passes
when run in isolation).
