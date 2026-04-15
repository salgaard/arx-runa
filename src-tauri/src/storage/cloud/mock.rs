//! In-memory [`CloudTransport`] mock for Phase 2.4 ceremony tests.
//!
//! Forward declaration for Phase 4.1 / Phase 4.3. Phase 2.4 ceremonies use
//! this mock to exercise upload / download round trips without a real
//! rclone backend. Phase 4 will replace it with `RcloneTransport`.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::Mutex;

use super::{CloudTransport, CloudTransportError};

/// In-memory cloud transport backed by a `HashMap<String, Vec<u8>>` under
/// a `tokio::sync::Mutex`.
///
/// Cloneable via `Arc` so multiple ceremony actors can share the same
/// simulated backend.
#[derive(Debug, Default, Clone)]
pub struct MockCloudTransport {
    store: Arc<Mutex<HashMap<String, Vec<u8>>>>,
}

impl MockCloudTransport {
    /// Creates an empty in-memory transport.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the number of stored blobs.
    pub async fn len(&self) -> usize {
        self.store.lock().await.len()
    }

    /// Returns `true` if the store has no blobs.
    pub async fn is_empty(&self) -> bool {
        self.store.lock().await.is_empty()
    }
}

#[async_trait]
impl CloudTransport for MockCloudTransport {
    async fn upload_blob(
        &self,
        name: &str,
        bytes: &[u8],
    ) -> Result<(), CloudTransportError> {
        self.store
            .lock()
            .await
            .insert(name.to_string(), bytes.to_vec());
        Ok(())
    }

    async fn download_blob(&self, name: &str) -> Result<Vec<u8>, CloudTransportError> {
        self.store
            .lock()
            .await
            .get(name)
            .cloned()
            .ok_or(CloudTransportError::NotFound)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_cloud_transport_upload_then_download_returns_bytes() {
        let transport = MockCloudTransport::new();
        let payload = b"vault header bytes".to_vec();

        transport
            .upload_blob("vault.json", &payload)
            .await
            .expect("upload must succeed");
        let recovered = transport
            .download_blob("vault.json")
            .await
            .expect("download must succeed");

        assert_eq!(recovered, payload);
    }

    #[tokio::test]
    async fn test_mock_cloud_transport_download_missing_returns_not_found() {
        let transport = MockCloudTransport::new();

        let result = transport.download_blob("missing.json").await;

        assert!(matches!(result, Err(CloudTransportError::NotFound)));
    }

    #[tokio::test]
    async fn test_mock_cloud_transport_upload_overwrites_existing_blob() {
        let transport = MockCloudTransport::new();

        transport
            .upload_blob("vault.json", b"first")
            .await
            .expect("upload must succeed");
        transport
            .upload_blob("vault.json", b"second")
            .await
            .expect("upload must succeed");

        let recovered = transport
            .download_blob("vault.json")
            .await
            .expect("download must succeed");
        assert_eq!(recovered, b"second");
    }

    #[tokio::test]
    async fn test_mock_cloud_transport_len_tracks_distinct_blob_names() {
        let transport = MockCloudTransport::new();

        assert!(transport.is_empty().await);
        transport.upload_blob("a", b"1").await.unwrap();
        transport.upload_blob("b", b"2").await.unwrap();
        transport.upload_blob("a", b"3").await.unwrap();

        assert_eq!(transport.len().await, 2);
    }
}
