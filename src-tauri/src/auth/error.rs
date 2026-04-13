//! Error types for the auth module.

use std::io;

use thiserror::Error;

/// Errors produced by the auth module.
#[non_exhaustive]
#[derive(Debug, Error)]
pub enum AuthError {
    /// A key-source operation failed.
    #[error(transparent)]
    KeySource(#[from] KeySourceError),
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
    use super::{AuthError, KeySourceError};

    #[test]
    fn test_auth_error_from_key_source_converts_variant() {
        let error = AuthError::from(KeySourceError::NotFound);

        let AuthError::KeySource(KeySourceError::NotFound) = error else {
            panic!("expected key source wrapper");
        };
    }
}
