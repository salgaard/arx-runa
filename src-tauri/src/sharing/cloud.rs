//! Cloud-layer operations for creating and fetching file shares.
//!
//! This module implements the outgoing-share creation ceremony (`create_share`)
//! and the incoming-share fetch ceremony (`fetch_received_share_to_local`).
//! `create_share` performs HPKE encryption via [`create_share_package`] to
//! seal the file key for the recipient. Chunk blob data itself is never
//! decrypted — it is copied and uploaded opaquely.

use std::path::{Path, PathBuf};

use uuid::Uuid;

use crate::crypto::KeyEncryptionKey;
use crate::sharing::error::SharingError;
use crate::sharing::packages::create_share_package;
use crate::sharing::store::{ShareRecord, SharingStore};
use crate::sharing::types::ContactId;
use crate::storage::CloudTransport;
use crate::storage::metadata_store::MetadataStore;

/// Output of a successful [`create_share`] call.
pub(crate) struct CreateShareOutput {
    /// The share identifier assigned to the new share row.
    pub share_id: String,
    /// The file-level grouping identifier shared by all recipients of this file version.
    pub file_share_id: String,
    /// HPKE-sealed share package wire bytes to deliver to the recipient.
    pub wire_bytes: Vec<u8>,
    /// B2 scoped key identifier stored for later revocation, if one was generated.
    pub download_key_id: Option<String>,
}

/// Data parameters for a [`create_share`] call.
pub(crate) struct CreateShareRequest {
    /// The file to share.
    pub file_id: Uuid,
    /// The recipient contact.
    pub contact_id: ContactId,
    /// Optional Unix-second expiry for the share.
    pub expires_at: Option<i64>,
    /// Current time as Unix seconds (used for `created_at`).
    pub now_unix_seconds: i64,
    /// Whether the recipient should be asked to send a receipt after downloading.
    pub receipt_requested: bool,
}

