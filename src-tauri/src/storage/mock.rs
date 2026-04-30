//! In-memory `MetadataStore` implementation for tests.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use uuid::Uuid;

use crate::storage::error::StorageError;
use crate::storage::metadata_store::MetadataStore;
use crate::storage::types::{ChunkRecord, EpochBlobRecord, EpochBufferEntry, Node, NodeId};
use crate::storage::validation::{
    immutable_meta_key_violation, is_immutable_manifest_meta_key, parse_chunk_size_bytes,
    validate_blob_name_uuid_v4, validate_chunk_target_node,
    validate_size_padded_matches_chunk_size,
};

/// Test-only in-memory metadata store.
#[derive(Debug, Clone)]
pub struct MockMetadataStore {
    /// Shared mutable in-memory state.
    inner: Arc<Mutex<MockState>>,
}

/// In-memory state backing `MockMetadataStore`.
#[derive(Debug)]
struct MockState {
    /// Node rows by primary key.
    nodes: HashMap<Uuid, Node>,
    /// Chunk rows grouped by node identifier.
    chunks_by_node: HashMap<Uuid, Vec<ChunkRecord>>,
    /// Manifest metadata key/value pairs.
    meta: HashMap<String, String>,
    /// Pending deletion queue entries with queued timestamp.
    pending_deletions: Vec<(String, i64)>,
    /// Epoch buffer staged plaintext entries.
    pub(super) epoch_buffer: Vec<EpochBufferEntry>,
    /// Epoch blob records by primary key.
    pub(super) epoch_blobs: HashMap<Uuid, EpochBlobRecord>,
}

impl Default for MockMetadataStore {
    /// Creates an empty store with default seeded `manifest_meta`.
    fn default() -> Self {
        let mut meta = HashMap::new();
        meta.insert("schema_version".to_owned(), "1".to_owned());
        meta.insert("vault_id".to_owned(), Uuid::nil().hyphenated().to_string());
        meta.insert("snapshot_counter".to_owned(), "0".to_owned());
        meta.insert("chunk_size_bytes".to_owned(), "4194304".to_owned());
        meta.insert("epoch_buffer_enabled".to_owned(), "false".to_owned());
        Self {
            inner: Arc::new(Mutex::new(MockState {
                nodes: HashMap::new(),
                chunks_by_node: HashMap::new(),
                meta,
                pending_deletions: Vec::new(),
                epoch_buffer: Vec::new(),
                epoch_blobs: HashMap::new(),
            })),
        }
    }
}

impl MockMetadataStore {
    /// Creates a default mock metadata store.
    pub fn new() -> Self {
        Self::default()
    }
}

/// Returns current Unix timestamp in seconds.
fn unix_timestamp_now() -> Result<i64, StorageError> {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| StorageError::Database(error.to_string()))?;
    i64::try_from(duration.as_secs()).map_err(|error| StorageError::Database(error.to_string()))
}

fn ensure_parent_is_directory_for_insert(
    state: &MockState,
    node: &Node,
) -> Result<(), StorageError> {
    if let Some(parent_id) = node.parent_id.map(|id| *id.as_uuid()) {
        if parent_id == *node.node_id.as_uuid() {
            return Err(StorageError::ConstraintViolation(
                "node cannot be its own parent".to_owned(),
            ));
        }

        let parent = state.nodes.get(&parent_id).ok_or_else(|| {
            StorageError::ConstraintViolation("missing parent node_id".to_owned())
        })?;
        if !matches!(parent.node_type, crate::storage::types::NodeType::Directory) {
            return Err(StorageError::ConstraintViolation(
                "parent must be a directory".to_owned(),
            ));
        }
    }
    Ok(())
}

fn ensure_node_type_file_key_wrapped_parity(node: &Node) -> Result<(), StorageError> {
    match node.node_type {
        crate::storage::types::NodeType::File if node.file_key_wrapped.is_none() => Err(
            StorageError::ConstraintViolation("file node requires file_key_wrapped".to_owned()),
        ),
        crate::storage::types::NodeType::Directory if node.file_key_wrapped.is_some() => {
            Err(StorageError::ConstraintViolation(
                "directory node must not include file_key_wrapped".to_owned(),
            ))
        }
        _ => Ok(()),
    }
}

fn ensure_move_respects_hierarchy(
    state: &MockState,
    node_id: Uuid,
    new_parent_id: Option<Uuid>,
) -> Result<(), StorageError> {
    if !state.nodes.contains_key(&node_id) {
        return Err(StorageError::NotFound);
    }

    if let Some(parent_id) = new_parent_id {
        if parent_id == node_id {
            return Err(StorageError::ConstraintViolation(
                "node cannot be its own parent".to_owned(),
            ));
        }

        let parent = state.nodes.get(&parent_id).ok_or_else(|| {
            StorageError::ConstraintViolation("missing parent node_id".to_owned())
        })?;
        if !matches!(parent.node_type, crate::storage::types::NodeType::Directory) {
            return Err(StorageError::ConstraintViolation(
                "parent must be a directory".to_owned(),
            ));
        }

        let mut current = Some(parent_id);
        while let Some(candidate) = current {
            if candidate == node_id {
                return Err(StorageError::ConstraintViolation(
                    "move would create a cycle".to_owned(),
                ));
            }
            current = state
                .nodes
                .get(&candidate)
                .and_then(|node| node.parent_id.map(|id| *id.as_uuid()));
        }
    }

    Ok(())
}

