//! In-memory [`CloudTransport`] mock.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::Mutex;

use super::{CloudTransport, CloudTransportError};

/// Plain-data mirror of [`CloudTransportError`] used for failure injection.
#[cfg(any(test, feature = "test-utils"))]
#[derive(Debug, Clone)]
pub enum CloudTransportErrorKind {
    NotFound,
    AuthenticationFailed,
    Timeout,
    IoError {
        kind: std::io::ErrorKind,
        message: String,
    },
    RcloneProcessFailed {
        exit_code: i32,
        stderr_sanitised: String,
    },
    Other(String),
}

/// In-memory cloud transport backed by a `HashMap<String, Vec<u8>>`.
#[derive(Debug, Default, Clone)]
pub struct MockCloudTransport {
    blobs: Arc<Mutex<HashMap<String, Vec<u8>>>>,
    #[cfg(any(test, feature = "test-utils"))]
    failure_paths: Arc<Mutex<HashMap<String, CloudTransportErrorKind>>>,
}

impl MockCloudTransport {
    /// Creates an empty in-memory transport.
    pub fn new() -> Self {
        Self::default()
    }

    #[cfg(any(test, feature = "test-utils"))]
    /// Injects a one-shot failure for `path`.
    pub async fn inject_failure(&self, path: &str, kind: CloudTransportErrorKind) {
        self.failure_paths
            .lock()
            .await
            .insert(path.to_owned(), kind);
    }

    #[cfg(any(test, feature = "test-utils"))]
    async fn check_failure(&self, path: &str) -> Result<(), CloudTransportError> {
        let maybe_kind = self.failure_paths.lock().await.remove(path);
        if let Some(kind) = maybe_kind {
            return Err(match kind {
                CloudTransportErrorKind::NotFound => CloudTransportError::NotFound,
                CloudTransportErrorKind::AuthenticationFailed => {
                    CloudTransportError::AuthenticationFailed
                }
                CloudTransportErrorKind::Timeout => CloudTransportError::Timeout,
                CloudTransportErrorKind::IoError { kind, message } => {
                    CloudTransportError::IoError(std::io::Error::new(kind, message))
                }
                CloudTransportErrorKind::RcloneProcessFailed {
                    exit_code,
                    stderr_sanitised,
                } => CloudTransportError::RcloneProcessFailed {
                    exit_code,
                    stderr_sanitised,
                },
                CloudTransportErrorKind::Other(message) => CloudTransportError::Other(message),
            });
        }
        Ok(())
    }
}

#[async_trait]
impl CloudTransport for MockCloudTransport {
    async fn upload_blob(
        &self,
        local_path: &Path,
        remote_path: &str,
    ) -> Result<(), CloudTransportError> {
        #[cfg(any(test, feature = "test-utils"))]
        self.check_failure(remote_path).await?;

        let bytes = tokio::fs::read(local_path).await?;
        self.blobs
            .lock()
            .await
            .insert(remote_path.to_owned(), bytes);
        Ok(())
    }

    async fn download_blob(
        &self,
        remote_path: &str,
        local_path: &Path,
    ) -> Result<(), CloudTransportError> {
        #[cfg(any(test, feature = "test-utils"))]
        self.check_failure(remote_path).await?;

        let bytes = self
            .blobs
            .lock()
            .await
            .get(remote_path)
            .cloned()
            .ok_or(CloudTransportError::NotFound)?;
        tokio::fs::write(local_path, bytes).await?;
        Ok(())
    }

    async fn delete_blob(&self, remote_path: &str) -> Result<(), CloudTransportError> {
        #[cfg(any(test, feature = "test-utils"))]
        self.check_failure(remote_path).await?;

        self.blobs.lock().await.remove(remote_path);
        Ok(())
    }

