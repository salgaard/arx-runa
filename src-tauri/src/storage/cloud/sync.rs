//! Cloud vault synchronisation flows.
//!
//! Implements push/pull/delete orchestration anchored to
//! `docs/architecture/designs/cloud-synchronisation/design.md`.

use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use futures_util::stream::{FuturesUnordered, StreamExt};
#[cfg(test)]
use rand::RngExt;
use rand::TryRng;
use serde::Serialize;
use thiserror::Error;
use tokio::io::AsyncReadExt;
use zeroize::Zeroizing;

use super::manifest_backup::{
    MANIFEST_BACKUP_BLOB_NAME, ManifestBackupSyncError, download_manifest_backup,
    upload_manifest_backup,
};
use super::remote_path::validate_remote_path;
use super::vault_header::VaultHeader;
use super::vault_header_io::{VAULT_HEADER_BLOB_NAME, VaultHeaderSyncError, upload_vault_header};
use super::{CloudTransport, CloudTransportError, SyncConfig};
#[cfg(test)]
use crate::crypto::compute_checksum;
use crate::crypto::{ManifestKey, SqlcipherKey};
use crate::storage::error::StorageError;
use crate::storage::metadata_store::MetadataStore;
use crate::storage::sqlcipher::{SqlCipherMetadataStore, read_snapshot_state_from_database};
use crate::storage::types::SyncChunkRecord;
use crate::storage::validation::validate_blob_name_uuid_v4;
use uuid::Uuid;

const CONFLICT_PROBE_DB_FILE_NAME: &str = "manifest-backup-conflict-probe.db";
const PENDING_DELETIONS_BATCH_LIMIT: usize = 128;

/// Cloud snapshot state used for local/cloud conflict detection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CloudSnapshotState {
    /// Cloud snapshot counter.
    pub snapshot_counter: u64,
    /// Optional cloud `last_synced_at` metadata.
    pub last_synced_at: Option<i64>,
}

/// Conflict payload surfaced when local/cloud `snapshot_counter` diverges.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SyncConflict {
    /// Local manifest `snapshot_counter`.
    pub local_counter: u64,
    /// Cloud manifest `snapshot_counter`.
    pub cloud_counter: u64,
    /// Optional local `last_synced_at`.
    pub local_last_synced: Option<i64>,
    /// Optional cloud `last_synced_at`.
    pub cloud_last_synced: Option<i64>,
}

/// Push-flow completion report.
#[derive(Debug, Clone)]
pub struct PushReport {
    /// Number of uploaded blob staging files.
    pub blobs_uploaded: usize,
    /// Names of all blobs successfully uploaded in this push (used to seed mirror queues).
    pub uploaded_blob_names: Vec<String>,
    /// Local snapshot counter after successful push.
    pub snapshot_counter_after: u64,
    /// Total push duration in seconds.
    pub duration_seconds: f64,
}

/// Pull-flow completion report.
#[derive(Debug, Clone)]
pub struct PullReport {
    /// Number of blobs downloaded and verified.
    pub blobs_downloaded: usize,
    /// Number of blobs skipped because they already existed in staging.
    pub blobs_skipped_present: usize,
    /// Total pull duration in seconds.
    pub duration_seconds: f64,
}

/// Best-effort cloud deletion report.
#[derive(Debug, Clone)]
pub struct CloudDeletionReport {
    /// Number of vault blobs deleted successfully.
    pub vault_blobs_deleted: usize,
    /// Vault blob paths that failed deletion or were rejected.
    pub vault_blobs_failed: Vec<String>,
    /// Whether manifest backup deletion succeeded.
    pub manifest_backup_deleted: bool,
    /// Whether vault header deletion succeeded.
    pub vault_header_deleted: bool,
}

/// Errors produced by cloud sync flows.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum SyncError {
    /// Cloud/local `snapshot_counter` mismatch.
    #[error("snapshot_counter conflict with cloud")]
    Conflict(SyncConflict),
    /// Cloud manifest backup exists but cannot be decrypted or integrity-checked.
    #[error("cloud manifest backup could not be decrypted or verified: {reason}")]
    CloudManifestUnreadable { reason: ManifestBackupSyncError },
    /// Push failed while uploading blob payloads.
    #[error("push failed during blob upload: {first_error}")]
    PushUploadFailed {
        /// First encountered upload error.
        first_error: Box<CloudTransportError>,
        /// Blob names uploaded successfully before failure.
        successful_uploads: Vec<String>,
    },
    /// Push failed while uploading manifest backup.
    #[error("push failed during manifest backup upload: {source}")]
    PushManifestBackupFailed { source: ManifestBackupSyncError },
    /// Snapshot rollback failed after manifest upload failure.
    #[error("snapshot_counter rollback failed after manifest-upload error")]
    RollbackFailed {
        /// Original manifest backup upload error.
        manifest_error: Box<ManifestBackupSyncError>,
        /// Rollback failure.
        rollback_error: Box<StorageError>,
    },
    /// Push failed after manifest success while uploading vault header.
    #[error("vault header upload failed: {source}")]
    VaultHeaderUploadFailed { source: VaultHeaderSyncError },
    /// Pull finished with one or more per-blob failures.
    #[error("pull completed with failures")]
    PullIncomplete {
        /// Blob names that failed checksum verification.
        verification_failures: Vec<String>,
        /// Transport failures keyed by blob name.
        transport_failures: Vec<(String, CloudTransportError)>,
    },
    /// Cloud transport failure.
    #[error("cloud transport operation failed: {source}")]
    Transport {
        /// Wrapped transport failure.
        #[from]
        source: CloudTransportError,
    },
    /// Manifest backup pipeline failure.
    #[error("manifest backup operation failed: {source}")]
    ManifestBackup {
        /// Wrapped manifest-backup failure.
        #[from]
        source: ManifestBackupSyncError,
    },
    /// Storage failure.
    #[error("storage error: {source}")]
    Storage {
        /// Wrapped storage failure.
        #[from]
        source: StorageError,
    },
    /// Local filesystem I/O failure.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

/// Reads cloud `snapshot_counter` and optional `last_synced_at` via a temporary
/// manifest-backup probe database.
pub(crate) async fn read_cloud_snapshot_state(
    cloud_transport: &dyn CloudTransport,
    staging_dir: &Path,
    manifest_key: &ManifestKey,
    sqlcipher_key: &SqlcipherKey,
) -> Result<Option<CloudSnapshotState>, SyncError> {
    let probe_path = staging_dir.join(CONFLICT_PROBE_DB_FILE_NAME);
    remove_file_if_present(&probe_path).await?;
    let manifest_key_bytes = Zeroizing::new(manifest_key.with_exposed(|bytes| *bytes));

    let download_result = download_manifest_backup(
        cloud_transport,
        staging_dir,
        &manifest_key_bytes,
        &probe_path,
        sqlcipher_key,
    )
    .await;

    let state_result = match download_result {
        Ok(()) => read_cloud_snapshot_from_probe_db(&probe_path, sqlcipher_key).await,
        Err(ManifestBackupSyncError::Transport(CloudTransportError::NotFound)) => Ok(None),
        Err(
            reason @ (ManifestBackupSyncError::CryptoFailed
            | ManifestBackupSyncError::IntegrityCheckFailed),
        ) => Err(SyncError::CloudManifestUnreadable { reason }),
        Err(source) => Err(SyncError::ManifestBackup { source }),
    };

    if let Err(cleanup_error) = remove_file_if_present(&probe_path).await {
        return Err(SyncError::Io(cleanup_error));
    }

    state_result
}

