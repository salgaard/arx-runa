use rand::Rng;
use rusqlite::{OptionalExtension, params};
use zeroize::Zeroizing;

use super::helpers::*;
use super::types::RotateKeyFileRequest;
use super::{STAGING_FILE_NAME, vault_header_blob_name};
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

/// Rotates the Tier 2 USB key file, re-wrapping all stored keys under a new
/// master key derived from the current password + new key file.
///
/// Only permitted for Tier 2 vaults; Tier 1 returns
/// [`AuthenticationError::VaultHeaderInvalid`].
pub async fn rotate_key_file(
    request: RotateKeyFileRequest<'_>,
    session_manager: &SessionManager,
    cloud_transport: &dyn CloudTransport,
    vault_header: &mut VaultHeader,
    vault_id: &VaultId,
) -> Result<(), AuthenticationError> {
    enforce_argon2_policy(&request.argon2_params)?;

    if vault_header.tier != 2 {
        return Err(AuthenticationError::VaultHeaderInvalid);
    }
    if session_manager.state().await != crate::auth::LifecycleState::Active {
        return Err(AuthenticationError::SessionNotActive);
    }

    let current_salt = decode_base64_32(&vault_header.argon2_salt)?;
    let current_params = argon2_params_from_json(&vault_header.argon2_params);

    let current_key_file: Zeroizing<[u8; 32]> = {
        let bytes = request
            .current_key_source
            .read_key()
            .map_err(|_| AuthenticationError::InvalidCredentials)?;
        let mut buffer: Zeroizing<[u8; 32]> = Zeroizing::new([0u8; 32]);
        buffer.copy_from_slice(bytes.as_slice());
        buffer
    };

    let mut current_master_key: Zeroizing<[u8; 32]> = Zeroizing::new([0u8; 32]);
    derive_master_key_into(
        request.password_bytes,
        Some(&current_key_file),
        &current_salt,
        &current_params,
        &mut current_master_key,
    )?;
    let current_session_keys = SessionKeys::from_master_key_bytes(&current_master_key)?;
    let current_kek =
        key_encryption_key_from_array(current_session_keys.key_encryption_key.expose());
    let current_sqlcipher = sqlcipher_key_from_array(current_session_keys.sqlcipher_key.expose());

    let parent = request
        .target_new_key_file_path
        .parent()
        .ok_or(AuthenticationError::VaultHeaderInvalid)?;
    if !tokio::fs::try_exists(parent)
        .await
        .map_err(|_| AuthenticationError::VaultHeaderInvalid)?
    {
        return Err(AuthenticationError::VaultHeaderInvalid);
    }
    let mut new_key_file: Zeroizing<[u8; 32]> = Zeroizing::new([0u8; 32]);
    rand::rng().fill_bytes(new_key_file.as_mut_slice());
    staging::write_owner_only(&request.target_new_key_file_path, new_key_file.as_slice()).await?;
    let new_key_file_blake3 = hex::encode(blake3::hash(new_key_file.as_slice()).as_bytes());

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
        request.password_bytes,
        Some(&new_key_file),
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
                        .prepare("SELECT id, file_key_wrapped FROM nodes")
                        .map_err(|_| AuthenticationError::InvalidCredentials)?;
                    let rows: Vec<(i64, Vec<u8>)> = stmt
                        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
                        .map_err(|_| AuthenticationError::InvalidCredentials)?
                        .collect::<Result<_, _>>()
                        .map_err(|_| AuthenticationError::InvalidCredentials)?;
                    for (row_id, wrapped_blob) in rows {
                        let wrapped_array: [u8; 72] = wrapped_blob
                            .try_into()
                            .map_err(|_| AuthenticationError::InvalidCredentials)?;
                        let wrapped = WrappedFileKey(wrapped_array);
                        let file_key = unwrap_file_key(&wrapped, &current_kek)
                            .map_err(|_| AuthenticationError::InvalidCredentials)?;
                        let rewrapped = wrap_file_key(&file_key, &new_kek)
                            .map_err(|_| AuthenticationError::VaultHeaderInvalid)?;
                        conn.execute(
                            "UPDATE nodes SET file_key_wrapped = ? WHERE id = ?",
                            params![rewrapped.0.to_vec(), row_id],
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
    if let Err(error) = rewrap_result {
        remove_file_if_exists(&request.target_new_key_file_path).await;
        return Err(error);
    }

    vault_header.argon2_salt = encode_base64(new_salt.as_slice());
    vault_header.argon2_params = argon2_params_to_json(&request.argon2_params);
    vault_header.key_file_blake3 = Some(new_key_file_blake3);
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
    let upload_result = cloud_transport
        .upload_blob(&vault_header_blob_name(), &json_bytes)
        .await;
    if upload_result.is_err() {
        return Err(AuthenticationError::VaultHeaderInvalid);
    }
    best_effort_cleanup_staging(&staging_path).await;

    session_manager
        .swap_active_session(new_session_keys)
        .await?;

    drop(current_master_key);
    drop(new_master_key);
    drop(new_salt);
    drop(current_key_file);
    drop(new_key_file);
    Ok(())
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
    async fn test_rotate_key_file_preserves_x25519_public_key_bytes() {
        let _lock = ceremony_lock().await;
        let mut vault = create_tier_two_vault().await;
        let old_key_source = MockKeySource::new(
            std::fs::read(&vault.key_file_path)
                .expect("key file must exist")
                .try_into()
                .expect("key file must be 32 bytes"),
        );

        let old_salt = decode_base64_32(&vault.header.argon2_salt).unwrap();
        let old_params = argon2_params_from_json(&vault.header.argon2_params);
        let mut old_master: Zeroizing<[u8; 32]> = Zeroizing::new([0u8; 32]);
        let old_key_file = std::fs::read(&vault.key_file_path).unwrap();
        let old_key_file_arr: [u8; 32] = old_key_file.as_slice().try_into().unwrap();
        let old_key_file_z: Zeroizing<[u8; 32]> = Zeroizing::new(old_key_file_arr);
        derive_master_key_into(
            TEST_PASSWORD,
            Some(&old_key_file_z),
            &old_salt,
            &old_params,
            &mut old_master,
        )
        .unwrap();
        let old_keys = SessionKeys::from_master_key_bytes(&old_master).unwrap();
        let old_sqlcipher: [u8; 32] = *old_keys.sqlcipher_key.expose();

        let vault_db_path = vault.vault_db_path.clone();
        let old_public_key: Vec<u8> = tokio::task::spawn_blocking(move || {
            let conn = open_sqlcipher(&vault_db_path, &old_sqlcipher).unwrap();
            conn.query_row(
                "SELECT public_key FROM vault_identity WHERE id = 1",
                [],
                |row| row.get(0),
            )
            .unwrap()
        })
        .await
        .unwrap();

        let new_key_file_path = vault._temp.path().join("new-key.bin");
        let request = RotateKeyFileRequest {
            password_bytes: TEST_PASSWORD,
            current_key_source: &old_key_source,
            target_new_key_file_path: new_key_file_path.clone(),
            recovery_phrase: None,
            argon2_params: test_params(),
            vault_db_path: vault.vault_db_path.clone(),
        };
        rotate_key_file(
            request,
            &vault.session,
            &vault.cloud,
            &mut vault.header,
            &vault.vault_id,
        )
        .await
        .expect("rotate_key_file must succeed");

        let new_salt = decode_base64_32(&vault.header.argon2_salt).unwrap();
        let new_params = argon2_params_from_json(&vault.header.argon2_params);
        let new_key_file = std::fs::read(&new_key_file_path).unwrap();
        let new_key_file_arr: [u8; 32] = new_key_file.as_slice().try_into().unwrap();
        let new_key_file_z: Zeroizing<[u8; 32]> = Zeroizing::new(new_key_file_arr);
        let mut new_master: Zeroizing<[u8; 32]> = Zeroizing::new([0u8; 32]);
        derive_master_key_into(
            TEST_PASSWORD,
            Some(&new_key_file_z),
            &new_salt,
            &new_params,
            &mut new_master,
        )
        .unwrap();
        let new_keys = SessionKeys::from_master_key_bytes(&new_master).unwrap();
        let new_sqlcipher: [u8; 32] = *new_keys.sqlcipher_key.expose();

        let vault_db_path = vault.vault_db_path.clone();
        let new_public_key: Vec<u8> = tokio::task::spawn_blocking(move || {
            let conn = open_sqlcipher(&vault_db_path, &new_sqlcipher).unwrap();
            conn.query_row(
                "SELECT public_key FROM vault_identity WHERE id = 1",
                [],
                |row| row.get(0),
            )
            .unwrap()
        })
        .await
        .unwrap();
        assert_eq!(old_public_key, new_public_key);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_rotate_key_file_updates_key_file_blake3_in_header() {
        let _lock = ceremony_lock().await;
        let mut vault = create_tier_two_vault().await;
        let old_blake3 = vault.header.key_file_blake3.clone();
        let old_bytes: [u8; 32] = std::fs::read(&vault.key_file_path)
            .unwrap()
            .try_into()
            .unwrap();
        let old_source = MockKeySource::new(old_bytes);
        let new_key_file_path = vault._temp.path().join("rotated.bin");
        let request = RotateKeyFileRequest {
            password_bytes: TEST_PASSWORD,
            current_key_source: &old_source,
            target_new_key_file_path: new_key_file_path.clone(),
            recovery_phrase: None,
            argon2_params: test_params(),
            vault_db_path: vault.vault_db_path.clone(),
        };
        rotate_key_file(
            request,
            &vault.session,
            &vault.cloud,
            &mut vault.header,
            &vault.vault_id,
        )
        .await
        .expect("rotate must succeed");
        let new_bytes = std::fs::read(&new_key_file_path).unwrap();
        let expected = hex::encode(blake3::hash(&new_bytes).as_bytes());
        assert_eq!(
            vault.header.key_file_blake3.as_deref(),
            Some(expected.as_str())
        );
        assert_ne!(vault.header.key_file_blake3, old_blake3);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_rotate_key_file_with_recovery_slot_re_wraps_slot_and_phrase_still_unlocks() {
        let _lock = ceremony_lock().await;
        let mut vault = create_tier_two_vault().await;

        let setup_request = SetupRecoveryRequest {
            current_password_bytes: TEST_PASSWORD,
            current_key_source: Some(&MockKeySource::new(
                std::fs::read(&vault.key_file_path)
                    .unwrap()
                    .try_into()
                    .unwrap(),
            )),
            argon2_params: test_params(),
            vault_db_path: vault.vault_db_path.clone(),
        };
        let phrase = setup_recovery(
            setup_request,
            &vault.session,
            &vault.cloud,
            &mut vault.header,
            &vault.vault_id,
        )
        .await
        .expect("setup_recovery must succeed");
        let phrase_string = phrase.as_str().to_string();

        let old_bytes: [u8; 32] = std::fs::read(&vault.key_file_path)
            .unwrap()
            .try_into()
            .unwrap();
        let old_source = MockKeySource::new(old_bytes);
        let new_path = vault._temp.path().join("rotated.bin");
        let request = RotateKeyFileRequest {
            password_bytes: TEST_PASSWORD,
            current_key_source: &old_source,
            target_new_key_file_path: new_path,
            recovery_phrase: Some(&phrase_string),
            argon2_params: test_params(),
            vault_db_path: vault.vault_db_path.clone(),
        };
        rotate_key_file(
            request,
            &vault.session,
            &vault.cloud,
            &mut vault.header,
            &vault.vault_id,
        )
        .await
        .expect("rotate must succeed");
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
        let _recovered = unwrap_master_key_from_recovery(&wrapped, &recovery_key, &vault.vault_id)
            .expect("phrase must unlock new master key after rotate");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_rotate_key_file_rejects_tier_one_vault() {
        let _lock = ceremony_lock().await;
        let mut vault = create_tier_one_vault().await;
        let source = MockKeySource::new([0xAAu8; 32]);
        let new_path = vault._temp.path().join("rotate.bin");
        let request = RotateKeyFileRequest {
            password_bytes: TEST_PASSWORD,
            current_key_source: &source,
            target_new_key_file_path: new_path,
            recovery_phrase: None,
            argon2_params: test_params(),
            vault_db_path: vault.vault_db_path.clone(),
        };
        let result = rotate_key_file(
            request,
            &vault.session,
            &vault.cloud,
            &mut vault.header,
            &vault.vault_id,
        )
        .await;
        assert!(matches!(
            result,
            Err(AuthenticationError::VaultHeaderInvalid)
        ));
    }
}