/// Creates an outgoing file share for a single recipient.
///
/// If an active share already exists for this file, the existing `file_share_id`
/// and cloud blobs are reused and only the per-recipient package is generated.
/// Otherwise, a new `file_share_id` is generated, each chunk blob is copied
/// from staging into `shared/<file_share_id>/<uuid>.blob` on the cloud, and a
/// new `ShareRecord` row is persisted.
///
/// Temporary local copies are stored as `shared-copy-<uuid>.blob` in
/// `staging_directory` and are deleted on both success and failure.
pub(crate) async fn create_share(
    request: CreateShareRequest,
    metadata_store: &dyn MetadataStore,
    sharing_store: &dyn SharingStore,
    cloud: &dyn CloudTransport,
    key_encryption_key: &KeyEncryptionKey,
    staging_directory: &Path,
) -> Result<CreateShareOutput, SharingError> {
    let CreateShareRequest {
        file_id,
        contact_id,
        expires_at,
        now_unix_seconds,
        receipt_requested,
    } = request;
    let file_id_str = file_id.hyphenated().to_string();

    let active_shares = sharing_store
        .list_active_shares_by_file(&file_id_str)
        .await?;

    let (file_share_id, need_blob_copy) = if let Some(first) = active_shares.first() {
        (first.file_share_id.clone(), false)
    } else {
        (Uuid::new_v4().hyphenated().to_string(), true)
    };

    if need_blob_copy {
        let mut chunks = metadata_store
            .get_chunks(file_id)
            .await
            .map_err(|e| SharingError::Backend(e.to_string()))?;

        chunks.sort_by_key(|c| c.chunk_index);

        let mut uploaded_remote_paths: Vec<String> = Vec::new();
        let mut temp_local_paths: Vec<PathBuf> = Vec::new();

        for chunk in &chunks {
            let temp_uuid = Uuid::new_v4().hyphenated().to_string();
            let local_copy_path = staging_directory.join(format!("shared-copy-{}.blob", temp_uuid));
            temp_local_paths.push(local_copy_path.clone());

            let source_path = staging_directory.join(format!("{}.blob", chunk.blob_name));

            // If the local staging blob was cleaned up after cloud sync, fetch it back.
            if !source_path.exists() {
                let remote = format!("vault/{}.blob", chunk.blob_name);
                if let Err(e) = cloud.download_blob(&remote, &source_path).await {
                    cleanup_temp_files(&temp_local_paths).await;
                    for uploaded in &uploaded_remote_paths {
                        let _ = cloud.delete_blob(uploaded).await;
                    }
                    return Err(SharingError::CloudOperation(format!(
                        "blob not in staging and cloud download failed (chunk {}): {}",
                        chunk.chunk_index, e
                    )));
                }
            }

            if let Err(io_err) = tokio::fs::copy(&source_path, &local_copy_path).await {
                cleanup_temp_files(&temp_local_paths).await;
                for remote in &uploaded_remote_paths {
                    let _ = cloud.delete_blob(remote).await;
                }
                return Err(SharingError::CloudOperation(format!(
                    "staging copy failed (chunk {}): {}",
                    chunk.chunk_index, io_err
                )));
            }

            let remote_path = format!("shared/{}/{}.blob", file_share_id, chunk.blob_name);

            if let Err(transport_err) = cloud.upload_blob(&local_copy_path, &remote_path).await {
                cleanup_temp_files(&temp_local_paths).await;
                for remote in &uploaded_remote_paths {
                    let _ = cloud.delete_blob(remote).await;
                }
                return Err(SharingError::CloudOperation(format!(
                    "upload failed (chunk {}): {}",
                    chunk.chunk_index, transport_err
                )));
            }

            uploaded_remote_paths.push(remote_path);
            let _ = tokio::fs::remove_file(&local_copy_path).await;
        }
    }

    let cloud_path_prefix = format!("shared/{}/", file_share_id);

    let (cloud_endpoint, download_key_id) = match cloud
        .generate_share_credentials(&cloud_path_prefix, 7 * 24 * 3600, receipt_requested)
        .await
    {
        Ok(Some(mut creds)) => {
            if receipt_requested {
                creds["receipt_requested"] = serde_json::json!(true);
            }
            let key_id = creds["key_id"].as_str().map(str::to_owned);
            (creds, key_id)
        }
        Ok(None) => (
            serde_json::json!({ "path_prefix": cloud_path_prefix }),
            None,
        ),
        Err(e) => {
            return Err(SharingError::CloudOperation(format!(
                "B2 application key creation failed: {}. \
                 Sharing requires a B2 key with writeKeys capability \
                 (or the master application key).",
                e
            )));
        }
    };

    let recipient_contact = sharing_store
        .get_contact(contact_id)
        .await
        .map_err(|_| SharingError::ContactNotFound)?;

    let wire = create_share_package(
        file_id,
        &recipient_contact.public_key,
        expires_at,
        cloud_endpoint,
        metadata_store,
        sharing_store,
        key_encryption_key,
    )
    .await?;

    let share_id = Uuid::new_v4().hyphenated().to_string();

    let share_record = ShareRecord {
        share_id: share_id.clone(),
        file_id: file_id_str,
        contact_id,
        file_share_id: file_share_id.clone(),
        cloud_path: cloud_path_prefix.clone(),
        created_at: now_unix_seconds,
        expires_at,
        revoked_at: None,
        download_key_id: download_key_id.clone(),
        receipt_requested,
        receipt_received_at: None,
    };

    sharing_store.insert_share(&share_record).await?;

    Ok(CreateShareOutput {
        share_id,
        file_share_id,
        wire_bytes: wire,
        download_key_id,
    })
}

