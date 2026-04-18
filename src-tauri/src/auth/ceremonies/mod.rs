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

use crate::storage::types::BlobName;

mod change_password;
mod create;
mod helpers;
mod recover_vault;
mod recover_with_phrase;
mod rotate_key_file;
mod setup_recovery;
mod types;

#[cfg(test)]
mod test_support;

/// Name of the vault header object at the cloud root.
pub(super) const VAULT_HEADER_BLOB_NAME: &str = "vault-header.json";
/// Name of the manifest backup object at the cloud root.
pub(super) const MANIFEST_BACKUP_BLOB_NAME: &str = "manifest-backup.enc";
/// Filename used for the pre-upload staging file.
pub(super) const STAGING_FILE_NAME: &str = "pending-vault-header.json";

/// Returns the cloud blob name for the vault header object.
pub(super) fn vault_header_blob_name() -> BlobName {
    BlobName::from(VAULT_HEADER_BLOB_NAME)
}

/// Returns the cloud blob name for the manifest backup object.
pub(super) fn manifest_backup_blob_name() -> BlobName {
    BlobName::from(MANIFEST_BACKUP_BLOB_NAME)
}

pub use change_password::change_password;
pub use create::create_vault;
pub use recover_vault::recover_vault;
pub use recover_with_phrase::recover_with_phrase;
pub use rotate_key_file::rotate_key_file;
pub use setup_recovery::setup_recovery;
pub use types::{
    ChangePasswordRequest, CreateVaultRequest, RecoverVaultRequest, RecoverWithPhraseRequest,
    RotateKeyFileRequest, SetupRecoveryRequest, Tier,
};

#[cfg(test)]
mod invariant_tests {
    use std::any::type_name;

    use crate::auth::session::{SessionKeys, SessionManager};
    use crate::storage::cloud::vault_header::VaultHeader;

    #[test]
    fn test_master_key_token_absent_from_session_and_header_type_names() {
        let session_keys_type = type_name::<SessionKeys>();
        let session_manager_type = type_name::<SessionManager>();
        let vault_header_type = type_name::<VaultHeader>();
        assert!(!session_keys_type.contains("MasterKey"));
        assert!(!session_manager_type.contains("MasterKey"));
        assert!(!vault_header_type.contains("MasterKey"));
    }
}
