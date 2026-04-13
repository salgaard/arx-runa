//! HKDF-SHA256 vault key derivation.

use crate::crypto::types::{KeyEncryptionKey, ManifestKey, SqlcipherKey};
use hkdf::Hkdf;
use secrecy::SecretBox;
use sha2::Sha256;

const HKDF_SALT: &[u8] = b"arx-runa-v1";
const HKDF_INFO_KEY_ENCRYPTION: &[u8] = b"arx-runa-key-encryption";
const HKDF_INFO_SQLCIPHER: &[u8] = b"arx-runa-sqlcipher";
const HKDF_INFO_MANIFEST_BACKUP: &[u8] = b"arx-runa-manifest-backup";

/// Vault-level keys derived from one master key.
pub struct VaultKeys {
    /// Key-encryption key used to wrap per-file keys.
    pub key_encryption_key: KeyEncryptionKey,
    /// SQLCipher database key.
    pub sqlcipher_key: SqlcipherKey,
    /// Manifest-backup encryption key.
    pub manifest_key: ManifestKey,
}

/// Derives vault keys from 32 bytes of master key material.
///
/// # Panics
/// Panics only if HKDF expansion fails, which cannot occur for 32-byte output
/// with SHA-256.
pub fn derive_vault_keys(master_key_bytes: &[u8; 32]) -> VaultKeys {
    let hkdf = Hkdf::<Sha256>::new(Some(HKDF_SALT), master_key_bytes);

    let key_encryption_key_secret_box = SecretBox::<[u8; 32]>::init_with_mut(|derived_key| {
        hkdf.expand(HKDF_INFO_KEY_ENCRYPTION, derived_key)
            .expect("HKDF expand must not fail for 32-byte output");
    });

    let sqlcipher_key_secret_box = SecretBox::<[u8; 32]>::init_with_mut(|derived_key| {
        hkdf.expand(HKDF_INFO_SQLCIPHER, derived_key)
            .expect("HKDF expand must not fail for 32-byte output");
    });

    let manifest_key_secret_box = SecretBox::<[u8; 32]>::init_with_mut(|derived_key| {
        hkdf.expand(HKDF_INFO_MANIFEST_BACKUP, derived_key)
            .expect("HKDF expand must not fail for 32-byte output");
    });

    VaultKeys {
        key_encryption_key: KeyEncryptionKey::from_secret_box(key_encryption_key_secret_box),
        sqlcipher_key: SqlcipherKey::from_secret_box(sqlcipher_key_secret_box),
        manifest_key: ManifestKey::from_secret_box(manifest_key_secret_box),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_derive_vault_keys_same_input_produces_same_output() {
        let master_key_bytes = [0x42u8; 32];
        let first = derive_vault_keys(&master_key_bytes);
        let second = derive_vault_keys(&master_key_bytes);

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
        let first = derive_vault_keys(&first_master_key_bytes);
        let second = derive_vault_keys(&second_master_key_bytes);

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
        let keys = derive_vault_keys(&master_key_bytes);

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
        let keys = derive_vault_keys(&master_key_bytes);

        assert_ne!(
            keys.key_encryption_key.expose(),
            keys.sqlcipher_key.expose()
        );
        assert_ne!(keys.key_encryption_key.expose(), keys.manifest_key.expose());
        assert_ne!(keys.sqlcipher_key.expose(), keys.manifest_key.expose());
    }
}
