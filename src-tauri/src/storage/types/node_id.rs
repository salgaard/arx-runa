use std::fmt::{Display, Formatter};

use uuid::Uuid;

/// Strongly-typed identifier for rows in the `nodes` table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeId(Uuid);

impl NodeId {
    /// Constructs a new node identifier from a UUID.
    pub fn new(value: Uuid) -> Self {
        Self(value)
    }

    /// Returns the inner UUID value.
    pub fn as_uuid(&self) -> &Uuid {
        &self.0
    }
}

impl Display for NodeId {
    /// Formats the node identifier as a hyphenated UUID string.
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}", self.0.hyphenated())
    }
}

impl From<Uuid> for NodeId {
    /// Converts a UUID into a `NodeId`.
    fn from(value: Uuid) -> Self {
        Self::new(value)
    }
}

