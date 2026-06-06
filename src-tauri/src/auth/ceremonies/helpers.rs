//! Internal helpers for ceremony flows.

use std::path::Path;

use base64::Engine;
use bip39::{Language, Mnemonic};
use rusqlite::ffi;
use rusqlite::{Connection, OptionalExtension, params};
use secrecy::SecretBox;
use uuid::Uuid;
use zeroize::Zeroizing;

use super::types::Argon2MigrationIntent;
use crate::auth::error::AuthenticationError;
use crate::auth::kdf::{Argon2Params, derive_master_key_into};
use crate::auth::session::SessionKeys;
use crate::crypto::{
    FileId, FileKey, KeyEncryptionKey, MasterKey, RecoveryKey, SqlcipherKey, VaultId,
    WrappedFileKey, WrappedMasterKey, unwrap_file_key, unwrap_master_key_from_recovery,
    wrap_file_key,
};
use crate::storage::cloud::VaultHeaderSyncError;
use crate::storage::cloud::vault_header::{Argon2ParametersJson, VaultHeader};

/// Converts runtime Argon2 parameters into vault-header JSON shape.
pub(super) fn argon2_parameters_to_json(parameters: &Argon2Params) -> Argon2ParametersJson {
    Argon2ParametersJson {
        memory_cost: parameters.memory_cost_kib,
        time_cost: parameters.time_cost,
        parallelism: parameters.parallelism,
    }
}

/// Converts vault-header Argon2 JSON fields into runtime parameters.
pub(super) fn argon2_parameters_from_json(json: &Argon2ParametersJson) -> Argon2Params {
    Argon2Params {
        memory_cost_kib: json.memory_cost,
        time_cost: json.time_cost,
        parallelism: json.parallelism,
    }
}

/// Enforces canonical Argon2 defaults for new-vault creation only.
pub(super) fn validate_new_vault_argon2_defaults(
    parameters: &Argon2Params,
) -> Result<(), AuthenticationError> {
    if *parameters == Argon2Params::DEFAULT {
        Ok(())
    } else {
        Err(AuthenticationError::VaultHeaderInvalid)
    }
}

/// Resolves effective Argon2 parameters for existing-vault ceremonies.
///
/// Default behavior preserves trusted header parameters. Explicit migration
/// intent is required before requested parameters are applied.
pub(super) fn resolve_existing_vault_argon2(
    trusted_header_parameters: &Argon2Params,
    requested_parameters: &Argon2Params,
    migration_intent: Argon2MigrationIntent,
) -> Result<Argon2Params, AuthenticationError> {
    match migration_intent {
        Argon2MigrationIntent::PreserveTrusted => Ok(*trusted_header_parameters),
        Argon2MigrationIntent::MigrateToRequested => Ok(*requested_parameters),
    }
}

/// Copies a borrowed 32-byte key into protected heap storage.
pub(super) fn secret_box_from_array(bytes: &[u8; 32]) -> SecretBox<[u8; 32]> {
    let mut boxed = Box::new([0u8; 32]);
    boxed.copy_from_slice(bytes);
    SecretBox::new(boxed)
}

/// Constructs a `FileKey` from borrowed bytes without by-value constructors.
pub(super) fn file_key_from_array(bytes: &[u8; 32]) -> FileKey {
    FileKey::from_secret_box(secret_box_from_array(bytes))
}

/// Constructs a `KeyEncryptionKey` from borrowed bytes without by-value constructors.
pub(super) fn key_encryption_key_from_array(bytes: &[u8; 32]) -> KeyEncryptionKey {
    KeyEncryptionKey::from_secret_box(secret_box_from_array(bytes))
}

/// Constructs a `MasterKey` from borrowed bytes without by-value constructors.
pub(super) fn master_key_from_array(bytes: &[u8; 32]) -> MasterKey {
    MasterKey::from_secret_box(secret_box_from_array(bytes))
}

/// Constructs a `RecoveryKey` from borrowed bytes without by-value constructors.
pub(super) fn recovery_key_from_array(bytes: &[u8; 32]) -> RecoveryKey {
    RecoveryKey::from_secret_box(secret_box_from_array(bytes))
}

