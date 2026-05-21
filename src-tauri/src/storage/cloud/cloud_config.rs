//! Primary cloud endpoint persistence (`cloud-config.json`).
//!
//! Anchored to `design.md#connection-descriptor`.

use std::path::{Path, PathBuf};

use crate::storage::cloud::{CloudEndpoint, CloudTransportError};
use crate::storage::staging;
use uuid::Uuid;

fn default_cloud_config_path() -> Result<PathBuf, CloudTransportError> {
    let data_dir = dirs::data_dir()
        .ok_or_else(|| CloudTransportError::Other("data directory unavailable".to_owned()))?;
    Ok(data_dir.join("arx-runa").join("cloud-config.json"))
}

#[cfg_attr(not(test), allow(dead_code))]
async fn load_primary_cloud_endpoint_from(
    path: &Path,
) -> Result<Option<CloudEndpoint>, CloudTransportError> {
    let bytes = match tokio::fs::read(path).await {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(CloudTransportError::IoError(error)),
    };

    let endpoint = serde_json::from_slice::<CloudEndpoint>(&bytes).map_err(|error| {
        CloudTransportError::Other(format!("invalid cloud-config.json: {error}"))
    })?;
    endpoint.validate()?;
    Ok(Some(endpoint))
}

async fn save_primary_cloud_endpoint_to(
    endpoint: &CloudEndpoint,
    target_path: &Path,
) -> Result<(), CloudTransportError> {
    if let Some(parent) = target_path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    endpoint.validate()?;

    let payload = serde_json::to_vec_pretty(endpoint)
        .map_err(|error| CloudTransportError::Other(error.to_string()))?;
    let temp_path = unique_sibling_path(target_path, "tmp");
    staging::write_owner_only(&temp_path, &payload)
        .await
        .map_err(|error| CloudTransportError::Other(error.to_string()))?;
    replace_file_cross_platform(&temp_path, target_path).await?;
    Ok(())
}

async fn replace_file_cross_platform(
    temp_path: &Path,
    target_path: &Path,
) -> Result<(), CloudTransportError> {
    let temp_path = temp_path.to_path_buf();
    let target_path = target_path.to_path_buf();
    tokio::task::spawn_blocking(move || -> Result<(), CloudTransportError> {
        let backup_path = unique_sibling_path(&target_path, "bak");
        let mut moved_target_to_backup = false;
        if target_path.exists() {
            std::fs::rename(&target_path, &backup_path)?;
            moved_target_to_backup = true;
        }

        if let Err(error) = std::fs::rename(&temp_path, &target_path) {
            if moved_target_to_backup
                && let Err(restore_error) = std::fs::rename(&backup_path, &target_path)
            {
                return Err(CloudTransportError::Other(format!(
                    "failed to replace cloud config and rollback failed: replace={error}; rollback={restore_error}"
                )));
            }
            let _ = std::fs::remove_file(&temp_path);
            return Err(CloudTransportError::IoError(error));
        }

        if moved_target_to_backup {
            let _ = std::fs::remove_file(&backup_path);
        }
        let _ = std::fs::remove_file(&temp_path);

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&target_path, std::fs::Permissions::from_mode(0o600))?;
        }
        Ok(())
    })
    .await
    .map_err(|error| CloudTransportError::Other(error.to_string()))?
}

fn unique_sibling_path(target_path: &Path, suffix: &str) -> PathBuf {
    let file_name = target_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("cloud-config.json");
    let unique = Uuid::new_v4().as_hyphenated().to_string();
    target_path.with_file_name(format!("{file_name}.{suffix}.{unique}"))
}

fn legacy_cloud_config_path() -> Option<PathBuf> {
    dirs::config_dir().map(|path| path.join("arx-runa").join("cloud-config.json"))
}

/// Loads the primary cloud endpoint from `cloud-config.json`, migrating from
/// the legacy config location if needed.
pub async fn load_primary_cloud_endpoint() -> Result<Option<CloudEndpoint>, CloudTransportError> {
    let canonical_path = default_cloud_config_path()?;
    if let Some(endpoint) = load_primary_cloud_endpoint_from(&canonical_path).await? {
        return Ok(Some(endpoint));
    }

    let Some(legacy_path) = legacy_cloud_config_path() else {
        return Ok(None);
    };
    if legacy_path == canonical_path {
        return Ok(None);
    }

    let Some(endpoint) = load_primary_cloud_endpoint_from(&legacy_path).await? else {
        return Ok(None);
    };

    save_primary_cloud_endpoint_to(&endpoint, &canonical_path).await?;
    let _ = tokio::fs::remove_file(&legacy_path).await;
    Ok(Some(endpoint))
}

