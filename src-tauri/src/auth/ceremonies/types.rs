//! Ceremony request and enum types.

use std::path::PathBuf;

use crate::auth::kdf::Argon2Params;
use crate::auth::key_source::KeySource;

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
    pub(super) fn as_u8(self) -> u8 {
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
