//! Scenario tests: real cloud transport (P6 — Backblaze B2).
//!
//! Skipped unless `ARX_TEST_B2_KEY_ID`, `ARX_TEST_B2_APP_KEY`, and
//! `ARX_TEST_B2_BUCKET` are set (sourced from `.env.test` by the caller or CI
//! secrets).  Each test run uses a unique path prefix inside the bucket so
//! concurrent runs never collide, and blobs are deleted before assertions so
//! the bucket stays clean even when a test fails.

use std::path::{Path, PathBuf};

use uuid::Uuid;

use crate::auth::ceremonies::test_support::*;
use crate::crypto::{KeyEncryptionKey, ManifestKey, SqlcipherKey, WrappedFileKey, unwrap_file_key};
use crate::storage::cloud::{CloudTransport, MANIFEST_BACKUP_BLOB_NAME};
use crate::storage::{
    BackupSyncMode, CloudEndpoint, DestinationSessionPublic, DestinationType, MetadataStore,
    RcloneTransport, SqlCipherMetadataStore, SyncConfig, decrypt_file, push_vault, upload_file,
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

struct B2TestConfig {
    key_id: String,
    app_key: String,
    bucket: String,
}

fn strip_whitespace(s: String) -> String {
    s.split_whitespace().collect()
}

fn b2_test_config() -> Option<B2TestConfig> {
    Some(B2TestConfig {
        key_id: strip_whitespace(std::env::var("ARX_TEST_B2_KEY_ID").ok()?),
        app_key: strip_whitespace(std::env::var("ARX_TEST_B2_APP_KEY").ok()?),
        bucket: std::env::var("ARX_TEST_B2_BUCKET").ok()?,
    })
}

/// Returns the bundled rclone sidecar binary path for the current target.
fn bundled_rclone() -> PathBuf {
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("bin");
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    return base.join("rclone-x86_64-pc-windows-msvc.exe");
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    return base.join("rclone-aarch64-apple-darwin");
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    return base.join("rclone-x86_64-apple-darwin");
    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    return base.join("rclone-aarch64-unknown-linux-gnu");
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    return base.join("rclone-x86_64-unknown-linux-gnu");
    #[allow(unreachable_code)]
    base.join("rclone")
}

fn b2_rclone_config_content(key_id: &str, app_key: &str) -> String {
    format!("[arxb2test]\ntype = b2\naccount = {key_id}\nkey = {app_key}\n")
}

fn build_b2_transport(
    config_path: &Path,
    bucket: &str,
    path_prefix: &str,
    key_id: &str,
    app_key: &str,
) -> RcloneTransport {
    std::fs::write(config_path, b2_rclone_config_content(key_id, app_key))
        .expect("rclone config must be writable");

    let destination = DestinationSessionPublic {
        destination_id: Uuid::new_v4().hyphenated().to_string(),
        label: "b2-scenario-test".to_owned(),
        destination_type: DestinationType::Cloud,
        rclone_remote_name: "arxb2test".to_owned(),
        bucket: bucket.to_owned(),
        path_prefix: path_prefix.to_owned(),
        is_primary: true,
        backup_mode: Some(BackupSyncMode::Mirror),
    };

    let endpoint = CloudEndpoint {
        provider: "b2".to_owned(),
        bucket: bucket.to_owned(),
        region: String::new(),
        endpoint: String::new(),
        path_prefix: path_prefix.to_owned(),
    };

    RcloneTransport::new(
        bundled_rclone(),
        config_path.to_path_buf(),
        &endpoint,
        &destination,
        SyncConfig::default(),
    )
    .expect("B2 RcloneTransport must be created from valid config")
}

async fn cleanup_transport(transport: &RcloneTransport) {
    let blobs = transport.list_blobs("").await.unwrap_or_default();
    for blob in blobs {
        let _ = transport.delete_blob(&blob).await;
    }
}

// ---------------------------------------------------------------------------
// Google Drive helpers
// ---------------------------------------------------------------------------

struct GdriveTestConfig {
    refresh_token: String,
}

fn gdrive_test_config() -> Option<GdriveTestConfig> {
    Some(GdriveTestConfig {
        refresh_token: strip_whitespace(std::env::var("ARX_TEST_GDRIVE_REFRESH_TOKEN").ok()?),
    })
}

fn gdrive_rclone_config_content(refresh_token: &str) -> String {
    // Non-empty placeholder access_token with past expiry: rclone sees "expired token with
    // refresh_token" and uses the refresh path.  An empty access_token causes rclone to treat
    // the token as absent entirely, skipping the refresh and returning "no refresh token".
    let token_json = format!(
        r#"{{"access_token":"x","token_type":"Bearer","refresh_token":"{refresh_token}","expiry":"2000-01-01T00:00:00Z"}}"#
    );
    format!("[arxdrivetest]\ntype = drive\nscope = drive\ntoken = {token_json}\n")
}

fn build_gdrive_transport(
    config_path: &Path,
    path_prefix: &str,
    refresh_token: &str,
) -> RcloneTransport {
    std::fs::write(config_path, gdrive_rclone_config_content(refresh_token))
        .expect("rclone Drive config must be writable");

    // Google Drive has no bucket concept: empty bucket, path_prefix is the Drive folder.
    let destination = DestinationSessionPublic {
        destination_id: Uuid::new_v4().hyphenated().to_string(),
        label: "drive-scenario-test".to_owned(),
        destination_type: DestinationType::Cloud,
        rclone_remote_name: "arxdrivetest".to_owned(),
        bucket: String::new(),
        path_prefix: path_prefix.to_owned(),
        is_primary: true,
        backup_mode: Some(BackupSyncMode::Mirror),
    };

    let endpoint = CloudEndpoint {
        provider: "drive".to_owned(),
        bucket: String::new(),
        region: String::new(),
        endpoint: String::new(),
        path_prefix: path_prefix.to_owned(),
    };

    RcloneTransport::new(
        bundled_rclone(),
        config_path.to_path_buf(),
        &endpoint,
        &destination,
        SyncConfig::default(),
    )
    .expect("Drive RcloneTransport must be created from valid config")
}

// ---------------------------------------------------------------------------
// OneDrive helpers
// ---------------------------------------------------------------------------

struct OneDriveTestConfig {
    refresh_token: String,
    drive_id: String,
}

fn onedrive_test_config() -> Option<OneDriveTestConfig> {
    Some(OneDriveTestConfig {
        refresh_token: strip_whitespace(std::env::var("ARX_TEST_ONEDRIVE_REFRESH_TOKEN").ok()?),
        drive_id: strip_whitespace(std::env::var("ARX_TEST_ONEDRIVE_DRIVE_ID").ok()?),
    })
}

fn onedrive_rclone_config_content(refresh_token: &str, drive_id: &str) -> String {
    let token_json = format!(
        r#"{{"access_token":"x","token_type":"Bearer","refresh_token":"{refresh_token}","expiry":"2000-01-01T00:00:00Z"}}"#
    );
    format!(
        "[arxonedrivetest]\ntype = onedrive\ntoken = {token_json}\ndrive_id = {drive_id}\ndrive_type = personal\n"
    )
}

fn build_onedrive_transport(
    config_path: &Path,
    path_prefix: &str,
    refresh_token: &str,
    drive_id: &str,
) -> RcloneTransport {
    std::fs::write(
        config_path,
        onedrive_rclone_config_content(refresh_token, drive_id),
    )
    .expect("rclone OneDrive config must be writable");

    let destination = DestinationSessionPublic {
        destination_id: Uuid::new_v4().hyphenated().to_string(),
        label: "onedrive-scenario-test".to_owned(),
        destination_type: DestinationType::Cloud,
        rclone_remote_name: "arxonedrivetest".to_owned(),
        bucket: String::new(),
        path_prefix: path_prefix.to_owned(),
        is_primary: true,
        backup_mode: Some(BackupSyncMode::Mirror),
    };

    let endpoint = CloudEndpoint {
        provider: "onedrive".to_owned(),
        bucket: String::new(),
        region: String::new(),
        endpoint: String::new(),
        path_prefix: path_prefix.to_owned(),
    };

    RcloneTransport::new(
        bundled_rclone(),
        config_path.to_path_buf(),
        &endpoint,
        &destination,
        SyncConfig::default(),
    )
    .expect("OneDrive RcloneTransport must be created from valid config")
}

// ---------------------------------------------------------------------------
// P6/B2: push_vault → manifest blob present in cloud
// ---------------------------------------------------------------------------

/// Pushes an empty vault to a real B2 bucket and verifies the manifest backup
/// blob is present, confirming end-to-end rclone ↔ B2 connectivity.
#[tokio::test(flavor = "multi_thread")]
async fn test_b2_push_vault_manifest_blob_present_after_sync() {
    let Some(creds) = b2_test_config() else {
        return;
    };
    let _lock = ceremony_lock().await;
    let vault = create_tier_one_vault().await;
    let derived = derive_vault_keys_tier_one(&vault);
    let sqlcipher_key = SqlcipherKey::from_bytes(derived.sqlcipher_key);
    let manifest_key = ManifestKey::from_bytes(derived.manifest_key);

    let store = SqlCipherMetadataStore::open(&vault.vault_db_path, &derived.sqlcipher_key)
        .await
        .expect("store must open");
    let staging = temp_dir();

    let path_prefix = format!("ci-{}", Uuid::new_v4().simple());
    let config_temp = temp_dir();
    let config_path = config_temp.path().join("rclone.conf");
    let transport = build_b2_transport(
        &config_path,
        &creds.bucket,
        &path_prefix,
        &creds.key_id,
        &creds.app_key,
    );

    push_vault(
        &vault.vault_db_path,
        &sqlcipher_key,
        &manifest_key,
        &store,
        &transport,
        &vault.header,
        staging.path(),
        &SyncConfig::default(),
        None,
    )
    .await
    .expect("push_vault to B2 must succeed");

    let blobs = transport
        .list_blobs("")
        .await
        .expect("list_blobs must succeed after push");

    cleanup_transport(&transport).await;

    assert!(
        blobs.contains(&MANIFEST_BACKUP_BLOB_NAME.to_owned()),
        "manifest backup blob must be present in B2 after push; listed blobs: {blobs:?}"
    );
}

// ---------------------------------------------------------------------------
// P6/B2: full round trip — bytes survive upload to cloud and download back
// ---------------------------------------------------------------------------

/// Uploads a file, pushes to real B2, downloads the encrypted chunk back, and
/// decrypts — asserting the recovered bytes are identical to the original.
#[tokio::test(flavor = "multi_thread")]
async fn test_b2_backup_round_trip_bytes_survive_upload_and_download() {
    let Some(creds) = b2_test_config() else {
        return;
    };
    let _lock = ceremony_lock().await;
    let vault = create_tier_one_vault().await;
    let derived = derive_vault_keys_tier_one(&vault);
    let sqlcipher_key = SqlcipherKey::from_bytes(derived.sqlcipher_key);
    let manifest_key = ManifestKey::from_bytes(derived.manifest_key);
    let kek = KeyEncryptionKey::from_bytes(derived.key_encryption_key);

    let store = SqlCipherMetadataStore::open(&vault.vault_db_path, &derived.sqlcipher_key)
        .await
        .expect("store must open");
    let staging = temp_dir();
    let staging_pending = staging.path().join("pending");
    tokio::fs::create_dir_all(&staging_pending)
        .await
        .expect("staging/pending must be created");

    let path_prefix = format!("ci-{}", Uuid::new_v4().simple());
    let config_temp = temp_dir();
    let config_path = config_temp.path().join("rclone.conf");
    let transport = build_b2_transport(
        &config_path,
        &creds.bucket,
        &path_prefix,
        &creds.key_id,
        &creds.app_key,
    );

    let source_bytes: &[u8] = b"b2 real cloud round trip test payload";
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
        &staging_pending,
        None,
    )
    .await
    .expect("upload_file must succeed");

    push_vault(
        &vault.vault_db_path,
        &sqlcipher_key,
        &manifest_key,
        &store,
        &transport,
        &vault.header,
        staging.path(),
        &SyncConfig::default(),
        None,
    )
    .await
    .expect("push_vault to B2 must succeed");

    let chunks = store
        .get_chunks(node_id)
        .await
        .expect("chunks must be queryable after upload");

    // Download chunk blobs from B2 into a local staging dir for decryption.
    let dl_staging = temp_dir();
    let dl_pending = dl_staging.path().join("pending");
    tokio::fs::create_dir_all(&dl_pending)
        .await
        .expect("download pending dir must be created");
    for chunk in &chunks {
        let local = dl_pending.join(format!("{}.blob", chunk.blob_name));
        transport
            .download_blob(&format!("vault/{}.blob", chunk.blob_name), &local)
            .await
            .expect("download_blob from B2 must succeed");
    }

    let wrapped_bytes = node
        .file_key_wrapped
        .expect("uploaded node must carry a wrapped file key");
    let file_key = unwrap_file_key(&WrappedFileKey::new(wrapped_bytes), &kek)
        .expect("file key must unwrap with the same KEK used at upload");

    let dest_temp = temp_dir();
    let dest_path = dest_temp.path().join("output.bin");
    decrypt_file(
        &dest_path,
        node_id,
        &file_key,
        source_bytes.len() as u64,
        &chunks,
        dl_staging.path(),
        &store,
        None,
    )
    .await
    .expect("decrypt_file must succeed");

    let result = tokio::fs::read(&dest_path)
        .await
        .expect("decrypted output must be readable");

    cleanup_transport(&transport).await;

    assert_eq!(
        result, source_bytes,
        "decrypted bytes must be identical to the original after B2 round trip"
    );
}