/// Saves the primary endpoint to `cloud-config.json`.
pub async fn save_primary_cloud_endpoint(
    endpoint: &CloudEndpoint,
) -> Result<(), CloudTransportError> {
    let target_path = default_cloud_config_path()?;
    save_primary_cloud_endpoint_to(endpoint, &target_path).await
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use tempfile::tempdir;

    use super::{
        load_primary_cloud_endpoint_from, replace_file_cross_platform,
        save_primary_cloud_endpoint_to,
    };
    use crate::storage::cloud::{CloudEndpoint, CloudTransportError};

    fn test_endpoint() -> CloudEndpoint {
        CloudEndpoint {
            provider: "s3".to_owned(),
            bucket: "bucket".to_owned(),
            region: "us-east-1".to_owned(),
            endpoint: "https://s3.example.com".to_owned(),
            path_prefix: "vault".to_owned(),
        }
    }

    fn cloud_config_path_under(base: &std::path::Path) -> PathBuf {
        base.join("arx-runa").join("cloud-config.json")
    }

    #[tokio::test]
    async fn test_cloud_config_round_trip_save_then_load() {
        let directory = tempdir().expect("tempdir should be created");
        let endpoint = test_endpoint();
        let path = cloud_config_path_under(directory.path());
        save_primary_cloud_endpoint_to(&endpoint, &path)
            .await
            .expect("save should succeed");

        let loaded = load_primary_cloud_endpoint_from(&path)
            .await
            .expect("load should succeed")
            .expect("endpoint should exist");
        assert_eq!(loaded, endpoint);
    }

    #[tokio::test]
    async fn test_cloud_config_missing_file_returns_none() {
        let directory = tempdir().expect("tempdir should be created");
        let path = cloud_config_path_under(directory.path());
        let loaded = load_primary_cloud_endpoint_from(&path)
            .await
            .expect("load should succeed");
        assert!(loaded.is_none());
    }

    #[tokio::test]
    async fn test_cloud_config_corrupt_json_returns_other_error() {
        let directory = tempdir().expect("tempdir should be created");
        let path = cloud_config_path_under(directory.path());
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"{not-json").unwrap();

        let result = load_primary_cloud_endpoint_from(&path).await;
        assert!(matches!(result, Err(CloudTransportError::Other(_))));
    }

    #[tokio::test]
    async fn test_cloud_config_rejects_endpoint_with_userinfo_on_load() {
        let directory = tempdir().expect("tempdir should be created");
        let path = cloud_config_path_under(directory.path());
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            &path,
            br#"{"provider":"s3","bucket":"b","region":"r","endpoint":"https://u:p@example.com","path_prefix":"vault"}"#,
        )
        .unwrap();
        let result = load_primary_cloud_endpoint_from(&path).await;
        assert!(matches!(result, Err(CloudTransportError::Other(_))));
    }

    #[tokio::test]
    async fn test_cloud_config_save_rejects_endpoint_with_query() {
        let directory = tempdir().expect("tempdir should be created");
        let path = cloud_config_path_under(directory.path());
        let mut endpoint = test_endpoint();
        endpoint.endpoint = "https://s3.example.com?sig=secret".to_owned();
        let result = save_primary_cloud_endpoint_to(&endpoint, &path).await;
        assert!(matches!(result, Err(CloudTransportError::Other(_))));
    }

    #[test]
    fn test_cloud_config_json_contains_only_non_sensitive_endpoint_keys() {
        let value = serde_json::to_value(test_endpoint()).expect("serialise should succeed");
        let object = value
            .as_object()
            .expect("endpoint should serialise to object");
        let mut keys: Vec<&str> = object.keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            vec!["bucket", "endpoint", "path_prefix", "provider", "region"]
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn test_cloud_config_save_sets_owner_only_permissions_on_unix() {
        use std::os::unix::fs::PermissionsExt;

        let directory = tempdir().expect("tempdir should be created");
        let path = cloud_config_path_under(directory.path());
        save_primary_cloud_endpoint_to(&test_endpoint(), &path)
            .await
            .expect("save should succeed");
        let metadata = std::fs::metadata(path).expect("metadata should be readable");
        assert_eq!(metadata.permissions().mode() & 0o777, 0o600);
    }

    #[tokio::test]
    async fn test_replace_file_cross_platform_overwrites_existing_target() {
        let directory = tempdir().expect("tempdir should be created");
        let temp = directory.path().join("temp.json.tmp");
        let target = directory.path().join("cloud-config.json");
        std::fs::write(&temp, b"new").unwrap();
        std::fs::write(&target, b"old").unwrap();

        replace_file_cross_platform(&temp, &target)
            .await
            .expect("replace should succeed");

        assert_eq!(std::fs::read(&target).unwrap(), b"new");
        assert!(!temp.exists());
    }

    #[tokio::test]
    async fn test_replace_file_cross_platform_rolls_back_when_temp_missing() {
        let directory = tempdir().expect("tempdir should be created");
        let temp = directory.path().join("missing-temp.json.tmp");
        let target = directory.path().join("cloud-config.json");
        std::fs::write(&target, b"old").unwrap();

        let result = replace_file_cross_platform(&temp, &target).await;
        assert!(matches!(result, Err(CloudTransportError::IoError(_))));
        assert_eq!(std::fs::read(&target).unwrap(), b"old");

        let entries: Vec<String> = std::fs::read_dir(directory.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert!(!entries.iter().any(|name| name.contains(".bak.")));
    }
}
