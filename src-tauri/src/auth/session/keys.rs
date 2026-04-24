use std::sync::Arc;

use zeroize::Zeroizing;

use crate::auth::error::AuthenticationError;
use crate::auth::kdf::{Argon2Params, derive_master_key_into};
use crate::crypto::derive_vault_keys;
use crate::memory::SecureBytes;
use crate::storage::SqlCipherMetadataStore;

/// Holds all derived keys for the duration of an authenticated session.
pub(crate) struct SessionKeys {
    /// Key-encryption key used to wrap file keys.
    pub(crate) key_encryption_key: SecureBytes<32>,
    /// SQLCipher key used for the metadata database.
    pub(crate) sqlcipher_key: SecureBytes<32>,
    /// Manifest key used for manifest backup encryption.
    pub(crate) manifest_key: SecureBytes<32>,
    /// Opened vault metadata store (SQLCipher connection).
    /// Held for the duration of the session and dropped in `lock()`.
    pub(crate) metadata_store: Option<Arc<SqlCipherMetadataStore>>,
}

impl SessionKeys {
    /// Derives `master_key` via Argon2id and expands it into three locked
    /// vault-level keys.
    pub(crate) fn derive(
        password_utf8_bytes: &[u8],
        key_file_bytes: Option<&[u8; 32]>,
        salt: &[u8; 32],
        params: &Argon2Params,
    ) -> Result<Self, AuthenticationError> {
        let mut master_key: Zeroizing<[u8; 32]> = Zeroizing::new([0u8; 32]);
        derive_master_key_into(
            password_utf8_bytes,
            key_file_bytes,
            salt,
            params,
            &mut master_key,
        )?;
        Self::from_master_key_bytes(&master_key)
    }

    /// Expands a caller-owned `master_key` into the three vault-level keys
    /// via HKDF-SHA256. Used by ceremony flows where the raw master-key
    /// bytes are held in a `Zeroizing<[u8; 32]>` binding in the outer
    /// function scope (so they can also be passed to recovery-slot wrap
    /// primitives) and must not run Argon2id a second time.
    pub(crate) fn from_master_key_bytes(
        master_key_bytes: &[u8; 32],
    ) -> Result<Self, AuthenticationError> {
        let vault_keys = derive_vault_keys(master_key_bytes)
            .map_err(|_| AuthenticationError::InvalidCredentials)?;
        let mut key_encryption_key = SecureBytes::<32>::new()?;
        let mut sqlcipher_key = SecureBytes::<32>::new()?;
        let mut manifest_key = SecureBytes::<32>::new()?;

        vault_keys
            .key_encryption_key
            .with_exposed(|bytes| key_encryption_key.as_mut().copy_from_slice(bytes));
        vault_keys
            .sqlcipher_key
            .with_exposed(|bytes| sqlcipher_key.as_mut().copy_from_slice(bytes));
        vault_keys
            .manifest_key
            .with_exposed(|bytes| manifest_key.as_mut().copy_from_slice(bytes));

        Ok(Self {
            key_encryption_key,
            sqlcipher_key,
            manifest_key,
            metadata_store: None,
        })
    }

    /// Returns a reference to the opened metadata store, if available.
    ///
    /// The store is only available while the session is `Active`.
    /// Returns `None` if the database could not be opened or the session is not active.
    pub(crate) fn get_metadata_store(&self) -> Option<Arc<SqlCipherMetadataStore>> {
        self.metadata_store.as_ref().map(Arc::clone)
    }
}

#[cfg(test)]
mod tests {
    use crate::auth::kdf::Argon2Params;
    use crate::memory::platform::set_force_lock_failure;

    use super::SessionKeys;

    const TEST_PARAMS: Argon2Params = Argon2Params {
        memory_cost_kib: 1024,
        time_cost: 1,
        parallelism: 1,
    };
    const TEST_SALT: [u8; 32] = [0x44u8; 32];

    struct ForceLockFailureGuard;

    impl ForceLockFailureGuard {
        fn new() -> Self {
            set_force_lock_failure(true);
            Self
        }
    }

    impl Drop for ForceLockFailureGuard {
        fn drop(&mut self) {
            set_force_lock_failure(false);
        }
    }

    #[test]
    fn test_session_keys_derive_tier1_produces_three_distinct_keys() {
        let keys = SessionKeys::derive(b"password", None, &TEST_SALT, &TEST_PARAMS)
            .expect("derive must succeed");
        assert_ne!(
            keys.key_encryption_key.expose(),
            keys.sqlcipher_key.expose()
        );
        assert_ne!(keys.key_encryption_key.expose(), keys.manifest_key.expose());
        assert_ne!(keys.sqlcipher_key.expose(), keys.manifest_key.expose());
    }

    #[test]
    fn test_session_keys_derive_tier2_produces_three_distinct_keys() {
        let key_file = [0x77u8; 32];
        let keys = SessionKeys::derive(b"password", Some(&key_file), &TEST_SALT, &TEST_PARAMS)
            .expect("derive must succeed");
        assert_ne!(
            keys.key_encryption_key.expose(),
            keys.sqlcipher_key.expose()
        );
        assert_ne!(keys.key_encryption_key.expose(), keys.manifest_key.expose());
    }

