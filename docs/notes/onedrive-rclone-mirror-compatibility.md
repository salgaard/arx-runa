# OneDrive Mirror Destination — rclone Compatibility Issue

## Symptom

When a secondary backup destination points at a OneDrive-managed local folder,
rclone fails with:

```
mkdir \\?\C:\Users\<user>: Access is denied.
```

This appears repeatedly (3 retries) for every blob, manifest, and vault-header
upload. Logged under `arx_runa_tauri_lib::ui::sync_commands` at WARN level.

## Root cause

rclone uses the Windows long-path prefix (`\\?\`) when constructing target
paths. OneDrive's virtual file system (Files On-Demand) driver intercepts these
`\\?\`-prefixed filesystem calls and rejects them with `Access is denied` when
trying to create or stat a directory that OneDrive considers under its control.

The failure occurs when the `path_prefix` stored for the destination points at
(or is a direct child of) a OneDrive-managed directory, e.g.:

```
C:\Users\chris\OneDrive - Contoso\ArxRuna-backup
```

rclone tries to `mkdir` each path component in turn; the call for the OneDrive
root fails before any subdirectory is created.

A secondary trigger: if `path_prefix` is accidentally set to the user's home
directory root (`C:\Users\chris`) rather than a dedicated subfolder, rclone
attempts `mkdir \\?\C:\Users\chris` which is additionally denied because the
OS protects the home root.

## Impact

- Non-fatal: `sync_backup` logs `continuing with remaining blobs` and moves on.
- Primary cloud sync and conflict detection are unaffected.
- The mirror destination is silently not backed up.

## Potential solutions

### Option 1 — Disable `\\?\` long-path handling (easiest workaround) ✅ Implemented
Pass `--local-no-unicode-normalization` and rely on rclone's `--no-check-dest`
mode, or add `nounc = true` to the rclone config blob for local/external-drive
destinations:

```toml
[arx_XXXXXXXX]
type = local
nounc = true
```

In `add_destination` (destination_commands.rs), change the local config blob
builder to:

```rust
format!("[{}]\ntype = local\nnounc = true\n", rclone_remote_name)
```

`nounc = true` disables the `\\?\` prefix on Windows, which sidesteps the
OneDrive driver interception.

### Option 2 — Detect OneDrive paths and warn at destination-add time
Check whether `path_prefix` resolves inside a known OneDrive root
(`%USERPROFILE%\OneDrive*`) and surface a warning or block the add with a
user-readable message advising them to use a non-OneDrive folder.

### Option 3 — Use rclone's OneDrive remote instead of local ✅ Implemented
Configure the destination as a `DestinationType::Cloud` pointing at a
OneDrive rclone remote (`type = onedrive`) rather than the local filesystem
mount. This uses OneDrive's native API and avoids the filesystem driver
conflict entirely. The OneDrive destination is now a first-class option in
the destination selector, with a guided OAuth setup flow.

### Option 4 — Validate path_prefix at destination-add time ✅ Implemented
Reject or warn when `path_prefix` equals the user's home directory or a
drive root, ensuring users always specify a dedicated subfolder. Additionally
rejects paths inside OneDrive-managed directories and directs users to the
OneDrive destination instead.

## Recommended approach

**Short term:** Apply Option 1 (`nounc = true`) — one-line change, no UX
impact, fixes the immediate failure. ✅ Done.

**Medium term:** Add Option 4 validation to prevent misconfigured paths. ✅ Done.

**Long term:** Consider Option 3 for users who specifically want OneDrive as
a backup target, as the native rclone OneDrive remote is more reliable than
the local filesystem mount. ✅ Done — OneDrive is now a first-class destination.

## References

- Observed: 2026-05-08, device B mirror during phase-7 conflict simulation test
- Destination type: `LocalPath` / `ExternalDrive`
- Affected file: `src-tauri/src/ui/destination_commands.rs` (`add_destination`)
- rclone docs: <https://rclone.org/local/#nounc>