/// Ensures the parent directory for `path` exists.
pub(super) async fn ensure_parent_directory_exists(path: &Path) -> Result<(), AuthenticationError> {
    if let Some(parent) = path.parent()
        && !tokio::fs::try_exists(parent)
            .await
            .map_err(|_| AuthenticationError::VaultHeaderInvalid)?
    {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|_| AuthenticationError::VaultHeaderInvalid)?;
    }
    Ok(())
}

/// Validates that recovery import can target `path` without overwriting an
/// existing database file.
pub(super) async fn precheck_recovery_destination(path: &Path) -> Result<(), AuthenticationError> {
    if tokio::fs::try_exists(path)
        .await
        .map_err(|_| AuthenticationError::VaultHeaderInvalid)?
    {
        return Err(AuthenticationError::VaultHeaderInvalid);
    }
    Ok(())
}

/// Removes `path` if it exists, logging unexpected cleanup failures.
pub(super) async fn remove_file_if_exists(path: &Path) {
    match tokio::fs::remove_file(path).await {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(cleanup_error) => tracing::warn!(?cleanup_error, "file cleanup failed"),
    }
}

/// Maps cloud vault-header sync failures to the auth boundary error.
pub(super) fn map_vault_header_sync_error(_error: VaultHeaderSyncError) -> AuthenticationError {
    AuthenticationError::VaultHeaderInvalid
}

/// Encodes raw bytes with standard base64.
pub(super) fn encode_base64(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

/// Decodes standard base64 into a fixed 32-byte array.
pub(super) fn decode_base64_32(input: &str) -> Result<[u8; 32], AuthenticationError> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(input)
        .map_err(|_| AuthenticationError::VaultHeaderInvalid)?;
    let array: [u8; 32] = bytes
        .try_into()
        .map_err(|_| AuthenticationError::VaultHeaderInvalid)?;
    Ok(array)
}

/// Decodes standard base64 into a fixed 72-byte array.
pub(super) fn decode_base64_72(input: &str) -> Result<[u8; 72], AuthenticationError> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(input)
        .map_err(|_| AuthenticationError::VaultHeaderInvalid)?;
    let array: [u8; 72] = bytes
        .try_into()
        .map_err(|_| AuthenticationError::VaultHeaderInvalid)?;
    Ok(array)
}

/// Opens a SQLCipher database and applies a raw-byte key via SQLCipher FFI.
pub(super) fn open_sqlcipher(
    path: &Path,
    sqlcipher_key: &SqlcipherKey,
) -> Result<Connection, AuthenticationError> {
    let conn = Connection::open(path).map_err(|_| AuthenticationError::VaultHeaderInvalid)?;
    let rc = sqlcipher_key.with_exposed(|key_bytes| {
        // SAFETY: `conn` is open for this thread and `sqlcipher_key` points to
        // a valid 32-byte buffer for the duration of the call.
        unsafe {
            ffi::sqlite3_key(
                conn.handle(),
                key_bytes.as_ptr().cast(),
                key_bytes.len() as i32,
            )
        }
    });
    if rc != ffi::SQLITE_OK {
        let error_message = {
            // SAFETY: `conn.handle()` remains valid while `conn` is alive and
            // `sqlite3_errmsg` returns a NUL-terminated string pointer owned by SQLite.
            unsafe {
                let message_ptr = ffi::sqlite3_errmsg(conn.handle());
                std::ffi::CStr::from_ptr(message_ptr)
                    .to_string_lossy()
                    .into_owned()
            }
        };
        tracing::warn!(rc, error_message, "sqlite3_key failed");
        return Err(AuthenticationError::InvalidCredentials);
    }
    Ok(conn)
}

