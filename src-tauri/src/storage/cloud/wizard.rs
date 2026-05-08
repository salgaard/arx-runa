//! Cloud provider setup helpers.

use std::ffi::OsString;
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::time::Duration;

use async_trait::async_trait;
use tokio::sync::oneshot;
use uuid::Uuid;
use zeroize::Zeroize;

use super::cloud_config::save_primary_cloud_endpoint;
use super::destination_session::{
    BackupSyncMode, DestinationSession, DestinationSessionPublic, DestinationType,
    delete_destination_session, destroy_session_rclone_conf, insert_destination_session,
};
use super::rclone_subprocess::run_rclone;
use super::remote_path::compose_remote_root;
use super::{CloudEndpoint, CloudTransportError};
use crate::storage::sqlcipher::SqlCipherMetadataStore;

type SaveEndpointFuture = Pin<Box<dyn Future<Output = Result<(), CloudTransportError>> + Send>>;
type SaveEndpointCallback = dyn Fn(&CloudEndpoint) -> SaveEndpointFuture + Send + Sync;

fn save_primary_cloud_endpoint_boxed(endpoint: &CloudEndpoint) -> SaveEndpointFuture {
    let endpoint = endpoint.clone();
    Box::pin(async move { save_primary_cloud_endpoint(&endpoint).await })
}

/// Request payload for S3 provider setup.
#[derive(Debug, Clone)]
pub struct S3SetupRequest {
    pub label: String,
    pub provider: String,
    pub bucket: String,
    pub region: String,
    pub endpoint: String,
    pub path_prefix: String,
    pub access_key_id: String,
    pub secret_access_key: String,
    pub is_primary: bool,
    pub backup_mode: Option<BackupSyncMode>,
}

impl Drop for S3SetupRequest {
    fn drop(&mut self) {
        self.access_key_id.zeroize();
        self.secret_access_key.zeroize();
    }
}

/// Request payload for Google Drive setup.
#[derive(Debug, Clone)]
pub struct GoogleDriveSetupRequest {
    pub label: String,
    pub path_prefix: String,
    pub is_primary: bool,
    pub backup_mode: Option<BackupSyncMode>,
    pub await_completion: bool,
}

/// Trusted runtime paths for Google Drive setup execution.
#[derive(Debug, Clone)]
pub struct GoogleDriveRuntimePaths {
    pub binary_path: PathBuf,
    pub config_path: PathBuf,
}

/// Result state for Google Drive setup flow.
#[derive(Debug, Clone)]
pub enum GoogleDriveSetupResult {
    GoogleDriveAuthPending { auth_url: String },
    Completed(DestinationSessionPublic),
}

/// Minimal opener abstraction to avoid binding wizard logic to UI/runtime concerns.
#[async_trait]
pub trait OpenerLike: Send + Sync {
    async fn open_url(&self, url: &str) -> Result<(), CloudTransportError>;
}

/// Sets up an S3 destination and persists destination + non-sensitive endpoint metadata.
pub async fn setup_s3_provider(
    input: S3SetupRequest,
    store: &SqlCipherMetadataStore,
) -> Result<DestinationSessionPublic, CloudTransportError> {
    setup_s3_provider_with_endpoint_saver(input, store, &save_primary_cloud_endpoint_boxed).await
}

