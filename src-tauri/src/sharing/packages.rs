//! Share-package creation and import.
//!
//! A share package is a `.vgshare` binary blob sealed with HPKE for a specific
//! X25519 recipient.  The inner JSON payload (`SharePackagePayload`) carries
//! the file key, chunk metadata, and cloud endpoint required for a recipient
//! to reconstruct access to a shared file.

use base64::Engine;
use secrecy::SecretBox;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zeroize::{Zeroize, Zeroizing};

use crate::crypto::types::{FileId, FileKey, KeyEncryptionKey, WrappedFileKey};
use crate::crypto::{unwrap_file_key, wrap_file_key};
use crate::sharing::error::SharingError;
use crate::sharing::hpke;
use crate::sharing::store::{ReceivedShare, SharingStore};
use crate::sharing::types::X25519PublicKey;
use crate::storage::metadata_store::MetadataStore;
use crate::storage::types::NodeType;
/// JSON payload sealed inside the HPKE envelope of a share package.
#[derive(Serialize, Deserialize)]
pub(crate) struct SharePackagePayload {
    /// Unique share identifier (UUID v4 hyphenated).
    pub share_id: String,
    /// File node identifier (UUID v4 hyphenated).
    pub file_id: String,
    /// Original file name.
    pub file_name: String,
    /// Number of chunks in the shared file.
    pub chunk_count: u32,
    /// Chunk size in bytes.
    pub chunk_size: u32,
    /// Ordered blob-name UUIDs for each chunk (UUID v4 hyphenated).
    pub chunk_uuids: Vec<String>,
    /// Base64-encoded 32-byte file key.
    pub file_key: String,
    /// Base64-encoded 32-byte X25519 sender public key.
    pub sender_public_key: String,
    /// Cloud endpoint metadata for locating the shared blobs.
    pub cloud_endpoint: serde_json::Value,
    /// Total file size in bytes (used by recipient to truncate last-chunk padding on decrypt).
    #[serde(default)]
    pub file_size: u64,
    /// Optional Unix timestamp when the share expires.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<i64>,
}

/// Creates a share package sealed for the given recipient.
///
/// Reads the file node and chunk metadata from the manifest, unwraps the file
/// key, builds the JSON payload, and seals it with HPKE for the recipient.
/// Returns the binary `.vgshare` wire bytes.
pub(crate) async fn create_share_package(
    file_id: Uuid,
    recipient_public_key: &X25519PublicKey,
    expires_at: Option<i64>,
    cloud_endpoint: serde_json::Value,
    metadata_store: &dyn MetadataStore,
    sharing_store: &dyn SharingStore,
    key_encryption_key: &KeyEncryptionKey,
) -> Result<Vec<u8>, SharingError> {
    let node = metadata_store
        .get_node(file_id)
        .await
        .map_err(|_| SharingError::Backend("file not found".to_owned()))?;

    if node.node_type != NodeType::File {
        return Err(SharingError::Backend("node is not a file".to_owned()));
    }

    let wrapped_bytes = node
        .file_key_wrapped
        .ok_or_else(|| SharingError::Backend("file node has no wrapped key".to_owned()))?;
    let file_key = unwrap_file_key(
        &WrappedFileKey::new(wrapped_bytes),
        &FileId::from_uuid(file_id),
        key_encryption_key,
    )
    .map_err(|_| SharingError::Backend("file key unwrap failed".to_owned()))?;

    let chunks = metadata_store
        .get_chunks(file_id)
        .await
        .map_err(|_| SharingError::Backend("chunk lookup failed".to_owned()))?;

    let chunk_uuids: Vec<String> = chunks.iter().map(|chunk| chunk.blob_name.clone()).collect();
    let chunk_count = chunks.len() as u32;

    let chunk_size_text = metadata_store
        .get_meta("chunk_size_bytes")
        .await
        .map_err(|_| SharingError::Backend("manifest meta lookup failed".to_owned()))?
        .ok_or_else(|| SharingError::Backend("chunk_size_bytes not set".to_owned()))?;
    let chunk_size: u32 = chunk_size_text
        .parse()
        .map_err(|_| SharingError::Backend("chunk_size_bytes is not a valid u32".to_owned()))?;

    let owner_public_key = sharing_store.get_own_public_key().await?;

    let file_key_base64 =
        file_key.with_exposed(|bytes| base64::engine::general_purpose::STANDARD.encode(bytes));

    let mut payload = SharePackagePayload {
        share_id: Uuid::new_v4().hyphenated().to_string(),
        file_id: file_id.hyphenated().to_string(),
        file_name: node.name.clone(),
        chunk_count,
        chunk_size,
        chunk_uuids,
        file_key: file_key_base64,
        sender_public_key: base64::engine::general_purpose::STANDARD
            .encode(owner_public_key.as_bytes()),
        cloud_endpoint,
        file_size: node.size_bytes,
        expires_at,
    };

    let plaintext = Zeroizing::new(
        serde_json::to_vec(&payload)
            .map_err(|error| SharingError::InvalidJsonPayload(error.to_string()))?,
    );
    payload.file_key.zeroize();

    let wire = hpke::seal(recipient_public_key, &plaintext)?;
    Ok(wire)
}

