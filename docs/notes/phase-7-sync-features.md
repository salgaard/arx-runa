# Phase 7 Sync Features

Implemented in `src-tauri/src/ui/sync_commands.rs`.

## Full `migrate_vault`

Re-downloads every vault blob from the current primary destination before uploading to the new one, then atomically swaps the primary.

**Flow:**
1. Enumerate all blobs from DB (`list_sync_chunks`)
2. Download any blobs not already in local staging from the primary transport
3. Upload all blobs to new destination (phases shown as 0–50% / 50–100% progress)
4. Upload manifest backup + vault header to new destination
5. `set_primary_destination` in DB (commit point — safe to retry before this)
6. Rebuild `rclone.conf` from DB and swap `AppState.cloud_transport`
7. Delete blobs that were downloaded solely for migration

**Error handling:** failure before step 5 is safe to retry; after step 5 the DB already reflects the new primary so the transport will be rebuilt correctly on next app start.

## `pull_and_reconcile`

Resolves a snapshot counter conflict after another device has pushed. Call this when `sync_to_cloud` returns a `Conflict` error, then re-call `sync_to_cloud`.

**Flow:**
1. Download cloud manifest backup into a temp probe DB (`probe-reconcile.db` in staging)
2. `merge_from_probe_db` — INSERT OR IGNORE into local nodes/chunks/epoch_blobs; returns cloud `snapshot_counter`
3. Delete probe DB
4. `pull_vault` — downloads any blobs referenced in local DB but missing from staging
5. `advance_snapshot_counter_to(cloud_counter)` — makes the next push conflict check pass

**Returns:** `ReconcileResult { blobs_pulled, local_blobs_staged, cloud_counter }`

## Backup retry (`sync_backup` + `get_backup_health`)

When a backup blob upload fails the blob may no longer be in staging by the next run (primary sync cleared it). The failure is tracked in `backup_upload_failures` (schema v5) and re-pulled from primary before the next attempt.

**`sync_backup` changes:**
- Before each destination's upload loop, queries `list_backup_failures` for that destination
- For each known failure: if blob was deleted from vault, clears the record; if blob is missing from staging, re-pulls it from the primary transport
- On upload failure: `record_backup_failure` (upsert, increments retry_count)
- On upload success: `clear_backup_failure`
- `SyncResult.backup_failures` is now populated with the count of blobs still failing after the run

**`get_backup_health`:** returns `Vec<DestinationHealth>` — per-destination counts of pending failure blobs from the DB table.

## DB schema

Added in `storage/schema.rs` as migration v5 (called from both `open()` and `create()` in `sqlcipher.rs`):

```sql
CREATE TABLE IF NOT EXISTS backup_upload_failures (
    blob_name      TEXT    NOT NULL,
    destination_id TEXT    NOT NULL,
    failed_at      INTEGER NOT NULL,
    retry_count    INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (blob_name, destination_id)
);
```

New `SqlCipherMetadataStore` methods: `record_backup_failure`, `clear_backup_failure`, `list_backup_failures`, `get_backup_failure_counts`, `advance_snapshot_counter_to`, `merge_from_probe_db`.

## Error handling

`IpcError::SyncConflict` variant added. `From<SyncError>` in `error.rs` routes `SyncError::Conflict` to it; all other sync errors map to `CloudError` or `InternalError`. The frontend receives `{ kind: "syncConflict" }` and can distinguish a conflict from a network failure.

## New IPC commands registered

| Command | Return type |
|---|---|
| `pull_and_reconcile` | `ReconcileResult` |
| `get_backup_health` | `Vec<DestinationHealth>` |

Frontend mirror types in `src/ipc_types/`: `reconcile_result.rs`, `destination_health.rs`.

---

## Gap 3 — `recover_from_cloud` new-device bootstrap (implemented)

Previously a stub that ignored `vault_header_path` and called `pull_vault` on the already-open local DB. Now fully implemented for new-device bootstrap where no local manifest exists yet.

**Flow:**
1. Read `vault_header_path` JSON → deserialise `VaultHeader` → extract `vault_id`
2. Derive `vault_db_path(vault_id)` and `vault_staging_dir(vault_id)`; `create_dir_all` the vault directory (new device has none)
3. `download_manifest_backup` from the primary transport into the fresh `db_path` (returns `DbPersistIo` error if file already exists — safe guard)
4. `SqlCipherMetadataStore::open` on the downloaded DB using `sqlcipher_key.with_exposed(|k| *k)`
5. `pull_vault` against the new store — downloads all blobs referenced in the manifest
6. Write-lock `AppState.database` and swap in the new store; old store drops immediately (no background tasks)