async fn setup_s3_provider_with_endpoint_saver(
    input: S3SetupRequest,
    store: &SqlCipherMetadataStore,
    save_primary_endpoint: &SaveEndpointCallback,
) -> Result<DestinationSessionPublic, CloudTransportError> {
    let mut input = input;
    let endpoint = CloudEndpoint {
        provider: "s3".to_owned(),
        bucket: input.bucket.clone(),
        region: input.region.clone(),
        endpoint: input.endpoint.clone(),
        path_prefix: input.path_prefix.clone(),
    };
    endpoint.validate()?;
    let remote_name = format!("arx-runa-{}", Uuid::new_v4());
    compose_remote_root(&remote_name, &input.bucket, &input.path_prefix)?;
    let mut destination = DestinationSession {
        destination_id: Uuid::new_v4().hyphenated().to_string(),
        label: input.label.clone(),
        destination_type: DestinationType::Cloud,
        rclone_remote_name: remote_name.clone(),
        rclone_config_blob: format!(
            "[{remote_name}]\ntype = s3\nprovider = {}\nregion = {}\nendpoint = {}\naccess_key_id = {}\nsecret_access_key = {}\n",
            input.provider,
            input.region,
            input.endpoint,
            input.access_key_id,
            input.secret_access_key
        ),
        bucket: input.bucket.clone(),
        path_prefix: input.path_prefix.clone(),
        is_primary: input.is_primary,
        backup_mode: input.backup_mode.clone(),
    };

    let persist_result = insert_destination_with_primary_endpoint(
        store,
        &destination,
        destination.is_primary.then_some(&endpoint),
        save_primary_endpoint,
    )
    .await;
    if let Err(error) = persist_result {
        destination.rclone_config_blob.zeroize();
        return Err(error);
    }

    input.access_key_id.zeroize();
    input.secret_access_key.zeroize();
    let destination_public = DestinationSessionPublic::from(&destination);
    destination.rclone_config_blob.zeroize();
    Ok(destination_public)
}

/// Sets up a Google Drive destination.
pub async fn setup_google_drive(
    opener: &impl OpenerLike,
    input: GoogleDriveSetupRequest,
    runtime_paths: &GoogleDriveRuntimePaths,
    store: &SqlCipherMetadataStore,
    oauth_completed: oneshot::Receiver<()>,
) -> Result<GoogleDriveSetupResult, CloudTransportError> {
    setup_google_drive_with_remote_name(
        opener,
        input,
        runtime_paths,
        store,
        oauth_completed,
        format!("arx-runa-{}", Uuid::new_v4()),
        &save_primary_cloud_endpoint_boxed,
    )
    .await
}

async fn setup_google_drive_with_remote_name(
    opener: &impl OpenerLike,
    input: GoogleDriveSetupRequest,
    runtime_paths: &GoogleDriveRuntimePaths,
    store: &SqlCipherMetadataStore,
    oauth_completed: oneshot::Receiver<()>,
    remote_name: String,
    save_primary_endpoint: &SaveEndpointCallback,
) -> Result<GoogleDriveSetupResult, CloudTransportError> {
    let result = async {
        compose_remote_root(&remote_name, "", &input.path_prefix)?;
        let create_output = run_rclone(
            &runtime_paths.binary_path,
            vec![
                OsString::from("config"),
                OsString::from("create"),
                OsString::from(&remote_name),
                OsString::from("drive"),
                OsString::from("scope=drive"),
                OsString::from("--non-interactive"),
                OsString::from("--config"),
                runtime_paths.config_path.as_os_str().to_os_string(),
            ],
            Duration::from_secs(60),
        )
        .await?;

        let auth_url = parse_google_auth_url(&create_output)?;
        opener.open_url(&auth_url).await?;

        if !input.await_completion {
            return Ok(GoogleDriveSetupResult::GoogleDriveAuthPending { auth_url });
        }

        oauth_completed.await.map_err(|_| {
            CloudTransportError::Other("google drive oauth wait cancelled".to_owned())
        })?;

        let mut dump_output = run_rclone(
            &runtime_paths.binary_path,
            vec![
                OsString::from("config"),
                OsString::from("dump"),
                OsString::from("--config"),
                runtime_paths.config_path.as_os_str().to_os_string(),
            ],
            Duration::from_secs(60),
        )
        .await?;
        let rclone_config_blob = match extract_remote_blob_from_dump(&dump_output, &remote_name) {
            Ok(blob) => blob,
            Err(error) => {
                dump_output.zeroize();
                return Err(error);
            }
        };
        dump_output.zeroize();

        let mut destination = DestinationSession {
            destination_id: Uuid::new_v4().hyphenated().to_string(),
            label: input.label.clone(),
            destination_type: DestinationType::Cloud,
            rclone_remote_name: remote_name,
            rclone_config_blob,
            bucket: String::new(),
            path_prefix: input.path_prefix.clone(),
            is_primary: input.is_primary,
            backup_mode: input.backup_mode.clone(),
        };

        let primary_endpoint = if destination.is_primary {
            let endpoint = CloudEndpoint {
                provider: "drive".to_owned(),
                bucket: String::new(),
                region: String::new(),
                endpoint: String::new(),
                path_prefix: input.path_prefix.clone(),
            };
            endpoint.validate()?;
            Some(endpoint)
        } else {
            None
        };

        let persist_result = insert_destination_with_primary_endpoint(
            store,
            &destination,
            primary_endpoint.as_ref(),
            save_primary_endpoint,
        )
        .await;
        if let Err(error) = persist_result {
            destination.rclone_config_blob.zeroize();
            return Err(error);
        }

        let destination_public = DestinationSessionPublic::from(&destination);
        destination.rclone_config_blob.zeroize();
        Ok(GoogleDriveSetupResult::Completed(destination_public))
    }
    .await;

    if !matches!(
        result,
        Ok(GoogleDriveSetupResult::GoogleDriveAuthPending { .. })
    ) {
        return attach_google_drive_runtime_cleanup(result, &runtime_paths.config_path).await;
    }
    result
}

