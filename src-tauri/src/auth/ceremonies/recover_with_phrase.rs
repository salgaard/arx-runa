use uuid::Uuid;
use zeroize::Zeroizing;

use super::helpers::*;
use super::types::RecoverWithPhraseRequest;
use super::{manifest_backup_blob_name, vault_header_blob_name};
use crate::auth::error::AuthenticationError;
use crate::auth::session::{SessionKeys, SessionManager};
use crate::crypto::{VaultId, WrappedMasterKey, unwrap_master_key_from_recovery};
use crate::storage::cloud::CloudTransport;
use crate::storage::cloud::manifest_backup::decrypt_manifest_backup;
use crate::storage::cloud::vault_header::VaultHeader;

/// Unlocks a vault using a BIP-39 recovery phrase, downloading the vault
/// header and manifest backup and installing the recovered session.
pub async fn recover_with_phrase(
    request: RecoverWithPhraseRequest<'_>,
    session_manager: &SessionManager,
    cloud_transport: &dyn CloudTransport,
) -> Result<VaultId, AuthenticationError> {
    let mnemonic = parse_mnemonic(request.phrase)?;
    let canonical = canonicalize_phrase(&mnemonic);

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

    if header.recovery_slots.is_empty() {
        return Err(AuthenticationError::NoRecoverySlot);
    }

    let mut recovered_master_key: Option<Zeroizing<[u8; 32]>> = None;
    for slot in header.recovery_slots.iter() {
        if slot.method != "bip39" {
            continue;
        }
        let slot_salt = decode_base64_32(&slot.argon2_salt)?;
        let slot_params = argon2_params_from_json(&slot.argon2_params);
        enforce_argon2_policy(&slot_params)?;
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
        match unwrap_master_key_from_recovery(&wrapped, &recovery_key, &vault_id) {
            Ok(master_key_typed) => {
                let mut bytes: Zeroizing<[u8; 32]> = Zeroizing::new([0u8; 32]);
                bytes.copy_from_slice(master_key_typed.expose());
                drop(master_key_typed);
                drop(recovery_key);
                recovered_master_key = Some(bytes);
                break;
            }
            Err(_) => {
                drop(recovery_key);
            }
        }
    }

    let master_key = recovered_master_key.ok_or(AuthenticationError::InvalidCredentials)?;
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
    async fn test_recover_with_phrase_correct_phrase_unlocks_vault_and_begins_session() {
        let _lock = ceremony_lock().await;
        let mut vault = create_tier_one_vault().await;
        let phrase = add_recovery_slot_and_return_phrase(&mut vault).await;
        let phrase_string = phrase.as_str().to_string();
        upload_manifest_backup_for(&vault).await;
        vault.session.lock().await;

        let new_session = test_session_manager();
        let new_temp = temp_dir();
        let new_db_path = new_temp.path().join("rp.db");
        let request = RecoverWithPhraseRequest {
            phrase: &phrase_string,
            vault_db_path: new_db_path.clone(),
        };
        let recovered_id = recover_with_phrase(request, &new_session, &vault.cloud)
            .await
            .expect("recover_with_phrase must succeed");
        assert_eq!(recovered_id, vault.vault_id);
        assert_eq!(
            new_session.state().await,
            crate::auth::LifecycleState::Active
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_recover_with_phrase_wrong_phrase_returns_invalid_credentials() {
        let _lock = ceremony_lock().await;
        let mut vault = create_tier_one_vault().await;
        let _phrase = add_recovery_slot_and_return_phrase(&mut vault).await;
        upload_manifest_backup_for(&vault).await;
        vault.session.lock().await;

        let wrong_phrase = Mnemonic::from_entropy_in(Language::English, &[0x11u8; 32])
            .unwrap()
            .words()
            .collect::<Vec<_>>()
            .join(" ");
        let new_session = test_session_manager();
        let new_temp = temp_dir();
        let request = RecoverWithPhraseRequest {
            phrase: &wrong_phrase,
            vault_db_path: new_temp.path().join("rp.db"),
        };
        let result = recover_with_phrase(request, &new_session, &vault.cloud).await;
        assert!(matches!(
            result,
            Err(AuthenticationError::InvalidCredentials)
        ));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_recover_with_phrase_invalid_checksum_returns_invalid_recovery_phrase_without_running_argon2id()
     {
        let _lock = ceremony_lock().await;
        let vault = create_tier_one_vault().await;
        let bad_phrase = "abandon ".repeat(23) + "abandon";
        let new_session = test_session_manager();
        let new_temp = temp_dir();
        let request = RecoverWithPhraseRequest {
            phrase: &bad_phrase,
            vault_db_path: new_temp.path().join("rp.db"),
        };
        let result = recover_with_phrase(request, &new_session, &vault.cloud).await;
        assert!(matches!(
            result,
            Err(AuthenticationError::InvalidRecoveryPhrase)
        ));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_recover_with_phrase_empty_recovery_slots_returns_no_recovery_slot() {
        let _lock = ceremony_lock().await;
        let vault = create_tier_one_vault().await;
        let valid_phrase = Mnemonic::from_entropy_in(Language::English, &[0x22u8; 32])
            .unwrap()
            .words()
            .collect::<Vec<_>>()
            .join(" ");
        let new_session = test_session_manager();
        let new_temp = temp_dir();
        let request = RecoverWithPhraseRequest {
            phrase: &valid_phrase,
            vault_db_path: new_temp.path().join("rp.db"),
        };
        let result = recover_with_phrase(request, &new_session, &vault.cloud).await;
        assert!(matches!(result, Err(AuthenticationError::NoRecoverySlot)));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_recover_with_phrase_canonicalises_whitespace_and_case_before_deriving() {
        let _lock = ceremony_lock().await;
        let mut vault = create_tier_one_vault().await;
        let phrase = add_recovery_slot_and_return_phrase(&mut vault).await;
        let phrase_with_extra_whitespace = phrase
            .as_str()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join("   ");
        upload_manifest_backup_for(&vault).await;
        vault.session.lock().await;

        let new_session = test_session_manager();
        let new_temp = temp_dir();
        let request = RecoverWithPhraseRequest {
            phrase: &phrase_with_extra_whitespace,
            vault_db_path: new_temp.path().join("rp.db"),
        };
        let result = recover_with_phrase(request, &new_session, &vault.cloud).await;
        assert!(result.is_ok());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_recovery_slot_cross_vault_transplant_fails() {
        let _lock = ceremony_lock().await;
        let mut vault_a = create_tier_one_vault().await;
        let _phrase_a = add_recovery_slot_and_return_phrase(&mut vault_a).await;
        let phrase_string = _phrase_a.as_str().to_string();
        let slot_a = vault_a.header.recovery_slots[0].clone();

        vault_a.session.lock().await;
        let temp_b = temp_dir();
        let vault_db_path_b = temp_b.path().join("vault_b.db");
        let cloud_b = MockCloudTransport::new();
        let session_b = test_session_manager();
        let request = CreateVaultRequest {
            tier: Tier::One,
            password_bytes: TEST_PASSWORD,
            target_key_file_path: None,
            vault_db_path: vault_db_path_b.clone(),
            argon2_params: test_params(),
        };
        let _vault_b_id = create_vault(request, &session_b, &cloud_b)
            .await
            .expect("create_vault b must succeed");
        let header_bytes_b = cloud_b
            .download_blob(&vault_header_blob_name())
            .await
            .unwrap();
        let mut header_b: VaultHeader = serde_json::from_slice(&header_bytes_b).unwrap();
        header_b.recovery_slots.push(slot_a);
        let updated_bytes = serde_json::to_vec_pretty(&header_b).unwrap();
        cloud_b
            .upload_blob(&vault_header_blob_name(), &updated_bytes)
            .await
            .unwrap();
        session_b.lock().await;

        let new_session = test_session_manager();
        let new_temp = temp_dir();
        let request = RecoverWithPhraseRequest {
            phrase: &phrase_string,
            vault_db_path: new_temp.path().join("cross.db"),
        };
        let result = recover_with_phrase(request, &new_session, &cloud_b).await;
        assert!(matches!(
            result,
            Err(AuthenticationError::InvalidCredentials)
        ));
    }
}
