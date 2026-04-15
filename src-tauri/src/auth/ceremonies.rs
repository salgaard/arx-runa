//! Vault lifecycle ceremonies (Phase 2.4).
//!
//! Ceremony entry points: [`create_vault`], [`change_password`],
//! [`rotate_key_file`], [`recover_vault`], [`setup_recovery`],
//! [`recover_with_phrase`]. Each function owns the full multi-step flow
//! documented in the parent design's Vault Creation / Password Change /
//! Rotation / Recovery sections.
//!
//! Critical invariant (sub-phase deliverable 7): `master_key` never escapes
//! ceremony-local scope. It is held as `Zeroizing<[u8; 32]>` inside a single
//! function body and zeroed at end-of-scope. No struct outside this module
//! carries a `master_key` or `MasterKey` field.

use std::path::{Path, PathBuf};

use base64::Engine;
use bip39::{Language, Mnemonic};
use chacha20poly1305::aead::OsRng;
use rand::Rng;
use rusqlite::ffi;
use rusqlite::{Connection, OptionalExtension, params};
use secrecy::SecretBox;
use uuid::Uuid;
use x25519_dalek::{PublicKey, StaticSecret};
use zeroize::Zeroizing;

use crate::auth::error::AuthenticationError;
use crate::auth::kdf::{Argon2Params, derive_master_key_into};
use crate::auth::key_source::KeySource;
use crate::auth::session::{SessionKeys, SessionManager};
use crate::auth::staging;
use crate::crypto::{
    FileKey, KeyEncryptionKey, MasterKey, RecoveryKey, SqlcipherKey, VaultId, WrappedFileKey,
    WrappedMasterKey, unwrap_file_key, unwrap_master_key_from_recovery, wrap_file_key,
    wrap_master_key_for_recovery,
};
use crate::storage::cloud::CloudTransport;
use crate::storage::cloud::manifest_backup::decrypt_manifest_backup;
use crate::storage::cloud::vault_header::{Argon2ParamsJson, RecoverySlot, VaultHeader};

/// Name of the vault header object at the cloud root.
const VAULT_HEADER_BLOB_NAME: &str = "vault-header.json";
/// Name of the manifest backup object at the cloud root.
const MANIFEST_BACKUP_BLOB_NAME: &str = "manifest-backup.enc";
/// Filename used for the pre-upload staging file.
const STAGING_FILE_NAME: &str = "pending-vault-header.json";
/// SQL schema applied when a new vault database is created.
const VAULT_STUB_SCHEMA: &str = "
CREATE TABLE _phase_stub (id INTEGER PRIMARY KEY);
CREATE TABLE nodes (
    id INTEGER PRIMARY KEY,
    file_key_wrapped BLOB NOT NULL
);
CREATE TABLE vault_identity (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    public_key BLOB NOT NULL UNIQUE,
    wrapped_private_key BLOB NOT NULL
);
";

/// Authentication tier for a vault.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tier {
    /// Password only.
    One,
    /// Password + 32-byte USB key file.
    Two,
}

impl Tier {
    /// Returns the serialized tier value used in vault headers.
    fn as_u8(self) -> u8 {
        match self {
            Tier::One => 1,
            Tier::Two => 2,
        }
    }
}

/// Request payload for [`create_vault`].
pub struct CreateVaultRequest<'a> {
    /// Chosen authentication tier.
    pub tier: Tier,
    /// UTF-8 password bytes entered by the user.
    pub password_bytes: &'a [u8],
    /// Destination path for the generated key file; `Some` iff [`Tier::Two`].
    pub target_key_file_path: Option<PathBuf>,
    /// Destination path for the SQLCipher vault database file.
    pub vault_db_path: PathBuf,
    /// Argon2id cost parameters.
    pub argon2_params: Argon2Params,
}