/// Rekeys an already-open SQLCipher connection via SQLCipher FFI.
pub(super) fn rekey_sqlcipher(
    conn: &Connection,
    new_sqlcipher_key: &SqlcipherKey,
) -> Result<(), AuthenticationError> {
    let rc = new_sqlcipher_key.with_exposed(|key_bytes| {
        // SAFETY: `conn` is open for this thread and `new_sqlcipher_key` points
        // to a valid 32-byte buffer for the duration of the call.
        unsafe {
            ffi::sqlite3_rekey(
                conn.handle(),
                key_bytes.as_ptr().cast(),
                key_bytes.len() as i32,
            )
        }
    });
    if rc != ffi::SQLITE_OK {
        let error_message = {
            // SAFETY: `conn.handle()` remains valid while `conn` is alive and
            // `sqlite3_errmsg` returns a NUL-terminated string pointer owned by SQLite.
            unsafe {
                let message_ptr = ffi::sqlite3_errmsg(conn.handle());
                std::ffi::CStr::from_ptr(message_ptr)
                    .to_string_lossy()
                    .into_owned()
            }
        };
        tracing::warn!(rc, error_message, "sqlite3_rekey failed");
        return Err(AuthenticationError::VaultHeaderInvalid);
    }
    Ok(())
}

/// Wraps 32-byte plaintext under the active session KEK, bound to `file_id`.
pub(super) fn wrap_with_session_kek(
    session_keys: &SessionKeys,
    file_id: &FileId,
    plaintext_bytes: &[u8; 32],
) -> Result<WrappedFileKey, AuthenticationError> {
    let key_encryption_key =
        key_encryption_key_from_array(session_keys.key_encryption_key.expose());
    let file_key = file_key_from_array(plaintext_bytes);
    wrap_file_key(&file_key, file_id, &key_encryption_key)
        .map_err(|_| AuthenticationError::VaultHeaderInvalid)
}

#[cfg(test)]
pub(super) fn wrap_with_kek_bytes(
    key_encryption_key_bytes: &[u8; 32],
    file_id: &FileId,
    plaintext_bytes: &[u8; 32],
) -> Result<WrappedFileKey, AuthenticationError> {
    let key_encryption_key = key_encryption_key_from_array(key_encryption_key_bytes);
    let file_key = file_key_from_array(plaintext_bytes);
    wrap_file_key(&file_key, file_id, &key_encryption_key)
        .map_err(|_| AuthenticationError::VaultHeaderInvalid)
}

#[cfg(test)]
pub(super) fn unwrap_with_kek_bytes(
    wrapped: &WrappedFileKey,
    file_id: &FileId,
    key_encryption_key_bytes: &[u8; 32],
) -> Result<FileKey, AuthenticationError> {
    let key_encryption_key = key_encryption_key_from_array(key_encryption_key_bytes);
    unwrap_file_key(wrapped, file_id, &key_encryption_key)
        .map_err(|_| AuthenticationError::InvalidCredentials)
}

/// Parses an English BIP-39 phrase and maps failures to auth errors.
pub(super) fn parse_mnemonic(phrase: &str) -> Result<Mnemonic, AuthenticationError> {
    Mnemonic::parse_in(Language::English, phrase)
        .map_err(|_| AuthenticationError::InvalidRecoveryPhrase)
}

/// Canonicalizes a mnemonic into the required space-delimited word form.
pub(super) fn canonicalize_phrase(mnemonic: &Mnemonic) -> Zeroizing<String> {
    Zeroizing::new(mnemonic.words().collect::<Vec<_>>().join(" "))
}

/// Derives a recovery key from phrase bytes and slot-local Argon2 parameters.
pub(super) fn derive_recovery_key_into(
    phrase_canonical_bytes: &[u8],
    salt: &[u8; 32],
    parameters: &Argon2Params,
    output: &mut [u8; 32],
) -> Result<(), AuthenticationError> {
    derive_master_key_into(phrase_canonical_bytes, None, salt, parameters, output)
}