/// In-place Fisher-Yates shuffle using the provided random-number generator.
#[cfg(test)]
pub(crate) fn fisher_yates_shuffle<T, R: RngExt + ?Sized>(items: &mut [T], rng: &mut R) {
    if items.len() < 2 {
        return;
    }
    for i in (1..items.len()).rev() {
        let j = rng.random_range(0..=i);
        items.swap(i, j);
    }
}

fn fisher_yates_shuffle_with_system_rng<T>(items: &mut [T]) -> Result<(), SyncError> {
    if items.len() < 2 {
        return Ok(());
    }
    let mut rng = rand::rngs::SysRng;
    for i in (1..items.len()).rev() {
        let j = unbiased_index(&mut rng, i + 1)?;
        items.swap(i, j);
    }
    Ok(())
}

fn unbiased_index(
    rng: &mut rand::rngs::SysRng,
    upper_exclusive: usize,
) -> Result<usize, SyncError> {
    if upper_exclusive == 0 {
        return Ok(0);
    }
    let bound = upper_exclusive as u64;
    let zone = u64::MAX - (u64::MAX % bound);
    loop {
        let sample = rng.try_next_u64().map_err(|error| SyncError::Transport {
            source: CloudTransportError::Other(format!("system RNG failure: {error}")),
        })?;
        if sample < zone {
            return Ok((sample % bound) as usize);
        }
    }
}

enum DownloadTaskError {
    Verification(String),
    Transport(String, CloudTransportError),
}

async fn upload_blob_task(
    blob_name: String,
    cloud_transport: &dyn CloudTransport,
    staging_dir: &Path,
) -> Result<String, CloudTransportError> {
    let remote_path = build_blob_remote_path(&blob_name).map_err(|error| {
        if let SyncError::Transport { source } = error {
            source
        } else {
            CloudTransportError::Other(error.to_string())
        }
    })?;
    let local_path = pending_blob_path(staging_dir, &blob_name);
    cloud_transport
        .upload_blob(&local_path, &remote_path)
        .await?;
    remove_file_if_present(&local_path)
        .await
        .map_err(CloudTransportError::IoError)?;
    Ok(blob_name)
}

async fn download_blob_task(
    chunk: SyncChunkRecord,
    cloud_transport: &dyn CloudTransport,
    staging_dir: &Path,
) -> Result<(), DownloadTaskError> {
    let blob_name = chunk.blob_name;
    let remote_path = build_blob_remote_path(&blob_name).map_err(|error| {
        DownloadTaskError::Transport(
            blob_name.clone(),
            CloudTransportError::Other(error.to_string()),
        )
    })?;
    let local_path = cache_blob_path(staging_dir, &blob_name);
    cloud_transport
        .download_blob(&remote_path, &local_path)
        .await
        .map_err(|error| DownloadTaskError::Transport(blob_name.clone(), error))?;
    let checksum_matches = verify_blob_checksum(&local_path, &chunk.blake3_checksum)
        .await
        .map_err(|error| {
            DownloadTaskError::Transport(blob_name.clone(), CloudTransportError::IoError(error))
        })?;
    if !checksum_matches {
        let _ = remove_file_if_present(&local_path).await;
        return Err(DownloadTaskError::Verification(blob_name));
    }
    Ok(())
}

/// Drives push upload operations for staged blobs.
pub(crate) async fn drive_blob_uploads(
    blobs: Vec<String>,
    cloud_transport: &dyn CloudTransport,
    staging_dir: &Path,
    max_concurrent: usize,
) -> Result<Vec<String>, (Box<CloudTransportError>, Vec<String>)> {
    let concurrency_limit = max_concurrent.max(1);
    let mut pending = blobs.into_iter();
    let mut in_flight = FuturesUnordered::new();
    let mut successful_uploads = Vec::new();
    let mut first_error: Option<Box<CloudTransportError>> = None;

    for _ in 0..concurrency_limit {
        if let Some(blob_name) = pending.next() {
            in_flight.push(upload_blob_task(blob_name, cloud_transport, staging_dir));
        }
    }

    while let Some(result) = in_flight.next().await {
        match result {
            Ok(blob_name) => {
                successful_uploads.push(blob_name);
                if first_error.is_none()
                    && let Some(blob_name) = pending.next()
                {
                    in_flight.push(upload_blob_task(blob_name, cloud_transport, staging_dir));
                }
            }
            Err(error) => {
                if first_error.is_none() {
                    first_error = Some(Box::new(error));
                }
            }
        }
    }

    if let Some(first_error) = first_error {
        Err((first_error, successful_uploads))
    } else {
        Ok(successful_uploads)
    }
}

/// Drives pull download operations, collecting verification and transport
/// failures.
pub(crate) async fn drive_blob_downloads(
    chunks_to_fetch: Vec<SyncChunkRecord>,
    cloud_transport: &dyn CloudTransport,
    staging_dir: &Path,
    max_concurrent: usize,
) -> Result<usize, (Vec<String>, Vec<(String, CloudTransportError)>)> {
    let concurrency_limit = max_concurrent.max(1);
    let mut pending = chunks_to_fetch.into_iter();
    let mut in_flight = FuturesUnordered::new();
    let mut downloaded = 0usize;
    let mut verification_failures = Vec::new();
    let mut transport_failures = Vec::new();

    for _ in 0..concurrency_limit {
        if let Some(chunk) = pending.next() {
            in_flight.push(download_blob_task(chunk, cloud_transport, staging_dir));
        }
    }

    while let Some(result) = in_flight.next().await {
        match result {
            Ok(()) => {
                downloaded += 1;
            }
            Err(DownloadTaskError::Verification(blob_name)) => {
                verification_failures.push(blob_name);
            }
            Err(DownloadTaskError::Transport(blob_name, error)) => {
                transport_failures.push((blob_name, error));
            }
        }
        if let Some(chunk) = pending.next() {
            in_flight.push(download_blob_task(chunk, cloud_transport, staging_dir));
        }
    }

    if verification_failures.is_empty() && transport_failures.is_empty() {
        Ok(downloaded)
    } else {
        Err((verification_failures, transport_failures))
    }
}

