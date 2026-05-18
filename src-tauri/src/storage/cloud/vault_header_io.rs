//! Cloud upload/download helpers for `vault-header.json`.

use std::fs::OpenOptions;
use std::io::{ErrorKind, Write};
#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use thiserror::Error;

use super::vault_header::{
    Argon2ParamsJson, VaultHeader, VaultHeaderError, VaultHeaderTrustPolicy,
};
use super::{CloudTransport, CloudTransportError};

const DEFAULT_ARGON2_MEMORY_COST_KIB: u32 = 65_536;
const DEFAULT_ARGON2_TIME_COST: u32 = 3;
const DEFAULT_ARGON2_PARALLELISM: u32 = 4;

/// Cloud-root blob name for the plaintext vault header JSON.
pub const VAULT_HEADER_BLOB_NAME: &str = "vault-header.json";
/// Upload-side staging filename for pending vault-header writes.
pub const VAULT_HEADER_UPLOAD_STAGING_FILE_NAME: &str = "pending-vault-header.json";
const VAULT_HEADER_DOWNLOAD_STAGING_FILE_NAME: &str = "pending-vault-header-download.json";

/// Errors produced while syncing `vault-header.json`.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum VaultHeaderSyncError {
    /// Vault-header serialisation failed.
    #[error("vault header serialisation failed")]
    SerialiseFailed,
    /// Staging-file local I/O failed.
    #[error("vault header staging I/O failed: {0}")]
    StagingIo(String),
    /// Cloud transport operation failed.
    #[error("vault header cloud transport failed: {0}")]
    Transport(#[from] CloudTransportError),
    /// Vault-header JSON decode failed.
    #[error("vault header JSON decode failed")]
    DeserialiseFailed,
    /// Vault-header validation failed.
    #[error("vault header validation failed: {0}")]
    Validation(#[from] VaultHeaderError),
}

/// Serialises and uploads the plaintext vault header JSON to cloud root.
///
/// `key_file_blake3` and `name` are stripped before upload so the cloud never
/// receives the key-file fingerprint (ZK correlation leak) or the human-readable
/// vault name (ZK metadata leak).
pub async fn upload_vault_header(
    header: &VaultHeader,
    cloud_transport: &dyn CloudTransport,
    staging_dir: &Path,
) -> Result<(), VaultHeaderSyncError> {
    let mut cloud_header = header.clone();
    cloud_header.key_file_blake3 = None;
    cloud_header.name = None;
    let json_bytes = serialise_pretty_json(&cloud_header)?;
    let staging_path = staging_dir.join(VAULT_HEADER_UPLOAD_STAGING_FILE_NAME);
    if let Err(error) = write_owner_only_staging_file(&staging_path, &json_bytes).await {
        let _ = remove_staging_file_if_present(&staging_path).await;
        return Err(VaultHeaderSyncError::StagingIo(error.to_string()));
    }

    if let Err(error) = cloud_transport
        .upload_blob(&staging_path, VAULT_HEADER_BLOB_NAME)
        .await
    {
        return Err(VaultHeaderSyncError::Transport(error));
    }
    if let Err(cleanup_error) = remove_staging_file_if_present(&staging_path).await {
        tracing::warn!(
            ?cleanup_error,
            "vault-header staging cleanup failed after successful upload"
        );
    }
    Ok(())
}

/// Downloads, deserialises, and validates the plaintext vault header JSON.
pub async fn download_vault_header(
    cloud_transport: &dyn CloudTransport,
    staging_dir: &Path,
    policy: VaultHeaderTrustPolicy<'_>,
) -> Result<VaultHeader, VaultHeaderSyncError> {
    let temp_path = staging_dir.join(VAULT_HEADER_DOWNLOAD_STAGING_FILE_NAME);
    if let Err(error) = cloud_transport
        .download_blob(VAULT_HEADER_BLOB_NAME, &temp_path)
        .await
    {
        if let Err(cleanup_error) = remove_staging_file_if_present(&temp_path).await {
            tracing::warn!(
                ?cleanup_error,
                "vault-header temp cleanup failed after download transport failure"
            );
        }
        return Err(VaultHeaderSyncError::Transport(error));
    }

    let bytes = tokio::fs::read(&temp_path)
        .await
        .map_err(|error| VaultHeaderSyncError::StagingIo(error.to_string()));
    let cleanup_result = remove_staging_file_if_present(&temp_path).await;
    if let Err(cleanup_error) = cleanup_result {
        tracing::warn!(
            ?cleanup_error,
            "vault-header temp cleanup failed after download"
        );
    }
    let bytes = bytes?;

    let header: VaultHeader =
        serde_json::from_slice(&bytes).map_err(|_| VaultHeaderSyncError::DeserialiseFailed)?;
    header.validate_trust_policy(policy)?;
    warn_if_bootstrap_below_defaults(policy, &header);
    Ok(header)
}

/// Serialises a value to pretty JSON bytes.
fn serialise_pretty_json<T: serde::Serialize>(value: &T) -> Result<Vec<u8>, VaultHeaderSyncError> {
    serde_json::to_vec_pretty(value).map_err(|_| VaultHeaderSyncError::SerialiseFailed)
}

/// Writes a staging file using owner-only permissions where supported.
async fn write_owner_only_staging_file(path: &Path, bytes: &[u8]) -> Result<(), std::io::Error> {
    let path = path.to_path_buf();
    let payload = bytes.to_vec();
    tokio::task::spawn_blocking(move || -> Result<(), std::io::Error> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut options = OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            options.mode(0o600);
        }
        let mut file = options.open(&path)?;
        file.write_all(&payload)?;
        file.sync_all()?;
        #[cfg(unix)]
        {
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
        }
        Ok(())
    })
    .await
    .map_err(|_| std::io::Error::other("failed to join staging write task"))?
}

