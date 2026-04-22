//! Arx Runa auth module.
//!
//! Authentication and session management: Argon2id KDF, USB key file, session
//! lifecycle, memory locking.

pub mod autodetect;
pub mod ceremonies;
pub mod config;
pub mod device_monitor;
pub mod error;
pub mod kdf;
pub mod key_source;
pub mod path_hint;
pub mod session;
pub mod staging;
pub mod transport_provider;
pub mod types;

pub use autodetect::find_key_file;
pub use transport_provider::TransportProvider;
pub use ceremonies::{
    Argon2MigrationIntent, ChangePasswordRequest, CreateVaultRequest, RecoverVaultRequest,
    RecoverWithPhraseRequest, RotateKeyFileRequest, SetupRecoveryRequest, Tier, change_password,
    create_vault, recover_vault, recover_with_phrase, rotate_key_file, setup_recovery,
};
pub use device_monitor::{DeviceEvent, DeviceMonitor};
pub use error::{AuthenticationError, KeySourceError};
pub use kdf::Argon2Params;
pub use key_source::{FileKeySource, KeySource};
pub use path_hint::{KeyHintStore, VaultHint};
pub use session::{LifecycleState, OperationGuard, SessionEvent, SessionManager};
pub use types::{AuthUser, AuthUserStore};

#[cfg(any(test, feature = "test-utils"))]
pub use device_monitor::MockDeviceMonitor;
#[cfg(any(test, feature = "test-utils"))]
pub use key_source::MockKeySource;

#[cfg(test)]
mod integration_tests {
    use std::sync::Arc;

    use tokio_stream::StreamExt;

    use super::{DeviceEvent, DeviceMonitor, MockDeviceMonitor, find_key_file};
    use crate::crypto::Blake3Hash;

    #[tokio::test]
    async fn test_autodetect_with_mock_device_monitor_finds_planted_key_file() {
        let mount_directory = tempfile::tempdir().expect("tempdir should be created");
        let key_file_path = mount_directory.path().join("key.bin");
        let key_bytes = [0x3Au8; 32];
        std::fs::write(&key_file_path, key_bytes).expect("key file should be written");
        let reference_hash = Blake3Hash(*blake3::hash(&key_bytes).as_bytes());

        let monitor = Arc::new(MockDeviceMonitor::new());
        let mut stream = monitor.watch();
        monitor.push(DeviceEvent::Mounted {
            mount_path: mount_directory.path().to_path_buf(),
        });

        let event = stream
            .next()
            .await
            .expect("mounted event should be produced");
        let DeviceEvent::Mounted { mount_path } = event else {
            panic!("expected mounted event");
        };

        let found_path = find_key_file(&mount_path, &reference_hash)
            .await
            .expect("autodetect should succeed")
            .expect("matching key file should be found");

        assert_eq!(found_path, key_file_path);
    }
}