/// Downloads any blobs required to decrypt `node_id` that are missing from `staging_dir`.
///
/// After a push sync the local blob files are removed; this restores only the
/// blobs needed for the requested file rather than pulling the entire vault.
/// Handles both regular multi-chunk files and epoch-packed files.
/// No-ops when all required blobs are already present locally.
pub(crate) async fn fetch_missing_file_blobs(
    node_id: Uuid,
    db: &dyn MetadataStore,
    staging_dir: &Path,
    cloud_transport: &dyn CloudTransport,
    on_blob_downloaded: Option<&(dyn Fn(u64, u64) + Send + Sync)>,
) -> Result<(), SyncError> {
    use std::collections::HashMap;

    let chunks = db.get_chunks(node_id).await?;
    let mut needed: HashMap<String, [u8; 32]> = HashMap::new();

    for chunk in &chunks {
        let (blob_name, checksum) = if let Some(epoch_id) = chunk.epoch_blob_id {
            let epoch = db.get_epoch_blob(epoch_id).await?;
            (epoch.blob_name, epoch.blake3_checksum)
        } else {
            (chunk.blob_name.clone(), chunk.blake3_checksum)
        };
        needed.entry(blob_name).or_insert(checksum);
    }

    let total = needed.len() as u64;
    let mut done: u64 = 0;

    for (blob_name, blake3_checksum) in needed {
        // Skip if blob is already present in either pending/ or cache/.
        if tokio::fs::try_exists(&pending_blob_path(staging_dir, &blob_name)).await?
            || tokio::fs::try_exists(&cache_blob_path(staging_dir, &blob_name)).await?
        {
            done += 1;
            if let Some(cb) = on_blob_downloaded {
                cb(done, total);
            }
            continue;
        }
        download_blob_task(
            SyncChunkRecord {
                blob_name,
                blake3_checksum,
            },
            cloud_transport,
            staging_dir,
        )
        .await
        .map_err(|e| match e {
            DownloadTaskError::Transport(_, err) => SyncError::Transport { source: err },
            DownloadTaskError::Verification(name) => SyncError::Transport {
                source: CloudTransportError::Other(format!(
                    "Blob checksum mismatch after download: {name}"
                )),
            },
        })?;
        done += 1;
        if let Some(cb) = on_blob_downloaded {
            cb(done, total);
        }
    }

    Ok(())
}

/// Pushes staged vault blobs, then uploads manifest backup and vault header.
///
/// The optional `progress` callback is invoked with
/// `(blobs_done, blobs_total, None)` immediately before blob uploads begin
/// (with `blobs_done = 0`) and again after all uploads complete
/// (with `blobs_done = successful_count`).  Pass `None` to suppress progress
/// reporting.  The callback MUST NOT import or depend on `tauri::`.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub async fn push_vault(
    vault_db_path: &Path,
    sqlcipher_key: &SqlcipherKey,
    manifest_key: &ManifestKey,
    metadata_store: &SqlCipherMetadataStore,
    cloud_transport: &dyn CloudTransport,
    vault_header: &VaultHeader,
    staging_dir: &Path,
    sync_config: &SyncConfig,
    progress: Option<&(dyn Fn(u32, u32, Option<&str>) + Send + Sync)>,
) -> Result<PushReport, SyncError> {
    sync_config
        .validate()
        .map_err(|source| SyncError::Transport { source })?;
    let started = Instant::now();
    let local_counter = read_required_u64_meta(metadata_store, "snapshot_counter").await?;
    let local_last_synced = read_optional_i64_meta(metadata_store, "last_synced_at").await?;
    let previous_last_synced_raw = metadata_store.get_meta("last_synced_at").await?;

    // CloudManifestUnreadable means the cloud manifest is encrypted with a different
    // key than what we hold — this happens after a phrase-recovery rekey where the
    // manifest backup upload was skipped (no transport at recovery time). Treat it as
    // "no valid cloud snapshot" so the push proceeds and overwrites the stale blob.
    let cloud_snapshot_state = match read_cloud_snapshot_state(
        cloud_transport,
        staging_dir,
        manifest_key,
        sqlcipher_key,
    )
    .await
    {
        Ok(state) => state,
        Err(SyncError::CloudManifestUnreadable { .. }) => {
            tracing::warn!(
                "Cloud manifest unreadable with current key (post-rekey stale blob); treating as first push"
            );
            None
        }
        Err(other) => return Err(other),
    };
    if let Some(cloud_state) = cloud_snapshot_state
        && cloud_state.snapshot_counter > local_counter
    {
        return Err(SyncError::Conflict(SyncConflict {
            local_counter,
            cloud_counter: cloud_state.snapshot_counter,
            local_last_synced,
            cloud_last_synced: cloud_state.last_synced_at,
        }));
    }

    let chunks = metadata_store.list_sync_chunks().await?;
    let mut upload_blobs = Vec::new();
    for chunk in chunks {
        validate_blob_name_uuid_v4(&chunk.blob_name)?;
        let local_path = pending_blob_path(staging_dir, &chunk.blob_name);
        if tokio::fs::try_exists(&local_path).await? {
            upload_blobs.push(chunk.blob_name);
        }
    }

    fisher_yates_shuffle_with_system_rng(&mut upload_blobs)?;
    let total_upload_blobs = upload_blobs.len();
    if !upload_blobs.is_empty() {
        cloud_transport
            .ensure_folder("vault")
            .await
            .map_err(|source| SyncError::Transport { source })?;
    }
    if let Some(cb) = progress {
        cb(0, total_upload_blobs as u32, None);
    }
    let successful_uploads = drive_blob_uploads(
        upload_blobs,
        cloud_transport,
        staging_dir,
        sync_config.max_concurrent as usize,
    )
    .await
    .map_err(
        |(first_error, successful_uploads)| SyncError::PushUploadFailed {
            first_error,
            successful_uploads,
        },
    )?;
    if let Some(cb) = progress {
        cb(
            successful_uploads.len() as u32,
            total_upload_blobs as u32,
            None,
        );
    }

    let new_counter = metadata_store.increment_snapshot_counter().await?;
    metadata_store
        .set_meta("last_synced_at", &now_unix_seconds().to_string())
        .await?;

    let manifest_key_bytes = Zeroizing::new(manifest_key.with_exposed(|bytes| *bytes));
    if let Err(source) = upload_manifest_backup(
        vault_db_path,
        sqlcipher_key,
        &manifest_key_bytes,
        cloud_transport,
        staging_dir,
    )
    .await
    {
        let rollback_result = metadata_store
            .rollback_snapshot_counter(local_counter)
            .await;
        let restore_result = restore_last_synced_at(metadata_store, previous_last_synced_raw).await;
        return match (rollback_result, restore_result) {
            (Ok(()), Ok(())) => Err(SyncError::PushManifestBackupFailed { source }),
            (Err(rollback_error), _) | (_, Err(rollback_error)) => Err(SyncError::RollbackFailed {
                manifest_error: Box::new(source),
                rollback_error: Box::new(rollback_error),
            }),
        };
    }

    upload_vault_header(vault_header, cloud_transport, staging_dir)
        .await
        .map_err(|source| SyncError::VaultHeaderUploadFailed { source })?;
    drain_pending_deletions(metadata_store, cloud_transport).await?;

    Ok(PushReport {
        blobs_uploaded: successful_uploads.len(),
        uploaded_blob_names: successful_uploads,
        snapshot_counter_after: new_counter,
        duration_seconds: started.elapsed().as_secs_f64(),
    })
}

