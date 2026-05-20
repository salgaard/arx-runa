//! Scenario tests: auth/recovery use cases (Use Case 3).
//!
//! Each test composes multiple ceremony calls in sequence, covering cross-ceremony
//! flows not captured by individual ceremony unit tests.

use uuid::Uuid;

use crate::auth::ceremonies::test_support::*;
use crate::auth::{
    Argon2MigrationIntent, AuthenticationError, ChangePasswordRequest, MockKeySource,
    RecoverWithPhraseRequest, RotateKeyFileRequest, change_password, recover_with_phrase,
    rotate_key_file,
};
use crate::crypto::KeyEncryptionKey;
use crate::storage::{SqlCipherMetadataStore, download_file_to_memory, upload_file};

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
            argon2_params: test_parameters(),
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
            argon2_params: test_parameters(),
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
            argon2_params: test_parameters(),
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
            argon2_params: test_parameters(),
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
            argon2_params: test_parameters(),
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
            argon2_params: test_parameters(),
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
            argon2_params: test_parameters(),
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

// ---------------------------------------------------------------------------
// Post-ceremony file access: upload → ceremony → download round-trips
// ---------------------------------------------------------------------------

/// After a password change, previously uploaded files must still be downloadable
/// using the new credentials. Verifies that the full rewrap pipeline keeps
/// decrypted content intact end-to-end.
#[tokio::test(flavor = "multi_thread")]
async fn test_change_password_existing_file_still_downloadable() {
    let _lock = ceremony_lock().await;
    let mut vault = create_tier_one_vault().await;

    // Upload a file using original credentials.
    let staging = vault._temp.path().join("staging");
    tokio::fs::create_dir_all(&staging)
        .await
        .expect("staging dir must be created");
    let content = b"content must survive password change";
    let source_path = staging.join("pre-change.bin");
    tokio::fs::write(&source_path, content)
        .await
        .expect("source file must be written");

    let original_keys = derive_vault_keys_tier_one(&vault);
    let pre_store =
        SqlCipherMetadataStore::open(&vault.vault_db_path, &original_keys.sqlcipher_key)
            .await
            .expect("store must open with original key");
    let original_kek = KeyEncryptionKey::from_bytes(original_keys.key_encryption_key);
    let node_id = Uuid::new_v4();
    upload_file(
        &source_path,
        node_id,
        None,
        "pre-change.bin",
        0,
        0,
        &pre_store,
        &original_kek,
        &staging,
        None,
    )
    .await
    .expect("upload before password change must succeed");
    drop(pre_store);

    // Change password — rewraps all file keys with new KEK.
    change_password(
        ChangePasswordRequest {
            current_password_bytes: TEST_PASSWORD,
            new_password_bytes: TEST_NEW_PASSWORD,
            current_key_source: None,
            recovery_phrase: None,
            argon2_params: test_parameters(),
            argon2_migration_intent: Argon2MigrationIntent::PreserveTrusted,
            vault_db_path: vault.vault_db_path.clone(),
        },
        &vault.session,
        &vault.cloud,
        &mut vault.header,
        &vault.vault_id,
    )
    .await
    .expect("change_password must succeed");

    // Re-derive keys from updated header and new password.
    let new_keys = derive_vault_keys_from_header(&vault.header, TEST_NEW_PASSWORD);
    let post_store = SqlCipherMetadataStore::open(&vault.vault_db_path, &new_keys.sqlcipher_key)
        .await
        .expect("store must open with new key after password change");
    let new_kek = KeyEncryptionKey::from_bytes(new_keys.key_encryption_key);

    let recovered = download_file_to_memory(node_id, &post_store, &new_kek, &staging, None)
        .await
        .expect("file must be downloadable after password change");

    assert_eq!(
        recovered.as_slice(),
        content,
        "decrypted content must match original after password change"
    );
}