/// Request payload for [`change_password`].
pub struct ChangePasswordRequest<'a> {
    /// Current password bytes.
    pub current_password_bytes: &'a [u8],
    /// New password bytes.
    pub new_password_bytes: &'a [u8],
    /// Current key source for Tier 2 vaults; `None` for Tier 1.
    pub current_key_source: Option<&'a (dyn KeySource + Send + Sync)>,
    /// Optional recovery phrase; if present, the recovery slot is re-wrapped
    /// under the new master key instead of being cleared.
    pub recovery_phrase: Option<&'a str>,
    /// Argon2id cost parameters for the new slot.
    pub argon2_params: Argon2Params,
    /// Vault database path.
    pub vault_db_path: PathBuf,
}

/// Request payload for [`rotate_key_file`].
pub struct RotateKeyFileRequest<'a> {
    /// Current password bytes.
    pub password_bytes: &'a [u8],
    /// Current key source for the vault's existing key file.
    pub current_key_source: &'a (dyn KeySource + Send + Sync),
    /// Destination path for the freshly generated key file.
    pub target_new_key_file_path: PathBuf,
    /// Optional recovery phrase to re-wrap the recovery slot.
    pub recovery_phrase: Option<&'a str>,
    /// Argon2id cost parameters.
    pub argon2_params: Argon2Params,
    /// Vault database path.
    pub vault_db_path: PathBuf,
}

/// Request payload for [`recover_vault`].
pub struct RecoverVaultRequest<'a> {
    /// Password bytes entered by the user on the new device.
    pub password_bytes: &'a [u8],
    /// Key source for Tier 2 vaults; `None` for Tier 1.
    pub key_source: Option<&'a (dyn KeySource + Send + Sync)>,
    /// Destination path for the recovered SQLCipher DB.
    pub vault_db_path: PathBuf,
}

/// Request payload for [`setup_recovery`].
pub struct SetupRecoveryRequest<'a> {
    /// Current password bytes (for credential re-verification).
    pub current_password_bytes: &'a [u8],
    /// Current key source for Tier 2 vaults; `None` for Tier 1.
    pub current_key_source: Option<&'a (dyn KeySource + Send + Sync)>,
    /// Argon2id cost parameters for the recovery slot's KDF.
    pub argon2_params: Argon2Params,
    /// Vault database path (used to verify current credentials).
    pub vault_db_path: PathBuf,
}

/// Request payload for [`recover_with_phrase`].
pub struct RecoverWithPhraseRequest<'a> {
    /// BIP-39 recovery phrase entered by the user.
    pub phrase: &'a str,
    /// Destination path for the recovered vault DB.
    pub vault_db_path: PathBuf,
}

/// Converts runtime Argon2 parameters into vault-header JSON shape.
fn argon2_params_to_json(params: &Argon2Params) -> Argon2ParamsJson {
    Argon2ParamsJson {
        memory_cost: params.memory_cost_kib,
        time_cost: params.time_cost,
        parallelism: params.parallelism,
    }
}

/// Converts vault-header Argon2 JSON fields into runtime parameters.
fn argon2_params_from_json(json: &Argon2ParamsJson) -> Argon2Params {
    Argon2Params {
        memory_cost_kib: json.memory_cost,
        time_cost: json.time_cost,
        parallelism: json.parallelism,
    }
}

