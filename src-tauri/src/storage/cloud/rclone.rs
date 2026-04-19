//! Production rclone-backed cloud transport implementation.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use super::destination_session::DestinationSessionPublic;
use super::rclone_subprocess::run_rclone;
use super::remote_path::{compose_remote_root, validate_remote_path, validate_remote_prefix};
use super::{CloudEndpoint, CloudTransport, CloudTransportError, SyncConfig};
use async_trait::async_trait;
use serde::Deserialize;

#[async_trait]
trait RcloneRunner: Send + Sync {
    async fn run(
        &self,
        args: Vec<OsString>,
        timeout: Duration,
    ) -> Result<String, CloudTransportError>;
}

struct RealRclone {
    binary_path: PathBuf,
}

#[async_trait]
impl RcloneRunner for RealRclone {
    async fn run(
        &self,
        args: Vec<OsString>,
        timeout: Duration,
    ) -> Result<String, CloudTransportError> {
        run_rclone(&self.binary_path, args, timeout).await
    }
}

/// Cloud transport implementation backed by the bundled `rclone` sidecar.
pub struct RcloneTransport {
    session_config_path: PathBuf,
    remote_root: String,
    sync_config: SyncConfig,
    runner: Arc<dyn RcloneRunner>,
}

impl RcloneTransport {
    /// Creates a production transport using the real rclone subprocess runner.
    pub fn new(
        binary_path: PathBuf,
        session_config_path: PathBuf,
        _endpoint: &CloudEndpoint,
        destination: &DestinationSessionPublic,
        sync_config: SyncConfig,
    ) -> Result<Self, CloudTransportError> {
        let remote_root = build_remote_root(destination)?;
        Ok(Self {
            session_config_path,
            remote_root,
            sync_config,
            runner: Arc::new(RealRclone { binary_path }),
        })
    }

    #[cfg(test)]
    fn with_runner(
        session_config_path: PathBuf,
        destination: &DestinationSessionPublic,
        sync_config: SyncConfig,
        runner: Arc<dyn RcloneRunner>,
    ) -> Result<Self, CloudTransportError> {
        Ok(Self {
            session_config_path,
            remote_root: build_remote_root(destination)?,
            sync_config,
            runner,
        })
    }

    fn base_args(&self) -> Vec<OsString> {
        vec![
            OsString::from("--config"),
            self.session_config_path.as_os_str().to_os_string(),
            OsString::from("--retries"),
            OsString::from("3"),
        ]
    }

    fn remote_target(&self, remote_path: &str) -> String {
        format!(
            "{}/{}",
            self.remote_root,
            remote_path.trim_start_matches('/')
        )
    }
}

#[derive(Debug, Deserialize)]
struct LsJsonEntry {
    #[serde(rename = "Path")]
    path: String,
}

#[async_trait]
impl CloudTransport for RcloneTransport {
    async fn upload_blob(
        &self,
        local_path: &Path,
        remote_path: &str,
    ) -> Result<(), CloudTransportError> {
        let remote_path = validate_remote_path(remote_path)?;
        let mut args = self.base_args();
        args.push(OsString::from("copyto"));
        args.push(local_path.as_os_str().to_os_string());
        args.push(OsString::from(self.remote_target(remote_path)));
        args.push(OsString::from("--quiet"));
        args.push(OsString::from("--no-traverse"));

        tracing::debug!(remote_path = %remote_path, "rclone upload");
        self.runner
            .run(
                args,
                Duration::from_secs(self.sync_config.operation_timeout_seconds),
            )
            .await
            .map(|_| ())
    }

    async fn download_blob(
        &self,
        remote_path: &str,
        local_path: &Path,
    ) -> Result<(), CloudTransportError> {
        let remote_path = validate_remote_path(remote_path)?;
        let mut args = self.base_args();
        args.push(OsString::from("copyto"));
        args.push(OsString::from(self.remote_target(remote_path)));
        args.push(local_path.as_os_str().to_os_string());
        args.push(OsString::from("--quiet"));
        args.push(OsString::from("--no-traverse"));

        tracing::debug!(remote_path = %remote_path, "rclone download");
        self.runner
            .run(
                args,
                Duration::from_secs(self.sync_config.operation_timeout_seconds),
            )
            .await
            .map(|_| ())
    }

    async fn delete_blob(&self, remote_path: &str) -> Result<(), CloudTransportError> {
        let remote_path = validate_remote_path(remote_path)?;
        let mut args = self.base_args();
        args.push(OsString::from("deletefile"));
        args.push(OsString::from(self.remote_target(remote_path)));
        args.push(OsString::from("--quiet"));

        tracing::debug!(remote_path = %remote_path, "rclone delete");
        self.runner
            .run(args, Duration::from_secs(30))
            .await
            .map(|_| ())
    }

    async fn list_blobs(&self, remote_prefix: &str) -> Result<Vec<String>, CloudTransportError> {
        let remote_prefix = validate_remote_prefix(remote_prefix)?;
        let mut args = self.base_args();
        args.push(OsString::from("lsjson"));
        let root_with_prefix = if remote_prefix.is_empty() {
            self.remote_root.clone()
        } else {
            self.remote_target(remote_prefix)
        };
        args.push(OsString::from(root_with_prefix));
        args.push(OsString::from("--recursive"));
        args.push(OsString::from("--files-only"));
        args.push(OsString::from("--no-mimetype"));
        args.push(OsString::from("--no-modtime"));

        let output = self.runner.run(args, Duration::from_secs(60)).await?;
        let mut parsed: Vec<String> = serde_json::from_str::<Vec<LsJsonEntry>>(&output)
            .map_err(|error| CloudTransportError::Other(format!("invalid lsjson output: {error}")))?
            .into_iter()
            .map(|entry| {
                if remote_prefix.is_empty() || entry.path.starts_with(remote_prefix) {
                    entry.path
                } else {
                    format!(
                        "{}/{}",
                        remote_prefix.trim_end_matches('/'),
                        entry.path.trim_start_matches('/')
                    )
                }
            })
            .collect();
        parsed.sort_unstable();
        Ok(parsed)
    }
}