// ---------------------------------------------------------------------------
// P6/Drive: push_vault → manifest blob present in Drive
// ---------------------------------------------------------------------------

/// Pushes an empty vault to a real Google Drive folder and verifies the
/// manifest backup blob is present, confirming rclone ↔ Drive OAuth works.
#[tokio::test(flavor = "multi_thread")]
async fn test_gdrive_push_vault_manifest_blob_present_after_sync() {
    let Some(creds) = gdrive_test_config() else {
        return;
    };
    let _lock = ceremony_lock().await;
    let vault = create_tier_one_vault().await;
    let derived = derive_vault_keys_tier_one(&vault);
    let sqlcipher_key = SqlcipherKey::from_bytes(derived.sqlcipher_key);
    let manifest_key = ManifestKey::from_bytes(derived.manifest_key);

    let store = SqlCipherMetadataStore::open(&vault.vault_db_path, &derived.sqlcipher_key)
        .await
        .expect("store must open");
    let staging = temp_dir();

    let path_prefix = format!("arx-runa-test/ci-{}", Uuid::new_v4().simple());
    let config_temp = temp_dir();
    let config_path = config_temp.path().join("rclone.conf");
    let transport = build_gdrive_transport(&config_path, &path_prefix, &creds.refresh_token);

    push_vault(
        &vault.vault_db_path,
        &sqlcipher_key,
        &manifest_key,
        &store,
        &transport,
        &vault.header,
        staging.path(),
        &SyncConfig::default(),
        None,
    )
    .await
    .expect("push_vault to Google Drive must succeed");

    let blobs = transport
        .list_blobs("")
        .await
        .expect("list_blobs must succeed after Drive push");

    cleanup_transport(&transport).await;

    assert!(
        blobs.contains(&MANIFEST_BACKUP_BLOB_NAME.to_owned()),
        "manifest backup blob must be present in Drive after push; listed blobs: {blobs:?}"
    );
}

