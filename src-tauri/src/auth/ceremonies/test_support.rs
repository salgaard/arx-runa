use std::path::PathBuf;
use std::time::Duration;

use zeroize::Zeroizing;

use super::helpers::*;
use super::*;
use crate::auth::Argon2Params;
use crate::auth::kdf::derive_master_key_into;
use crate::auth::session::{SessionKeys, SessionManager};
use crate::crypto::VaultId;
use crate::storage::cloud::CloudTransport;
use crate::storage::cloud::manifest_backup::encrypt_manifest_backup;
use crate::storage::cloud::mock::MockCloudTransport;
use crate::storage::cloud::vault_header::VaultHeader;

pub(super) const TEST_PASSWORD: &[u8] = b"correct horse battery staple";
pub(super) const TEST_NEW_PASSWORD: &[u8] = b"stapler battery horse correct";
pub(super) const TEST_WRONG_PASSWORD: &[u8] = b"not the password";

static CEREMONY_TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

pub(super) async fn ceremony_lock() -> tokio::sync::MutexGuard<'static, ()> {
    CEREMONY_TEST_LOCK.lock().await
}

pub(super) fn test_params() -> Argon2Params {
    Argon2Params {
        memory_cost_kib: 1024,
        time_cost: 1,
        parallelism: 1,
    }
}

pub(super) fn test_session_manager() -> SessionManager {
    SessionManager::with_timeout(Duration::from_secs(3600))
}

pub(super) fn temp_dir() -> tempfile::TempDir {
    tempfile::tempdir().expect("tempdir must be created")
}

pub(super) struct TierOneVault {
    pub(super) _temp: tempfile::TempDir,
    pub(super) vault_db_path: PathBuf,
    pub(super) cloud: MockCloudTransport,
    pub(super) session: SessionManager,
    pub(super) vault_id: VaultId,
    pub(super) header: VaultHeader,
}

pub(super) async fn create_tier_one_vault() -> TierOneVault {
    let temp = temp_dir();
    let vault_db_path = temp.path().join("vault.db");
    let cloud = MockCloudTransport::new();
    let session = test_session_manager();
    let request = CreateVaultRequest {
        tier: Tier::One,
        password_bytes: TEST_PASSWORD,
        target_key_file_path: None,
        vault_db_path: vault_db_path.clone(),
        argon2_params: test_params(),
    };
    let vault_id = create_vault(request, &session, &cloud)
        .await
        .expect("create_vault tier 1 must succeed");
    let header_bytes = cloud
        .download_blob(&vault_header_blob_name())
        .await
        .expect("header must be present after create_vault");
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

pub(super) struct TierTwoVault {
    pub(super) _temp: tempfile::TempDir,
    pub(super) vault_db_path: PathBuf,
    pub(super) key_file_path: PathBuf,
    pub(super) cloud: MockCloudTransport,
    pub(super) session: SessionManager,
    pub(super) vault_id: VaultId,
    pub(super) header: VaultHeader,
}

pub(super) async fn create_tier_two_vault() -> TierTwoVault {
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
        argon2_params: test_params(),
    };
    let vault_id = create_vault(request, &session, &cloud)
        .await
        .expect("create_vault tier 2 must succeed");
    let header_bytes = cloud
        .download_blob(&vault_header_blob_name())
        .await
        .expect("header must be present after create_vault");
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

pub(super) async fn add_recovery_slot_and_return_phrase(
    vault: &mut TierOneVault,
) -> Zeroizing<String> {
    let request = SetupRecoveryRequest {
        current_password_bytes: TEST_PASSWORD,
        current_key_source: None,
        argon2_params: test_params(),
        vault_db_path: vault.vault_db_path.clone(),
    };
    setup_recovery(
        request,
        &vault.session,
        &vault.cloud,
        &mut vault.header,
        &vault.vault_id,
    )
    .await
    .expect("setup_recovery must succeed")
}

pub(super) async fn upload_manifest_backup_for(vault: &TierOneVault) {
    upload_manifest_backup_payload_for(
        vault,
        b"CREATE TABLE IF NOT EXISTS imported_stub (id INTEGER);",
    )
    .await;
}

pub(super) async fn upload_manifest_backup_payload_for(vault: &TierOneVault, payload: &[u8]) {
    let salt = decode_base64_32(&vault.header.argon2_salt).unwrap();
    let params = argon2_params_from_json(&vault.header.argon2_params);
    let mut master: Zeroizing<[u8; 32]> = Zeroizing::new([0u8; 32]);
    derive_master_key_into(TEST_PASSWORD, None, &salt, &params, &mut master).unwrap();
    let keys = SessionKeys::from_master_key_bytes(&master).unwrap();
    let manifest_key: [u8; 32] = *keys.manifest_key.expose();
    let wire = encrypt_manifest_backup(payload.to_vec(), &manifest_key).unwrap();
    vault
        .cloud
        .upload_blob(&manifest_backup_blob_name(), &wire)
        .await
        .unwrap();
}