    #[test]
    fn test_session_keys_derive_is_deterministic_for_same_inputs() {
        let first = SessionKeys::derive(b"password", None, &TEST_SALT, &TEST_PARAMS).unwrap();
        let second = SessionKeys::derive(b"password", None, &TEST_SALT, &TEST_PARAMS).unwrap();
        assert_eq!(
            first.key_encryption_key.expose(),
            second.key_encryption_key.expose()
        );
        assert_eq!(first.sqlcipher_key.expose(), second.sqlcipher_key.expose());
        assert_eq!(first.manifest_key.expose(), second.manifest_key.expose());
    }

    #[test]
    fn test_session_keys_derive_different_passwords_produce_different_key_encryption_keys() {
        let first = SessionKeys::derive(b"password-a", None, &TEST_SALT, &TEST_PARAMS).unwrap();
        let second = SessionKeys::derive(b"password-b", None, &TEST_SALT, &TEST_PARAMS).unwrap();
        assert_ne!(
            first.key_encryption_key.expose(),
            second.key_encryption_key.expose()
        );
    }

    #[test]
    fn test_session_keys_derive_different_key_files_produce_different_key_encryption_keys() {
        let first = SessionKeys::derive(b"password", Some(&[0x01u8; 32]), &TEST_SALT, &TEST_PARAMS)
            .unwrap();
        let second =
            SessionKeys::derive(b"password", Some(&[0x02u8; 32]), &TEST_SALT, &TEST_PARAMS)
                .unwrap();
        assert_ne!(
            first.key_encryption_key.expose(),
            second.key_encryption_key.expose()
        );
    }

    #[test]
    fn test_session_keys_tier1_and_tier2_produce_different_key_encryption_keys() {
        let key_file = [0x88u8; 32];
        let tier_one = SessionKeys::derive(b"password", None, &TEST_SALT, &TEST_PARAMS).unwrap();
        let tier_two =
            SessionKeys::derive(b"password", Some(&key_file), &TEST_SALT, &TEST_PARAMS).unwrap();
        assert_ne!(
            tier_one.key_encryption_key.expose(),
            tier_two.key_encryption_key.expose()
        );
    }

    #[test]
    fn test_session_keys_derive_returns_memory_lock_failed_when_lock_is_forced_to_fail() {
        let _guard = ForceLockFailureGuard::new();
        let result = SessionKeys::derive(b"password", None, &TEST_SALT, &TEST_PARAMS);
        let error = match result {
            Ok(_) => panic!("forced lock failure must propagate"),
            Err(error) => error,
        };
        let crate::auth::error::AuthenticationError::MemoryLockFailed(message) = error else {
            panic!("expected MemoryLockFailed variant, got {error:?}");
        };
        assert_eq!(message, expected_platform_failure_message());
    }

    #[test]
    fn test_session_keys_derive_with_non_ascii_utf8_password_succeeds_for_tier1_and_tier2() {
        let password = "påssw🔐rd漢字";
        let key_file = [0x5Au8; 32];

        let tier_one =
            SessionKeys::derive(password.as_bytes(), None, &TEST_SALT, &TEST_PARAMS).unwrap();
        let tier_two = SessionKeys::derive(
            password.as_bytes(),
            Some(&key_file),
            &TEST_SALT,
            &TEST_PARAMS,
        )
        .unwrap();

        assert_ne!(
            tier_one.key_encryption_key.expose(),
            tier_two.key_encryption_key.expose()
        );
    }

    #[test]
    fn test_session_keys_derive_with_empty_password_succeeds_for_tier1_and_tier2() {
        let key_file = [0xA5u8; 32];

        let tier_one = SessionKeys::derive(b"", None, &TEST_SALT, &TEST_PARAMS).unwrap();
        let tier_two = SessionKeys::derive(b"", Some(&key_file), &TEST_SALT, &TEST_PARAMS).unwrap();

        assert_ne!(
            tier_one.key_encryption_key.expose(),
            tier_two.key_encryption_key.expose()
        );
    }

    #[test]
    fn test_session_keys_from_master_key_bytes_matches_derive_result() {
        use zeroize::Zeroizing;

        use crate::auth::kdf::derive_master_key_into;

        let mut master_key_bytes: Zeroizing<[u8; 32]> = Zeroizing::new([0u8; 32]);
        derive_master_key_into(
            b"password",
            None,
            &TEST_SALT,
            &TEST_PARAMS,
            &mut master_key_bytes,
        )
        .expect("master key derive must succeed");

        let from_master = SessionKeys::from_master_key_bytes(&master_key_bytes)
            .expect("from_master_key_bytes must succeed");
        let from_derive = SessionKeys::derive(b"password", None, &TEST_SALT, &TEST_PARAMS)
            .expect("derive must succeed");

        assert_eq!(
            from_master.key_encryption_key.expose(),
            from_derive.key_encryption_key.expose(),
        );
        assert_eq!(
            from_master.sqlcipher_key.expose(),
            from_derive.sqlcipher_key.expose(),
        );
        assert_eq!(
            from_master.manifest_key.expose(),
            from_derive.manifest_key.expose(),
        );
    }

    #[cfg(target_os = "windows")]
    fn expected_platform_failure_message() -> String {
        String::from(
            "Cannot lock session keys in memory (system working set quota exceeded). Try closing other applications or restarting Arx Runa.",
        )
    }

    #[cfg(target_os = "linux")]
    fn expected_platform_failure_message() -> String {
        String::from(
            "Cannot lock memory. Increase the memory lock limit: `ulimit -l unlimited` or edit `/etc/security/limits.conf`.",
        )
    }

    #[cfg(target_os = "macos")]
    fn expected_platform_failure_message() -> String {
        String::from(
            "Cannot lock memory. Ensure sufficient physical RAM is available and try again.",
        )
    }
}
