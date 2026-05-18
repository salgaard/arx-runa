//! Scenario tests: auth/recovery use cases (Use Case 3).
//!
//! Each test composes multiple ceremony calls in sequence, covering cross-ceremony
//! flows not captured by individual ceremony unit tests.

use crate::auth::ceremonies::test_support::*;
use crate::auth::{
    Argon2MigrationIntent, AuthenticationError, ChangePasswordRequest, MockKeySource,
    RecoverWithPhraseRequest, RotateKeyFileRequest, change_password, recover_with_phrase,
    rotate_key_file,
};

// ---------------------------------------------------------------------------
// UC3: full recovery via phrase (Tier 1)
// ---------------------------------------------------------------------------

/// Happy path: create vault → add phrase → lock → recover → session active with same vault_id.
#[tokio::test(flavor = "multi_thread")]
async fn test_tier1_full_recovery_via_phrase_restores_active_session() {
    let _lock = ceremony_lock().await;
    let mut vault = create_tier_one_vault().await;
    let phrase = add_recovery_slot_and_return_phrase(&mut vault).await;
    upload_manifest_backup_for(&vault).await;
    vault.session.lock().await;

    let new_session = test_session_manager();
    let new_temp = temp_dir();

    let (recovered_id, _header) = recover_with_phrase(
        RecoverWithPhraseRequest {
            phrase: phrase.as_bytes(),
            vault_db_path: new_temp.path().join("recovered.db"),
            new_password_bytes: TEST_NEW_PASSWORD,
            new_key_file_path: None,
            argon2_params: test_params(),
            argon2_migration_intent: Argon2MigrationIntent::PreserveTrusted,
            vault_header: Some(vault.header.clone()),
        },
        &new_session,
        &vault.cloud,
    )
    .await
    .expect("recover_with_phrase must succeed");

    assert_eq!(recovered_id, vault.vault_id);
    assert_eq!(
        new_session.state().await,
        crate::auth::LifecycleState::Active
    );
}

// ---------------------------------------------------------------------------
// UC3: Tier 2 key rotation invalidates the old key
// ---------------------------------------------------------------------------

/// Tier 2: rotate key file succeeds → second rotation with the stale key bytes is rejected.
#[tokio::test(flavor = "multi_thread")]
async fn test_tier2_key_rotation_old_key_rejected_on_reuse() {
    let _lock = ceremony_lock().await;
    let mut vault = create_tier_two_vault().await;

    let old_key_bytes: [u8; 32] = std::fs::read(&vault.key_file_path)
        .expect("key file must exist")
        .try_into()
        .expect("key file must be 32 bytes");
    let old_source = MockKeySource::new(old_key_bytes);
    let new_key_path = vault._temp.path().join("rotated.bin");

    rotate_key_file(
        RotateKeyFileRequest {
            password_bytes: TEST_PASSWORD,
            current_key_source: &old_source,
            target_new_key_file_path: new_key_path.clone(),
            recovery_phrase: None,
            argon2_params: test_params(),
            argon2_migration_intent: Argon2MigrationIntent::PreserveTrusted,
            vault_db_path: vault.vault_db_path.clone(),
        },
        &vault.session,
        &vault.cloud,
        &mut vault.header,
        &vault.vault_id,
    )
    .await
    .expect("first rotate_key_file must succeed");

    let second_new_key_path = vault._temp.path().join("rotated2.bin");
    let result = rotate_key_file(
        RotateKeyFileRequest {
            password_bytes: TEST_PASSWORD,
            current_key_source: &old_source,
            target_new_key_file_path: second_new_key_path.clone(),
            recovery_phrase: None,
            argon2_params: test_params(),
            argon2_migration_intent: Argon2MigrationIntent::PreserveTrusted,
            vault_db_path: vault.vault_db_path.clone(),
        },
        &vault.session,
        &vault.cloud,
        &mut vault.header,
        &vault.vault_id,
    )
    .await;

    assert!(
        matches!(result, Err(AuthenticationError::InvalidCredentials)),
        "stale key must be rejected after rotation, got: {result:?}"
    );
    assert!(
        !second_new_key_path.exists(),
        "no key file must be written when rotation fails"
    );
}

// ---------------------------------------------------------------------------
// UC3: change password with / without phrase — recovery slot behaviour
// ---------------------------------------------------------------------------

/// Password change supplying phrase → recovery slot count stays at 1 in the updated header.
#[tokio::test(flavor = "multi_thread")]
async fn test_change_password_with_phrase_recovery_slot_preserved_in_header() {
    let _lock = ceremony_lock().await;
    let mut vault = create_tier_one_vault().await;
    let phrase = add_recovery_slot_and_return_phrase(&mut vault).await;

    change_password(
        ChangePasswordRequest {
            current_password_bytes: TEST_PASSWORD,
            new_password_bytes: TEST_NEW_PASSWORD,
            current_key_source: None,
            recovery_phrase: Some(phrase.as_bytes()),
            argon2_params: test_params(),
            argon2_migration_intent: Argon2MigrationIntent::PreserveTrusted,
            vault_db_path: vault.vault_db_path.clone(),
        },
        &vault.session,
        &vault.cloud,
        &mut vault.header,
        &vault.vault_id,
    )
    .await
    .expect("change_password with phrase must succeed");

    assert_eq!(
        vault.header.recovery_slots.len(),
        1,
        "recovery slot must be re-wrapped and preserved after password change"
    );
}

