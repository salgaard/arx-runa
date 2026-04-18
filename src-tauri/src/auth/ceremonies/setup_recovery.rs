use bip39::{Language, Mnemonic};
use rand::Rng;
use zeroize::Zeroizing;

use super::helpers::*;
use super::types::SetupRecoveryRequest;
use super::{STAGING_FILE_NAME, VAULT_HEADER_BLOB_NAME};
use crate::auth::error::AuthenticationError;
use crate::auth::kdf::derive_master_key_into;
use crate::auth::session::{SessionKeys, SessionManager};
use crate::auth::staging;
use crate::crypto::{VaultId, wrap_master_key_for_recovery};
use crate::storage::cloud::CloudTransport;
use crate::storage::cloud::vault_header::{RecoverySlot, VaultHeader};

/// Adds a BIP-39 recovery slot to the vault header, returning the freshly
/// generated 24-word recovery phrase exactly once.
///
/// The caller **must** display the phrase, require user acknowledgement,
/// then drop the `Zeroizing<String>`. The phrase is never persisted, never
/// logged, and never returned again.
pub async fn setup_recovery(
    request: SetupRecoveryRequest<'_>,
    session_manager: &SessionManager,
    cloud_transport: &dyn CloudTransport,
    vault_header: &mut VaultHeader,
    vault_id: &VaultId,
) -> Result<Zeroizing<String>, AuthenticationError> {
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

    let mut master_key: Zeroizing<[u8; 32]> = Zeroizing::new([0u8; 32]);
    derive_master_key_into(
        request.current_password_bytes,
        current_key_file_bytes.as_deref(),
        &current_salt,
        &current_params,
        &mut master_key,
    )?;
    let fresh_session_keys = SessionKeys::from_master_key_bytes(&master_key)?;
    let verify_sqlcipher_key = sqlcipher_key_from_array(fresh_session_keys.sqlcipher_key.expose());
    let verify_kek = key_encryption_key_from_array(fresh_session_keys.key_encryption_key.expose());
    verify_credentials_via_identity_row(&request.vault_db_path, verify_sqlcipher_key, verify_kek)
        .await?;
    drop(fresh_session_keys);

    let mut entropy: Zeroizing<[u8; 32]> = Zeroizing::new([0u8; 32]);
    rand::rng().fill_bytes(entropy.as_mut_slice());
    let mnemonic = Mnemonic::from_entropy_in(Language::English, entropy.as_slice())
        .map_err(|_| AuthenticationError::VaultHeaderInvalid)?;
    let phrase_string = canonicalize_phrase(&mnemonic);
    drop(entropy);

    let mut recovery_salt: Zeroizing<[u8; 32]> = Zeroizing::new([0u8; 32]);
    rand::rng().fill_bytes(recovery_salt.as_mut_slice());
    let mut recovery_key_bytes: Zeroizing<[u8; 32]> = Zeroizing::new([0u8; 32]);
    derive_recovery_key_into(
        phrase_string.as_bytes(),
        &recovery_salt,
        &request.argon2_params,
        &mut recovery_key_bytes,
    )?;
    let recovery_key = recovery_key_from_array(&recovery_key_bytes);
    drop(recovery_key_bytes);

    let master_key_typed = master_key_from_array(&master_key);
    let wrapped = wrap_master_key_for_recovery(&master_key_typed, &recovery_key, vault_id)
        .map_err(|_| AuthenticationError::VaultHeaderInvalid)?;
    drop(master_key_typed);
    drop(recovery_key);

    let slot = RecoverySlot {
        method: "bip39".into(),
        argon2_salt: encode_base64(recovery_salt.as_slice()),
        argon2_params: argon2_params_to_json(&request.argon2_params),
        wrapped_master_key: encode_base64(&wrapped.0),
    };
    vault_header.recovery_slots.push(slot);

    let json_bytes = match serde_json::to_vec_pretty(&vault_header) {
        Ok(bytes) => bytes,
        Err(_) => {
            vault_header.recovery_slots.pop();
            return Err(AuthenticationError::VaultHeaderInvalid);
        }
    };
    let staging_dir = staging::staging_directory().await?;
    let staging_path = staging_dir.join(STAGING_FILE_NAME);
    if let Err(error) = staging::write_owner_only(&staging_path, &json_bytes).await {
        vault_header.recovery_slots.pop();
        return Err(error);
    }
    let upload_result = cloud_transport
        .upload_blob(&staging_path, VAULT_HEADER_BLOB_NAME)
        .await;
    let cleanup_result = staging::remove_if_exists(&staging_path).await;
    if upload_result.is_err() {
        vault_header.recovery_slots.pop();
        if let Err(cleanup_error) = cleanup_result {
            tracing::warn!(
                ?cleanup_error,
                "staging cleanup failed after upload failure"
            );
        }
        return Err(AuthenticationError::VaultHeaderInvalid);
    }
    if let Err(cleanup_error) = cleanup_result {
        tracing::warn!(
            ?cleanup_error,
            "staging cleanup failed after successful upload"
        );
    }

    drop(master_key);
    drop(recovery_salt);
    drop(current_key_file_bytes);
    Ok(phrase_string)
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
    async fn test_setup_recovery_adds_bip39_slot_to_vault_header() {
        let _lock = ceremony_lock().await;
        let mut vault = create_tier_one_vault().await;
        let _phrase = add_recovery_slot_and_return_phrase(&mut vault).await;
        assert_eq!(vault.header.recovery_slots.len(), 1);
        assert_eq!(vault.header.recovery_slots[0].method, "bip39");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_setup_recovery_wrapped_master_key_decodes_to_seventy_two_bytes() {
        let _lock = ceremony_lock().await;
        let mut vault = create_tier_one_vault().await;
        let _phrase = add_recovery_slot_and_return_phrase(&mut vault).await;
        let slot = &vault.header.recovery_slots[0];
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(&slot.wrapped_master_key)
            .unwrap();
        assert_eq!(decoded.len(), 72);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_setup_recovery_returns_phrase_only_once_in_zeroizing_string() {
        let _lock = ceremony_lock().await;
        let mut vault = create_tier_one_vault().await;
        let phrase = add_recovery_slot_and_return_phrase(&mut vault).await;
        let word_count = phrase.as_str().split_whitespace().count();
        assert_eq!(word_count, 24);
        let parsed = Mnemonic::parse_in(Language::English, phrase.as_str())
            .expect("phrase must be valid BIP-39");
        assert_eq!(parsed.words().count(), 24);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_setup_recovery_rejects_wrong_current_credentials_via_identity_unwrap() {
        let _lock = ceremony_lock().await;
        let mut vault = create_tier_one_vault().await;
        let request = SetupRecoveryRequest {
            current_password_bytes: TEST_WRONG_PASSWORD,
            current_key_source: None,
            argon2_params: test_params(),
            vault_db_path: vault.vault_db_path.clone(),
        };
        let result = setup_recovery(
            request,
            &vault.session,
            &vault.cloud,
            &mut vault.header,
            &vault.vault_id,
        )
        .await;
        assert!(matches!(
            result,
            Err(AuthenticationError::InvalidCredentials)
        ));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_recovery_phrase_never_appears_in_any_persistent_writer_output() {
        let _lock = ceremony_lock().await;
        let mut vault = create_tier_one_vault().await;
        let phrase = add_recovery_slot_and_return_phrase(&mut vault).await;
        let phrase_bytes = phrase.as_bytes().to_vec();

        vault
            .cloud
            .download_blob(
                VAULT_HEADER_BLOB_NAME,
                &vault._temp.path().join("setup-recovery-header.json"),
            )
            .await
            .expect("header must be present");
        let header_bytes =
            std::fs::read(vault._temp.path().join("setup-recovery-header.json")).unwrap();
        assert!(
            !header_bytes
                .windows(phrase_bytes.len())
                .any(|w| w == phrase_bytes.as_slice()),
            "recovery phrase must not appear in vault-header.json"
        );
        let db_bytes = std::fs::read(&vault.vault_db_path).expect("db file must exist");
        assert!(
            !db_bytes
                .windows(phrase_bytes.len())
                .any(|w| w == phrase_bytes.as_slice()),
            "recovery phrase must not appear in vault db"
        );
    }
}
