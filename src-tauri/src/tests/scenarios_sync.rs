//! Scenario tests: cross-device sync (Use Case 2 — Cross-Device Access).
//!
//! Tests snapshot_counter conflict detection and conflict-copy renaming.

use crate::auth::ceremonies::test_support::*;
use crate::crypto::{ManifestKey, SqlcipherKey};
use crate::storage::{SqlCipherMetadataStore, SyncConfig, SyncError, push_vault};
use crate::ui::sync_commands::conflict_name;

/// Conflict copy name appends the expected suffix before the file extension.
#[test]
fn test_conflict_copy_name_has_correct_suffix_before_extension() {
    let result = conflict_name("photo.jpg");
    assert!(
        result.ends_with(".jpg"),
        "extension must be preserved, got: {result}"
    );
    assert!(
        result.contains("conflicted copy"),
        "name must contain 'conflicted copy', got: {result}"
    );
    assert!(
        result.starts_with("photo"),
        "original stem must be preserved, got: {result}"
    );
}

/// Conflict copy for a file without an extension appends the suffix at the end.
#[test]
fn test_conflict_copy_name_no_extension_suffix_appended_at_end() {
    let result = conflict_name("Makefile");
    assert!(
        result.starts_with("Makefile"),
        "original name must be preserved as prefix, got: {result}"
    );
    assert!(
        result.contains("conflicted copy"),
        "name must contain 'conflicted copy', got: {result}"
    );
}

// ---------------------------------------------------------------------------
// UC2: snapshot_counter conflict detection
// ---------------------------------------------------------------------------

/// Device A pushes (counter → 1), device B is initialised from A's DB and pushes (counter → 2),
/// then A's second push detects the stale counter and returns SyncError::Conflict.
#[tokio::test(flavor = "multi_thread")]
async fn test_push_detects_stale_manifest_snapshot_counter_returns_conflict() {
    let _lock = ceremony_lock().await;
    let vault = create_tier_one_vault().await;
    let derived = derive_vault_keys_tier_one(&vault);
    let sqlcipher_key = SqlcipherKey::from_bytes(derived.sqlcipher_key);
    let manifest_key = ManifestKey::from_bytes(derived.manifest_key);
    let sync_config = SyncConfig::default();

    let store_a = SqlCipherMetadataStore::open(&vault.vault_db_path, &derived.sqlcipher_key)
        .await
        .expect("device A store must open");
    let staging_a = temp_dir();

    // Device A: first push → cloud counter = 1, A's DB counter = 1.
    push_vault(
        &vault.vault_db_path,
        &sqlcipher_key,
        &manifest_key,
        &store_a,
        &vault.cloud,
        &vault.header,
        staging_a.path(),
        &sync_config,
        None,
    )
    .await
    .expect("device A first push must succeed");

    // Device B: copy A's DB (counter = 1) to a new path and push → cloud counter = 2.
    let device_b_temp = temp_dir();
    let device_b_db = device_b_temp.path().join("vault-b.db");
    tokio::fs::copy(&vault.vault_db_path, &device_b_db)
        .await
        .expect("vault DB copy to device B must succeed");
    let store_b = SqlCipherMetadataStore::open(&device_b_db, &derived.sqlcipher_key)
        .await
        .expect("device B store must open");
    let staging_b = temp_dir();

    push_vault(
        &device_b_db,
        &sqlcipher_key,
        &manifest_key,
        &store_b,
        &vault.cloud,
        &vault.header,
        staging_b.path(),
        &sync_config,
        None,
    )
    .await
    .expect("device B push must succeed");

    // Device A: second push → local counter = 1, cloud counter = 2 → Conflict.
    let result = push_vault(
        &vault.vault_db_path,
        &sqlcipher_key,
        &manifest_key,
        &store_a,
        &vault.cloud,
        &vault.header,
        staging_a.path(),
        &sync_config,
        None,
    )
    .await;

    assert!(
        matches!(result, Err(SyncError::Conflict(_))),
        "second push from device A must return SyncError::Conflict, got: {result:?}"
    );
}
