//! Ceremony request and enum types.

use std::path::PathBuf;

use crate::auth::kdf::Argon2Params;
use crate::auth::key_source::KeySource;
use crate::storage::cloud::destination_session::DestinationSession;
use crate::storage::cloud::vault_header::VaultHeader;

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

/// Intent for Argon2-parameter handling in existing-vault ceremonies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Argon2MigrationIntent {
    /// Preserve trusted parameters from the existing vault header.
    #[default]
    PreserveTrusted,
    /// Explicitly migrate to the requested parameters.
    MigrateToRequested,
}

/// Request payload for [`create_vault`].
pub struct CreateVaultRequest<'a> {
    /// Optional pre-allocated vault UUID.
    ///
    /// When `Some`, the ceremony uses this UUID as the vault identifier instead
    /// of generating a new one. Callers that pre-create the vault directory
    /// should supply this so the directory name always matches the vault_id and
    /// no post-ceremony rename is required.
    pub suggested_vault_id: Option<uuid::Uuid>,
    /// Chosen authentication tier.
    pub tier: Tier,
    /// UTF-8 password bytes entered by the user.
    pub password_bytes: &'a [u8],
    /// Destination path for the generated key file; `Some` iff [`Tier::Two`].
    /// The file must not already exist.
    pub target_key_file_path: Option<PathBuf>,
    /// Destination path for the SQLCipher vault database file.
    pub vault_db_path: PathBuf,
    /// Argon2id cost parameters.
    pub argon2_params: Argon2Params,
    /// Immutable chunk size configured for this vault.
    pub chunk_size_bytes: u64,
    /// Enables epoch buffer routing for sub-chunk files when true.
    pub epoch_buffer_enabled: bool,
    /// Optional human-readable vault name written into the vault header.
    pub vault_name: Option<String>,
    /// Primary destination session to insert into the vault DB during the ceremony,
    /// before the session is installed. When `Some`, destination insertion is atomic
    /// with vault creation: the session becomes `Active` only if insertion succeeds.
    pub primary_destination: Option<DestinationSession>,
}

impl<'a> CreateVaultRequest<'a> {
    /// Default per-vault chunk size (4 MiB).
    pub const DEFAULT_CHUNK_SIZE_BYTES: u64 = 4 * 1024 * 1024;
    /// Default epoch buffer flag.
    pub const DEFAULT_EPOCH_BUFFER_ENABLED: bool = false;

    /// Creates a request using default chunk and epoch-buffer settings.
    pub fn with_defaults(
        tier: Tier,
        password_bytes: &'a [u8],
        target_key_file_path: Option<PathBuf>,
        vault_db_path: PathBuf,
        argon2_params: Argon2Params,
    ) -> Self {
        Self {
            suggested_vault_id: None,
            tier,
            password_bytes,
            target_key_file_path,
            vault_db_path,
            argon2_params,
            chunk_size_bytes: Self::DEFAULT_CHUNK_SIZE_BYTES,
            epoch_buffer_enabled: Self::DEFAULT_EPOCH_BUFFER_ENABLED,
            vault_name: None,
            primary_destination: None,
        }
    }
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
    pub recovery_phrase: Option<&'a [u8]>,
    /// Argon2id cost parameters for the new slot.
    pub argon2_params: Argon2Params,
    /// Argon2 parameter migration intent.
    pub argon2_migration_intent: Argon2MigrationIntent,
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
    /// The file must not already exist.
    pub target_new_key_file_path: PathBuf,
    /// Optional recovery phrase to re-wrap the recovery slot.
    pub recovery_phrase: Option<&'a [u8]>,
    /// Argon2id cost parameters.
    pub argon2_params: Argon2Params,
    /// Argon2 parameter migration intent.
    pub argon2_migration_intent: Argon2MigrationIntent,
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
    /// Argon2 parameter migration intent.
    pub argon2_migration_intent: Argon2MigrationIntent,
    /// Vault database path (used to verify current credentials).
    pub vault_db_path: PathBuf,
}

/// Request payload for [`recover_with_phrase`].
pub struct RecoverWithPhraseRequest<'a> {
    /// BIP-39 recovery phrase entered by the user.
    pub phrase: &'a [u8],
    /// Destination path for the recovered vault DB.
    pub vault_db_path: PathBuf,
    /// New password bytes to re-key the vault to after phrase recovery.
    pub new_password_bytes: &'a [u8],
    /// Destination path for a newly generated key file (Tier 2 USB key loss only).
    /// Must be `None` for Tier 1 vaults; must be `Some` for Tier 2 vaults.
    pub new_key_file_path: Option<PathBuf>,
    /// Argon2id cost parameters for the new primary slot.
    pub argon2_params: Argon2Params,
    /// Argon2 parameter migration intent.
    pub argon2_migration_intent: Argon2MigrationIntent,
    /// Pre-loaded local vault header. When `Some`, the ceremony skips the cloud
    /// header download and uses this directly. Required when cloud transport is
    /// unavailable (e.g. pre-auth recovery from `LoginPage`).
    pub vault_header: Option<VaultHeader>,
}

/// Type of operation that was interrupted and pending recovery.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq)]
pub enum PendingOperation {
    /// Password change operation was interrupted.
    ChangePassword,
    /// Key file rotation operation was interrupted.
    RotateKeyFile,
}

/// Artifact written to disk when a vault ceremony is interrupted mid-operation.
///
/// This structure is serialized to `pending-vault-header.json` in the config directory.
/// The artifact is plaintext to allow recovery even if decryption keys are lost.
/// After recovery or manual intervention, the file must be explicitly deleted.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct PendingVaultHeader {
    /// The vault identifier being modified.
    pub vault_id: String,
    /// The type of operation that was interrupted.
    pub operation: PendingOperation,
    /// Serialized vault header (plaintext; can be re-downloaded if lost).
    pub vault_header_json: String,
    /// Timestamp when this artifact was written.
    pub created_at: std::time::SystemTime,
}