/// Fetches the encrypted chunk blobs for a received share into the local
/// staging directory so the caller can decrypt them for preview or export.
///
/// Blobs are stored in the staging directory as `<blob_name>.blob`.
/// The function does not decrypt the data — the caller must call
/// `decrypt_file` with the wrapped file key from the `ReceivedShare` row.
///
/// Returns the list of local staging paths (one per chunk).
///
/// # Path prefix validation
///
/// `cloud_endpoint.path_prefix` is read from the share row and validated to be non-empty.
/// It is then prepended to each chunk UUID to construct the remote blob path.
/// The path_prefix must follow the standard share namespace pattern: `shared/<file_share_id>/`.
pub(crate) async fn fetch_received_share_to_local(
    share_id: &str,
    sharing_store: &dyn SharingStore,
    cloud: &dyn CloudTransport,
    staging_directory: &Path,
    rclone_binary_path: Option<std::path::PathBuf>,
) -> Result<Vec<PathBuf>, SharingError> {
    let received_share = sharing_store.get_received_share(share_id).await?;

    // Ensure the staging directory exists before writing the rclone conf file or
    // receiving any blob downloads into it.
    crate::storage::staging::ensure_staging_directory(staging_directory)
        .await
        .map_err(|e| SharingError::CloudOperation(format!("staging directory: {e}")))?;

    // If the share has embedded B2 scoped credentials, build a temporary rclone
    // transport so the download bypasses the vault owner's own cloud config.
    let b2_transport: Option<crate::storage::cloud::RcloneTransport> = if let (
        Some(bin),
        Some("b2"),
    ) = (
        rclone_binary_path.as_deref(),
        received_share
            .cloud_endpoint
            .get("provider")
            .and_then(|v| v.as_str()),
    ) {
        let key_id = received_share
            .cloud_endpoint
            .get("key_id")
            .and_then(|v| v.as_str())
            .ok_or(SharingError::InvalidSharePackage)?;
        let app_key = received_share
            .cloud_endpoint
            .get("application_key")
            .and_then(|v| v.as_str())
            .ok_or(SharingError::InvalidSharePackage)?;
        let bucket = received_share
            .cloud_endpoint
            .get("bucket")
            .and_then(|v| v.as_str())
            .ok_or(SharingError::InvalidSharePackage)?;
        let path_prefix = received_share
            .cloud_endpoint
            .get("path_prefix")
            .and_then(|v| v.as_str())
            .ok_or(SharingError::InvalidSharePackage)?;

        let conf_content =
            format!("[arxshare-dl]\ntype = b2\naccount = {key_id}\nkey = {app_key}\n");
        let conf_path = staging_directory.join(format!("dl-{share_id}.conf"));
        tokio::fs::write(&conf_path, conf_content.as_bytes())
            .await
            .map_err(|e| SharingError::CloudOperation(format!("temp conf write failed: {e}")))?;

        // Trim any trailing slash from path_prefix so that remote_target's
        // "{remote_root}/{remote_path}" join does not produce a double slash.
        let remote_root = format!("arxshare-dl:{bucket}/{}", path_prefix.trim_end_matches('/'));
        Some(
            crate::storage::cloud::RcloneTransport::new_for_share_download(
                bin.to_path_buf(),
                conf_path,
                remote_root,
            ),
        )
    } else {
        None
    };

    // If no scoped transport was built and the share has no provider field, the
    // sender did not embed credentials (non-B2 backend).  Using the recipient's
    // own transport would target the wrong bucket and silently produce nothing.
    if b2_transport.is_none() && received_share.cloud_endpoint.get("provider").is_none() {
        return Err(SharingError::CloudOperation(
            "share download requires sender cloud credentials; \
             only Backblaze B2 shares support cross-user download"
                .into(),
        ));
    }

    let effective_cloud: &dyn CloudTransport = match &b2_transport {
        Some(t) => t,
        None => cloud,
    };

    let path_prefix = received_share
        .cloud_endpoint
        .get("path_prefix")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let mut local_paths: Vec<PathBuf> = Vec::new();

    for (i, chunk_uuid) in received_share.chunk_uuids.iter().enumerate() {
        if !is_uuid_v4_str(chunk_uuid) {
            cleanup_temp_files(&local_paths).await;
            if b2_transport.is_some() {
                let _ =
                    tokio::fs::remove_file(staging_directory.join(format!("dl-{share_id}.conf")))
                        .await;
            }
            return Err(SharingError::InvalidSharePackage);
        }

        let local_path = staging_directory.join(format!("{}.blob", chunk_uuid));

        if !local_path.starts_with(staging_directory) {
            cleanup_temp_files(&local_paths).await;
            return Err(SharingError::InvalidSharePackage);
        }

        // When using a B2 scoped transport the remote root already includes the
        // path_prefix, so we address blobs relative to the remote root.
        let remote_path = if b2_transport.is_some() {
            format!("{chunk_uuid}.blob")
        } else {
            if path_prefix.is_empty() {
                cleanup_temp_files(&local_paths).await;
                return Err(SharingError::InvalidSharePackage);
            }
            format!("{path_prefix}{chunk_uuid}.blob")
        };

        if let Err(transport_err) = effective_cloud
            .download_blob(&remote_path, &local_path)
            .await
        {
            cleanup_temp_files(&local_paths).await;
            if b2_transport.is_some() {
                let _ =
                    tokio::fs::remove_file(staging_directory.join(format!("dl-{share_id}.conf")))
                        .await;
            }
            return Err(SharingError::CloudOperation(format!(
                "download failed (chunk {}): {}",
                i, transport_err
            )));
        }

        // Guard against rclone reporting success (exit 0) without writing the
        // blob file.  Without this check the missing file would only be
        // discovered later as a cryptic "decrypt failed: file not found" error.
        if !tokio::fs::try_exists(&local_path).await.unwrap_or(false) {
            cleanup_temp_files(&local_paths).await;
            if b2_transport.is_some() {
                let _ =
                    tokio::fs::remove_file(staging_directory.join(format!("dl-{share_id}.conf")))
                        .await;
            }
            return Err(SharingError::CloudOperation(format!(
                "blob not written by transport after reporting success (chunk {})",
                i
            )));
        }

        local_paths.push(local_path);
    }

    // Clean up temp rclone conf after successful download.
    if b2_transport.is_some() {
        let _ = tokio::fs::remove_file(staging_directory.join(format!("dl-{share_id}.conf"))).await;
    }

    Ok(local_paths)
}