fn build_remote_root(
    destination: &DestinationSessionPublic,
) -> Result<String, CloudTransportError> {
    compose_remote_root(
        &destination.rclone_remote_name,
        &destination.bucket,
        &destination.path_prefix,
    )
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;

    use super::*;
    use crate::storage::cloud::{BackupSyncMode, DestinationSessionPublic, DestinationType};
    use tokio::sync::Mutex;

    #[derive(Default)]
    struct StubRclone {
        scripted: Mutex<VecDeque<Result<String, CloudTransportError>>>,
    }

    impl StubRclone {
        fn from(scripted: Vec<Result<String, CloudTransportError>>) -> Arc<Self> {
            Arc::new(Self {
                scripted: Mutex::new(VecDeque::from(scripted)),
            })
        }
    }

    #[async_trait]
    impl RcloneRunner for StubRclone {
        async fn run(
            &self,
            _args: Vec<OsString>,
            _timeout: Duration,
        ) -> Result<String, CloudTransportError> {
            self.scripted
                .lock()
                .await
                .pop_front()
                .unwrap_or_else(|| Ok(String::new()))
        }
    }

    fn destination() -> DestinationSessionPublic {
        DestinationSessionPublic {
            destination_id: "dest-1".to_owned(),
            label: "primary".to_owned(),
            destination_type: DestinationType::Cloud,
            rclone_remote_name: "remote".to_owned(),
            bucket: "bucket".to_owned(),
            path_prefix: "vault".to_owned(),
            is_primary: true,
            backup_mode: Some(BackupSyncMode::Mirror),
        }
    }

    fn transport_with(scripted: Vec<Result<String, CloudTransportError>>) -> RcloneTransport {
        RcloneTransport::with_runner(
            PathBuf::from("session.conf"),
            &destination(),
            SyncConfig::default(),
            StubRclone::from(scripted),
        )
        .expect("destination should produce valid remote root")
    }

    #[tokio::test]
    async fn test_upload_blob_success() {
        let transport = transport_with(vec![Ok(String::new())]);
        let result = transport
            .upload_blob(Path::new("local.bin"), "vault-header.json")
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_download_blob_not_found_propagates() {
        let transport = transport_with(vec![Err(CloudTransportError::NotFound)]);
        let result = transport
            .download_blob("vault-header.json", Path::new("local.bin"))
            .await;
        assert!(matches!(result, Err(CloudTransportError::NotFound)));
    }

    #[tokio::test]
    async fn test_delete_blob_timeout_propagates() {
        let transport = transport_with(vec![Err(CloudTransportError::Timeout)]);
        let result = transport.delete_blob("vault-header.json").await;
        assert!(matches!(result, Err(CloudTransportError::Timeout)));
    }

    #[tokio::test]
    async fn test_list_blobs_parses_json_and_sorts() {
        let transport = transport_with(vec![Ok(
            r#"[{"Path":"vault/b.blob"},{"Path":"vault/a.blob"}]"#.to_owned(),
        )]);
        let result = transport.list_blobs("vault/").await.unwrap();
        assert_eq!(result, vec!["vault/a.blob", "vault/b.blob"]);
    }

    #[tokio::test]
    async fn test_non_zero_rclone_failure_is_preserved() {
        let transport = transport_with(vec![Err(CloudTransportError::RcloneProcessFailed {
            exit_code: 7,
            stderr_sanitised: "failed".to_owned(),
        })]);
        let result = transport.delete_blob("vault/a.blob").await;
        assert!(matches!(
            result,
            Err(CloudTransportError::RcloneProcessFailed { exit_code: 7, .. })
        ));
    }

    #[tokio::test]
    async fn test_authentication_failed_is_preserved() {
        let transport = transport_with(vec![Err(CloudTransportError::AuthenticationFailed)]);
        let result = transport
            .upload_blob(Path::new("local.bin"), "vault/a.blob")
            .await;
        assert!(matches!(
            result,
            Err(CloudTransportError::AuthenticationFailed)
        ));
    }

    #[tokio::test]
    async fn test_stubbed_round_trip_sequence_upload_list_download_delete() {
        let transport = transport_with(vec![
            Ok(String::new()),
            Ok(r#"[{"Path":"vault/header.json"}]"#.to_owned()),
            Ok(String::new()),
            Ok(String::new()),
        ]);

        transport
            .upload_blob(Path::new("header.bin"), "vault/header.json")
            .await
            .unwrap();
        let listed = transport.list_blobs("vault/").await.unwrap();
        assert_eq!(listed, vec!["vault/header.json"]);
        transport
            .download_blob("vault/header.json", Path::new("downloaded.bin"))
            .await
            .unwrap();
        transport.delete_blob("vault/header.json").await.unwrap();
    }

    #[test]
    fn test_transport_creation_fails_closed_for_invalid_remote_root_components() {
        let mut invalid = destination();
        invalid.path_prefix = "/vault".to_owned();
        let result = RcloneTransport::with_runner(
            PathBuf::from("session.conf"),
            &invalid,
            SyncConfig::default(),
            StubRclone::from(vec![]),
        );
        assert!(matches!(result, Err(CloudTransportError::Other(_))));
    }
}