async fn insert_destination_with_primary_endpoint(
    store: &SqlCipherMetadataStore,
    destination: &DestinationSession,
    primary_endpoint: Option<&CloudEndpoint>,
    save_primary_endpoint: &SaveEndpointCallback,
) -> Result<(), CloudTransportError> {
    insert_destination_session(store, destination)
        .await
        .map_err(|error| CloudTransportError::Other(error.to_string()))?;

    if let Some(endpoint) = primary_endpoint
        && let Err(save_error) = save_primary_endpoint(endpoint).await
    {
        if let Err(rollback_error) =
            delete_destination_session(store, &destination.destination_id).await
        {
            return Err(CloudTransportError::Other(format!(
                "failed to save primary cloud endpoint and rollback destination: save={save_error}; rollback={rollback_error}"
            )));
        }
        return Err(save_error);
    }
    Ok(())
}

async fn attach_google_drive_runtime_cleanup(
    result: Result<GoogleDriveSetupResult, CloudTransportError>,
    config_path: &std::path::Path,
) -> Result<GoogleDriveSetupResult, CloudTransportError> {
    let cleanup_result = destroy_session_rclone_conf(config_path).await;
    match (result, cleanup_result) {
        (Ok(value), Ok(())) => Ok(value),
        (Err(error), Ok(())) => Err(error),
        (Ok(_), Err(cleanup_error)) => Err(cleanup_error),
        (Err(error), Err(cleanup_error)) => Err(CloudTransportError::Other(format!(
            "{error}; google drive runtime config cleanup failed: {cleanup_error}"
        ))),
    }
}

fn parse_google_auth_url(output: &str) -> Result<String, CloudTransportError> {
    const PREFIX: &str = "If your browser doesn't open automatically, go to the following link:";
    output
        .lines()
        .find_map(|line| {
            if line.contains(PREFIX) {
                line.rfind("http")
                    .map(|index| line[index..].trim().to_owned())
            } else {
                None
            }
        })
        .ok_or_else(|| {
            CloudTransportError::Other("missing google auth URL in rclone output".to_owned())
        })
}

