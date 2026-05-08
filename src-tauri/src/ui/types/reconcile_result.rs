//! Result type for the `pull_and_reconcile` IPC command.

use serde::Serialize;

/// Result returned from a completed `pull_and_reconcile` operation.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReconcileResult {
    /// Number of pending cloud deletions drained during reconciliation.
    pub pending_deletions_drained: u32,
    /// Cloud snapshot counter this device is now aligned to.
    pub cloud_counter: u64,
}
