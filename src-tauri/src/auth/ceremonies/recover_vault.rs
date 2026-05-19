use uuid::Uuid;
use zeroize::Zeroizing;

use super::VAULT_HEADER_BLOB_NAME;
use super::helpers::*;
use super::types::RecoverVaultRequest;
use crate::auth::TransportProvider;
use crate::auth::error::AuthenticationError;
use crate::auth::kdf::derive_master_key_into;
use crate::auth::session::{SessionKeys, SessionManager};
use crate::auth::staging;
use crate::crypto::{SqlcipherKey, VaultId};
use crate::storage;
use crate::storage::cloud::vault_header::{VaultHeader, VaultHeaderTrustPolicy};
use crate::storage::cloud::{ManifestBackupSyncError, download_manifest_backup};

/// Recovers a vault onto a new device by downloading its header and
/// manifest backup, re-deriving the session keys, and importing the
/// backup into a fresh local SQLCipher DB.
pub async fn recover_vault(
    request: RecoverVaultRequest<'_>,
    session_manager: &SessionManager,
    cloud_transport: &dyn TransportProvider,
) -> Result<VaultId, AuthenticationError> {
    let install_reservation = session_manager.reserve_session_install().await?;
    precheck_recovery_destination(&request.vault_db_path).await?;

    let staging_dir = staging::staging_directory().await?;
    let header_download_path = staging_dir.join("recover-vault-header.json");

    let _ = staging::remove_if_exists(&header_download_path).await;
    staging::write_owner_only(&header_download_path, b"").await?;
    if cloud_transport
        .download_blob(VAULT_HEADER_BLOB_NAME, &header_download_path)
        .await
        .is_err()
    {
        let _ = staging::remove_if_exists(&header_download_path).await;
        return Err(AuthenticationError::VaultHeaderInvalid);
    }
    let header_bytes = match tokio::fs::read(&header_download_path).await {
        Ok(bytes) => bytes,
        Err(_) => {
            let _ = staging::remove_if_exists(&header_download_path).await;
            return Err(AuthenticationError::VaultHeaderInvalid);
        }
    };
    let _ = staging::remove_if_exists(&header_download_path).await;
    let header: VaultHeader = serde_json::from_slice(&header_bytes)
        .map_err(|_| AuthenticationError::VaultHeaderInvalid)?;
    header
        .validate_structure()
        .map_err(|_| AuthenticationError::VaultHeaderInvalid)?;
    header
        .validate_trust_policy(VaultHeaderTrustPolicy::Bootstrap)
        .map_err(|_| AuthenticationError::VaultHeaderInvalid)?;

    let vault_uuid =
        Uuid::parse_str(&header.vault_id).map_err(|_| AuthenticationError::VaultHeaderInvalid)?;
    let vault_id = VaultId::from_uuid(vault_uuid);
    let salt = decode_base64_32(&header.argon2_salt)?;
    let parameters = argon2_parameters_from_json(&header.argon2_params);

    let key_file_bytes: Option<Zeroizing<[u8; 32]>> = match (header.tier, request.key_source) {
        (1, _) => None,
        (2, Some(source)) => {
            let bytes = source
                .read_key()
                .map_err(|_| AuthenticationError::KeyFileNotFound)?;
            let mut buffer: Zeroizing<[u8; 32]> = Zeroizing::new([0u8; 32]);
            buffer.copy_from_slice(bytes.as_slice());
            Some(buffer)
        }
        (2, None) => return Err(AuthenticationError::KeyFileNotFound),
        _ => return Err(AuthenticationError::VaultHeaderInvalid),
    };

    let mut master_key: Zeroizing<[u8; 32]> = Zeroizing::new([0u8; 32]);
    derive_master_key_into(
        request.password_bytes,
        key_file_bytes.as_deref(),
        &salt,
        &parameters,
        &mut master_key,
    )?;
    let session_keys = SessionKeys::from_master_key_bytes(&master_key)?;
    drop(master_key);
    let manifest_key_bytes = Zeroizing::new(*session_keys.manifest_key.expose());
    let sqlcipher_key = {
        use secrecy::SecretBox;
        let mut boxed = Box::new([0u8; 32]);
        boxed.copy_from_slice(session_keys.sqlcipher_key.expose());
        SqlcipherKey::from_secret_box(SecretBox::new(boxed))
    };
    let storage_staging_dir = storage::staging::default_staging_directory()
        .map_err(|_| AuthenticationError::VaultHeaderInvalid)?;
    storage::staging::ensure_staging_directory(&storage_staging_dir)
        .await
        .map_err(|_| AuthenticationError::VaultHeaderInvalid)?;
    download_manifest_backup(
        cloud_transport,
        &storage_staging_dir,
        &manifest_key_bytes,
        &vault_id,
        &request.vault_db_path,
        &sqlcipher_key,
    )
    .await
    .map_err(map_manifest_backup_sync_error)?;

    session_manager
        .finalize_session_install(
            install_reservation,
            session_keys,
            vault_id.to_uuid().to_string(),
            &request.vault_db_path,
        )
        .await?;

    drop(key_file_bytes);
    Ok(vault_id)
}

