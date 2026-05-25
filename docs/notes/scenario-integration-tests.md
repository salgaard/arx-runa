# Scenario Integration Tests

The `src-tauri/src/tests/` module contains end-to-end scenario tests that exercise the full storage, crypto, and auth stack without Tauri IPC. They are grouped by use case.

## Test files

| File | Use case | Providers |
|------|----------|-----------|
| `scenarios_auth.rs` | Auth ceremonies (Tier 1 & 2 vaults, recovery) | Mock |
| `scenarios_backup.rs` | Encrypt/decrypt pipeline, EXIF stripping | Mock |
| `scenarios_sync.rs` | Push/pull vault, stale manifest conflict detection | Mock |
| `scenarios_destinations.rs` | Multi-destination mirror vs accumulating semantics | Mock |
| `scenarios_sharing.rs` | Share package round-trip (create → import → decrypt) | Mock |
| `scenarios_real_cloud.rs` | Push vault + round-trip bytes against live cloud APIs | B2, Drive, OneDrive |

## Mock tests (22 total across first five files)

All mock tests use `MockCloudTransport` (in-memory blob store) and the helpers from `auth::ceremonies::test_support`. They run offline with no credentials and are always included in `cargo test`.

Key scenarios covered:

- **Auth**: Tier 1 vault create/unlock/lock, Tier 2 (password + key file), recovery phrase setup and full restore onto a fresh DB path.
- **Backup**: Encrypt → stage → decrypt round-trip confirms byte-identical output; EXIF stripping removes APP1 segments from JPEG bytes before staging.
- **Sync**: Push increments `snapshot_counter`; a second device pushing from a stale copy triggers a `StorageError` conflict.
- **Destinations**: Two `MockCloudTransport` instances pushed in sequence carry identical blob sets (mirror semantics); pending deletions are flushed on the next push (blob absent after delete + sync); blob survives when no push follows a deletion (accumulating semantics); a manifest-upload timeout on destination A does not prevent a successful push to destination B.
- **Sharing**: `create_share_package` wraps a `FileKey` for a recipient public key; `import_share_package` unwraps it; the decrypted file key matches the original.

## Real cloud tests (`scenarios_real_cloud.rs`)

Six tests — two per provider — that hit live cloud APIs using the bundled rclone sidecar. They are skipped automatically when the required env vars are absent, so they never block CI unless credentials are present.

### Providers and env vars

| Provider | Env vars required |
|----------|-------------------|
| Backblaze B2 | `ARX_TEST_B2_KEY_ID`, `ARX_TEST_B2_APP_KEY`, `ARX_TEST_B2_BUCKET` |
| Google Drive | `ARX_TEST_GDRIVE_REFRESH_TOKEN` |
| OneDrive | `ARX_TEST_ONEDRIVE_REFRESH_TOKEN`, `ARX_TEST_ONEDRIVE_DRIVE_ID` |

Store credentials in `.env.test` (already gitignored via `.gitignore`'s `.env.*` pattern) and source before running:

```powershell
# PowerShell
Get-Content .env.test | ForEach-Object {
    if ($_ -match '^([^#=]+)=(.+)$') { $env:($Matches[1]) = $Matches[2] }
}
```

```bash
# Bash / Git Bash
set -a && . .env.test && set +a
```

### What each test does

**`test_<provider>_push_vault_manifest_blob_present_after_sync`**
Creates a Tier 1 vault, derives keys, opens a `SqlCipherMetadataStore`, calls `push_vault` against a live `RcloneTransport`, then asserts that `manifest/manifest-backup.blob` is present in the cloud. Cleans up all blobs before asserting.

**`test_<provider>_backup_round_trip_bytes_survive_upload_and_download`**
Encrypts a small file with `upload_file` (stages chunks locally), calls `push_vault` to move chunks to cloud, then calls `decrypt_file` fetching chunks back from cloud. Asserts the decrypted bytes are identical to the original source. Cleans up all blobs before asserting.

### Isolation

Each test run generates a UUID-suffixed path prefix so concurrent runs never collide:

| Provider | Remote root composed by rclone |
|----------|-------------------------------|
| B2 | `arxb2test:arx-runa-test/ci-<uuid>` (bucket=`arx-runa-test`, prefix=`ci-<uuid>`) |
| Drive | `arxdrivetest:arx-runa-test/ci-<uuid>` (no bucket; prefix=`arx-runa-test/ci-<uuid>`) |
| OneDrive | `arxonedrivetest:arx-runa-test/ci-<uuid>` (no bucket; prefix=`arx-runa-test/ci-<uuid>`) |

Drive and OneDrive have no bucket layer, so the `arx-runa-test` segment is baked into the `path_prefix` to keep test blobs off the root of the user's cloud storage. Cleanup (`delete_blob` on every listed blob) runs before assertions so the folder stays empty even when a test fails mid-way.

### rclone config notes

- **B2**: `[arxb2test] type = b2 account = <key_id> key = <app_key>`
- **Drive**: `[arxdrivetest] type = drive scope = drive token = {...}` — requires a non-empty `access_token` placeholder (`"x"`) with a past expiry date so rclone recognises the token as expired and uses the refresh path rather than treating it as absent.
- **OneDrive**: `[arxonedrivetest] type = onedrive token = {...} drive_id = <id> drive_type = personal` — same placeholder trick; `drive_id` is the GUID visible in the OneDrive URL.

Token strings read from env vars pass through `strip_whitespace()` before use to tolerate newlines that terminal line-wrapping can silently inject into long values.
