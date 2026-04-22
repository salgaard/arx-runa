use chacha20poly1305::aead::OsRng;
use rand::Rng;
use rusqlite::params;
use uuid::Uuid;
use x25519_dalek::{PublicKey, StaticSecret};
use zeroize::Zeroizing;

use super::helpers::*;
use super::types::{CreateVaultRequest, Tier};
use crate::auth::error::AuthenticationError;
use crate::auth::kdf::derive_master_key_into;
use crate::auth::session::{SessionKeys, SessionManager};
use crate::auth::staging;
use crate::auth::TransportProvider;
use crate::crypto::VaultId;
use crate::storage::cloud::vault_header::VaultHeader;
use crate::storage::cloud::{
    VAULT_HEADER_UPLOAD_STAGING_FILE_NAME, upload_vault_header,
};
use crate::storage::schema::{apply_canonical_schema, seed_manifest_meta};
use std::path::Path;

#[cfg(test)]
use super::{STAGING_FILE_NAME, VAULT_HEADER_BLOB_NAME};

/// Creates a new vault: derives keys, creates the SQLCipher DB, builds and
/// uploads the vault header, and installs the resulting session.
///
/// Returns the newly generated [`VaultId`] on success.
///
/// # Errors
/// - [`AuthenticationError::VaultHeaderInvalid`] if the target key file's
///   parent directory is missing (Tier 2), the DB file already exists, or
///   the cloud upload fails.
/// - [`AuthenticationError::InvalidCredentials`] if key derivation or DB
///   creation fails.
/// - [`AuthenticationError::SessionAlreadyActive`] if a session is already
///   installed.
pub async fn create_vault(
    request: CreateVaultRequest<'_>,
    session_manager: &SessionManager,
    cloud_transport: &dyn TransportProvider,
) -> Result<VaultId, AuthenticationError> {
    validate_new_vault_argon2_defaults(&request.argon2_params)?;
    let install_reservation = session_manager.reserve_session_install().await?;

    match (request.tier, request.target_key_file_path.as_ref()) {
        (Tier::One, Some(_)) | (Tier::Two, None) => {
            return Err(AuthenticationError::VaultHeaderInvalid);
        }
        _ => {}
    }

    if tokio::fs::try_exists(&request.vault_db_path)
        .await
        .map_err(|_| AuthenticationError::VaultHeaderInvalid)?
    {
        return Err(AuthenticationError::VaultHeaderInvalid);
    }

    ensure_parent_directory_exists(&request.vault_db_path).await?;

    if request.tier == Tier::Two {
        let key_file_path = request
            .target_key_file_path
            .as_ref()
            .ok_or(AuthenticationError::VaultHeaderInvalid)?;
        let parent = key_file_path
            .parent()
            .ok_or(AuthenticationError::VaultHeaderInvalid)?;
        if !tokio::fs::try_exists(parent)
            .await
            .map_err(|_| AuthenticationError::VaultHeaderInvalid)?
        {
            return Err(AuthenticationError::VaultHeaderInvalid);
        }
    }

    let vault_id = VaultId::from_uuid(Uuid::new_v4());

    let mut key_file_bytes: Option<Zeroizing<[u8; 32]>> = None;
    let mut key_file_blake3_hex: Option<String> = None;
    if request.tier == Tier::Two {
        let key_file_path = request
            .target_key_file_path
            .as_ref()
            .ok_or(AuthenticationError::VaultHeaderInvalid)?;
        let mut buffer: Zeroizing<[u8; 32]> = Zeroizing::new([0u8; 32]);
        rand::rng().fill_bytes(buffer.as_mut_slice());
        staging::write_owner_only_new(key_file_path, buffer.as_slice()).await?;
        let digest = blake3::hash(buffer.as_slice());
        key_file_blake3_hex = Some(hex::encode(digest.as_bytes()));
        key_file_bytes = Some(buffer);
    }

    let mut argon2_salt: Zeroizing<[u8; 32]> = Zeroizing::new([0u8; 32]);
    rand::rng().fill_bytes(argon2_salt.as_mut_slice());

    let mut master_key: Zeroizing<[u8; 32]> = Zeroizing::new([0u8; 32]);
    let derive_result = derive_master_key_into(
        request.password_bytes,
        key_file_bytes.as_deref(),
        &argon2_salt,
        &request.argon2_params,
        &mut master_key,
    );
    if let Err(error) = derive_result {
        if let Some(key_file_path) = request.target_key_file_path.as_ref() {
            remove_file_if_exists(key_file_path).await;
        }
        return Err(error);
    }

    let session_keys = match SessionKeys::from_master_key_bytes(&master_key) {
        Ok(keys) => keys,
        Err(error) => {
            if let Some(key_file_path) = request.target_key_file_path.as_ref() {
                remove_file_if_exists(key_file_path).await;
            }
            return Err(error);
        }
    };

    let static_secret = StaticSecret::random_from_rng(OsRng);
    let x25519_secret_bytes: Zeroizing<[u8; 32]> = Zeroizing::new(static_secret.to_bytes());
    let public_key = PublicKey::from(&static_secret);
    let public_key_bytes = public_key.to_bytes();

    let wrapped_private_key = wrap_with_session_kek(&session_keys, &x25519_secret_bytes)?;

    let sqlcipher_key = sqlcipher_key_from_array(session_keys.sqlcipher_key.expose());
    let vault_db_path_owned = request.vault_db_path.clone();
    let vault_id_uuid = vault_id.to_uuid();
    let chunk_size_bytes = request.chunk_size_bytes;
    let epoch_buffer_enabled = request.epoch_buffer_enabled;
    let wrapped_private_key_vec: Vec<u8> = wrapped_private_key.0.to_vec();
    let public_key_vec: Vec<u8> = public_key_bytes.to_vec();
    let db_result: Result<(), AuthenticationError> =
        tokio::task::spawn_blocking(move || -> Result<(), AuthenticationError> {
            let conn = open_sqlcipher(&vault_db_path_owned, &sqlcipher_key)?;
            apply_canonical_schema(&conn).map_err(|_| AuthenticationError::VaultHeaderInvalid)?;
            seed_manifest_meta(&conn, vault_id_uuid, chunk_size_bytes, epoch_buffer_enabled)
                .map_err(|_| AuthenticationError::VaultHeaderInvalid)?;
            conn.execute(
                "INSERT INTO vault_identity (id, public_key, wrapped_private_key) VALUES (1, ?, ?)",
                params![public_key_vec, wrapped_private_key_vec],
            )
            .map_err(|_| AuthenticationError::VaultHeaderInvalid)?;
            drop(conn);
            Ok(())
        })
        .await
        .map_err(|_| AuthenticationError::VaultHeaderInvalid)?;
    if let Err(error) = db_result {
        if let Some(key_file_path) = request.target_key_file_path.as_ref() {
            remove_file_if_exists(key_file_path).await;
        }
        remove_file_if_exists(&request.vault_db_path).await;
        return Err(error);
    }

    let header = VaultHeader {
        vault_id: vault_id.to_uuid().to_string(),
        schema_version: VaultHeader::SCHEMA_VERSION,
        tier: request.tier.as_u8(),
        argon2_salt: encode_base64(argon2_salt.as_slice()),
        argon2_params: argon2_params_to_json(&request.argon2_params),
        key_file_blake3: key_file_blake3_hex,
        recovery_slots: Vec::new(),
    };

    let staging_dir = staging::staging_directory().await?;
    let staging_path = staging_dir.join(VAULT_HEADER_UPLOAD_STAGING_FILE_NAME);
    let upload_result = upload_vault_header(&header, cloud_transport, &staging_dir).await;
    if let Err(error) = upload_result {
        rollback_after_header_publish_failure(
            &staging_path,
            None,
            request.target_key_file_path.as_deref(),
            &request.vault_db_path,
            "staging cleanup failed after vault-header publish failure",
        )
        .await;
        return Err(map_vault_header_sync_error(error));
    }
    session_manager
        .finalize_session_install(install_reservation, session_keys, vault_id.to_uuid().to_string())
        .await?;

    drop(master_key);
    drop(x25519_secret_bytes);
    drop(argon2_salt);
    drop(key_file_bytes);

    Ok(vault_id)
}