/// Returns `true` if `s` is a valid hyphenated UUID v4 string.
fn is_uuid_v4_str(s: &str) -> bool {
    Uuid::parse_str(s)
        .map(|u| u.get_version_num() == 4)
        .unwrap_or(false)
}

/// Removes a list of local files on a best-effort basis, silently ignoring
/// individual removal errors.
async fn cleanup_temp_files(paths: &[PathBuf]) {
    for path in paths {
        let _ = tokio::fs::remove_file(path).await;
    }
}

#[cfg(test)]
mod tests {
    use async_trait::async_trait;

    use super::*;
    use crate::sharing::store::{Contact, ReceivedShare, ShareRecord, SharingStore};
    use crate::sharing::types::{ContactId, X25519PublicKey};
    use crate::storage::cloud::mock::MockCloudTransport;

    /// Minimal `SharingStore` mock that returns a fixed `ReceivedShare`.
    struct MockSharingStoreForFetch {
        received_share: ReceivedShare,
    }

    #[async_trait]
    impl SharingStore for MockSharingStoreForFetch {
        async fn get_own_public_key(&self) -> Result<X25519PublicKey, SharingError> {
            unimplemented!()
        }

        async fn insert_contact(&self, _contact: &Contact) -> Result<(), SharingError> {
            unimplemented!()
        }

        async fn get_contact(&self, _contact_id: ContactId) -> Result<Contact, SharingError> {
            unimplemented!()
        }

        async fn list_contacts(&self) -> Result<Vec<Contact>, SharingError> {
            unimplemented!()
        }

        async fn delete_contact(&self, _contact_id: ContactId) -> Result<(), SharingError> {
            unimplemented!()
        }

        async fn insert_received_share(&self, _row: &ReceivedShare) -> Result<(), SharingError> {
            unimplemented!()
        }

        async fn get_received_share(&self, _share_id: &str) -> Result<ReceivedShare, SharingError> {
            Ok(self.received_share.clone())
        }

        async fn list_received_shares(&self) -> Result<Vec<ReceivedShare>, SharingError> {
            unimplemented!()
        }

        async fn insert_share(&self, _share: &ShareRecord) -> Result<(), SharingError> {
            unimplemented!()
        }

        async fn get_share(&self, _share_id: &str) -> Result<ShareRecord, SharingError> {
            unimplemented!()
        }

        async fn list_shares_by_file(
            &self,
            _file_id: &str,
        ) -> Result<Vec<ShareRecord>, SharingError> {
            unimplemented!()
        }

