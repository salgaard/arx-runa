//! Shared application state for Tauri IPC commands.

use std::sync::Arc;

#[cfg(not(any(test, feature = "test-utils")))]
use async_trait::async_trait;
use tokio::sync::RwLock;

use crate::auth::{DeviceMonitor, SessionManager};
#[cfg(not(any(test, feature = "test-utils")))]
use crate::storage::CloudTransportError;
use crate::storage::SqlCipherMetadataStore;
use crate::storage::cloud::CloudTransport;
use crate::ui::types::SyncStatus;

/// Shared application state injected into every Tauri command via `tauri::State`.
///
/// Contains only IPC and runtime orchestration handles — no key material,
/// no passwords, and no file-content buffers.
// Fields wired in Phase 6.5; present here as scaffolding.
#[allow(dead_code)]
pub struct AppState {
    /// Encrypted manifest database. `None` until a vault is opened.
    pub(crate) database: Arc<RwLock<Option<SqlCipherMetadataStore>>>,
    /// Cloud transport implementation (rclone-backed in production).
    pub(crate) cloud_transport: Arc<dyn CloudTransport>,
    /// Platform device monitor for USB key-file autodetection.
    pub(crate) device_monitor: Arc<dyn DeviceMonitor>,
    /// Session lifecycle manager (timeout, state machine, key zeroization).
    pub(crate) session_manager: Arc<SessionManager>,
    /// Cached sync status updated by sync commands.
    pub(crate) sync_status: Arc<RwLock<SyncStatus>>,
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
}

impl AppState {
    /// Constructs application state with platform-appropriate component selection.
    ///
    /// Uses the platform `DeviceMonitor` implementation selected at compile time.
    /// Under `test` or `test-utils` builds, uses `MockCloudTransport` instead of
    /// `NoOpCloudTransport`.
    pub fn construct_default() -> Self {
        #[cfg(any(test, feature = "test-utils"))]
        let cloud_transport: Arc<dyn CloudTransport> =
            Arc::new(crate::storage::cloud::mock::MockCloudTransport::default());
        #[cfg(not(any(test, feature = "test-utils")))]
        let cloud_transport: Arc<dyn CloudTransport> = Arc::new(NoOpCloudTransport);

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
            database: Arc::new(RwLock::new(None)),
            cloud_transport,
            device_monitor,
            session_manager: Arc::new(SessionManager::from_config()),
            sync_status: Arc::new(RwLock::new(SyncStatus::default())),
        }
    }
}
