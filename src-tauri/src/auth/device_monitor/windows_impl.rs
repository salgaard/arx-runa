//! Windows `DeviceMonitor` implementation using WMI volume-change events.

use std::path::PathBuf;
use std::pin::Pin;

use serde::Deserialize;
use tokio::sync::mpsc;
use tokio_stream::Stream;
use tokio_stream::wrappers::ReceiverStream;
use wmi::{COMLibrary, WMIConnection};

use crate::auth::device_monitor::{DeviceEvent, DeviceMonitor};

/// Monitors removable Windows volume events.
pub struct WindowsDeviceMonitor;

impl WindowsDeviceMonitor {
    /// Creates a Windows device monitor.
    pub fn new() -> Self {
        Self
    }
}

impl Default for WindowsDeviceMonitor {
    fn default() -> Self {
        Self::new()
    }
}

/// WMI projection of `Win32_VolumeChangeEvent`.
#[derive(Debug, Deserialize)]
#[serde(rename = "Win32_VolumeChangeEvent")]
#[serde(rename_all = "PascalCase")]
struct VolumeChangeEvent {
    event_type: u16,
    drive_name: Option<String>,
}

impl DeviceMonitor for WindowsDeviceMonitor {
    fn watch(&self) -> Pin<Box<dyn Stream<Item = DeviceEvent> + Send>> {
        let (sender, receiver) = mpsc::channel(32);
        tokio::task::spawn_blocking(move || {
            if let Err(error) = run_wmi_loop(sender) {
                tracing::warn!(%error, "windows device monitor loop exited");
            }
        });
        Box::pin(ReceiverStream::new(receiver))
    }
}

/// Runs the blocking WMI notification loop.
fn run_wmi_loop(sender: mpsc::Sender<DeviceEvent>) -> wmi::WMIResult<()> {
    let com_library = COMLibrary::new()?;
    let wmi_connection = WMIConnection::new(com_library)?;
    let iterator = wmi_connection.notification::<VolumeChangeEvent>()?;

    for event in iterator {
        let event = match event {
            Ok(event) => event,
            Err(_) => continue,
        };
        let Some(drive_name) = event.drive_name else {
            continue;
        };
        if !is_removable_drive(&drive_name) {
            continue;
        }

        let mount_path = PathBuf::from(normalize_drive_root(&drive_name));
        let device_event = match event.event_type {
            2 => DeviceEvent::Mounted { mount_path },
            3 => DeviceEvent::Unmounted { mount_path },
            _ => continue,
        };

        if sender.blocking_send(device_event).is_err() {
            break;
        }
    }

    Ok(())
}

/// Normalizes a drive string into a root path like `E:\`.
fn normalize_drive_root(drive_name: &str) -> String {
    if drive_name.ends_with('\\') {
        drive_name.to_owned()
    } else {
        format!("{drive_name}\\")
    }
}

/// Returns whether a Windows drive root is removable.
fn is_removable_drive(drive_root: &str) -> bool {
    use windows::Win32::Storage::FileSystem::GetDriveTypeW;
    use windows::core::PCWSTR;

    let normalized = normalize_drive_root(drive_root);
    let mut wide_drive_root: Vec<u16> = normalized.encode_utf16().collect();
    wide_drive_root.push(0);

    // SAFETY: `wide_drive_root` is a valid, null-terminated UTF-16 buffer
    // that remains alive for the duration of the `GetDriveTypeW` call.
    let drive_type = unsafe { GetDriveTypeW(PCWSTR(wide_drive_root.as_ptr())) };
    drive_type == 2
}