/// Verifies credentials by unwrapping the persisted identity key with fresh keys.
///
/// `identity_file_id` must be the `FileId` used when the identity key was
/// originally wrapped (typically `FileId::new(*vault_id.as_bytes())`).
pub(super) async fn verify_credentials_via_identity_row(
    vault_db_path: &Path,
    sqlcipher_key: SqlcipherKey,
    key_encryption_key: KeyEncryptionKey,
    identity_file_id: FileId,
) -> Result<(), AuthenticationError> {
    let vault_db_path = vault_db_path.to_path_buf();
    tokio::task::spawn_blocking(move || -> Result<(), AuthenticationError> {
        let conn = open_sqlcipher(&vault_db_path, &sqlcipher_key)?;
        let wrapped_blob: Vec<u8> = conn
            .query_row(
                "SELECT wrapped_private_key FROM vault_identity WHERE id = 1",
                [],
                |row| row.get(0),
            )
            .map_err(|_| AuthenticationError::InvalidCredentials)?;
        let wrapped_array: [u8; 72] = wrapped_blob
            .try_into()
            .map_err(|_| AuthenticationError::VaultHeaderInvalid)?;
        let wrapped = WrappedFileKey::new(wrapped_array);
        unwrap_file_key(&wrapped, &identity_file_id, &key_encryption_key)
            .map_err(|_| AuthenticationError::InvalidCredentials)?;
        Ok(())
    })
    .await
    .map_err(|_| AuthenticationError::VaultHeaderInvalid)?
}

/// Resolves what to do with the recovery slot during a rekey ceremony.
pub(super) enum RecoverySlotAction {
    /// Slot exists and the phrase was verified — rewrap it under the new master key.
    KeepAndRewrap(RecoveryKey),
    /// Slot exists but no phrase was supplied — the caller must clear all slots.
    Remove,
    /// No recovery slots exist — nothing to do.
    NoSlots,
}

/// Inspects the vault header's recovery slots and validates any supplied phrase,
/// returning a [`RecoverySlotAction`] that drives the caller's post-rekey header update.
pub(super) fn resolve_recovery_slot_action(
    vault_header: &VaultHeader,
    recovery_phrase: Option<&[u8]>,
    vault_id: &VaultId,
) -> Result<RecoverySlotAction, AuthenticationError> {
    if vault_header.recovery_slots.is_empty() {
        return Ok(RecoverySlotAction::NoSlots);
    }
    match recovery_phrase {
        None => Ok(RecoverySlotAction::Remove),
        Some(phrase) => {
            let phrase_str = std::str::from_utf8(phrase)
                .map_err(|_| AuthenticationError::InvalidRecoveryPhrase)?;
            let mnemonic = parse_mnemonic(phrase_str)?;
            let canonical = canonicalize_phrase(&mnemonic);
            let slot_index = vault_header
                .recovery_slots
                .iter()
                .position(|slot| slot.method == "bip39")
                .ok_or(AuthenticationError::NoRecoverySlot)?;
            let slot = &vault_header.recovery_slots[slot_index];
            let slot_salt = decode_base64_32(&slot.argon2_salt)?;
            let slot_params = argon2_parameters_from_json(&slot.argon2_params);
            let wrapped_bytes = decode_base64_72(&slot.wrapped_master_key)?;
            let wrapped = WrappedMasterKey::new(wrapped_bytes);
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
                Ok(_master_key) => Ok(RecoverySlotAction::KeepAndRewrap(recovery_key)),
                Err(_) => Err(AuthenticationError::InvalidCredentials),
            }
        }
    }
}

