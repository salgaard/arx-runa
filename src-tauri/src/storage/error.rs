//! Error types for the storage module.

use rusqlite::ErrorCode;
use thiserror::Error;

use crate::crypto::CryptoError;

/// Errors produced by the storage module.
#[non_exhaustive]
#[derive(Debug, Error)]
pub enum StorageError {
    /// A database operation failed for a non-classified reason.
    #[error("database operation failed: {0}")]
    Database(String),
    /// The requested record does not exist.
    #[error("record not found")]
    NotFound,
    /// A blob checksum verification failed.
    #[error("blob checksum mismatch")]
    ChecksumMismatch,
    /// A filesystem or database-open I/O operation failed.
    #[error("I/O operation failed: {0}")]
    Io(String),
    /// The SQLCipher key does not match the database.
    #[error("incorrect SQLCipher key for manifest database")]
    WrongKey,
    /// A database constraint was violated.
    #[error("constraint violation: {0}")]
    ConstraintViolation(String),
    /// The file is staged in the epoch buffer but has not yet been flushed to an encrypted blob.
    #[error("file {0} is pending epoch flush — call flush_epoch_buffer before downloading")]
    EpochBufferNotFlushed(uuid::Uuid),
}

impl StorageError {
    /// Maps `rusqlite` failures into storage-domain errors.
    pub(crate) fn from_rusqlite(error: rusqlite::Error) -> Self {
        match error {
            rusqlite::Error::SqliteFailure(sqlite_error, message) => {
                if sqlite_error.extended_code == rusqlite::ffi::SQLITE_NOTADB {
                    Self::WrongKey
                } else if sqlite_error.code == ErrorCode::ConstraintViolation {
                    Self::ConstraintViolation(
                        message.unwrap_or_else(|| "constraint violation".to_owned()),
                    )
                } else if sqlite_error.code == ErrorCode::CannotOpen {
                    Self::Io(message.unwrap_or_else(|| "unable to open database".to_owned()))
                } else {
                    Self::Database(message.unwrap_or_else(|| sqlite_error.to_string()))
                }
            }
            other => Self::Database(other.to_string()),
        }
    }

    /// Maps crypto-domain failures into storage-domain errors.
    pub(crate) fn from_crypto(error: CryptoError) -> Self {
        match error {
            CryptoError::ChecksumMismatch => Self::ChecksumMismatch,
            other => Self::Database(other.to_string()),
        }
    }
}

impl From<CryptoError> for StorageError {
    /// Converts crypto errors into storage errors via [`StorageError::from_crypto`].
    fn from(error: CryptoError) -> Self {
        Self::from_crypto(error)
    }
}

#[cfg(test)]
mod tests {
    use crate::crypto::CryptoError;

    use super::StorageError;

    /// Verifies SQLITE_NOTADB maps to `WrongKey`.
    #[test]
    fn test_from_rusqlite_sqlite_notadb_maps_to_wrong_key() {
        let error = rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error {
                code: rusqlite::ErrorCode::Unknown,
                extended_code: rusqlite::ffi::SQLITE_NOTADB,
            },
            Some("file is not a database".to_owned()),
        );

