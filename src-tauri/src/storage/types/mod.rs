//! Domain types for the storage module.
//!
//! Newtypes added in implementation phases.

mod blob_name;
mod chunk_record;
mod epoch_blob_record;
mod epoch_buffer_entry;
mod node;
mod node_id;
mod sync_chunk_record;

pub use blob_name::BlobName;
pub use chunk_record::ChunkRecord;
pub use epoch_blob_record::EpochBlobRecord;
pub use epoch_buffer_entry::EpochBufferEntry;
pub use node::{Node, NodeType};
pub use node_id::NodeId;
pub use sync_chunk_record::SyncChunkRecord;