async fn rollback_after_header_publish_failure(
    staging_path: &Path,
    staging_cleanup_result: Option<Result<(), AuthenticationError>>,
    target_key_file_path: Option<&Path>,
    vault_db_path: &Path,
    staging_cleanup_warning_message: &'static str,
) {
    let cleanup_result = match staging_cleanup_result {
        Some(result) => result,
        None => staging::remove_if_exists(staging_path).await,
    };
    if let Err(cleanup_error) = cleanup_result {
        tracing::warn!(?cleanup_error, staging_cleanup_warning_message);
    }
    if let Some(key_file_path) = target_key_file_path {
        remove_file_if_exists(key_file_path).await;
    }
    remove_file_if_exists(vault_db_path).await;
}

#[cfg(test)]
mod tests {
    #![allow(unused_imports)]
    use super::super::helpers::*;
    use super::super::test_support::*;
    use super::*;
    use async_trait::async_trait;
    use base64::Engine;
    use bip39::{Language, Mnemonic};
    use rusqlite::params;
    use uuid::Uuid;
    use zeroize::Zeroizing;

    use crate::auth::error::AuthenticationError;
    use crate::auth::kdf::derive_master_key_into;
    use crate::auth::key_source::MockKeySource;
    use crate::auth::session::{SessionKeys, SessionManager};
    use crate::auth::staging;
    use crate::auth::{
        Argon2Params, CreateVaultRequest, SetupRecoveryRequest, Tier, create_vault, setup_recovery,
    };
    use crate::crypto::{
        RecoveryKey, VaultId, WrappedFileKey, WrappedMasterKey, unwrap_master_key_from_recovery,
    };
    use crate::storage::cloud::mock::MockCloudTransport;
    use crate::storage::cloud::vault_header::VaultHeader;
    use crate::storage::cloud::{CloudTransport, CloudTransportError};