#[async_trait]
impl MetadataStore for MockMetadataStore {
    /// Inserts one node row.
    async fn insert_node(&self, node: &Node) -> Result<(), StorageError> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|error| StorageError::Database(error.to_string()))?;
        let node_uuid = *node.node_id.as_uuid();
        ensure_node_type_file_key_wrapped_parity(node)?;
        ensure_parent_is_directory_for_insert(&guard, node)?;
        if guard.nodes.contains_key(&node_uuid) {
            return Err(StorageError::ConstraintViolation(
                "duplicate node_id".to_owned(),
            ));
        }
        guard.nodes.insert(node_uuid, node.clone());
        Ok(())
    }

    /// Inserts chunk rows and simulates unique constraints.
    async fn insert_chunks(&self, chunks: &[ChunkRecord]) -> Result<(), StorageError> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|error| StorageError::Database(error.to_string()))?;
        let chunk_size_value = guard.meta.get("chunk_size_bytes").ok_or_else(|| {
            StorageError::Database("missing manifest_meta key: chunk_size_bytes".to_owned())
        })?;
        let chunk_size_bytes = parse_chunk_size_bytes(chunk_size_value)?;
        let mut all_blob_names = HashSet::new();
        for existing_chunks in guard.chunks_by_node.values() {
            for existing in existing_chunks {
                all_blob_names.insert(existing.blob_name.clone());
            }
        }

        for chunk in chunks {
            let node_uuid = *chunk.node_id.as_uuid();
            let node_type = guard.nodes.get(&node_uuid).map(|node| node.node_type);
            validate_chunk_target_node(node_type)?;
            validate_blob_name_uuid_v4(&chunk.blob_name)?;
            validate_size_padded_matches_chunk_size(chunk.size_padded, chunk_size_bytes)?;
            if all_blob_names.contains(&chunk.blob_name) {
                return Err(StorageError::ConstraintViolation(
                    "duplicate blob_name".to_owned(),
                ));
            }

            let node_chunks = guard.chunks_by_node.entry(node_uuid).or_default();
            if node_chunks
                .iter()
                .any(|existing| existing.chunk_index == chunk.chunk_index)
            {
                return Err(StorageError::ConstraintViolation(
                    "duplicate (node_id, chunk_index)".to_owned(),
                ));
            }

            node_chunks.push(chunk.clone());
            all_blob_names.insert(chunk.blob_name.clone());
            node_chunks.sort_by_key(|item| item.chunk_index);
        }

        Ok(())
    }

    /// Inserts a file node and chunk rows atomically.
    async fn insert_file_with_chunks(
        &self,
        node: &Node,
        chunks: &[ChunkRecord],
    ) -> Result<(), StorageError> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|error| StorageError::Database(error.to_string()))?;

        let node_uuid = *node.node_id.as_uuid();
        ensure_node_type_file_key_wrapped_parity(node)?;
        ensure_parent_is_directory_for_insert(&guard, node)?;
        if guard.nodes.contains_key(&node_uuid) {
            return Err(StorageError::ConstraintViolation(
                "duplicate node_id".to_owned(),
            ));
        }

        let chunk_size_value = guard.meta.get("chunk_size_bytes").ok_or_else(|| {
            StorageError::Database("missing manifest_meta key: chunk_size_bytes".to_owned())
        })?;
        let chunk_size_bytes = parse_chunk_size_bytes(chunk_size_value)?;
        let mut all_blob_names = HashSet::new();
        for existing_chunks in guard.chunks_by_node.values() {
            for existing in existing_chunks {
                all_blob_names.insert(existing.blob_name.clone());
            }
        }
        let mut seen_chunk_indices = HashSet::new();
        for chunk in chunks {
            if chunk.node_id != node.node_id {
                return Err(StorageError::ConstraintViolation(
                    "chunk node_id does not match inserted file node".to_owned(),
                ));
            }
            validate_chunk_target_node(Some(node.node_type))?;
            validate_blob_name_uuid_v4(&chunk.blob_name)?;
            validate_size_padded_matches_chunk_size(chunk.size_padded, chunk_size_bytes)?;
            if all_blob_names.contains(&chunk.blob_name) {
                return Err(StorageError::ConstraintViolation(
                    "duplicate blob_name".to_owned(),
                ));
            }
            if !seen_chunk_indices.insert(chunk.chunk_index) {
                return Err(StorageError::ConstraintViolation(
                    "duplicate (node_id, chunk_index)".to_owned(),
                ));
            }
            all_blob_names.insert(chunk.blob_name.clone());
        }

        guard.nodes.insert(node_uuid, node.clone());
        let node_chunks = guard.chunks_by_node.entry(node_uuid).or_default();
        node_chunks.extend(chunks.iter().cloned());
        node_chunks.sort_by_key(|item| item.chunk_index);
        Ok(())
    }

    /// Retrieves one node by ID.
    async fn get_node(&self, node_id: Uuid) -> Result<Node, StorageError> {
        let guard = self
            .inner
            .lock()
            .map_err(|error| StorageError::Database(error.to_string()))?;
        guard
            .nodes
            .get(&node_id)
            .cloned()
            .ok_or(StorageError::NotFound)
    }

    /// Lists direct children for a parent node.
    async fn list_children(&self, parent_id: Uuid) -> Result<Vec<Node>, StorageError> {
        let guard = self
            .inner
            .lock()
            .map_err(|error| StorageError::Database(error.to_string()))?;
        let mut children = guard
            .nodes
            .values()
            .filter(|node| node.parent_id.map(|id| *id.as_uuid()) == Some(parent_id))
            .cloned()
            .collect::<Vec<_>>();
        children.sort_by(|left, right| left.name.cmp(&right.name));
        Ok(children)
    }

    /// Returns chunks for a node ordered by index.
    async fn get_chunks(&self, node_id: Uuid) -> Result<Vec<ChunkRecord>, StorageError> {
        let guard = self
            .inner
            .lock()
            .map_err(|error| StorageError::Database(error.to_string()))?;
        Ok(guard
            .chunks_by_node
            .get(&node_id)
            .cloned()
            .unwrap_or_default())
    }

    /// Renames a node and updates `modified_at`.
    async fn rename_node(
        &self,
        node_id: Uuid,
        new_name: &str,
        modified_at: i64,
    ) -> Result<(), StorageError> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|error| StorageError::Database(error.to_string()))?;
        let node = guard
            .nodes
            .get_mut(&node_id)
            .ok_or(StorageError::NotFound)?;
        node.name = new_name.to_owned();
        node.modified_at = modified_at;
        Ok(())
    }

    /// Moves a node and updates `modified_at`.
    async fn move_node(
        &self,
        node_id: Uuid,
        new_parent_id: Option<Uuid>,
        modified_at: i64,
    ) -> Result<(), StorageError> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|error| StorageError::Database(error.to_string()))?;
        ensure_move_respects_hierarchy(&guard, node_id, new_parent_id)?;
        let node = guard
            .nodes
            .get_mut(&node_id)
            .ok_or(StorageError::NotFound)?;
        node.parent_id = new_parent_id.map(NodeId::new);
        node.modified_at = modified_at;
        Ok(())
    }

    /// Deletes a node, cascades chunk removal, and enqueues blob deletions.
    async fn delete_node(&self, node_id: Uuid) -> Result<(), StorageError> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|error| StorageError::Database(error.to_string()))?;
        if !guard.nodes.contains_key(&node_id) {
            return Err(StorageError::NotFound);
        }

        let mut subtree_node_ids = vec![node_id];
        let mut index = 0usize;
        while index < subtree_node_ids.len() {
            let current = subtree_node_ids[index];
            let descendants = guard
                .nodes
                .values()
                .filter(|node| node.parent_id.map(|id| *id.as_uuid()) == Some(current))
                .map(|node| *node.node_id.as_uuid())
                .collect::<Vec<_>>();
            subtree_node_ids.extend(descendants);
            index += 1;
        }

        let queued_at = unix_timestamp_now()?;
        for subtree_node_id in &subtree_node_ids {
            if let Some(chunks) = guard.chunks_by_node.remove(subtree_node_id) {
                for chunk in chunks {
                    if !guard
                        .pending_deletions
                        .iter()
                        .any(|(blob_name, _)| blob_name == &chunk.blob_name)
                    {
                        guard.pending_deletions.push((chunk.blob_name, queued_at));
                    }
                }
            }
        }
        for subtree_node_id in subtree_node_ids {
            guard.nodes.remove(&subtree_node_id);
        }
        guard
            .pending_deletions
            .sort_by(|left, right| left.1.cmp(&right.1).then_with(|| left.0.cmp(&right.0)));

        Ok(())
    }

    /// Lists queued pending deletions up to `limit`.
    async fn list_pending_deletions(&self, limit: usize) -> Result<Vec<String>, StorageError> {
        let guard = self
            .inner
            .lock()
            .map_err(|error| StorageError::Database(error.to_string()))?;
        let mut pending_deletions = guard.pending_deletions.clone();
        pending_deletions
            .sort_by(|left, right| left.1.cmp(&right.1).then_with(|| left.0.cmp(&right.0)));
        Ok(pending_deletions
            .iter()
            .take(limit)
            .map(|(blob_name, _)| blob_name.clone())
            .collect())
    }

    /// Removes one queued pending deletion.
    async fn mark_deletion_complete(&self, blob_name: &str) -> Result<(), StorageError> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|error| StorageError::Database(error.to_string()))?;
        guard
            .pending_deletions
            .retain(|(queued_blob_name, _)| queued_blob_name != blob_name);
        Ok(())
    }

    /// Retrieves a manifest metadata value by key.
    async fn get_meta(&self, key: &str) -> Result<Option<String>, StorageError> {
        let guard = self
            .inner
            .lock()
            .map_err(|error| StorageError::Database(error.to_string()))?;
        Ok(guard.meta.get(key).cloned())
    }

    /// Sets or updates a manifest metadata key.
    async fn set_meta(&self, key: &str, value: &str) -> Result<(), StorageError> {
        if is_immutable_manifest_meta_key(key) {
            return Err(immutable_meta_key_violation(key));
        }

        let mut guard = self
            .inner
            .lock()
            .map_err(|error| StorageError::Database(error.to_string()))?;
        guard.meta.insert(key.to_owned(), value.to_owned());
        Ok(())
    }

    /// Atomically increments and returns `snapshot_counter`.
    async fn increment_snapshot_counter(&self) -> Result<u64, StorageError> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|error| StorageError::Database(error.to_string()))?;
        let value = guard
            .meta
            .get("snapshot_counter")
            .ok_or(StorageError::NotFound)?;
        let next = value
            .parse::<u64>()
            .map_err(|_| {
                StorageError::Database(
                    "invalid snapshot_counter: not an unsigned integer".to_owned(),
                )
            })?
            .checked_add(1)
            .ok_or_else(|| {
                StorageError::Database("invalid snapshot_counter: overflow".to_owned())
            })?;
        guard
            .meta
            .insert("snapshot_counter".to_owned(), next.to_string());
        Ok(next)
    }

    /// Inserts a file node row without any associated chunk rows.
    async fn insert_file_node_only(&self, node: &Node) -> Result<(), StorageError> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|error| StorageError::Database(error.to_string()))?;
        let node_uuid = *node.node_id.as_uuid();
        ensure_node_type_file_key_wrapped_parity(node)?;
        ensure_parent_is_directory_for_insert(&guard, node)?;
        if guard.nodes.contains_key(&node_uuid) {
            return Err(StorageError::ConstraintViolation(
                "duplicate node_id".to_owned(),
            ));
        }
        guard.nodes.insert(node_uuid, node.clone());
        Ok(())
    }

    /// Inserts a file node and stages its epoch buffer entry atomically.
    ///
    /// The mock acquires the lock once so both mutations are visible together,
    /// which is sufficient for tests (mocks do not crash).
    async fn insert_file_node_and_stage_epoch_entry(
        &self,
        node: &Node,
        plaintext: Vec<u8>,
    ) -> Result<(), StorageError> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|error| StorageError::Database(error.to_string()))?;
        let node_uuid = *node.node_id.as_uuid();
        ensure_node_type_file_key_wrapped_parity(node)?;
        ensure_parent_is_directory_for_insert(&guard, node)?;
        if guard.nodes.contains_key(&node_uuid) {
            return Err(StorageError::ConstraintViolation(
                "duplicate node_id".to_owned(),
            ));
        }
        guard.nodes.insert(node_uuid, node.clone());
        let size_bytes = plaintext.len() as u64;
        guard.epoch_buffer.push(EpochBufferEntry {
            entry_id: Uuid::new_v4(),
            node_id: node_uuid,
            plaintext,
            size_bytes,
        });
        Ok(())
    }

    /// Stages a plaintext entry in the epoch buffer for the given node.
    async fn stage_epoch_entry(
        &self,
        node_id: Uuid,
        plaintext: Vec<u8>,
    ) -> Result<(), StorageError> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|error| StorageError::Database(error.to_string()))?;
        let size_bytes = plaintext.len() as u64;
        guard.epoch_buffer.push(EpochBufferEntry {
            entry_id: Uuid::new_v4(),
            node_id,
            plaintext,
            size_bytes,
        });
        Ok(())
    }

    /// Returns the total number of bytes currently staged in the epoch buffer.
    async fn get_epoch_buffer_total_bytes(&self) -> Result<u64, StorageError> {
        let guard = self
            .inner
            .lock()
            .map_err(|error| StorageError::Database(error.to_string()))?;
        let total = guard
            .epoch_buffer
            .iter()
            .map(|entry| entry.size_bytes)
            .fold(0u64, u64::saturating_add);
        Ok(total)
    }

    /// Returns all entries currently staged in the epoch buffer.
    async fn get_epoch_buffer_entries(&self) -> Result<Vec<EpochBufferEntry>, StorageError> {
        let guard = self
            .inner
            .lock()
            .map_err(|error| StorageError::Database(error.to_string()))?;
        Ok(guard.epoch_buffer.clone())
    }

    /// Atomically stores the epoch blob record, inserts chunk rows, and clears the buffer.
    async fn commit_epoch_flush(
        &self,
        record: &EpochBlobRecord,
        extents: &[(Uuid, u32, u64, u64)],
    ) -> Result<(), StorageError> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|error| StorageError::Database(error.to_string()))?;
        guard
            .epoch_blobs
            .insert(record.epoch_blob_id, record.clone());
        for &(node_id, chunk_index, byte_offset, byte_length) in extents {
            let chunk = ChunkRecord {
                chunk_id: Uuid::new_v4(),
                node_id: node_id.into(),
                chunk_index,
                blob_name: record.blob_name.clone(),
                size_padded: record.size_padded,
                blake3_checksum: record.blake3_checksum,
                epoch_blob_id: Some(record.epoch_blob_id),
                byte_offset: Some(byte_offset),
                byte_length: Some(byte_length),
            };
            guard.chunks_by_node.entry(node_id).or_default().push(chunk);
        }
        guard.epoch_buffer.clear();
        Ok(())
    }

    /// Retrieves an epoch blob record by identifier.
    async fn get_epoch_blob(&self, epoch_blob_id: Uuid) -> Result<EpochBlobRecord, StorageError> {
        let guard = self
            .inner
            .lock()
            .map_err(|error| StorageError::Database(error.to_string()))?;
        guard
            .epoch_blobs
            .get(&epoch_blob_id)
            .cloned()
            .ok_or(StorageError::NotFound)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use uuid::Uuid;

    use super::MockMetadataStore;
    use crate::storage::MetadataStore;
    use crate::storage::error::StorageError;
    use crate::storage::types::{ChunkRecord, Node, NodeType};

    fn directory_node(node_id: Uuid, parent_id: Option<Uuid>, name: &str) -> Node {
        Node::new(
            node_id,
            parent_id,
            NodeType::Directory,
            name.to_owned(),
            1,
            1,
            0,
            None,
        )
    }

    fn file_node(node_id: Uuid, parent_id: Option<Uuid>, name: &str) -> Node {
        Node::new(
            node_id,
            parent_id,
            NodeType::File,
            name.to_owned(),
            1,
            1,
            42,
            Some([3; 72]),
        )
    }

    fn chunk_for(node_id: Uuid, chunk_index: u32, blob_name: &str) -> ChunkRecord {
        ChunkRecord {
            chunk_id: Uuid::new_v4(),
            node_id: node_id.into(),
            chunk_index,
            blob_name: blob_name.to_owned(),
            size_padded: 4_194_304,
            blake3_checksum: [1; 32],
            epoch_blob_id: None,
            byte_offset: None,
            byte_length: None,
        }
    }

    #[tokio::test]
    async fn test_delete_node_removes_subtree_and_enqueues_all_subtree_chunks() {
        let store = MockMetadataStore::new();

        let root_id = Uuid::new_v4();
        let child_dir_id = Uuid::new_v4();
        let grandchild_dir_id = Uuid::new_v4();
        let child_file_id = Uuid::new_v4();
        let grandchild_file_id = Uuid::new_v4();
        let sibling_file_id = Uuid::new_v4();

        store
            .insert_node(&directory_node(root_id, None, "root"))
            .await
            .expect("root should insert");
        store
            .insert_node(&directory_node(child_dir_id, Some(root_id), "child"))
            .await
            .expect("child dir should insert");
        store
            .insert_node(&directory_node(
                grandchild_dir_id,
                Some(child_dir_id),
                "grandchild",
            ))
            .await
            .expect("grandchild dir should insert");
        store
            .insert_node(&file_node(child_file_id, Some(child_dir_id), "child.txt"))
            .await
            .expect("child file should insert");
        store
            .insert_node(&file_node(
                grandchild_file_id,
                Some(grandchild_dir_id),
                "grandchild.txt",
            ))
            .await
            .expect("grandchild file should insert");
        store
            .insert_node(&file_node(sibling_file_id, Some(root_id), "sibling.txt"))
            .await
            .expect("sibling file should insert");

        store
            .insert_chunks(&[
                chunk_for(child_file_id, 0, "11111111-1111-4111-8111-111111111111"),
                chunk_for(
                    grandchild_file_id,
                    0,
                    "22222222-2222-4222-8222-222222222222",
                ),
                chunk_for(sibling_file_id, 0, "33333333-3333-4333-8333-333333333333"),
            ])
            .await
            .expect("chunks should insert");

        store
            .delete_node(child_dir_id)
            .await
            .expect("subtree delete should succeed");

        assert!(matches!(
            store.get_node(child_dir_id).await,
            Err(StorageError::NotFound)
        ));
        assert!(matches!(
            store.get_node(grandchild_dir_id).await,
            Err(StorageError::NotFound)
        ));
        assert!(matches!(
            store.get_node(child_file_id).await,
            Err(StorageError::NotFound)
        ));
        assert!(matches!(
            store.get_node(grandchild_file_id).await,
            Err(StorageError::NotFound)
        ));
        assert!(store.get_node(sibling_file_id).await.is_ok());

        let pending = store
            .list_pending_deletions(10)
            .await
            .expect("pending deletions should list");
        assert_eq!(
            pending,
            vec![
                "11111111-1111-4111-8111-111111111111".to_owned(),
                "22222222-2222-4222-8222-222222222222".to_owned(),
            ]
        );
    }

    #[tokio::test]
    async fn test_list_pending_deletions_orders_by_queued_at_then_blob_name_before_limit() {
        let store = MockMetadataStore::new();
        {
            let mut guard = store
                .inner
                .lock()
                .expect("mock store lock should not be poisoned");
            guard.pending_deletions = vec![
                ("cccccccc-cccc-4ccc-8ccc-cccccccccccc".to_owned(), 20),
                ("bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb".to_owned(), 10),
                ("aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa".to_owned(), 10),
            ];
        }

        let pending = store
            .list_pending_deletions(2)
            .await
            .expect("pending deletions should list");
        assert_eq!(
            pending,
            vec![
                "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa".to_owned(),
                "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb".to_owned(),
            ]
        );
    }

    #[tokio::test]
    async fn test_mark_deletion_complete_existing_blob_removes_only_target_blob() {
        let store = MockMetadataStore::new();
        {
            let mut guard = store
                .inner
                .lock()
                .expect("mock store lock should not be poisoned");
            guard.pending_deletions = vec![
                ("aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa".to_owned(), 10),
                ("bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb".to_owned(), 11),
            ];
        }

        store
            .mark_deletion_complete("aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa")
            .await
            .expect("deletion completion should succeed");
        let pending = store
            .list_pending_deletions(10)
            .await
            .expect("pending deletions should list");

        assert_eq!(
            pending,
            vec!["bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb".to_owned()]
        );
    }

    #[tokio::test]
    async fn test_insert_node_rejects_file_parent_constraint_violation() {
        let store = MockMetadataStore::new();
        let file_parent_id = Uuid::new_v4();
        let child_id = Uuid::new_v4();

        store
            .insert_node(&file_node(file_parent_id, None, "file-parent.txt"))
            .await
            .expect("file parent should insert");

        let result = store
            .insert_node(&file_node(child_id, Some(file_parent_id), "child.txt"))
            .await;
        assert!(matches!(
            result,
            Err(StorageError::ConstraintViolation(message)) if message.contains("directory")
        ));
    }

    #[tokio::test]
    async fn test_insert_node_rejects_self_parent_constraint_violation() {
        let store = MockMetadataStore::new();
        let node_id = Uuid::new_v4();

        let result = store
            .insert_node(&directory_node(node_id, Some(node_id), "self-parent"))
            .await;
        assert!(matches!(
            result,
            Err(StorageError::ConstraintViolation(message)) if message.contains("own parent")
        ));
    }

    #[tokio::test]
    async fn test_insert_node_rejects_file_without_wrapped_key_constraint_violation() {
        let store = MockMetadataStore::new();
        let file_without_key = Node::new(
            Uuid::new_v4(),
            None,
            NodeType::File,
            "missing-key.txt".to_owned(),
            1,
            1,
            42,
            None,
        );

        let result = store.insert_node(&file_without_key).await;
        assert!(matches!(
            result,
            Err(StorageError::ConstraintViolation(message)) if message.contains("file_key_wrapped")
        ));
    }

    #[tokio::test]
    async fn test_insert_node_rejects_directory_with_wrapped_key_constraint_violation() {
        let store = MockMetadataStore::new();
        let directory_with_key = Node::new(
            Uuid::new_v4(),
            None,
            NodeType::Directory,
            "bad-dir".to_owned(),
            1,
            1,
            0,
            Some([4; 72]),
        );

        let result = store.insert_node(&directory_with_key).await;
        assert!(matches!(
            result,
            Err(StorageError::ConstraintViolation(message)) if message.contains("file_key_wrapped")
        ));
    }

    #[tokio::test]
    async fn test_insert_chunks_rejects_invalid_blob_name_constraint_violation() {
        let store = MockMetadataStore::new();
        let file_id = Uuid::new_v4();
        store
            .insert_node(&file_node(file_id, None, "file.txt"))
            .await
            .expect("file should insert");

        let result = store
            .insert_chunks(&[chunk_for(file_id, 0, "not-a-uuid")])
            .await;
        assert!(matches!(
            result,
            Err(StorageError::ConstraintViolation(message)) if message.contains("blob_name")
        ));
    }

    #[tokio::test]
    async fn test_insert_chunks_rejects_non_v4_blob_name_constraint_violation() {
        let store = MockMetadataStore::new();
        let file_id = Uuid::new_v4();
        store
            .insert_node(&file_node(file_id, None, "file.txt"))
            .await
            .expect("file should insert");

        let result = store
            .insert_chunks(&[chunk_for(
                file_id,
                0,
                "f81d4fae-7dec-11d0-a765-00a0c91e6bf6",
            )])
            .await;
        assert!(matches!(
            result,
            Err(StorageError::ConstraintViolation(message)) if message.contains("UUID v4")
        ));
    }

    #[tokio::test]
    async fn test_insert_chunks_rejects_missing_node_constraint_violation() {
        let store = MockMetadataStore::new();
        let missing_node_id = Uuid::new_v4();

        let result = store
            .insert_chunks(&[chunk_for(
                missing_node_id,
                0,
                "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
            )])
            .await;
        assert!(matches!(
            result,
            Err(StorageError::ConstraintViolation(message)) if message.contains("missing")
        ));
    }

    #[tokio::test]
    async fn test_insert_chunks_rejects_directory_node_constraint_violation() {
        let store = MockMetadataStore::new();
        let directory_id = Uuid::new_v4();
        store
            .insert_node(&directory_node(directory_id, None, "dir"))
            .await
            .expect("directory should insert");

        let result = store
            .insert_chunks(&[chunk_for(
                directory_id,
                0,
                "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb",
            )])
            .await;
        assert!(matches!(
            result,
            Err(StorageError::ConstraintViolation(message)) if message.contains("directory")
        ));
    }

    #[tokio::test]
    async fn test_insert_chunks_rejects_size_padded_mismatch_constraint_violation() {
        let store = MockMetadataStore::new();
        let file_id = Uuid::new_v4();
        store
            .insert_node(&file_node(file_id, None, "file.txt"))
            .await
            .expect("file should insert");
        let mut chunk = chunk_for(file_id, 0, "cccccccc-cccc-4ccc-8ccc-cccccccccccc");
        chunk.size_padded = 1024;

        let result = store.insert_chunks(&[chunk]).await;
        assert!(matches!(
            result,
            Err(StorageError::ConstraintViolation(message)) if message.contains("size_padded")
        ));
    }

    #[tokio::test]
    async fn test_move_node_rejects_cycle_constraint_violation() {
        let store = MockMetadataStore::new();
        let root_id = Uuid::new_v4();
        let child_id = Uuid::new_v4();
        let grandchild_id = Uuid::new_v4();

        store
            .insert_node(&directory_node(root_id, None, "root"))
            .await
            .expect("root should insert");
        store
            .insert_node(&directory_node(child_id, Some(root_id), "child"))
            .await
            .expect("child should insert");
        store
            .insert_node(&directory_node(grandchild_id, Some(child_id), "grandchild"))
            .await
            .expect("grandchild should insert");

        let result = store.move_node(root_id, Some(grandchild_id), 2).await;
        assert!(matches!(
            result,
            Err(StorageError::ConstraintViolation(message)) if message.contains("cycle")
        ));
    }

    #[tokio::test]
    async fn test_move_node_rejects_file_parent_constraint_violation() {
        let store = MockMetadataStore::new();
        let root_id = Uuid::new_v4();
        let destination_file_id = Uuid::new_v4();
        let moving_id = Uuid::new_v4();

        store
            .insert_node(&directory_node(root_id, None, "root"))
            .await
            .expect("root should insert");
        store
            .insert_node(&file_node(
                destination_file_id,
                Some(root_id),
                "destination.txt",
            ))
            .await
            .expect("destination file should insert");
        store
            .insert_node(&directory_node(moving_id, Some(root_id), "moving"))
            .await
            .expect("moving node should insert");

        let result = store
            .move_node(moving_id, Some(destination_file_id), 3)
            .await;
        assert!(matches!(
            result,
            Err(StorageError::ConstraintViolation(message)) if message.contains("directory")
        ));
    }

    #[tokio::test]
    async fn test_list_children_parent_with_unsorted_names_returns_name_sorted_children() {
        let store = MockMetadataStore::new();
        let root_id = Uuid::new_v4();
        let child_a_id = Uuid::new_v4();
        let child_b_id = Uuid::new_v4();
        let grandchild_id = Uuid::new_v4();
        store
            .insert_node(&directory_node(root_id, None, "root"))
            .await
            .expect("root should insert");
        store
            .insert_node(&file_node(child_b_id, Some(root_id), "b-file"))
            .await
            .expect("child b should insert");
        store
            .insert_node(&directory_node(child_a_id, Some(root_id), "a-dir"))
            .await
            .expect("child a should insert");
        store
            .insert_node(&file_node(grandchild_id, Some(child_a_id), "nested"))
            .await
            .expect("nested should insert");

        let children = store
            .list_children(root_id)
            .await
            .expect("children should list");
        let names = children
            .into_iter()
            .map(|node| node.name)
            .collect::<Vec<_>>();

        assert_eq!(names, vec!["a-dir".to_owned(), "b-file".to_owned()]);
    }

    #[tokio::test]
    async fn test_rename_node_existing_node_updates_name_and_modified_at() {
        let store = MockMetadataStore::new();
        let node_id = Uuid::new_v4();
        store
            .insert_node(&file_node(node_id, None, "before.txt"))
            .await
            .expect("node should insert");

        store
            .rename_node(node_id, "after.txt", 77)
            .await
            .expect("rename should succeed");

        let renamed = store.get_node(node_id).await.expect("node should load");
        assert_eq!(renamed.name, "after.txt");
        assert_eq!(renamed.modified_at, 77);
    }

    #[tokio::test]
    async fn test_move_node_existing_node_updates_parent_and_modified_at() {
        let store = MockMetadataStore::new();
        let root_id = Uuid::new_v4();
        let parent_a_id = Uuid::new_v4();
        let parent_b_id = Uuid::new_v4();
        let moving_id = Uuid::new_v4();
        store
            .insert_node(&directory_node(root_id, None, "root"))
            .await
            .expect("root should insert");
        store
            .insert_node(&directory_node(parent_a_id, Some(root_id), "a"))
            .await
            .expect("parent a should insert");
        store
            .insert_node(&directory_node(parent_b_id, Some(root_id), "b"))
            .await
            .expect("parent b should insert");
        store
            .insert_node(&file_node(moving_id, Some(parent_a_id), "moving.txt"))
            .await
            .expect("moving should insert");

        store
            .move_node(moving_id, Some(parent_b_id), 88)
            .await
            .expect("move should succeed");

        let moved = store.get_node(moving_id).await.expect("node should load");
        assert_eq!(moved.parent_id.map(|id| *id.as_uuid()), Some(parent_b_id));
        assert_eq!(moved.modified_at, 88);
    }

    #[tokio::test]
    async fn test_insert_node_zero_byte_file_accepts_and_round_trips() {
        let store = MockMetadataStore::new();
        let node_id = Uuid::new_v4();
        let zero_byte_file = Node::new(
            node_id,
            None,
            NodeType::File,
            "empty.txt".to_owned(),
            1,
            1,
            0,
            Some([3; 72]),
        );

        store
            .insert_node(&zero_byte_file)
            .await
            .expect("zero-byte file should insert");

        let stored = store.get_node(node_id).await.expect("file should load");
        assert_eq!(stored.size_bytes, 0);
        assert!(matches!(stored.node_type, NodeType::File));
    }

    #[tokio::test]
    async fn test_set_meta_rejects_immutable_keys_and_allows_mutable_key() {
        let store = MockMetadataStore::new();
        let immutable_result = store.set_meta("chunk_size_bytes", "8192").await;
        assert!(matches!(
            immutable_result,
            Err(StorageError::ConstraintViolation(message)) if message.contains("immutable")
        ));

        store
            .set_meta("last_synced_at", "123")
            .await
            .expect("mutable key should succeed");
        assert_eq!(
            store
                .get_meta("last_synced_at")
                .await
                .expect("value should be readable"),
            Some("123".to_owned())
        );
    }

    #[tokio::test]
    async fn test_set_meta_rejects_snapshot_counter_key() {
        let store = MockMetadataStore::new();
        let result = store.set_meta("snapshot_counter", "99").await;
        assert!(matches!(
            result,
            Err(StorageError::ConstraintViolation(message)) if message.contains("snapshot_counter")
        ));
    }

    #[tokio::test]
    async fn test_increment_snapshot_counter_reports_invalid_value() {
        let store = MockMetadataStore::new();
        {
            let mut guard = store
                .inner
                .lock()
                .expect("mock store lock should not be poisoned");
            guard
                .meta
                .insert("snapshot_counter".to_owned(), "invalid".to_owned());
        }

        let result = store.increment_snapshot_counter().await;
        assert!(matches!(
            result,
            Err(StorageError::Database(message)) if message.contains("snapshot_counter")
        ));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_increment_snapshot_counter_concurrent_calls_return_unique_sequence() {
        let store = Arc::new(MockMetadataStore::new());
        let mut tasks = Vec::new();
        for _ in 0..16 {
            let store = Arc::clone(&store);
            tasks.push(tokio::spawn(async move {
                store
                    .increment_snapshot_counter()
                    .await
                    .expect("increment should succeed")
            }));
        }
        let mut values = Vec::new();
        for task in tasks {
            values.push(task.await.expect("task should complete"));
        }
        values.sort_unstable();

        assert_eq!(values, (1u64..=16).collect::<Vec<_>>());
        assert_eq!(
            store
                .get_meta("snapshot_counter")
                .await
                .expect("meta should load"),
            Some("16".to_owned())
        );
    }
}
