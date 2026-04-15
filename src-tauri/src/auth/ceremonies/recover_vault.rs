use uuid::Uuid;
use zeroize::Zeroizing;

use super::helpers::*;
use super::types::RecoverVaultRequest;
use super::{manifest_backup_blob_name, vault_header_blob_name};
use crate::auth::error::AuthenticationError;
use crate::auth::kdf::derive_master_key_into;
use crate::auth::session::{SessionKeys, SessionManager};
use crate::crypto::VaultId;
use crate::storage::cloud::CloudTransport;
use crate::storage::cloud::manifest_backup::decrypt_manifest_backup;
use crate::storage::cloud::vault_header::VaultHeader;

/// Recovers a vault onto a new device by downloading its header and
/// manifest backup, re-deriving the session keys, and importing the
/// backup into a fresh local SQLCipher DB.
pub async fn recover_vault(
    request: RecoverVaultRequest<'_>,
    session_manager: &SessionManager,
    cloud_transport: &dyn CloudTransport,
) -> Result<VaultId, AuthenticationError> {
    let header_bytes = cloud_transport
        .download_blob(&vault_header_blob_name())
        .await
        .map_err(|_| AuthenticationError::VaultHeaderInvalid)?;
    let header: VaultHeader = serde_json::from_slice(&header_bytes)
        .map_err(|_| AuthenticationError::VaultHeaderInvalid)?;
    header
        .validate()
        .map_err(|_| AuthenticationError::VaultHeaderInvalid)?;

    let vault_uuid =
        Uuid::parse_str(&header.vault_id).map_err(|_| AuthenticationError::VaultHeaderInvalid)?;
    let vault_id = VaultId::from_uuid(vault_uuid);
    let salt = decode_base64_32(&header.argon2_salt)?;
    let params = argon2_params_from_json(&header.argon2_params);
    enforce_argon2_policy(&params)?;

    let key_file_bytes: Option<Zeroizing<[u8; 32]>> = match (header.tier, request.key_source) {
        (1, _) => None,
        (2, Some(source)) => {
            let bytes = source
                .read_key()
                .map_err(|_| AuthenticationError::KeyFileNotFound)?;
            let expected_hex = header
                .key_file_blake3
                .as_ref()
                .ok_or(AuthenticationError::VaultHeaderInvalid)?;
            let expected_digest =
                hex::decode(expected_hex).map_err(|_| AuthenticationError::VaultHeaderInvalid)?;
            let actual_digest = blake3::hash(bytes.as_slice());
            if expected_digest.as_slice() != actual_digest.as_bytes() {
                return Err(AuthenticationError::KeyFileNotFound);
            }
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
        &params,
        &mut master_key,
    )?;
    let session_keys = SessionKeys::from_master_key_bytes(&master_key)?;
    let sqlcipher_key = sqlcipher_key_from_array(session_keys.sqlcipher_key.expose());

    let backup_wire = cloud_transport
        .download_blob(&manifest_backup_blob_name())
        .await
        .map_err(|_| AuthenticationError::VaultHeaderInvalid)?;
    let plaintext = decrypt_manifest_backup(&backup_wire, session_keys.manifest_key.expose())
        .map_err(|_| AuthenticationError::InvalidCredentials)?;

    if tokio::fs::try_exists(&request.vault_db_path)
        .await
        .map_err(|_| AuthenticationError::VaultHeaderInvalid)?
    {
        return Err(AuthenticationError::VaultHeaderInvalid);
    }
    ensure_parent_directory_exists(&request.vault_db_path).await?;

    let vault_db_path = request.vault_db_path.clone();
    let db_result: Result<(), AuthenticationError> =
        tokio::task::spawn_blocking(move || -> Result<(), AuthenticationError> {
            let sql_text = std::str::from_utf8(plaintext.as_slice())
                .map_err(|_| AuthenticationError::VaultHeaderInvalid)?;
            let conn = open_sqlcipher(&vault_db_path, sqlcipher_key.expose())?;
            conn.execute_batch(sql_text)
                .map_err(|_| AuthenticationError::VaultHeaderInvalid)?;
            drop(conn);
            Ok(())
        })
        .await
        .map_err(|_| AuthenticationError::VaultHeaderInvalid)?;
    db_result?;

    session_manager.install_session(session_keys).await?;

    drop(master_key);
    drop(key_file_bytes);
    Ok(vault_id)
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
        CreateVaultRequest, SetupRecoveryRequest, Tier, create_vault, setup_recovery,
    };
    use crate::crypto::{
        RecoveryKey, VaultId, WrappedFileKey, WrappedMasterKey, unwrap_master_key_from_recovery,
    };
    use crate::storage::cloud::CloudTransport;
    use crate::storage::cloud::mock::MockCloudTransport;
    use crate::storage::cloud::vault_header::VaultHeader;

    #[tokio::test(flavor = "multi_thread")]
    async fn test_recover_vault_reconstructs_session_from_cloud_header_and_manifest_backup() {
        let _lock = ceremony_lock().await;
        let vault = create_tier_one_vault().await;
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
        let vault = create_tier_one_vault().await;
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
}