/// After a Tier 2 key-file rotation, previously uploaded files must still be
/// downloadable using the new password + new key file. Verifies that the rewrap
/// pipeline does not corrupt content when the Tier 2 factor changes.
#[tokio::test(flavor = "multi_thread")]
async fn test_tier2_key_rotate_existing_file_still_downloadable() {
    let _lock = ceremony_lock().await;
    let mut vault = create_tier_two_vault().await;

    // Upload using original Tier 2 credentials.
    let staging = vault._temp.path().join("staging");
    tokio::fs::create_dir_all(&staging)
        .await
        .expect("staging dir must be created");
    let content = b"content must survive key-file rotation";
    let source_path = staging.join("pre-rotate.bin");
    tokio::fs::write(&source_path, content)
        .await
        .expect("source file must be written");

    let original_key_bytes: [u8; 32] = std::fs::read(&vault.key_file_path)
        .expect("original key file must be readable")
        .try_into()
        .expect("key file must be 32 bytes");
    let original_source = MockKeySource::new(original_key_bytes);
    let original_keys =
        derive_vault_keys_from_header_tier_two(&vault.header, TEST_PASSWORD, &original_key_bytes);
    let pre_store =
        SqlCipherMetadataStore::open(&vault.vault_db_path, &original_keys.sqlcipher_key)
            .await
            .expect("store must open with original key");
    let original_kek = KeyEncryptionKey::from_bytes(original_keys.key_encryption_key);
    let node_id = Uuid::new_v4();
    upload_file(
        &source_path,
        node_id,
        None,
        "pre-rotate.bin",
        0,
        0,
        &pre_store,
        &original_kek,
        &staging,
        None,
    )
    .await
    .expect("upload before key rotation must succeed");
    drop(pre_store);

    // Rotate key file — rewraps all file keys with new KEK.
    let new_key_path = vault._temp.path().join("rotated.bin");
    rotate_key_file(
        RotateKeyFileRequest {
            password_bytes: TEST_PASSWORD,
            current_key_source: &original_source,
            target_new_key_file_path: new_key_path.clone(),
            recovery_phrase: None,
            argon2_params: test_parameters(),
            argon2_migration_intent: Argon2MigrationIntent::PreserveTrusted,
            vault_db_path: vault.vault_db_path.clone(),
        },
        &vault.session,
        &vault.cloud,
        &mut vault.header,
        &vault.vault_id,
    )
    .await
    .expect("rotate_key_file must succeed");

    // Re-derive keys from updated header and new key file.
    let new_key_bytes: [u8; 32] = std::fs::read(&new_key_path)
        .expect("rotated key file must be readable")
        .try_into()
        .expect("rotated key file must be 32 bytes");
    let new_keys =
        derive_vault_keys_from_header_tier_two(&vault.header, TEST_PASSWORD, &new_key_bytes);
    let post_store = SqlCipherMetadataStore::open(&vault.vault_db_path, &new_keys.sqlcipher_key)
        .await
        .expect("store must open with new key after rotation");
    let new_kek = KeyEncryptionKey::from_bytes(new_keys.key_encryption_key);

    let recovered = download_file_to_memory(node_id, &post_store, &new_kek, &staging, None)
        .await
        .expect("file must be downloadable after key-file rotation");

    assert_eq!(
        recovered.as_slice(),
        content,
        "decrypted content must match original after key-file rotation"
    );
}

/// After phrase-based recovery, previously uploaded files must be accessible
/// using the new credentials. The recovery ceremony re-wraps file keys; this
/// test verifies the re-wrap preserves the decryptable content end-to-end.
#[tokio::test(flavor = "multi_thread")]
async fn test_tier1_recovery_with_phrase_existing_file_still_downloadable() {
    let _lock = ceremony_lock().await;
    let mut vault = create_tier_one_vault().await;
    let phrase = add_recovery_slot_and_return_phrase(&mut vault).await;

    // Upload a file BEFORE the manifest backup so the backup includes the file.
    let staging = vault._temp.path().join("staging");
    tokio::fs::create_dir_all(&staging)
        .await
        .expect("staging dir must be created");
    let content = b"content must survive phrase recovery";
    let source_path = staging.join("pre-recovery.bin");
    tokio::fs::write(&source_path, content)
        .await
        .expect("source file must be written");

    let original_keys = derive_vault_keys_tier_one(&vault);
    let pre_store =
        SqlCipherMetadataStore::open(&vault.vault_db_path, &original_keys.sqlcipher_key)
            .await
            .expect("store must open with original key");
    let original_kek = KeyEncryptionKey::from_bytes(original_keys.key_encryption_key);
    let node_id = Uuid::new_v4();
    upload_file(
        &source_path,
        node_id,
        None,
        "pre-recovery.bin",
        0,
        0,
        &pre_store,
        &original_kek,
        &staging,
        None,
    )
    .await
    .expect("upload before recovery must succeed");
    drop(pre_store);

    // Upload manifest backup now that the file is in the manifest.
    upload_manifest_backup_for(&vault).await;

    // Lock then recover with phrase onto a new vault DB.
    vault.session.lock().await;
    let new_session = test_session_manager();
    let new_temp = temp_dir();
    let recovered_db_path = new_temp.path().join("recovered.db");

    let (_recovered_id, recovered_header) = recover_with_phrase(
        RecoverWithPhraseRequest {
            phrase: phrase.as_bytes(),
            vault_db_path: recovered_db_path.clone(),
            new_password_bytes: TEST_NEW_PASSWORD,
            new_key_file_path: None,
            argon2_params: test_parameters(),
            argon2_migration_intent: Argon2MigrationIntent::PreserveTrusted,
            vault_header: Some(vault.header.clone()),
        },
        &new_session,
        &vault.cloud,
    )
    .await
    .expect("recover_with_phrase must succeed");

    // Derive new keys from the recovered header and new password.
    let new_keys = derive_vault_keys_from_header(&recovered_header, TEST_NEW_PASSWORD);
    let post_store = SqlCipherMetadataStore::open(&recovered_db_path, &new_keys.sqlcipher_key)
        .await
        .expect("recovered vault DB must open with new key");
    let new_kek = KeyEncryptionKey::from_bytes(new_keys.key_encryption_key);

    // Blobs are still in the original staging dir — recovery only restores
    // the manifest (metadata); blob files remain on the local filesystem.
    let recovered = download_file_to_memory(node_id, &post_store, &new_kek, &staging, None)
        .await
        .expect("file must be downloadable after phrase recovery");

    assert_eq!(
        recovered.as_slice(),
        content,
        "decrypted content must match original after phrase recovery"
    );
}
