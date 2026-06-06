//! Production rclone-backed cloud transport implementation.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use super::destination_session::DestinationSessionPublic;
use super::rclone_subprocess::run_rclone;
use super::remote_path::{compose_remote_root, validate_remote_path, validate_remote_prefix};
use super::{CloudEndpoint, CloudTransport, CloudTransportError, DestinationType, SyncConfig};
use async_trait::async_trait;
use serde::Deserialize;
use zeroize::Zeroizing;

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
    /// The `remote_name:bucket` path used for container-level operations (e.g., mkdir).
    /// Does not include the path prefix. Empty for share-download transports.
    bucket_root: String,
    sync_config: SyncConfig,
    runner: Arc<dyn RcloneRunner>,
    /// Provider-specific sharing config (e.g. Google Drive SA JSON).
    /// Loaded from the vault DB at session open time; never logged.
    sharing_config: Option<serde_json::Value>,
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
            bucket_root: format!("{}:{}", destination.rclone_remote_name, destination.bucket),
            sync_config,
            runner: Arc::new(RealRclone { binary_path }),
            sharing_config: None,
        })
    }

    /// Creates a transport for downloading a received share using embedded B2 credentials.
    ///
    /// `remote_root` must be the full rclone remote root string, e.g. `"arxshare-dl:bucket/prefix"`.
    pub(crate) fn new_for_share_download(
        binary_path: PathBuf,
        session_config_path: PathBuf,
        remote_root: String,
    ) -> Self {
        Self {
            session_config_path,
            remote_root,
            bucket_root: String::new(),
            sync_config: SyncConfig::default(),
            runner: Arc::new(RealRclone { binary_path }),
            sharing_config: None,
        }
    }

    /// Attaches a provider-specific sharing configuration to this transport.
    ///
    /// For Google Drive remotes, `config` must be the parsed Service Account JSON object.
    /// This value is never logged; callers must zeroize the source string after calling.
    pub fn with_sharing_config(mut self, config: Option<serde_json::Value>) -> Self {
        self.sharing_config = config;
        self
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
            bucket_root: format!("{}:{}", destination.rclone_remote_name, destination.bucket),
            sync_config,
            runner,
            sharing_config: None,
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
            .run(
                args,
                Duration::from_secs(self.sync_config.operation_timeout_seconds),
            )
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

        let output = self
            .runner
            .run(
                args,
                Duration::from_secs(self.sync_config.operation_timeout_seconds),
            )
            .await?;
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

    async fn cleanup_session_artifacts(&self) -> Result<(), CloudTransportError> {
        Ok(())
    }

    async fn ensure_folder(&self, remote_prefix: &str) -> Result<(), CloudTransportError> {
        let remote_prefix = validate_remote_path(remote_prefix)?;
        let mut args = self.base_args();
        args.push(OsString::from("mkdir"));
        args.push(OsString::from(self.remote_target(remote_prefix)));
        tracing::debug!(remote_prefix = %remote_prefix, "rclone mkdir");
        self.runner
            .run(
                args,
                Duration::from_secs(self.sync_config.operation_timeout_seconds),
            )
            .await
            .map(|_| ())
    }

    async fn ensure_container(&self) -> Result<(), CloudTransportError> {
        let mut args = self.base_args();
        args.push(OsString::from("mkdir"));
        args.push(OsString::from(&self.bucket_root));
        match self
            .runner
            .run(
                args,
                Duration::from_secs(self.sync_config.operation_timeout_seconds),
            )
            .await
        {
            Ok(_) => Ok(()),
            Err(CloudTransportError::RcloneProcessFailed {
                ref stderr_sanitised,
                ..
            }) if stderr_sanitised.contains("already_exists")
                || stderr_sanitised
                    .to_ascii_lowercase()
                    .contains("bucket name is already in use") =>
            {
                Err(CloudTransportError::BucketNameTaken)
            }
            Err(e) => Err(e),
        }
    }

    /// Generates scoped credentials for a share recipient.
    ///
    /// For Backblaze B2 remotes: creates a time-limited scoped application key.
    /// For Google Drive remotes: grants the configured Service Account writer
    /// permission on the share folder and pre-creates the receipt placeholder
    /// blobs (so the quota-less SA can later overwrite them in place).
    /// Returns `SharingNotSupported` for other backends.
    async fn generate_share_credentials(
        &self,
        path_prefix: &str,
        ttl_seconds: u32,
    ) -> Result<Option<serde_json::Value>, CloudTransportError> {
        let conf = Zeroizing::new(
            tokio::fs::read_to_string(&self.session_config_path)
                .await
                .map_err(CloudTransportError::IoError)?,
        );

        let remote_name = self
            .remote_root
            .split_once(':')
            .map(|(name, _)| name)
            .unwrap_or("");

        // B2 path
        if let Some((master_key_id, master_app_key)) =
            crate::sharing::b2_api::parse_b2_api_keys_for_remote(&conf, remote_name)
        {
            return self
                .generate_b2_share_credentials(
                    path_prefix,
                    ttl_seconds,
                    &master_key_id,
                    &master_app_key,
                )
                .await;
        }

        // Google Drive path
        if let Some((client_id, client_secret, refresh_token, root_folder_id)) =
            crate::sharing::gdrive_api::parse_gdrive_oauth_from_conf(&conf, remote_name)
        {
            return self
                .generate_gdrive_share_credentials(
                    path_prefix,
                    ttl_seconds,
                    &client_id,
                    &client_secret,
                    &refresh_token,
                    root_folder_id.as_deref(),
                )
                .await;
        }

        // SAFETY: message is a static string literal — safe to forward to frontend via IpcError.
        Err(CloudTransportError::SharingNotSupported(format!(
            "Remote '{remote_name}' is not a supported sharing backend \
             (Backblaze B2 or Google Drive)."
        )))
    }
}