**Key detail:** `ManifestKey::with_exposed` is used to extract `[u8; 32]` for `download_manifest_backup` (which takes `&[u8; 32]`), while the `ManifestKey` newtype is passed directly to `pull_vault` (which takes `&ManifestKey`).

## Gap 4a — Conflict dialog (implemented)

**Backend:** `From<SyncError>` mapping corrected — `SyncError::Conflict` now emits `IpcError::SyncConflict` (was incorrectly emitting `CloudError`).

**Frontend (`src/state/sync_context.rs`):**
- `SyncActions::sync()` checks `e.kind == "syncConflict"` in the `sync_to_cloud` error branch and sets `SyncState::conflict` instead of `error`
- New `SyncActions::dismiss_conflict()` — clears `conflict` field
- New `SyncActions::pull_and_reconcile_then_sync()` — clears conflict, calls `pull_and_reconcile` IPC, then re-calls `sync()` on success

**New component (`src/components/sync_conflict_dialog.rs`):**
- Reads `SyncState::conflict` reactively; renders a modal overlay when `Some`
- Cancel → `dismiss_conflict`; "Pull & Sync" → `pull_and_reconcile_then_sync`
- Mounted in `AppShell` (`src/layout.rs`) so it is always present in the component tree

## Gap 4b — Backup health badge (implemented)

**`src/state/sync_context.rs`:**
- `SyncState` gains `backup_health: Vec<DestinationHealth>` (cleared in `clear()` per Zero-Trace)
- `sync()` calls `get_backup_health` after each `sync_backup` and updates `backup_health`

**`src/destinations.rs`:**
- `DestinationList` creates a `LocalResource` for `get_backup_health` that re-fetches on `refresh_count` changes and when `SyncState::last_synced_at` changes (post-sync refresh)
- `DestinationItem` gains `pending_failures: u32` prop (default 0); shows a `text-danger` badge when `> 0`

**Capabilities (`src-tauri/capabilities/default.json`):** `allow-pull-and-reconcile` and `allow-get-backup-health` added.

---

## Manual testing

### `recover_from_cloud` (new-device bootstrap)

**Prerequisites:** a vault that has been synced to a cloud/local-path destination at least once.

1. Locate the vault header on the source device: `%APPDATA%\arx-runa\vaults\<vault-id>\vault-header.json` (Windows) or `~/.local/share/arx-runa/vaults/<vault-id>/vault-header.json` (Linux/macOS). The same file is stored unencrypted at the cloud root and can be downloaded from there.
2. On the "new device" (or to simulate one, rename/move the local vault directory out of the way so no local DB exists), authenticate using the vault recovery ceremony — this establishes a session with the correct keys without requiring a local DB.
3. The recovery ceremony calls `recover_from_cloud` with the path to the vault header. Confirm it completes without error.
4. Verify: `vault.db` created at the expected path; subsequent `list_directory` returns the vault's files; staged blobs present in the staging directory.

### Conflict dialog (Gap 4a)

**Easiest local simulation — hard-code conflict state during development:**

Temporarily add `s.conflict = Some("Another device has synced. Pull changes and continue?".into());` inside `SyncProvider`'s initialization in `sync_context.rs`, run the app, and verify the dialog renders with correct text and both buttons work. Remove after verifying.

**Real conflict simulation:**

1. Sync device A to cloud (snapshot counter = N).
2. On device B (same vault, not yet synced), make any file change — do not sync.
3. Sync device A again so the cloud counter advances to N+1.
4. Now sync device B — `push_vault` detects the counter mismatch and returns `SyncError::Conflict`, which surfaces as `IpcError::SyncConflict`.
5. Verify: conflict dialog appears with the "Another device has synced" message.
6. Click **Cancel** — dialog closes, vault state unchanged.
7. Re-trigger sync, click **Pull & Sync** — `pull_and_reconcile` runs (progress visible), then sync completes normally.

**Automated backend check:**
```bash
cargo test -p arx-runa -- "conflict" --nocapture
```

### Backup health badge (Gap 4b)

**Setup:** add at least one non-primary backup destination via the Destinations page. A local-path destination (pointing to a temp directory) works without any cloud credentials.

**Force a failure:**

1. Sync once — blobs upload to the backup destination successfully, no badge shown.
2. Make the destination directory read-only:
   - Windows: `icacls <path> /deny Everyone:W`
   - macOS/Linux: `chmod 444 <path>`
3. Make a file change and sync again. `sync_backup` will fail for that destination and record it in `backup_upload_failures`.
4. Verify: navigate to Destinations — the affected destination shows the red failure badge (`N backup failure(s)`). No page reload needed; the badge appears as soon as the sync completes.
5. Restore write access (`icacls <path> /grant Everyone:W` or `chmod 755 <path>`) and sync again. Badge disappears.

**Automated backend check:**
```bash
cargo test -p arx-runa -- "backup" --nocapture
```
