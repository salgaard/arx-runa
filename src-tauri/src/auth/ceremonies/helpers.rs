//! Internal helpers for ceremony flows.

use std::path::Path;

use base64::Engine;
use bip39::{Language, Mnemonic};
use rusqlite::Connection;
use rusqlite::ffi;
use secrecy::SecretBox;
use zeroize::Zeroizing;

use crate::auth::error::AuthenticationError;
use crate::auth::kdf::{Argon2Params, derive_master_key_into};
use crate::auth::session::SessionKeys;
use crate::crypto::{
    FileKey, KeyEncryptionKey, MasterKey, RecoveryKey, SqlcipherKey, WrappedFileKey,
    unwrap_file_key, wrap_file_key,
};
use crate::storage::cloud::VaultHeaderSyncError;
use crate::storage::cloud::vault_header::Argon2ParamsJson;

/// Converts runtime Argon2 parameters into vault-header JSON shape.
pub(super) fn argon2_params_to_json(params: &Argon2Params) -> Argon2ParamsJson {
    Argon2ParamsJson {
        memory_cost: params.memory_cost_kib,
        time_cost: params.time_cost,
        parallelism: params.parallelism,
    }
}

/// Converts vault-header Argon2 JSON fields into runtime parameters.
pub(super) fn argon2_params_from_json(json: &Argon2ParamsJson) -> Argon2Params {
    Argon2Params {
        memory_cost_kib: json.memory_cost,
        time_cost: json.time_cost,
        parallelism: json.parallelism,
    }
}

/// Enforces the canonical Argon2 policy for all ceremony request payloads.
pub(super) fn enforce_argon2_policy(params: &Argon2Params) -> Result<(), AuthenticationError> {
    #[cfg(test)]
    {
        let _ = params;
        Ok(())
    }

    #[cfg(not(test))]
    {
        if *params == Argon2Params::DEFAULT {
            Ok(())
        } else {
            Err(AuthenticationError::VaultHeaderInvalid)
        }
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

/// Constructs a `SqlcipherKey` from borrowed bytes without by-value constructors.
pub(super) fn sqlcipher_key_from_array(bytes: &[u8; 32]) -> SqlcipherKey {
    SqlcipherKey::from_secret_box(secret_box_from_array(bytes))
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
    sqlcipher_key: &[u8; 32],
) -> Result<Connection, AuthenticationError> {
    let conn = Connection::open(path).map_err(|_| AuthenticationError::VaultHeaderInvalid)?;
    let rc = {
        // SAFETY: `conn` is open for this thread and `sqlcipher_key` points to
        // a valid 32-byte buffer for the duration of the call.
        unsafe {
            ffi::sqlite3_key(
                conn.handle(),
                sqlcipher_key.as_ptr().cast(),
                sqlcipher_key.len() as i32,
            )
        }
    };
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
    new_sqlcipher_key: &[u8; 32],
) -> Result<(), AuthenticationError> {
    let rc = {
        // SAFETY: `conn` is open for this thread and `new_sqlcipher_key` points
        // to a valid 32-byte buffer for the duration of the call.
        unsafe {
            ffi::sqlite3_rekey(
                conn.handle(),
                new_sqlcipher_key.as_ptr().cast(),
                new_sqlcipher_key.len() as i32,
            )
        }
    };
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

/// Wraps 32-byte plaintext under the active session KEK.
pub(super) fn wrap_with_session_kek(
    session_keys: &SessionKeys,
    plaintext_bytes: &[u8; 32],
) -> Result<WrappedFileKey, AuthenticationError> {
    let key_encryption_key =
        key_encryption_key_from_array(session_keys.key_encryption_key.expose());
    let file_key = file_key_from_array(plaintext_bytes);
    wrap_file_key(&file_key, &key_encryption_key)
        .map_err(|_| AuthenticationError::VaultHeaderInvalid)
}

#[cfg(test)]
pub(super) fn wrap_with_kek_bytes(
    key_encryption_key_bytes: &[u8; 32],
    plaintext_bytes: &[u8; 32],
) -> Result<WrappedFileKey, AuthenticationError> {
    let key_encryption_key = key_encryption_key_from_array(key_encryption_key_bytes);
    let file_key = file_key_from_array(plaintext_bytes);
    wrap_file_key(&file_key, &key_encryption_key)
        .map_err(|_| AuthenticationError::VaultHeaderInvalid)
}

#[cfg(test)]
pub(super) fn unwrap_with_kek_bytes(
    wrapped: &WrappedFileKey,
    key_encryption_key_bytes: &[u8; 32],
) -> Result<FileKey, AuthenticationError> {
    let key_encryption_key = key_encryption_key_from_array(key_encryption_key_bytes);
    unwrap_file_key(wrapped, &key_encryption_key)
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
    params: &Argon2Params,
    output: &mut [u8; 32],
) -> Result<(), AuthenticationError> {
    derive_master_key_into(phrase_canonical_bytes, None, salt, params, output)
}

/// Verifies credentials by unwrapping the persisted identity key with fresh keys.
pub(super) async fn verify_credentials_via_identity_row(
    vault_db_path: &Path,
    sqlcipher_key: SqlcipherKey,
    key_encryption_key: KeyEncryptionKey,
) -> Result<(), AuthenticationError> {
    let vault_db_path = vault_db_path.to_path_buf();
    tokio::task::spawn_blocking(move || -> Result<(), AuthenticationError> {
        let conn = open_sqlcipher(&vault_db_path, sqlcipher_key.expose())?;
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
        let wrapped = WrappedFileKey(wrapped_array);
        unwrap_file_key(&wrapped, &key_encryption_key)
            .map_err(|_| AuthenticationError::InvalidCredentials)?;
        Ok(())
    })
    .await
    .map_err(|_| AuthenticationError::VaultHeaderInvalid)?
}