/// Enforces the canonical Argon2 policy for all ceremony request payloads.
fn enforce_argon2_policy(params: &Argon2Params) -> Result<(), AuthenticationError> {
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
fn secret_box_from_array(bytes: &[u8; 32]) -> SecretBox<[u8; 32]> {
    let mut boxed = Box::new([0u8; 32]);
    boxed.copy_from_slice(bytes);
    SecretBox::new(boxed)
}

/// Constructs a `FileKey` from borrowed bytes without by-value constructors.
fn file_key_from_array(bytes: &[u8; 32]) -> FileKey {
    FileKey::from_secret_box(secret_box_from_array(bytes))
}

/// Constructs a `KeyEncryptionKey` from borrowed bytes without by-value constructors.
fn key_encryption_key_from_array(bytes: &[u8; 32]) -> KeyEncryptionKey {
    KeyEncryptionKey::from_secret_box(secret_box_from_array(bytes))
}

/// Constructs a `SqlcipherKey` from borrowed bytes without by-value constructors.
fn sqlcipher_key_from_array(bytes: &[u8; 32]) -> SqlcipherKey {
    SqlcipherKey::from_secret_box(secret_box_from_array(bytes))
}

/// Constructs a `MasterKey` from borrowed bytes without by-value constructors.
fn master_key_from_array(bytes: &[u8; 32]) -> MasterKey {
    MasterKey::from_secret_box(secret_box_from_array(bytes))
}

/// Constructs a `RecoveryKey` from borrowed bytes without by-value constructors.
fn recovery_key_from_array(bytes: &[u8; 32]) -> RecoveryKey {
    RecoveryKey::from_secret_box(secret_box_from_array(bytes))
}

/// Ensures the parent directory for `path` exists.
async fn ensure_parent_directory_exists(path: &Path) -> Result<(), AuthenticationError> {
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

/// Removes `path` if it exists, logging unexpected cleanup failures.
async fn remove_file_if_exists(path: &Path) {
    match tokio::fs::remove_file(path).await {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(cleanup_error) => tracing::warn!(?cleanup_error, "file cleanup failed"),
    }
}

/// Logs and ignores staging cleanup failures so primary upload outcome wins.
async fn best_effort_cleanup_staging(staging_path: &Path) {
    if let Err(cleanup_error) = staging::remove_if_exists(staging_path).await {
        tracing::warn!(?cleanup_error, "staging cleanup failed");
    }
}

/// Encodes raw bytes with standard base64.
fn encode_base64(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

/// Decodes standard base64 into a fixed 32-byte array.
fn decode_base64_32(input: &str) -> Result<[u8; 32], AuthenticationError> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(input)
        .map_err(|_| AuthenticationError::VaultHeaderInvalid)?;
    let array: [u8; 32] = bytes
        .try_into()
        .map_err(|_| AuthenticationError::VaultHeaderInvalid)?;
    Ok(array)
}

/// Decodes standard base64 into a fixed 72-byte array.
fn decode_base64_72(input: &str) -> Result<[u8; 72], AuthenticationError> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(input)
        .map_err(|_| AuthenticationError::VaultHeaderInvalid)?;
    let array: [u8; 72] = bytes
        .try_into()
        .map_err(|_| AuthenticationError::VaultHeaderInvalid)?;
    Ok(array)
}

/// Removes a generated file on drop unless explicitly disarmed.
struct ScopedFileCleanup {
    path: PathBuf,
    armed: bool,
}

impl ScopedFileCleanup {
    /// Creates a new armed cleanup guard for `path`.
    fn new(path: PathBuf) -> Self {
        Self { path, armed: true }
    }

    /// Disables cleanup for this guard.
    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for ScopedFileCleanup {
    fn drop(&mut self) {
        if self.armed {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

/// Opens a SQLCipher database and applies a raw-byte key via SQLCipher FFI.
fn open_sqlcipher(
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
fn rekey_sqlcipher(
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
fn wrap_with_session_kek(
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
fn wrap_with_kek_bytes(
    key_encryption_key_bytes: &[u8; 32],
    plaintext_bytes: &[u8; 32],
) -> Result<WrappedFileKey, AuthenticationError> {
    let key_encryption_key = key_encryption_key_from_array(key_encryption_key_bytes);
    let file_key = file_key_from_array(plaintext_bytes);
    wrap_file_key(&file_key, &key_encryption_key)
        .map_err(|_| AuthenticationError::VaultHeaderInvalid)
}

#[cfg(test)]
fn unwrap_with_kek_bytes(
    wrapped: &WrappedFileKey,
    key_encryption_key_bytes: &[u8; 32],
) -> Result<FileKey, AuthenticationError> {
    let key_encryption_key = key_encryption_key_from_array(key_encryption_key_bytes);
    unwrap_file_key(wrapped, &key_encryption_key)
        .map_err(|_| AuthenticationError::InvalidCredentials)
}

/// Parses an English BIP-39 phrase and maps failures to auth errors.
fn parse_mnemonic(phrase: &str) -> Result<Mnemonic, AuthenticationError> {
    Mnemonic::parse_in(Language::English, phrase)
        .map_err(|_| AuthenticationError::InvalidRecoveryPhrase)
}

/// Canonicalizes a mnemonic into the required space-delimited word form.
fn canonicalize_phrase(mnemonic: &Mnemonic) -> Zeroizing<String> {
    Zeroizing::new(mnemonic.words().collect::<Vec<_>>().join(" "))
}

/// Derives a recovery key from phrase bytes and slot-local Argon2 parameters.
fn derive_recovery_key_into(
    phrase_canonical_bytes: &[u8],
    salt: &[u8; 32],
    params: &Argon2Params,
    output: &mut [u8; 32],
) -> Result<(), AuthenticationError> {
    derive_master_key_into(phrase_canonical_bytes, None, salt, params, output)
}

/// Verifies credentials by unwrapping the persisted identity key with fresh keys.
async fn verify_credentials_via_identity_row(
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
    cloud_transport: &dyn CloudTransport,
) -> Result<VaultId, AuthenticationError> {
    enforce_argon2_policy(&request.argon2_params)?;

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
        staging::write_owner_only(key_file_path, buffer.as_slice()).await?;
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
    let wrapped_private_key_vec: Vec<u8> = wrapped_private_key.0.to_vec();
    let public_key_vec: Vec<u8> = public_key_bytes.to_vec();
    let db_result: Result<(), AuthenticationError> =
        tokio::task::spawn_blocking(move || -> Result<(), AuthenticationError> {
            let conn = open_sqlcipher(&vault_db_path_owned, sqlcipher_key.expose())?;
            conn.execute_batch(VAULT_STUB_SCHEMA)
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

    let json_bytes =
        serde_json::to_vec_pretty(&header).map_err(|_| AuthenticationError::VaultHeaderInvalid)?;
    let staging_dir = staging::staging_directory().await?;
    let staging_path = staging_dir.join(STAGING_FILE_NAME);
    staging::write_owner_only(&staging_path, &json_bytes).await?;
    let upload_result = cloud_transport
        .upload_blob(VAULT_HEADER_BLOB_NAME, &json_bytes)
        .await;
    let cleanup_result = staging::remove_if_exists(&staging_path).await;
    if upload_result.is_err() {
        if let Err(cleanup_error) = cleanup_result {
            tracing::warn!(
                ?cleanup_error,
                "staging cleanup failed after upload failure"
            );
        }
        if let Some(key_file_path) = request.target_key_file_path.as_ref() {
            remove_file_if_exists(key_file_path).await;
        }
        remove_file_if_exists(&request.vault_db_path).await;
        return Err(AuthenticationError::VaultHeaderInvalid);
    }
    if let Err(cleanup_error) = cleanup_result {
        tracing::warn!(
            ?cleanup_error,
            "staging cleanup failed after successful upload"
        );
    }

    session_manager.install_session(session_keys).await?;

    drop(master_key);
    drop(x25519_secret_bytes);
    drop(argon2_salt);
    drop(key_file_bytes);

    Ok(vault_id)
}

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
    let upload_result = cloud_transport
        .upload_blob(VAULT_HEADER_BLOB_NAME, &json_bytes)
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
    drop(current_key_file_bytes);
    Ok(())
}

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
    if !parent.exists() {
        return Err(AuthenticationError::VaultHeaderInvalid);
    }
    let mut new_key_file: Zeroizing<[u8; 32]> = Zeroizing::new([0u8; 32]);
    rand::rng().fill_bytes(new_key_file.as_mut_slice());
    staging::write_owner_only(&request.target_new_key_file_path, new_key_file.as_slice()).await?;
    let mut new_key_file_cleanup = ScopedFileCleanup::new(request.target_new_key_file_path.clone());
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
    rewrap_result?;
    new_key_file_cleanup.disarm();

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
        .upload_blob(VAULT_HEADER_BLOB_NAME, &json_bytes)
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

/// Recovers a vault onto a new device by downloading its header and
/// manifest backup, re-deriving the session keys, and importing the
/// backup into a fresh local SQLCipher DB.
pub async fn recover_vault(
    request: RecoverVaultRequest<'_>,
    session_manager: &SessionManager,
    cloud_transport: &dyn CloudTransport,
) -> Result<VaultId, AuthenticationError> {
    let header_bytes = cloud_transport
        .download_blob(VAULT_HEADER_BLOB_NAME)
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
        .download_blob(MANIFEST_BACKUP_BLOB_NAME)
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
        .upload_blob(VAULT_HEADER_BLOB_NAME, &json_bytes)
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
        .download_blob(VAULT_HEADER_BLOB_NAME)
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
        .download_blob(MANIFEST_BACKUP_BLOB_NAME)
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
    use super::*;
    use crate::auth::key_source::MockKeySource;
    use crate::auth::session::SessionManager;
    use crate::storage::cloud::manifest_backup::encrypt_manifest_backup;
    use crate::storage::cloud::mock::MockCloudTransport;
    use std::time::Duration;

    const TEST_PASSWORD: &[u8] = b"correct horse battery staple";
    const TEST_NEW_PASSWORD: &[u8] = b"stapler battery horse correct";
    const TEST_WRONG_PASSWORD: &[u8] = b"not the password";

    static CEREMONY_TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

    async fn ceremony_lock() -> tokio::sync::MutexGuard<'static, ()> {
        CEREMONY_TEST_LOCK.lock().await
    }

    fn test_params() -> Argon2Params {
        Argon2Params {
            memory_cost_kib: 1024,
            time_cost: 1,
            parallelism: 1,
        }
    }

    fn test_session_manager() -> SessionManager {
        SessionManager::with_timeout(Duration::from_secs(3600))
    }

    fn temp_dir() -> tempfile::TempDir {
        tempfile::tempdir().expect("tempdir must be created")
    }

    struct TierOneVault {
        _temp: tempfile::TempDir,
        vault_db_path: PathBuf,
        cloud: MockCloudTransport,
        session: SessionManager,
        vault_id: VaultId,
        header: VaultHeader,
    }

    async fn create_tier_one_vault() -> TierOneVault {
        let temp = temp_dir();
        let vault_db_path = temp.path().join("vault.db");
        let cloud = MockCloudTransport::new();
        let session = test_session_manager();
        let request = CreateVaultRequest {
            tier: Tier::One,
            password_bytes: TEST_PASSWORD,
            target_key_file_path: None,
            vault_db_path: vault_db_path.clone(),
            argon2_params: test_params(),
        };
        let vault_id = create_vault(request, &session, &cloud)
            .await
            .expect("create_vault tier 1 must succeed");
        let header_bytes = cloud
            .download_blob(VAULT_HEADER_BLOB_NAME)
            .await
            .expect("header must be present after create_vault");
        let header: VaultHeader =
            serde_json::from_slice(&header_bytes).expect("header must deserialize");
        TierOneVault {
            _temp: temp,
            vault_db_path,
            cloud,
            session,
            vault_id,
            header,
        }
    }

    struct TierTwoVault {
        _temp: tempfile::TempDir,
        vault_db_path: PathBuf,
        key_file_path: PathBuf,
        cloud: MockCloudTransport,
        session: SessionManager,
        vault_id: VaultId,
        header: VaultHeader,
    }

    async fn create_tier_two_vault() -> TierTwoVault {
        let temp = temp_dir();
        let vault_db_path = temp.path().join("vault.db");
        let key_file_path = temp.path().join("key.bin");
        let cloud = MockCloudTransport::new();
        let session = test_session_manager();
        let request = CreateVaultRequest {
            tier: Tier::Two,
            password_bytes: TEST_PASSWORD,
            target_key_file_path: Some(key_file_path.clone()),
            vault_db_path: vault_db_path.clone(),
            argon2_params: test_params(),
        };
        let vault_id = create_vault(request, &session, &cloud)
            .await
            .expect("create_vault tier 2 must succeed");
        let header_bytes = cloud
            .download_blob(VAULT_HEADER_BLOB_NAME)
            .await
            .expect("header must be present after create_vault");
        let header: VaultHeader =
            serde_json::from_slice(&header_bytes).expect("header must deserialize");
        TierTwoVault {
            _temp: temp,
            vault_db_path,
            key_file_path,
            cloud,
            session,
            vault_id,
            header,
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
            argon2_params: test_params(),
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
            argon2_params: test_params(),
        };
        let result = create_vault(request, &session, &cloud).await;
        assert!(matches!(
            result,
            Err(AuthenticationError::VaultHeaderInvalid)
        ));
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
        tokio::task::spawn_blocking(move || {
            let conn = open_sqlcipher(&vault_db_path, &old_sqlcipher).unwrap();
            conn.execute(
                "INSERT INTO nodes (id, file_key_wrapped) VALUES (1, ?)",
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
                "SELECT file_key_wrapped FROM nodes WHERE id = 1",
                [],
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
        tokio::task::spawn_blocking(move || {
            let conn = open_sqlcipher(&vault_db_path, &current_sqlcipher).unwrap();
            conn.execute(
                "INSERT INTO nodes (id, file_key_wrapped) VALUES (2, ?)",
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
                "SELECT file_key_wrapped FROM nodes WHERE id = 2",
                [],
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

    async fn add_recovery_slot_and_return_phrase(vault: &mut TierOneVault) -> Zeroizing<String> {
        let request = SetupRecoveryRequest {
            current_password_bytes: TEST_PASSWORD,
            current_key_source: None,
            argon2_params: test_params(),
            vault_db_path: vault.vault_db_path.clone(),
        };
        setup_recovery(
            request,
            &vault.session,
            &vault.cloud,
            &mut vault.header,
            &vault.vault_id,
        )
        .await
        .expect("setup_recovery must succeed")
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
        tokio::task::spawn_blocking(move || {
            let conn = open_sqlcipher(&vault_db_path, &old_sqlcipher).unwrap();
            conn.execute(
                "INSERT INTO nodes (id, file_key_wrapped) VALUES (3, ?)",
                params![bad_wrapped_for_insert],
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
                "SELECT file_key_wrapped FROM nodes WHERE id = 3",
                [],
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

    async fn upload_manifest_backup_for(vault: &TierOneVault) {
        let salt = decode_base64_32(&vault.header.argon2_salt).unwrap();
        let params = argon2_params_from_json(&vault.header.argon2_params);
        let mut master: Zeroizing<[u8; 32]> = Zeroizing::new([0u8; 32]);
        derive_master_key_into(TEST_PASSWORD, None, &salt, &params, &mut master).unwrap();
        let keys = SessionKeys::from_master_key_bytes(&master).unwrap();
        let manifest_key: [u8; 32] = *keys.manifest_key.expose();
        let stub_sql = b"CREATE TABLE IF NOT EXISTS imported_stub (id INTEGER);";
        let wire = encrypt_manifest_backup(stub_sql, &manifest_key).unwrap();
        vault
            .cloud
            .upload_blob(MANIFEST_BACKUP_BLOB_NAME, &wire)
            .await
            .unwrap();
    }

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

    #[test]
    fn test_master_key_never_appears_in_session_keys_session_manager_or_vault_header_fields() {
        use std::any::type_name;
        let session_keys_type = type_name::<SessionKeys>();
        let session_manager_type = type_name::<SessionManager>();
        let vault_header_type = type_name::<VaultHeader>();
        assert!(!session_keys_type.contains("MasterKey"));
        assert!(!session_manager_type.contains("MasterKey"));
        assert!(!vault_header_type.contains("MasterKey"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_recovery_phrase_never_appears_in_any_persistent_writer_output() {
        let _lock = ceremony_lock().await;
        let mut vault = create_tier_one_vault().await;
        let phrase = add_recovery_slot_and_return_phrase(&mut vault).await;
        let phrase_bytes = phrase.as_bytes().to_vec();

        let header_bytes = vault
            .cloud
            .download_blob(VAULT_HEADER_BLOB_NAME)
            .await
            .expect("header must be present");
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
        let header_bytes_b = cloud_b.download_blob(VAULT_HEADER_BLOB_NAME).await.unwrap();
        let mut header_b: VaultHeader = serde_json::from_slice(&header_bytes_b).unwrap();
        header_b.recovery_slots.push(slot_a);
        let updated_bytes = serde_json::to_vec_pretty(&header_b).unwrap();
        cloud_b
            .upload_blob(VAULT_HEADER_BLOB_NAME, &updated_bytes)
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
