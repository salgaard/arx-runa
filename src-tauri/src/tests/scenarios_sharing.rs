//! Scenario tests: file sharing (Use Case 4 — Personal File Sharing).
//!
//! Tests the share package round trip: sender creates a package → recipient imports it →
//! recipient can unwrap the original file key.

use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::crypto::{FileId, KeyEncryptionKey, WrappedFileKey, generate_file_key, unwrap_file_key};
use crate::sharing::{
    Contact, ContactId, FileShareSnapshot, ReceivedShare, ShareRecord, SharingError, SharingStore,
    X25519PublicKey, create_share_package, import_share_package,
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
    file_shares: Arc<Mutex<Vec<FileShareSnapshot>>>,
    contacts: Arc<Mutex<Vec<Contact>>>,
    shares: Arc<Mutex<Vec<ShareRecord>>>,
}

impl FakeSharingStore {
    /// Builds an empty store owned by `owner_public_key`.
    fn empty(owner_public_key: X25519PublicKey) -> Self {
        Self {
            owner_public_key,
            received_shares: Arc::new(Mutex::new(vec![])),
            file_shares: Arc::new(Mutex::new(vec![])),
            contacts: Arc::new(Mutex::new(vec![])),
            shares: Arc::new(Mutex::new(vec![])),
        }
    }
}

#[async_trait]
impl SharingStore for FakeSharingStore {
    async fn get_own_public_key(&self) -> Result<X25519PublicKey, SharingError> {
        Ok(self.owner_public_key)
    }

    async fn insert_contact(&self, contact: &Contact) -> Result<(), SharingError> {
        self.contacts.lock().await.push(contact.clone());
        Ok(())
    }

