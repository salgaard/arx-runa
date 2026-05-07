use serde::Deserialize;

/// Mirror of `src-tauri/src/ui/types/reconcile_result.rs::ReconcileResult`.
/// Kept in sync manually until shared type generation is introduced.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReconcileResult {
    /// Number of blobs pulled from the primary destination into local staging.
    pub blobs_pulled: u32,
    /// Number of local blobs that were already staged and untouched.
    pub local_blobs_staged: u32,
    /// Cloud snapshot counter this device is now aligned to.
    pub cloud_counter: u64,
}