    async fn list_blobs(&self, remote_prefix: &str) -> Result<Vec<String>, CloudTransportError> {
        #[cfg(any(test, feature = "test-utils"))]
        self.check_failure(remote_prefix).await?;

        let mut matches: Vec<String> = self
            .blobs
            .lock()
            .await
            .keys()
            .filter(|key| key.starts_with(remote_prefix) && key.len() > remote_prefix.len())
            .cloned()
            .collect();
        matches.sort_unstable();
        Ok(matches)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::cloud::CloudEndpoint;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_mock_upload_download_round_trip_preserves_bytes() {
        let transport = MockCloudTransport::new();
        let directory = tempdir().unwrap();
        let source_path = directory.path().join("source.bin");
        let download_path = directory.path().join("download.bin");
        let payload = b"vault header bytes".to_vec();
        tokio::fs::write(&source_path, &payload).await.unwrap();

        transport
            .upload_blob(&source_path, "vault/vault-header.json")
            .await
            .unwrap();
        transport
            .download_blob("vault/vault-header.json", &download_path)
            .await
            .unwrap();

        let recovered = tokio::fs::read(&download_path).await.unwrap();
        assert_eq!(recovered, payload);
    }

    #[tokio::test]
    async fn test_mock_download_missing_path_returns_not_found() {
        let transport = MockCloudTransport::new();
        let directory = tempdir().unwrap();
        let download_path = directory.path().join("missing.bin");

        let result = transport
            .download_blob("missing.json", &download_path)
            .await;

        assert!(matches!(result, Err(CloudTransportError::NotFound)));
    }

    #[tokio::test]
    async fn test_mock_upload_overwrites_existing_blob_idempotently() {
        let transport = MockCloudTransport::new();
        let directory = tempdir().unwrap();
        let first_path = directory.path().join("first.bin");
        let second_path = directory.path().join("second.bin");
        let download_path = directory.path().join("download.bin");
        tokio::fs::write(&first_path, b"first").await.unwrap();
        tokio::fs::write(&second_path, b"second").await.unwrap();

        transport
            .upload_blob(&first_path, "vault.json")
            .await
            .unwrap();
        transport
            .upload_blob(&second_path, "vault.json")
            .await
            .unwrap();
        transport
            .download_blob("vault.json", &download_path)
            .await
            .unwrap();

        let recovered = tokio::fs::read(download_path).await.unwrap();
        assert_eq!(recovered, b"second");
    }

    #[tokio::test]
    async fn test_mock_delete_removes_blob() {
        let transport = MockCloudTransport::new();
        let directory = tempdir().unwrap();
        let source_path = directory.path().join("source.bin");
        let download_path = directory.path().join("download.bin");
        tokio::fs::write(&source_path, b"x").await.unwrap();

        transport
            .upload_blob(&source_path, "vault/a.blob")
            .await
            .unwrap();
        transport.delete_blob("vault/a.blob").await.unwrap();

        let result = transport
            .download_blob("vault/a.blob", &download_path)
            .await;
        assert!(matches!(result, Err(CloudTransportError::NotFound)));
    }

    #[tokio::test]
    async fn test_mock_delete_nonexistent_path_is_idempotent() {
        let transport = MockCloudTransport::new();

        let result = transport.delete_blob("missing").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_mock_list_blobs_filters_by_prefix() {
        let transport = MockCloudTransport::new();
        let directory = tempdir().unwrap();
        let payload_path = directory.path().join("payload.bin");
        tokio::fs::write(&payload_path, b"x").await.unwrap();

        transport
            .upload_blob(&payload_path, "vault/a.blob")
            .await
            .unwrap();
        transport
            .upload_blob(&payload_path, "vault/b.blob")
            .await
            .unwrap();
        transport
            .upload_blob(&payload_path, "manifest/x.blob")
            .await
            .unwrap();

        let blobs = transport.list_blobs("vault/").await.unwrap();
        assert_eq!(blobs, vec!["vault/a.blob", "vault/b.blob"]);
    }

    #[tokio::test]
    async fn test_mock_list_blobs_empty_prefix_returns_all_paths() {
        let transport = MockCloudTransport::new();
        let directory = tempdir().unwrap();
        let payload_path = directory.path().join("payload.bin");
        tokio::fs::write(&payload_path, b"x").await.unwrap();

        transport
            .upload_blob(&payload_path, "b/path")
            .await
            .unwrap();
        transport
            .upload_blob(&payload_path, "a/path")
            .await
            .unwrap();

        let blobs = transport.list_blobs("").await.unwrap();
        assert_eq!(blobs, vec!["a/path", "b/path"]);
    }

    #[tokio::test]
    async fn test_mock_list_blobs_prefix_results_are_stable_and_exclude_exact_prefix_path() {
        let transport = MockCloudTransport::new();
        let directory = tempdir().unwrap();
        let payload_path = directory.path().join("payload.bin");
        tokio::fs::write(&payload_path, b"x").await.unwrap();

        transport
            .upload_blob(&payload_path, "vault/")
            .await
            .unwrap();
        transport
            .upload_blob(&payload_path, "vault/z.blob")
            .await
            .unwrap();
        transport
            .upload_blob(&payload_path, "vault/a.blob")
            .await
            .unwrap();

        let first = transport.list_blobs("vault/").await.unwrap();
        let second = transport.list_blobs("vault/").await.unwrap();

        assert_eq!(first, vec!["vault/a.blob", "vault/z.blob"]);
        assert_eq!(second, first);
    }

    #[tokio::test]
    async fn test_mock_inject_authentication_failure_variant() {
        let transport = MockCloudTransport::new();
        transport
            .inject_failure("vault/auth", CloudTransportErrorKind::AuthenticationFailed)
            .await;
        let directory = tempdir().unwrap();
        let source_path = directory.path().join("source.bin");
        tokio::fs::write(&source_path, b"x").await.unwrap();

        let result = transport.upload_blob(&source_path, "vault/auth").await;
        assert!(matches!(
            result,
            Err(CloudTransportError::AuthenticationFailed)
        ));
    }

    #[tokio::test]
    async fn test_mock_inject_timeout_variant_on_upload() {
        let transport = MockCloudTransport::new();
        transport
            .inject_failure("vault/timeout", CloudTransportErrorKind::Timeout)
            .await;
        let directory = tempdir().unwrap();
        let source_path = directory.path().join("source.bin");
        tokio::fs::write(&source_path, b"x").await.unwrap();

        let result = transport.upload_blob(&source_path, "vault/timeout").await;
        assert!(matches!(result, Err(CloudTransportError::Timeout)));
    }

    #[tokio::test]
    async fn test_mock_inject_not_found_variant_on_download_is_one_shot() {
        let transport = MockCloudTransport::new();
        let directory = tempdir().unwrap();
        let source_path = directory.path().join("source.bin");
        let download_path = directory.path().join("download.bin");
        tokio::fs::write(&source_path, b"present").await.unwrap();
        transport
            .upload_blob(&source_path, "vault/not-found")
            .await
            .unwrap();
        transport
            .inject_failure("vault/not-found", CloudTransportErrorKind::NotFound)
            .await;

        let first = transport
            .download_blob("vault/not-found", &download_path)
            .await;
        assert!(matches!(first, Err(CloudTransportError::NotFound)));

        transport
            .download_blob("vault/not-found", &download_path)
            .await
            .unwrap();
        let recovered = tokio::fs::read(download_path).await.unwrap();
        assert_eq!(recovered, b"present");
    }

    #[tokio::test]
    async fn test_mock_inject_io_error_variant_on_delete_preserves_kind_and_message() {
        let transport = MockCloudTransport::new();
        transport
            .inject_failure(
                "vault/io",
                CloudTransportErrorKind::IoError {
                    kind: std::io::ErrorKind::PermissionDenied,
                    message: "permission denied".to_string(),
                },
            )
            .await;

        let result = transport.delete_blob("vault/io").await;
        match result {
            Err(CloudTransportError::IoError(error)) => {
                assert_eq!(error.kind(), std::io::ErrorKind::PermissionDenied);
                assert_eq!(error.to_string(), "permission denied");
            }
            other => panic!("unexpected result: {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_mock_inject_rclone_process_failed_variant_carries_exit_code_and_stderr() {
        let transport = MockCloudTransport::new();
        transport
            .inject_failure(
                "vault/rclone",
                CloudTransportErrorKind::RcloneProcessFailed {
                    exit_code: 12,
                    stderr_sanitised: "sanitised".to_string(),
                },
            )
            .await;
        let directory = tempdir().unwrap();
        let download_path = directory.path().join("download.bin");

        let result = transport
            .download_blob("vault/rclone", &download_path)
            .await;
        match result {
            Err(CloudTransportError::RcloneProcessFailed {
                exit_code,
                stderr_sanitised,
            }) => {
                assert_eq!(exit_code, 12);
                assert_eq!(stderr_sanitised, "sanitised");
            }
            other => panic!("unexpected result: {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_mock_inject_other_variant_on_delete() {
        let transport = MockCloudTransport::new();
        transport
            .inject_failure(
                "vault/other",
                CloudTransportErrorKind::Other("forced-other".to_string()),
            )
            .await;

        let result = transport.delete_blob("vault/other").await;
        assert!(matches!(
            result,
            Err(CloudTransportError::Other(message)) if message == "forced-other"
        ));
    }

    #[tokio::test]
    async fn test_mock_upload_bubbles_io_error_when_local_path_unreadable() {
        let transport = MockCloudTransport::new();
        let missing_path = tempdir().unwrap().path().join("missing/source.bin");

        let result = transport.upload_blob(&missing_path, "vault/io").await;
        assert!(matches!(result, Err(CloudTransportError::IoError(_))));
    }

    #[test]
    fn test_cloud_endpoint_serde_round_trip_preserves_all_fields() {
        let endpoint = CloudEndpoint {
            provider: "s3".to_string(),
            bucket: "bucket-name".to_string(),
            region: "".to_string(),
            endpoint: "".to_string(),
            path_prefix: "vaults/a".to_string(),
        };

        let json = serde_json::to_string(&endpoint).unwrap();
        let decoded: CloudEndpoint = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, endpoint);
    }

    #[test]
    fn test_cloud_endpoint_equality_differs_when_path_prefix_differs() {
        let endpoint_a = CloudEndpoint {
            provider: "s3".to_string(),
            bucket: "bucket-name".to_string(),
            region: "us-east-1".to_string(),
            endpoint: "https://example.com".to_string(),
            path_prefix: "vaults/a".to_string(),
        };
        let endpoint_b = CloudEndpoint {
            path_prefix: "vaults/b".to_string(),
            ..endpoint_a.clone()
        };

        assert_ne!(endpoint_a, endpoint_b);
    }
}