        assert!(matches!(
            StorageError::from_rusqlite(error),
            StorageError::WrongKey
        ));
    }

    /// Verifies constraint failures map to `ConstraintViolation`.
    #[test]
    fn test_from_rusqlite_constraint_maps_to_constraint_violation() {
        let error = rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error {
                code: rusqlite::ErrorCode::ConstraintViolation,
                extended_code: rusqlite::ffi::SQLITE_CONSTRAINT_UNIQUE,
            },
            Some("UNIQUE constraint failed".to_owned()),
        );

        assert!(matches!(
            StorageError::from_rusqlite(error),
            StorageError::ConstraintViolation(message) if message.contains("UNIQUE")
        ));
    }

    /// Verifies constraint failures without a message use the default text.
    #[test]
    fn test_from_rusqlite_constraint_without_message_uses_default() {
        let error = rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error {
                code: rusqlite::ErrorCode::ConstraintViolation,
                extended_code: rusqlite::ffi::SQLITE_CONSTRAINT,
            },
            None,
        );

        assert!(matches!(
            StorageError::from_rusqlite(error),
            StorageError::ConstraintViolation(message) if message == "constraint violation"
        ));
    }

    /// Verifies cannot-open failures map to `Io`.
    #[test]
    fn test_from_rusqlite_cantopen_maps_to_io() {
        let error = rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error {
                code: rusqlite::ErrorCode::CannotOpen,
                extended_code: rusqlite::ffi::SQLITE_CANTOPEN,
            },
            Some("unable to open database file".to_owned()),
        );

        assert!(matches!(
            StorageError::from_rusqlite(error),
            StorageError::Io(message) if message.contains("open")
        ));
    }

    /// Verifies cannot-open failures without a message use the default text.
    #[test]
    fn test_from_rusqlite_cantopen_without_message_uses_default() {
        let error = rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error {
                code: rusqlite::ErrorCode::CannotOpen,
                extended_code: rusqlite::ffi::SQLITE_CANTOPEN,
            },
            None,
        );

        assert!(matches!(
            StorageError::from_rusqlite(error),
            StorageError::Io(message) if message == "unable to open database"
        ));
    }

    /// Verifies unrelated SQLite failures map to `Database`.
    #[test]
    fn test_from_rusqlite_other_sqlite_failure_maps_to_database() {
        let error = rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error {
                code: rusqlite::ErrorCode::DatabaseCorrupt,
                extended_code: rusqlite::ffi::SQLITE_CORRUPT,
            },
            Some("database disk image is malformed".to_owned()),
        );

        assert!(matches!(
            StorageError::from_rusqlite(error),
            StorageError::Database(message) if message.contains("malformed")
        ));
    }

    /// Verifies non-SQLiteFailure rusqlite errors map to `Database`.
    #[test]
    fn test_from_rusqlite_non_sqlite_failure_maps_to_database() {
        let error = rusqlite::Error::InvalidQuery;

        assert!(matches!(
            StorageError::from_rusqlite(error),
            StorageError::Database(message) if message.contains("Query is not read-only")
                || !message.is_empty()
        ));
    }

    /// Verifies `ChecksumMismatch` is constructible and formats correctly.
    #[test]
    fn test_checksum_mismatch_variant_formats_expected_message() {
        let error = StorageError::ChecksumMismatch;

        assert_eq!(error.to_string(), "blob checksum mismatch");
    }

    /// Verifies crypto checksum mismatch maps to storage checksum mismatch.
    #[test]
    fn test_from_crypto_checksum_mismatch_maps_to_checksum_mismatch() {
        assert!(matches!(
            StorageError::from_crypto(CryptoError::ChecksumMismatch),
            StorageError::ChecksumMismatch
        ));
    }

    /// Verifies decryption failures map to storage database errors.
    #[test]
    fn test_from_crypto_decryption_failed_maps_to_database() {
        assert!(matches!(
            StorageError::from_crypto(CryptoError::DecryptionFailed),
            StorageError::Database(message) if message == "decryption failed: authentication tag mismatch"
        ));
    }

    /// Verifies encryption failures map to storage database errors.
    #[test]
    fn test_from_crypto_encryption_failed_maps_to_database() {
        assert!(matches!(
            StorageError::from_crypto(CryptoError::EncryptionFailed),
            StorageError::Database(message) if message == "chunk encryption failed"
        ));
    }

    /// Verifies invalid-blob errors map to storage database errors and preserve display text.
    #[test]
    fn test_from_crypto_invalid_blob_format_maps_to_database_preserves_display() {
        assert!(matches!(
            StorageError::from_crypto(CryptoError::InvalidBlobFormat { expected: 40, actual: 10 }),
            StorageError::Database(message) if message == "invalid blob format: expected at least 40 bytes, got 10"
        ));
    }

    /// Verifies key-wrap failures map to storage database errors.
    #[test]
    fn test_from_crypto_key_wrap_failed_maps_to_database() {
        assert!(matches!(
            StorageError::from_crypto(CryptoError::KeyWrapFailed),
            StorageError::Database(message) if message == "key wrap failed"
        ));
    }

    /// Verifies key-unwrap failures map to storage database errors.
    #[test]
    fn test_from_crypto_key_unwrap_failed_maps_to_database() {
        assert!(matches!(
            StorageError::from_crypto(CryptoError::KeyUnwrapFailed),
            StorageError::Database(message) if message == "key unwrap failed"
        ));
    }

    /// Verifies key-derivation failures map to storage database errors.
    #[test]
    fn test_from_crypto_key_derivation_failed_maps_to_database() {
        assert!(matches!(
            StorageError::from_crypto(CryptoError::KeyDerivationFailed),
            StorageError::Database(message) if message == "key derivation failed"
        ));
    }

    /// Verifies the `From<CryptoError>` implementation delegates to `from_crypto`.
    #[test]
    fn test_from_trait_delegates_to_from_crypto() {
        assert!(matches!(
            StorageError::from(CryptoError::ChecksumMismatch),
            StorageError::ChecksumMismatch
        ));
    }

    /// Verifies that `EpochBufferNotFlushed` formats correctly and includes the UUID.
    #[test]
    fn test_epoch_buffer_not_flushed_returns_correct_message() {
        let id = uuid::Uuid::parse_str("12345678-1234-4234-8234-123456789abc")
            .expect("test UUID must parse");
        let error = StorageError::EpochBufferNotFlushed(id);
        let message = error.to_string();
        assert!(
            message.contains("12345678-1234-4234-8234-123456789abc"),
            "error message must include the UUID: {message}"
        );
        assert!(
            message.contains("pending epoch flush"),
            "error message must mention pending epoch flush: {message}"
        );
    }
}