impl RcloneTransport {
    /// Pre-creates the owner-owned `receipts/` and `import-receipts/` placeholder
    /// blobs for a Google Drive share.
    ///
    /// Uploaded via the owner's own (quota-bearing) transport so the recipient's
    /// Service Account can later overwrite them in place. Best-effort: every
    /// failure is logged at `warn` and does not abort share creation.
    async fn precreate_gdrive_receipt_placeholders(&self, path_prefix: &str) {
        let base = path_prefix.trim_end_matches('/');
        let local = std::env::temp_dir().join(format!(
            "arx-rcpt-placeholder-{}.blob",
            uuid::Uuid::new_v4()
        ));
        if let Err(e) = tokio::fs::write(&local, crate::sharing::RECEIPT_PLACEHOLDER).await {
            tracing::warn!(%e, "receipt placeholder: temp write failed");
            return;
        }
        for dir in ["receipts", "import-receipts"] {
            let remote_path = format!("{base}/{dir}/{}", crate::sharing::RECEIPT_BLOB_NAME);
            if let Err(e) = self.upload_blob(&local, &remote_path).await {
                tracing::warn!(%e, dir, "receipt placeholder: pre-create failed");
            }
        }
        let _ = tokio::fs::remove_file(&local).await;
    }

    async fn generate_b2_share_credentials(
        &self,
        path_prefix: &str,
        ttl_seconds: u32,
        master_key_id: &str,
        master_app_key: &str,
    ) -> Result<Option<serde_json::Value>, CloudTransportError> {
        let bucket_name = self
            .remote_root
            .split_once(':')
            .and_then(|(_, rest)| rest.split('/').next())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| {
                CloudTransportError::Other("cannot extract bucket name from remote root".to_owned())
            })?
            .to_owned();

        let auth = crate::sharing::b2_api::b2_authorize_account(master_key_id, master_app_key)
            .await
            .map_err(|_| CloudTransportError::Other("B2 authorization failed".to_owned()))?;