/// Pulls vault blobs described by an already-imported local manifest.
///
/// The optional `progress` callback is invoked with
/// `(blobs_done, blobs_total, None)` immediately before blob downloads begin
/// (with `blobs_done = 0`) and again after all downloads complete.
/// Pass `None` to suppress progress reporting.  The callback MUST NOT import
/// or depend on `tauri::`.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub async fn pull_vault(
    _vault_db_path: &Path,
    _sqlcipher_key: &SqlcipherKey,
    _manifest_key: &ManifestKey,
    metadata_store_after_import: &SqlCipherMetadataStore,
    cloud_transport: &dyn CloudTransport,
    staging_dir: &Path,
    sync_config: &SyncConfig,
    progress: Option<&(dyn Fn(u32, u32, Option<&str>) + Send + Sync)>,
) -> Result<PullReport, SyncError> {
    sync_config
        .validate()
        .map_err(|source| SyncError::Transport { source })?;
    let started = Instant::now();
    let chunks = metadata_store_after_import.list_sync_chunks().await?;
    let mut chunks_to_fetch = Vec::new();
    let mut blobs_skipped_present = 0usize;

    for chunk in chunks {
        validate_blob_name_uuid_v4(&chunk.blob_name)?;
        let cache_path = cache_blob_path(staging_dir, &chunk.blob_name);
        if tokio::fs::try_exists(&cache_path).await? {
            if verify_blob_checksum(&cache_path, &chunk.blake3_checksum).await? {
                blobs_skipped_present += 1;
            } else {
                remove_file_if_present(&cache_path).await?;
                chunks_to_fetch.push(chunk);
            }
            continue;
        }
        // Also check the pending directory: blobs staged locally for upload but not yet on
        // cloud (e.g. device B's changes after a conflict). If the checksum matches they are
        // already correct on-disk and don't need to be downloaded.
        let pending_path = pending_blob_path(staging_dir, &chunk.blob_name);
        if tokio::fs::try_exists(&pending_path).await?
            && let Ok(true) = verify_blob_checksum(&pending_path, &chunk.blake3_checksum).await
        {
            blobs_skipped_present += 1;
            continue;
        }
        chunks_to_fetch.push(chunk);
    }

    let total_to_fetch = chunks_to_fetch.len();
    if let Some(cb) = progress {
        cb(0, total_to_fetch as u32, None);
    }
    let blobs_downloaded = drive_blob_downloads(
        chunks_to_fetch,
        cloud_transport,
        staging_dir,
        sync_config.max_concurrent as usize,
    )
    .await
    .map_err(
        |(verification_failures, transport_failures)| SyncError::PullIncomplete {
            verification_failures,
            transport_failures,
        },
    )?;
    if let Some(cb) = progress {
        cb(blobs_downloaded as u32, total_to_fetch as u32, None);
    }
    drain_pending_deletions(metadata_store_after_import, cloud_transport).await?;

    Ok(PullReport {
        blobs_downloaded,
        blobs_skipped_present,
        duration_seconds: started.elapsed().as_secs_f64(),
    })
}

/// Deletes cloud-side vault blobs plus manifest backup and vault header.
pub async fn delete_vault_from_cloud(
    cloud_transport: &dyn CloudTransport,
) -> Result<CloudDeletionReport, SyncError> {
    validate_remote_path("vault/")?;
    let blob_paths = cloud_transport.list_blobs("vault/").await?;
    let mut report = CloudDeletionReport {
        vault_blobs_deleted: 0,
        vault_blobs_failed: Vec::new(),
        manifest_backup_deleted: false,
        vault_header_deleted: false,
    };

    for blob_path in blob_paths {
        if validate_vault_blob_remote_path(&blob_path).is_err() {
            report.vault_blobs_failed.push(blob_path);
            continue;
        }
        match cloud_transport.delete_blob(&blob_path).await {
            Ok(()) => report.vault_blobs_deleted += 1,
            Err(_) => report.vault_blobs_failed.push(blob_path),
        }
    }

    report.manifest_backup_deleted = cloud_transport
        .delete_blob(MANIFEST_BACKUP_BLOB_NAME)
        .await
        .is_ok();
    report.vault_header_deleted = cloud_transport
        .delete_blob(VAULT_HEADER_BLOB_NAME)
        .await
        .is_ok();
    Ok(report)
}

/// Returns the path for a locally-encrypted blob awaiting upload.
fn pending_blob_path(staging_dir: &Path, blob_name: &str) -> PathBuf {
    staging_dir
        .join("pending")
        .join(format!("{blob_name}.blob"))
}

/// Returns the path for a blob fetched from cloud for local viewing/decryption.
fn cache_blob_path(staging_dir: &Path, blob_name: &str) -> PathBuf {
    staging_dir.join("cache").join(format!("{blob_name}.blob"))
}

fn build_blob_remote_path(blob_name: &str) -> Result<String, SyncError> {
    validate_blob_name_uuid_v4(blob_name)?;
    let remote_path = format!("vault/{blob_name}.blob");
    validate_remote_path(&remote_path)?;
    Ok(remote_path)
}

fn validate_vault_blob_remote_path(path: &str) -> Result<(), SyncError> {
    validate_remote_path(path)?;
    if !path.starts_with("vault/") || !path.ends_with(".blob") {
        return Err(SyncError::Transport {
            source: CloudTransportError::Other("invalid vault blob path".to_owned()),
        });
    }
    let blob_name = &path["vault/".len()..path.len() - ".blob".len()];
    validate_blob_name_uuid_v4(blob_name)?;
    Ok(())
}

async fn read_cloud_snapshot_from_probe_db(
    probe_path: &Path,
    sqlcipher_key: &SqlcipherKey,
) -> Result<Option<CloudSnapshotState>, SyncError> {
    let probe_path = probe_path.to_path_buf();
    let sqlcipher_key = sqlcipher_key_from_array(sqlcipher_key.with_exposed(|bytes| *bytes));
    tokio::task::spawn_blocking(move || -> Result<Option<CloudSnapshotState>, SyncError> {
        let (snapshot_counter, last_synced_at) =
            read_snapshot_state_from_database(&probe_path, &sqlcipher_key)?;
        Ok(Some(CloudSnapshotState {
            snapshot_counter,
            last_synced_at,
        }))
    })
    .await
    .map_err(|error| SyncError::Storage {
        source: StorageError::Database(error.to_string()),
    })?
}

