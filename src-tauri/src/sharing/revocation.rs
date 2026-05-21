//! Cooperative revocation of outgoing file shares.

use crate::sharing::error::SharingError;
use crate::sharing::store::SharingStore;
use crate::storage::CloudTransport;
use crate::storage::cloud::CloudTransportError;

/// Cooperatively revokes a share by deleting the per-recipient cloud blobs
/// and marking the share row as revoked.
///
/// Revocation is retryable on partial failure: `SharingError::RevocationPartial`
/// is returned if a blob deletion fails, with the index of the first failed
/// deletion. The caller may retry by invoking `revoke_share` again.
/// `revoked_at` is only written after ALL blob deletions succeed.
///
/// If no other active shares reference the same `file_share_id`, the shared
/// cloud blobs are also deleted.  Otherwise the blobs remain live for the
/// remaining recipients.
pub(crate) async fn revoke_share(
    share_id: &str,
    now_unix_seconds: i64,
    sharing_store: &dyn SharingStore,
    cloud: &dyn CloudTransport,
) -> Result<(), SharingError> {
    let share = sharing_store.get_share(share_id).await?;

    if share.revoked_at.is_some() {
        return Err(SharingError::ShareAlreadyRevoked);
    }

    let remaining = sharing_store
        .list_active_shares_by_file_share_id(&share.file_share_id)
        .await?;
    let is_last = remaining.len() == 1 && remaining[0].share_id == share_id;

    if is_last {
        let blob_paths = cloud
            .list_blobs(&format!("shared/{}/", share.file_share_id))
            .await
            .map_err(|error| SharingError::CloudOperation(error.to_string()))?;

        for (index, blob_path) in blob_paths.iter().enumerate() {
            match cloud.delete_blob(blob_path).await {
                Ok(()) | Err(CloudTransportError::NotFound) => {}
                Err(_) => {
                    return Err(SharingError::RevocationPartial {
                        failed_index: index,
                    });
                }
            }
        }
    }

    sharing_store
        .set_share_revoked_at(share_id, now_unix_seconds)
        .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;
    use uuid::Uuid;

    use super::revoke_share;
    use crate::sharing::error::SharingError;
    use crate::sharing::store::{Contact, ReceivedShare, ShareRecord, SharingStore};
    use crate::sharing::types::{ContactId, X25519PublicKey};
    use crate::storage::CloudTransport;
    use crate::storage::cloud::mock::{CloudTransportErrorKind, MockCloudTransport};

    /// Minimal in-memory state backing `MockSharingStore`.
    #[derive(Debug, Default)]
    struct MockSharingState {
        shares: HashMap<String, ShareRecord>,
        contacts: HashMap<ContactId, Contact>,
    }

    /// In-memory `SharingStore` for revocation unit tests.
    #[derive(Debug, Default, Clone)]
    struct MockSharingStore {
        state: Arc<Mutex<MockSharingState>>,
    }

    impl MockSharingStore {
        /// Creates a new empty store.
        fn new() -> Self {
            Self::default()
        }

        /// Seeds a share row directly into the store.
        fn seed_share(&self, share: ShareRecord) {
            self.state
                .lock()
                .unwrap()
                .shares
                .insert(share.share_id.clone(), share);
        }

        /// Seeds a contact row directly into the store.
        #[allow(dead_code)]
        fn seed_contact(&self, contact: Contact) {
            self.state
                .lock()
                .unwrap()
                .contacts
                .insert(contact.contact_id, contact);
        }

        /// Returns all share rows as a snapshot.
        fn snapshot_shares(&self) -> Vec<ShareRecord> {
            self.state
                .lock()
                .unwrap()
                .shares
                .values()
                .cloned()
                .collect()
        }
    }

    #[async_trait]
    impl SharingStore for MockSharingStore {
        async fn get_own_public_key(&self) -> Result<X25519PublicKey, SharingError> {
            Ok(X25519PublicKey::new([0u8; 32]))
        }

        async fn insert_contact(&self, contact: &Contact) -> Result<(), SharingError> {
            self.state
                .lock()
                .unwrap()
                .contacts
                .insert(contact.contact_id, contact.clone());
            Ok(())
        }

        async fn get_contact(&self, contact_id: ContactId) -> Result<Contact, SharingError> {
            self.state
                .lock()
                .unwrap()
                .contacts
                .get(&contact_id)
                .cloned()
                .ok_or(SharingError::ContactNotFound)
        }

        async fn list_contacts(&self) -> Result<Vec<Contact>, SharingError> {
            Ok(self
                .state
                .lock()
                .unwrap()
                .contacts
                .values()
                .cloned()
                .collect())
        }

        async fn delete_contact(&self, contact_id: ContactId) -> Result<(), SharingError> {
            self.state.lock().unwrap().contacts.remove(&contact_id);
            Ok(())
        }

        async fn insert_received_share(&self, _row: &ReceivedShare) -> Result<(), SharingError> {
            Ok(())
        }

        async fn get_received_share(&self, _share_id: &str) -> Result<ReceivedShare, SharingError> {
            Err(SharingError::ReceivedShareNotFound)
        }

        async fn list_received_shares(&self) -> Result<Vec<ReceivedShare>, SharingError> {
            Ok(vec![])
        }

        async fn insert_share(&self, share: &ShareRecord) -> Result<(), SharingError> {
            self.state
                .lock()
                .unwrap()
                .shares
                .insert(share.share_id.clone(), share.clone());
            Ok(())
        }

        async fn get_share(&self, share_id: &str) -> Result<ShareRecord, SharingError> {
            self.state
                .lock()
                .unwrap()
                .shares
                .get(share_id)
                .cloned()
                .ok_or(SharingError::ShareNotFound)
        }

        async fn list_shares_by_file(
            &self,
            file_id: &str,
        ) -> Result<Vec<ShareRecord>, SharingError> {
            Ok(self
                .state
                .lock()
                .unwrap()
                .shares
                .values()
                .filter(|s| s.file_id == file_id)
                .cloned()
                .collect())
        }

        async fn list_active_shares_by_file(
            &self,
            file_id: &str,
        ) -> Result<Vec<ShareRecord>, SharingError> {
            Ok(self
                .state
                .lock()
                .unwrap()
                .shares
                .values()
                .filter(|s| s.file_id == file_id && s.revoked_at.is_none())
                .cloned()
                .collect())
        }

        async fn list_active_shares_by_file_share_id(
            &self,
            file_share_id: &str,
        ) -> Result<Vec<ShareRecord>, SharingError> {
            Ok(self
                .state
                .lock()
                .unwrap()
                .shares
                .values()
                .filter(|s| s.file_share_id == file_share_id && s.revoked_at.is_none())
                .cloned()
                .collect())
        }

        async fn set_share_revoked_at(
            &self,
            share_id: &str,
            revoked_at: i64,
        ) -> Result<(), SharingError> {
            let mut state = self.state.lock().unwrap();
            let share = state
                .shares
                .get_mut(share_id)
                .ok_or(SharingError::ShareNotFound)?;
            if share.revoked_at.is_some() {
                return Err(SharingError::ShareAlreadyRevoked);
            }
            share.revoked_at = Some(revoked_at);
            Ok(())
        }
    }

    /// Builds a minimal `ShareRecord` for test seeding.
    fn make_share(share_id: &str, file_share_id: &str, file_id: &str) -> ShareRecord {
        ShareRecord {
            share_id: share_id.to_owned(),
            file_id: file_id.to_owned(),
            contact_id: ContactId::from_uuid(Uuid::new_v4()),
            file_share_id: file_share_id.to_owned(),
            cloud_path: format!("shared/{}/", file_share_id),
            created_at: 1_000_000,
            expires_at: None,
            revoked_at: None,
            download_key_id: None,
            download_folder_id: None,
            receipt_requested: false,
            receipt_received_at: None,
        }
    }

    /// Verifies that `revoke_share` returns `ShareNotFound` when the share does not exist.
    #[tokio::test]
    async fn test_revoke_share_on_missing_share_returns_share_not_found() {
        let store = MockSharingStore::new();
        let cloud = MockCloudTransport::new();

        let result = revoke_share("nonexistent-id", 9_999, &store, &cloud).await;

        assert!(matches!(result, Err(SharingError::ShareNotFound)));
    }

    /// Verifies that `revoke_share` returns `ShareAlreadyRevoked` when the share is revoked.
    #[tokio::test]
    async fn test_revoke_share_on_already_revoked_share_returns_share_already_revoked() {
        let store = MockSharingStore::new();
        let cloud = MockCloudTransport::new();
        let file_share_id = Uuid::new_v4().hyphenated().to_string();
        let file_id = Uuid::new_v4().hyphenated().to_string();
        let mut share = make_share("share-1", &file_share_id, &file_id);
        share.revoked_at = Some(500);
        store.seed_share(share);

        let result = revoke_share("share-1", 9_999, &store, &cloud).await;

        assert!(matches!(result, Err(SharingError::ShareAlreadyRevoked)));
    }

    /// Verifies that `revoke_share` deletes the shared blobs when this is the last active share
    /// and marks the share revoked only after all deletions succeed.
    #[tokio::test]
    async fn test_revoke_share_last_active_deletes_blobs_then_marks_revoked() {
        let temp = tempfile::tempdir().expect("tempdir should be created");
        let store = MockSharingStore::new();
        let cloud = MockCloudTransport::new();
        let file_share_id = Uuid::new_v4().hyphenated().to_string();
        let file_id = Uuid::new_v4().hyphenated().to_string();
        store.seed_share(make_share("share-1", &file_share_id, &file_id));

        let local_blob = temp.path().join("chunk.blob");
        tokio::fs::write(&local_blob, b"data")
            .await
            .expect("seed blob write should succeed");
        let blob_key = format!("shared/{}/chunk.blob", file_share_id);
        cloud
            .upload_blob(&local_blob, &blob_key)
            .await
            .expect("seed upload should succeed");

        let result = revoke_share("share-1", 9_999, &store, &cloud).await;

        assert!(result.is_ok());

        let listed = cloud
            .list_blobs(&format!("shared/{}/", file_share_id))
            .await
            .expect("list_blobs should succeed");
        assert!(listed.is_empty(), "shared blobs should have been deleted");

        let revoked = store
            .snapshot_shares()
            .into_iter()
            .find(|s| s.share_id == "share-1")
            .expect("share row should exist");
        assert_eq!(revoked.revoked_at, Some(9_999));
    }

    /// Verifies that `revoke_share` does NOT delete shared blobs when another active share
    /// still references the same `file_share_id`, and still marks the target share revoked.
    #[tokio::test]
    async fn test_revoke_share_not_last_active_preserves_blobs_and_marks_revoked() {
        let temp = tempfile::tempdir().expect("tempdir should be created");
        let store = MockSharingStore::new();
        let cloud = MockCloudTransport::new();
        let file_share_id = Uuid::new_v4().hyphenated().to_string();
        let file_id = Uuid::new_v4().hyphenated().to_string();
        store.seed_share(make_share("share-1", &file_share_id, &file_id));
        store.seed_share(make_share("share-2", &file_share_id, &file_id));

        let local_blob = temp.path().join("chunk.blob");
        tokio::fs::write(&local_blob, b"data")
            .await
            .expect("seed blob write should succeed");
        let blob_key = format!("shared/{}/chunk.blob", file_share_id);
        cloud
            .upload_blob(&local_blob, &blob_key)
            .await
            .expect("seed upload should succeed");

        let result = revoke_share("share-1", 7_777, &store, &cloud).await;

        assert!(result.is_ok());

        let listed = cloud
            .list_blobs(&format!("shared/{}/", file_share_id))
            .await
            .expect("list_blobs should succeed");
        assert!(
            !listed.is_empty(),
            "shared blobs must not be deleted while other shares are active"
        );

        let revoked = store
            .snapshot_shares()
            .into_iter()
            .find(|s| s.share_id == "share-1")
            .expect("share row should exist");
        assert_eq!(revoked.revoked_at, Some(7_777));
    }

    /// Verifies that `revoke_share` returns `RevocationPartial` when a blob deletion fails,
    /// and does NOT write `revoked_at` before all deletions succeed.
    #[tokio::test]
    async fn test_revoke_share_partial_blob_deletion_failure_returns_revocation_partial_without_marking_revoked()
     {
        let temp = tempfile::tempdir().expect("tempdir should be created");
        let store = MockSharingStore::new();
        let cloud = MockCloudTransport::new();
        let file_share_id = Uuid::new_v4().hyphenated().to_string();
        let file_id = Uuid::new_v4().hyphenated().to_string();
        store.seed_share(make_share("share-1", &file_share_id, &file_id));

        let local_blob = temp.path().join("chunk.blob");
        tokio::fs::write(&local_blob, b"data")
            .await
            .expect("seed blob write should succeed");
        let blob_key = format!("shared/{}/chunk.blob", file_share_id);
        cloud
            .upload_blob(&local_blob, &blob_key)
            .await
            .expect("seed upload should succeed");

        cloud
            .inject_failure(&blob_key, CloudTransportErrorKind::Timeout)
            .await;

        let result = revoke_share("share-1", 9_999, &store, &cloud).await;

        assert!(
            matches!(
                result,
                Err(SharingError::RevocationPartial { failed_index: 0 })
            ),
            "expected RevocationPartial at index 0"
        );

        let share = store
            .snapshot_shares()
            .into_iter()
            .find(|s| s.share_id == "share-1")
            .expect("share row should exist");
        assert!(
            share.revoked_at.is_none(),
            "revoked_at must not be set when blob deletion fails"
        );
    }
}
