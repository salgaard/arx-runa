//! Migration progress update type.

use serde::Serialize;

/// Progress update emitted during `migrate_vault`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MigrationProgress {
    /// Overall migration progress from 0 to 100.
    pub percent: u8,
    /// Number of blobs transferred to the new destination so far.
    pub blobs_transferred: u32,
    /// Total number of blobs to transfer.
    pub blobs_total: u32,
    /// Human-readable description of the current migration phase.
    pub current_phase: String,
}
