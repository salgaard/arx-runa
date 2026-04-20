use std::convert::TryFrom;

use uuid::Uuid;

use super::NodeId;

/// Classifies whether a node is a file or a directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeType {
    /// A file node with encrypted chunk rows.
    File,
    /// A directory node with no file payload.
    Directory,
}

impl AsRef<str> for NodeType {
    /// Returns the canonical SQL string for this node type.
    fn as_ref(&self) -> &str {
        match self {
            Self::File => "file",
            Self::Directory => "directory",
        }
    }
}

impl TryFrom<&str> for NodeType {
    type Error = String;

    /// Parses a SQL node-type string into a strongly typed value.
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "file" => Ok(Self::File),
            "directory" => Ok(Self::Directory),
            _ => Err(format!("invalid node_type: {value}")),
        }
    }
}

/// Domain representation of a row in the `nodes` table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    /// Primary key of the node row.
    pub node_id: NodeId,
    /// Parent node identifier or `None` for root-level nodes.
    pub parent_id: Option<NodeId>,
    /// File or directory discriminator.
    pub node_type: NodeType,
    /// Plaintext display name.
    pub name: String,
    /// Creation timestamp in Unix seconds.
    pub created_at: i64,
    /// Last modification timestamp in Unix seconds.
    pub modified_at: i64,
    /// Original file size in bytes, `0` for directories.
    pub size_bytes: u64,
    /// Wrapped file key bytes for files, `None` for directories.
    pub file_key_wrapped: Option<[u8; 72]>,
}

impl Node {
    /// Builds a new node from strongly-typed fields.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        node_id: Uuid,
        parent_id: Option<Uuid>,
        node_type: NodeType,
        name: String,
        created_at: i64,
        modified_at: i64,
        size_bytes: u64,
        file_key_wrapped: Option<[u8; 72]>,
    ) -> Self {
        Self {
            node_id: NodeId::new(node_id),
            parent_id: parent_id.map(NodeId::new),
            node_type,
            name,
            created_at,
            modified_at,
            size_bytes,
            file_key_wrapped,
        }
    }
}