        let client = reqwest::Client::new();
        let bucket_id = crate::sharing::b2_api::b2_get_bucket_id(&client, &auth, &bucket_name)
            .await
            .map_err(|e| {
                CloudTransportError::Other(format!(
                    "B2 bucket lookup failed for '{bucket_name}': {e}"
                ))
            })?;

        let capabilities = vec!["readFiles", "listBuckets", "writeFiles"];

        let app_key = crate::sharing::b2_api::b2_create_application_key(
            &client,
            &auth,
            &bucket_id,
            path_prefix,
            &capabilities,
            ttl_seconds,
        )
        .await
        .map_err(|e| {
            CloudTransportError::Other(format!(
                "B2 key creation failed (check that your key has writeKeys capability): {e}"
            ))
        })?;

        tracing::debug!(key_id = %app_key.application_key_id, "created B2 scoped key");

        let creds = serde_json::json!({
            "provider": "b2",
            "bucket": bucket_name,
            "download_url": auth.download_url,
            "key_id": app_key.application_key_id,
            "application_key": app_key.application_key,
            "path_prefix": path_prefix,
            "receipt_requested": true,
        });

        Ok(Some(creds))
    }

    async fn generate_gdrive_share_credentials(
        &self,
        path_prefix: &str,
        _ttl_seconds: u32,
        client_id: &str,
        client_secret: &str,
        refresh_token: &str,
        root_folder_id: Option<&str>,
    ) -> Result<Option<serde_json::Value>, CloudTransportError> {
        use crate::sharing::gdrive_api;

        let sa_config = self.sharing_config.as_ref().ok_or_else(|| {
            // SAFETY: message is a static string literal — safe to forward to frontend via IpcError.
            CloudTransportError::SharingNotSupported(
                "Google Drive sharing requires a Service Account to be configured \
                 in vault settings (Destinations → Sharing Setup)."
                    .to_owned(),
            )
        })?;

        let sa_email = sa_config["client_email"]
            .as_str()
            .ok_or_else(|| {
                CloudTransportError::Other(
                    "Service Account JSON missing 'client_email' field".to_owned(),
                )
            })?
            .to_owned();

        let sa_json_str = Zeroizing::new(serde_json::to_string(sa_config).map_err(|e| {
            CloudTransportError::Other(format!("failed to serialise SA JSON: {e}"))
        })?);
        // Note: `sa_json_str` is subsequently moved into a `serde_json::Value` via `json!{}`.
        // `serde_json::Value` does not implement `Zeroize`, so the heap copy inside that value
        // is not covered by zeroing on drop. The `Zeroizing` wrapper above still ensures this
        // local binding is zeroed when it goes out of scope.

        let client = reqwest::Client::new();
        let token = if client_id.is_empty() {
            // No custom OAuth credentials stored (rclone built-in app); read the access token
            // that rclone wrote to the session config during the most recent sync operation.
            let conf = Zeroizing::new(
                tokio::fs::read_to_string(&self.session_config_path)
                    .await
                    .map_err(CloudTransportError::IoError)?,
            );
            let remote_name = self
                .remote_root
                .split_once(':')
                .map(|(n, _)| n)
                .unwrap_or("");
            let access_token = gdrive_api::parse_gdrive_access_token_from_conf(&conf, remote_name)
                .ok_or_else(|| {
                    CloudTransportError::Other(
                        "Drive access token not found in session config; \
                             sync the vault to refresh it."
                            .to_owned(),
                    )
                })?;
            gdrive_api::GdriveAccessToken { access_token }
        } else {
            gdrive_api::gdrive_refresh_token(&client, client_id, client_secret, refresh_token)
                .await
                .map_err(|e| {
                    CloudTransportError::Other(format!("Drive token refresh failed: {e}"))
                })?
        };

        // remote_root is "remote:dest_path_prefix" (e.g. "arx_20e5c4d0:arx-runa-new").
        // path_prefix is relative to that destination root (e.g. "shared/<id>/").
        // Drive folder resolution must walk the full path from root_folder_id (or Drive root).
        let dest_path = self
            .remote_root
            .split_once(':')
            .map(|(_, p)| p.trim_matches('/'))
            .unwrap_or("");
        let full_drive_path = if dest_path.is_empty() {
            path_prefix.to_owned()
        } else {
            format!("{}/{}", dest_path, path_prefix.trim_start_matches('/'))
        };

        let folder_id = gdrive_api::gdrive_resolve_folder_id(
            &client,
            &token.access_token,
            root_folder_id,
            &full_drive_path,
        )
        .await
        .map_err(|e| {
            CloudTransportError::Other(format!(
                "Drive folder lookup failed for '{full_drive_path}': {e}"
            ))
        })?;

        let permission = gdrive_api::gdrive_create_permission(
            &client,
            &token.access_token,
            &folder_id,
            &sa_email,
            // Google Drive rejects expirationTime on personal My Drive items; revocation
            // deletes the permission explicitly so a TTL is not required for correctness.
            None,
        )
        .await
        .map_err(|e| CloudTransportError::Other(format!("Drive permission grant failed: {e}")))?;

        tracing::debug!(permission_id = %permission.permission_id, "granted Google Drive SA permission");

        // Pre-create the owner-owned receipt blobs so the recipient SA (no Drive
        // storage quota) can update them in place instead of creating new files.
        // Best-effort: failure only disables receipts, not the share itself.
        self.precreate_gdrive_receipt_placeholders(path_prefix)
            .await;

        Ok(Some(serde_json::json!({
            "provider": "drive",
            "folder_id": folder_id,
            "sa_credentials_json": sa_json_str.as_str(),
            "path_prefix": path_prefix,
            "permission_id": permission.permission_id,
            // Mirrors B2: signals the recipient to upload delivery-receipt blobs. The
            // SA is granted writer access (see `gdrive_create_permission`) so it can.
            "receipt_requested": true,
        })))
    }
}