/// Maps manifest-backup sync failures into ceremony-visible auth errors.
fn map_manifest_backup_sync_error(error: ManifestBackupSyncError) -> AuthenticationError {
    match error {
        ManifestBackupSyncError::CryptoFailed | ManifestBackupSyncError::IntegrityCheckFailed => {
            AuthenticationError::InvalidCredentials
        }
        ManifestBackupSyncError::Transport(_)
        | ManifestBackupSyncError::StagingIo(_)
        | ManifestBackupSyncError::Vacuum(_)
        | ManifestBackupSyncError::ExportRead(_)
        | ManifestBackupSyncError::DbPersistIo(_) => AuthenticationError::VaultHeaderInvalid,
    }
}

#[cfg(test)]
mod tests {
    #![allow(unused_imports)]
    use super::super::helpers::*;
    use super::super::test_support::*;
    use super::*;
    use base64::Engine;
    use bip39::{Language, Mnemonic};
    use rusqlite::params;
    use uuid::Uuid;
    use zeroize::Zeroizing;

    use crate::auth::error::AuthenticationError;
    use crate::auth::kdf::derive_master_key_into;
    use crate::auth::key_source::MockKeySource;
    use crate::auth::session::{SessionKeys, SessionManager};
    use crate::auth::{
        Argon2Params, CreateVaultRequest, SetupRecoveryRequest, Tier, create_vault, setup_recovery,
    };
    use crate::crypto::{
        RecoveryKey, VaultId, WrappedFileKey, WrappedMasterKey, unwrap_master_key_from_recovery,
    };
    use crate::storage::cloud::CloudTransport;
    use crate::storage::cloud::mock::MockCloudTransport;
    use crate::storage::cloud::vault_header::VaultHeader;