        async fn list_active_shares_by_file(
            &self,
            _file_id: &str,
        ) -> Result<Vec<ShareRecord>, SharingError> {
            unimplemented!()
        }

        async fn list_active_shares_by_file_share_id(
            &self,
            _file_share_id: &str,
        ) -> Result<Vec<ShareRecord>, SharingError> {
            unimplemented!()
        }

        async fn set_share_revoked_at(
            &self,
            _share_id: &str,
            _revoked_at: i64,
        ) -> Result<(), SharingError> {
            unimplemented!()
        }
    }

    /// Verifies that `is_uuid_v4_str` returns `true` for a freshly generated UUID v4.
    #[test]
    fn test_is_uuid_v4_str_accepts_valid_v4() {
        let uuid_v4 = Uuid::new_v4().hyphenated().to_string();
        assert!(is_uuid_v4_str(&uuid_v4));
    }

    /// Verifies that `is_uuid_v4_str` returns `false` for arbitrary strings and
    /// non-v4 UUID strings.
    #[test]
    fn test_is_uuid_v4_str_rejects_non_v4() {
        assert!(!is_uuid_v4_str("not-a-uuid"));
        assert!(!is_uuid_v4_str("shared-copy-1234"));
        // RFC 4122 name-based (v5) UUID
        assert!(!is_uuid_v4_str("6ba7b810-9dad-11d1-80b4-00c04fd430c8"));
        // All-zeros UUID (version 0)
        assert!(!is_uuid_v4_str("00000000-0000-0000-0000-000000000000"));
    }

    /// Verifies that `fetch_received_share_to_local` rejects a path-traversal
    /// chunk UUID before any cloud download is attempted.
    #[tokio::test]
    async fn test_fetch_validates_chunk_uuid_rejects_path_traversal() {
        let store = MockSharingStoreForFetch {
            received_share: ReceivedShare {
                share_id: "share-1".to_owned(),
                sender_contact_id: None,
                sender_public_key: X25519PublicKey::new([0u8; 32]),
                file_id: Uuid::new_v4().hyphenated().to_string(),
                file_name: "test.txt".to_owned(),
                file_key_wrapped: [0u8; 72],
                chunk_count: 1,
                chunk_size: 4096,
                chunk_uuids: vec!["../../../etc/passwd".to_owned()],
                cloud_endpoint: serde_json::json!({
                    "provider": "webdav",
                    "path_prefix": "shared/test/"
                }),
                expires_at: None,
                imported_at: 0,
            },
        };
        let cloud = MockCloudTransport::new();
        let staging = std::path::Path::new("/tmp/staging");

        let result = fetch_received_share_to_local("share-1", &store, &cloud, staging, None).await;

        assert!(
            matches!(result, Err(SharingError::InvalidSharePackage)),
            "expected InvalidSharePackage, got {result:?}"
        );
    }

    /// Verifies that `fetch_received_share_to_local` rejects a received share
    /// with a missing path_prefix in the cloud_endpoint.
    #[tokio::test]
    async fn test_fetch_rejects_missing_path_prefix() {
        let store = MockSharingStoreForFetch {
            received_share: ReceivedShare {
                share_id: "share-2".to_owned(),
                sender_contact_id: None,
                sender_public_key: X25519PublicKey::new([0u8; 32]),
                file_id: Uuid::new_v4().hyphenated().to_string(),
                file_name: "test.txt".to_owned(),
                file_key_wrapped: [0u8; 72],
                chunk_count: 1,
                chunk_size: 4096,
                chunk_uuids: vec![Uuid::new_v4().hyphenated().to_string()],
                // provider present so credentials guard passes; path_prefix absent triggers InvalidSharePackage
                cloud_endpoint: serde_json::json!({ "provider": "webdav" }),
                expires_at: None,
                imported_at: 0,
            },
        };
        let cloud = MockCloudTransport::new();
        let staging = std::path::Path::new("/tmp/staging");

        let result = fetch_received_share_to_local("share-2", &store, &cloud, staging, None).await;

        assert!(
            matches!(result, Err(SharingError::InvalidSharePackage)),
            "expected InvalidSharePackage for missing path_prefix, got {result:?}"
        );
    }