    async fn get_contact(&self, contact_id: ContactId) -> Result<Contact, SharingError> {
        self.contacts
            .lock()
            .await
            .iter()
            .find(|c| c.contact_id == contact_id)
            .cloned()
            .ok_or(SharingError::ContactNotFound)
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

    async fn insert_share(&self, share: &ShareRecord) -> Result<(), SharingError> {
        self.shares.lock().await.push(share.clone());
        Ok(())
    }

    async fn get_share(&self, share_id: &str) -> Result<ShareRecord, SharingError> {
        self.shares
            .lock()
            .await
            .iter()
            .find(|s| s.share_id == share_id)
            .cloned()
            .ok_or(SharingError::ShareNotFound)
    }

    async fn list_shares_by_file(&self, file_id: &str) -> Result<Vec<ShareRecord>, SharingError> {
        Ok(self
            .shares
            .lock()
            .await
            .iter()
            .filter(|s| s.file_id == file_id)
            .cloned()
            .collect())
    }

    async fn list_active_shares_by_file(
        &self,
        file_id: &str,
    ) -> Result<Vec<ShareRecord>, SharingError> {
        Ok(self
            .shares
            .lock()
            .await
            .iter()
            .filter(|s| s.file_id == file_id && s.revoked_at.is_none())
            .cloned()
            .collect())
    }

    async fn list_active_shares_by_file_share_id(
        &self,
        file_share_id: &str,
    ) -> Result<Vec<ShareRecord>, SharingError> {
        Ok(self
            .shares
            .lock()
            .await
            .iter()
            .filter(|s| s.file_share_id == file_share_id && s.revoked_at.is_none())
            .cloned()
            .collect())
    }

    async fn set_share_revoked_at(
        &self,
        _share_id: &str,
        _revoked_at: i64,
    ) -> Result<(), SharingError> {
        unimplemented!()
    }

    async fn insert_file_share(&self, snapshot: &FileShareSnapshot) -> Result<(), SharingError> {
        self.file_shares.lock().await.push(snapshot.clone());
        Ok(())
    }

    async fn get_file_share(
        &self,
        file_share_id: &str,
    ) -> Result<Option<FileShareSnapshot>, SharingError> {
        Ok(self
            .file_shares
            .lock()
            .await
            .iter()
            .find(|s| s.file_share_id == file_share_id)
            .cloned())
    }

    async fn delete_file_share(&self, file_share_id: &str) -> Result<(), SharingError> {
        self.file_shares
            .lock()
            .await
            .retain(|s| s.file_share_id != file_share_id);
        Ok(())
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

/// Full round trip: sender seals a share package from a re-encryption snapshot → recipient
/// imports it → recipient can unwrap the fresh share key with their own KEK.
#[tokio::test(flavor = "multi_thread")]
async fn test_share_package_round_trip_recipient_decrypts_file_key() {
    // Keys
    let recipient_kek = KeyEncryptionKey::from_bytes([0xBB; 32]);

    // X25519 keypairs
    let (sender_secret_bytes, sender_public_key) = make_x25519_keypair();
    let _ = sender_secret_bytes; // sender secret not needed for create
    let (recipient_secret_bytes, recipient_public_key) = make_x25519_keypair();

    // Fresh share key — re-encryption uses a key independent of any vault file key.
    let share_file_key = generate_file_key();
    let share_file_uuid = Uuid::new_v4();

    // The re-encryption snapshot the sender would have produced for this file.
    let snapshot = FileShareSnapshot {
        file_share_id: Uuid::new_v4().hyphenated().to_string(),
        share_file_id: share_file_uuid.hyphenated().to_string(),
        share_file_key_wrapped: [0u8; 72],
        chunk_uuids: vec![Uuid::new_v4().hyphenated().to_string()],
        chunk_size: 4_194_304,
        file_size: 1024,
        file_name: "shared-document.pdf".to_owned(),
        created_at: 0,
    };

    // Sharing stores
    let sender_sharing_store = FakeSharingStore::empty(sender_public_key);
    let recipient_sharing_store = FakeSharingStore::empty(recipient_public_key);

    // Sender seals the package from the snapshot + fresh share key.
    let wire = create_share_package(
        &recipient_public_key,
        None,
        serde_json::Value::Null,
        &snapshot,
        &share_file_key,
        &sender_sharing_store,
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

    // The recipient's stored wrapped key must unwrap (under their own KEK) to the share key.
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
        share_file_key.expose(),
        "recovered key must match the fresh share key the sender re-encrypted under"
    );
    assert_eq!(received_share.file_id, snapshot.share_file_id);
    assert_eq!(received_share.file_name, "shared-document.pdf");
}

/// Regression (UC4): sharing a file packed into a *cross-file* epoch blob.
///
/// Two files are packed into a single epoch blob via the production flush path. Sharing one of
/// them must re-encrypt only that file's bytes under a fresh share key, so the recipient recovers
/// exactly the shared file's plaintext — never the co-packed file, never the vault's epoch key.
/// This reproduces the original "decryption failed" bug, where the recipient decrypted the epoch
/// blob with the file's own id/key instead of the epoch's, and would have leaked the co-packed
/// file had decryption ever succeeded.
#[tokio::test(flavor = "multi_thread")]
async fn test_share_epoch_packed_file_recipient_recovers_only_shared_plaintext() {
    use crate::crypto::{Blake3Hash, ChunkIndex, decrypt_chunk, verify_checksum};
    use crate::sharing::DisplayName;
    use crate::sharing::cloud::{CreateShareRequest, create_share};
    use crate::storage::CloudTransport;
    use crate::storage::cloud::mock::MockCloudTransport;
    use crate::storage::vault_ops::flush_epoch_buffer;
    use zeroize::Zeroizing;

    let now = 1_700_000_000_i64;
    let chunk_size: u64 = 4_194_304; // MockMetadataStore default chunk_size_bytes

    let sender_kek = KeyEncryptionKey::from_bytes([0xAA; 32]);
    let recipient_kek = KeyEncryptionKey::from_bytes([0xBB; 32]);
    let (recipient_secret, recipient_public) = make_x25519_keypair();
    let (_sender_secret, sender_public) = make_x25519_keypair();

    let staging = tempfile::tempdir().expect("staging temp dir");

    // Two distinct files that will be packed into a single epoch blob.
    let other_plaintext =
        b"co-packed unrelated file that must never leak to the recipient".to_vec();
    let shared_plaintext =
        b"the secret shared file payload that the recipient should recover".to_vec();

    let other_node_id = Uuid::new_v4();
    let shared_node_id = Uuid::new_v4();
    let other_node = Node::new(
        other_node_id,
        None,
        NodeType::File,
        "other.bin".to_owned(),
        now,
        now,
        other_plaintext.len() as u64,
        Some([0x11; 72]),
    );
    let shared_node = Node::new(
        shared_node_id,
        None,
        NodeType::File,
        "shared.bin".to_owned(),
        now,
        now,
        shared_plaintext.len() as u64,
        Some([0x22; 72]),
    );

    let metadata_store = MockMetadataStore::new();
    metadata_store
        .insert_file_node_and_stage_epoch_entry(
            &other_node,
            Zeroizing::new(other_plaintext.clone()),
        )
        .await
        .expect("stage other file");
    metadata_store
        .insert_file_node_and_stage_epoch_entry(
            &shared_node,
            Zeroizing::new(shared_plaintext.clone()),
        )
        .await
        .expect("stage shared file");

    flush_epoch_buffer(
        &metadata_store,
        &sender_kek,
        staging.path(),
        chunk_size,
        None,
    )
    .await
    .expect("flush epoch buffer");

    // Confirm the fixture really packed both files into one shared epoch blob.
    let shared_chunks = metadata_store
        .get_chunks(shared_node_id)
        .await
        .expect("shared chunks");
    assert_eq!(shared_chunks.len(), 1);
    assert!(
        shared_chunks[0].epoch_blob_id.is_some(),
        "shared file must be epoch-packed"
    );
    let other_chunks = metadata_store
        .get_chunks(other_node_id)
        .await
        .expect("other chunks");
    assert_eq!(
        other_chunks[0].epoch_blob_id, shared_chunks[0].epoch_blob_id,
        "both files must share one epoch blob"
    );

    // Sender shares the epoch-packed file with a contact.
    let cloud = MockCloudTransport::new();
    let contact_id = ContactId::from_uuid(Uuid::new_v4());
    let sender_store = FakeSharingStore::empty(sender_public);
    sender_store
        .insert_contact(&Contact {
            contact_id,
            display_name: DisplayName::new("Recipient").expect("display name"),
            email: None,
            public_key: recipient_public,
            created_at: now,
        })
        .await
        .expect("seed contact");

    let output = create_share(
        CreateShareRequest {
            file_id: shared_node_id,
            contact_id,
            expires_at: None,
            now_unix_seconds: now,
        },
        &metadata_store,
        &sender_store,
        &cloud,
        &sender_kek,
        staging.path(),
    )
    .await
    .expect("create_share must succeed for an epoch-packed file");

    // Recipient imports the package.
    let recipient_store = FakeSharingStore::empty(recipient_public);
    let received = import_share_package(
        &output.wire_bytes,
        &recipient_secret,
        &recipient_kek,
        &recipient_store,
        now,
    )
    .await
    .expect("import must succeed");

    // The shared file_id must be a fresh identifier, not the vault node id.
    assert_ne!(received.file_id, shared_node_id.hyphenated().to_string());

    // Recipient reconstructs the file from the shared blobs using the package's fresh key.
    let share_file_id =
        FileId::from_uuid(Uuid::parse_str(&received.file_id).expect("share file_id uuid"));
    let share_key = unwrap_file_key(
        &WrappedFileKey::new(received.file_key_wrapped),
        &share_file_id,
        &recipient_kek,
    )
    .expect("unwrap fresh share key");

    let path_prefix = received.cloud_endpoint["path_prefix"]
        .as_str()
        .expect("path_prefix present")
        .to_owned();
    let file_size = received.cloud_endpoint["_file_size"]
        .as_u64()
        .expect("file_size present");

    let download_dir = tempfile::tempdir().expect("download dir");
    let recipient_chunk_size = received.chunk_size as usize;
    let mut recovered: Vec<u8> = Vec::new();
    for (index, blob_uuid) in received.chunk_uuids.iter().enumerate() {
        let remote = format!("{path_prefix}{blob_uuid}.blob");
        let local = download_dir.path().join(format!("{blob_uuid}.blob"));
        cloud
            .download_blob(&remote, &local)
            .await
            .expect("download shared blob");
        let blob_bytes = tokio::fs::read(&local).await.expect("read shared blob");
        let hash: [u8; 32] = blake3::hash(&blob_bytes).into();
        let verified = verify_checksum(blob_bytes, &Blake3Hash(hash)).expect("verify checksum");
        let padded = decrypt_chunk(
            verified,
            &share_key,
            &share_file_id,
            ChunkIndex::new(index as u32),
        )
        .expect("decrypt shared chunk with fresh key");
        let take = (file_size as usize)
            .saturating_sub(index * recipient_chunk_size)
            .min(padded.len());
        recovered.extend_from_slice(&padded[..take]);
    }

    assert_eq!(
        recovered, shared_plaintext,
        "recipient must recover exactly the shared file's plaintext"
    );
}
