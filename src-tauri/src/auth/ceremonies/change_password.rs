use rand::Rng;
use rusqlite::{OptionalExtension, params};
use zeroize::Zeroizing;

use super::helpers::*;
use super::types::ChangePasswordRequest;
use super::{STAGING_FILE_NAME, VAULT_HEADER_BLOB_NAME};
use crate::auth::error::AuthenticationError;
use crate::auth::kdf::derive_master_key_into;
use crate::auth::session::{SessionKeys, SessionManager};
use crate::auth::staging;
use crate::crypto::{
    RecoveryKey, VaultId, WrappedFileKey, WrappedMasterKey, unwrap_file_key,
    unwrap_master_key_from_recovery, wrap_file_key, wrap_master_key_for_recovery,
};
use crate::storage::cloud::CloudTransport;
use crate::storage::cloud::vault_header::VaultHeader;

/// Changes the user's password by re-wrapping all stored keys under a new
/// master key and rekeying the SQLCipher database.
///
/// The active session is swapped via [`SessionManager::swap_active_session`].
///
/// # Errors
/// - [`AuthenticationError::SessionNotActive`] if no session is active.
/// - [`AuthenticationError::InvalidCredentials`] if the current credentials
///   do not unwrap the vault identity row, or a re-wrap step fails.
/// - [`AuthenticationError::InvalidRecoveryPhrase`] if a recovery phrase is
///   supplied but fails the BIP-39 checksum.
pub async fn change_password(
    request: ChangePasswordRequest<'_>,
    session_manager: &SessionManager,
    cloud_transport: &dyn CloudTransport,
    vault_header: &mut VaultHeader,
    vault_id: &VaultId,
) -> Result<(), AuthenticationError> {
    enforce_argon2_policy(&request.argon2_params)?;
    let _operation_guard = session_manager.begin_operation();

    if session_manager.state().await != crate::auth::LifecycleState::Active {
        return Err(AuthenticationError::SessionNotActive);
    }

    let current_salt = decode_base64_32(&vault_header.argon2_salt)?;
    let current_params = argon2_params_from_json(&vault_header.argon2_params);

    let current_key_file_bytes: Option<Zeroizing<[u8; 32]>> =
        match (vault_header.tier, request.current_key_source) {
            (1, _) => None,
            (2, Some(source)) => {
                let bytes = source
                    .read_key()
                    .map_err(|_| AuthenticationError::InvalidCredentials)?;
                let mut buffer: Zeroizing<[u8; 32]> = Zeroizing::new([0u8; 32]);
                buffer.copy_from_slice(bytes.as_slice());
                Some(buffer)
            }
            _ => return Err(AuthenticationError::InvalidCredentials),
        };

    let mut current_master_key: Zeroizing<[u8; 32]> = Zeroizing::new([0u8; 32]);
    derive_master_key_into(
        request.current_password_bytes,
        current_key_file_bytes.as_deref(),
        &current_salt,
        &current_params,
        &mut current_master_key,
    )?;
    let current_session_keys = SessionKeys::from_master_key_bytes(&current_master_key)?;
    let current_kek =
        key_encryption_key_from_array(current_session_keys.key_encryption_key.expose());
    let current_sqlcipher = sqlcipher_key_from_array(current_session_keys.sqlcipher_key.expose());

    let mut will_remove_slots = false;
    let mut recovery_key_for_rewrap: Option<RecoveryKey> = None;
    if !vault_header.recovery_slots.is_empty() {
        match request.recovery_phrase {
            None => will_remove_slots = true,
            Some(phrase) => {
                let mnemonic = parse_mnemonic(phrase)?;
                let canonical = canonicalize_phrase(&mnemonic);
                let slot_index = vault_header
                    .recovery_slots
                    .iter()
                    .position(|slot| slot.method == "bip39")
                    .ok_or(AuthenticationError::NoRecoverySlot)?;
                let slot = &vault_header.recovery_slots[slot_index];
                let slot_salt = decode_base64_32(&slot.argon2_salt)?;
                let slot_params = argon2_params_from_json(&slot.argon2_params);
                let wrapped_bytes = decode_base64_72(&slot.wrapped_master_key)?;
                let wrapped = WrappedMasterKey(wrapped_bytes);

                let mut recovery_key_bytes: Zeroizing<[u8; 32]> = Zeroizing::new([0u8; 32]);
                derive_recovery_key_into(
                    canonical.as_bytes(),
                    &slot_salt,
                    &slot_params,
                    &mut recovery_key_bytes,
                )?;
                let recovery_key = recovery_key_from_array(&recovery_key_bytes);
                drop(recovery_key_bytes);
                match unwrap_master_key_from_recovery(&wrapped, &recovery_key, vault_id) {
                    Ok(_master_key) => {
                        recovery_key_for_rewrap = Some(recovery_key);
                    }
                    Err(_) => return Err(AuthenticationError::InvalidCredentials),
                }
            }
        }
    }

    let mut new_salt: Zeroizing<[u8; 32]> = Zeroizing::new([0u8; 32]);
    rand::rng().fill_bytes(new_salt.as_mut_slice());
    let mut new_master_key: Zeroizing<[u8; 32]> = Zeroizing::new([0u8; 32]);
    derive_master_key_into(
        request.new_password_bytes,
        current_key_file_bytes.as_deref(),
        &new_salt,
        &request.argon2_params,
        &mut new_master_key,
    )?;
    let new_session_keys = SessionKeys::from_master_key_bytes(&new_master_key)?;
    let new_kek = key_encryption_key_from_array(new_session_keys.key_encryption_key.expose());
    let new_sqlcipher = sqlcipher_key_from_array(new_session_keys.sqlcipher_key.expose());

    let vault_db_path = request.vault_db_path.clone();
    let rewrap_result: Result<(), AuthenticationError> =
        tokio::task::spawn_blocking(move || -> Result<(), AuthenticationError> {
            let conn = open_sqlcipher(&vault_db_path, current_sqlcipher.expose())?;
            conn.execute_batch("BEGIN IMMEDIATE;")
                .map_err(|_| AuthenticationError::InvalidCredentials)?;
            let transaction_result = (|| -> Result<(), AuthenticationError> {
                {
                    let mut stmt = conn
                        .prepare(
                            "SELECT node_id, file_key_wrapped FROM nodes WHERE file_key_wrapped IS NOT NULL AND node_id IS NOT NULL",
                        )
                        .map_err(|_| AuthenticationError::InvalidCredentials)?;
                    let rows: Vec<(String, Vec<u8>)> = stmt
                        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
                        .map_err(|_| AuthenticationError::InvalidCredentials)?
                        .collect::<Result<_, _>>()
                        .map_err(|_| AuthenticationError::InvalidCredentials)?;
                    for (node_id, wrapped_blob) in rows {
                        let wrapped_array: [u8; 72] = wrapped_blob
                            .try_into()
                            .map_err(|_| AuthenticationError::InvalidCredentials)?;
                        let wrapped = WrappedFileKey(wrapped_array);
                        let file_key = unwrap_file_key(&wrapped, &current_kek)
                            .map_err(|_| AuthenticationError::InvalidCredentials)?;
                        let rewrapped = wrap_file_key(&file_key, &new_kek)
                            .map_err(|_| AuthenticationError::VaultHeaderInvalid)?;
                        conn.execute(
                            "UPDATE nodes SET file_key_wrapped = ? WHERE node_id = ?",
                            params![rewrapped.0.to_vec(), node_id],
                        )
                        .map_err(|_| AuthenticationError::InvalidCredentials)?;
                    }
                }
                let identity_wrapped: Option<Vec<u8>> = conn
                    .query_row(
                        "SELECT wrapped_private_key FROM vault_identity WHERE id = 1",
                        [],
                        |row| row.get(0),
                    )
                    .optional()
                    .map_err(|_| AuthenticationError::InvalidCredentials)?;
                if let Some(wrapped_blob) = identity_wrapped {
                    let wrapped_array: [u8; 72] = wrapped_blob
                        .try_into()
                        .map_err(|_| AuthenticationError::InvalidCredentials)?;
                    let wrapped = WrappedFileKey(wrapped_array);
                    let file_key = unwrap_file_key(&wrapped, &current_kek)
                        .map_err(|_| AuthenticationError::InvalidCredentials)?;
                    let rewrapped = wrap_file_key(&file_key, &new_kek)
                        .map_err(|_| AuthenticationError::VaultHeaderInvalid)?;
                    conn.execute(
                        "UPDATE vault_identity SET wrapped_private_key = ? WHERE id = 1",
                        params![rewrapped.0.to_vec()],
                    )
                    .map_err(|_| AuthenticationError::InvalidCredentials)?;
                }
                Ok(())
            })();
            match transaction_result {
                Ok(()) => {
                    conn.execute_batch("COMMIT;")
                        .map_err(|_| AuthenticationError::InvalidCredentials)?;
                    rekey_sqlcipher(&conn, new_sqlcipher.expose())?;
                    drop(conn);
                    Ok(())
                }
                Err(error) => {
                    let _ = conn.execute_batch("ROLLBACK;");
                    drop(conn);
                    Err(error)
                }
            }
        })
        .await
        .map_err(|_| AuthenticationError::VaultHeaderInvalid)?;
    rewrap_result?;

    vault_header.argon2_salt = encode_base64(new_salt.as_slice());
    vault_header.argon2_params = argon2_params_to_json(&request.argon2_params);
    if will_remove_slots {
        vault_header.recovery_slots.clear();
    } else if let Some(recovery_key) = recovery_key_for_rewrap.as_ref() {
        let master_key = master_key_from_array(&new_master_key);
        let rewrapped = wrap_master_key_for_recovery(&master_key, recovery_key, vault_id)
            .map_err(|_| AuthenticationError::VaultHeaderInvalid)?;
        drop(master_key);
        if let Some(slot) = vault_header
            .recovery_slots
            .iter_mut()
            .find(|slot| slot.method == "bip39")
        {
            slot.wrapped_master_key = encode_base64(&rewrapped.0);
        }
    }
    drop(recovery_key_for_rewrap);

    let json_bytes = serde_json::to_vec_pretty(&vault_header)
        .map_err(|_| AuthenticationError::VaultHeaderInvalid)?;
    let staging_dir = staging::staging_directory().await?;
    let staging_path = staging_dir.join(STAGING_FILE_NAME);
    staging::write_owner_only(&staging_path, &json_bytes).await?;
    session_manager
        .swap_active_session(new_session_keys)
        .await?;
    let upload_result = cloud_transport
        .upload_blob(&staging_path, VAULT_HEADER_BLOB_NAME)
        .await;
    if upload_result.is_err() {
        return Err(AuthenticationError::VaultHeaderInvalid);
    }
    best_effort_cleanup_staging(&staging_path).await;

    drop(current_master_key);
    drop(new_master_key);
    drop(new_salt);
    drop(current_key_file_bytes);
    Ok(())
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
        CreateVaultRequest, SetupRecoveryRequest, Tier, create_vault, setup_recovery,
    };
    use crate::crypto::{
        RecoveryKey, VaultId, WrappedFileKey, WrappedMasterKey, unwrap_master_key_from_recovery,
    };
    use crate::storage::cloud::CloudTransport;
    use crate::storage::cloud::CloudTransportError;
    use crate::storage::cloud::mock::MockCloudTransport;
    use crate::storage::cloud::vault_header::VaultHeader;

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
    async fn test_change_password_old_kek_cannot_unwrap_file_keys_after_change() {
        let _lock = ceremony_lock().await;
        let mut vault = create_tier_one_vault().await;

        let current_salt = decode_base64_32(&vault.header.argon2_salt).unwrap();
        let current_params = argon2_params_from_json(&vault.header.argon2_params);
        let mut old_master: Zeroizing<[u8; 32]> = Zeroizing::new([0u8; 32]);
        derive_master_key_into(
            TEST_PASSWORD,
            None,
            &current_salt,
            &current_params,
            &mut old_master,
        )
        .unwrap();
        let old_keys = SessionKeys::from_master_key_bytes(&old_master).unwrap();
        let old_kek: [u8; 32] = *old_keys.key_encryption_key.expose();
        let old_sqlcipher: [u8; 32] = *old_keys.sqlcipher_key.expose();

        let file_key_plain = [0xCDu8; 32];
        let wrapped = wrap_with_kek_bytes(&old_kek, &file_key_plain).unwrap();
        let vault_db_path = vault.vault_db_path.clone();
        let wrapped_vec = wrapped.0.to_vec();
        let node_id = "00000000-0000-0000-0000-000000000001";
        tokio::task::spawn_blocking(move || {
            let conn = open_sqlcipher(&vault_db_path, &old_sqlcipher).unwrap();
            conn.execute(
                "INSERT INTO nodes (node_id, parent_id, node_type, name, created_at, modified_at, size_bytes, file_key_wrapped) VALUES (?, NULL, 'file', 'fixture', 0, 0, 0, ?)",
                params![node_id, wrapped_vec],
            )
            .unwrap();
        })
        .await
        .unwrap();

        let request = ChangePasswordRequest {
            current_password_bytes: TEST_PASSWORD,
            new_password_bytes: TEST_NEW_PASSWORD,
            current_key_source: None,
            recovery_phrase: None,
            argon2_params: test_params(),
            vault_db_path: vault.vault_db_path.clone(),
        };
        change_password(
            request,
            &vault.session,
            &vault.cloud,
            &mut vault.header,
            &vault.vault_id,
        )
        .await
        .expect("change_password must succeed");

        let new_salt = decode_base64_32(&vault.header.argon2_salt).unwrap();
        let new_params = argon2_params_from_json(&vault.header.argon2_params);
        let mut new_master: Zeroizing<[u8; 32]> = Zeroizing::new([0u8; 32]);
        derive_master_key_into(
            TEST_NEW_PASSWORD,
            None,
            &new_salt,
            &new_params,
            &mut new_master,
        )
        .unwrap();
        let new_keys = SessionKeys::from_master_key_bytes(&new_master).unwrap();
        let new_sqlcipher: [u8; 32] = *new_keys.sqlcipher_key.expose();

        let vault_db_path = vault.vault_db_path.clone();
        let row_blob: Vec<u8> = tokio::task::spawn_blocking(move || {
            let conn = open_sqlcipher(&vault_db_path, &new_sqlcipher).unwrap();
            conn.query_row(
                "SELECT file_key_wrapped FROM nodes WHERE node_id = ?",
                params!["00000000-0000-0000-0000-000000000001"],
                |row| row.get(0),
            )
            .unwrap()
        })
        .await
        .unwrap();
        let wrapped_array: [u8; 72] = row_blob.try_into().unwrap();
        let wrapped_after = WrappedFileKey(wrapped_array);
        let unwrap_result = unwrap_with_kek_bytes(&wrapped_after, &old_kek);
        assert!(matches!(
            unwrap_result,
            Err(AuthenticationError::InvalidCredentials)
        ));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_change_password_new_kek_can_unwrap_file_keys_after_change() {
        let _lock = ceremony_lock().await;
        let mut vault = create_tier_one_vault().await;

        let current_salt = decode_base64_32(&vault.header.argon2_salt).unwrap();
        let current_params = argon2_params_from_json(&vault.header.argon2_params);
        let mut current_master: Zeroizing<[u8; 32]> = Zeroizing::new([0u8; 32]);
        derive_master_key_into(
            TEST_PASSWORD,
            None,
            &current_salt,
            &current_params,
            &mut current_master,
        )
        .unwrap();
        let current_keys = SessionKeys::from_master_key_bytes(&current_master).unwrap();
        let current_kek: [u8; 32] = *current_keys.key_encryption_key.expose();
        let current_sqlcipher: [u8; 32] = *current_keys.sqlcipher_key.expose();

        let file_key_plain = [0x77u8; 32];
        let wrapped = wrap_with_kek_bytes(&current_kek, &file_key_plain).unwrap();
        let vault_db_path = vault.vault_db_path.clone();
        let wrapped_vec = wrapped.0.to_vec();
        let node_id = "00000000-0000-0000-0000-000000000002";
        tokio::task::spawn_blocking(move || {
            let conn = open_sqlcipher(&vault_db_path, &current_sqlcipher).unwrap();
            conn.execute(
                "INSERT INTO nodes (node_id, parent_id, node_type, name, created_at, modified_at, size_bytes, file_key_wrapped) VALUES (?, NULL, 'file', 'fixture', 0, 0, 0, ?)",
                params![node_id, wrapped_vec],
            )
            .unwrap();
        })
        .await
        .unwrap();

        let request = ChangePasswordRequest {
            current_password_bytes: TEST_PASSWORD,
            new_password_bytes: TEST_NEW_PASSWORD,
            current_key_source: None,
            recovery_phrase: None,
            argon2_params: test_params(),
            vault_db_path: vault.vault_db_path.clone(),
        };
        change_password(
            request,
            &vault.session,
            &vault.cloud,
            &mut vault.header,
            &vault.vault_id,
        )
        .await
        .expect("change_password must succeed");

        let new_salt = decode_base64_32(&vault.header.argon2_salt).unwrap();
        let new_params = argon2_params_from_json(&vault.header.argon2_params);
        let mut new_master: Zeroizing<[u8; 32]> = Zeroizing::new([0u8; 32]);
        derive_master_key_into(
            TEST_NEW_PASSWORD,
            None,
            &new_salt,
            &new_params,
            &mut new_master,
        )
        .unwrap();
        let new_keys = SessionKeys::from_master_key_bytes(&new_master).unwrap();
        let new_kek: [u8; 32] = *new_keys.key_encryption_key.expose();
        let new_sqlcipher: [u8; 32] = *new_keys.sqlcipher_key.expose();

        let vault_db_path = vault.vault_db_path.clone();
        let row_blob: Vec<u8> = tokio::task::spawn_blocking(move || {
            let conn = open_sqlcipher(&vault_db_path, &new_sqlcipher).unwrap();
            conn.query_row(
                "SELECT file_key_wrapped FROM nodes WHERE node_id = ?",
                params!["00000000-0000-0000-0000-000000000002"],
                |row| row.get(0),
            )
            .unwrap()
        })
        .await
        .unwrap();
        let wrapped_array: [u8; 72] = row_blob.try_into().unwrap();
        let recovered = unwrap_with_kek_bytes(&WrappedFileKey(wrapped_array), &new_kek)
            .expect("unwrap with new kek must succeed");
        assert_eq!(*recovered.expose(), file_key_plain);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_change_password_sqlcipher_opens_with_new_key_and_rejects_old_key() {
        let _lock = ceremony_lock().await;
        let mut vault = create_tier_one_vault().await;
        let current_salt = decode_base64_32(&vault.header.argon2_salt).unwrap();
        let current_params = argon2_params_from_json(&vault.header.argon2_params);
        let mut old_master: Zeroizing<[u8; 32]> = Zeroizing::new([0u8; 32]);
        derive_master_key_into(
            TEST_PASSWORD,
            None,
            &current_salt,
            &current_params,
            &mut old_master,
        )
        .unwrap();
        let old_keys = SessionKeys::from_master_key_bytes(&old_master).unwrap();
        let old_sqlcipher: [u8; 32] = *old_keys.sqlcipher_key.expose();

        let request = ChangePasswordRequest {
            current_password_bytes: TEST_PASSWORD,
            new_password_bytes: TEST_NEW_PASSWORD,
            current_key_source: None,
            recovery_phrase: None,
            argon2_params: test_params(),
            vault_db_path: vault.vault_db_path.clone(),
        };
        change_password(
            request,
            &vault.session,
            &vault.cloud,
            &mut vault.header,
            &vault.vault_id,
        )
        .await
        .expect("change_password must succeed");

        let new_salt = decode_base64_32(&vault.header.argon2_salt).unwrap();
        let new_params = argon2_params_from_json(&vault.header.argon2_params);
        let mut new_master: Zeroizing<[u8; 32]> = Zeroizing::new([0u8; 32]);
        derive_master_key_into(
            TEST_NEW_PASSWORD,
            None,
            &new_salt,
            &new_params,
            &mut new_master,
        )
        .unwrap();
        let new_keys = SessionKeys::from_master_key_bytes(&new_master).unwrap();
        let new_sqlcipher: [u8; 32] = *new_keys.sqlcipher_key.expose();

        let vault_db_path_for_new = vault.vault_db_path.clone();
        let opens_with_new = tokio::task::spawn_blocking(move || {
            open_sqlcipher(&vault_db_path_for_new, &new_sqlcipher).is_ok()
        })
        .await
        .unwrap();
        assert!(opens_with_new);

        let vault_db_path_for_old = vault.vault_db_path.clone();
        let opens_with_old = tokio::task::spawn_blocking(move || {
            match open_sqlcipher(&vault_db_path_for_old, &old_sqlcipher) {
                Ok(conn) => conn
                    .query_row("SELECT id FROM vault_identity WHERE id = 1", [], |row| {
                        row.get::<_, i64>(0)
                    })
                    .is_ok(),
                Err(_) => false,
            }
        })
        .await
        .unwrap();
        assert!(!opens_with_old);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_change_password_with_recovery_slot_re_wraps_slot_and_phrase_still_unlocks() {
        let _lock = ceremony_lock().await;
        let mut vault = create_tier_one_vault().await;
        let phrase = add_recovery_slot_and_return_phrase(&mut vault).await;
        let phrase_string: String = phrase.as_str().to_string();

        let request = ChangePasswordRequest {
            current_password_bytes: TEST_PASSWORD,
            new_password_bytes: TEST_NEW_PASSWORD,
            current_key_source: None,
            recovery_phrase: Some(&phrase_string),
            argon2_params: test_params(),
            vault_db_path: vault.vault_db_path.clone(),
        };
        change_password(
            request,
            &vault.session,
            &vault.cloud,
            &mut vault.header,
            &vault.vault_id,
        )
        .await
        .expect("change_password with recovery must succeed");
        assert_eq!(vault.header.recovery_slots.len(), 1);

        let slot = &vault.header.recovery_slots[0];
        let slot_salt = decode_base64_32(&slot.argon2_salt).unwrap();
        let slot_params = argon2_params_from_json(&slot.argon2_params);
        let wrapped = WrappedMasterKey(decode_base64_72(&slot.wrapped_master_key).unwrap());

        let mut recovery_key_bytes: Zeroizing<[u8; 32]> = Zeroizing::new([0u8; 32]);
        derive_recovery_key_into(
            phrase_string.as_bytes(),
            &slot_salt,
            &slot_params,
            &mut recovery_key_bytes,
        )
        .unwrap();
        let recovery_key = RecoveryKey::from_bytes(*recovery_key_bytes);
        let recovered = unwrap_master_key_from_recovery(&wrapped, &recovery_key, &vault.vault_id)
            .expect("unwrap with phrase must succeed after password change");

        let new_salt = decode_base64_32(&vault.header.argon2_salt).unwrap();
        let new_params = argon2_params_from_json(&vault.header.argon2_params);
        let mut new_master: Zeroizing<[u8; 32]> = Zeroizing::new([0u8; 32]);
        derive_master_key_into(
            TEST_NEW_PASSWORD,
            None,
            &new_salt,
            &new_params,
            &mut new_master,
        )
        .unwrap();
        assert_eq!(recovered.expose(), &*new_master);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_change_password_without_recovery_phrase_clears_recovery_slots() {
        let _lock = ceremony_lock().await;
        let mut vault = create_tier_one_vault().await;
        let _phrase = add_recovery_slot_and_return_phrase(&mut vault).await;
        assert_eq!(vault.header.recovery_slots.len(), 1);

        let request = ChangePasswordRequest {
            current_password_bytes: TEST_PASSWORD,
            new_password_bytes: TEST_NEW_PASSWORD,
            current_key_source: None,
            recovery_phrase: None,
            argon2_params: test_params(),
            vault_db_path: vault.vault_db_path.clone(),
        };
        change_password(
            request,
            &vault.session,
            &vault.cloud,
            &mut vault.header,
            &vault.vault_id,
        )
        .await
        .expect("change_password without recovery must succeed");
        assert!(vault.header.recovery_slots.is_empty());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_change_password_failure_inside_rewrap_transaction_rolls_back_to_old_state() {
        let _lock = ceremony_lock().await;
        let mut vault = create_tier_one_vault().await;
        let current_salt = decode_base64_32(&vault.header.argon2_salt).unwrap();
        let current_params = argon2_params_from_json(&vault.header.argon2_params);
        let mut old_master: Zeroizing<[u8; 32]> = Zeroizing::new([0u8; 32]);
        derive_master_key_into(
            TEST_PASSWORD,
            None,
            &current_salt,
            &current_params,
            &mut old_master,
        )
        .unwrap();
        let old_keys = SessionKeys::from_master_key_bytes(&old_master).unwrap();
        let old_sqlcipher: [u8; 32] = *old_keys.sqlcipher_key.expose();

        let bad_wrapped = vec![0u8; 72];
        let vault_db_path = vault.vault_db_path.clone();
        let bad_wrapped_for_insert = bad_wrapped.clone();
        let node_id = "00000000-0000-0000-0000-000000000003";
        tokio::task::spawn_blocking(move || {
            let conn = open_sqlcipher(&vault_db_path, &old_sqlcipher).unwrap();
            conn.execute(
                "INSERT INTO nodes (node_id, parent_id, node_type, name, created_at, modified_at, size_bytes, file_key_wrapped) VALUES (?, NULL, 'file', 'fixture', 0, 0, 0, ?)",
                params![node_id, bad_wrapped_for_insert],
            )
            .unwrap();
        })
        .await
        .unwrap();

        let header_before = vault.header.clone();
        let request = ChangePasswordRequest {
            current_password_bytes: TEST_PASSWORD,
            new_password_bytes: TEST_NEW_PASSWORD,
            current_key_source: None,
            recovery_phrase: None,
            argon2_params: test_params(),
            vault_db_path: vault.vault_db_path.clone(),
        };
        let result = change_password(
            request,
            &vault.session,
            &vault.cloud,
            &mut vault.header,
            &vault.vault_id,
        )
        .await;
        assert!(result.is_err());

        let vault_db_path = vault.vault_db_path.clone();
        let row_blob: Vec<u8> = tokio::task::spawn_blocking(move || {
            let conn = open_sqlcipher(&vault_db_path, &old_sqlcipher).unwrap();
            conn.query_row(
                "SELECT file_key_wrapped FROM nodes WHERE node_id = ?",
                params!["00000000-0000-0000-0000-000000000003"],
                |row| row.get(0),
            )
            .unwrap()
        })
        .await
        .unwrap();
        assert_eq!(row_blob, bad_wrapped);
        assert_eq!(vault.header.argon2_salt, header_before.argon2_salt);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_change_password_upload_failure_keeps_new_local_keys_and_returns_vault_header_invalid()
     {
        let _lock = ceremony_lock().await;
        let mut vault = create_tier_one_vault().await;
        let failing_cloud = UploadFailCloudTransport;
        let old_salt = vault.header.argon2_salt.clone();

        let pending_path = staging::staging_directory()
            .await
            .expect("staging dir must exist")
            .join(STAGING_FILE_NAME);
        let _ = staging::remove_if_exists(&pending_path).await;

        let request = ChangePasswordRequest {
            current_password_bytes: TEST_PASSWORD,
            new_password_bytes: TEST_NEW_PASSWORD,
            current_key_source: None,
            recovery_phrase: None,
            argon2_params: test_params(),
            vault_db_path: vault.vault_db_path.clone(),
        };
        let result = change_password(
            request,
            &vault.session,
            &failing_cloud,
            &mut vault.header,
            &vault.vault_id,
        )
        .await;
        assert!(matches!(
            result,
            Err(AuthenticationError::VaultHeaderInvalid)
        ));
        assert_ne!(vault.header.argon2_salt, old_salt);

        let new_salt = decode_base64_32(&vault.header.argon2_salt).unwrap();
        let new_params = argon2_params_from_json(&vault.header.argon2_params);
        let mut new_master: Zeroizing<[u8; 32]> = Zeroizing::new([0u8; 32]);
        derive_master_key_into(
            TEST_NEW_PASSWORD,
            None,
            &new_salt,
            &new_params,
            &mut new_master,
        )
        .unwrap();
        let new_keys = SessionKeys::from_master_key_bytes(&new_master).unwrap();
        let expected_sqlcipher: [u8; 32] = *new_keys.sqlcipher_key.expose();
        let session_sqlcipher = vault
            .session
            .with_sqlcipher_key(|key| *key)
            .await
            .expect("session must remain active with rotated keys");
        assert_eq!(session_sqlcipher, expected_sqlcipher);

        let pending_exists = tokio::fs::try_exists(&pending_path)
            .await
            .expect("staging probe must succeed");
        assert!(pending_exists);
        let _ = staging::remove_if_exists(&pending_path).await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_change_password_ignores_nodes_row_with_null_node_id_during_rewrap() {
        let _lock = ceremony_lock().await;
        let mut vault = create_tier_one_vault().await;

        let current_salt = decode_base64_32(&vault.header.argon2_salt).unwrap();
        let current_params = argon2_params_from_json(&vault.header.argon2_params);
        let mut current_master: Zeroizing<[u8; 32]> = Zeroizing::new([0u8; 32]);
        derive_master_key_into(
            TEST_PASSWORD,
            None,
            &current_salt,
            &current_params,
            &mut current_master,
        )
        .unwrap();
        let current_keys = SessionKeys::from_master_key_bytes(&current_master).unwrap();
        let current_kek: [u8; 32] = *current_keys.key_encryption_key.expose();
        let current_sqlcipher: [u8; 32] = *current_keys.sqlcipher_key.expose();

        let wrapped = wrap_with_kek_bytes(&current_kek, &[0x33u8; 32]).unwrap();
        let wrapped_vec = wrapped.0.to_vec();
        let vault_db_path = vault.vault_db_path.clone();
        tokio::task::spawn_blocking(move || {
            let conn = open_sqlcipher(&vault_db_path, &current_sqlcipher).unwrap();
            conn.execute(
                "INSERT INTO nodes (node_id, parent_id, node_type, name, created_at, modified_at, size_bytes, file_key_wrapped) VALUES (NULL, NULL, 'file', 'malformed', 0, 0, 0, ?)",
                params![wrapped_vec],
            )
            .unwrap();
        })
        .await
        .unwrap();

        let request = ChangePasswordRequest {
            current_password_bytes: TEST_PASSWORD,
            new_password_bytes: TEST_NEW_PASSWORD,
            current_key_source: None,
            recovery_phrase: None,
            argon2_params: test_params(),
            vault_db_path: vault.vault_db_path.clone(),
        };
        let result = change_password(
            request,
            &vault.session,
            &vault.cloud,
            &mut vault.header,
            &vault.vault_id,
        )
        .await;
        assert!(result.is_ok());
    }
}
