//! `DeviceMonitor` trait, `DeviceEvent` enum, and platform implementations.

use std::path::PathBuf;
use std::pin::Pin;

use tokio_stream::Stream;

/// A removable-storage mount or unmount event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeviceEvent {
    /// A removable device was mounted at `mount_path`.
    Mounted { mount_path: PathBuf },
    /// A removable device was unmounted from `mount_path`.
    Unmounted { mount_path: PathBuf },
}

/// Monitors for removable storage mount and unmount events.
pub trait DeviceMonitor: Send + Sync {
    /// Returns a stream of mount and unmount events.
    fn watch(&self) -> Pin<Box<dyn Stream<Item = DeviceEvent> + Send>>;
}

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::LinuxDeviceMonitor;

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
pub use windows::WindowsDeviceMonitor;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::MacOsDeviceMonitor;

#[cfg(any(test, feature = "test-utils"))]
mod mock;
#[cfg(any(test, feature = "test-utils"))]
pub use mock::MockDeviceMonitor;
