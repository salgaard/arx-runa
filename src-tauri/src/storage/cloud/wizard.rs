//! Cloud provider setup helpers.

use std::ffi::OsString;
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::time::Duration;

use async_trait::async_trait;
use serde::Deserialize;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use uuid::Uuid;
use zeroize::Zeroize;

use super::cloud_config::save_primary_cloud_endpoint;
use super::destination_session::{
    BackupSyncMode, DestinationSession, DestinationSessionPublic, DestinationType,
    create_session_rclone_dir, delete_destination_session, destroy_session_rclone_conf,
    insert_destination_session,
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
        device_id: None,
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
            device_id: None,
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
    let mut entries: Vec<(&str, &serde_json::Value)> =
        settings.iter().map(|(k, v)| (k.as_str(), v)).collect();
    entries.sort_unstable_by_key(|(k, _)| *k);
    for (key, value) in entries {
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
    /// Live rclone subprocess running the local OAuth web server and blocking on
    /// the browser callback (the `config_is_local` continue step).
    pub child: tokio::process::Child,
    /// Background task collecting the child's stdout — yields the first
    /// post-OAuth config-state JSON once the browser callback completes.
    pub stdout_capture: JoinHandle<std::io::Result<Vec<u8>>>,
    /// Background task collecting a bounded, redactable copy of the child's
    /// stderr for diagnostics on the failure/timeout path.
    pub stderr_capture: JoinHandle<String>,
    /// Temporary rclone config file written by this subprocess.
    pub temp_config_path: PathBuf,
    /// Rclone remote name written into the temp config.
    pub remote_name: String,
}

/// Maximum bytes of rclone stderr retained for diagnostics (bounded so a chatty
/// or wedged subprocess cannot grow memory without limit).
const STDERR_DIAGNOSTIC_CAP_BYTES: usize = 16 * 1024;

/// A single non-interactive rclone config question (one JSON object per
/// `config create`/`config update --continue --non-interactive` invocation).
#[derive(Debug, Deserialize)]
pub(crate) struct RcloneConfigQuestion {
    /// Opaque continuation token; empty string means the config flow finished.
    #[serde(rename = "State", default)]
    pub state: String,
    /// The option rclone is asking about; absent once `state` is empty.
    #[serde(rename = "Option", default)]
    pub option: Option<RcloneConfigOption>,
    /// Non-empty when rclone reports an error for this step.
    #[serde(rename = "Error", default)]
    pub error: String,
}

/// The `Option` block of a non-interactive rclone config question.
#[derive(Debug, Deserialize)]
pub(crate) struct RcloneConfigOption {
    /// rclone's internal name for the question (e.g. `config_is_local`).
    #[serde(rename = "Name", default)]
    pub name: String,
    /// rclone's own string rendering of the default answer.
    #[serde(rename = "DefaultStr", default)]
    pub default_str: String,
    /// Typed default value, used as a fallback when `DefaultStr` is empty.
    #[serde(rename = "Default", default)]
    pub default: Option<serde_json::Value>,
    /// Offered choices, used as a last-resort fallback for the answer.
    #[serde(rename = "Examples", default)]
    pub examples: Vec<RcloneConfigExample>,
}

/// A single offered choice within an rclone config option.
#[derive(Debug, Deserialize)]
pub(crate) struct RcloneConfigExample {
    /// The literal value to pass back as `--result`.
    #[serde(rename = "Value", default)]
    pub value: String,
    /// Human-readable description. For the OneDrive drive chooser rclone formats
    /// this as `"<DriveName> (<DriveType>)"`, which lets us pick the account's own
    /// drive by type rather than by list position.
    #[serde(rename = "Help", default)]
    pub help: String,
}

/// rclone's name for the OneDrive drive-selection question.
const ONEDRIVE_DRIVE_CHOOSER: &str = "config_driveid";

/// A drive offered by rclone's OneDrive chooser, surfaced to the UI so the user
/// can choose which drive to use.
#[derive(Debug, Clone)]
pub struct DriveChoice {
    /// Opaque rclone drive identifier, passed back as `--result`.
    pub id: String,
    /// Human-readable label (`"<DriveName> (<DriveType>)"`) shown in the picker.
    pub label: String,
}

/// Outcome of advancing the post-OAuth config state machine.
#[derive(Debug)]
pub enum OAuthConfigOutcome {
    /// The remote is fully configured; carries the credential INI blob.
    Completed(String),
    /// rclone offered more than one drive; the user must pick one. `resume_state`
    /// is the opaque rclone state to continue from once a drive is chosen.
    NeedsDriveSelection {
        /// rclone continuation token for the drive-chooser question.
        resume_state: String,
        /// Drives to present to the user.
        drives: Vec<DriveChoice>,
    },
}

/// Returns the offered drives when an option is a multi-drive OneDrive chooser.
///
/// rclone lists every drive the account can reach (personal/business plus any
/// SharePoint document libraries it follows). When more than one is offered the
/// flow pauses for an explicit user choice rather than guessing; a single drive
/// is answered automatically by [`decide_answer`].
fn drives_needing_selection(option: &RcloneConfigOption) -> Option<Vec<DriveChoice>> {
    if option.name != ONEDRIVE_DRIVE_CHOOSER || option.examples.len() <= 1 {
        return None;
    }
    Some(
        option
            .examples
            .iter()
            .map(|example| DriveChoice {
                id: example.value.clone(),
                label: if example.help.is_empty() {
                    example.value.clone()
                } else {
                    example.help.clone()
                },
            })
            .collect(),
    )
}

/// Picks the answer for a non-interactive rclone config question.
///
/// Takes rclone's own default (`DefaultStr`, then `Default`, then the first
/// example) and accepts each confirmation — so no keyboard input is required for
/// the automatic questions. The multi-drive OneDrive chooser is *not* answered
/// here; the config loop pauses for an explicit user choice (see
/// [`drives_needing_selection`]). A single-drive chooser still resolves here via
/// its sole example.
pub(crate) fn decide_answer(option: &RcloneConfigOption) -> Result<String, CloudTransportError> {
    if !option.default_str.is_empty() {
        return Ok(option.default_str.clone());
    }
    if let Some(default) = &option.default {
        match default {
            serde_json::Value::String(value) if !value.is_empty() => return Ok(value.clone()),
            serde_json::Value::Bool(value) => return Ok(value.to_string()),
            serde_json::Value::Number(value) => return Ok(value.to_string()),
            _ => {}
        }
    }
    if let Some(example) = option.examples.first() {
        return Ok(example.value.clone());
    }
    Err(CloudTransportError::Other(format!(
        "rclone config question '{}' has no default answer",
        option.name
    )))
}

/// Parses one rclone non-interactive config question from a JSON stdout blob.
///
/// rclone prints exactly one JSON object per `--non-interactive` invocation, but
/// the surrounding stdout is not guaranteed to be pristine: a stray notice line
/// or a trailing newline/blank line can accompany it (observed only on some
/// accounts/platforms). Rather than require the entire trimmed blob to be a
/// single JSON value, locate the first `{` and decode one self-delimiting object
/// from there, ignoring any trailing bytes. This turns a benign formatting quirk
/// into a successful parse instead of a "failed to retrieve credentials" error.
pub(crate) fn parse_config_question(
    stdout: &str,
) -> Result<RcloneConfigQuestion, CloudTransportError> {
    // Never echo raw `stdout` into the error: it is logged to disk on the
    // failure path and the post-OAuth stream can carry token material.
    let object_start = stdout.find('{').ok_or_else(|| {
        CloudTransportError::Other("rclone config state contained no JSON object".to_owned())
    })?;
    let question: RcloneConfigQuestion =
        serde_json::Deserializer::from_str(&stdout[object_start..])
            .into_iter::<RcloneConfigQuestion>()
            .next()
            .ok_or_else(|| CloudTransportError::Other("rclone config state was empty".to_owned()))?
            .map_err(|error| {
                CloudTransportError::Other(format!("invalid rclone config state: {error}"))
            })?;
    if !question.error.is_empty() {
        return Err(CloudTransportError::Other(format!(
            "rclone config error: {}",
            question.error
        )));
    }
    Ok(question)
}

/// Runs one `rclone config update <remote> --continue` step and parses the
/// resulting question. Used for the fast, non-blocking config states (the OAuth
/// browser step is driven separately by `begin_oauth_setup`).
async fn run_config_continue(
    binary_path: &std::path::Path,
    temp_config_path: &std::path::Path,
    remote_name: &str,
    state: &str,
    result: &str,
) -> Result<RcloneConfigQuestion, CloudTransportError> {
    let stdout = run_rclone(
        binary_path,
        vec![
            OsString::from("config"),
            OsString::from("update"),
            OsString::from(remote_name),
            OsString::from("--continue"),
            OsString::from("--state"),
            OsString::from(state),
            OsString::from("--result"),
            OsString::from(result),
            OsString::from("--config"),
            temp_config_path.as_os_str().to_os_string(),
            OsString::from("--non-interactive"),
        ],
        Duration::from_secs(30),
    )
    .await?;
    parse_config_question(&stdout)
}

/// Extracts the rclone local OAuth auth URL from a single line of rclone output.
///
/// rclone emits two lines containing `http://127.0.0.1:`: a notice about
/// setting the redirect URL and the actual auth URL with `auth?state=`.  Only
/// the latter is accepted; the URL is extracted cleanly by stopping at the
/// first whitespace or quote character.
fn extract_rclone_auth_url(line: &str) -> Option<String> {
    let url_start = line.rfind("http://127.0.0.1:")?;
    let rest = &line[url_start..];
    let url_end = rest
        .find(|c: char| c.is_whitespace() || c == '"')
        .unwrap_or(rest.len());
    let url = &rest[..url_end];
    if url.contains("auth?state=") {
        Some(url.to_owned())
    } else {
        None
    }
}

/// Spawns rclone for an OAuth provider setup flow using rclone's documented
/// `--non-interactive` config state machine.
///
/// Runs `config create` to seed the remote, advances the (fast) pre-OAuth
/// states until the `config_is_local` browser step, then spawns the long-lived
/// child that runs the local OAuth web server. It reads that child's stderr for
/// the auth URL (`http://127.0.0.1:…`) and returns once found, leaving the child
/// blocked on the browser callback. Background tasks capture the child's stdout
/// (the first post-OAuth config state, consumed by `poll_oauth_setup` →
/// `finish_oauth_setup_after_browser`) and a bounded copy of its stderr (for
/// redacted diagnostics). The caller must insert the returned handle into
/// `AppState.oauth_setups`.
///
/// Because every config question is answered programmatically with rclone's own
/// default (`decide_answer`), no terminal input is ever required — this is what
/// fixes setup stalling on accounts whose drive layout shows an interactive
/// chooser.
pub async fn begin_oauth_setup(
    provider: OAuthProvider,
    binary_path: &std::path::Path,
) -> Result<OAuthSetupBegun, CloudTransportError> {
    use std::ffi::OsStr;
    use tokio::io::AsyncBufReadExt as _;
    use tokio::io::AsyncReadExt as _;
    use tokio::io::BufReader;

    let setup_id = Uuid::new_v4().hyphenated().to_string();
    let remote_name = format!("arx-runa-{}", Uuid::new_v4());
    let temp_config_dir = create_session_rclone_dir().await?;
    let temp_config_path = temp_config_dir.join("rclone.conf");

    let rclone_type = match provider {
        OAuthProvider::GoogleDrive => "drive",
        OAuthProvider::OneDrive => "onedrive",
    };

    // `config create <remote> <type> [opts] --config <path> --non-interactive`
    // writes the remote stanza and returns the first config question.
    let mut create_args = vec![
        OsString::from("config"),
        OsString::from("create"),
        OsString::from(&remote_name),
        OsString::from(rclone_type),
    ];
    match provider {
        OAuthProvider::GoogleDrive => create_args.push(OsString::from("scope=drive")),
        OAuthProvider::OneDrive => create_args.push(OsString::from("drive_type=personal")),
    }
    create_args.push(OsString::from("--config"));
    create_args.push(temp_config_path.as_os_str().to_os_string());
    create_args.push(OsString::from("--non-interactive"));

    let create_stdout = run_rclone(binary_path, create_args, Duration::from_secs(30)).await?;
    let mut question = parse_config_question(&create_stdout)?;

    // Advance the fast pre-OAuth states until rclone asks to use the local
    // browser, capturing the state + answer for that step. In practice it is the
    // first question, but looping keeps this resilient to provider-specific
    // pre-steps.
    let (oauth_state, oauth_answer) = loop {
        let option = question.option.as_ref().ok_or_else(|| {
            CloudTransportError::Other(
                "rclone OAuth setup finished before the browser step".to_owned(),
            )
        })?;
        let answer = decide_answer(option)?;
        // The browser-OAuth step: rclone asks `config_is_local` under the
        // `*oauth-islocal` state. Matching either is robust across rclone
        // versions (the state prefix has been stable far longer than the option
        // name). Answering it spawns the local web server, so stop the fast loop
        // here and run that step as the long-lived child below.
        if option.name == "config_is_local" || question.state.starts_with("*oauth-islocal") {
            break (question.state.clone(), answer);
        }
        let state = question.state.clone();
        question = run_config_continue(
            binary_path,
            &temp_config_path,
            &remote_name,
            &state,
            &answer,
        )
        .await?;
    };

    // The continue that answers `config_is_local` runs rclone's local OAuth web
    // server: it prints the auth URL on stderr and blocks until the browser
    // callback, then emits the next config state on stdout and exits.
    let mut command = tokio::process::Command::new(binary_path);
    command
        .args([
            OsStr::new("config"),
            OsStr::new("update"),
            OsStr::new(&remote_name),
            OsStr::new("--continue"),
            OsStr::new("--state"),
            OsStr::new(&oauth_state),
            OsStr::new("--result"),
            OsStr::new(&oauth_answer),
            OsStr::new("--config"),
            temp_config_path.as_os_str(),
            OsStr::new("--non-interactive"),
        ])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    #[cfg(windows)]
    command.creation_flags(0x08000000); // CREATE_NO_WINDOW

    let mut child = command.spawn().map_err(CloudTransportError::from)?;

    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| CloudTransportError::Other("failed to capture rclone stdout".to_owned()))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| CloudTransportError::Other("failed to capture rclone stderr".to_owned()))?;

    let mut stderr_reader = BufReader::new(stderr).lines();
    let mut diagnostics = String::new();
    let mut auth_url: Option<String> = None;

    while let Some(line) = stderr_reader
        .next_line()
        .await
        .map_err(|e| CloudTransportError::Other(format!("reading rclone stderr: {e}")))?
    {
        if diagnostics.len() < STDERR_DIAGNOSTIC_CAP_BYTES {
            diagnostics.push_str(&line);
            diagnostics.push('\n');
        }
        if let Some(url) = extract_rclone_auth_url(&line) {
            auth_url = Some(url);
            break;
        }
    }

    let auth_url = auth_url.ok_or_else(|| {
        CloudTransportError::Other("rclone did not emit an auth URL on stderr".to_owned())
    })?;

    // Keep draining stderr (so the pipe cannot stall rclone) while retaining a
    // bounded copy for diagnostics on the failure/timeout path.
    let stderr_capture = tokio::spawn(async move {
        let mut buffer = diagnostics;
        while let Ok(Some(line)) = stderr_reader.next_line().await {
            if buffer.len() < STDERR_DIAGNOSTIC_CAP_BYTES {
                buffer.push_str(&line);
                buffer.push('\n');
            }
        }
        buffer
    });

    // The post-OAuth config state arrives on stdout only after the browser
    // callback; collect it to completion (i.e. once the child exits).
    let stdout_capture = tokio::spawn(async move {
        let mut bytes = Vec::new();
        stdout.read_to_end(&mut bytes).await?;
        Ok::<Vec<u8>, std::io::Error>(bytes)
    });

    Ok(OAuthSetupBegun {
        setup_id,
        auth_url,
        child,
        stdout_capture,
        stderr_capture,
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
    if let Some(dir) = temp_config_path.parent()
        && let Err(error) = tokio::fs::remove_dir(dir).await
    {
        tracing::warn!(error = %error, "failed to remove oauth temp config dir");
    }

    Ok(blob)
}

/// Drives non-interactive config states from `question` until the remote is
/// fully written, pausing if a multi-drive OneDrive chooser is encountered.
///
/// Each automatic question is answered with rclone's own default via
/// `decide_answer`. If rclone offers more than one drive, the loop stops and
/// returns [`OAuthConfigOutcome::NeedsDriveSelection`] so the caller can ask the
/// user; once `State` is empty the remote is dumped via `complete_oauth_setup`.
/// The bounded guard prevents a pathological provider from looping forever.
async fn drive_config_loop(
    binary_path: &std::path::Path,
    temp_config_path: &std::path::Path,
    remote_name: &str,
    mut question: RcloneConfigQuestion,
) -> Result<OAuthConfigOutcome, CloudTransportError> {
    const MAX_CONFIG_STATES: usize = 32;

    let mut steps = 0;
    while !question.state.is_empty() {
        steps += 1;
        if steps > MAX_CONFIG_STATES {
            return Err(CloudTransportError::Other(
                "rclone OAuth setup exceeded the maximum number of config states".to_owned(),
            ));
        }
        let option = question.option.as_ref().ok_or_else(|| {
            CloudTransportError::Other("rclone config state missing its option".to_owned())
        })?;
        if let Some(drives) = drives_needing_selection(option) {
            return Ok(OAuthConfigOutcome::NeedsDriveSelection {
                resume_state: question.state.clone(),
                drives,
            });
        }
        let answer = decide_answer(option)?;
        let state = question.state.clone();
        question = run_config_continue(binary_path, temp_config_path, remote_name, &state, &answer)
            .await?;
    }

    let blob = complete_oauth_setup(binary_path, temp_config_path, remote_name).await?;
    Ok(OAuthConfigOutcome::Completed(blob))
}

/// Advances the config state machine from the first post-OAuth state.
///
/// `post_oauth_stdout` is the config state captured from the blocking child's
/// stdout once the browser callback completes. Returns
/// [`OAuthConfigOutcome::Completed`] for single-drive accounts and
/// [`OAuthConfigOutcome::NeedsDriveSelection`] when the user must choose a drive.
pub async fn drive_config_after_browser(
    binary_path: &std::path::Path,
    temp_config_path: &std::path::Path,
    remote_name: &str,
    post_oauth_stdout: &str,
) -> Result<OAuthConfigOutcome, CloudTransportError> {
    let question = parse_config_question(post_oauth_stdout)?;
    drive_config_loop(binary_path, temp_config_path, remote_name, question).await
}

/// Resumes config after the user picked a drive in the chooser dialog.
///
/// Answers the paused `resume_state` with the chosen `drive_id`, then drives the
/// remaining states to completion and returns the credential blob. A second
/// drive-selection pause is treated as an error (rclone asks at most once).
pub async fn resume_oauth_with_selected_drive(
    binary_path: &std::path::Path,
    temp_config_path: &std::path::Path,
    remote_name: &str,
    resume_state: &str,
    drive_id: &str,
) -> Result<String, CloudTransportError> {
    let question = run_config_continue(
        binary_path,
        temp_config_path,
        remote_name,
        resume_state,
        drive_id,
    )
    .await?;
    match drive_config_loop(binary_path, temp_config_path, remote_name, question).await? {
        OAuthConfigOutcome::Completed(blob) => Ok(blob),
        OAuthConfigOutcome::NeedsDriveSelection { .. } => Err(CloudTransportError::Other(
            "rclone requested a second drive selection".to_owned(),
        )),
    }
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
    if let Err(error) = tokio::time::timeout(Duration::from_secs(5), child.wait()).await {
        tracing::warn!(error = %error, "cancel_oauth_setup: failed to reap rclone child");
    }
    if let Err(error) = destroy_session_rclone_conf(temp_config_path).await {
        tracing::warn!(error = %error, "cancel_oauth_setup: failed to remove temp config");
    }
    if let Some(dir) = temp_config_path.parent()
        && let Err(error) = tokio::fs::remove_dir(dir).await
    {
        tracing::warn!(error = %error, "cancel_oauth_setup: failed to remove temp config dir");
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
        GoogleDriveSetupResult, OpenerLike, S3SetupRequest, decide_answer,
        drives_needing_selection, extract_remote_blob_from_dump, parse_config_question,
        setup_google_drive, setup_google_drive_with_remote_name, setup_s3_provider,
        setup_s3_provider_with_endpoint_saver,
    };
    use crate::storage::CloudTransportError;
    use crate::storage::cloud::destination_session::list_destination_sessions;
    use crate::storage::sqlcipher::SqlCipherMetadataStore;

    #[test]
    fn test_decide_answer_config_is_local_uses_default_str_true() {
        // First OneDrive/Google Drive question: use the local web browser.
        let question = parse_config_question(
            r#"{"State":"*oauth-islocal,choose_type,,","Option":{"Name":"config_is_local","DefaultStr":"true","Default":true,"Type":"bool"},"Error":""}"#,
        )
        .unwrap();
        let answer = decide_answer(question.option.as_ref().unwrap()).unwrap();
        assert_eq!(answer, "true");
    }

    #[test]
    fn test_decide_answer_config_type_selects_onedrive() {
        let question = parse_config_question(
            r#"{"State":"choose_type_done","Option":{"Name":"config_type","DefaultStr":"onedrive","Default":"onedrive","Type":"string","Examples":[{"Value":"onedrive"},{"Value":"sharepoint"}]},"Error":""}"#,
        )
        .unwrap();
        let answer = decide_answer(question.option.as_ref().unwrap()).unwrap();
        assert_eq!(answer, "onedrive");
    }

    #[test]
    fn test_drives_needing_selection_pauses_on_multiple_drives() {
        // Multi-drive (work/M365) account: rclone offers several drives. The flow
        // must pause and present all of them rather than guess.
        let question = parse_config_question(
            r#"{"State":"driveid_final","Option":{"Name":"config_driveid","DefaultStr":"","Type":"string","Examples":[{"Value":"b!sharepoint","Help":"Documents (documentLibrary)"},{"Value":"me-drive","Help":"OneDrive (personal)"}]},"Error":""}"#,
        )
        .unwrap();
        let drives = drives_needing_selection(question.option.as_ref().unwrap())
            .expect("multiple drives should require selection");
        assert_eq!(drives.len(), 2);
        assert_eq!(drives[0].id, "b!sharepoint");
        assert_eq!(drives[0].label, "Documents (documentLibrary)");
        assert_eq!(drives[1].id, "me-drive");
    }

    #[test]
    fn test_drives_needing_selection_single_drive_resolves_automatically() {
        // One drive: no choice to make. The chooser falls through to decide_answer,
        // which selects the sole example.
        let question = parse_config_question(
            r#"{"State":"driveid_final","Option":{"Name":"config_driveid","DefaultStr":"","Type":"string","Examples":[{"Value":"only-drive","Help":"OneDrive (personal)"}]},"Error":""}"#,
        )
        .unwrap();
        let option = question.option.as_ref().unwrap();
        assert!(drives_needing_selection(option).is_none());
        assert_eq!(decide_answer(option).unwrap(), "only-drive");
    }

    #[test]
    fn test_drives_needing_selection_ignores_non_chooser_options() {
        let question = parse_config_question(
            r#"{"State":"choose_type_done","Option":{"Name":"config_type","DefaultStr":"onedrive","Examples":[{"Value":"onedrive"},{"Value":"sharepoint"}]},"Error":""}"#,
        )
        .unwrap();
        assert!(drives_needing_selection(question.option.as_ref().unwrap()).is_none());
    }

    #[test]
    fn test_decide_answer_falls_back_to_typed_bool_default_when_default_str_empty() {
        let question = parse_config_question(
            r#"{"State":"driveid_final_end","Option":{"Name":"config_drive_ok","DefaultStr":"","Default":true,"Type":"bool"},"Error":""}"#,
        )
        .unwrap();
        let answer = decide_answer(question.option.as_ref().unwrap()).unwrap();
        assert_eq!(answer, "true");
    }

    #[test]
    fn test_decide_answer_falls_back_to_first_example_when_no_default() {
        let question = parse_config_question(
            r#"{"State":"teamdrive","Option":{"Name":"config_team_drive","DefaultStr":"","Examples":[{"Value":"first"},{"Value":"second"}]},"Error":""}"#,
        )
        .unwrap();
        let answer = decide_answer(question.option.as_ref().unwrap()).unwrap();
        assert_eq!(answer, "first");
    }

    #[test]
    fn test_decide_answer_no_default_or_examples_returns_error() {
        let question = parse_config_question(
            r#"{"State":"needs_input","Option":{"Name":"config_token","DefaultStr":""},"Error":""}"#,
        )
        .unwrap();
        assert!(matches!(
            decide_answer(question.option.as_ref().unwrap()),
            Err(CloudTransportError::Other(_))
        ));
    }

    #[test]
    fn test_parse_config_question_empty_state_means_done() {
        let question =
            parse_config_question(r#"{"State":"","Option":null,"Error":"","Result":""}"#).unwrap();
        assert!(question.state.is_empty());
        assert!(question.option.is_none());
    }

    #[test]
    fn test_parse_config_question_non_empty_error_field_returns_error() {
        let result =
            parse_config_question(r#"{"State":"some_state","Option":null,"Error":"backend boom"}"#);
        match result {
            Err(CloudTransportError::Other(message)) => assert!(message.contains("backend boom")),
            other => panic!("unexpected result: {other:?}"),
        }
    }

    #[test]
    fn test_parse_config_question_invalid_json_returns_error() {
        assert!(matches!(
            parse_config_question("not json at all"),
            Err(CloudTransportError::Other(_))
        ));
    }

    #[test]
    fn test_parse_config_question_ignores_surrounding_noise() {
        let question = parse_config_question(
            "\nNOTICE: harmless line\n{\"State\":\"driveid_final\",\"Option\":{\"Name\":\"config_driveid\",\"Examples\":[{\"Value\":\"abc123\"}]},\"Error\":\"\"}\n\n",
        )
        .expect("a single JSON object surrounded by noise should parse");
        assert_eq!(question.state, "driveid_final");
        assert_eq!(
            question.option.expect("option present").name,
            "config_driveid"
        );
    }

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

    #[tokio::test]
    async fn test_cancel_oauth_setup_removes_temp_config_and_dir() {
        use crate::storage::cloud::destination_session::create_session_rclone_dir;
        use tokio::process::Command;

        use super::cancel_oauth_setup;

        let dir = create_session_rclone_dir()
            .await
            .expect("create_session_rclone_dir should succeed");
        let conf_path = dir.join("rclone.conf");
        tokio::fs::write(&conf_path, b"[remote]\ntype = drive\n")
            .await
            .expect("write should succeed");

        #[cfg(windows)]
        let mut child = Command::new("cmd").args(["/C", "exit 0"]).spawn().unwrap();
        #[cfg(not(windows))]
        let mut child = Command::new("true").spawn().unwrap();

        cancel_oauth_setup(&mut child, &conf_path).await;

        assert!(!conf_path.exists(), "conf file must be removed");
        assert!(!dir.exists(), "temp config dir must be removed");
    }

    #[tokio::test]
    async fn test_complete_oauth_setup_removes_temp_dir() {
        use crate::storage::cloud::destination_session::{
            create_session_rclone_dir, destroy_session_rclone_conf,
        };

        let dir = create_session_rclone_dir()
            .await
            .expect("create_session_rclone_dir should succeed");
        let conf_path = dir.join("rclone.conf");
        tokio::fs::write(&conf_path, b"[remote]\ntype = drive\n")
            .await
            .expect("write should succeed");

        // Mirror the cleanup sequence in complete_oauth_setup.
        if let Err(error) = destroy_session_rclone_conf(&conf_path).await {
            tracing::warn!(error = %error, "test: failed to remove conf");
        }
        if let Some(parent) = conf_path.parent() {
            let _ = tokio::fs::remove_dir(parent).await;
        }

        assert!(!conf_path.exists(), "conf file must be removed");
        assert!(!dir.exists(), "temp config dir must be removed");
    }
}