// ---------------------------------------------------------------------------
// P6/Drive: full round trip — bytes survive upload to Drive and download back
// ---------------------------------------------------------------------------

/// Uploads a file, pushes to real Google Drive, downloads the encrypted chunk
/// back, and decrypts — asserting the recovered bytes are identical.
#[tokio::test(flavor = "multi_thread")]
async fn test_gdrive_backup_round_trip_bytes_survive_upload_and_download() {
    let Some(creds) = gdrive_test_config() else {
        return;
    };
    let _lock = ceremony_lock().await;
    let vault = create_tier_one_vault().await;
    let derived = derive_vault_keys_tier_one(&vault);
    let sqlcipher_key = SqlcipherKey::from_bytes(derived.sqlcipher_key);
    let manifest_key = ManifestKey::from_bytes(derived.manifest_key);
    let kek = KeyEncryptionKey::from_bytes(derived.key_encryption_key);

    let store = SqlCipherMetadataStore::open(&vault.vault_db_path, &derived.sqlcipher_key)
        .await
        .expect("store must open");
    let staging = temp_dir();
    let staging_pending = staging.path().join("pending");
    tokio::fs::create_dir_all(&staging_pending)
        .await
        .expect("staging/pending must be created");

    let path_prefix = format!("arx-runa-test/ci-{}", Uuid::new_v4().simple());
    let config_temp = temp_dir();
    let config_path = config_temp.path().join("rclone.conf");
    let transport = build_gdrive_transport(&config_path, &path_prefix, &creds.refresh_token);

    let source_bytes: &[u8] = b"gdrive real cloud round trip test payload";
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
        &staging_pending,
        None,
    )
    .await
    .expect("upload_file must succeed");

    push_vault(
        &vault.vault_db_path,
        &sqlcipher_key,
        &manifest_key,
        &store,
        &transport,
        &vault.header,
        staging.path(),
        &SyncConfig::default(),
        None,
    )
    .await
    .expect("push_vault to Google Drive must succeed");

    let chunks = store
        .get_chunks(node_id)
        .await
        .expect("chunks must be queryable after upload");

    let dl_staging = temp_dir();
    let dl_pending = dl_staging.path().join("pending");
    tokio::fs::create_dir_all(&dl_pending)
        .await
        .expect("download pending dir must be created");
    for chunk in &chunks {
        let local = dl_pending.join(format!("{}.blob", chunk.blob_name));
        transport
            .download_blob(&format!("vault/{}.blob", chunk.blob_name), &local)
            .await
            .expect("download_blob from Drive must succeed");
    }

    let wrapped_bytes = node
        .file_key_wrapped
        .expect("uploaded node must carry a wrapped file key");
    let file_key = unwrap_file_key(&WrappedFileKey::new(wrapped_bytes), &kek)
        .expect("file key must unwrap with the same KEK used at upload");

    let dest_temp = temp_dir();
    let dest_path = dest_temp.path().join("output.bin");
    decrypt_file(
        &dest_path,
        node_id,
        &file_key,
        source_bytes.len() as u64,
        &chunks,
        dl_staging.path(),
        &store,
        None,
    )
    .await
    .expect("decrypt_file must succeed");

    let result = tokio::fs::read(&dest_path)
        .await
        .expect("decrypted output must be readable");

    cleanup_transport(&transport).await;

    assert_eq!(
        result, source_bytes,
        "decrypted bytes must be identical to the original after Drive round trip"
    );
}