    async fn create_tier_one_vault_with_default_params() -> TierOneVault {
        let temp = temp_dir();
        let vault_db_path = temp.path().join("vault-default-params.db");
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
        let vault_id = create_vault(request, &session, &cloud)
            .await
            .expect("create_vault with default Argon2 params must succeed");
        let header_bytes = {
            let header_download_path = temp.path().join("recover-vault-header.json");
            cloud
                .download_blob(VAULT_HEADER_BLOB_NAME, &header_download_path)
                .await
                .expect("header must be present after create_vault");
            tokio::fs::read(header_download_path)
                .await
                .expect("header must be readable")
        };
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

    #[tokio::test(flavor = "multi_thread")]
    async fn test_recover_vault_reconstructs_session_from_cloud_header_and_manifest_backup() {
        let _lock = ceremony_lock().await;
        let vault = create_tier_one_vault_with_default_params().await;
        upload_manifest_backup_for(&vault).await;
        vault.session.lock().await;

        let new_session = test_session_manager();
        let new_temp = temp_dir();
        let new_db_path = new_temp.path().join("recovered.db");
        let request = RecoverVaultRequest {
            password_bytes: TEST_PASSWORD,
            key_source: None,
            vault_db_path: new_db_path.clone(),
        };
        let recovered_vault_id = recover_vault(request, &new_session, &vault.cloud)
            .await
            .expect("recover_vault must succeed");
        assert_eq!(recovered_vault_id, vault.vault_id);
        assert!(new_db_path.exists());
        assert_eq!(
            new_session.state().await,
            crate::auth::LifecycleState::Active
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_create_vault_and_re_authenticate_round_trip_without_recovery_slot() {
        let _lock = ceremony_lock().await;
        let vault = create_tier_one_vault_with_default_params().await;
        upload_manifest_backup_for(&vault).await;
        vault.session.lock().await;

        let new_session = test_session_manager();
        let new_temp = temp_dir();
        let new_db_path = new_temp.path().join("round.db");
        let request = RecoverVaultRequest {
            password_bytes: TEST_PASSWORD,
            key_source: None,
            vault_db_path: new_db_path,
        };
        let recovered_vault_id = recover_vault(request, &new_session, &vault.cloud)
            .await
            .expect("recover_vault must succeed");
        assert_eq!(recovered_vault_id, vault.vault_id);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_recover_vault_when_session_active_returns_session_already_active_before_cloud_download()
     {
        let _lock = ceremony_lock().await;
        let active_session = test_session_manager();
        let seed_temp = temp_dir();
        let seed_cloud = MockCloudTransport::new();
        let seed_request = CreateVaultRequest {
            tier: Tier::One,
            password_bytes: TEST_PASSWORD,
            target_key_file_path: None,
            vault_db_path: seed_temp.path().join("seed.db"),
            argon2_params: Argon2Params::DEFAULT,
            chunk_size_bytes: CreateVaultRequest::DEFAULT_CHUNK_SIZE_BYTES,
            epoch_buffer_enabled: CreateVaultRequest::DEFAULT_EPOCH_BUFFER_ENABLED,
            vault_name: None,
            suggested_vault_id: None,
            primary_destination: None,
        };
        create_vault(seed_request, &active_session, &seed_cloud)
            .await
            .expect("seed create_vault must activate session");
        let temp = temp_dir();
        let request = RecoverVaultRequest {
            password_bytes: TEST_PASSWORD,
            key_source: None,
            vault_db_path: temp.path().join("should-not-create.db"),
        };
        let empty_cloud = MockCloudTransport::new();

        let result = recover_vault(request, &active_session, &empty_cloud).await;

        assert!(matches!(
            result,
            Err(AuthenticationError::SessionAlreadyActive)
        ));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_recover_vault_downloaded_header_argon2_params_below_floor_returns_vault_header_invalid()
     {
        let _lock = ceremony_lock().await;
        let vault = create_tier_one_vault_with_default_params().await;
        vault
            .cloud
            .download_blob(
                VAULT_HEADER_BLOB_NAME,
                &vault._temp.path().join("recover-vault-floor-header.json"),
            )
            .await
            .expect("header must be present");
        let header_bytes =
            std::fs::read(vault._temp.path().join("recover-vault-floor-header.json")).unwrap();
        let mut header: VaultHeader =
            serde_json::from_slice(&header_bytes).expect("header must deserialize");
        header.argon2_params.memory_cost = 19_455;
        let updated_header = serde_json::to_vec_pretty(&header).expect("header must serialize");
        let updated_header_path = vault
            ._temp
            .path()
            .join("recover-vault-floor-header-updated.json");
        std::fs::write(&updated_header_path, &updated_header).expect("header file must be written");
        vault
            .cloud
            .upload_blob(&updated_header_path, VAULT_HEADER_BLOB_NAME)
            .await
            .expect("header upload must succeed");
        vault.session.lock().await;

        let new_session = test_session_manager();
        let destination_root = temp_dir();
        let request = RecoverVaultRequest {
            password_bytes: TEST_PASSWORD,
            key_source: None,
            vault_db_path: destination_root.path().join("recover.db"),
        };

        let result = recover_vault(request, &new_session, &vault.cloud).await;

        assert!(matches!(
            result,
            Err(AuthenticationError::VaultHeaderInvalid)
        ));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_recover_vault_failed_import_cleans_temp_and_keeps_destination_absent() {
        let _lock = ceremony_lock().await;
        let vault = create_tier_one_vault_with_default_params().await;
        upload_manifest_backup_payload_for(&vault, b"not valid sql").await;
        vault.session.lock().await;

        let new_session = test_session_manager();
        let destination_root = temp_dir();
        let target = destination_root.path().join("recover.db");
        let request = RecoverVaultRequest {
            password_bytes: TEST_PASSWORD,
            key_source: None,
            vault_db_path: target.clone(),
        };

        let result = recover_vault(request, &new_session, &vault.cloud).await;

        assert!(matches!(
            result,
            Err(AuthenticationError::InvalidCredentials)
        ));
        assert!(!target.exists());
        let remaining_entries = std::fs::read_dir(destination_root.path())
            .expect("read_dir must succeed")
            .count();
        assert_eq!(remaining_entries, 0);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_recover_vault_manifest_backup_missing_returns_vault_header_invalid() {
        let _lock = ceremony_lock().await;
        let vault = create_tier_one_vault_with_default_params().await;
        vault.session.lock().await;

        let new_session = test_session_manager();
        let destination_root = temp_dir();
        let request = RecoverVaultRequest {
            password_bytes: TEST_PASSWORD,
            key_source: None,
            vault_db_path: destination_root.path().join("recover.db"),
        };

        let result = recover_vault(request, &new_session, &vault.cloud).await;

        assert!(matches!(
            result,
            Err(AuthenticationError::VaultHeaderInvalid)
        ));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_recover_vault_destination_exists_returns_vault_header_invalid() {
        let _lock = ceremony_lock().await;
        let destination_root = temp_dir();
        let destination = destination_root.path().join("recover.db");
        std::fs::write(&destination, b"existing destination").expect("seed file must be written");
        let request = RecoverVaultRequest {
            password_bytes: TEST_PASSWORD,
            key_source: None,
            vault_db_path: destination.clone(),
        };

        let result =
            recover_vault(request, &test_session_manager(), &MockCloudTransport::new()).await;

        assert!(matches!(
            result,
            Err(AuthenticationError::VaultHeaderInvalid)
        ));
        assert_eq!(
            std::fs::read(&destination).expect("seed file must remain readable"),
            b"existing destination"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_recover_vault_wrong_password_returns_invalid_credentials() {
        let _lock = ceremony_lock().await;
        let vault = create_tier_one_vault_with_default_params().await;
        upload_manifest_backup_for(&vault).await;
        vault.session.lock().await;

        let new_session = test_session_manager();
        let destination_root = temp_dir();
        let target = destination_root.path().join("recover.db");
        let request = RecoverVaultRequest {
            password_bytes: TEST_WRONG_PASSWORD,
            key_source: None,
            vault_db_path: target.clone(),
        };

        let result = recover_vault(request, &new_session, &vault.cloud).await;

        assert!(matches!(
            result,
            Err(AuthenticationError::InvalidCredentials)
        ));
        assert!(!target.exists());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_recover_vault_corrupted_manifest_backup_returns_invalid_credentials() {
        let _lock = ceremony_lock().await;
        let vault = create_tier_one_vault_with_default_params().await;
        upload_corrupted_manifest_backup_for(&vault).await;
        vault.session.lock().await;

        let new_session = test_session_manager();
        let destination_root = temp_dir();
        let target = destination_root.path().join("recover.db");
        let request = RecoverVaultRequest {
            password_bytes: TEST_PASSWORD,
            key_source: None,
            vault_db_path: target.clone(),
        };

        let result = recover_vault(request, &new_session, &vault.cloud).await;

        assert!(matches!(
            result,
            Err(AuthenticationError::InvalidCredentials)
        ));
        assert!(!target.exists());
    }
}