fn extract_remote_blob_from_dump(
    dump_output: &str,
    remote_name: &str,
) -> Result<String, CloudTransportError> {
    let parsed: serde_json::Value = serde_json::from_str(dump_output).map_err(|error| {
        CloudTransportError::Other(format!("invalid rclone config dump: {error}"))
    })?;
    let remotes = parsed.as_object().ok_or_else(|| {
        CloudTransportError::Other("invalid rclone config dump: expected object".to_owned())
    })?;
    let remote = remotes.get(remote_name).ok_or_else(|| {
        CloudTransportError::Other(format!("missing remote stanza in dump for '{remote_name}'"))
    })?;
    let settings = remote.as_object().ok_or_else(|| {
        CloudTransportError::Other(format!(
            "invalid rclone remote settings for '{remote_name}': expected object"
        ))
    })?;

    let mut lines = vec![format!("[{remote_name}]")];
    let mut keys: Vec<&str> = settings.keys().map(String::as_str).collect();
    keys.sort_unstable();
    for key in keys {
        let value = settings.get(key).expect("key should exist");
        if value.is_null() {
            return Err(CloudTransportError::Other(format!(
                "invalid rclone value for key '{key}' in remote '{remote_name}'"
            )));
        }
        let rendered = value
            .as_str()
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| value.to_string());
        lines.push(format!("{key} = {rendered}"));
    }
    Ok(format!("{}\n", lines.join("\n")))
}

/// Which cloud provider to use for an OAuth setup flow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OAuthProvider {
    /// Google Drive (rclone type `drive`).
    GoogleDrive,
    /// Microsoft OneDrive personal (rclone type `onedrive`).
    OneDrive,
}

/// Return value of `begin_oauth_setup` — the live subprocess and all metadata
/// needed for polling, cancellation, and insertion into `AppState.oauth_setups`.
pub struct OAuthSetupBegun {
    /// Opaque identifier for this setup, used for polling and cancellation.
    pub setup_id: String,
    /// Local rclone auth URL the frontend must open in the system browser.
    pub auth_url: String,
    /// Live rclone subprocess waiting for the OAuth browser callback.
    pub child: tokio::process::Child,
    /// Temporary rclone config file written by this subprocess.
    pub temp_config_path: PathBuf,
    /// Rclone remote name written into the temp config.
    pub remote_name: String,
}

/// Spawns rclone for an OAuth provider setup flow.
///
/// Reads rclone stderr line-by-line until the local auth URL
/// (`http://127.0.0.1:…`) is found, then spawns a background drain task so
/// the pipe buffer cannot stall the rclone process.  The caller must insert the
/// returned handle into `AppState.oauth_setups`.
pub async fn begin_oauth_setup(
    provider: OAuthProvider,
    binary_path: &std::path::Path,
) -> Result<OAuthSetupBegun, CloudTransportError> {
    use std::ffi::OsStr;
    use tokio::io::AsyncBufReadExt as _;
    use tokio::io::BufReader;

    let setup_id = Uuid::new_v4().hyphenated().to_string();
    let remote_name = format!("arx-runa-{}", Uuid::new_v4());
    let temp_config_path = std::env::temp_dir().join(format!("arx-runa-oauth-{setup_id}.conf"));

    let rclone_type = match provider {
        OAuthProvider::GoogleDrive => "drive",
        OAuthProvider::OneDrive => "onedrive",
    };

    let mut extra_args: Vec<&OsStr> = Vec::new();
    let scope_flag;
    if provider == OAuthProvider::GoogleDrive {
        scope_flag = OsString::from("scope=drive");
        extra_args.push(scope_flag.as_os_str());
    }

    let mut command = tokio::process::Command::new(binary_path);
    command
        .args([
            OsStr::new("config"),
            OsStr::new("create"),
            OsStr::new(&remote_name),
            OsStr::new(rclone_type),
        ])
        .args(&extra_args)
        .args([
            OsStr::new("--non-interactive"),
            OsStr::new("--config"),
            temp_config_path.as_os_str(),
        ])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped());

    let mut child = command.spawn().map_err(CloudTransportError::from)?;

    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| CloudTransportError::Other("failed to capture rclone stderr".to_owned()))?;

    let mut reader = BufReader::new(stderr).lines();
    let mut auth_url = None;
    while let Some(line) = reader.next_line().await.map_err(|error| {
        CloudTransportError::Other(format!("reading rclone stderr: {error}"))
    })? {
        if line.contains("http://127.0.0.1:") {
            auth_url = line
                .rfind("http://127.0.0.1:")
                .map(|index| line[index..].trim().to_owned());
            break;
        }
    }
    let auth_url = auth_url.ok_or_else(|| {
        CloudTransportError::Other("rclone did not emit an auth URL on stderr".to_owned())
    })?;

    tokio::spawn(async move {
        while let Ok(Some(_)) = reader.next_line().await {}
    });

    Ok(OAuthSetupBegun {
        setup_id,
        auth_url,
        child,
        temp_config_path,
        remote_name,
    })
}