// ---------------------------------------------------------------------------
// P6/OneDrive: push_vault → manifest blob present in OneDrive
// ---------------------------------------------------------------------------

/// Pushes an empty vault to a real OneDrive folder and verifies the manifest
/// backup blob is present, confirming rclone ↔ OneDrive OAuth works.
#[tokio::test(flavor = "multi_thread")]
async fn test_onedrive_push_vault_manifest_blob_present_after_sync() {
    let Some(creds) = onedrive_test_config() else {
        return;
    };
    let _lock = ceremony_lock().await;
    let vault = create_tier_one_vault().await;
    let derived = derive_vault_keys_tier_one(&vault);
    let sqlcipher_key = SqlcipherKey::from_bytes(derived.sqlcipher_key);
    let manifest_key = ManifestKey::from_bytes(derived.manifest_key);

    let store = SqlCipherMetadataStore::open(&vault.vault_db_path, &derived.sqlcipher_key)
        .await
        .expect("store must open");
    let staging = temp_dir();

    let path_prefix = format!("arx-runa-test/ci-{}", Uuid::new_v4().simple());
    let config_temp = temp_dir();
    let config_path = config_temp.path().join("rclone.conf");
    let transport = build_onedrive_transport(
        &config_path,
        &path_prefix,
        &creds.refresh_token,
        &creds.drive_id,
    );

    push_vault(
        &vault.vault_db_path,
        &sqlcipher_key,
        &manifest_key,
        &store,
        &transport,
        &vault.header,
        staging.path(),
        &SyncConfig::default(),
        None,
    )
    .await
    .expect("push_vault to OneDrive must succeed");

    let blobs = transport
        .list_blobs("")
        .await
        .expect("list_blobs must succeed after OneDrive push");

    cleanup_transport(&transport).await;

    assert!(
        blobs.contains(&MANIFEST_BACKUP_BLOB_NAME.to_owned()),
        "manifest backup blob must be present in OneDrive after push; listed blobs: {blobs:?}"
    );
}

