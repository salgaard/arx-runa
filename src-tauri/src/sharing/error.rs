//! Error types for the sharing module.

use thiserror::Error;

/// Errors produced by sharing identity, contacts, and HPKE operations.
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
    /// HPKE open authentication failed (KEM decap, CTX mismatch, or stream decrypt).
    #[error("authentication failed")]
    AuthenticationFailed,
    /// Share package wire format is invalid.
    #[error("malformed share package: {0}")]
    MalformedSharePackage(String),
    /// JSON payload inside share package is invalid.
    #[error("invalid JSON payload: {0}")]
    InvalidJsonPayload(String),
    /// Decoded file key length is not 32 bytes.
    #[error("invalid file key length: expected 32 bytes, got {0}")]
    InvalidFileKeyLength(usize),
    /// Decoded sender public key length is not 32 bytes.
    #[error("invalid sender public key length: expected 32 bytes, got {0}")]
    InvalidSenderPublicKeyLength(usize),
    /// The requested received share does not exist.
    #[error("received share not found")]
    ReceivedShareNotFound,
    /// A cloud transport operation underpinning share creation, revocation, or fetch failed.
    #[error("sharing cloud operation failed: {0}")]
    CloudOperation(String),
    /// The active cloud destination does not support sharing.
    #[error("sharing not supported by this destination: {0}")]
    NotSupported(String),
    /// Revocation blob deletion stopped mid-loop; caller retries by re-invoking `revoke_share`.
    #[error("revocation partial: failed at blob index {failed_index}")]
    RevocationPartial { failed_index: usize },
    /// The requested outgoing share row does not exist.
    #[error("share not found")]
    ShareNotFound,
    /// A share cannot be revoked because it is already revoked.
    #[error("share already revoked")]
    ShareAlreadyRevoked,
    /// Strong revocation attempted on a file with no re-encryption candidates.
    #[error("no active shares to rotate for file_share_id")]
    NoActiveSharesForRotation,
    /// A received share package contains an invalid or untrusted field value.
    #[error("invalid share package")]
    InvalidSharePackage,
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

    /// Verifies `AuthenticationFailed` has the expected display text.
    #[test]
    fn test_sharing_error_authentication_failed_formats_expected_message() {
        assert_eq!(
            SharingError::AuthenticationFailed.to_string(),
            "authentication failed"
        );
    }

    /// Verifies `MalformedSharePackage` has the expected display text.
    #[test]
    fn test_sharing_error_malformed_share_package_formats_expected_message() {
        let error = SharingError::MalformedSharePackage("wire length < 64".to_owned());
        assert_eq!(
            error.to_string(),
            "malformed share package: wire length < 64"
        );
    }

    /// Verifies `InvalidJsonPayload` has the expected display text.
    #[test]
    fn test_sharing_error_invalid_json_payload_formats_expected_message() {
        let error = SharingError::InvalidJsonPayload("missing field".to_owned());
        assert_eq!(error.to_string(), "invalid JSON payload: missing field");
    }

    /// Verifies `InvalidFileKeyLength` has the expected display text.
    #[test]
    fn test_sharing_error_invalid_file_key_length_formats_expected_message() {
        let error = SharingError::InvalidFileKeyLength(31);
        assert_eq!(
            error.to_string(),
            "invalid file key length: expected 32 bytes, got 31"
        );
    }

    /// Verifies `InvalidSenderPublicKeyLength` has the expected display text.
    #[test]
    fn test_sharing_error_invalid_sender_public_key_length_formats_expected_message() {
        let error = SharingError::InvalidSenderPublicKeyLength(33);
        assert_eq!(
            error.to_string(),
            "invalid sender public key length: expected 32 bytes, got 33"
        );
    }

    /// Verifies `ReceivedShareNotFound` has the expected display text.
    #[test]
    fn test_sharing_error_received_share_not_found_formats_expected_message() {
        assert_eq!(
            SharingError::ReceivedShareNotFound.to_string(),
            "received share not found"
        );
    }

    /// Verifies `CloudOperation` has the expected display text.
    #[test]
    fn test_sharing_error_cloud_operation_formats_expected_message() {
        let error = SharingError::CloudOperation("transport timed out".to_owned());
        assert_eq!(
            error.to_string(),
            "sharing cloud operation failed: transport timed out"
        );
    }

    /// Verifies `RevocationPartial` has the expected display text.
    #[test]
    fn test_sharing_error_revocation_partial_formats_expected_message() {
        let error = SharingError::RevocationPartial { failed_index: 3 };
        assert_eq!(
            error.to_string(),
            "revocation partial: failed at blob index 3"
        );
    }

    /// Verifies `ShareNotFound` has the expected display text.
    #[test]
    fn test_sharing_error_share_not_found_formats_expected_message() {
        assert_eq!(SharingError::ShareNotFound.to_string(), "share not found");
    }

    /// Verifies `ShareAlreadyRevoked` has the expected display text.
    #[test]
    fn test_sharing_error_share_already_revoked_formats_expected_message() {
        assert_eq!(
            SharingError::ShareAlreadyRevoked.to_string(),
            "share already revoked"
        );
    }

    /// Verifies `NoActiveSharesForRotation` has the expected display text.
    #[test]
    fn test_sharing_error_no_active_shares_for_rotation_formats_expected_message() {
        assert_eq!(
            SharingError::NoActiveSharesForRotation.to_string(),
            "no active shares to rotate for file_share_id"
        );
    }
}
