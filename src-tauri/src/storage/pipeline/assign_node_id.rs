use crate::storage::types::{ChunkRecord, NodeId};

/// Assigns a node identifier to all generated chunk records.
pub(crate) fn assign_node_id(records: &mut [ChunkRecord], node_id: NodeId) {
    for record in records {
        record.node_id = node_id;
    }
}
