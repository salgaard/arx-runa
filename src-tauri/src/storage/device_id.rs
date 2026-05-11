//! Device identity: a UUID v4 stored locally, never synced to cloud.
//!
//! Used to scope `LocalPath` and `ExternalDrive` backup destinations to the
//! device that created them, preventing cross-device sync errors when the
//! shared manifest is restored on a different machine.

use uuid::Uuid;

use crate::storage::error::StorageError;

/// Returns the persistent device UUID, creating it on first call.
///
/// The ID is stored at `<data_dir>/arx-runa/device_id` as a plain UTF-8
/// UUID v4 string.  It is never uploaded to cloud storage.
pub async fn get_or_create_device_id() -> Result<String, StorageError> {
    let path = dirs::data_dir()
        .ok_or_else(|| StorageError::Io("data_dir not available".to_owned()))?
        .join("arx-runa")
        .join("device_id");

    if let Ok(existing) = tokio::fs::read_to_string(&path).await {
        let trimmed = existing.trim().to_owned();
        if !trimmed.is_empty() {
            return Ok(trimmed);
        }
    }

    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| StorageError::Io(e.to_string()))?;
    }

    let id = Uuid::new_v4().hyphenated().to_string();
    tokio::fs::write(&path, &id)
        .await
        .map_err(|e| StorageError::Io(e.to_string()))?;

    Ok(id)
}
