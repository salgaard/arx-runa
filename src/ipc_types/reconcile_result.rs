use serde::Deserialize;

/// Mirror of `src-tauri/src/ui/types/reconcile_result.rs::ReconcileResult`.
/// Kept in sync manually until shared type generation is introduced.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReconcileResult {
    /// Number of pending cloud deletions drained during reconciliation.
    pub pending_deletions_drained: u32,
    /// Cloud snapshot counter this device is now aligned to.
    pub cloud_counter: u64,
    /// Names of files that were renamed as conflicted copies during reconciliation.
    pub conflicts_renamed: Vec<String>,
}
