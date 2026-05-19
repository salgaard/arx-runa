use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use zeroize::Zeroizing;

use super::helpers::*;
use super::types::Argon2MigrationIntent;
use super::*;
use crate::auth::Argon2Params;
use crate::auth::MockKeySource;
use crate::auth::kdf::derive_master_key_into;
use crate::auth::session::{SessionKeys, SessionManager};
use crate::crypto::{SqlcipherKey, VaultId};
use crate::storage::cloud::CloudTransport;
use crate::storage::cloud::manifest_backup::encrypt_manifest_backup;
use crate::storage::cloud::mock::MockCloudTransport;
use crate::storage::cloud::vault_header::VaultHeader;
use crate::storage::cloud::{MANIFEST_BACKUP_BLOB_NAME, upload_manifest_backup};

pub(crate) const TEST_PASSWORD: &[u8] = b"correct horse battery staple";
pub(crate) const TEST_NEW_PASSWORD: &[u8] = b"stapler battery horse correct";
pub(crate) const TEST_WRONG_PASSWORD: &[u8] = b"not the password";

static CEREMONY_TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

pub(crate) async fn ceremony_lock() -> tokio::sync::MutexGuard<'static, ()> {
    CEREMONY_TEST_LOCK.lock().await
}

pub(crate) fn test_params() -> Argon2Params {
    Argon2Params {
        memory_cost_kib: 1024,
        time_cost: 1,
        parallelism: 1,
    }
}

pub(crate) fn test_session_manager() -> SessionManager {
    SessionManager::with_timeout(Duration::from_secs(3600))
}

pub(crate) fn temp_dir() -> tempfile::TempDir {
    tempfile::tempdir().expect("tempdir must be created")
}

pub(crate) struct TierOneVault {
    pub(crate) _temp: tempfile::TempDir,
    pub(crate) vault_db_path: PathBuf,
    pub(crate) cloud: MockCloudTransport,
    pub(crate) session: SessionManager,
    pub(crate) vault_id: VaultId,
    pub(crate) header: VaultHeader,
}

pub(crate) async fn create_tier_one_vault() -> TierOneVault {
    let temp = temp_dir();
    let vault_db_path = temp.path().join("vault.db");
    let cloud = MockCloudTransport::new();
    let session = test_session_manager();
    let request = CreateVaultRequest {
        tier: Tier::One,
        password_bytes: TEST_PASSWORD,
        target_key_file_path: None,
        vault_db_path: vault_db_path.clone(),
        argon2_params: Argon2Params::DEFAULT,
        chunk_size_bytes: CreateVaultRequest::DEFAULT_CHUNK_SIZE_BYTES,
        epoch_buffer_enabled: CreateVaultRequest::DEFAULT_EPOCH_BUFFER_ENABLED,
        vault_name: None,
        suggested_vault_id: None,
        primary_destination: None,
    };
    let cloud_arc: Arc<dyn CloudTransport> = Arc::new(cloud.clone());
    let vault_id = create_vault(request, &session, cloud_arc.as_ref())
        .await
        .expect("create_vault tier 1 must succeed");
    let header_download_path = temp.path().join("test-support-tier1-header.json");
    cloud
        .download_blob(VAULT_HEADER_BLOB_NAME, &header_download_path)
        .await
        .expect("header must be present after create_vault");
    let header_bytes = std::fs::read(header_download_path).expect("header must be readable");
    let header: VaultHeader =
        serde_json::from_slice(&header_bytes).expect("header must deserialize");
    TierOneVault {
        _temp: temp,
        vault_db_path,
        cloud,
        session,
        vault_id,
        header,
    }
}

pub(crate) struct TierTwoVault {
    pub(crate) _temp: tempfile::TempDir,
    pub(crate) vault_db_path: PathBuf,
    pub(crate) key_file_path: PathBuf,
    pub(crate) cloud: MockCloudTransport,
    pub(crate) session: SessionManager,
    pub(crate) vault_id: VaultId,
    pub(crate) header: VaultHeader,
}

