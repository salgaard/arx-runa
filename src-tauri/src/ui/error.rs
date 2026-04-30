//! Sanitised IPC error type for the Arx Runa Tauri command surface.
//!
//! All error variants carry user-safe, fixed English strings. No paths, key material,
//! or internal error text are ever included in outbound messages. Sensitive details are
//! logged server-side with `tracing::error!`.

use serde::Serialize;
use thiserror::Error;

/// Errors returned to the frontend over the Tauri IPC boundary.
///
/// Every variant carries a user-safe message string. Internal details are logged
/// server-side via `tracing::error!` and never included in the message.
#[derive(Debug, Serialize, Error)]
#[serde(tag = "kind", content = "message")]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub enum IpcError {
    /// The vault is locked and requires authentication.
    #[error("Vault is locked: {0}")]
    VaultLocked(String),
    /// Authentication failed (wrong password, wrong key file, or HPKE mismatch).
    #[error("Authentication failed: {0}")]
    AuthenticationFailed(String),
    /// The requested resource was not found.
    #[error("Not found: {0}")]
    NotFound(String),
    /// A record with the given identifier already exists.
    #[error("Already exists: {0}")]
    AlreadyExists(String),
    /// A cloud storage operation failed.
    #[error("Cloud error: {0}")]
    CloudError(String),
    /// The request contained invalid or missing input.
    #[error("Invalid input: {0}")]
    InvalidInput(String),
    /// An unexpected internal error occurred.
    #[error("Internal error: {0}")]
    InternalError(String),
    /// The file is staged in the epoch buffer and cannot be downloaded until flushed.
    #[error("Pending flush: {0}")]
    PendingFlush(String),
}

#[allow(unreachable_patterns)]
impl From<crate::auth::AuthenticationError> for IpcError {
    /// Maps authentication errors to sanitised IPC error variants without leaking internal details.
    fn from(error: crate::auth::AuthenticationError) -> Self {
        tracing::error!("auth error: {:?}", error);
        use crate::auth::AuthenticationError as A;
        match error {
            A::InvalidCredentials => IpcError::AuthenticationFailed("Invalid credentials".into()),
            A::KeyFileNotFound => IpcError::AuthenticationFailed("Key file not found".into()),
            A::MemoryLockFailed(_) => {
                IpcError::InternalError("Cannot lock memory for session keys".into())
            }
            A::VaultHeaderInvalid => IpcError::InternalError("Vault configuration error".into()),
            A::SessionAlreadyActive => {
                IpcError::InvalidInput("A session is already active; lock it first".into())
            }
            A::SessionNotActive => IpcError::VaultLocked("No active session".into()),
            A::InvalidRecoveryPhrase => IpcError::InvalidInput("Recovery phrase is invalid".into()),
            A::NoRecoverySlot => {
                IpcError::InvalidInput("No recovery slot configured for this vault".into())
            }
            A::KeySource(_) => {
                IpcError::AuthenticationFailed("Key file is missing or invalid".into())
            }
            _ => IpcError::InternalError("An error occurred".into()),
        }
    }
}

#[allow(unreachable_patterns)]
impl From<crate::storage::StorageError> for IpcError {
    /// Maps storage errors to sanitised IPC error variants without leaking internal details.
    fn from(error: crate::storage::StorageError) -> Self {
        tracing::error!("storage error: {:?}", error);
        use crate::storage::StorageError as S;
        match error {
            S::NotFound => IpcError::NotFound("File or directory not found".into()),
            S::ChecksumMismatch => IpcError::InternalError("Data integrity error".into()),
            S::WrongKey => IpcError::AuthenticationFailed("Vault database key mismatch".into()),
            S::ConstraintViolation(_) => {
                IpcError::AlreadyExists("A record with this identifier already exists".into())
            }
            S::EpochBufferNotFlushed(_) => IpcError::PendingFlush(
                "File is pending encryption — flush the epoch buffer first".into(),
            ),
            S::Database(_) | S::Io(_) => IpcError::InternalError("An error occurred".into()),
            _ => IpcError::InternalError("An error occurred".into()),
        }
    }
}

