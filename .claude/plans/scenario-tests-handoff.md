# Scenario Tests — Handoff Prompt for New Session

## What this is

You are continuing work on scenario/integration tests for **Arx Runa** — a zero-knowledge bring-your-own-cloud file encryption desktop app (Tauri + Rust + Leptos). The goal is to automate the manual testing the developer currently does (vault recovery, sync, destinations, sharing, backup pipeline).

## What's already done — DO NOT redo

### Infrastructure (already merged into working tree)
- `src-tauri/src/auth/ceremonies/test_support.rs` — all `pub(super)` changed to `pub(crate)`
- `src-tauri/src/auth/ceremonies/mod.rs` — `mod test_support` is now `pub(crate) mod test_support`
- `src-tauri/src/lib.rs` — has `#[cfg(test)] mod tests;`
- `src-tauri/src/ui/sync_commands.rs` — `conflict_name` is now `pub(crate)`
- `src-tauri/src/tests/mod.rs` — wires 5 scenario submodules

### Already passing tests (9 total)
Run with: `cargo test --package arx-runa-tauri -- tests::scenarios`

**`src-tauri/src/tests/scenarios_auth.rs`** (5 tests):
- `test_tier1_full_recovery_via_phrase_restores_active_session`
- `test_tier2_key_rotation_old_key_rejected_on_reuse`
- `test_change_password_with_phrase_recovery_slot_preserved_in_header`
- `test_change_password_without_phrase_recovery_slot_cleared`
- `test_recovery_phrase_non_bip39_word_rejected_as_invalid_recovery_phrase`

**`src-tauri/src/tests/scenarios_sync.rs`** (2 tests):
- `test_conflict_copy_name_has_correct_suffix_before_extension`
- `test_conflict_copy_name_no_extension_suffix_appended_at_end`

**`src-tauri/tests/rclone_integration.rs`** (1 test, env-gated):
- `test_rclone_transport_round_trip_with_local_remote` — runs with `ARX_RCLONE_INTEGRATION=1`

### Stub files (have doc comments only, no tests yet)
- `src-tauri/src/tests/scenarios_backup.rs`
- `src-tauri/src/tests/scenarios_sharing.rs`
- `src-tauri/src/tests/scenarios_destinations.rs`

---

## Test infrastructure you must use

### Vault fixtures — import with `use crate::auth::ceremonies::test_support::*;`
```rust
TEST_PASSWORD: &[u8]       // b"correct horse battery staple"
TEST_NEW_PASSWORD: &[u8]   // b"stapler battery horse correct"
TEST_WRONG_PASSWORD: &[u8] // b"not the password"

create_tier_one_vault() -> TierOneVault
// fields: _temp, vault_db_path, cloud: MockCloudTransport, session: SessionManager, vault_id, header

create_tier_two_vault() -> TierTwoVault
// fields: _temp, vault_db_path, key_file_path, cloud, session, vault_id, header

add_recovery_slot_and_return_phrase(vault: &mut TierOneVault) -> Zeroizing<String>
upload_manifest_backup_for(vault: &TierOneVault)
upload_manifest_backup_payload_for(vault: &TierOneVault, payload: &[u8])
upload_corrupted_manifest_backup_for(vault: &TierOneVault)
ceremony_lock() -> MutexGuard<'static, ()>   // ALL ceremony tests must hold this
test_params() -> Argon2Params                // fast: 1 MiB, t=1, p=1
test_session_manager() -> SessionManager    // 1-hour timeout
temp_dir() -> TempDir
```

### Key types — all pub via `crate::auth::*`
```rust
use crate::auth::{
    Argon2MigrationIntent, AuthenticationError,
    ChangePasswordRequest, RecoverWithPhraseRequest, RotateKeyFileRequest, SetupRecoveryRequest,
    MockKeySource,   // MockKeySource::new([u8; 32]) — only in #[cfg(test)]
    change_password, recover_with_phrase, rotate_key_file, setup_recovery,
    LifecycleState,
};
```