pub(crate) async fn create_tier_two_vault() -> TierTwoVault {
    let temp = temp_dir();
    let vault_db_path = temp.path().join("vault.db");
    let key_file_path = temp.path().join("key.bin");
    let cloud = MockCloudTransport::new();
    let session = test_session_manager();
    let request = CreateVaultRequest {
        tier: Tier::Two,
        password_bytes: TEST_PASSWORD,
        target_key_file_path: Some(key_file_path.clone()),
        vault_db_path: vault_db_path.clone(),
        argon2_params: Argon2Params::DEFAULT,
        chunk_size_bytes: CreateVaultRequest::DEFAULT_CHUNK_SIZE_BYTES,
        epoch_buffer_enabled: CreateVaultRequest::DEFAULT_EPOCH_BUFFER_ENABLED,
        vault_name: None,
        suggested_vault_id: None,
        primary_destination: None,
    };
    let cloud_arc: Arc<dyn CloudTransport> = Arc::new(cloud.clone());
    let vault_id = create_vault(request, &session, cloud_arc.as_ref())
        .await
        .expect("create_vault tier 2 must succeed");
    let header_download_path = temp.path().join("test-support-tier2-header.json");
    cloud
        .download_blob(VAULT_HEADER_BLOB_NAME, &header_download_path)
        .await
        .expect("header must be present after create_vault");
    let header_bytes = std::fs::read(header_download_path).expect("header must be readable");
    let header: VaultHeader =
        serde_json::from_slice(&header_bytes).expect("header must deserialize");
    TierTwoVault {
        _temp: temp,
        vault_db_path,
        key_file_path,
        cloud,
        session,
        vault_id,
        header,
    }
}

pub(crate) async fn add_recovery_slot_and_return_phrase(
    vault: &mut TierOneVault,
) -> Zeroizing<String> {
    let request = SetupRecoveryRequest {
        current_password_bytes: TEST_PASSWORD,
        current_key_source: None,
        argon2_params: test_params(),
        argon2_migration_intent: Argon2MigrationIntent::PreserveTrusted,
        vault_db_path: vault.vault_db_path.clone(),
    };
    let cloud_arc: Arc<dyn CloudTransport> = Arc::new(vault.cloud.clone());
    setup_recovery(
        request,
        &vault.session,
        cloud_arc.as_ref(),
        &mut vault.header,
        &vault.vault_id,
    )
    .await
    .expect("setup_recovery must succeed")
}

pub(crate) async fn upload_manifest_backup_for(vault: &TierOneVault) {
    let salt = decode_base64_32(&vault.header.argon2_salt).unwrap();
    let params = argon2_params_from_json(&vault.header.argon2_params);
    let mut master: Zeroizing<[u8; 32]> = Zeroizing::new([0u8; 32]);
    derive_master_key_into(TEST_PASSWORD, None, &salt, &params, &mut master).unwrap();
    let keys = SessionKeys::from_master_key_bytes(&master).unwrap();
    let sqlcipher_key = SqlcipherKey::from_slice(keys.sqlcipher_key.expose());
    let manifest_key: [u8; 32] = *keys.manifest_key.expose();
    let staging_root = tempfile::tempdir().expect("manifest backup staging tempdir must exist");
    upload_manifest_backup(
        &vault.vault_db_path,
        &sqlcipher_key,
        &manifest_key,
        &vault.vault_id,
        &vault.cloud,
        staging_root.path(),
    )
    .await
    .expect("manifest backup upload must succeed");
}

pub(crate) async fn upload_manifest_backup_payload_for(vault: &TierOneVault, payload: &[u8]) {
    let salt = decode_base64_32(&vault.header.argon2_salt).unwrap();
    let params = argon2_params_from_json(&vault.header.argon2_params);
    let mut master: Zeroizing<[u8; 32]> = Zeroizing::new([0u8; 32]);
    derive_master_key_into(TEST_PASSWORD, None, &salt, &params, &mut master).unwrap();
    let keys = SessionKeys::from_master_key_bytes(&master).unwrap();
    let manifest_key: [u8; 32] = *keys.manifest_key.expose();
    let wire = encrypt_manifest_backup(
        Zeroizing::new(payload.to_vec()),
        &manifest_key,
        &vault.vault_id,
    )
    .unwrap();
    let staging_root = tempfile::tempdir().expect("payload bypass staging tempdir must exist");
    let upload_path = staging_root
        .path()
        .join("test-support-manifest-backup.blob");
    tokio::fs::write(&upload_path, &wire)
        .await
        .expect("payload bypass write must succeed");
    vault
        .cloud
        .upload_blob(&upload_path, MANIFEST_BACKUP_BLOB_NAME)
        .await
        .unwrap();
    let _ = tokio::fs::remove_file(&upload_path).await;
}

