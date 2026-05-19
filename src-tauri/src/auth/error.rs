//! Error types for the auth module.

use std::io;

use thiserror::Error;

use crate::memory::MemoryLockError;

/// Errors produced by the auth module.
#[non_exhaustive]
#[derive(Debug, Error)]
pub enum AuthenticationError {
    /// Authentication failed. Returned for wrong password, wrong key file,
    /// or both.
    #[error("authentication failed")]
    InvalidCredentials,

    /// No 32-byte file on the mounted volume matched the vault header's
    /// BLAKE3 fingerprint.
    #[error("key file not found on the device")]
    KeyFileNotFound,

    /// Memory locking failed while constructing session keys.
    #[error("cannot lock memory for session keys")]
    MemoryLockFailed(String),

    /// The vault header was missing, malformed, or failed integrity checks.
    #[error("vault header is missing or corrupt")]
    VaultHeaderInvalid,

    /// `authenticate()` was called while a session was already active.
    #[error("session is already active; call lock() before re-authenticating")]
    SessionAlreadyActive,

    /// A ceremony requiring an active session was called while no session
    /// was active.
    #[error("session is not active; authenticate first")]
    SessionNotActive,

    /// BIP-39 recovery phrase failed checksum or wordlist validation. This
    /// is returned before any Argon2id derivation runs so the error path is
    /// not timing-distinguishable from `InvalidCredentials`.
    #[error("recovery phrase checksum is invalid")]
    InvalidRecoveryPhrase,

    /// The vault header has no recovery slot configured.
    #[error("no recovery slot is configured for this vault")]
    NoRecoverySlot,

    /// A key-source operation failed.
    #[error(transparent)]
    KeySource(#[from] KeySourceError),
}

impl From<MemoryLockError> for AuthenticationError {
    fn from(error: MemoryLockError) -> Self {
        let MemoryLockError::PlatformFailure { platform_message } = error;
        Self::MemoryLockFailed(platform_message)
    }
}

impl AuthenticationError {
    /// Returns a fixed variant name string with no inner payload, safe to log.
    #[allow(unreachable_patterns)]
    pub fn kind_name(&self) -> &'static str {
        match self {
            Self::InvalidCredentials => "InvalidCredentials",
            Self::KeyFileNotFound => "KeyFileNotFound",
            Self::MemoryLockFailed(_) => "MemoryLockFailed",
            Self::VaultHeaderInvalid => "VaultHeaderInvalid",
            Self::SessionAlreadyActive => "SessionAlreadyActive",
            Self::SessionNotActive => "SessionNotActive",
            Self::InvalidRecoveryPhrase => "InvalidRecoveryPhrase",
            Self::NoRecoverySlot => "NoRecoverySlot",
            Self::KeySource(_) => "KeySource",
            _ => "Unknown",
        }
    }
}

/// Errors produced by a [`crate::auth::KeySource`] implementation.
#[non_exhaustive]
#[derive(Debug, Error)]
pub enum KeySourceError {
    /// The configured key-file path does not exist.
    #[error("key file not found")]
    NotFound,

    /// The file exists but is not exactly 32 bytes.
    #[error("key file has invalid size: {actual} bytes (expected 32)")]
    InvalidSize { actual: usize },

    /// An unrecoverable I/O error occurred while accessing key material or hints.
    #[error("I/O operation failed")]
    IoFailed(#[source] io::Error),
}

#[cfg(test)]
mod tests {
    use super::{AuthenticationError, KeySourceError};
    use crate::memory::MemoryLockError;

    #[test]
    fn test_authentication_error_from_key_source_converts_variant() {
        let error = AuthenticationError::from(KeySourceError::NotFound);

        let AuthenticationError::KeySource(KeySourceError::NotFound) = error else {
            panic!("expected key source wrapper");
        };
    }

    #[test]
    fn test_authentication_error_from_memory_lock_error_carries_platform_message() {
        let expected = String::from(
            "Cannot lock memory. Ensure sufficient physical RAM is available and try again.",
        );
        let error = AuthenticationError::from(MemoryLockError::PlatformFailure {
            platform_message: expected.clone(),
        });

        let AuthenticationError::MemoryLockFailed(actual) = error else {
            panic!("expected memory-lock wrapper");
        };
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_authentication_error_invalid_credentials_display_matches_design() {
        assert_eq!(
            AuthenticationError::InvalidCredentials.to_string(),
            "authentication failed",
        );
    }

    #[test]
    fn test_authentication_error_key_file_not_found_display_matches_design() {
        assert_eq!(
            AuthenticationError::KeyFileNotFound.to_string(),
            "key file not found on the device",
        );
    }

    #[test]
    fn test_authentication_error_vault_header_invalid_display_matches_design() {
        assert_eq!(
            AuthenticationError::VaultHeaderInvalid.to_string(),
            "vault header is missing or corrupt",
        );
    }

    #[test]
    fn test_authentication_error_session_already_active_display_matches_design() {
        assert_eq!(
            AuthenticationError::SessionAlreadyActive.to_string(),
            "session is already active; call lock() before re-authenticating",
        );
    }

    #[test]
    fn test_authentication_error_memory_lock_failed_display_matches_design() {
        let error = AuthenticationError::MemoryLockFailed(String::from(
            "Cannot lock memory. Ensure sufficient physical RAM is available and try again.",
        ));
        assert_eq!(error.to_string(), "cannot lock memory for session keys");
    }

    #[test]
    fn test_authentication_error_session_not_active_display_matches_design() {
        assert_eq!(
            AuthenticationError::SessionNotActive.to_string(),
            "session is not active; authenticate first",
        );
    }

    #[test]
    fn test_authentication_error_invalid_recovery_phrase_display_matches_design() {
        assert_eq!(
            AuthenticationError::InvalidRecoveryPhrase.to_string(),
            "recovery phrase checksum is invalid",
        );
    }

    #[test]
    fn test_authentication_error_no_recovery_slot_display_matches_design() {
        assert_eq!(
            AuthenticationError::NoRecoverySlot.to_string(),
            "no recovery slot is configured for this vault",
        );
    }

    /// Verifies that `kind_name()` returns a fixed string that never includes inner payload.
    #[test]
    fn test_authentication_error_kind_name_excludes_inner_payload() {
        let cases: &[(AuthenticationError, &str)] = &[
            (
                AuthenticationError::InvalidCredentials,
                "InvalidCredentials",
            ),
            (AuthenticationError::KeyFileNotFound, "KeyFileNotFound"),
            (
                AuthenticationError::MemoryLockFailed("secret platform text".into()),
                "MemoryLockFailed",
            ),
            (
                AuthenticationError::VaultHeaderInvalid,
                "VaultHeaderInvalid",
            ),
            (
                AuthenticationError::SessionAlreadyActive,
                "SessionAlreadyActive",
            ),
            (AuthenticationError::SessionNotActive, "SessionNotActive"),
            (
                AuthenticationError::InvalidRecoveryPhrase,
                "InvalidRecoveryPhrase",
            ),
            (AuthenticationError::NoRecoverySlot, "NoRecoverySlot"),
            (
                AuthenticationError::KeySource(KeySourceError::NotFound),
                "KeySource",
            ),
        ];
        for (error, expected) in cases {
            let name = error.kind_name();
            assert_eq!(name, *expected, "kind_name mismatch for {expected}");
            assert!(
                !name.contains("secret"),
                "inner payload must not appear in kind_name: {name}"
            );
        }
    }
}
