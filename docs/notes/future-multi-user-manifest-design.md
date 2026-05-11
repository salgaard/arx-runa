# Future: Operation-Log Manifest for Multi-User Vaults (Option B)

## Background

The current manifest is a **monolithic SQLite snapshot**. The entire DB is uploaded as the authoritative vault state on every sync. Whoever pushes last wins. This works for single-user vaults synced across devices (conflicts are rare and acceptable to resolve manually), but it breaks for shared vaults with concurrent writes — the second writer's changes are silently overwritten.

The tactical fix (Option A) captures locally pending entries before the DB replacement and re-inserts them after. It handles the common case but is fundamentally fighting against the snapshot model.

## The Real Solution

Replace the snapshot model with an **append-only operation log** (also called event sourcing or a commit log).

### Core Idea

Instead of uploading the entire DB, each sync push appends a small **delta record** to the cloud log:

```
op_log/{vault_id}/{counter:016x}.op   ← one file per sync operation
```

Each delta record contains:
- Counter (monotonically incrementing, same role as `snapshot_counter` today)
- Timestamp
- List of operations:
  - `AddFile { node_id, parent_id, name, file_key_wrapped, chunks: [...] }`
  - `DeleteFile { node_id }`
  - `AddDirectory { node_id, parent_id, name }`
  - `DeleteDirectory { node_id }`
  - `RenameNode { node_id, new_parent_id, new_name }`

The delta is encrypted with the manifest key (same as the current manifest backup).

### Conflict Resolution

When User B detects a conflict (cloud counter > local counter):
1. Download all delta records between local counter and cloud counter
2. Replay them on top of local state (pull missing cloud changes in)
3. Check for semantic conflicts: same path added by both → rename one copy
4. Append User B's own delta record at the new counter
5. Upload

This is equivalent to git's rebase model: take your local commits, apply the remote commits first, then re-apply yours.

### Periodic Compaction

To avoid replaying the full log from genesis, periodically (e.g., every 1000 operations) publish a **compacted snapshot** of the full DB. New clients start from the latest snapshot and replay only deltas since then.

### Implications

- **Cloud storage**: Many small delta files + periodic snapshot. Object stores (S3, B2, OneDrive) handle this well.
- **Conflict semantics**: Conflicts become explicit and recoverable. True path collisions (both users added `docs/report.txt`) are surfaced rather than silently losing one.
- **Offline support**: A client can accumulate deltas locally while offline, then merge when reconnected.
- **Schema migration**: Deltas carry a schema version. Old clients that don't understand new operation types can refuse to apply them and prompt for an upgrade.
- **Backward compatibility**: Breaking change to the manifest format. Requires a vault migration path (export all nodes from current DB → produce initial log record → switch format). Could be done at vault creation for new vaults first.

## When to Consider This

- When shared vaults (multiple distinct users with separate keys or shared KEK) become a first-class feature
- When the Option A tactical fix proves insufficient (e.g., partial epoch blob re-insertion edge cases surface in production)
- When offline-first sync is a product requirement

## References

- Option A implementation: see `src-tauri/src/ui/sync_commands.rs` `pull_and_reconcile`
- Current snapshot model: `src-tauri/src/storage/cloud/sync.rs` `push_vault` / `pull_vault`
- Manifest encryption: `src-tauri/src/storage/cloud/sync.rs` `upload_manifest_backup` / `download_manifest_backup`
