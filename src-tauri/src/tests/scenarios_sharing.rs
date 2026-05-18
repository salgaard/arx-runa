//! Scenario tests: file sharing (Use Case 4 — Personal File Sharing).
//!
//! Tests the share package round trip: sender creates a package → recipient imports it →
//! recipient can unwrap the original file key.

use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::crypto::{
    FileId, KeyEncryptionKey, WrappedFileKey, generate_file_key, unwrap_file_key, wrap_file_key,
};
use crate::sharing::{
    Contact, ContactId, ReceivedShare, ShareRecord, SharingError, SharingStore, X25519PublicKey,
    create_share_package, import_share_package,
};
use crate::storage::mock::MockMetadataStore;
use crate::storage::{MetadataStore, Node, NodeType};

// ---------------------------------------------------------------------------
// Test doubles
// ---------------------------------------------------------------------------

/// Minimal in-memory sharing store for scenario tests.
struct FakeSharingStore {
    owner_public_key: X25519PublicKey,
    received_shares: Arc<Mutex<Vec<ReceivedShare>>>,
}

#[async_trait]
impl SharingStore for FakeSharingStore {
    async fn get_own_public_key(&self) -> Result<X25519PublicKey, SharingError> {
        Ok(self.owner_public_key)
    }

    async fn insert_contact(&self, _contact: &Contact) -> Result<(), SharingError> {
        unimplemented!()
    }

    async fn get_contact(&self, _contact_id: ContactId) -> Result<Contact, SharingError> {
        unimplemented!()
    }

    async fn list_contacts(&self) -> Result<Vec<Contact>, SharingError> {
        unimplemented!()
    }

    async fn delete_contact(&self, _contact_id: ContactId) -> Result<(), SharingError> {
        unimplemented!()
    }

    async fn insert_received_share(&self, row: &ReceivedShare) -> Result<(), SharingError> {
        self.received_shares.lock().await.push(row.clone());
        Ok(())
    }

    async fn get_received_share(&self, share_id: &str) -> Result<ReceivedShare, SharingError> {
        self.received_shares
            .lock()
            .await
            .iter()
            .find(|s| s.share_id == share_id)
            .cloned()
            .ok_or(SharingError::ReceivedShareNotFound)
    }

    async fn list_received_shares(&self) -> Result<Vec<ReceivedShare>, SharingError> {
        Ok(self.received_shares.lock().await.clone())
    }

    async fn insert_share(&self, _share: &ShareRecord) -> Result<(), SharingError> {
        unimplemented!()
    }

    async fn get_share(&self, _share_id: &str) -> Result<ShareRecord, SharingError> {
        unimplemented!()
    }

    async fn list_shares_by_file(&self, _file_id: &str) -> Result<Vec<ShareRecord>, SharingError> {
        unimplemented!()
    }

    async fn list_active_shares_by_file(
        &self,
        _file_id: &str,
    ) -> Result<Vec<ShareRecord>, SharingError> {
        unimplemented!()
    }

    async fn list_active_shares_by_file_share_id(
        &self,
        _file_share_id: &str,
    ) -> Result<Vec<ShareRecord>, SharingError> {
        unimplemented!()
    }

    async fn set_share_revoked_at(
        &self,
        _share_id: &str,
        _revoked_at: i64,
    ) -> Result<(), SharingError> {
        unimplemented!()
    }
}

/// Generates a fresh X25519 keypair and returns `(secret_bytes, public_key)`.
fn make_x25519_keypair() -> ([u8; 32], X25519PublicKey) {
    use x25519_dalek::{PublicKey, StaticSecret};
    let secret_bytes: [u8; 32] = rand::random();
    let secret = StaticSecret::from(secret_bytes);
    let public = PublicKey::from(&secret);
    (secret_bytes, X25519PublicKey::new(*public.as_bytes()))
}

// ---------------------------------------------------------------------------
// UC4: share package round trip
// ---------------------------------------------------------------------------

/// Full round trip: sender creates a share package → recipient imports it → recipient can
/// unwrap the original file key with their own KEK.
#[tokio::test(flavor = "multi_thread")]
async fn test_share_package_round_trip_recipient_decrypts_file_key() {
    // Keys
    let sender_kek = KeyEncryptionKey::from_bytes([0xAA; 32]);
    let recipient_kek = KeyEncryptionKey::from_bytes([0xBB; 32]);

    // X25519 keypairs
    let (sender_secret_bytes, sender_public_key) = make_x25519_keypair();
    let _ = sender_secret_bytes; // sender secret not needed for create
    let (recipient_secret_bytes, recipient_public_key) = make_x25519_keypair();

    // File key — original value that the recipient must recover.
    let file_key = generate_file_key();

    // Build node and insert into metadata store.
    let file_id = Uuid::new_v4();

    // Wrap file_key with sender KEK for storage in the metadata store.
    let wrapped = wrap_file_key(&file_key, &FileId::from_uuid(file_id), &sender_kek)
        .expect("wrap_file_key must succeed");
    let node = Node::new(
        file_id,
        None,
        NodeType::File,
        "shared-document.pdf".to_owned(),
        1_700_000_000,
        1_700_000_000,
        0,
        Some(*wrapped.as_bytes()),
    );
    let metadata_store = MockMetadataStore::new();
    metadata_store
        .insert_file_with_chunks(&node, &[])
        .await
        .expect("insert_file_with_chunks must succeed");

    // Sharing stores
    let sender_sharing_store = FakeSharingStore {
        owner_public_key: sender_public_key,
        received_shares: Arc::new(Mutex::new(vec![])),
    };
    let recipient_sharing_store = FakeSharingStore {
        owner_public_key: recipient_public_key,
        received_shares: Arc::new(Mutex::new(vec![])),
    };

    // Sender creates the share package.
    let wire = create_share_package(
        file_id,
        &recipient_public_key,
        None,
        serde_json::Value::Null,
        &metadata_store,
        &sender_sharing_store,
        &sender_kek,
    )
    .await
    .expect("create_share_package must succeed");

    // Recipient imports the package.
    let received_share = import_share_package(
        &wire,
        &recipient_secret_bytes,
        &recipient_kek,
        &recipient_sharing_store,
        1_700_000_000,
    )
    .await
    .expect("import_share_package must succeed");

    // Unwrap the file key stored in the received share using the recipient KEK.
    let share_file_id = FileId::from_uuid(
        Uuid::parse_str(&received_share.file_id).expect("share file_id must be valid uuid"),
    );
    let recovered_key = unwrap_file_key(
        &WrappedFileKey::new(received_share.file_key_wrapped),
        &share_file_id,
        &recipient_kek,
    )
    .expect("file key must unwrap with recipient KEK");

    assert_eq!(
        recovered_key.expose(),
        file_key.expose(),
        "recovered file key must match the original key the sender shared"
    );
}
