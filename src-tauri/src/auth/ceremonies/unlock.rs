use std::path::Path;
use std::path::PathBuf;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64_STANDARD;

use crate::auth::error::AuthenticationError;
use crate::auth::kdf::Argon2Params;
use crate::auth::key_source::FileKeySource;
use crate::auth::session::SessionManager;
use crate::storage::cloud::vault_header::VaultHeader;

/// Reads the vault header at `header_path`, derives keys from `password_utf8_bytes`
/// (and optionally `key_file_path` for Tier 2), and installs the resulting session
/// via `session_manager.authenticate()`.
///
/// Returns the vault identifier embedded in the header on success.
///
/// # Errors
/// - [`AuthenticationError::VaultHeaderInvalid`] — header missing, malformed, or corrupt salt.
/// - [`AuthenticationError::KeyFileNotFound`] — Tier 2 vault but no key file provided.
/// - [`AuthenticationError::InvalidCredentials`] — wrong password or key file.
/// - [`AuthenticationError::MemoryLockFailed`] — platform memory-lock failure.
/// - [`AuthenticationError::SessionAlreadyActive`] — a session is already active.
pub async fn unlock_vault(
    password_utf8_bytes: &[u8],
    key_file_path: Option<PathBuf>,
    header_path: &Path,
    session_manager: &SessionManager,
) -> Result<String, AuthenticationError> {
    let header_json = tokio::fs::read_to_string(header_path)
        .await
        .map_err(|_| AuthenticationError::VaultHeaderInvalid)?;
    let header: VaultHeader =
        serde_json::from_str(&header_json).map_err(|_| AuthenticationError::VaultHeaderInvalid)?;

    let salt_bytes = B64_STANDARD
        .decode(&header.argon2_salt)
        .map_err(|_| AuthenticationError::VaultHeaderInvalid)?;
    let salt: [u8; 32] = salt_bytes
        .try_into()
        .map_err(|_| AuthenticationError::VaultHeaderInvalid)?;

    let params = Argon2Params {
        memory_cost_kib: header.argon2_params.memory_cost,
        time_cost: header.argon2_params.time_cost,
        parallelism: header.argon2_params.parallelism,
    };

    let key_source_opt: Option<FileKeySource> = if header.tier == 2 {
        let path = key_file_path.ok_or(AuthenticationError::KeyFileNotFound)?;
        Some(FileKeySource::new(path))
    } else {
        None
    };
    let key_source_ref: Option<&(dyn crate::auth::KeySource + Send + Sync)> = key_source_opt
        .as_ref()
        .map(|ks| ks as &(dyn crate::auth::KeySource + Send + Sync));

    let vault_id = header.vault_id.clone();
    session_manager
        .authenticate(
            password_utf8_bytes,
            key_source_ref,
            &salt,
            &params,
            vault_id.clone(),
        )
        .await?;

    Ok(vault_id)
}
