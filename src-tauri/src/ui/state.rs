//! Shared application state for Tauri IPC commands.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::OnceLock;
use std::time::Instant;

#[cfg(not(any(test, feature = "test-utils")))]
use async_trait::async_trait;
use tokio::sync::RwLock;

use crate::auth::{DeviceMonitor, SessionManager};
#[cfg(not(any(test, feature = "test-utils")))]
use crate::storage::CloudTransportError;
use crate::storage::cloud::CloudTransport;
use crate::ui::types::SyncStatus;

/// In-flight rclone OAuth subprocess handle.
///
/// Created by `begin_google_drive_setup` / `begin_onedrive_setup`, consumed by
/// `poll_oauth_setup` or `cancel_oauth_setup`.  The `child` field has its stderr
/// already taken; a drain task is spawned at begin time.
pub struct OAuthSetupHandle {
    /// Live rclone subprocess waiting for the OAuth browser callback.
    pub child: tokio::process::Child,
    /// Temporary rclone config file written by this subprocess.
    pub temp_config_path: PathBuf,
    /// Rclone remote name used for this setup (e.g. `arx-runa-<uuid>`).
    pub remote_name: String,
    /// When this handle was created; used to enforce a setup timeout.
    pub started_at: Instant,
}

/// Shared application state injected into every Tauri command via `tauri::State`.
///
/// Contains only IPC and runtime orchestration handles — no key material,
/// no passwords, and no file-content buffers.
///
/// # Lock acquisition order
///
/// When a handler must hold more than one lock simultaneously, always acquire
/// them in this order to prevent deadlock:
///
/// 1. `sync_mutex`
/// 2. `flush_mutex`
/// 3. `oauth_setups`
/// 4. `allowed_reveal_paths`
/// 5. `cloud_transport` (RwLock)
/// 6. `sync_status` (RwLock)
/// 7. `active_vault_id` (RwLock)
///
/// In practice `flush_mutex` is always released before `cloud_transport` is
/// acquired (e.g. `sync_backup`), so concurrent holding of multiple locks is
/// rare. Respect this order in any new multi-lock code path.
pub struct AppState {
    /// Cloud transport implementation; swappable post-authenticate.
    ///
    /// Default is `NoOpCloudTransport`. After authenticate/create_vault,
    /// the inner `Arc<dyn CloudTransport>` is replaced with `RcloneTransport`.
    /// On lock/delete it is reset to `NoOpCloudTransport`.
    pub(crate) cloud_transport: Arc<RwLock<Arc<dyn CloudTransport>>>,
    /// Platform device monitor for USB key-file autodetection.
    pub(crate) device_monitor: Arc<dyn DeviceMonitor>,
    /// Session lifecycle manager (timeout, state machine, key zeroization).
    pub(crate) session_manager: Arc<SessionManager>,
    /// Cached sync status updated by sync commands.
    pub(crate) sync_status: Arc<RwLock<SyncStatus>>,
    /// Tauri app handle for emitting window events (populated in setup hook).
    pub(crate) app_handle: OnceLock<tauri::AppHandle>,
    /// Active vault identifier, mirrors SessionManager for quick IPC reads.
    pub(crate) active_vault_id: Arc<RwLock<Option<String>>>,
    /// Mutex ensuring at most one epoch-buffer flush runs at a time.
    pub(crate) flush_mutex: Arc<tokio::sync::Mutex<()>>,
    /// Mutex preventing concurrent `sync_to_cloud` / `sync_backup` invocations.
    ///
    /// Guards the shared rclone config files and mirror-temp directory from
    /// races when the UI issues a second sync before the first completes.
    pub(crate) sync_mutex: Arc<tokio::sync::Mutex<()>>,
    /// In-flight OAuth setup subprocesses keyed by opaque setup ID.
    ///
    /// Uses `tokio::sync::Mutex` so it can be held across `.await` points
    /// inside `poll_oauth_setup`.
    pub(crate) oauth_setups: Arc<tokio::sync::Mutex<HashMap<String, OAuthSetupHandle>>>,
    /// Canonical paths that `reveal_in_explorer` is permitted to open this session.
    ///
    /// Populated by `download_received_share` on successful decryption; cleared on
    /// vault lock. Also allows any path under `app_data_dir`.
    pub(crate) allowed_reveal_paths: Arc<tokio::sync::Mutex<HashSet<PathBuf>>>,
    /// Vault header fields that are immutable for the duration of a session.
    ///
    /// Set by `authenticate`/`create_vault`; cleared on lock. Avoids re-reading
    /// `vault-header.json` on every `get_session_status` poll (every 5 s).
    pub(crate) session_vault_info: Arc<RwLock<Option<SessionVaultInfo>>>,
}