### MockCloudTransport
```rust
use crate::storage::cloud::mock::{CloudTransportErrorKind, MockCloudTransport};
// MockCloudTransport::new()  — empty in-memory backing HashMap
// .inject_failure(path, CloudTransportErrorKind::Timeout)  — one-shot
// Error kinds: NotFound | AuthenticationFailed | Timeout | IoError{..} | RcloneProcessFailed{..} | Other(String)
// Clone: shares the same backing Arc<Mutex<HashMap>>
// Implements CloudTransport: upload_blob, download_blob, delete_blob, list_blobs
```

### Test pattern
```rust
#[tokio::test(flavor = "multi_thread")]
async fn test_<unit>_<scenario>_<expected_outcome>() {
    let _lock = ceremony_lock().await;
    // arrange → act → assert
}
```

---

## Remaining scenarios to implement

### Priority 1 — `scenarios_auth.rs` (missing Tier 2 recovery)

**Problem**: `add_recovery_slot_and_return_phrase` only accepts `&mut TierOneVault`.
For Tier 2 recovery, you need to call `setup_recovery` directly with a key source.
Add a helper to `test_support.rs` (already `pub(crate)`) or inline it.

```
test_tier2_full_recovery_via_phrase_creates_new_key_file_and_restores_session:
  create_tier_two_vault → call setup_recovery with MockKeySource from key_file_path
  → upload_manifest_backup (inline, using Tier2 key derivation) → lock →
  recover_with_phrase(new_key_file_path=Some(...)) → assert session Active + new key file 32 bytes
```

**Key issue for Tier 2 manifest backup**: `upload_manifest_backup_for` derives master key with
`derive_master_key_into(TEST_PASSWORD, None, ...)`. For Tier 2 it must be
`derive_master_key_into(TEST_PASSWORD, Some(&key_file_bytes), ...)`.
The `derive_master_key_into` and `SessionKeys` are both accessible via `crate::auth::kdf` and
`crate::auth::session`. The `sqlcipher_key_from_array` and `argon2_params_from_json` are in
`crate::auth::ceremonies::helpers` (private) — check if accessible or replicate inline.

---

### Priority 2 — `scenarios_backup.rs` (UC1 — Personal Backup pipeline)

First, investigate the public entry points for the file pipeline. The key question is:
what is the highest-level function accessible without going through Tauri `AppState`?
Look at `src-tauri/src/storage/pipeline/mod.rs` and `src-tauri/src/storage/vault_ops/upload_file.rs`.

```
test_backup_encrypt_decrypt_round_trip_bytes_identical:
  create_tier_one_vault → open SqlCipherMetadataStore (derive keys from vault) →
  write test bytes to temp file → encrypt+stage → upload chunks to vault.cloud →
  download chunks → decrypt → assert output == input bytes

test_exif_stripped_from_jpeg_before_staging:
  create_tier_one_vault → build minimal JPEG with APP1/EXIF segment (use exif.rs) →
  encrypt+stage → read staged blob → parse JPEG segments → assert no APP1 segment
  (Note: src-tauri/src/storage/pipeline/exif.rs is a new file in the current working tree)
```

---

### Priority 3 — `scenarios_sync.rs` (UC2 — Cross-Device Sync)

Investigate `src-tauri/src/storage/cloud/sync.rs` for library-level sync functions that
don't require `AppState`. Key function to find: whatever `sync_to_cloud` in
`ui/sync_commands.rs` calls internally.

```
test_push_detects_stale_manifest_snapshot_counter_returns_conflict:
  Create vault (device A) → push → open second SqlCipherMetadataStore with same vault_id
  and same MockCloudTransport (Clone) → device B pushes (increments counter) →
  device A tries to push again → assert SyncError::Conflict (or equivalent stale-manifest error)
```

---

### Priority 4 — `scenarios_destinations.rs` (UC5 — Multi-Destination)