/// Password change without phrase → recovery slot is cleared from the header.
#[tokio::test(flavor = "multi_thread")]
async fn test_change_password_without_phrase_recovery_slot_cleared() {
    let _lock = ceremony_lock().await;
    let mut vault = create_tier_one_vault().await;
    add_recovery_slot_and_return_phrase(&mut vault).await;

    change_password(
        ChangePasswordRequest {
            current_password_bytes: TEST_PASSWORD,
            new_password_bytes: TEST_NEW_PASSWORD,
            current_key_source: None,
            recovery_phrase: None,
            argon2_params: test_params(),
            argon2_migration_intent: Argon2MigrationIntent::PreserveTrusted,
            vault_db_path: vault.vault_db_path.clone(),
        },
        &vault.session,
        &vault.cloud,
        &mut vault.header,
        &vault.vault_id,
    )
    .await
    .expect("change_password without phrase must succeed");

    assert_eq!(
        vault.header.recovery_slots.len(),
        0,
        "recovery slot must be removed when no phrase is supplied at password change"
    );
}

// ---------------------------------------------------------------------------
// UC3: invalid phrase rejected before Argon2
// ---------------------------------------------------------------------------

/// A phrase containing a non-BIP39 word fails checksum validation before any Argon2 derivation.
#[tokio::test(flavor = "multi_thread")]
async fn test_recovery_phrase_non_bip39_word_rejected_as_invalid_recovery_phrase() {
    let _lock = ceremony_lock().await;
    let mut vault = create_tier_one_vault().await;
    add_recovery_slot_and_return_phrase(&mut vault).await;
    upload_manifest_backup_for(&vault).await;
    vault.session.lock().await;

    // "xyzzy" is not a BIP-39 English word, so Mnemonic::parse_in returns Err
    // and the ceremony returns InvalidRecoveryPhrase without attempting Argon2.
    let bad_phrase = "xyzzy abandon abandon abandon abandon abandon abandon \
                      abandon abandon abandon abandon abandon abandon abandon \
                      abandon abandon abandon abandon abandon abandon abandon \
                      abandon abandon abandon";
    let new_session = test_session_manager();
    let new_temp = temp_dir();

    let result = recover_with_phrase(
        RecoverWithPhraseRequest {
            phrase: bad_phrase.as_bytes(),
            vault_db_path: new_temp.path().join("bad-phrase.db"),
            new_password_bytes: TEST_NEW_PASSWORD,
            new_key_file_path: None,
            argon2_params: test_params(),
            argon2_migration_intent: Argon2MigrationIntent::PreserveTrusted,
            vault_header: Some(vault.header.clone()),
        },
        &new_session,
        &vault.cloud,
    )
    .await;

    assert!(
        matches!(result, Err(AuthenticationError::InvalidRecoveryPhrase)),
        "phrase with non-BIP39 word must be rejected as InvalidRecoveryPhrase, got: {result:?}"
    );
}

// ---------------------------------------------------------------------------
// UC3: full recovery via phrase (Tier 2)
// ---------------------------------------------------------------------------

/// Tier 2 recovery: create vault with key file → add phrase → lock → recover with phrase →
/// new key file written, session active, vault ID preserved.
#[tokio::test(flavor = "multi_thread")]
async fn test_tier2_full_recovery_via_phrase_creates_new_key_file_and_restores_session() {
    let _lock = ceremony_lock().await;
    let mut vault = create_tier_two_vault().await;
    let phrase = add_recovery_slot_and_return_phrase_tier_two(&mut vault).await;
    upload_manifest_backup_for_tier_two(&vault).await;
    vault.session.lock().await;

    let new_session = test_session_manager();
    let new_temp = temp_dir();
    let new_vault_db_path = new_temp.path().join("recovered.db");
    let new_key_file_path = new_temp.path().join("recovered-key.bin");

    let (recovered_id, _header) = recover_with_phrase(
        RecoverWithPhraseRequest {
            phrase: phrase.as_bytes(),
            vault_db_path: new_vault_db_path,
            new_password_bytes: TEST_NEW_PASSWORD,
            new_key_file_path: Some(new_key_file_path.clone()),
            argon2_params: test_params(),
            argon2_migration_intent: Argon2MigrationIntent::PreserveTrusted,
            vault_header: Some(vault.header.clone()),
        },
        &new_session,
        &vault.cloud,
    )
    .await
    .expect("tier 2 recover_with_phrase must succeed");

    assert_eq!(
        recovered_id, vault.vault_id,
        "recovered vault ID must match original"
    );
    assert!(
        new_key_file_path.exists(),
        "new key file must be written during tier 2 recovery"
    );
    assert_eq!(
        std::fs::read(&new_key_file_path)
            .expect("new key file must be readable")
            .len(),
        32,
        "new key file must be exactly 32 bytes"
    );
    assert_eq!(
        new_session.state().await,
        crate::auth::LifecycleState::Active,
        "session must be active after successful tier 2 recovery"
    );
}