fn sqlcipher_key_from_array(bytes: [u8; 32]) -> SqlcipherKey {
    let mut boxed = Box::new([0u8; 32]);
    boxed.copy_from_slice(&bytes);
    SqlcipherKey::from_secret_box(secrecy::SecretBox::new(boxed))
}

async fn remove_file_if_present(path: &Path) -> Result<(), std::io::Error> {
    match tokio::fs::remove_file(path).await {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

async fn verify_blob_checksum(
    path: &Path,
    expected_checksum: &[u8; 32],
) -> Result<bool, std::io::Error> {
    let mut file = tokio::fs::File::open(path).await?;
    let mut hasher = blake3::Hasher::new();
    let mut buffer = [0u8; 8 * 1024];
    loop {
        let read = file.read(&mut buffer).await?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    let checksum = hasher.finalize();
    Ok(checksum.as_bytes() == expected_checksum)
}

pub(crate) async fn drain_pending_deletions(
    metadata_store: &SqlCipherMetadataStore,
    cloud_transport: &dyn CloudTransport,
) -> Result<usize, SyncError> {
    let mut completed = 0usize;
    loop {
        let pending = metadata_store
            .list_pending_deletions(PENDING_DELETIONS_BATCH_LIMIT)
            .await?;
        if pending.is_empty() {
            break;
        }

        let mut batch_completed = 0usize;
        for blob_name in pending {
            let remote_path = match build_blob_remote_path(&blob_name) {
                Ok(path) => path,
                Err(error) => {
                    tracing::warn!(
                        blob_name = %blob_name,
                        error = %error,
                        "failed to validate pending deletion blob name; leaving row queued"
                    );
                    continue;
                }
            };

            match cloud_transport.delete_blob(&remote_path).await {
                Ok(()) => {
                    metadata_store.mark_deletion_complete(&blob_name).await?;
                    completed += 1;
                    batch_completed += 1;
                }
                Err(CloudTransportError::NotFound) => {
                    // Blob was never uploaded to the cloud (e.g. deleted locally
                    // before the first sync). The desired state is already achieved.
                    tracing::debug!(
                        blob_name = %blob_name,
                        remote_path = %remote_path,
                        "blob not found in cloud during pending deletion drain; \
                         treating as already deleted"
                    );
                    metadata_store.mark_deletion_complete(&blob_name).await?;
                    completed += 1;
                    batch_completed += 1;
                }
                Err(error) => {
                    tracing::warn!(
                        blob_name = %blob_name,
                        remote_path = %remote_path,
                        error = %error,
                        "cloud delete failed for pending deletion; leaving row queued"
                    );
                }
            }
        }

        if batch_completed == 0 {
            tracing::warn!("stopping pending deletion drain after zero-progress batch");
            break;
        }
    }

    Ok(completed)
}

async fn restore_last_synced_at(
    metadata_store: &SqlCipherMetadataStore,
    previous_last_synced_raw: Option<String>,
) -> Result<(), StorageError> {
    match previous_last_synced_raw {
        Some(previous_value) => {
            metadata_store
                .set_meta("last_synced_at", &previous_value)
                .await
        }
        None => metadata_store.clear_last_synced_at().await,
    }
}

async fn read_required_u64_meta(
    metadata_store: &SqlCipherMetadataStore,
    key: &str,
) -> Result<u64, SyncError> {
    let raw = metadata_store
        .get_meta(key)
        .await?
        .ok_or_else(|| SyncError::Storage {
            source: StorageError::Database(format!("missing manifest_meta key: {key}")),
        })?;
    raw.parse::<u64>().map_err(|_| SyncError::Storage {
        source: StorageError::Database(format!("invalid {key}: not an unsigned integer")),
    })
}

async fn read_optional_i64_meta(
    metadata_store: &SqlCipherMetadataStore,
    key: &str,
) -> Result<Option<i64>, SyncError> {
    let raw = metadata_store.get_meta(key).await?;
    Ok(raw.and_then(|value| value.parse::<i64>().ok()))
}

fn now_unix_seconds() -> i64 {
    let duration = SystemTime::now().duration_since(UNIX_EPOCH).ok();
    duration
        .and_then(|duration| i64::try_from(duration.as_secs()).ok())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine as _;
    use rand::SeedableRng;
    use tempfile::tempdir;
    use uuid::Uuid;

    use crate::crypto::{ManifestKey, SqlcipherKey};
    use crate::storage::MetadataStore;
    use crate::storage::cloud::CloudTransport;
    use crate::storage::cloud::mock::{CloudTransportErrorKind, MockCloudTransport};
    use crate::storage::cloud::vault_header::{Argon2ParamsJson, VaultHeader};
    use crate::storage::types::{ChunkRecord, Node, NodeType};

    fn sample_header() -> VaultHeader {
        VaultHeader {
            vault_id: Uuid::new_v4().hyphenated().to_string(),
            schema_version: VaultHeader::SCHEMA_VERSION,
            tier: 1,
            argon2_salt: base64::engine::general_purpose::STANDARD.encode([0x11u8; 32]),
            argon2_params: Argon2ParamsJson {
                memory_cost: 65_536,
                time_cost: 3,
                parallelism: 4,
            },
            key_file_blake3: None,
            recovery_slots: Vec::new(),
            name: None,
        }
    }

    async fn setup_store(
        db_path: &Path,
        key_bytes: &[u8; 32],
    ) -> Result<SqlCipherMetadataStore, StorageError> {
        SqlCipherMetadataStore::create(db_path, key_bytes, Uuid::new_v4(), 4_194_304, false).await
    }

    async fn insert_single_chunk(
        store: &SqlCipherMetadataStore,
        blob_name: &str,
        payload: &[u8],
    ) -> Result<Uuid, StorageError> {
        let file_id = Uuid::new_v4();
        let node = Node::new(
            file_id,
            None,
            NodeType::File,
            "file.txt".to_owned(),
            1,
            1,
            payload.len() as u64,
            Some([9; 72]),
        );
        store.insert_node(&node).await?;
        let chunk = ChunkRecord {
            chunk_id: Uuid::new_v4(),
            node_id: file_id.into(),
            chunk_index: 0,
            blob_name: blob_name.to_owned(),
            size_padded: 4_194_304,
            blake3_checksum: compute_checksum(payload).0,
            epoch_blob_id: None,
            byte_offset: None,
            byte_length: None,
        };
        store.insert_chunks(&[chunk]).await?;
        Ok(file_id)
    }

    #[test]
    fn test_fisher_yates_shuffle_preserves_length_and_elements() {
        let mut values = vec![1, 2, 3, 4, 5];
        let mut rng = rand::rngs::StdRng::seed_from_u64(7);
        let before = values.clone();
        fisher_yates_shuffle(&mut values, &mut rng);
        assert_eq!(values.len(), before.len());
        let mut sorted_before = before;
        let mut sorted_after = values.clone();
        sorted_before.sort_unstable();
        sorted_after.sort_unstable();
        assert_eq!(sorted_before, sorted_after);
    }

    #[test]
    fn test_fisher_yates_shuffle_deterministic_with_seeded_rng() {
        let mut first = vec![1, 2, 3, 4, 5];
        let mut second = vec![1, 2, 3, 4, 5];
        let mut rng_a = rand::rngs::StdRng::seed_from_u64(99);
        let mut rng_b = rand::rngs::StdRng::seed_from_u64(99);
        fisher_yates_shuffle(&mut first, &mut rng_a);
        fisher_yates_shuffle(&mut second, &mut rng_b);
        assert_eq!(first, second);
    }

    #[tokio::test]
    async fn test_read_cloud_snapshot_state_returns_none_on_not_found() {
        let temp = tempdir().expect("tempdir should be created");
        let cloud = MockCloudTransport::new();
        let manifest_key = ManifestKey::from_bytes([2; 32]);
        let sqlcipher_key = SqlcipherKey::from_bytes([3; 32]);

        let result = read_cloud_snapshot_state(&cloud, temp.path(), &manifest_key, &sqlcipher_key)
            .await
            .expect("state lookup should succeed");
        assert!(result.is_none());
        assert!(!temp.path().join(CONFLICT_PROBE_DB_FILE_NAME).exists());
    }

    #[tokio::test]
    async fn test_read_cloud_snapshot_state_returns_state_on_present_manifest() {
        let temp = tempdir().expect("tempdir should be created");
        let db_path = temp.path().join("manifest.db");
        let key_bytes = [4u8; 32];
        let sqlcipher_key = SqlcipherKey::from_bytes(key_bytes);
        let manifest_key = ManifestKey::from_bytes([9u8; 32]);
        let cloud = MockCloudTransport::new();
        let store = setup_store(&db_path, &key_bytes)
            .await
            .expect("store should be created");
        let _ = store
            .increment_snapshot_counter()
            .await
            .expect("counter increment should succeed");
        store
            .set_meta("last_synced_at", "123")
            .await
            .expect("set_meta should succeed");
        upload_manifest_backup(
            &db_path,
            &sqlcipher_key,
            manifest_key.expose(),
            &cloud,
            temp.path(),
        )
        .await
        .expect("manifest upload should succeed");

        let state = read_cloud_snapshot_state(&cloud, temp.path(), &manifest_key, &sqlcipher_key)
            .await
            .expect("state lookup should succeed")
            .expect("manifest should exist");
        assert_eq!(state.snapshot_counter, 1);
        assert_eq!(state.last_synced_at, Some(123));
        assert!(!temp.path().join(CONFLICT_PROBE_DB_FILE_NAME).exists());
    }

    #[tokio::test]
    async fn test_read_cloud_snapshot_state_returns_unreadable_on_wrong_manifest_key() {
        let temp = tempdir().expect("tempdir should be created");
        let db_path = temp.path().join("manifest.db");
        let key_bytes = [4u8; 32];
        let sqlcipher_key = SqlcipherKey::from_bytes(key_bytes);
        let manifest_key = ManifestKey::from_bytes([9u8; 32]);
        let wrong_manifest_key = ManifestKey::from_bytes([10u8; 32]);
        let cloud = MockCloudTransport::new();
        let _store = setup_store(&db_path, &key_bytes)
            .await
            .expect("store should be created");
        upload_manifest_backup(
            &db_path,
            &sqlcipher_key,
            manifest_key.expose(),
            &cloud,
            temp.path(),
        )
        .await
        .expect("manifest upload should succeed");

        let result =
            read_cloud_snapshot_state(&cloud, temp.path(), &wrong_manifest_key, &sqlcipher_key)
                .await;
        assert!(matches!(
            result,
            Err(SyncError::CloudManifestUnreadable { .. })
        ));
        assert!(!temp.path().join(CONFLICT_PROBE_DB_FILE_NAME).exists());
    }

    #[tokio::test]
    async fn test_push_vault_first_push_with_no_cloud_manifest_skips_conflict_check_and_succeeds() {
        let temp = tempdir().expect("tempdir should be created");
        let db_path = temp.path().join("manifest.db");
        let key_bytes = [7u8; 32];
        let sqlcipher_key = SqlcipherKey::from_bytes(key_bytes);
        let manifest_key = ManifestKey::from_bytes([8u8; 32]);
        let cloud = MockCloudTransport::new();
        let store = setup_store(&db_path, &key_bytes)
            .await
            .expect("store should be created");
        let _uploaded_file_id = insert_single_chunk(
            &store,
            "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
            b"encrypted-payload",
        )
        .await
        .expect("chunk insert should succeed");
        let deleted_file_id = insert_single_chunk(
            &store,
            "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb",
            b"queued-delete",
        )
        .await
        .expect("delete candidate insert should succeed");
        store
            .delete_node(deleted_file_id)
            .await
            .expect("delete should enqueue pending deletion");
        tokio::fs::create_dir_all(temp.path().join("pending"))
            .await
            .expect("pending dir should be created");
        tokio::fs::write(
            pending_blob_path(temp.path(), "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa"),
            b"encrypted-payload",
        )
        .await
        .expect("staging blob should be written");

        let report = push_vault(
            &db_path,
            &sqlcipher_key,
            &manifest_key,
            &store,
            &cloud,
            &sample_header(),
            temp.path(),
            &SyncConfig::default(),
            None,
        )
        .await
        .expect("push should succeed");

        assert_eq!(report.blobs_uploaded, 1);
        assert_eq!(report.snapshot_counter_after, 1);
        assert!(
            store
                .list_pending_deletions(10)
                .await
                .expect("pending deletions should load")
                .is_empty()
        );
    }

    #[tokio::test]
    async fn test_push_vault_treats_stale_manifest_from_rekey_as_first_push_and_succeeds() {
        // After a phrase-recovery rekey with no cloud transport, the cloud still holds
        // a manifest backup encrypted with the old manifest key. push_vault must treat
        // this as "no valid cloud snapshot" rather than a hard error, so the push
        // proceeds and overwrites the stale blob.
        let temp = tempdir().expect("tempdir should be created");
        let db_path = temp.path().join("manifest.db");
        let old_key_bytes = [1u8; 32];
        let new_key_bytes = [7u8; 32];
        let old_manifest_key = ManifestKey::from_bytes([2u8; 32]);
        let new_manifest_key = ManifestKey::from_bytes([8u8; 32]);
        let old_sqlcipher_key = SqlcipherKey::from_bytes(old_key_bytes);
        let new_sqlcipher_key = SqlcipherKey::from_bytes(new_key_bytes);
        let cloud = MockCloudTransport::new();
        let staging_dir = temp.path();

        // Set up an "old" store and upload a manifest backup under the old key to
        // simulate what was on cloud before phrase recovery.
        let old_db_path = temp.path().join("old_manifest.db");
        let old_store = setup_store(&old_db_path, &old_key_bytes)
            .await
            .expect("old store should be created");
        let old_manifest_key_bytes = Zeroizing::new(old_manifest_key.with_exposed(|bytes| *bytes));
        upload_manifest_backup(
            &old_db_path,
            &old_sqlcipher_key,
            &old_manifest_key_bytes,
            &cloud,
            staging_dir,
        )
        .await
        .expect("old manifest upload should succeed");
        drop(old_store);

        // Set up the rekeyed store (new credentials, counter starts at 0).
        let store = setup_store(&db_path, &new_key_bytes)
            .await
            .expect("new store should be created");
        insert_single_chunk(
            &store,
            "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
            b"encrypted-payload",
        )
        .await
        .expect("chunk insert should succeed");
        tokio::fs::create_dir_all(staging_dir.join("pending"))
            .await
            .expect("pending dir should be created");
        tokio::fs::write(
            pending_blob_path(staging_dir, "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa"),
            b"encrypted-payload",
        )
        .await
        .expect("staging blob should be written");

        // push_vault must succeed despite the cloud manifest being encrypted with
        // the old key, treating it as a first push.
        let report = push_vault(
            &db_path,
            &new_sqlcipher_key,
            &new_manifest_key,
            &store,
            &cloud,
            &sample_header(),
            staging_dir,
            &SyncConfig::default(),
            None,
        )
        .await
        .expect("push should succeed despite stale cloud manifest");

        assert_eq!(report.blobs_uploaded, 1);
        assert_eq!(report.snapshot_counter_after, 1);
    }

    #[tokio::test]
    async fn test_push_vault_aborts_when_cloud_counter_differs_with_conflict_error() {
        let temp = tempdir().expect("tempdir should be created");
        let local_db = temp.path().join("local.db");
        let cloud_db = temp.path().join("cloud.db");
        let key_bytes = [11u8; 32];
        let sqlcipher_key = SqlcipherKey::from_bytes(key_bytes);
        let manifest_key = ManifestKey::from_bytes([12u8; 32]);
        let cloud = MockCloudTransport::new();
        let local_store = setup_store(&local_db, &key_bytes)
            .await
            .expect("local store should be created");
        let cloud_store = setup_store(&cloud_db, &key_bytes)
            .await
            .expect("cloud store should be created");
        let _ = cloud_store
            .increment_snapshot_counter()
            .await
            .expect("counter increment should succeed");
        upload_manifest_backup(
            &cloud_db,
            &sqlcipher_key,
            manifest_key.expose(),
            &cloud,
            temp.path(),
        )
        .await
        .expect("manifest upload should succeed");

        let result = push_vault(
            &local_db,
            &sqlcipher_key,
            &manifest_key,
            &local_store,
            &cloud,
            &sample_header(),
            temp.path(),
            &SyncConfig::default(),
            None,
        )
        .await;

        assert!(matches!(result, Err(SyncError::Conflict(_))));
    }

    #[tokio::test]
    async fn test_push_vault_rolls_back_snapshot_counter_on_manifest_upload_failure() {
        let temp = tempdir().expect("tempdir should be created");
        let db_path = temp.path().join("manifest.db");
        let key_bytes = [17u8; 32];
        let wrong_sqlcipher_key = SqlcipherKey::from_bytes([19u8; 32]);
        let manifest_key = ManifestKey::from_bytes([18u8; 32]);
        let cloud = MockCloudTransport::new();
        let store = setup_store(&db_path, &key_bytes)
            .await
            .expect("store should be created");

        let result = push_vault(
            &db_path,
            &wrong_sqlcipher_key,
            &manifest_key,
            &store,
            &cloud,
            &sample_header(),
            temp.path(),
            &SyncConfig::default(),
            None,
        )
        .await;

        assert!(matches!(
            result,
            Err(SyncError::PushManifestBackupFailed { .. })
        ));
        assert_eq!(
            store
                .get_meta("snapshot_counter")
                .await
                .expect("meta should load"),
            Some("0".to_owned())
        );
    }

    #[tokio::test]
    async fn test_drive_blob_downloads_records_verification_failure_and_deletes_file() {
        let temp = tempdir().expect("tempdir should be created");
        let cloud = MockCloudTransport::new();
        let blob_name = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa".to_owned();
        let remote_path = format!("vault/{blob_name}.blob");
        let upload_path = temp.path().join("upload.blob");
        tokio::fs::write(&upload_path, b"payload-a")
            .await
            .expect("upload path should write");
        cloud
            .upload_blob(&upload_path, &remote_path)
            .await
            .expect("mock upload should succeed");
        tokio::fs::create_dir_all(temp.path().join("cache"))
            .await
            .expect("cache dir should be created");

        let result = drive_blob_downloads(
            vec![SyncChunkRecord {
                blob_name: blob_name.clone(),
                blake3_checksum: [0xCD; 32],
            }],
            &cloud,
            temp.path(),
            1,
        )
        .await;

        assert!(matches!(result, Err((verification, _)) if verification == vec![blob_name]));
        assert!(!cache_blob_path(temp.path(), "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa").exists());
    }

    #[tokio::test]
    async fn test_delete_vault_from_cloud_partial_failure_records_failed_blobs() {
        let temp = tempdir().expect("tempdir should be created");
        let cloud = MockCloudTransport::new();
        let source = temp.path().join("blob.bin");
        tokio::fs::write(&source, b"x")
            .await
            .expect("source should be created");
        let ok_blob = "vault/aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa.blob";
        let fail_blob = "vault/bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb.blob";
        cloud
            .upload_blob(&source, ok_blob)
            .await
            .expect("upload should work");
        cloud
            .upload_blob(&source, fail_blob)
            .await
            .expect("upload should work");
        cloud
            .upload_blob(&source, MANIFEST_BACKUP_BLOB_NAME)
            .await
            .expect("manifest should upload");
        cloud
            .upload_blob(&source, VAULT_HEADER_BLOB_NAME)
            .await
            .expect("header should upload");
        cloud
            .inject_failure(fail_blob, CloudTransportErrorKind::Timeout)
            .await;

        let report = delete_vault_from_cloud(&cloud)
            .await
            .expect("delete should return report");
        assert_eq!(report.vault_blobs_deleted, 1);
        assert_eq!(report.vault_blobs_failed, vec![fail_blob.to_owned()]);
        assert!(report.manifest_backup_deleted);
        assert!(report.vault_header_deleted);
    }

    #[tokio::test]
    async fn test_pull_vault_pending_deletion_delete_failure_preserves_queue_for_retry() {
        let temp = tempdir().expect("tempdir should be created");
        let db_path = temp.path().join("manifest.db");
        let key_bytes = [21u8; 32];
        let store = setup_store(&db_path, &key_bytes)
            .await
            .expect("store should be created");
        let cloud = MockCloudTransport::new();
        let file_id = insert_single_chunk(
            &store,
            "cccccccc-cccc-4ccc-8ccc-cccccccccccc",
            b"pending-delete",
        )
        .await
        .expect("chunk insert should succeed");
        store
            .delete_node(file_id)
            .await
            .expect("delete should enqueue pending deletion");
        cloud
            .inject_failure(
                "vault/cccccccc-cccc-4ccc-8ccc-cccccccccccc.blob",
                CloudTransportErrorKind::Timeout,
            )
            .await;

        pull_vault(
            temp.path(),
            &SqlcipherKey::from_bytes([1; 32]),
            &ManifestKey::from_bytes([2; 32]),
            &store,
            &cloud,
            temp.path(),
            &SyncConfig::default(),
            None,
        )
        .await
        .expect("pull should succeed");

        assert_eq!(
            store
                .list_pending_deletions(10)
                .await
                .expect("pending deletions should load"),
            vec!["cccccccc-cccc-4ccc-8ccc-cccccccccccc".to_owned()]
        );
    }

    #[tokio::test]
    async fn test_pull_vault_pending_deletion_retries_eventually_clear_queue() {
        let temp = tempdir().expect("tempdir should be created");
        let db_path = temp.path().join("manifest.db");
        let key_bytes = [22u8; 32];
        let store = setup_store(&db_path, &key_bytes)
            .await
            .expect("store should be created");
        let cloud = MockCloudTransport::new();
        let file_id = insert_single_chunk(
            &store,
            "dddddddd-dddd-4ddd-8ddd-dddddddddddd",
            b"pending-delete",
        )
        .await
        .expect("chunk insert should succeed");
        store
            .delete_node(file_id)
            .await
            .expect("delete should enqueue pending deletion");
        cloud
            .inject_failure(
                "vault/dddddddd-dddd-4ddd-8ddd-dddddddddddd.blob",
                CloudTransportErrorKind::Timeout,
            )
            .await;

        pull_vault(
            temp.path(),
            &SqlcipherKey::from_bytes([1; 32]),
            &ManifestKey::from_bytes([2; 32]),
            &store,
            &cloud,
            temp.path(),
            &SyncConfig::default(),
            None,
        )
        .await
        .expect("first pull should succeed");
        assert_eq!(
            store
                .list_pending_deletions(10)
                .await
                .expect("pending deletions should load"),
            vec!["dddddddd-dddd-4ddd-8ddd-dddddddddddd".to_owned()]
        );

        pull_vault(
            temp.path(),
            &SqlcipherKey::from_bytes([1; 32]),
            &ManifestKey::from_bytes([2; 32]),
            &store,
            &cloud,
            temp.path(),
            &SyncConfig::default(),
            None,
        )
        .await
        .expect("second pull should succeed");
        assert!(
            store
                .list_pending_deletions(10)
                .await
                .expect("pending deletions should load")
                .is_empty()
        );
    }

    #[tokio::test]
    async fn test_pull_vault_pending_deletion_not_found_in_cloud_clears_queue() {
        // Scenario: file deleted locally before first sync. The blob was never
        // uploaded, so the cloud returns NotFound. drain_pending_deletions must
        // treat this as idempotent success and clear the row.
        let temp = tempdir().expect("tempdir should be created");
        let db_path = temp.path().join("manifest.db");
        let key_bytes = [23u8; 32];
        let store = setup_store(&db_path, &key_bytes)
            .await
            .expect("store should be created");
        let cloud = MockCloudTransport::new();
        let file_id = insert_single_chunk(
            &store,
            "eeeeeeee-eeee-4eee-8eee-eeeeeeeeeeee",
            b"never-uploaded",
        )
        .await
        .expect("chunk insert should succeed");
        store
            .delete_node(file_id)
            .await
            .expect("delete should enqueue pending deletion");
        cloud
            .inject_failure(
                "vault/eeeeeeee-eeee-4eee-8eee-eeeeeeeeeeee.blob",
                CloudTransportErrorKind::NotFound,
            )
            .await;

        pull_vault(
            temp.path(),
            &SqlcipherKey::from_bytes([1; 32]),
            &ManifestKey::from_bytes([2; 32]),
            &store,
            &cloud,
            temp.path(),
            &SyncConfig::default(),
            None,
        )
        .await
        .expect("pull should succeed");

        assert!(
            store
                .list_pending_deletions(10)
                .await
                .expect("pending deletions should load")
                .is_empty(),
            "NotFound from cloud should clear the pending deletion row"
        );
    }

    #[tokio::test]
    async fn test_pull_vault_skips_cloud_download_for_blob_present_in_pending_directory() {
        // Scenario: device B has a locally-staged blob in pending/ that was never uploaded
        // (conflict detected before push). After merge_from_probe_db both device B's and
        // device A's chunk records coexist in the DB. pull_vault must recognise device B's
        // own pending blob by falling back to pending_blob_path so it does not attempt a
        // cloud download that would fail (the blob has never been uploaded).
        let temp = tempdir().expect("tempdir should be created");
        let db_path = temp.path().join("manifest.db");
        let key_bytes = [30u8; 32];
        let store = setup_store(&db_path, &key_bytes)
            .await
            .expect("store should be created");
        let cloud = MockCloudTransport::new();
        let payload = b"locally-staged-blob";
        let blob_name = "ffffffff-ffff-4fff-8fff-ffffffffffff";
        let _file_id = insert_single_chunk(&store, blob_name, payload)
            .await
            .expect("chunk insert should succeed");

        tokio::fs::create_dir_all(temp.path().join("pending"))
            .await
            .expect("pending dir should be created");
        tokio::fs::write(pending_blob_path(temp.path(), blob_name), payload)
            .await
            .expect("pending blob should be written");

        let report = pull_vault(
            &db_path,
            &SqlcipherKey::from_bytes([1; 32]),
            &ManifestKey::from_bytes([2; 32]),
            &store,
            &cloud,
            temp.path(),
            &SyncConfig::default(),
            None,
        )
        .await
        .expect("pull should succeed without attempting cloud download for pending blob");

        assert_eq!(
            report.blobs_downloaded, 0,
            "pending blob should not be downloaded from cloud"
        );
        assert_eq!(
            report.blobs_skipped_present, 1,
            "pending blob should be counted as already present"
        );
    }
}