**Important**: `sync_to_cloud` and `sync_backup` in `ui/sync_commands.rs` take
`State<'_, AppState>` (Tauri IPC). These cannot be called directly from library tests.
The approach is to test the STORAGE LAYER directly using two `MockCloudTransport` instances
driven in sequence (simulating what `sync_backup` does for each destination).

Look at `src-tauri/src/storage/cloud/sync.rs` for `push_vault` / `pull_vault` — these
likely take a single `&dyn CloudTransport` and are the right level to test.

```
test_mirror_destinations_receive_identical_blob_sets_after_sync:
  Create vault → upload a file → push_vault to transport_A → push_vault to transport_B →
  list_blobs on A and B → assert identical sets

test_mirror_destination_blob_absent_after_delete_and_sync:
  push file to mirror transport → delete file from vault manifest →
  sync (call delete_blob on mirror for removed blobs) → list_blobs →
  assert blob gone

test_accumulating_destination_retains_blob_after_file_deleted:
  push file to accumulating transport → delete from vault manifest →
  do NOT call delete_blob on accumulating transport → list_blobs →
  assert blob still present (accumulating never deletes)

test_primary_destination_cannot_be_deleted:
  Investigate DestinationSession/destination management API to find the
  "delete destination" function and assert it returns an error when the target
  is the primary destination.

test_sync_failure_on_one_destination_does_not_prevent_other_destination_sync:
  Two transports: inject Timeout failure on transport_A → sync to both →
  assert transport_B has all blobs despite transport_A failure
```

---

### Priority 5 — `scenarios_sharing.rs` (UC4 — File Sharing)

Investigate `src-tauri/src/sharing/packages.rs` for `create_share_package` and
`import_share_package`. These are likely library-level and testable without AppState.

```
test_share_package_round_trip_recipient_decrypts_file_key:
  Create sender vault → upload file → retrieve file key from metadata store →
  create SharingStore for sender (with keypair) → create SharingStore for recipient
  (different keypair) → create_share_package(file_key, recipient_public_key) →
  import_share_package on recipient's store → assert decrypted file key matches original

test_download_receipt_written_to_cloud_after_recipient_accesses_file:
  Extend above → recipient "downloads" share →
  assert blob exists under "shared/<share_id>/receipts/" in MockCloudTransport
```

---

### Priority 6 — Real cloud destination integration tests (env-gated)

Pattern: follow `src-tauri/tests/rclone_integration.rs` — gate with env var check at top.

Proposed env vars to check (user will provide values):
- `ARX_B2_BUCKET`, `ARX_B2_KEY_ID`, `ARX_B2_APP_KEY` — Backblaze B2
- `ARX_GDRIVE_SERVICE_ACCOUNT_JSON` — Google Drive service account path
- `ARX_S3_ENDPOINT`, `ARX_S3_ACCESS_KEY`, `ARX_S3_SECRET_KEY`, `ARX_S3_BUCKET` — S3-compatible
- `ARX_LOCAL_PATH` — local filesystem (easiest, no credentials)

For each provider: full round-trip (create vault → push → pull → verify manifest integrity).
These go in a new file `src-tauri/tests/scenarios_cloud_destinations.rs`.

The user has cloud credentials to provide — ask them before implementing cloud tests.

---

## How to run

```powershell
# All scenario tests (fast, no cloud)
cargo test --package arx-runa-tauri -- tests::scenarios --nocapture

# Single category
cargo test --package arx-runa-tauri -- tests::scenarios_destinations --nocapture

# Real cloud tests (once env vars set)
$env:ARX_RCLONE_INTEGRATION = "1"; cargo test --package arx-runa-tauri -- scenarios_cloud
```

## Naming convention (from rust.md)
`test_<unit>_<scenario>_<expected_outcome>`

## Hard rules (from CLAUDE.md)
- Never write unencrypted sensitive data to disk
- No abbreviations (`chunk_index` not `chunk_idx`)
- `pub(crate)` inside `#[cfg(test)]` — never expose test helpers to production surface
- `cargo fmt` + `cargo clippy -- -D warnings` before commit
