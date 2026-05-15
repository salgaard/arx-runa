//! Scenario tests: multi-destination backup (Use Case 5).
//!
//! Tests mirror vs accumulating destination semantics at the storage layer,
//! using two independent MockCloudTransport instances driven in sequence.

use uuid::Uuid;

use crate::auth::ceremonies::test_support::*;
use crate::crypto::{KeyEncryptionKey, ManifestKey, SqlcipherKey};
use crate::storage::cloud::mock::{CloudTransportErrorKind, MockCloudTransport};
use crate::storage::cloud::{CloudTransport, MANIFEST_BACKUP_BLOB_NAME};
use crate::storage::{MetadataStore, SqlCipherMetadataStore, SyncConfig, push_vault, upload_file};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Uploads a small test file into the vault manifest and staged blobs.
///
/// Blobs are written to `staging_pending` (`staging_dir/pending/`) so that
/// `push_vault` (called with `staging_dir`) can locate them.
async fn stage_one_file(
    store: &SqlCipherMetadataStore,
    kek: &KeyEncryptionKey,
    staging_pending: &std::path::Path,
) -> (Uuid, crate::storage::Node) {
    let source_temp = temp_dir();
    let source_path = source_temp.path().join("data.bin");
    tokio::fs::write(&source_path, b"destination scenario test payload")
        .await
        .expect("source file must be writable");

    let node_id = Uuid::new_v4();
    let node = upload_file(
        &source_path,
        node_id,
        None,
        "data.bin",
        1_700_000_000,
        1_700_000_000,
        store,
        kek,
        staging_pending,
        None,
    )
    .await
    .expect("upload_file must succeed");
    (node_id, node)
}

// ---------------------------------------------------------------------------
// UC5: mirror destinations receive identical blob sets
// ---------------------------------------------------------------------------

/// Both mirror destinations are pushed in sequence; their cloud blob-name sets are identical.
///
/// A file is staged for transport A only (its pending blob is deleted after upload). Transport B
/// receives an independent push of the same vault metadata — both should have the same manifest
/// and vault-header blobs, confirming that mirror semantics produce identical vault structure.
#[tokio::test(flavor = "multi_thread")]
async fn test_mirror_destinations_receive_identical_blob_sets_after_sync() {
    let _lock = ceremony_lock().await;
    let vault = create_tier_one_vault().await;
    let derived = derive_vault_keys_tier_one(&vault);
    let sqlcipher_key = SqlcipherKey::from_bytes(derived.sqlcipher_key);
    let manifest_key = ManifestKey::from_bytes(derived.manifest_key);
    let sync_config = SyncConfig::default();

    // Open two independent metadata stores pointing at the same vault DB (simulating two
    // destination sessions reading the same local manifest).
    let store_a = SqlCipherMetadataStore::open(&vault.vault_db_path, &derived.sqlcipher_key)
        .await
        .expect("store A must open");
    let store_b = SqlCipherMetadataStore::open(&vault.vault_db_path, &derived.sqlcipher_key)
        .await
        .expect("store B must open");
    let staging_a = temp_dir();
    let staging_b = temp_dir();

    let transport_a = MockCloudTransport::new();
    let transport_b = MockCloudTransport::new();

    push_vault(
        &vault.vault_db_path,
        &sqlcipher_key,
        &manifest_key,
        &store_a,
        &transport_a,
        &vault.header,
        staging_a.path(),
        &sync_config,
        None,
    )
    .await
    .expect("push to transport A must succeed");

    push_vault(
        &vault.vault_db_path,
        &sqlcipher_key,
        &manifest_key,
        &store_b,
        &transport_b,
        &vault.header,
        staging_b.path(),
        &sync_config,
        None,
    )
    .await
    .expect("push to transport B must succeed");

    let mut blobs_a = transport_a
        .list_blobs("")
        .await
        .expect("list_blobs transport A must succeed");
    let mut blobs_b = transport_b
        .list_blobs("")
        .await
        .expect("list_blobs transport B must succeed");

    blobs_a.sort();
    blobs_b.sort();

    assert_eq!(
        blobs_a, blobs_b,
        "both mirror destinations must carry the same blob names"
    );
}

// ---------------------------------------------------------------------------
// UC5: mirror — blob absent after delete + sync
// ---------------------------------------------------------------------------

/// After a file is deleted from the manifest and the vault is pushed, the chunk blob is absent
/// from the cloud transport (pending_deletions drained).
#[tokio::test(flavor = "multi_thread")]
async fn test_mirror_destination_blob_absent_after_delete_and_sync() {
    let _lock = ceremony_lock().await;
    let vault = create_tier_one_vault().await;
    let derived = derive_vault_keys_tier_one(&vault);
    let sqlcipher_key = SqlcipherKey::from_bytes(derived.sqlcipher_key);
    let manifest_key = ManifestKey::from_bytes(derived.manifest_key);
    let kek = KeyEncryptionKey::from_bytes(derived.key_encryption_key);
    let sync_config = SyncConfig::default();

    let store = SqlCipherMetadataStore::open(&vault.vault_db_path, &derived.sqlcipher_key)
        .await
        .expect("store must open");
    let staging = temp_dir();
    let staging_pending = staging.path().join("pending");
    tokio::fs::create_dir_all(&staging_pending)
        .await
        .expect("staging/pending must be created");

    let transport = vault.cloud.clone();
    let (node_id, _) = stage_one_file(&store, &kek, &staging_pending).await;

    // First push — chunk blob lands in cloud.
    push_vault(
        &vault.vault_db_path,
        &sqlcipher_key,
        &manifest_key,
        &store,
        &transport,
        &vault.header,
        staging.path(),
        &sync_config,
        None,
    )
    .await
    .expect("first push must succeed");

    let blobs_before = transport
        .list_blobs("vault/")
        .await
        .expect("list_blobs must succeed after first push");
    assert!(
        !blobs_before.is_empty(),
        "chunk blob must be present in cloud after first push"
    );

    // Delete node from manifest — enqueues pending_deletions.
    store
        .delete_node(node_id)
        .await
        .expect("delete_node must succeed");

    // Second push — pending_deletions drained, blob removed from cloud.
    push_vault(
        &vault.vault_db_path,
        &sqlcipher_key,
        &manifest_key,
        &store,
        &transport,
        &vault.header,
        staging.path(),
        &sync_config,
        None,
    )
    .await
    .expect("second push must succeed");

    let blobs_after = transport
        .list_blobs("vault/")
        .await
        .expect("list_blobs must succeed after second push");
    assert!(
        blobs_after.is_empty(),
        "chunk blob must be absent from cloud after delete + push"
    );
}

