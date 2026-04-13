//! HKDF-SHA256 vault key derivation.

use crate::crypto::error::CryptoError;
use crate::crypto::types::{KeyEncryptionKey, ManifestKey, SqlcipherKey};
use hkdf::Hkdf;
use secrecy::SecretBox;
use sha2::Sha256;

/// Fixed HKDF-SHA256 salt used for all vault-level derivations.
pub(crate) const HKDF_SALT: &[u8] = b"arx-runa-v1";

/// HKDF `info` string for the key-encryption key.
pub(crate) const HKDF_INFO_KEY_ENCRYPTION: &[u8] = b"arx-runa-key-encryption";

/// HKDF `info` string for the SQLCipher DB key.
pub(crate) const HKDF_INFO_SQLCIPHER: &[u8] = b"arx-runa-sqlcipher";

/// HKDF `info` string for the manifest-backup key.
pub(crate) const HKDF_INFO_MANIFEST_BACKUP: &[u8] = b"arx-runa-manifest-backup";

/// Vault-level keys derived from one master key.
pub struct VaultKeys {
    /// Key-encryption key used to wrap per-file keys.
    pub key_encryption_key: KeyEncryptionKey,
    /// SQLCipher database key.
    pub sqlcipher_key: SqlcipherKey,
    /// Manifest-backup encryption key.
    pub manifest_key: ManifestKey,
}

/// Runs a single HKDF-SHA256 extract/expand into a caller-provided buffer.
///
/// # Errors
/// Returns `CryptoError::KeyDerivationFailed` if HKDF expansion fails.
pub(crate) fn expand_vault_key_into(
    master_key_bytes: &[u8; 32],
    info: &[u8],
    output: &mut [u8; 32],
) -> Result<(), CryptoError> {
    let hkdf = Hkdf::<Sha256>::new(Some(HKDF_SALT), master_key_bytes);
    hkdf.expand(info, output)
        .map_err(|_| CryptoError::KeyDerivationFailed)
}

/// Derives vault keys from 32 bytes of master key material.
///
/// # Errors
/// Returns `CryptoError::KeyDerivationFailed` if HKDF expansion fails.
pub fn derive_vault_keys(master_key_bytes: &[u8; 32]) -> Result<VaultKeys, CryptoError> {
    Ok(VaultKeys {
        key_encryption_key: KeyEncryptionKey::from_secret_box(expand_into_secret_box(
            master_key_bytes,
            HKDF_INFO_KEY_ENCRYPTION,
        )?),
        sqlcipher_key: SqlcipherKey::from_secret_box(expand_into_secret_box(
            master_key_bytes,
            HKDF_INFO_SQLCIPHER,
        )?),
        manifest_key: ManifestKey::from_secret_box(expand_into_secret_box(
            master_key_bytes,
            HKDF_INFO_MANIFEST_BACKUP,
        )?),
    })
}

/// Runs HKDF-SHA256 expand into a fresh `SecretBox` heap buffer.
fn expand_into_secret_box(
    master_key_bytes: &[u8; 32],
    info: &[u8],
) -> Result<SecretBox<[u8; 32]>, CryptoError> {
    let mut expand_result = Ok(());
    let secret_box = SecretBox::<[u8; 32]>::init_with_mut(|buffer| {
        expand_result = expand_vault_key_into(master_key_bytes, info, buffer);
    });
    expand_result.map(|()| secret_box)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_derive_vault_keys_same_input_produces_same_output() {
        let master_key_bytes = [0x42u8; 32];
        let first = derive_vault_keys(&master_key_bytes).expect("derive must succeed");
        let second = derive_vault_keys(&master_key_bytes).expect("derive must succeed");

        assert_eq!(
            first.key_encryption_key.expose(),
            second.key_encryption_key.expose()
        );
        assert_eq!(first.sqlcipher_key.expose(), second.sqlcipher_key.expose());
        assert_eq!(first.manifest_key.expose(), second.manifest_key.expose());
    }

    #[test]
    fn test_derive_vault_keys_different_inputs_produce_different_outputs() {
        let first_master_key_bytes = [0x01u8; 32];
        let second_master_key_bytes = [0x02u8; 32];
        let first = derive_vault_keys(&first_master_key_bytes).expect("derive must succeed");
        let second = derive_vault_keys(&second_master_key_bytes).expect("derive must succeed");

        assert_ne!(
            first.key_encryption_key.expose(),
            second.key_encryption_key.expose()
        );
        assert_ne!(first.sqlcipher_key.expose(), second.sqlcipher_key.expose());
        assert_ne!(first.manifest_key.expose(), second.manifest_key.expose());
    }

    #[test]
    fn test_derive_vault_keys_single_input_produces_distinct_keys() {
        let master_key_bytes = [0xA5u8; 32];
        let keys = derive_vault_keys(&master_key_bytes).expect("derive must succeed");

        assert_ne!(
            keys.key_encryption_key.expose(),
            keys.sqlcipher_key.expose()
        );
        assert_ne!(keys.key_encryption_key.expose(), keys.manifest_key.expose());
        assert_ne!(keys.sqlcipher_key.expose(), keys.manifest_key.expose());
    }

    #[test]
    fn test_derive_vault_keys_all_zero_master_key_succeeds() {
        let master_key_bytes = [0u8; 32];
        let keys = derive_vault_keys(&master_key_bytes).expect("derive must succeed");

        assert_ne!(
            keys.key_encryption_key.expose(),
            keys.sqlcipher_key.expose()
        );
        assert_ne!(keys.key_encryption_key.expose(), keys.manifest_key.expose());
        assert_ne!(keys.sqlcipher_key.expose(), keys.manifest_key.expose());
    }

    #[test]
    fn test_expand_vault_key_into_matches_derive_vault_keys_output() {
        let master_key_bytes = [0x77u8; 32];
        let vault_keys = derive_vault_keys(&master_key_bytes).expect("derive must succeed");
        let mut output = [0u8; 32];
        expand_vault_key_into(&master_key_bytes, HKDF_INFO_KEY_ENCRYPTION, &mut output)
            .expect("expand must succeed");
        assert_eq!(&output, vault_keys.key_encryption_key.expose());
    }
}
