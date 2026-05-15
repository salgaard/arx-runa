# Add Scenario Test

Add a scenario-level integration test for: $ARGUMENTS

Scenario tests live in `src-tauri/src/tests/` and compose real library calls using `MockCloudTransport` and vault ceremony fixtures — no Tauri runtime, no UI, no external cloud.

---

## Step 1 — Classify the scenario

Map the description to the correct file:

| Keywords in description | Target file |
|---|---|
| recovery, phrase, password, key file, tier 1/2, rotate, lock, unlock, authenticate | `src-tauri/src/tests/scenarios_auth.rs` |
| encrypt, decrypt, upload file, download file, EXIF, pipeline, backup | `src-tauri/src/tests/scenarios_backup.rs` |
| sync, conflict, snapshot counter, manifest, cross-device, pull, push, stale | `src-tauri/src/tests/scenarios_sync.rs` |
| share, recipient, package, receipt, revoke, contact, sending | `src-tauri/src/tests/scenarios_sharing.rs` |
| destination, mirror, accumulating, primary, backup failure, multi-destination | `src-tauri/src/tests/scenarios_destinations.rs` |

Read the target file now to understand existing imports and the last test's structure.

---

## Step 2 — Know the test infrastructure

### Vault fixtures (`crate::auth::ceremonies::test_support::*`)

All available via `use crate::auth::ceremonies::test_support::*;` in `#[cfg(test)]` code:

```rust
// Constants
TEST_PASSWORD: &[u8]       // b"correct horse battery staple"
TEST_NEW_PASSWORD: &[u8]   // b"stapler battery horse correct"
TEST_WRONG_PASSWORD: &[u8] // b"not the password"

// Fixtures
create_tier_one_vault() -> TierOneVault
// TierOneVault { _temp, vault_db_path, cloud: MockCloudTransport, session: SessionManager, vault_id, header }

create_tier_two_vault() -> TierTwoVault
// TierTwoVault { _temp, vault_db_path, key_file_path, cloud, session, vault_id, header }

// Helpers
add_recovery_slot_and_return_phrase(vault: &mut TierOneVault) -> Zeroizing<String>
upload_manifest_backup_for(vault: &TierOneVault)              // required before phrase recovery
upload_manifest_backup_payload_for(vault, payload: &[u8])    // arbitrary payload
upload_corrupted_manifest_backup_for(vault: &TierOneVault)   // for corruption tests
ceremony_lock() -> MutexGuard<'static, ()>                   // serialise all ceremony tests
test_params() -> Argon2Params                                 // fast: 1 MiB, t=1, p=1
test_session_manager() -> SessionManager                     // 1-hour timeout
temp_dir() -> TempDir
```

### Cloud transport (`crate::storage::cloud::mock::MockCloudTransport`)

```rust
MockCloudTransport::new()   // empty in-memory transport
cloud.inject_failure("vault-header.json", CloudTransportErrorKind::Timeout)  // one-shot
// Error kinds: NotFound | AuthenticationFailed | Timeout | IoError{..} | RcloneProcessFailed{..} | Other(String)
// Implements CloudTransport: upload_blob, download_blob, delete_blob, list_blobs
// Clone shares the same backing HashMap
```

### Ceremony APIs (all pub via `crate::auth::*`)

```rust
create_vault(CreateVaultRequest, &SessionManager, &dyn TransportProvider) -> VaultId
change_password(ChangePasswordRequest, &SessionManager, &dyn TransportProvider, &mut VaultHeader, &VaultId)
recover_with_phrase(RecoverWithPhraseRequest, &SessionManager, &dyn TransportProvider) -> (VaultId, VaultHeader)
setup_recovery(SetupRecoveryRequest, &SessionManager, &dyn TransportProvider, &mut VaultHeader, &VaultId)
rotate_key_file(RotateKeyFileRequest, &SessionManager, &dyn TransportProvider, &mut VaultHeader, &VaultId)
```

### Key types

```rust
use crate::auth::{
    Argon2MigrationIntent, AuthenticationError, ChangePasswordRequest,
    MockKeySource,  // MockKeySource::new([u8; 32]) — only in #[cfg(test)]
    RecoverWithPhraseRequest, RotateKeyFileRequest, SetupRecoveryRequest,
    change_password, recover_with_phrase, rotate_key_file, setup_recovery,
};
```

### Test naming convention (from `rust.md`)

```
test_<unit>_<scenario>_<expected_outcome>
```

Examples already in the codebase:
- `test_tier1_full_recovery_via_phrase_restores_active_session`
- `test_tier2_key_rotation_old_key_rejected_on_reuse`
- `test_change_password_with_phrase_recovery_slot_preserved_in_header`

### Test template

```rust
#[tokio::test(flavor = "multi_thread")]
async fn test_<unit>_<scenario>_<expected_outcome>() {
    let _lock = ceremony_lock().await;

    // Arrange
    let mut vault = create_tier_one_vault().await;
    // ... setup

    // Act
    let result = some_ceremony(...).await;

    // Assert
    assert!(matches!(result, Ok(_)), "..., got: {result:?}");
}
```

---

## Step 3 — Design the test

Present:
1. Proposed test name following `test_<unit>_<scenario>_<expected_outcome>`
2. Arrange / Act / Assert outline
3. Which helpers from Step 2 will be used
4. Expected assertion

**Stop here and wait for approval before writing any code.**

---

## Step 4 — Implement

Write the test to the target file. Follow all existing indentation and import patterns in that file.

---

## Step 5 — Verify

Run:
```
cargo test --package arx-runa-tauri -- tests::scenarios_<category>::<test_name> --nocapture
```

Report the test output. If it fails, diagnose and fix before reporting success.