// ---------------------------------------------------------------------------
// UC5: accumulating — blob retained when deletions are not pushed
// ---------------------------------------------------------------------------

/// Deleting a file from the manifest creates a pending deletion but does NOT remove the blob
/// from cloud if no subsequent push is made — the cloud acts as an accumulating destination.
#[tokio::test(flavor = "multi_thread")]
async fn test_accumulating_destination_retains_blob_after_file_deleted() {
    let _lock = ceremony_lock().await;
    let vault = create_tier_one_vault().await;
    let derived = derive_vault_keys_tier_one(&vault);
    let sqlcipher_key = SqlcipherKey::from_bytes(derived.sqlcipher_key);
    let manifest_key = ManifestKey::from_bytes(derived.manifest_key);
    let kek = KeyEncryptionKey::from_bytes(derived.key_encryption_key);
    let sync_config = SyncConfig::default();

    let store = SqlCipherMetadataStore::open(&vault.vault_db_path, &derived.sqlcipher_key)
        .await
        .expect("store must open");
    let staging = temp_dir();
    let staging_pending = staging.path().join("pending");
    tokio::fs::create_dir_all(&staging_pending)
        .await
        .expect("staging/pending must be created");

    let transport = vault.cloud.clone();
    let (node_id, _) = stage_one_file(&store, &kek, &staging_pending).await;

    // Push once — chunk blob lands in cloud.
    push_vault(
        &vault.vault_db_path,
        &sqlcipher_key,
        &manifest_key,
        &store,
        &transport,
        &vault.header,
        staging.path(),
        &sync_config,
        None,
    )
    .await
    .expect("push must succeed");

    // Delete node locally — pending deletion enqueued but NOT pushed.
    store
        .delete_node(node_id)
        .await
        .expect("delete_node must succeed");

    // Cloud blob must still be present (no push since deletion).
    let blobs = transport
        .list_blobs("vault/")
        .await
        .expect("list_blobs must succeed");
    assert!(
        !blobs.is_empty(),
        "chunk blob must remain in cloud when pending deletion is not pushed"
    );
}

// ---------------------------------------------------------------------------
// UC5: failure on one destination does not block the other
// ---------------------------------------------------------------------------

/// A manifest-upload failure on destination A is isolated — destination B receives a complete
/// successful push.
#[tokio::test(flavor = "multi_thread")]
async fn test_sync_failure_on_one_destination_does_not_prevent_other_destination_sync() {
    let _lock = ceremony_lock().await;
    let vault = create_tier_one_vault().await;
    let derived = derive_vault_keys_tier_one(&vault);
    let sqlcipher_key = SqlcipherKey::from_bytes(derived.sqlcipher_key);
    let manifest_key = ManifestKey::from_bytes(derived.manifest_key);
    let sync_config = SyncConfig::default();

    let store = SqlCipherMetadataStore::open(&vault.vault_db_path, &derived.sqlcipher_key)
        .await
        .expect("store must open");
    let staging = temp_dir();

    let transport_a = MockCloudTransport::new();
    let transport_b = MockCloudTransport::new();

    // Inject a one-shot Timeout failure on transport A for the manifest backup upload.
    transport_a
        .inject_failure(MANIFEST_BACKUP_BLOB_NAME, CloudTransportErrorKind::Timeout)
        .await;

    // Push to transport A — must fail during manifest backup upload.
    let result_a = push_vault(
        &vault.vault_db_path,
        &sqlcipher_key,
        &manifest_key,
        &store,
        &transport_a,
        &vault.header,
        staging.path(),
        &sync_config,
        None,
    )
    .await;
    assert!(
        result_a.is_err(),
        "push to transport A must fail due to injected manifest timeout"
    );

    // Push to transport B — must succeed independently.
    push_vault(
        &vault.vault_db_path,
        &sqlcipher_key,
        &manifest_key,
        &store,
        &transport_b,
        &vault.header,
        staging.path(),
        &sync_config,
        None,
    )
    .await
    .expect("push to transport B must succeed independently of transport A failure");

    let blobs_b = transport_b
        .list_blobs("")
        .await
        .expect("list_blobs on transport B must succeed");
    assert!(
        blobs_b.contains(&MANIFEST_BACKUP_BLOB_NAME.to_owned()),
        "transport B must have the manifest backup blob after successful push"
    );
}