/// Imports a share package received from another vault.
///
/// Opens the HPKE envelope, deserialises the JSON payload, validates fields,
/// wraps the file key under the local key-encryption key, and persists a
/// `ReceivedShare` row.
pub(crate) async fn import_share_package(
    wire: &[u8],
    recipient_private_key_bytes: &[u8; 32],
    key_encryption_key: &KeyEncryptionKey,
    sharing_store: &dyn SharingStore,
    now_unix_seconds: i64,
) -> Result<ReceivedShare, SharingError> {
    let plaintext = hpke::open(recipient_private_key_bytes, wire)?;

    let mut payload: SharePackagePayload = serde_json::from_slice(&plaintext)
        .map_err(|error| SharingError::InvalidJsonPayload(error.to_string()))?;

    validate_payload(&payload)?;

    let file_key_bytes = decode_file_key(&payload.file_key)?;
    let sender_public_key_bytes = decode_sender_public_key(&payload.sender_public_key)?;
    payload.file_key.zeroize();

    let file_key = FileKey::from_secret_box(SecretBox::<[u8; 32]>::init_with_mut(|buffer| {
        buffer.copy_from_slice(&*file_key_bytes);
    }));
    let share_file_id_uuid = uuid::Uuid::parse_str(&payload.file_id)
        .map_err(|_| SharingError::InvalidJsonPayload("invalid file_id".to_owned()))?;
    let wrapped = wrap_file_key(
        &file_key,
        &FileId::from_uuid(share_file_id_uuid),
        key_encryption_key,
    )
    .map_err(|_| SharingError::Backend("file key wrap failed".to_owned()))?;

    let mut cloud_endpoint = payload.cloud_endpoint;
    if payload.file_size > 0 {
        cloud_endpoint["_file_size"] = serde_json::json!(payload.file_size);
    }

    let row = ReceivedShare {
        share_id: payload.share_id,
        sender_contact_id: None,
        sender_public_key: X25519PublicKey::new(sender_public_key_bytes),
        file_id: payload.file_id,
        file_name: payload.file_name,
        file_key_wrapped: *wrapped.as_bytes(),
        chunk_count: payload.chunk_count,
        chunk_size: payload.chunk_size,
        chunk_uuids: payload.chunk_uuids,
        cloud_endpoint,
        expires_at: payload.expires_at,
        imported_at: now_unix_seconds,
    };

    sharing_store.insert_received_share(&row).await?;
    Ok(row)
}

/// Validates structural invariants of a deserialized share-package payload.
fn validate_payload(payload: &SharePackagePayload) -> Result<(), SharingError> {
    if payload.share_id.is_empty() {
        return Err(SharingError::InvalidJsonPayload(
            "share_id is empty".to_owned(),
        ));
    }
    if payload.file_id.is_empty() {
        return Err(SharingError::InvalidJsonPayload(
            "file_id is empty".to_owned(),
        ));
    }
    if payload.file_name.is_empty() {
        return Err(SharingError::InvalidJsonPayload(
            "file_name is empty".to_owned(),
        ));
    }
    if payload.chunk_count as usize != payload.chunk_uuids.len() {
        return Err(SharingError::InvalidJsonPayload(format!(
            "chunk_count ({}) does not match chunk_uuids length ({})",
            payload.chunk_count,
            payload.chunk_uuids.len()
        )));
    }
    for (index, uuid_string) in payload.chunk_uuids.iter().enumerate() {
        let parsed = Uuid::parse_str(uuid_string).map_err(|_| {
            SharingError::InvalidJsonPayload(format!("chunk_uuids[{index}] is not a valid UUID"))
        })?;
        if parsed.get_version_num() != 4 {
            return Err(SharingError::InvalidJsonPayload(format!(
                "chunk_uuids[{index}] is not UUID v4"
            )));
        }
    }
    Ok(())
}

