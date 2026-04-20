//! Error types for the sharing module.

use thiserror::Error;

/// Errors produced by sharing identity and contacts operations.
#[non_exhaustive]
#[derive(Debug, Error)]
pub enum SharingError {
    /// The vault identity row does not exist.
    #[error("identity not initialised: vault_identity row missing")]
    IdentityMissing,
    /// The requested contact does not exist.
    #[error("contact not found")]
    ContactNotFound,
    /// A contact-specific constraint failed.
    #[error("contact constraint violation: {0}")]
    ConstraintViolation(String),
    /// The identity public key length is not 32 bytes.
    #[error("invalid X25519 public key length: expected 32 bytes, got {0}")]
    InvalidPublicKeyLength(usize),
    /// A persisted contact identifier is invalid.
    #[error("invalid contact identifier: {0}")]
    InvalidContactId(String),
    /// Display-name validation failed.
    #[error("display name must not be empty")]
    EmptyDisplayName,
    /// Backend storage failed with a non-classified error.
    #[error("sharing storage backend error: {0}")]
    Backend(String),
}

#[cfg(test)]
mod tests {
    use crate::sharing::SharingError;

    /// Verifies `IdentityMissing` has the expected display text.
    #[test]
    fn test_sharing_error_identity_missing_formats_expected_message() {
        assert_eq!(
            SharingError::IdentityMissing.to_string(),
            "identity not initialised: vault_identity row missing"
        );
    }

    /// Verifies `ContactNotFound` has the expected display text.
    #[test]
    fn test_sharing_error_contact_not_found_formats_expected_message() {
        assert_eq!(
            SharingError::ContactNotFound.to_string(),
            "contact not found"
        );
    }

    /// Verifies `ConstraintViolation` has the expected display text.
    #[test]
    fn test_sharing_error_constraint_violation_formats_expected_message() {
        let error = SharingError::ConstraintViolation("duplicate key".to_owned());
        assert_eq!(
            error.to_string(),
            "contact constraint violation: duplicate key"
        );
    }

    /// Verifies `InvalidPublicKeyLength` has the expected display text.
    #[test]
    fn test_sharing_error_invalid_public_key_length_formats_expected_message() {
        let error = SharingError::InvalidPublicKeyLength(31);
        assert_eq!(
            error.to_string(),
            "invalid X25519 public key length: expected 32 bytes, got 31"
        );
    }

    /// Verifies `InvalidContactId` has the expected display text.
    #[test]
    fn test_sharing_error_invalid_contact_id_formats_expected_message() {
        let error = SharingError::InvalidContactId("bad-uuid".to_owned());
        assert_eq!(error.to_string(), "invalid contact identifier: bad-uuid");
    }

    /// Verifies `EmptyDisplayName` has the expected display text.
    #[test]
    fn test_sharing_error_empty_display_name_formats_expected_message() {
        assert_eq!(
            SharingError::EmptyDisplayName.to_string(),
            "display name must not be empty"
        );
    }

    /// Verifies `Backend` has the expected display text.
    #[test]
    fn test_sharing_error_backend_formats_expected_message() {
        let error = SharingError::Backend("database operation failed".to_owned());
        assert_eq!(
            error.to_string(),
            "sharing storage backend error: database operation failed"
        );
    }
}