fn build_remote_root(
    destination: &DestinationSessionPublic,
) -> Result<String, CloudTransportError> {
    match destination.destination_type {
        DestinationType::LocalPath | DestinationType::ExternalDrive => {
            let path = destination.path_prefix.replace('\\', "/");
            super::remote_path::validate_local_path_prefix(&path)?;
            Ok(format!("{}:{}", destination.rclone_remote_name, path))
        }
        _ => compose_remote_root(
            &destination.rclone_remote_name,
            &destination.bucket,
            &destination.path_prefix,
        ),
    }
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

    #[test]
    fn test_transport_creation_fails_for_local_path_with_parent_traversal() {
        let mut invalid = destination();
        invalid.destination_type = DestinationType::LocalPath;
        invalid.path_prefix = "/home/user/../../etc/passwd".to_owned();
        let result = RcloneTransport::with_runner(
            PathBuf::from("session.conf"),
            &invalid,
            SyncConfig::default(),
            StubRclone::from(vec![]),
        );
        assert!(matches!(result, Err(CloudTransportError::Other(_))));
    }

    #[test]
    fn test_transport_creation_succeeds_for_local_path_with_drive_letter() {
        let mut local = destination();
        local.destination_type = DestinationType::LocalPath;
        local.rclone_remote_name = "local-drive".to_owned();
        local.path_prefix = "D:/vault/arx".to_owned();
        let result = RcloneTransport::with_runner(
            PathBuf::from("session.conf"),
            &local,
            SyncConfig::default(),
            StubRclone::from(vec![]),
        );
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_ensure_container_bucket_name_taken_returns_bucket_name_taken_error() {
        let transport = transport_with(vec![Err(CloudTransportError::RcloneProcessFailed {
            exit_code: 1,
            stderr_sanitised: "b2 bucket already_exists".to_string(),
        })]);
        let result = transport.ensure_container().await;
        assert!(matches!(result, Err(CloudTransportError::BucketNameTaken)));
    }

    #[tokio::test]
    async fn test_ensure_container_success_returns_ok() {
        let transport = transport_with(vec![Ok(String::new())]);
        let result = transport.ensure_container().await;
        assert!(result.is_ok());
    }
}