    /// Verifies that `fetch_received_share_to_local` rejects a received share
    /// with an empty string path_prefix in the cloud_endpoint.
    #[tokio::test]
    async fn test_fetch_rejects_empty_path_prefix() {
        let store = MockSharingStoreForFetch {
            received_share: ReceivedShare {
                share_id: "share-3".to_owned(),
                sender_contact_id: None,
                sender_public_key: X25519PublicKey::new([0u8; 32]),
                file_id: Uuid::new_v4().hyphenated().to_string(),
                file_name: "test.txt".to_owned(),
                file_key_wrapped: [0u8; 72],
                chunk_count: 1,
                chunk_size: 4096,
                chunk_uuids: vec![Uuid::new_v4().hyphenated().to_string()],
                cloud_endpoint: serde_json::json!({ "provider": "webdav", "path_prefix": "" }),
                expires_at: None,
                imported_at: 0,
            },
        };
        let cloud = MockCloudTransport::new();
        let staging = std::path::Path::new("/tmp/staging");

        let result = fetch_received_share_to_local("share-3", &store, &cloud, staging, None).await;

        assert!(
            matches!(result, Err(SharingError::InvalidSharePackage)),
            "expected InvalidSharePackage for empty path_prefix, got {result:?}"
        );
    }

    /// Verifies that `fetch_received_share_to_local` accepts a valid path_prefix
    /// and continues to validate other share fields (chunk UUIDs, etc).
    #[tokio::test]
    async fn test_fetch_accepts_valid_path_prefix() {
        let valid_chunk_uuid = Uuid::new_v4().hyphenated().to_string();
        let store = MockSharingStoreForFetch {
            received_share: ReceivedShare {
                share_id: "share-4".to_owned(),
                sender_contact_id: None,
                sender_public_key: X25519PublicKey::new([0u8; 32]),
                file_id: Uuid::new_v4().hyphenated().to_string(),
                file_name: "test.txt".to_owned(),
                file_key_wrapped: [0u8; 72],
                chunk_count: 1,
                chunk_size: 4096,
                chunk_uuids: vec![valid_chunk_uuid.clone()],
                // provider present (non-B2) so the no-credentials early-return is skipped
                cloud_endpoint: serde_json::json!({
                    "provider": "webdav",
                    "path_prefix": "shared/file-share-id-123/"
                }),
                expires_at: None,
                imported_at: 0,
            },
        };
        let cloud = MockCloudTransport::new();
        let staging = std::path::Path::new("/tmp/staging");

        let result = fetch_received_share_to_local("share-4", &store, &cloud, staging, None).await;

        // MockCloudTransport has no blobs so the download fails, but path_prefix
        // validation passed and the no-credentials guard was not triggered.
        assert!(
            matches!(result, Err(SharingError::CloudOperation(_))),
            "expected CloudOperation error (blob not found), got {result:?}"
        );
    }

    #[tokio::test]
    async fn test_fetch_rejects_share_without_provider_credentials() {
        let store = MockSharingStoreForFetch {
            received_share: ReceivedShare {
                share_id: "share-5".to_owned(),
                sender_contact_id: None,
                sender_public_key: X25519PublicKey::new([0u8; 32]),
                file_id: Uuid::new_v4().hyphenated().to_string(),
                file_name: "test.txt".to_owned(),
                file_key_wrapped: [0u8; 72],
                chunk_count: 1,
                chunk_size: 4096,
                chunk_uuids: vec![Uuid::new_v4().hyphenated().to_string()],
                // Only path_prefix stored — sender used a non-B2 backend and
                // generate_share_credentials returned None.
                cloud_endpoint: serde_json::json!({ "path_prefix": "shared/file-share-id-456/" }),
                expires_at: None,
                imported_at: 0,
            },
        };
        let cloud = MockCloudTransport::new();
        let staging = std::path::Path::new("/tmp/staging");

        let result = fetch_received_share_to_local("share-5", &store, &cloud, staging, None).await;

        match result {
            Err(SharingError::CloudOperation(msg)) => {
                assert!(
                    msg.contains("sender cloud credentials"),
                    "expected credentials error, got: {msg}"
                );
            }
            other => panic!("expected CloudOperation(credentials), got {other:?}"),
        }
    }
}