/// Re-wraps all file keys and the identity key inside an open SQLCipher connection,
/// then commits and rekeys the database.  Called from inside `spawn_blocking`.
pub(super) fn rekey_vault_db(
    conn: Connection,
    current_kek: &KeyEncryptionKey,
    new_kek: &KeyEncryptionKey,
    new_sqlcipher: SqlcipherKey,
    vault_id_bytes: [u8; 16],
) -> Result<(), AuthenticationError> {
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
                let file_id = FileId::from_uuid(
                    Uuid::parse_str(&node_id)
                        .map_err(|_| AuthenticationError::InvalidCredentials)?,
                );
                let wrapped_array: [u8; 72] = wrapped_blob
                    .try_into()
                    .map_err(|_| AuthenticationError::InvalidCredentials)?;
                let wrapped = WrappedFileKey::new(wrapped_array);
                let file_key = unwrap_file_key(&wrapped, &file_id, current_kek)
                    .map_err(|_| AuthenticationError::InvalidCredentials)?;
                let rewrapped = wrap_file_key(&file_key, &file_id, new_kek)
                    .map_err(|_| AuthenticationError::VaultHeaderInvalid)?;
                conn.execute(
                    "UPDATE nodes SET file_key_wrapped = ? WHERE node_id = ?",
                    params![rewrapped.as_bytes().to_vec(), node_id],
                )
                .map_err(|_| AuthenticationError::InvalidCredentials)?;
            }
        }
        // Epoch-packed files are encrypted with a per-epoch-blob key wrapped under
        // the KEK (AAD = epoch_blob_id), stored in `epoch_blobs.file_key_wrapped`.
        // These must be re-wrapped on rekey too; otherwise epoch-packed files
        // become undecryptable after recovery or password change (the old-KEK
        // wrapping can no longer be unwrapped, surfacing as a chunk-decrypt
        // failure). A row that cannot be unwrapped with the current KEK is
        // tolerated (skipped with a warning) so a legacy vault re-keyed before
        // this fix existed can still complete recovery and open the rest of its
        // files rather than aborting the whole rekey.
        {
            let mut stmt = conn
                .prepare("SELECT epoch_blob_id, file_key_wrapped FROM epoch_blobs")
                .map_err(|_| AuthenticationError::InvalidCredentials)?;
            let rows: Vec<(String, Vec<u8>)> = stmt
                .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
                .map_err(|_| AuthenticationError::InvalidCredentials)?
                .collect::<Result<_, _>>()
                .map_err(|_| AuthenticationError::InvalidCredentials)?;
            for (epoch_blob_id, wrapped_blob) in rows {
                let Ok(epoch_uuid) = Uuid::parse_str(&epoch_blob_id) else {
                    tracing::warn!("skipping epoch blob with invalid id during rekey");
                    continue;
                };
                let file_id = FileId::from_uuid(epoch_uuid);
                let Ok(wrapped_array) = <[u8; 72]>::try_from(wrapped_blob) else {
                    tracing::warn!(%epoch_blob_id, "skipping epoch blob with malformed wrapped key during rekey");
                    continue;
                };
                let wrapped = WrappedFileKey::new(wrapped_array);
                let file_key = match unwrap_file_key(&wrapped, &file_id, current_kek) {
                    Ok(key) => key,
                    Err(_) => {
                        tracing::warn!(
                            %epoch_blob_id,
                            "epoch blob key could not be unwrapped with current KEK during rekey; leaving as-is (legacy stranded key)"
                        );
                        continue;
                    }
                };
                let rewrapped = wrap_file_key(&file_key, &file_id, new_kek)
                    .map_err(|_| AuthenticationError::VaultHeaderInvalid)?;
                conn.execute(
                    "UPDATE epoch_blobs SET file_key_wrapped = ? WHERE epoch_blob_id = ?",
                    params![rewrapped.as_bytes().to_vec(), epoch_blob_id],
                )
                .map_err(|_| AuthenticationError::InvalidCredentials)?;
            }
        }
        let identity_file_id = FileId::new(vault_id_bytes);
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
            let wrapped = WrappedFileKey::new(wrapped_array);
            let file_key = unwrap_file_key(&wrapped, &identity_file_id, current_kek)
                .map_err(|_| AuthenticationError::InvalidCredentials)?;
            let rewrapped = wrap_file_key(&file_key, &identity_file_id, new_kek)
                .map_err(|_| AuthenticationError::VaultHeaderInvalid)?;
            conn.execute(
                "UPDATE vault_identity SET wrapped_private_key = ? WHERE id = 1",
                params![rewrapped.as_bytes().to_vec()],
            )
            .map_err(|_| AuthenticationError::InvalidCredentials)?;
        }
        Ok(())
    })();
    match transaction_result {
        Ok(()) => {
            conn.execute_batch("COMMIT;")
                .map_err(|_| AuthenticationError::InvalidCredentials)?;
            rekey_sqlcipher(&conn, &new_sqlcipher)?;
            drop(conn);
            Ok(())
        }
        Err(error) => {
            if let Err(rb) = conn.execute_batch("ROLLBACK;") {
                tracing::warn!(
                    ?rb,
                    "rollback failed during vault rekey; connection will be dropped"
                );
            }
            drop(conn);
            Err(error)
        }
    }
}