#[allow(unreachable_patterns)]
impl From<crate::sharing::SharingError> for IpcError {
    /// Maps sharing errors to sanitised IPC error variants without leaking internal details.
    fn from(error: crate::sharing::SharingError) -> Self {
        tracing::error!("sharing error: {:?}", error);
        use crate::sharing::SharingError as Sh;
        match error {
            Sh::AuthenticationFailed => {
                IpcError::AuthenticationFailed("Share authentication failed".into())
            }
            Sh::ContactNotFound | Sh::ShareNotFound | Sh::ReceivedShareNotFound => {
                IpcError::NotFound("Share record not found".into())
            }
            Sh::ShareAlreadyRevoked | Sh::NoActiveSharesForRotation => {
                IpcError::InvalidInput("Share cannot be revoked in its current state".into())
            }
            Sh::CloudOperation(_) | Sh::RevocationPartial { .. } => {
                IpcError::CloudError("Cloud share operation failed".into())
            }
            Sh::MalformedSharePackage(_)
            | Sh::InvalidJsonPayload(_)
            | Sh::InvalidFileKeyLength(_)
            | Sh::InvalidSenderPublicKeyLength(_)
            | Sh::InvalidPublicKeyLength(_)
            | Sh::InvalidSharePackage => {
                IpcError::InvalidInput("Share package is malformed".into())
            }
            Sh::EmptyDisplayName => IpcError::InvalidInput("Display name is required".into()),
            Sh::InvalidContactId(_) => {
                IpcError::InvalidInput("Contact identifier is invalid".into())
            }
            Sh::ConstraintViolation(_) => {
                IpcError::AlreadyExists("A sharing record already exists".into())
            }
            Sh::IdentityMissing | Sh::Backend(_) => {
                IpcError::InternalError("An error occurred".into())
            }
            _ => IpcError::InternalError("An error occurred".into()),
        }
    }
}

#[allow(unreachable_patterns)]
impl From<crate::storage::SyncError> for IpcError {
    /// Maps sync errors to sanitised IPC error variants without leaking internal details.
    fn from(error: crate::storage::SyncError) -> Self {
        tracing::error!("sync error: {:?}", error);
        use crate::storage::SyncError as Sy;
        match error {
            Sy::Conflict(_) => IpcError::CloudError("Cloud snapshot conflict".into()),
            Sy::Transport { .. }
            | Sy::CloudManifestUnreadable { .. }
            | Sy::PushUploadFailed { .. }
            | Sy::PushManifestBackupFailed { .. }
            | Sy::VaultHeaderUploadFailed { .. }
            | Sy::PullIncomplete { .. } => IpcError::CloudError("Cloud operation failed".into()),
            Sy::Storage { source } => IpcError::from(source),
            Sy::Io(_) | Sy::ManifestBackup { .. } | Sy::RollbackFailed { .. } => {
                IpcError::InternalError("An error occurred".into())
            }
            _ => IpcError::InternalError("An error occurred".into()),
        }
    }
}

