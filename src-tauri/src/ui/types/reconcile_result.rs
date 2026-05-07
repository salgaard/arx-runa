//! Result type for the `pull_and_reconcile` IPC command.

use serde::Serialize;

/// Result returned from a completed `pull_and_reconcile` operation.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReconcileResult {
    /// Number of blobs pulled from the primary destination into local staging.
    pub blobs_pulled: u32,
    /// Number of local blobs that were already staged and untouched.
    pub local_blobs_staged: u32,
    /// Cloud snapshot counter this device is now aligned to.
    pub cloud_counter: u64,
}
