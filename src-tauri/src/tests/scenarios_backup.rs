//! Scenario tests: backup pipeline (Use Case 1 — Zero-Knowledge Personal Backup).
//!
//! Tests the full encrypt → stage → upload → download → decrypt round trip,
//! and that EXIF metadata is stripped before chunks leave the client.

use std::collections::HashSet;

use uuid::Uuid;

use crate::auth::ceremonies::test_support::*;
use crate::crypto::{FileId, KeyEncryptionKey, WrappedFileKey, unwrap_file_key};
use crate::storage::pipeline::exif::strip_exif;
use crate::storage::{
    MetadataStore, SqlCipherMetadataStore, decrypt_file, download_file_to_memory, upload_file,
};

// ---------------------------------------------------------------------------
// Filesystem monitoring helper
// ---------------------------------------------------------------------------

/// Captures a snapshot of all files in a directory and can assert that no
/// non-`.blob` files were added after the snapshot.
struct FileSystemMonitor {
    dir: std::path::PathBuf,
    baseline: HashSet<std::path::PathBuf>,
}

impl FileSystemMonitor {
    /// Record the current set of files under `dir` (non-recursive).
    fn new(dir: &std::path::Path) -> Self {
        let baseline = Self::list_files(dir);
        Self {
            dir: dir.to_owned(),
            baseline,
        }
    }