    #[derive(Debug, Default)]
    struct UploadFailCloudTransport;

    #[async_trait]
    impl CloudTransport for UploadFailCloudTransport {
        async fn upload_blob(
            &self,
            _local_path: &std::path::Path,
            _remote_path: &str,
        ) -> Result<(), CloudTransportError> {
            Err(CloudTransportError::Other(
                "forced upload failure".to_string(),
            ))
        }

        async fn download_blob(
            &self,
            _remote_path: &str,
            _local_path: &std::path::Path,
        ) -> Result<(), CloudTransportError> {
            Err(CloudTransportError::NotFound)
        }

        async fn delete_blob(&self, _remote_path: &str) -> Result<(), CloudTransportError> {
            Ok(())
        }

        async fn list_blobs(
            &self,
            _remote_prefix: &str,
        ) -> Result<Vec<String>, CloudTransportError> {
            Ok(Vec::new())
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_create_vault_tier_one_produces_header_with_null_key_file_blake3_and_empty_recovery_slots()
     {
        let _lock = ceremony_lock().await;
        let vault = create_tier_one_vault().await;
        assert_eq!(vault.header.tier, 1);
        assert!(vault.header.key_file_blake3.is_none());
        assert!(vault.header.recovery_slots.is_empty());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_create_vault_tier_two_generates_key_file_and_sets_key_file_blake3() {
        let _lock = ceremony_lock().await;
        let vault = create_tier_two_vault().await;
        assert_eq!(vault.header.tier, 2);
        assert!(vault.key_file_path.exists());
        let key_bytes = std::fs::read(&vault.key_file_path).expect("key file must exist");
        assert_eq!(key_bytes.len(), 32);
        let expected_hex = hex::encode(blake3::hash(&key_bytes).as_bytes());
        assert_eq!(
            vault.header.key_file_blake3.as_deref(),
            Some(expected_hex.as_str())
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_create_vault_opens_sqlcipher_with_derived_sqlcipher_key() {
        let _lock = ceremony_lock().await;
        let vault = create_tier_one_vault().await;
        assert!(vault.vault_db_path.exists());
        assert_eq!(vault.header.schema_version, VaultHeader::SCHEMA_VERSION);
        assert_eq!(
            VaultId::from_uuid(Uuid::parse_str(&vault.header.vault_id).unwrap()),
            vault.vault_id
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_create_vault_rejects_non_default_argon2_params() {
        let _lock = ceremony_lock().await;
        let temp = temp_dir();
        let vault_db_path = temp.path().join("vault.db");
        let cloud = MockCloudTransport::new();
        let session = test_session_manager();
        let request = CreateVaultRequest {
            tier: Tier::One,
            password_bytes: TEST_PASSWORD,
            target_key_file_path: None,
            vault_db_path,
            argon2_params: test_params(),
            chunk_size_bytes: CreateVaultRequest::DEFAULT_CHUNK_SIZE_BYTES,
            epoch_buffer_enabled: CreateVaultRequest::DEFAULT_EPOCH_BUFFER_ENABLED,
        };

        let result = create_vault(request, &session, &cloud).await;

        assert!(matches!(
            result,
            Err(AuthenticationError::VaultHeaderInvalid)
        ));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_create_vault_rejects_missing_target_key_file_path_for_tier_two() {
        let _lock = ceremony_lock().await;
        let temp = temp_dir();
        let vault_db_path = temp.path().join("vault.db");
        let cloud = MockCloudTransport::new();
        let session = test_session_manager();
        let request = CreateVaultRequest {
            tier: Tier::Two,
            password_bytes: TEST_PASSWORD,
            target_key_file_path: None,
            vault_db_path,
            argon2_params: Argon2Params::DEFAULT,
            chunk_size_bytes: CreateVaultRequest::DEFAULT_CHUNK_SIZE_BYTES,
            epoch_buffer_enabled: CreateVaultRequest::DEFAULT_EPOCH_BUFFER_ENABLED,
        };
        let result = create_vault(request, &session, &cloud).await;
        assert!(matches!(
            result,
            Err(AuthenticationError::VaultHeaderInvalid)
        ));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_create_vault_rejects_writable_parent_missing_for_tier_two() {
        let _lock = ceremony_lock().await;
        let temp = temp_dir();
        let vault_db_path = temp.path().join("vault.db");
        let nonexistent_parent = temp.path().join("does-not-exist").join("key.bin");
        let cloud = MockCloudTransport::new();
        let session = test_session_manager();
        let request = CreateVaultRequest {
            tier: Tier::Two,
            password_bytes: TEST_PASSWORD,
            target_key_file_path: Some(nonexistent_parent),
            vault_db_path,
            argon2_params: Argon2Params::DEFAULT,
            chunk_size_bytes: CreateVaultRequest::DEFAULT_CHUNK_SIZE_BYTES,
            epoch_buffer_enabled: CreateVaultRequest::DEFAULT_EPOCH_BUFFER_ENABLED,
        };
        let result = create_vault(request, &session, &cloud).await;
        assert!(matches!(
            result,
            Err(AuthenticationError::VaultHeaderInvalid)
        ));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_create_vault_rejects_existing_tier_two_key_file_path_without_overwrite() {
        let _lock = ceremony_lock().await;
        let temp = temp_dir();
        let vault_db_path = temp.path().join("vault.db");
        let key_file_path = temp.path().join("existing-key.bin");
        let existing_content = b"keep-existing-content";
        std::fs::write(&key_file_path, existing_content).expect("seed key file must be written");

        let cloud = MockCloudTransport::new();
        let session = test_session_manager();
        let request = CreateVaultRequest {
            tier: Tier::Two,
            password_bytes: TEST_PASSWORD,
            target_key_file_path: Some(key_file_path.clone()),
            vault_db_path,
            argon2_params: Argon2Params::DEFAULT,
            chunk_size_bytes: CreateVaultRequest::DEFAULT_CHUNK_SIZE_BYTES,
            epoch_buffer_enabled: CreateVaultRequest::DEFAULT_EPOCH_BUFFER_ENABLED,
        };

        let result = create_vault(request, &session, &cloud).await;
        assert!(matches!(
            result,
            Err(AuthenticationError::VaultHeaderInvalid)
        ));
        let preserved = std::fs::read(&key_file_path).expect("key file must remain readable");
        assert_eq!(preserved, existing_content);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_create_vault_when_session_active_returns_session_already_active_without_new_side_effects()
     {
        let _lock = ceremony_lock().await;
        let existing_vault = create_tier_one_vault().await;
        let temp = temp_dir();
        let new_vault_db_path = temp.path().join("new-vault.db");
        let new_key_file_path = temp.path().join("new-key.bin");
        existing_vault
            .cloud
            .download_blob(
                VAULT_HEADER_BLOB_NAME,
                &temp.path().join("header-before.json"),
            )
            .await
            .expect("existing header must be available");
        let header_before = tokio::fs::read(temp.path().join("header-before.json"))
            .await
            .expect("existing header bytes must be readable");
        let request = CreateVaultRequest {
            tier: Tier::Two,
            password_bytes: TEST_PASSWORD,
            target_key_file_path: Some(new_key_file_path.clone()),
            vault_db_path: new_vault_db_path.clone(),
            argon2_params: Argon2Params::DEFAULT,
            chunk_size_bytes: CreateVaultRequest::DEFAULT_CHUNK_SIZE_BYTES,
            epoch_buffer_enabled: CreateVaultRequest::DEFAULT_EPOCH_BUFFER_ENABLED,
        };

        let result = create_vault(request, &existing_vault.session, &existing_vault.cloud).await;

        assert!(matches!(
            result,
            Err(AuthenticationError::SessionAlreadyActive)
        ));
        assert!(!new_vault_db_path.exists());
        assert!(!new_key_file_path.exists());
        existing_vault
            .cloud
            .download_blob(
                VAULT_HEADER_BLOB_NAME,
                &temp.path().join("header-after.json"),
            )
            .await
            .expect("existing header must remain available");
        let header_after = tokio::fs::read(temp.path().join("header-after.json"))
            .await
            .expect("existing header bytes must be readable");
        assert_eq!(header_after, header_before);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_create_vault_upload_failure_preserves_no_session_and_cleans_staging_and_local_files()
     {
        let _lock = ceremony_lock().await;
        let temp = temp_dir();
        let session = test_session_manager();
        let cloud = UploadFailCloudTransport;
        let vault_db_path = temp.path().join("vault.db");
        let key_file_path = temp.path().join("key.bin");
        let pending_path = staging::staging_directory()
            .await
            .expect("staging dir must exist")
            .join(STAGING_FILE_NAME);
        let _ = staging::remove_if_exists(&pending_path).await;
        let request = CreateVaultRequest {
            tier: Tier::Two,
            password_bytes: TEST_PASSWORD,
            target_key_file_path: Some(key_file_path.clone()),
            vault_db_path: vault_db_path.clone(),
            argon2_params: Argon2Params::DEFAULT,
            chunk_size_bytes: CreateVaultRequest::DEFAULT_CHUNK_SIZE_BYTES,
            epoch_buffer_enabled: CreateVaultRequest::DEFAULT_EPOCH_BUFFER_ENABLED,
        };

        let result = create_vault(request, &session, &cloud).await;

        assert!(matches!(
            result,
            Err(AuthenticationError::VaultHeaderInvalid)
        ));
        assert_eq!(
            session.state().await,
            crate::auth::LifecycleState::NoSession
        );
        assert!(!vault_db_path.exists());
        assert!(!key_file_path.exists());
        assert!(!tokio::fs::try_exists(&pending_path).await.unwrap_or(false));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_create_vault_staging_write_failure_preserves_no_session_and_cleans_local_files() {
        let _lock = ceremony_lock().await;
        let temp = temp_dir();
        let session = test_session_manager();
        let cloud = MockCloudTransport::new();
        let vault_db_path = temp.path().join("vault.db");
        let key_file_path = temp.path().join("key.bin");
        let pending_path = staging::staging_directory()
            .await
            .expect("staging dir must exist")
            .join(STAGING_FILE_NAME);
        let _ = staging::remove_if_exists(&pending_path).await;
        let _ = tokio::fs::remove_dir_all(&pending_path).await;
        tokio::fs::create_dir_all(&pending_path)
            .await
            .expect("directory at staging file path must be creatable");
        let request = CreateVaultRequest {
            tier: Tier::Two,
            password_bytes: TEST_PASSWORD,
            target_key_file_path: Some(key_file_path.clone()),
            vault_db_path: vault_db_path.clone(),
            argon2_params: Argon2Params::DEFAULT,
            chunk_size_bytes: CreateVaultRequest::DEFAULT_CHUNK_SIZE_BYTES,
            epoch_buffer_enabled: CreateVaultRequest::DEFAULT_EPOCH_BUFFER_ENABLED,
        };

        let result = create_vault(request, &session, &cloud).await;

        assert!(matches!(
            result,
            Err(AuthenticationError::VaultHeaderInvalid)
        ));
        assert_eq!(
            session.state().await,
            crate::auth::LifecycleState::NoSession
        );
        assert!(!vault_db_path.exists());
        assert!(!key_file_path.exists());
        tokio::fs::remove_dir_all(&pending_path)
            .await
            .expect("directory at staging path must be removable");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_create_vault_success_cleans_pending_header_staging_file() {
        let _lock = ceremony_lock().await;
        let temp = temp_dir();
        let session = test_session_manager();
        let cloud = MockCloudTransport::new();
        let vault_db_path = temp.path().join("vault-success.db");
        let pending_path = staging::staging_directory()
            .await
            .expect("staging dir must exist")
            .join(STAGING_FILE_NAME);
        let _ = staging::remove_if_exists(&pending_path).await;
        let request = CreateVaultRequest {
            tier: Tier::One,
            password_bytes: TEST_PASSWORD,
            target_key_file_path: None,
            vault_db_path,
            argon2_params: Argon2Params::DEFAULT,
            chunk_size_bytes: CreateVaultRequest::DEFAULT_CHUNK_SIZE_BYTES,
            epoch_buffer_enabled: CreateVaultRequest::DEFAULT_EPOCH_BUFFER_ENABLED,
        };

        let result = create_vault(request, &session, &cloud).await;

        assert!(result.is_ok());
        assert!(!tokio::fs::try_exists(&pending_path).await.unwrap_or(false));
    }
}