/// Runs `rclone config dump` against the given temp config and extracts the
/// credential INI stanza for `remote_name`.
///
/// The caller receives the raw blob and is responsible for passing it to
/// `add_destination`; nothing is persisted here.
pub async fn complete_oauth_setup(
    binary_path: &std::path::Path,
    temp_config_path: &std::path::Path,
    remote_name: &str,
) -> Result<String, CloudTransportError> {
    let mut dump_output = run_rclone(
        binary_path,
        vec![
            OsString::from("config"),
            OsString::from("dump"),
            OsString::from("--config"),
            temp_config_path.as_os_str().to_os_string(),
        ],
        Duration::from_secs(15),
    )
    .await?;

    let blob = match extract_remote_blob_from_dump(&dump_output, remote_name) {
        Ok(blob) => blob,
        Err(error) => {
            dump_output.zeroize();
            return Err(error);
        }
    };
    dump_output.zeroize();

    if let Err(error) = destroy_session_rclone_conf(temp_config_path).await {
        tracing::warn!(error = %error, "failed to remove oauth temp config");
    }

    Ok(blob)
}

/// Kills the rclone subprocess and removes the temporary config file.
///
/// Safe to call even if the process has already exited.
pub async fn cancel_oauth_setup(
    child: &mut tokio::process::Child,
    temp_config_path: &std::path::Path,
) {
    if let Err(error) = child.start_kill() {
        tracing::warn!(error = %error, "cancel_oauth_setup: failed to kill rclone child");
    }
    if let Err(error) =
        tokio::time::timeout(Duration::from_secs(5), child.wait()).await
    {
        tracing::warn!(error = %error, "cancel_oauth_setup: failed to reap rclone child");
    }
    if let Err(error) = destroy_session_rclone_conf(temp_config_path).await {
        tracing::warn!(error = %error, "cancel_oauth_setup: failed to remove temp config");
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use tempfile::tempdir;
    use tokio::sync::{Mutex, oneshot};
    use uuid::Uuid;

    use super::{
        CloudEndpoint, DestinationType, GoogleDriveRuntimePaths, GoogleDriveSetupRequest,
        GoogleDriveSetupResult, OpenerLike, S3SetupRequest, extract_remote_blob_from_dump,
        setup_google_drive, setup_google_drive_with_remote_name, setup_s3_provider,
        setup_s3_provider_with_endpoint_saver,
    };
    use crate::storage::CloudTransportError;
    use crate::storage::cloud::destination_session::list_destination_sessions;
    use crate::storage::sqlcipher::SqlCipherMetadataStore;

    #[derive(Default)]
    struct RecordingOpener {
        urls: Mutex<Vec<String>>,
    }

    #[async_trait::async_trait]
    impl OpenerLike for RecordingOpener {
        async fn open_url(&self, url: &str) -> Result<(), CloudTransportError> {
            self.urls.lock().await.push(url.to_owned());
            Ok(())
        }
    }

    async fn create_store() -> (tempfile::TempDir, SqlCipherMetadataStore) {
        let directory = tempdir().expect("tempdir should be created");
        let db_path = directory.path().join("manifest.db");
        let store =
            SqlCipherMetadataStore::create(&db_path, &[7u8; 32], Uuid::new_v4(), 4_194_304, false)
                .await
                .expect("store should be created");
        (directory, store)
    }

    fn ok_save_endpoint(_endpoint: &CloudEndpoint) -> super::SaveEndpointFuture {
        Box::pin(async { Ok(()) })
    }

    fn fail_save_endpoint(_endpoint: &CloudEndpoint) -> super::SaveEndpointFuture {
        Box::pin(async {
            Err(CloudTransportError::Other(
                "simulated cloud-config save failure".to_owned(),
            ))
        })
    }

    #[tokio::test]
    async fn test_setup_s3_provider_persists_destination_without_credential_endpoint_fields() {
        let (_directory, store) = create_store().await;
        let request = S3SetupRequest {
            label: "primary".to_owned(),
            provider: "AWS".to_owned(),
            bucket: "bucket".to_owned(),
            region: "us-east-1".to_owned(),
            endpoint: "https://s3.example.com".to_owned(),
            path_prefix: "vault".to_owned(),
            access_key_id: "AKIA...".to_owned(),
            secret_access_key: "secret".to_owned(),
            is_primary: false,
            backup_mode: None,
        };

        let destination = setup_s3_provider(request, &store)
            .await
            .expect("s3 setup should succeed");
        assert_eq!(destination.destination_type, DestinationType::Cloud);
        assert!(!destination.rclone_remote_name.is_empty());

        let sessions = list_destination_sessions(&store).await.unwrap();
        assert_eq!(sessions.len(), 1);
        assert!(!sessions[0].rclone_config_blob.contains("password"));
    }

    #[tokio::test]
    async fn test_setup_s3_provider_rejects_invalid_root_components() {
        let (_directory, store) = create_store().await;
        let request = S3SetupRequest {
            label: "primary".to_owned(),
            provider: "AWS".to_owned(),
            bucket: "bucket".to_owned(),
            region: "us-east-1".to_owned(),
            endpoint: "https://s3.example.com".to_owned(),
            path_prefix: "/vault".to_owned(),
            access_key_id: "AKIA...".to_owned(),
            secret_access_key: "secret".to_owned(),
            is_primary: false,
            backup_mode: None,
        };

        let result = setup_s3_provider(request, &store).await;
        assert!(matches!(result, Err(CloudTransportError::Other(_))));
    }

    #[tokio::test]
    async fn test_setup_s3_provider_rolls_back_destination_when_primary_endpoint_save_fails() {
        let (_directory, store) = create_store().await;
        let request = S3SetupRequest {
            label: "primary".to_owned(),
            provider: "AWS".to_owned(),
            bucket: "bucket".to_owned(),
            region: "us-east-1".to_owned(),
            endpoint: "https://s3.example.com".to_owned(),
            path_prefix: "vault".to_owned(),
            access_key_id: "AKIA...".to_owned(),
            secret_access_key: "secret".to_owned(),
            is_primary: true,
            backup_mode: None,
        };
        let result =
            setup_s3_provider_with_endpoint_saver(request, &store, &fail_save_endpoint).await;
        assert!(matches!(result, Err(CloudTransportError::Other(_))));
        let sessions = list_destination_sessions(&store).await.unwrap();
        assert!(sessions.is_empty());
    }

    #[test]
    fn test_extract_remote_blob_from_dump_returns_single_remote_stanza() {
        let dump = r#"{
            "remote-a": {"type": "drive", "token": "abc"},
            "remote-b": {"type": "s3"}
        }"#;
        let blob = extract_remote_blob_from_dump(dump, "remote-a").expect("extract should succeed");
        assert!(blob.contains("[remote-a]"));
        assert!(blob.contains("type = drive"));
        assert!(!blob.contains("[remote-b]"));
    }

    #[test]
    fn test_extract_remote_blob_from_dump_rejects_missing_remote() {
        let dump = r#"{"remote-a":{"type":"drive"}}"#;
        let result = extract_remote_blob_from_dump(dump, "remote-missing");
        assert!(matches!(result, Err(CloudTransportError::Other(_))));
    }

    #[test]
    fn test_extract_remote_blob_from_dump_allows_object_values() {
        let dump = r#"{"remote-a":{"type":"drive","token":{"access_token":"abc"}}}"#;
        let blob = extract_remote_blob_from_dump(dump, "remote-a").expect("extract should succeed");
        assert!(blob.contains(r#"token = {"access_token":"abc"}"#));
    }

    #[tokio::test]
    async fn test_google_drive_result_pending_variant_carries_auth_url() {
        let pending = GoogleDriveSetupResult::GoogleDriveAuthPending {
            auth_url: "https://example.com".to_owned(),
        };
        match pending {
            GoogleDriveSetupResult::GoogleDriveAuthPending { auth_url } => {
                assert_eq!(auth_url, "https://example.com")
            }
            _ => panic!("unexpected variant"),
        }
    }

    #[tokio::test]
    async fn test_google_drive_setup_request_is_constructible_for_mock_opener_flow() {
        let opener = RecordingOpener::default();
        opener.open_url("https://example.com").await.unwrap();
        let urls = opener.urls.lock().await.clone();
        assert_eq!(urls, vec!["https://example.com"]);

        let (_tx, rx) = oneshot::channel::<()>();
        let request = GoogleDriveSetupRequest {
            label: "gdrive".to_owned(),
            path_prefix: "vault".to_owned(),
            is_primary: true,
            backup_mode: None,
            await_completion: false,
        };
        assert!(!request.label.is_empty());
        drop(rx);
    }

    #[cfg(unix)]
    fn fixture_binary_path() -> PathBuf {
        use std::os::unix::fs::PermissionsExt;

        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fake_rclone.sh");
        let mut permissions = std::fs::metadata(&path).unwrap().permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&path, permissions).unwrap();
        path
    }

    #[cfg(windows)]
    fn fixture_binary_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fake_rclone.cmd")
    }

    #[tokio::test]
    async fn test_setup_google_drive_returns_pending_and_records_opened_url() {
        let (_directory, store) = create_store().await;
        let opener = RecordingOpener::default();
        let temp = tempdir().unwrap();
        let runtime = GoogleDriveRuntimePaths {
            binary_path: fixture_binary_path(),
            config_path: temp.path().join("rclone.conf"),
        };
        let (_tx, rx) = oneshot::channel();
        let result = setup_google_drive(
            &opener,
            GoogleDriveSetupRequest {
                label: "drive".to_owned(),
                path_prefix: "vault".to_owned(),
                is_primary: true,
                backup_mode: None,
                await_completion: false,
            },
            &runtime,
            &store,
            rx,
        )
        .await
        .expect("google drive setup should succeed");

        match result {
            GoogleDriveSetupResult::GoogleDriveAuthPending { auth_url } => {
                assert_eq!(auth_url, "https://example.com/oauth");
            }
            _ => panic!("expected pending variant"),
        }
        let urls = opener.urls.lock().await.clone();
        assert_eq!(urls, vec!["https://example.com/oauth"]);
    }

    #[tokio::test]
    async fn test_setup_google_drive_terminal_success_cleans_runtime_config() {
        let (_directory, store) = create_store().await;
        let opener = RecordingOpener::default();
        let temp = tempdir().unwrap();
        let config_path = temp.path().join("rclone.conf");
        std::fs::write(&config_path, b"temp config").unwrap();
        let runtime = GoogleDriveRuntimePaths {
            binary_path: fixture_binary_path(),
            config_path: config_path.clone(),
        };
        let (tx, rx) = oneshot::channel();
        tx.send(()).unwrap();
        let result = setup_google_drive_with_remote_name(
            &opener,
            GoogleDriveSetupRequest {
                label: "drive".to_owned(),
                path_prefix: "vault".to_owned(),
                is_primary: false,
                backup_mode: None,
                await_completion: true,
            },
            &runtime,
            &store,
            rx,
            "remote".to_owned(),
            &ok_save_endpoint,
        )
        .await;
        assert!(matches!(result, Ok(GoogleDriveSetupResult::Completed(_))));
        assert!(!config_path.exists());
    }

    #[tokio::test]
    async fn test_setup_google_drive_terminal_cancel_cleans_runtime_config() {
        let (_directory, store) = create_store().await;
        let opener = RecordingOpener::default();
        let temp = tempdir().unwrap();
        let config_path = temp.path().join("rclone.conf");
        std::fs::write(&config_path, b"temp config").unwrap();
        let runtime = GoogleDriveRuntimePaths {
            binary_path: fixture_binary_path(),
            config_path: config_path.clone(),
        };
        let (tx, rx) = oneshot::channel::<()>();
        drop(tx);
        let result = setup_google_drive_with_remote_name(
            &opener,
            GoogleDriveSetupRequest {
                label: "drive".to_owned(),
                path_prefix: "vault".to_owned(),
                is_primary: false,
                backup_mode: None,
                await_completion: true,
            },
            &runtime,
            &store,
            rx,
            "remote".to_owned(),
            &ok_save_endpoint,
        )
        .await;
        assert!(matches!(result, Err(CloudTransportError::Other(_))));
        assert!(!config_path.exists());
    }

    #[tokio::test]
    async fn test_setup_google_drive_terminal_error_cleans_runtime_config() {
        let (_directory, store) = create_store().await;
        let opener = RecordingOpener::default();
        let temp = tempdir().unwrap();
        let config_path = temp.path().join("rclone.conf");
        std::fs::write(&config_path, b"temp config").unwrap();
        let runtime = GoogleDriveRuntimePaths {
            binary_path: fixture_binary_path(),
            config_path: config_path.clone(),
        };
        let (tx, rx) = oneshot::channel();
        tx.send(()).unwrap();
        let result = setup_google_drive_with_remote_name(
            &opener,
            GoogleDriveSetupRequest {
                label: "drive".to_owned(),
                path_prefix: "vault".to_owned(),
                is_primary: false,
                backup_mode: None,
                await_completion: true,
            },
            &runtime,
            &store,
            rx,
            "remote-does-not-exist".to_owned(),
            &ok_save_endpoint,
        )
        .await;
        assert!(matches!(result, Err(CloudTransportError::Other(_))));
        assert!(!config_path.exists());
    }

    #[tokio::test]
    async fn test_setup_google_drive_rolls_back_destination_when_primary_endpoint_save_fails() {
        let (_directory, store) = create_store().await;
        let opener = RecordingOpener::default();
        let temp = tempdir().unwrap();
        let runtime = GoogleDriveRuntimePaths {
            binary_path: fixture_binary_path(),
            config_path: temp.path().join("rclone.conf"),
        };
        let (tx, rx) = oneshot::channel();
        tx.send(()).unwrap();
        let result = setup_google_drive_with_remote_name(
            &opener,
            GoogleDriveSetupRequest {
                label: "drive".to_owned(),
                path_prefix: "vault".to_owned(),
                is_primary: true,
                backup_mode: None,
                await_completion: true,
            },
            &runtime,
            &store,
            rx,
            "remote".to_owned(),
            &fail_save_endpoint,
        )
        .await;
        assert!(matches!(result, Err(CloudTransportError::Other(_))));
        let sessions = list_destination_sessions(&store).await.unwrap();
        assert!(sessions.is_empty());
    }
}
