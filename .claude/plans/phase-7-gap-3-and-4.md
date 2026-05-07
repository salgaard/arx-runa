# Phase 7 Gaps: recover_from_cloud bootstrap + frontend wiring

## Gap 3 — `recover_from_cloud` new-device bootstrap

### Problem
`recover_from_cloud` is a Phase 6.5 stub. It calls `pull_vault` on the already-open local manifest and ignores `vault_header_path`. A new device has no local manifest yet — it needs to download and import the remote one first.

### What needs doing
1. **Read `vault_header_path`** to get the vault ID and locate the local DB path.
2. **Download the manifest backup** from the primary transport into a temp path (`download_manifest_backup`). The target path must not exist yet — this is a fresh DB file.
3. **Open the downloaded DB** as a `SqlCipherMetadataStore` (call `SqlCipherMetadataStore::open`) using the session sqlcipher key.
4. **Call `pull_vault`** against the newly opened store to download all blobs referenced in it.
5. **Swap the live database** in `AppState.database` to the newly populated store. (Currently `AppState.database` is `Arc<RwLock<Option<SqlCipherMetadataStore>>>`; replace the inner value.)
6. Return `Ok(())`.

### Key files
- `src-tauri/src/ui/sync_commands.rs` — `recover_from_cloud` (line ~291)
- `src-tauri/src/storage/cloud/manifest_backup.rs` — `download_manifest_backup`
- `src-tauri/src/storage/sqlcipher.rs` — `SqlCipherMetadataStore::open`
- `src-tauri/src/ui/state.rs` — `AppState.database` field

### Risk
Medium. The DB swap is the delicate part — must hold the write lock during swap and ensure the old store is fully dropped before the new one becomes active. Check whether `SqlCipherMetadataStore` has any background tasks that need cancelling on drop.

---

## Gap 4 — Frontend wiring

### 4a — Conflict dialog (ties to `pull_and_reconcile`)
When `sync_to_cloud` returns `{ kind: "syncConflict" }`, the UI should surface a dialog: *"Another device has synced. Pull changes and continue?"* with Confirm / Cancel.

- Confirm → calls `pull_and_reconcile` (stream progress), then re-calls `sync_to_cloud`.
- Cancel → dismisses, leaves vault in current state.

Location: `src/actions/sync_actions.rs` (or wherever `sync_to_cloud` is invoked) — add a match arm on `IpcError::SyncConflict` before the generic error handler.

### 4b — Backup health badge
`get_backup_health` returns `Vec<DestinationHealth>`. Call it after each `sync_backup` completes and after the destinations list loads. Show a badge or warning icon on any destination with `pending_failure_blobs > 0`.

Location: likely `src/components/destination_list.rs` or the destinations page. Call `invoke("get_backup_health")` and merge results into the destination entries by `destination_id`.

### Key frontend types (already mirrored)
- `src/ipc_types/reconcile_result.rs` — `ReconcileResult`
- `src/ipc_types/destination_health.rs` — `DestinationHealth`

### New IPC request types needed
`pull_and_reconcile` takes only a progress channel — no request struct required. `get_backup_health` takes no arguments.
