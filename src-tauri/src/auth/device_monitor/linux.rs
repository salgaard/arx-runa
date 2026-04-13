//! Linux `DeviceMonitor` implementation using `udev`.

use std::path::{Path, PathBuf};
use std::pin::Pin;

use tokio::sync::mpsc;
use tokio_stream::Stream;
use tokio_stream::wrappers::ReceiverStream;

use crate::auth::device_monitor::{DeviceEvent, DeviceMonitor};

/// Monitors removable Linux block-device mount events.
pub struct LinuxDeviceMonitor;

impl LinuxDeviceMonitor {
    /// Creates a Linux device monitor.
    pub fn new() -> Self {
        Self
    }
}

impl Default for LinuxDeviceMonitor {
    fn default() -> Self {
        Self::new()
    }
}

impl DeviceMonitor for LinuxDeviceMonitor {
    fn watch(&self) -> Pin<Box<dyn Stream<Item = DeviceEvent> + Send>> {
        let (sender, receiver) = mpsc::channel(32);
        tokio::task::spawn_blocking(move || {
            if let Err(error) = run_udev_loop(sender) {
                tracing::warn!(%error, "linux device monitor loop exited");
            }
        });
        Box::pin(ReceiverStream::new(receiver))
    }
}

/// Runs the blocking udev monitor loop.
fn run_udev_loop(sender: mpsc::Sender<DeviceEvent>) -> std::io::Result<()> {
    let socket = udev::MonitorBuilder::new()?
        .match_subsystem_devtype("block", "partition")?
        .listen()?;

    for event in socket.iter() {
        if !is_usb_partition(&event) {
            continue;
        }

        let event_type = event.event_type();
        let Some(device_node) = event.devnode() else {
            continue;
        };
        let Some(mount_path) = resolve_mount_path(device_node) else {
            continue;
        };

        let device_event = match event_type {
            udev::EventType::Add => DeviceEvent::Mounted { mount_path },
            udev::EventType::Remove => DeviceEvent::Unmounted { mount_path },
            _ => continue,
        };

        if sender.blocking_send(device_event).is_err() {
            break;
        }
    }

    Ok(())
}

/// Returns whether a udev event represents a removable USB partition.
fn is_usb_partition(event: &udev::Event) -> bool {
    let by_bus = event
        .property_value("ID_BUS")
        .and_then(|value| value.to_str())
        .map(|value| value == "usb")
        .unwrap_or(false);
    let by_thumb_drive_flag = event
        .property_value("ID_DRIVE_THUMB")
        .and_then(|value| value.to_str())
        .map(|value| value == "1")
        .unwrap_or(false);

    by_bus || by_thumb_drive_flag
}

/// Resolves a Linux device node to a mount path via `/proc/self/mountinfo`.
fn resolve_mount_path(device_node: &Path) -> Option<PathBuf> {
    let mountinfo = std::fs::read_to_string("/proc/self/mountinfo").ok()?;
    resolve_mount_path_from_mountinfo(&mountinfo, device_node)
}

/// Resolves a Linux device node from mountinfo content.
fn resolve_mount_path_from_mountinfo(mountinfo: &str, device_node: &Path) -> Option<PathBuf> {
    for line in mountinfo.lines() {
        let Some((source, mount_point)) = parse_mountinfo_line(line) else {
            continue;
        };
        if source == device_node {
            return Some(mount_point);
        }
    }
    None
}

/// Parses one mountinfo line and returns `(source, mount_point)`.
fn parse_mountinfo_line(line: &str) -> Option<(PathBuf, PathBuf)> {
    let (left, right) = line.split_once(" - ")?;

    let mut left_fields = left.split_whitespace();
    let _mount_id = left_fields.next()?;
    let _parent_id = left_fields.next()?;
    let _major_minor = left_fields.next()?;
    let _root = left_fields.next()?;
    let mount_point = left_fields.next()?;

    let mut right_fields = right.split_whitespace();
    let _filesystem_type = right_fields.next()?;
    let source = right_fields.next()?;

    Some((PathBuf::from(source), PathBuf::from(mount_point)))
}

#[cfg(test)]
mod tests {
    use super::resolve_mount_path_from_mountinfo;

    #[test]
    fn test_resolve_mount_path_parses_sample_mountinfo() {
        let mountinfo = "\
30 23 8:1 / / rw,relatime - ext4 /dev/sda1 rw\n\
31 23 8:17 / /media/usb rw,nosuid,nodev - vfat /dev/sdb1 rw";

        let mount_path =
            resolve_mount_path_from_mountinfo(mountinfo, std::path::Path::new("/dev/sdb1"));

        assert_eq!(mount_path, Some(std::path::PathBuf::from("/media/usb")));
    }
}