/// Decodes a base64-encoded file key, returning exactly 32 bytes.
fn decode_file_key(encoded: &str) -> Result<Zeroizing<[u8; 32]>, SharingError> {
    let decoded = Zeroizing::new(
        base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .map_err(|_| {
                SharingError::InvalidJsonPayload("file_key is not valid base64".to_owned())
            })?,
    );
    if decoded.len() != 32 {
        return Err(SharingError::InvalidFileKeyLength(decoded.len()));
    }
    let mut buffer = Zeroizing::new([0u8; 32]);
    buffer.copy_from_slice(&decoded);
    Ok(buffer)
}

/// Decodes a base64-encoded sender public key, returning exactly 32 bytes.
fn decode_sender_public_key(encoded: &str) -> Result<[u8; 32], SharingError> {
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|_| {
            SharingError::InvalidJsonPayload("sender_public_key is not valid base64".to_owned())
        })?;
    if decoded.len() != 32 {
        return Err(SharingError::InvalidSenderPublicKeyLength(decoded.len()));
    }
    let mut buffer = [0u8; 32];
    buffer.copy_from_slice(&decoded);
    Ok(buffer)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use async_trait::async_trait;
    use base64::Engine;
    use tokio::sync::Mutex;
    use uuid::Uuid;

    use crate::crypto::types::KeyEncryptionKey;
    use crate::sharing::error::SharingError;
    use crate::sharing::store::{Contact, ReceivedShare, ShareRecord, SharingStore};
    use crate::sharing::types::{ContactId, X25519PublicKey};
    use crate::storage::error::StorageError;
    use crate::storage::metadata_store::MetadataStore;
    use crate::storage::types::{ChunkRecord, Node, NodeId, NodeType};

    use super::{create_share_package, import_share_package};

    struct FakeMetadataStore {
        node: Node,
        chunks: Vec<ChunkRecord>,
        chunk_size_bytes: String,
    }

    #[async_trait]
    impl MetadataStore for FakeMetadataStore {
        async fn insert_node(&self, _node: &Node) -> Result<(), StorageError> {
            unimplemented!()
        }
        async fn insert_chunks(&self, _chunks: &[ChunkRecord]) -> Result<(), StorageError> {
            unimplemented!()
        }
        async fn insert_file_with_chunks(
            &self,
            _node: &Node,
            _chunks: &[ChunkRecord],
        ) -> Result<(), StorageError> {
            unimplemented!()
        }
        async fn get_node(&self, _node_id: Uuid) -> Result<Node, StorageError> {
            Ok(self.node.clone())
        }
        async fn list_children(&self, _parent_id: Uuid) -> Result<Vec<Node>, StorageError> {
            unimplemented!()
        }
        async fn get_chunks(&self, _node_id: Uuid) -> Result<Vec<ChunkRecord>, StorageError> {
            Ok(self.chunks.clone())
        }
        async fn rename_node(
            &self,
            _node_id: Uuid,
            _new_name: &str,
            _modified_at: i64,
        ) -> Result<(), StorageError> {
            unimplemented!()
        }
        async fn move_node(
            &self,
            _node_id: Uuid,
            _new_parent_id: Option<Uuid>,
            _modified_at: i64,
        ) -> Result<(), StorageError> {
            unimplemented!()
        }
        async fn delete_node(&self, _node_id: Uuid) -> Result<(), StorageError> {
            unimplemented!()
        }
        async fn list_pending_deletions(&self, _limit: usize) -> Result<Vec<String>, StorageError> {
            unimplemented!()
        }
        async fn mark_deletion_complete(&self, _blob_name: &str) -> Result<(), StorageError> {
            unimplemented!()
        }
        async fn get_meta(&self, key: &str) -> Result<Option<String>, StorageError> {
            if key == "chunk_size_bytes" {
                Ok(Some(self.chunk_size_bytes.clone()))
            } else {
                Ok(None)
            }
        }
        async fn set_meta(&self, _key: &str, _value: &str) -> Result<(), StorageError> {
            unimplemented!()
        }
        async fn increment_snapshot_counter(&self) -> Result<u64, StorageError> {
            unimplemented!()
        }
        async fn insert_file_node_only(&self, _node: &Node) -> Result<(), StorageError> {
            unimplemented!()
        }
        async fn insert_file_node_and_stage_epoch_entry(
            &self,
            _node: &Node,
            _plaintext: Vec<u8>,
        ) -> Result<(), StorageError> {
            unimplemented!()
        }
        async fn stage_epoch_entry(
            &self,
            _node_id: Uuid,
            _plaintext: Vec<u8>,
        ) -> Result<(), StorageError> {
            unimplemented!()
        }
        async fn get_epoch_buffer_total_bytes(&self) -> Result<u64, StorageError> {
            unimplemented!()
        }
        async fn get_epoch_buffer_entries(
            &self,
        ) -> Result<Vec<crate::storage::types::EpochBufferEntry>, StorageError> {
            unimplemented!()
        }
        async fn commit_epoch_flush(
            &self,
            _record: &crate::storage::types::EpochBlobRecord,
            _extents: &[(Uuid, u32, u64, u64)],
        ) -> Result<(), StorageError> {
            unimplemented!()
        }
        async fn get_epoch_blob(
            &self,
            _epoch_blob_id: Uuid,
        ) -> Result<crate::storage::types::EpochBlobRecord, StorageError> {
            unimplemented!()
        }
        async fn get_epoch_buffer_node_ids(&self) -> Result<Vec<Uuid>, StorageError> {
            Ok(vec![])
        }
    }

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
            let shares = self.received_shares.lock().await;
            shares
                .iter()
                .find(|share| share.share_id == share_id)
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

        async fn list_shares_by_file(
            &self,
            _file_id: &str,
        ) -> Result<Vec<ShareRecord>, SharingError> {
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

    fn make_test_kek() -> KeyEncryptionKey {
        KeyEncryptionKey::from_bytes([0xAA; 32])
    }

    fn make_wrapped_file_key(kek: &KeyEncryptionKey, node_id: Uuid) -> [u8; 72] {
        use crate::crypto::generate_file_key::generate_file_key;
        use crate::crypto::types::FileId;
        use crate::crypto::wrap_key::wrap_file_key;
        let file_key = generate_file_key();
        let wrapped =
            wrap_file_key(&file_key, &FileId::from_uuid(node_id), kek).expect("wrap must succeed");
        *wrapped.as_bytes()
    }

    fn make_test_node(node_id: Uuid, file_key_wrapped: [u8; 72]) -> Node {
        Node::new(
            node_id,
            None,
            NodeType::File,
            "test-document.pdf".to_owned(),
            1_700_000_000,
            1_700_000_000,
            1024,
            Some(file_key_wrapped),
        )
    }

    fn make_test_chunks(node_id: &NodeId) -> Vec<ChunkRecord> {
        vec![ChunkRecord {
            chunk_id: Uuid::new_v4(),
            node_id: *node_id,
            chunk_index: 0,
            blob_name: Uuid::new_v4().hyphenated().to_string(),
            size_padded: 4_194_304,
            blake3_checksum: [0x55; 32],
            epoch_blob_id: None,
            byte_offset: None,
            byte_length: None,
        }]
    }

    fn make_x25519_keypair() -> ([u8; 32], X25519PublicKey) {
        use rand::Rng;
        use x25519_dalek::{PublicKey, StaticSecret};
        let mut secret_bytes = [0u8; 32];
        rand::rng().fill_bytes(&mut secret_bytes);
        let secret = StaticSecret::from(secret_bytes);
        let public = PublicKey::from(&secret);
        (secret_bytes, X25519PublicKey::new(*public.as_bytes()))
    }

    /// Verifies a create→import round trip recovers file name, chunk data, and wraps the key.
    #[tokio::test]
    async fn test_create_import_round_trip_recovers_share_metadata() {
        let kek = make_test_kek();
        let node_id = Uuid::new_v4();
        let wrapped = make_wrapped_file_key(&kek, node_id);
        let node = make_test_node(node_id, wrapped);
        let chunks = make_test_chunks(&node.node_id);
        let (sender_secret, sender_public_key) = make_x25519_keypair();
        let (recipient_secret, recipient_public_key) = make_x25519_keypair();

        let metadata_store = FakeMetadataStore {
            node: node.clone(),
            chunks: chunks.clone(),
            chunk_size_bytes: "4194304".to_owned(),
        };
        let sharing_store = FakeSharingStore {
            owner_public_key: sender_public_key,
            received_shares: Arc::new(Mutex::new(Vec::new())),
        };

        let wire = create_share_package(
            *node.node_id.as_uuid(),
            &recipient_public_key,
            Some(1_800_000_000),
            serde_json::json!({"provider": "s3", "bucket": "test"}),
            &metadata_store,
            &sharing_store,
            &kek,
        )
        .await
        .expect("create should succeed");

        let import_kek = make_test_kek();
        let received = import_share_package(
            &wire,
            &recipient_secret,
            &import_kek,
            &sharing_store,
            1_700_000_100,
        )
        .await
        .expect("import should succeed");

        assert_eq!(received.file_name, "test-document.pdf");
        assert_eq!(received.chunk_count, 1);
        assert_eq!(received.chunk_size, 4_194_304);
        assert_eq!(received.chunk_uuids.len(), 1);
        assert_eq!(received.expires_at, Some(1_800_000_000));
        assert_eq!(received.imported_at, 1_700_000_100);
        assert_eq!(received.sender_public_key, sender_public_key);
        assert!(!received.file_key_wrapped.iter().all(|&byte| byte == 0));
        let _ = sender_secret;
    }

    /// Verifies `expires_at: None` round-trips correctly.
    #[tokio::test]
    async fn test_create_import_round_trip_preserves_none_expires_at() {
        let kek = make_test_kek();
        let node_id = Uuid::new_v4();
        let wrapped = make_wrapped_file_key(&kek, node_id);
        let node = make_test_node(node_id, wrapped);
        let chunks = make_test_chunks(&node.node_id);
        let (_sender_secret, sender_public_key) = make_x25519_keypair();
        let (recipient_secret, recipient_public_key) = make_x25519_keypair();

        let metadata_store = FakeMetadataStore {
            node: node.clone(),
            chunks,
            chunk_size_bytes: "4194304".to_owned(),
        };
        let sharing_store = FakeSharingStore {
            owner_public_key: sender_public_key,
            received_shares: Arc::new(Mutex::new(Vec::new())),
        };

        let wire = create_share_package(
            *node.node_id.as_uuid(),
            &recipient_public_key,
            None,
            serde_json::json!({}),
            &metadata_store,
            &sharing_store,
            &kek,
        )
        .await
        .expect("create should succeed");

        let received = import_share_package(
            &wire,
            &recipient_secret,
            &kek,
            &sharing_store,
            1_700_000_200,
        )
        .await
        .expect("import should succeed");

        assert_eq!(received.expires_at, None);
    }

    /// Verifies that a missing `file_name` field causes `InvalidJsonPayload`.
    #[tokio::test]
    async fn test_import_missing_file_name_returns_invalid_json_payload() {
        let (recipient_secret, recipient_public_key) = make_x25519_keypair();
        let mut payload = serde_json::json!({
            "share_id": Uuid::new_v4().hyphenated().to_string(),
            "file_id": Uuid::new_v4().hyphenated().to_string(),
            "file_name": "doc.txt",
            "chunk_count": 0,
            "chunk_size": 4194304,
            "chunk_uuids": [],
            "file_key": base64::engine::general_purpose::STANDARD.encode([0x11u8; 32]),
            "sender_public_key": base64::engine::general_purpose::STANDARD.encode([0x22u8; 32]),
            "cloud_endpoint": {}
        });
        payload.as_object_mut().unwrap().remove("file_name");
        let plaintext = serde_json::to_vec(&payload).unwrap();
        let wire = crate::sharing::hpke::seal(&recipient_public_key, &plaintext).unwrap();

        let kek = make_test_kek();
        let sharing_store = FakeSharingStore {
            owner_public_key: X25519PublicKey::new([0; 32]),
            received_shares: Arc::new(Mutex::new(Vec::new())),
        };

        let result = import_share_package(&wire, &recipient_secret, &kek, &sharing_store, 0).await;

        assert!(matches!(result, Err(SharingError::InvalidJsonPayload(_))));
    }

    /// Verifies that a 31-byte file key (after base64 decode) returns `InvalidFileKeyLength(31)`.
    #[tokio::test]
    async fn test_import_short_file_key_returns_invalid_file_key_length() {
        let (recipient_secret, recipient_public_key) = make_x25519_keypair();
        let payload = serde_json::json!({
            "share_id": Uuid::new_v4().hyphenated().to_string(),
            "file_id": Uuid::new_v4().hyphenated().to_string(),
            "file_name": "doc.txt",
            "chunk_count": 0,
            "chunk_size": 4194304,
            "chunk_uuids": [],
            "file_key": base64::engine::general_purpose::STANDARD.encode([0x11u8; 31]),
            "sender_public_key": base64::engine::general_purpose::STANDARD.encode([0x22u8; 32]),
            "cloud_endpoint": {}
        });
        let plaintext = serde_json::to_vec(&payload).unwrap();
        let wire = crate::sharing::hpke::seal(&recipient_public_key, &plaintext).unwrap();

        let kek = make_test_kek();
        let sharing_store = FakeSharingStore {
            owner_public_key: X25519PublicKey::new([0; 32]),
            received_shares: Arc::new(Mutex::new(Vec::new())),
        };

        let result = import_share_package(&wire, &recipient_secret, &kek, &sharing_store, 0).await;

        assert!(
            matches!(result, Err(SharingError::InvalidFileKeyLength(31))),
            "expected InvalidFileKeyLength(31), got {result:?}"
        );
    }

    /// Verifies that a 33-byte sender public key returns `InvalidSenderPublicKeyLength(33)`.
    #[tokio::test]
    async fn test_import_long_sender_key_returns_invalid_sender_public_key_length() {
        let (recipient_secret, recipient_public_key) = make_x25519_keypair();
        let payload = serde_json::json!({
            "share_id": Uuid::new_v4().hyphenated().to_string(),
            "file_id": Uuid::new_v4().hyphenated().to_string(),
            "file_name": "doc.txt",
            "chunk_count": 0,
            "chunk_size": 4194304,
            "chunk_uuids": [],
            "file_key": base64::engine::general_purpose::STANDARD.encode([0x11u8; 32]),
            "sender_public_key": base64::engine::general_purpose::STANDARD.encode([0x22u8; 33]),
            "cloud_endpoint": {}
        });
        let plaintext = serde_json::to_vec(&payload).unwrap();
        let wire = crate::sharing::hpke::seal(&recipient_public_key, &plaintext).unwrap();

        let kek = make_test_kek();
        let sharing_store = FakeSharingStore {
            owner_public_key: X25519PublicKey::new([0; 32]),
            received_shares: Arc::new(Mutex::new(Vec::new())),
        };

        let result = import_share_package(&wire, &recipient_secret, &kek, &sharing_store, 0).await;

        assert!(
            matches!(result, Err(SharingError::InvalidSenderPublicKeyLength(33))),
            "expected InvalidSenderPublicKeyLength(33), got {result:?}"
        );
    }

    /// Verifies that imported shares have non-empty wrapped key and sender public key.
    #[tokio::test]
    async fn test_import_populates_file_key_wrapped_and_sender_public_key() {
        let kek = make_test_kek();
        let node_id = Uuid::new_v4();
        let wrapped = make_wrapped_file_key(&kek, node_id);
        let node = make_test_node(node_id, wrapped);
        let chunks = make_test_chunks(&node.node_id);
        let (_sender_secret, sender_public_key) = make_x25519_keypair();
        let (recipient_secret, recipient_public_key) = make_x25519_keypair();

        let metadata_store = FakeMetadataStore {
            node: node.clone(),
            chunks,
            chunk_size_bytes: "4194304".to_owned(),
        };
        let received_shares = Arc::new(Mutex::new(Vec::new()));
        let sharing_store = FakeSharingStore {
            owner_public_key: sender_public_key,
            received_shares: received_shares.clone(),
        };

        let wire = create_share_package(
            *node.node_id.as_uuid(),
            &recipient_public_key,
            None,
            serde_json::json!({"remote": "test"}),
            &metadata_store,
            &sharing_store,
            &kek,
        )
        .await
        .expect("create should succeed");

        let received = import_share_package(
            &wire,
            &recipient_secret,
            &kek,
            &sharing_store,
            1_700_000_300,
        )
        .await
        .expect("import should succeed");

        assert_ne!(received.sender_public_key, X25519PublicKey::new([0; 32]));
        assert!(!received.file_key_wrapped.iter().all(|&byte| byte == 0));

        let persisted = received_shares.lock().await;
        assert_eq!(persisted.len(), 1);
        assert_eq!(persisted[0].share_id, received.share_id);
    }
}