impl From<crate::storage::CloudTransportError> for IpcError {
    /// Maps cloud transport errors to sanitised IPC error variants without leaking internal details.
    fn from(error: crate::storage::CloudTransportError) -> Self {
        tracing::error!("cloud transport error: {:?}", error);
        use crate::storage::CloudTransportError as C;
        match error {
            C::NotFound => IpcError::NotFound("Cloud blob not found".into()),
            C::AuthenticationFailed => IpcError::CloudError("Cloud authentication failed".into()),
            C::Timeout | C::IoError(_) | C::RcloneProcessFailed { .. } | C::Other(_) => {
                IpcError::CloudError("Cloud operation failed".into())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies the IpcError serde shape produces `{"kind": "...", "message": "..."}`.
    #[test]
    fn test_ipc_error_serialises_to_expected_shape() {
        let err = IpcError::NotFound("File or directory not found".into());
        let value = serde_json::to_value(&err).expect("serialisation must succeed");
        assert_eq!(value["kind"], "notFound");
        assert_eq!(value["message"], "File or directory not found");
    }

    /// Verifies each IpcError variant serialises with a camelCase kind field.
    #[test]
    fn test_ipc_error_all_variants_serialise_with_camel_case_kind() {
        let cases = [
            (IpcError::VaultLocked("x".into()), "vaultLocked"),
            (
                IpcError::AuthenticationFailed("x".into()),
                "authenticationFailed",
            ),
            (IpcError::NotFound("x".into()), "notFound"),
            (IpcError::AlreadyExists("x".into()), "alreadyExists"),
            (IpcError::CloudError("x".into()), "cloudError"),
            (IpcError::InvalidInput("x".into()), "invalidInput"),
            (IpcError::InternalError("x".into()), "internalError"),
            (IpcError::PendingFlush("x".into()), "pendingFlush"),
        ];
        for (err, expected_kind) in cases {
            let value = serde_json::to_value(&err).expect("serialisation must succeed");
            assert_eq!(
                value["kind"], expected_kind,
                "variant kind mismatch for {expected_kind}"
            );
        }
    }

    /// Verifies that `From<AuthenticationError>` never leaks source detail into the message.
    #[test]
    fn test_from_auth_error_invalid_credentials_emits_safe_message() {
        let err = IpcError::from(crate::auth::AuthenticationError::InvalidCredentials);
        let value = serde_json::to_value(&err).expect("serialisation must succeed");
        let message = value["message"].as_str().expect("message must be a string");
        assert!(
            !message.contains("password"),
            "message must not contain 'password'"
        );
        assert!(!message.contains("key"), "message must not contain 'key'");
    }

    /// Verifies that `From<AuthenticationError::MemoryLockFailed>` does not include the platform message.
    #[test]
    fn test_from_auth_error_memory_lock_failed_does_not_include_platform_message() {
        let err = IpcError::from(crate::auth::AuthenticationError::MemoryLockFailed(
            "VirtualLock failed: access denied".into(),
        ));
        let value = serde_json::to_value(&err).expect("serialisation must succeed");
        let message = value["message"].as_str().expect("message must be a string");
        assert!(
            !message.contains("VirtualLock"),
            "platform message must not appear in IPC response"
        );
        assert!(
            !message.contains("access denied"),
            "platform message must not appear in IPC response"
        );
    }

    /// Verifies that `From<StorageError::ConstraintViolation>` does not include SQL text.
    #[test]
    fn test_from_storage_constraint_violation_does_not_include_sql_text() {
        let err = IpcError::from(crate::storage::StorageError::ConstraintViolation(
            "UNIQUE constraint failed: nodes.node_id".into(),
        ));
        let value = serde_json::to_value(&err).expect("serialisation must succeed");
        let message = value["message"].as_str().expect("message must be a string");
        assert!(
            !message.contains("UNIQUE"),
            "SQL text must not appear in IPC response"
        );
        assert!(
            !message.contains("nodes.node_id"),
            "schema detail must not appear in IPC response"
        );
    }

    /// Verifies that `From<SharingError::InvalidContactId>` does not include the UUID.
    #[test]
    fn test_from_sharing_invalid_contact_id_does_not_include_uuid() {
        let err = IpcError::from(crate::sharing::SharingError::InvalidContactId(
            "00000000-0000-0000-0000-000000000000".into(),
        ));
        let value = serde_json::to_value(&err).expect("serialisation must succeed");
        let message = value["message"].as_str().expect("message must be a string");
        assert!(
            !message.contains("00000000"),
            "UUID must not appear in IPC response"
        );
    }

    /// Verifies SyncError::Storage recursively delegates to From<StorageError>.
    #[test]
    fn test_from_sync_error_storage_delegates_to_storage_from() {
        let err = IpcError::from(crate::storage::SyncError::Storage {
            source: crate::storage::StorageError::NotFound,
        });
        let value = serde_json::to_value(&err).expect("serialisation must succeed");
        assert_eq!(value["kind"], "notFound");
    }

    /// Verifies From<CloudTransportError> for non-NotFound variants emits CloudError.
    #[test]
    fn test_from_cloud_transport_error_timeout_emits_cloud_error() {
        let err = IpcError::from(crate::storage::CloudTransportError::Timeout);
        let value = serde_json::to_value(&err).expect("serialisation must succeed");
        assert_eq!(value["kind"], "cloudError");
    }

    /// Verifies From<SharingError::AuthenticationFailed> emits authenticationFailed kind.
    #[test]
    fn test_from_sharing_error_authentication_failed_emits_correct_kind() {
        let err = IpcError::from(crate::sharing::SharingError::AuthenticationFailed);
        let value = serde_json::to_value(&err).expect("serialisation must succeed");
        assert_eq!(value["kind"], "authenticationFailed");
        let message = value["message"].as_str().expect("message must be string");
        assert!(
            !message.contains("KEM"),
            "KEM context must not appear in message"
        );
        assert!(
            !message.contains("CTX"),
            "CTX context must not appear in message"
        );
    }

    /// Verifies that `From<AuthenticationError::KeySource(InvalidSize)>` does not leak the
    /// actual byte count into the IPC message.
    #[test]
    fn test_from_auth_error_key_source_invalid_size_does_not_include_size_number() {
        let err = IpcError::from(crate::auth::AuthenticationError::KeySource(
            crate::auth::KeySourceError::InvalidSize { actual: 31 },
        ));
        let value = serde_json::to_value(&err).expect("serialisation must succeed");
        let message = value["message"].as_str().expect("message must be a string");
        assert!(
            !message.contains("31"),
            "key file size must not appear in IPC message"
        );
        assert_eq!(value["kind"], "authenticationFailed");
    }

    /// Verifies that `From<AuthenticationError::SessionAlreadyActive>` emits invalidInput.
    #[test]
    fn test_from_auth_error_session_already_active_emits_invalid_input() {
        let err = IpcError::from(crate::auth::AuthenticationError::SessionAlreadyActive);
        let value = serde_json::to_value(&err).expect("serialisation must succeed");
        assert_eq!(value["kind"], "invalidInput");
    }

    /// Verifies that `From<AuthenticationError::SessionNotActive>` emits vaultLocked.
    #[test]
    fn test_from_auth_error_session_not_active_emits_vault_locked() {
        let err = IpcError::from(crate::auth::AuthenticationError::SessionNotActive);
        let value = serde_json::to_value(&err).expect("serialisation must succeed");
        assert_eq!(value["kind"], "vaultLocked");
    }

    /// Verifies that `From<AuthenticationError::NoRecoverySlot>` emits invalidInput.
    #[test]
    fn test_from_auth_error_no_recovery_slot_emits_invalid_input() {
        let err = IpcError::from(crate::auth::AuthenticationError::NoRecoverySlot);
        let value = serde_json::to_value(&err).expect("serialisation must succeed");
        assert_eq!(value["kind"], "invalidInput");
    }

    /// Verifies that `From<AuthenticationError::InvalidRecoveryPhrase>` emits invalidInput.
    #[test]
    fn test_from_auth_error_invalid_recovery_phrase_emits_invalid_input() {
        let err = IpcError::from(crate::auth::AuthenticationError::InvalidRecoveryPhrase);
        let value = serde_json::to_value(&err).expect("serialisation must succeed");
        assert_eq!(value["kind"], "invalidInput");
    }

    /// Verifies that `From<StorageError::NotFound>` emits notFound.
    #[test]
    fn test_from_storage_not_found_emits_not_found() {
        let err = IpcError::from(crate::storage::StorageError::NotFound);
        let value = serde_json::to_value(&err).expect("serialisation must succeed");
        assert_eq!(value["kind"], "notFound");
    }

    /// Verifies that `From<StorageError::WrongKey>` emits authenticationFailed.
    #[test]
    fn test_from_storage_wrong_key_emits_authentication_failed() {
        let err = IpcError::from(crate::storage::StorageError::WrongKey);
        let value = serde_json::to_value(&err).expect("serialisation must succeed");
        assert_eq!(value["kind"], "authenticationFailed");
    }

    /// Verifies that `From<StorageError::ChecksumMismatch>` emits internalError.
    #[test]
    fn test_from_storage_checksum_mismatch_emits_internal_error() {
        let err = IpcError::from(crate::storage::StorageError::ChecksumMismatch);
        let value = serde_json::to_value(&err).expect("serialisation must succeed");
        assert_eq!(value["kind"], "internalError");
    }

    /// Verifies that `From<SharingError::ContactNotFound>` emits notFound.
    #[test]
    fn test_from_sharing_contact_not_found_emits_not_found() {
        let err = IpcError::from(crate::sharing::SharingError::ContactNotFound);
        let value = serde_json::to_value(&err).expect("serialisation must succeed");
        assert_eq!(value["kind"], "notFound");
    }

    /// Verifies that `From<SharingError::ShareAlreadyRevoked>` emits invalidInput.
    #[test]
    fn test_from_sharing_share_already_revoked_emits_invalid_input() {
        let err = IpcError::from(crate::sharing::SharingError::ShareAlreadyRevoked);
        let value = serde_json::to_value(&err).expect("serialisation must succeed");
        assert_eq!(value["kind"], "invalidInput");
    }

    /// Verifies that `From<SharingError::ConstraintViolation>` does not include SQL text or
    /// schema details in the IPC message and emits alreadyExists.
    #[test]
    fn test_from_sharing_constraint_violation_does_not_include_source_detail() {
        let err = IpcError::from(crate::sharing::SharingError::ConstraintViolation(
            "UNIQUE constraint failed: shares.share_id".into(),
        ));
        let value = serde_json::to_value(&err).expect("serialisation must succeed");
        let message = value["message"].as_str().expect("message must be a string");
        assert!(
            !message.contains("UNIQUE"),
            "SQL text must not appear in IPC message"
        );
        assert!(
            !message.contains("shares.share_id"),
            "schema detail must not appear in IPC message"
        );
        assert_eq!(value["kind"], "alreadyExists");
    }

    /// Verifies that `From<SyncError::Conflict>` emits cloudError and does not leak conflict
    /// counter values into the IPC message.
    ///
    /// Note: `SyncError::Conflict` wraps a `SyncConflict` struct (not a plain string);
    /// the sentinel counter `987654321` must not appear in the sanitised message.
    #[test]
    fn test_from_sync_error_conflict_emits_cloud_error() {
        let err = IpcError::from(crate::storage::SyncError::Conflict(
            crate::storage::SyncConflict {
                local_counter: 987_654_321,
                cloud_counter: 123_456_789,
                local_last_synced: None,
                cloud_last_synced: None,
            },
        ));
        let value = serde_json::to_value(&err).expect("serialisation must succeed");
        assert_eq!(value["kind"], "cloudError");
        let message = value["message"].as_str().expect("message must be a string");
        assert!(
            !message.contains("987654321"),
            "conflict counter must not appear in IPC message"
        );
    }

    /// Verifies that `From<CloudTransportError::AuthenticationFailed>` emits cloudError.
    #[test]
    fn test_from_cloud_transport_authentication_failed_emits_cloud_error() {
        let err = IpcError::from(crate::storage::CloudTransportError::AuthenticationFailed);
        let value = serde_json::to_value(&err).expect("serialisation must succeed");
        assert_eq!(value["kind"], "cloudError");
    }

    /// Verifies that a serialised `IpcError` contains exactly the two expected fields
    /// ("kind" and "message"), preventing accidental serde field leaks.
    #[test]
    fn test_ipc_error_serde_shape_has_no_extra_fields() {
        let err = IpcError::VaultLocked("test".into());
        let value = serde_json::to_value(&err).expect("serialisation must succeed");
        let obj = value
            .as_object()
            .expect("serialised IpcError must be a JSON object");
        assert_eq!(
            obj.len(),
            2,
            "IpcError must serialise to exactly two fields: 'kind' and 'message'"
        );
        assert!(obj.contains_key("kind"), "JSON object must contain 'kind'");
        assert!(
            obj.contains_key("message"),
            "JSON object must contain 'message'"
        );
    }

    /// Verifies that `From<StorageError::EpochBufferNotFlushed>` emits `pendingFlush` kind.
    #[test]
    fn test_from_storage_epoch_buffer_not_flushed_emits_pending_flush() {
        let id = uuid::Uuid::parse_str("aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa")
            .expect("test UUID must parse");
        let err = IpcError::from(crate::storage::StorageError::EpochBufferNotFlushed(id));
        let value = serde_json::to_value(&err).expect("serialisation must succeed");
        assert_eq!(
            value["kind"], "pendingFlush",
            "EpochBufferNotFlushed must map to pendingFlush kind"
        );
        let message = value["message"].as_str().expect("message must be a string");
        assert!(
            !message.contains("aaaaaaaa"),
            "UUID must not leak into IPC response: {message}"
        );
    }
}