/// Removes a staging file, tolerating missing-file races.
async fn remove_staging_file_if_present(path: &Path) -> Result<(), std::io::Error> {
    match tokio::fs::remove_file(path).await {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

/// Emits a warning in bootstrap mode when Argon2 params are below Arx defaults.
fn warn_if_bootstrap_below_defaults(policy: VaultHeaderTrustPolicy<'_>, header: &VaultHeader) {
    if policy != VaultHeaderTrustPolicy::Bootstrap {
        return;
    }

    let primary_below_defaults = params_below_arx_defaults(&header.argon2_params);
    let recovery_below_defaults = header
        .recovery_slots
        .iter()
        .filter(|slot| params_below_arx_defaults(&slot.argon2_params))
        .count();

    if primary_below_defaults || recovery_below_defaults > 0 {
        tracing::warn!(
            schema_version = header.schema_version,
            primary_memory_cost = header.argon2_params.memory_cost,
            primary_time_cost = header.argon2_params.time_cost,
            primary_parallelism = header.argon2_params.parallelism,
            default_memory_cost = DEFAULT_ARGON2_MEMORY_COST_KIB,
            default_time_cost = DEFAULT_ARGON2_TIME_COST,
            default_parallelism = DEFAULT_ARGON2_PARALLELISM,
            recovery_slots_below_defaults = recovery_below_defaults,
            "vault header argon2 parameters are below Arx defaults during bootstrap"
        );
    }
}

/// Returns whether a parameter set is below Arx default values.
fn params_below_arx_defaults(params: &Argon2ParamsJson) -> bool {
    params.memory_cost < DEFAULT_ARGON2_MEMORY_COST_KIB
        || params.time_cost < DEFAULT_ARGON2_TIME_COST
        || params.parallelism < DEFAULT_ARGON2_PARALLELISM
}

#[cfg(test)]
mod tests {
    use async_trait::async_trait;
    use base64::Engine as _;
    use serde::Serialize;
    use tempfile::tempdir;

    use super::*;
    use crate::storage::cloud::CloudTransport;
    use crate::storage::cloud::mock::{CloudTransportErrorKind, MockCloudTransport};
    use crate::storage::cloud::vault_header::{
        Argon2ParamsJson, RecoverySlot, TrustedVaultHeaderAnchor, VaultHeader, VaultHeaderError,
    };

    fn sample_tier_one_header() -> VaultHeader {
        VaultHeader {
            vault_id: "00000000-0000-4000-8000-000000000001".to_owned(),
            schema_version: VaultHeader::SCHEMA_VERSION,
            tier: 1,
            argon2_salt: base64::engine::general_purpose::STANDARD.encode([0x11u8; 32]),
            argon2_params: Argon2ParamsJson {
                memory_cost: 65_536,
                time_cost: 3,
                parallelism: 4,
            },
            key_file_blake3: None,
            recovery_slots: Vec::new(),
            name: None,
        }
    }

    fn sample_tier_two_header_with_recovery_slot() -> VaultHeader {
        VaultHeader {
            vault_id: "00000000-0000-4000-8000-000000000002".to_owned(),
            schema_version: VaultHeader::SCHEMA_VERSION,
            tier: 2,
            argon2_salt: base64::engine::general_purpose::STANDARD.encode([0x22u8; 32]),
            argon2_params: Argon2ParamsJson {
                memory_cost: 65_536,
                time_cost: 3,
                parallelism: 4,
            },
            key_file_blake3: Some(hex::encode([0x33u8; 32])),
            recovery_slots: vec![RecoverySlot {
                method: "bip39".to_owned(),
                argon2_salt: base64::engine::general_purpose::STANDARD.encode([0x44u8; 32]),
                argon2_params: Argon2ParamsJson {
                    memory_cost: 65_536,
                    time_cost: 3,
                    parallelism: 4,
                },
                wrapped_master_key: base64::engine::general_purpose::STANDARD.encode([0x55u8; 72]),
            }],
            name: None,
        }
    }

    async fn read_uploaded_header(cloud: &MockCloudTransport, root: &Path) -> Vec<u8> {
        let download_path = root.join("downloaded-vault-header.json");
        cloud
            .download_blob(VAULT_HEADER_BLOB_NAME, &download_path)
            .await
            .expect("uploaded header should be present");
        tokio::fs::read(download_path)
            .await
            .expect("uploaded header should be readable")
    }

    #[derive(Debug)]
    struct PartialWriteFailingTransport {
        bytes: Vec<u8>,
    }

    #[async_trait]
    impl CloudTransport for PartialWriteFailingTransport {
        async fn upload_blob(
            &self,
            _local_path: &Path,
            _remote_path: &str,
        ) -> Result<(), CloudTransportError> {
            Ok(())
        }

        async fn download_blob(
            &self,
            _remote_path: &str,
            local_path: &Path,
        ) -> Result<(), CloudTransportError> {
            tokio::fs::write(local_path, &self.bytes).await?;
            Err(CloudTransportError::Timeout)
        }

        async fn delete_blob(&self, _remote_path: &str) -> Result<(), CloudTransportError> {
            Ok(())
        }

        async fn list_blobs(
            &self,
            _remote_prefix: &str,
        ) -> Result<Vec<String>, CloudTransportError> {
            Ok(Vec::new())
        }
    }

    #[tokio::test]
    async fn test_upload_vault_header_stores_plaintext_json_at_expected_remote_path() {
        let cloud = MockCloudTransport::new();
        let directory = tempdir().expect("tempdir");
        let header = sample_tier_one_header();

        upload_vault_header(&header, &cloud, directory.path())
            .await
            .expect("upload should succeed");

        let bytes = read_uploaded_header(&cloud, directory.path()).await;
        let parsed: VaultHeader = serde_json::from_slice(&bytes).expect("json should decode");
        assert_eq!(parsed, header);
    }

    #[tokio::test]
    async fn test_upload_vault_header_removes_staging_file_on_success() {
        let cloud = MockCloudTransport::new();
        let directory = tempdir().expect("tempdir");
        let header = sample_tier_one_header();

        upload_vault_header(&header, &cloud, directory.path())
            .await
            .expect("upload should succeed");

        let staging_path = directory.path().join(VAULT_HEADER_UPLOAD_STAGING_FILE_NAME);
        assert!(
            !tokio::fs::try_exists(staging_path)
                .await
                .expect("staging existence check should succeed")
        );
    }

    #[tokio::test]
    async fn test_upload_vault_header_retains_staging_file_on_transport_failure() {
        let cloud = MockCloudTransport::new();
        cloud
            .inject_failure(
                VAULT_HEADER_BLOB_NAME,
                CloudTransportErrorKind::RcloneProcessFailed {
                    exit_code: 9,
                    stderr_sanitised: "forced".to_owned(),
                },
            )
            .await;
        let directory = tempdir().expect("tempdir");
        let header = sample_tier_one_header();

        let result = upload_vault_header(&header, &cloud, directory.path()).await;
        assert!(matches!(result, Err(VaultHeaderSyncError::Transport(_))));
        let staging_path = directory.path().join(VAULT_HEADER_UPLOAD_STAGING_FILE_NAME);
        assert!(
            tokio::fs::try_exists(staging_path)
                .await
                .expect("staging existence check should succeed")
        );
    }

    #[derive(Debug)]
    struct FailingSerialize;

    impl Serialize for FailingSerialize {
        fn serialize<S>(&self, _serializer: S) -> Result<S::Ok, S::Error>
        where
            S: serde::Serializer,
        {
            Err(serde::ser::Error::custom("forced serialise failure"))
        }
    }

    #[test]
    fn test_upload_vault_header_rejects_serialise_failure() {
        let result = serialise_pretty_json(&FailingSerialize);
        assert!(matches!(result, Err(VaultHeaderSyncError::SerialiseFailed)));
    }

    #[tokio::test]
    async fn test_download_vault_header_round_trip_preserves_tier1_header_fields() {
        let cloud = MockCloudTransport::new();
        let directory = tempdir().expect("tempdir");
        let header = sample_tier_one_header();
        upload_vault_header(&header, &cloud, directory.path())
            .await
            .expect("upload should succeed");

        let recovered =
            download_vault_header(&cloud, directory.path(), VaultHeaderTrustPolicy::Bootstrap)
                .await
                .expect("download should succeed");
        assert_eq!(recovered, header);
    }

    #[tokio::test]
    async fn test_download_vault_header_round_trip_preserves_tier2_with_recovery_slot() {
        let cloud = MockCloudTransport::new();
        let directory = tempdir().expect("tempdir");
        let header = sample_tier_two_header_with_recovery_slot();
        upload_vault_header(&header, &cloud, directory.path())
            .await
            .expect("upload should succeed");

        let recovered =
            download_vault_header(&cloud, directory.path(), VaultHeaderTrustPolicy::Bootstrap)
                .await
                .expect("download should succeed");

        // Cloud header must not carry key_file_blake3 or name (ZK boundary).
        let mut expected = header.clone();
        expected.key_file_blake3 = None;
        expected.name = None;
        assert_eq!(recovered, expected);
    }

    #[tokio::test]
    async fn test_download_vault_header_rejects_malformed_json() {
        let cloud = MockCloudTransport::new();
        let directory = tempdir().expect("tempdir");
        let source_path = directory.path().join("malformed-header.json");
        tokio::fs::write(&source_path, b"not json")
            .await
            .expect("write should succeed");
        cloud
            .upload_blob(&source_path, VAULT_HEADER_BLOB_NAME)
            .await
            .expect("seed should succeed");

        let result =
            download_vault_header(&cloud, directory.path(), VaultHeaderTrustPolicy::Bootstrap)
                .await;
        assert!(matches!(
            result,
            Err(VaultHeaderSyncError::DeserialiseFailed)
        ));
        let temp_path = directory
            .path()
            .join(VAULT_HEADER_DOWNLOAD_STAGING_FILE_NAME);
        assert!(
            !tokio::fs::try_exists(temp_path)
                .await
                .expect("temp existence check should succeed")
        );
    }

    #[tokio::test]
    async fn test_download_vault_header_rejects_structurally_invalid_header() {
        let cloud = MockCloudTransport::new();
        let directory = tempdir().expect("tempdir");
        let mut header = sample_tier_one_header();
        header.argon2_salt = base64::engine::general_purpose::STANDARD.encode([0x99u8; 16]);
        upload_vault_header(&header, &cloud, directory.path())
            .await
            .expect("upload should succeed");

        let result =
            download_vault_header(&cloud, directory.path(), VaultHeaderTrustPolicy::Bootstrap)
                .await;
        assert!(matches!(
            result,
            Err(VaultHeaderSyncError::Validation(
                VaultHeaderError::SaltWrongLength
            ))
        ));
    }

    #[tokio::test]
    async fn test_download_vault_header_rejects_argon2_params_below_floor() {
        let cloud = MockCloudTransport::new();
        let directory = tempdir().expect("tempdir");
        let mut header = sample_tier_one_header();
        header.argon2_params.memory_cost = 19_455;
        upload_vault_header(&header, &cloud, directory.path())
            .await
            .expect("upload should succeed");

        let result =
            download_vault_header(&cloud, directory.path(), VaultHeaderTrustPolicy::Bootstrap)
                .await;
        assert!(matches!(
            result,
            Err(VaultHeaderSyncError::Validation(
                VaultHeaderError::Argon2ParamsBelowMinimum
            ))
        ));
    }

    #[tokio::test]
    async fn test_download_vault_header_rejects_recovery_slot_wrapped_blob_wrong_length() {
        let cloud = MockCloudTransport::new();
        let directory = tempdir().expect("tempdir");
        let mut header = sample_tier_two_header_with_recovery_slot();
        header.recovery_slots[0].wrapped_master_key =
            base64::engine::general_purpose::STANDARD.encode([0x11u8; 40]);
        upload_vault_header(&header, &cloud, directory.path())
            .await
            .expect("upload should succeed");

        let result =
            download_vault_header(&cloud, directory.path(), VaultHeaderTrustPolicy::Bootstrap)
                .await;
        assert!(matches!(
            result,
            Err(VaultHeaderSyncError::Validation(
                VaultHeaderError::RecoverySlotBlobWrongLength
            ))
        ));
    }

    #[tokio::test]
    async fn test_download_vault_header_rejects_existing_device_anchor_mismatch() {
        let cloud = MockCloudTransport::new();
        let directory = tempdir().expect("tempdir");
        let header = sample_tier_one_header();
        upload_vault_header(&header, &cloud, directory.path())
            .await
            .expect("upload should succeed");
        let trusted_anchor = TrustedVaultHeaderAnchor {
            vault_id: "00000000-0000-4000-8000-0000000000ff".to_owned(),
            argon2_salt: header.argon2_salt.clone(),
            argon2_params: header.argon2_params.clone(),
        };

        let result = download_vault_header(
            &cloud,
            directory.path(),
            VaultHeaderTrustPolicy::ExistingDevice {
                trusted_anchor: &trusted_anchor,
            },
        )
        .await;
        assert!(matches!(
            result,
            Err(VaultHeaderSyncError::Validation(
                VaultHeaderError::TrustedVaultIdMismatch
            ))
        ));
    }

    #[tokio::test]
    async fn test_download_vault_header_removes_temp_file_on_success_and_failure() {
        let cloud = MockCloudTransport::new();
        let directory = tempdir().expect("tempdir");
        let temp_path = directory
            .path()
            .join(VAULT_HEADER_DOWNLOAD_STAGING_FILE_NAME);

        let not_found_result =
            download_vault_header(&cloud, directory.path(), VaultHeaderTrustPolicy::Bootstrap)
                .await;
        assert!(matches!(
            not_found_result,
            Err(VaultHeaderSyncError::Transport(
                CloudTransportError::NotFound
            ))
        ));
        assert!(
            !tokio::fs::try_exists(&temp_path)
                .await
                .expect("temp existence check should succeed")
        );

        let header = sample_tier_one_header();
        upload_vault_header(&header, &cloud, directory.path())
            .await
            .expect("upload should succeed");
        let success_result =
            download_vault_header(&cloud, directory.path(), VaultHeaderTrustPolicy::Bootstrap)
                .await;
        assert!(success_result.is_ok());
        assert!(
            !tokio::fs::try_exists(&temp_path)
                .await
                .expect("temp existence check should succeed")
        );
    }

    #[tokio::test]
    async fn test_download_vault_header_transport_failure_cleans_up_temp_file() {
        let cloud = PartialWriteFailingTransport {
            bytes: b"partial".to_vec(),
        };
        let directory = tempdir().expect("tempdir");
        let temp_path = directory
            .path()
            .join(VAULT_HEADER_DOWNLOAD_STAGING_FILE_NAME);

        let result =
            download_vault_header(&cloud, directory.path(), VaultHeaderTrustPolicy::Bootstrap)
                .await;
        assert!(matches!(
            result,
            Err(VaultHeaderSyncError::Transport(
                CloudTransportError::Timeout
            ))
        ));
        assert!(
            !tokio::fs::try_exists(&temp_path)
                .await
                .expect("temp existence check should succeed")
        );
    }

    #[tokio::test]
    async fn test_download_vault_header_surfaces_transport_not_found() {
        let cloud = MockCloudTransport::new();
        let directory = tempdir().expect("tempdir");

        let result =
            download_vault_header(&cloud, directory.path(), VaultHeaderTrustPolicy::Bootstrap)
                .await;
        assert!(matches!(
            result,
            Err(VaultHeaderSyncError::Transport(
                CloudTransportError::NotFound
            ))
        ));
    }

    #[tokio::test]
    async fn test_download_vault_header_bootstrap_accepts_below_arx_defaults_without_error() {
        let cloud = MockCloudTransport::new();
        let directory = tempdir().expect("tempdir");
        let mut header = sample_tier_one_header();
        header.argon2_params = Argon2ParamsJson {
            memory_cost: 19_456,
            time_cost: 2,
            parallelism: 1,
        };
        upload_vault_header(&header, &cloud, directory.path())
            .await
            .expect("upload should succeed");

        let result =
            download_vault_header(&cloud, directory.path(), VaultHeaderTrustPolicy::Bootstrap)
                .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_upload_vault_header_overwrites_previous_remote_blob_idempotently() {
        let cloud = MockCloudTransport::new();
        let directory = tempdir().expect("tempdir");
        let first = sample_tier_one_header();
        let second = sample_tier_two_header_with_recovery_slot();

        upload_vault_header(&first, &cloud, directory.path())
            .await
            .expect("first upload should succeed");
        upload_vault_header(&second, &cloud, directory.path())
            .await
            .expect("second upload should succeed");

        let recovered =
            download_vault_header(&cloud, directory.path(), VaultHeaderTrustPolicy::Bootstrap)
                .await
                .expect("download should succeed");

        // Cloud header must not carry key_file_blake3 or name (ZK boundary).
        let mut expected = second.clone();
        expected.key_file_blake3 = None;
        expected.name = None;
        assert_eq!(recovered, expected);
    }

    #[tokio::test]
    async fn test_upload_vault_header_strips_key_file_blake3_and_name_from_cloud_json() {
        let cloud = MockCloudTransport::new();
        let directory = tempdir().expect("tempdir");
        let mut header = sample_tier_two_header_with_recovery_slot();
        header.name = Some("My Secret Vault".to_owned());

        upload_vault_header(&header, &cloud, directory.path())
            .await
            .expect("upload should succeed");

        let bytes = read_uploaded_header(&cloud, directory.path()).await;
        let json = std::str::from_utf8(&bytes).expect("cloud JSON must be UTF-8");
        assert!(
            !json.contains("key_file_blake3"),
            "cloud header must not contain key_file_blake3"
        );
        assert!(
            !json.contains("name"),
            "cloud header must not contain vault name"
        );
    }

    #[tokio::test]
    async fn test_upload_vault_header_surfaces_staging_io_failure() {
        let cloud = MockCloudTransport::new();
        let directory = tempdir().expect("tempdir");
        let staging_file_path = directory.path().join("not-a-directory");
        tokio::fs::write(&staging_file_path, b"x")
            .await
            .expect("write should succeed");
        let header = sample_tier_one_header();

        let result = upload_vault_header(&header, &cloud, &staging_file_path).await;
        assert!(matches!(result, Err(VaultHeaderSyncError::StagingIo(_))));
    }
}