pub(crate) async fn upload_corrupted_manifest_backup_for(vault: &TierOneVault) {
    upload_manifest_backup_for(vault).await;
    let staging_root = tempfile::tempdir().expect("corruption staging tempdir must be created");
    let wire_path = staging_root
        .path()
        .join("test-support-corrupt-manifest-backup.blob");
    vault
        .cloud
        .download_blob(MANIFEST_BACKUP_BLOB_NAME, &wire_path)
        .await
        .expect("manifest backup must exist before corruption");
    let mut wire = tokio::fs::read(&wire_path)
        .await
        .expect("manifest backup wire must be readable");
    let last_index = wire
        .len()
        .checked_sub(1)
        .expect("manifest backup wire must be non-empty");
    wire[last_index] ^= 0x01;
    tokio::fs::write(&wire_path, &wire)
        .await
        .expect("corrupted manifest backup wire must be writable");
    vault
        .cloud
        .upload_blob(&wire_path, MANIFEST_BACKUP_BLOB_NAME)
        .await
        .expect("corrupted manifest backup upload must succeed");
    let _ = tokio::fs::remove_file(&wire_path).await;
}

/// Raw vault key bytes derived from master key — accessible to tests in `crate::tests`.
pub(crate) struct DerivedVaultKeys {
    /// Bytes suitable for `SqlCipherMetadataStore::open/create`.
    pub(crate) sqlcipher_key: [u8; 32],
    /// Bytes for constructing `ManifestKey::from_bytes`.
    pub(crate) manifest_key: [u8; 32],
    /// Bytes for constructing `KeyEncryptionKey::from_bytes`.
    pub(crate) key_encryption_key: [u8; 32],
}

/// Derives all vault keys for a Tier 1 vault using the test password.
pub(crate) fn derive_vault_keys_tier_one(vault: &TierOneVault) -> DerivedVaultKeys {
    let salt = decode_base64_32(&vault.header.argon2_salt).unwrap();
    let params = argon2_params_from_json(&vault.header.argon2_params);
    let mut master: Zeroizing<[u8; 32]> = Zeroizing::new([0u8; 32]);
    derive_master_key_into(TEST_PASSWORD, None, &salt, &params, &mut master).unwrap();
    let keys = SessionKeys::from_master_key_bytes(&master).unwrap();
    DerivedVaultKeys {
        sqlcipher_key: *keys.sqlcipher_key.expose(),
        manifest_key: *keys.manifest_key.expose(),
        key_encryption_key: *keys.key_encryption_key.expose(),
    }
}

/// Adds a recovery phrase slot to a Tier 2 vault using the test password and key file.
pub(crate) async fn add_recovery_slot_and_return_phrase_tier_two(
    vault: &mut TierTwoVault,
) -> Zeroizing<String> {
    let key_bytes: [u8; 32] = std::fs::read(&vault.key_file_path)
        .expect("key file must be readable")
        .try_into()
        .expect("key file must be 32 bytes");
    let key_source = MockKeySource::new(key_bytes);
    let request = SetupRecoveryRequest {
        current_password_bytes: TEST_PASSWORD,
        current_key_source: Some(&key_source),
        argon2_params: test_params(),
        argon2_migration_intent: Argon2MigrationIntent::PreserveTrusted,
        vault_db_path: vault.vault_db_path.clone(),
    };
    let cloud_arc: Arc<dyn CloudTransport> = Arc::new(vault.cloud.clone());
    setup_recovery(
        request,
        &vault.session,
        cloud_arc.as_ref(),
        &mut vault.header,
        &vault.vault_id,
    )
    .await
    .expect("setup_recovery tier 2 must succeed")
}

/// Uploads a manifest backup for a Tier 2 vault using the test password and key file.
pub(crate) async fn upload_manifest_backup_for_tier_two(vault: &TierTwoVault) {
    let key_bytes = std::fs::read(&vault.key_file_path).expect("key file must be readable");
    let key_bytes_32: &[u8; 32] = key_bytes
        .as_slice()
        .try_into()
        .expect("key file must be 32 bytes");
    let salt = decode_base64_32(&vault.header.argon2_salt).unwrap();
    let params = argon2_params_from_json(&vault.header.argon2_params);
    let mut master: Zeroizing<[u8; 32]> = Zeroizing::new([0u8; 32]);
    derive_master_key_into(
        TEST_PASSWORD,
        Some(key_bytes_32),
        &salt,
        &params,
        &mut master,
    )
    .unwrap();
    let keys = SessionKeys::from_master_key_bytes(&master).unwrap();
    let sqlcipher_key = SqlcipherKey::from_slice(keys.sqlcipher_key.expose());
    let manifest_key: [u8; 32] = *keys.manifest_key.expose();
    let staging_root =
        tempfile::tempdir().expect("tier 2 manifest backup staging tempdir must exist");
    upload_manifest_backup(
        &vault.vault_db_path,
        &sqlcipher_key,
        &manifest_key,
        &vault.vault_id,
        &vault.cloud,
        staging_root.path(),
    )
    .await
    .expect("tier 2 manifest backup upload must succeed");
}
