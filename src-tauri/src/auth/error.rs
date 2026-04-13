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
}
