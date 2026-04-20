//! In-memory `DeviceMonitor` used by tests.

use std::pin::Pin;
use std::sync::Mutex;

use tokio::sync::mpsc;
use tokio_stream::Stream;
use tokio_stream::wrappers::ReceiverStream;

use crate::auth::device_monitor::{DeviceEvent, DeviceMonitor};

/// A `DeviceMonitor` that emits events pushed by tests.
pub struct MockDeviceMonitor {
    sender: mpsc::Sender<DeviceEvent>,
    receiver: Mutex<Option<mpsc::Receiver<DeviceEvent>>>,
}

impl MockDeviceMonitor {
    /// Creates a mock monitor with a bounded channel.
    pub fn new() -> Self {
        let (sender, receiver) = mpsc::channel(32);
        Self {
            sender,
            receiver: Mutex::new(Some(receiver)),
        }
    }

    /// Pushes a synthetic device event.
    pub fn push(&self, event: DeviceEvent) {
        self.sender
            .try_send(event)
            .expect("mock device monitor channel should accept event");
    }
}

impl Default for MockDeviceMonitor {
    fn default() -> Self {
        Self::new()
    }
}

impl DeviceMonitor for MockDeviceMonitor {
    fn watch(&self) -> Pin<Box<dyn Stream<Item = DeviceEvent> + Send>> {
        let receiver = self
            .receiver
            .lock()
            .expect("mock device monitor mutex should not be poisoned")
            .take()
            .expect("MockDeviceMonitor::watch called more than once");
        Box::pin(ReceiverStream::new(receiver))
    }
}

#[cfg(test)]
mod tests {
    use tokio_stream::StreamExt;

    use super::MockDeviceMonitor;
    use crate::auth::device_monitor::{DeviceEvent, DeviceMonitor};

    #[tokio::test]
    async fn test_mock_device_monitor_emits_mounted_then_unmounted_in_order() {
        let monitor = MockDeviceMonitor::new();
        let mount_path = std::path::PathBuf::from("C:\\test-mount");
        let mut stream = monitor.watch();

        monitor.push(DeviceEvent::Mounted {
            mount_path: mount_path.clone(),
        });
        monitor.push(DeviceEvent::Unmounted {
            mount_path: mount_path.clone(),
        });

        let first = stream.next().await.expect("first event should exist");
        let second = stream.next().await.expect("second event should exist");

        assert_eq!(
            first,
            DeviceEvent::Mounted {
                mount_path: mount_path.clone()
            }
        );
        assert_eq!(second, DeviceEvent::Unmounted { mount_path });
    }

    #[test]
    #[should_panic(expected = "MockDeviceMonitor::watch called more than once")]
    fn test_mock_device_monitor_second_watch_panics() {
        let monitor = MockDeviceMonitor::new();
        let _first = monitor.watch();
        let _second = monitor.watch();
    }
}