// ---------------------------------------------------------------------------
// P6/OneDrive: full round trip — bytes survive upload to OneDrive and back
// ---------------------------------------------------------------------------

/// Uploads a file, pushes to real OneDrive, downloads the encrypted chunk
/// back, and decrypts — asserting the recovered bytes are identical.
#[tokio::test(flavor = "multi_thread")]
async fn test_onedrive_backup_round_trip_bytes_survive_upload_and_download() {
    let Some(creds) = onedrive_test_config() else {
        return;
    };
    let _lock = ceremony_lock().await;
    let vault = create_tier_one_vault().await;
    let derived = derive_vault_keys_tier_one(&vault);
    let sqlcipher_key = SqlcipherKey::from_bytes(derived.sqlcipher_key);
    let manifest_key = ManifestKey::from_bytes(derived.manifest_key);
    let kek = KeyEncryptionKey::from_bytes(derived.key_encryption_key);

    let store = SqlCipherMetadataStore::open(&vault.vault_db_path, &derived.sqlcipher_key)
        .await
        .expect("store must open");
    let staging = temp_dir();
    let staging_pending = staging.path().join("pending");
    tokio::fs::create_dir_all(&staging_pending)
        .await
        .expect("staging/pending must be created");

    let path_prefix = format!("arx-runa-test/ci-{}", Uuid::new_v4().simple());
    let config_temp = temp_dir();
    let config_path = config_temp.path().join("rclone.conf");
    let transport = build_onedrive_transport(
        &config_path,
        &path_prefix,
        &creds.refresh_token,
        &creds.drive_id,
    );

    let source_bytes: &[u8] = b"onedrive real cloud round trip test payload";
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
        &staging_pending,
        None,
    )
    .await
    .expect("upload_file must succeed");

    push_vault(
        &vault.vault_db_path,
        &sqlcipher_key,
        &manifest_key,
        &store,
        &transport,
        &vault.header,
        staging.path(),
        &SyncConfig::default(),
        None,
    )
    .await
    .expect("push_vault to OneDrive must succeed");

    let chunks = store
        .get_chunks(node_id)
        .await
        .expect("chunks must be queryable after upload");

    let dl_staging = temp_dir();
    let dl_pending = dl_staging.path().join("pending");
    tokio::fs::create_dir_all(&dl_pending)
        .await
        .expect("download pending dir must be created");
    for chunk in &chunks {
        let local = dl_pending.join(format!("{}.blob", chunk.blob_name));
        transport
            .download_blob(&format!("vault/{}.blob", chunk.blob_name), &local)
            .await
            .expect("download_blob from OneDrive must succeed");
    }

    let wrapped_bytes = node
        .file_key_wrapped
        .expect("uploaded node must carry a wrapped file key");
    let file_key = unwrap_file_key(&WrappedFileKey::new(wrapped_bytes), &kek)
        .expect("file key must unwrap with the same KEK used at upload");

    let dest_temp = temp_dir();
    let dest_path = dest_temp.path().join("output.bin");
    decrypt_file(
        &dest_path,
        node_id,
        &file_key,
        source_bytes.len() as u64,
        &chunks,
        dl_staging.path(),
        &store,
        None,
    )
    .await
    .expect("decrypt_file must succeed");

    let result = tokio::fs::read(&dest_path)
        .await
        .expect("decrypted output must be readable");

    cleanup_transport(&transport).await;

    assert_eq!(
        result, source_bytes,
        "decrypted bytes must be identical to the original after OneDrive round trip"
    );
}