/// Immutable vault metadata cached at session-open time.
#[derive(Debug, Clone)]
pub(crate) struct SessionVaultInfo {
    /// Authentication tier (1 = password only, 2 = password + USB key file).
    pub vault_tier: u8,
    /// Whether a BIP-39 recovery slot is configured for this vault.
    pub has_recovery_slot: bool,
}

/// No-op cloud transport used until Phase 6.5 wires a real `RcloneTransport`.
///
/// Phase 6.1 command scaffolds return `InternalError("command not yet wired")` before
/// reaching the transport, so these methods are unreachable in practice.
#[cfg(not(any(test, feature = "test-utils")))]
struct NoOpCloudTransport;

#[cfg(not(any(test, feature = "test-utils")))]
#[async_trait]
impl CloudTransport for NoOpCloudTransport {
    /// Not wired in Phase 6.1.
    async fn upload_blob(
        &self,
        _local_path: &std::path::Path,
        _remote_path: &str,
    ) -> Result<(), CloudTransportError> {
        Err(CloudTransportError::Other(
            "cloud transport not configured".into(),
        ))
    }

    /// Not wired in Phase 6.1.
    async fn download_blob(
        &self,
        _remote_path: &str,
        _local_path: &std::path::Path,
    ) -> Result<(), CloudTransportError> {
        Err(CloudTransportError::Other(
            "cloud transport not configured".into(),
        ))
    }

    /// Not wired in Phase 6.1.
    async fn delete_blob(&self, _remote_path: &str) -> Result<(), CloudTransportError> {
        Err(CloudTransportError::Other(
            "cloud transport not configured".into(),
        ))
    }

    /// Not wired in Phase 6.1.
    async fn list_blobs(&self, _remote_prefix: &str) -> Result<Vec<String>, CloudTransportError> {
        Err(CloudTransportError::Other(
            "cloud transport not configured".into(),
        ))
    }

    fn is_configured(&self) -> bool {
        false
    }
}

impl AppState {
    /// Constructs application state with platform-appropriate component selection.
    ///
    /// Uses the platform `DeviceMonitor` implementation selected at compile time.
    /// Under `test` or `test-utils` builds, uses `MockCloudTransport` instead of
    /// `NoOpCloudTransport`.
    pub fn construct_default() -> Self {
        #[cfg(any(test, feature = "test-utils"))]
        let cloud_transport: Arc<RwLock<Arc<dyn CloudTransport>>> = Arc::new(RwLock::new(
            Arc::new(crate::storage::cloud::mock::MockCloudTransport::default()),
        ));
        #[cfg(not(any(test, feature = "test-utils")))]
        let cloud_transport: Arc<RwLock<Arc<dyn CloudTransport>>> =
            Arc::new(RwLock::new(Arc::new(NoOpCloudTransport)));

        #[cfg(target_os = "windows")]
        let device_monitor: Arc<dyn DeviceMonitor> =
            Arc::new(crate::auth::device_monitor::WindowsDeviceMonitor::new());
        #[cfg(target_os = "linux")]
        let device_monitor: Arc<dyn DeviceMonitor> =
            Arc::new(crate::auth::device_monitor::LinuxDeviceMonitor::new());
        #[cfg(target_os = "macos")]
        let device_monitor: Arc<dyn DeviceMonitor> =
            Arc::new(crate::auth::device_monitor::MacOsDeviceMonitor::new());

        Self {
            cloud_transport,
            device_monitor,
            session_manager: Arc::new(SessionManager::from_config()),
            sync_status: Arc::new(RwLock::new(SyncStatus::default())),
            app_handle: OnceLock::new(),
            active_vault_id: Arc::new(RwLock::new(None)),
            flush_mutex: Arc::new(tokio::sync::Mutex::new(())),
            sync_mutex: Arc::new(tokio::sync::Mutex::new(())),
            oauth_setups: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            allowed_reveal_paths: Arc::new(tokio::sync::Mutex::new(HashSet::new())),
            session_vault_info: Arc::new(RwLock::new(None)),
        }
    }

    /// Replaces the active cloud transport with a new implementation.
    ///
    /// Called after authenticate/create_vault to install an `RcloneTransport`.
    pub(crate) async fn swap_cloud_transport(&self, transport: Arc<dyn CloudTransport>) {
        *self.cloud_transport.write().await = transport;
    }

    /// Resets the cloud transport back to the no-op default.
    ///
    /// Called on lock or vault deletion.
    pub(crate) async fn reset_cloud_transport(&self) {
        #[cfg(any(test, feature = "test-utils"))]
        let noop: Arc<dyn CloudTransport> =
            Arc::new(crate::storage::cloud::mock::MockCloudTransport::default());
        #[cfg(not(any(test, feature = "test-utils")))]
        let noop: Arc<dyn CloudTransport> = Arc::new(NoOpCloudTransport);
        *self.cloud_transport.write().await = noop;
    }
}