    fn list_files(dir: &std::path::Path) -> HashSet<std::path::PathBuf> {
        std::fs::read_dir(dir)
            .map(|entries| {
                entries
                    .flatten()
                    .map(|e| e.path())
                    .filter(|p| p.is_file())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Assert that no files with a non-`.blob` extension were added since snapshot.
    fn assert_no_new_non_blob_files(&self) {
        let current = Self::list_files(&self.dir);
        let new_files: Vec<_> = current
            .difference(&self.baseline)
            .filter(|p| {
                p.extension()
                    .and_then(|e| e.to_str())
                    .map(|ext| ext != "blob")
                    .unwrap_or(true)
            })
            .collect();
        assert!(
            new_files.is_empty(),
            "Zero-Trace violation: non-blob files appeared in staging dir:\n  {}",
            new_files
                .iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
                .join("\n  "),
        );
    }

    /// Assert that no NEW files matching a pattern appeared since the snapshot.
    fn assert_no_files_matching(&self, pattern: &str) {
        let current = Self::list_files(&self.dir);
        let matches: Vec<_> = current
            .difference(&self.baseline)
            .filter(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .map(|n| n.contains(pattern))
                    .unwrap_or(false)
            })
            .collect();
        assert!(
            matches.is_empty(),
            "Zero-Trace violation: files matching `{}` found in dir:\n  {}",
            pattern,
            matches
                .iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
                .join("\n  "),
        );
    }
}

// ---------------------------------------------------------------------------
// UC1: encrypt → stage → decrypt round trip
// ---------------------------------------------------------------------------

/// Full round trip: upload a small file, decrypt from the staged blobs, bytes match exactly.
#[tokio::test(flavor = "multi_thread")]
async fn test_backup_encrypt_decrypt_round_trip_bytes_identical() {
    let _lock = ceremony_lock().await;
    let vault = create_tier_one_vault().await;
    let derived = derive_vault_keys_tier_one(&vault);
    let store = SqlCipherMetadataStore::open(&vault.vault_db_path, &derived.sqlcipher_key)
        .await
        .expect("store must open with derived sqlcipher key");
    let kek = KeyEncryptionKey::from_bytes(derived.key_encryption_key);
    let staging_dir = temp_dir();

    let source_bytes: &[u8] = b"hello arx runa round trip test";
    let source_temp = temp_dir();
    let source_path = source_temp.path().join("input.bin");
    tokio::fs::write(&source_path, source_bytes)
        .await
        .expect("source file must be writable");

    let node_id = Uuid::new_v4();
    let node = upload_file(
        &source_path,
        node_id,
        None,
        "input.bin",
        1_700_000_000,
        1_700_000_000,
        &store,
        &kek,
        staging_dir.path(),
        None,
    )
    .await
    .expect("upload_file must succeed");

    let wrapped_bytes = node
        .file_key_wrapped
        .expect("uploaded file node must carry a wrapped file key");
    let file_key = unwrap_file_key(
        &WrappedFileKey::new(wrapped_bytes),
        &FileId::from_uuid(node_id),
        &kek,
    )
    .expect("file key must unwrap with the same KEK used at upload");

    let chunks = store
        .get_chunks(node_id)
        .await
        .expect("chunks must be queryable after upload");

    let dest_temp = temp_dir();
    let dest_path = dest_temp.path().join("output.bin");
    decrypt_file(
        &dest_path,
        node_id,
        &file_key,
        source_bytes.len() as u64,
        &chunks,
        staging_dir.path(),
        &store,
        None,
    )
    .await
    .expect("decrypt_file must succeed");

    let result = tokio::fs::read(&dest_path)
        .await
        .expect("decrypted output must be readable");
    assert_eq!(
        result, source_bytes,
        "decrypted bytes must be identical to original"
    );
}

// ---------------------------------------------------------------------------
// UC1: EXIF stripping before staging
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Zero-Trace: filesystem monitoring
// ---------------------------------------------------------------------------

/// Verifies that uploading and then downloading to memory leaves only encrypted
/// `.blob` files in the staging directory — no plaintext files of any kind.
#[tokio::test(flavor = "multi_thread")]
async fn test_no_plaintext_file_in_staging_after_upload_and_download_to_memory() {
    let _lock = ceremony_lock().await;
    let vault = create_tier_one_vault().await;
    let derived = derive_vault_keys_tier_one(&vault);
    let store = SqlCipherMetadataStore::open(&vault.vault_db_path, &derived.sqlcipher_key)
        .await
        .expect("store must open");
    let kek = KeyEncryptionKey::from_bytes(derived.key_encryption_key);
    let staging_dir = temp_dir();

    let source_bytes: &[u8] = b"zero trace filesystem test payload";
    let source_temp = temp_dir();
    let source_path = source_temp.path().join("input.bin");
    tokio::fs::write(&source_path, source_bytes)
        .await
        .expect("source must be writable");

    // Snapshot staging dir before upload.
    let monitor = FileSystemMonitor::new(staging_dir.path());

    let node_id = Uuid::new_v4();
    upload_file(
        &source_path,
        node_id,
        None,
        "input.bin",
        1_700_000_000,
        1_700_000_000,
        &store,
        &kek,
        staging_dir.path(),
        None,
    )
    .await
    .expect("upload_file must succeed");

    // After upload, only .blob files should have appeared.
    monitor.assert_no_new_non_blob_files();

    // Download to memory — no additional files should appear in staging.
    let result = download_file_to_memory(node_id, &store, &kek, staging_dir.path(), None)
        .await
        .expect("download_file_to_memory must succeed");

    monitor.assert_no_new_non_blob_files();
    assert_eq!(
        result.as_slice(),
        source_bytes,
        "in-memory decryption must recover original bytes"
    );
}

/// Verifies that a failed decrypt (tampered blob) does not leave any
/// `.arx-runa-decrypt-*.tmp` files behind in the destination directory.
#[tokio::test(flavor = "multi_thread")]
async fn test_atomic_temp_file_not_left_on_decrypt_error() {
    let _lock = ceremony_lock().await;
    let vault = create_tier_one_vault().await;
    let derived = derive_vault_keys_tier_one(&vault);
    let store = SqlCipherMetadataStore::open(&vault.vault_db_path, &derived.sqlcipher_key)
        .await
        .expect("store must open");
    let kek = KeyEncryptionKey::from_bytes(derived.key_encryption_key);
    let staging_dir = temp_dir();

    let source_bytes = vec![0xABu8; 1024];
    let source_temp = temp_dir();
    let source_path = source_temp.path().join("source.bin");
    tokio::fs::write(&source_path, &source_bytes)
        .await
        .expect("source must be writable");

    let node_id = Uuid::new_v4();
    let node = upload_file(
        &source_path,
        node_id,
        None,
        "source.bin",
        1_700_000_000,
        1_700_000_000,
        &store,
        &kek,
        staging_dir.path(),
        None,
    )
    .await
    .expect("upload_file must succeed");

    // Tamper with the first blob to force a checksum mismatch.
    let wrapped_bytes = node.file_key_wrapped.expect("node must carry wrapped key");
    let file_key = unwrap_file_key(
        &WrappedFileKey::new(wrapped_bytes),
        &FileId::from_uuid(node_id),
        &kek,
    )
    .expect("key must unwrap");
    let chunks = store
        .get_chunks(node_id)
        .await
        .expect("chunks must be queryable");
    let blob_path = staging_dir
        .path()
        .join(format!("{}.blob", chunks[0].blob_name));
    let mut blob_bytes = tokio::fs::read(&blob_path)
        .await
        .expect("blob must be readable");
    blob_bytes[0] ^= 0xFF;
    tokio::fs::write(&blob_path, &blob_bytes)
        .await
        .expect("tampered blob must be writable");

    let dest_temp = temp_dir();
    let dest_path = dest_temp.path().join("output.bin");
    let monitor = FileSystemMonitor::new(dest_temp.path());

    // Decrypt must fail due to checksum mismatch.
    let result = decrypt_file(
        &dest_path,
        node_id,
        &file_key,
        source_bytes.len() as u64,
        &chunks,
        staging_dir.path(),
        &store,
        None,
    )
    .await;

    assert!(result.is_err(), "decrypt must fail on tampered blob");

    // No .tmp files must remain in the destination directory.
    monitor.assert_no_files_matching(".tmp");
}

/// Verifies that `download_file_to_memory` does not create any files in
/// `std::env::temp_dir()` with "arx-runa" in their name.
///
/// This guards against reintroduction of the `tempfile::tempdir()` pattern
/// that was removed from `get_file_content`.
#[tokio::test(flavor = "multi_thread")]
async fn test_download_to_memory_leaves_no_os_temp_files() {
    let _lock = ceremony_lock().await;
    let vault = create_tier_one_vault().await;
    let derived = derive_vault_keys_tier_one(&vault);
    let store = SqlCipherMetadataStore::open(&vault.vault_db_path, &derived.sqlcipher_key)
        .await
        .expect("store must open");
    let kek = KeyEncryptionKey::from_bytes(derived.key_encryption_key);
    let staging_dir = temp_dir();

    let source_bytes: &[u8] = b"os temp dir zero trace check";
    let source_temp = temp_dir();
    let source_path = source_temp.path().join("input.bin");
    tokio::fs::write(&source_path, source_bytes)
        .await
        .expect("source must be writable");

    let node_id = Uuid::new_v4();
    upload_file(
        &source_path,
        node_id,
        None,
        "input.bin",
        1_700_000_000,
        1_700_000_000,
        &store,
        &kek,
        staging_dir.path(),
        None,
    )
    .await
    .expect("upload must succeed");

    // Snapshot OS temp dir before decrypt.
    let os_temp = std::env::temp_dir();
    let os_monitor = FileSystemMonitor::new(&os_temp);

    download_file_to_memory(node_id, &store, &kek, staging_dir.path(), None)
        .await
        .expect("download_to_memory must succeed");

    // No arx-runa files must appear in OS temp dir.
    os_monitor.assert_no_files_matching("arx-runa");
}

// ---------------------------------------------------------------------------
// UC1: EXIF stripping before staging
// ---------------------------------------------------------------------------

/// JPEG with an APP1 segment has that segment stripped by strip_exif before any chunk leaves
/// the client.
#[test]
fn test_exif_stripped_from_jpeg_before_staging() {
    // Minimal JPEG: SOI + APP1 (marker FF E1, length 14, "Exif\0\0" + 6 bytes) + EOI.
    // The length field (0x000E = 14) includes its own 2 bytes + 12 bytes of payload.
    let jpeg_with_exif: Vec<u8> = vec![
        0xFF, 0xD8, // SOI
        0xFF, 0xE1, // APP1 marker
        0x00, 0x0E, // length = 14
        0x45, 0x78, 0x69, 0x66, 0x00, 0x00, // "Exif\0\0"
        0xDE, 0xAD, 0xBE, 0xEF, 0xCA, 0xFE, // fake EXIF payload
        0xFF, 0xD9, // EOI
    ];

    let stripped = strip_exif(jpeg_with_exif);

    assert!(
        !stripped.windows(2).any(|w| w == [0xFF, 0xE1]),
        "APP1 marker must not appear in stripped output"
    );
}

// ---------------------------------------------------------------------------
// sync_to_destination: per-destination backup logic
// ---------------------------------------------------------------------------

mod sync_to_destination_tests {
    use std::collections::HashSet;

    use tauri::ipc::Channel;
    use uuid::Uuid;

    use crate::auth::ceremonies::test_support::*;
    use crate::crypto::{ManifestKey, SqlcipherKey};
    use crate::storage::SqlCipherMetadataStore;
    use crate::storage::cloud::mock::{CloudTransportErrorKind, MockCloudTransport};
    use crate::storage::cloud::{CloudTransport, CloudTransportError};
    use crate::ui::sync_commands::sync_to_destination;
    use crate::ui::types::SyncProgressUpdate;

    const DEST_ID: &str = "test-backup-dest";

    fn noop_progress() -> Channel<SyncProgressUpdate> {
        Channel::new(|_| Ok(()))
    }

    struct Ctx {
        vault: TierOneVault,
        store: SqlCipherMetadataStore,
        sqlcipher_key: SqlcipherKey,
        manifest_key: ManifestKey,
        staging: tempfile::TempDir,
        mirror_tmp: tempfile::TempDir,
    }

    async fn setup() -> Ctx {
        let vault = create_tier_one_vault().await;
        let derived = derive_vault_keys_tier_one(&vault);
        let sqlcipher_key = SqlcipherKey::from_bytes(derived.sqlcipher_key);
        let manifest_key = ManifestKey::from_bytes(derived.manifest_key);
        let store = SqlCipherMetadataStore::open(&vault.vault_db_path, &derived.sqlcipher_key)
            .await
            .expect("store must open");
        Ctx {
            vault,
            store,
            sqlcipher_key,
            manifest_key,
            staging: temp_dir(),
            mirror_tmp: temp_dir(),
        }
    }

    /// Pending blob present in staging dir → upload succeeds, counts.uploaded = 1,
    /// pending record cleared, blob reachable on dest transport.
    #[tokio::test(flavor = "multi_thread")]
    async fn test_sync_to_destination_pending_blob_upload_succeeds_records_cleared() {
        let _lock = ceremony_lock().await;
        let ctx = setup().await;

        let blob_name = Uuid::new_v4().to_string();
        tokio::fs::write(
            ctx.staging.path().join(format!("{blob_name}.blob")),
            b"encrypted blob payload",
        )
        .await
        .unwrap();

        let mut all_blobs = HashSet::new();
        all_blobs.insert(blob_name.clone());
        ctx.store
            .bulk_insert_pending_backup(std::slice::from_ref(&blob_name), DEST_ID)
            .await
            .unwrap();

        let dest = MockCloudTransport::new();
        let primary = MockCloudTransport::new();

        let counts = sync_to_destination(
            DEST_ID,
            std::slice::from_ref(&blob_name),
            &all_blobs,
            &dest,
            &primary,
            &ctx.store,
            &ctx.vault.vault_db_path,
            &ctx.sqlcipher_key,
            &ctx.manifest_key,
            &ctx.vault.vault_id,
            &ctx.vault.header,
            ctx.staging.path(),
            ctx.mirror_tmp.path(),
            false,
            &noop_progress(),
        )
        .await
        .expect("sync_to_destination must succeed");

        assert_eq!(counts.uploaded, 1, "one blob must be reported uploaded");
        assert_eq!(counts.failed, 0, "no failures expected");

        assert!(
            ctx.store
                .list_pending_backups(DEST_ID)
                .await
                .unwrap()
                .is_empty(),
            "pending record must be cleared on success"
        );

        let verify = ctx.mirror_tmp.path().join("verify.bin");
        dest.download_blob(&format!("vault/{blob_name}.blob"), &verify)
            .await
            .expect("uploaded blob must be reachable on dest transport");
    }

    /// Blob in pending queue but absent from all_blob_names (orphan) → records cleared,
    /// no upload attempted, counts all zero.
    #[tokio::test(flavor = "multi_thread")]
    async fn test_sync_to_destination_orphan_blob_records_cleared_no_upload() {
        let _lock = ceremony_lock().await;
        let ctx = setup().await;

        let orphan = Uuid::new_v4().to_string();
        ctx.store
            .bulk_insert_pending_backup(std::slice::from_ref(&orphan), DEST_ID)
            .await
            .unwrap();

        let dest = MockCloudTransport::new();
        let primary = MockCloudTransport::new();

        let counts = sync_to_destination(
            DEST_ID,
            std::slice::from_ref(&orphan),
            &HashSet::new(),
            &dest,
            &primary,
            &ctx.store,
            &ctx.vault.vault_db_path,
            &ctx.sqlcipher_key,
            &ctx.manifest_key,
            &ctx.vault.vault_id,
            &ctx.vault.header,
            ctx.staging.path(),
            ctx.mirror_tmp.path(),
            false,
            &noop_progress(),
        )
        .await
        .expect("sync_to_destination must succeed");

        assert_eq!(counts.uploaded, 0, "orphan must not be uploaded");
        assert_eq!(counts.failed, 0, "orphan must not increment failures");

        assert!(
            ctx.store
                .list_pending_backups(DEST_ID)
                .await
                .unwrap()
                .is_empty(),
            "orphan pending record must be cleared"
        );

        let verify = ctx.staging.path().join("verify.bin");
        let result = dest
            .download_blob(&format!("vault/{orphan}.blob"), &verify)
            .await;
        assert!(
            matches!(result, Err(CloudTransportError::NotFound)),
            "orphan must not have been uploaded to dest transport"
        );
    }

    /// Blob in all_blob_names and pending queue but absent from both staging dir and primary
    /// cloud → records cleared (blob is considered resolved), no failure increment.
    #[tokio::test(flavor = "multi_thread")]
    async fn test_sync_to_destination_blob_absent_from_primary_records_cleared_no_failure() {
        let _lock = ceremony_lock().await;
        let ctx = setup().await;

        let blob_name = Uuid::new_v4().to_string();
        let mut all_blobs = HashSet::new();
        all_blobs.insert(blob_name.clone());
        ctx.store
            .bulk_insert_pending_backup(std::slice::from_ref(&blob_name), DEST_ID)
            .await
            .unwrap();

        // primary has no blobs — download returns NotFound
        let dest = MockCloudTransport::new();
        let primary = MockCloudTransport::new();

        let counts = sync_to_destination(
            DEST_ID,
            std::slice::from_ref(&blob_name),
            &all_blobs,
            &dest,
            &primary,
            &ctx.store,
            &ctx.vault.vault_db_path,
            &ctx.sqlcipher_key,
            &ctx.manifest_key,
            &ctx.vault.vault_id,
            &ctx.vault.header,
            ctx.staging.path(),
            ctx.mirror_tmp.path(),
            false,
            &noop_progress(),
        )
        .await
        .expect("sync_to_destination must succeed");

        assert_eq!(counts.uploaded, 0);
        assert_eq!(
            counts.failed, 0,
            "absent-from-primary blob must not increment failures"
        );

        assert!(
            ctx.store
                .list_pending_backups(DEST_ID)
                .await
                .unwrap()
                .is_empty(),
            "pending record must be cleared when blob is absent from primary"
        );
    }

    /// Upload to dest transport fails → record_backup_failure called, counts.failed = 1,
    /// pending record NOT cleared.
    #[tokio::test(flavor = "multi_thread")]
    async fn test_sync_to_destination_upload_failure_records_failure_and_increments_failed() {
        let _lock = ceremony_lock().await;
        let ctx = setup().await;

        let blob_name = Uuid::new_v4().to_string();
        tokio::fs::write(
            ctx.staging.path().join(format!("{blob_name}.blob")),
            b"encrypted blob payload",
        )
        .await
        .unwrap();

        let mut all_blobs = HashSet::new();
        all_blobs.insert(blob_name.clone());
        ctx.store
            .bulk_insert_pending_backup(std::slice::from_ref(&blob_name), DEST_ID)
            .await
            .unwrap();

        let dest = MockCloudTransport::new();
        dest.inject_failure(
            &format!("vault/{blob_name}.blob"),
            CloudTransportErrorKind::Other("simulated upload failure".to_string()),
        )
        .await;
        let primary = MockCloudTransport::new();

        let counts = sync_to_destination(
            DEST_ID,
            std::slice::from_ref(&blob_name),
            &all_blobs,
            &dest,
            &primary,
            &ctx.store,
            &ctx.vault.vault_db_path,
            &ctx.sqlcipher_key,
            &ctx.manifest_key,
            &ctx.vault.vault_id,
            &ctx.vault.header,
            ctx.staging.path(),
            ctx.mirror_tmp.path(),
            false,
            &noop_progress(),
        )
        .await
        .expect("sync_to_destination must succeed despite upload failure");

        assert_eq!(
            counts.uploaded, 0,
            "failed blob must not increment uploaded"
        );
        assert_eq!(
            counts.failed, 1,
            "failed upload must increment failed count"
        );

        let failure_counts = ctx.store.get_backup_failure_counts().await.unwrap();
        assert_eq!(
            failure_counts,
            vec![(DEST_ID.to_string(), 1)],
            "record_backup_failure must have been called exactly once"
        );
    }

    /// Mirror mode: orphan UUID-v4 blob on dest is deleted; legitimate blob is kept;
    /// non-UUID-v4 blob name is skipped (not deleted).
    #[tokio::test(flavor = "multi_thread")]
    async fn test_sync_to_destination_mirror_mode_deletes_orphan_skips_non_uuid_v4() {
        let _lock = ceremony_lock().await;
        let ctx = setup().await;

        let legitimate = Uuid::new_v4().to_string();
        let orphan = Uuid::new_v4().to_string();
        let non_uuid = "not-a-uuid";

        let fake_blob = ctx.staging.path().join("fake.bin");
        tokio::fs::write(&fake_blob, b"x").await.unwrap();

        let dest = MockCloudTransport::new();
        dest.upload_blob(&fake_blob, &format!("vault/{legitimate}.blob"))
            .await
            .unwrap();
        dest.upload_blob(&fake_blob, &format!("vault/{orphan}.blob"))
            .await
            .unwrap();
        dest.upload_blob(&fake_blob, &format!("vault/{non_uuid}.blob"))
            .await
            .unwrap();

        let mut all_blobs = HashSet::new();
        all_blobs.insert(legitimate.clone());

        let primary = MockCloudTransport::new();

        let counts = sync_to_destination(
            DEST_ID,
            &[],
            &all_blobs,
            &dest,
            &primary,
            &ctx.store,
            &ctx.vault.vault_db_path,
            &ctx.sqlcipher_key,
            &ctx.manifest_key,
            &ctx.vault.vault_id,
            &ctx.vault.header,
            ctx.staging.path(),
            ctx.mirror_tmp.path(),
            true,
            &noop_progress(),
        )
        .await
        .expect("sync_to_destination in mirror mode must succeed");

        assert_eq!(
            counts.deleted, 1,
            "exactly one orphan UUID-v4 blob must be deleted"
        );

        let verify = ctx.mirror_tmp.path().join("verify.bin");

        let result = dest
            .download_blob(&format!("vault/{orphan}.blob"), &verify)
            .await;
        assert!(
            matches!(result, Err(CloudTransportError::NotFound)),
            "orphan blob must have been deleted from dest transport"
        );

        dest.download_blob(&format!("vault/{legitimate}.blob"), &verify)
            .await
            .expect("legitimate blob must remain after mirror mode");

        dest.download_blob(&format!("vault/{non_uuid}.blob"), &verify)
            .await
            .expect("non-UUID-v4 blob must be skipped (not deleted) by mirror mode");
    }

    /// Empty pending list with mirror_mode = false → returns all-zero counts, no panic.
    #[tokio::test(flavor = "multi_thread")]
    async fn test_sync_to_destination_empty_pending_returns_zero_counts() {
        let _lock = ceremony_lock().await;
        let ctx = setup().await;

        let dest = MockCloudTransport::new();
        let primary = MockCloudTransport::new();

        let counts = sync_to_destination(
            DEST_ID,
            &[],
            &HashSet::new(),
            &dest,
            &primary,
            &ctx.store,
            &ctx.vault.vault_db_path,
            &ctx.sqlcipher_key,
            &ctx.manifest_key,
            &ctx.vault.vault_id,
            &ctx.vault.header,
            ctx.staging.path(),
            ctx.mirror_tmp.path(),
            false,
            &noop_progress(),
        )
        .await
        .expect("sync_to_destination with empty pending must succeed");

        assert_eq!(counts.uploaded, 0);
        assert_eq!(counts.failed, 0);
        assert_eq!(counts.deleted, 0);
    }
}
