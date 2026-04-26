//! Destination session persistence helpers backed by SQLCipher.

use std::path::Path;

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;
use tokio::io::AsyncWriteExt;
use zeroize::Zeroizing;

use crate::storage::cloud::CloudTransportError;
use crate::storage::error::StorageError;
use crate::storage::sqlcipher::{DestinationSessionRow, SqlCipherMetadataStore};
use crate::storage::staging;

/// Destination type persisted in `destination_sessions.destination_type`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum DestinationType {
    Cloud,
    ExternalDrive,
    LocalPath,
}

impl DestinationType {
    fn as_sql_tag(&self) -> &'static str {
        match self {
            Self::Cloud => "cloud",
            Self::ExternalDrive => "external_drive",
            Self::LocalPath => "local_path",
        }
    }

    #[cfg_attr(not(test), allow(dead_code))]
    fn from_sql_tag(tag: &str) -> Result<Self, StorageError> {
        match tag {
            "cloud" => Ok(Self::Cloud),
            "external_drive" => Ok(Self::ExternalDrive),
            "local_path" => Ok(Self::LocalPath),
            _ => Err(StorageError::Database(format!(
                "invalid destination_type tag: {tag}"
            ))),
        }
    }
}

/// Backup policy persisted in `destination_sessions.backup_mode`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum BackupSyncMode {
    Mirror,
    Accumulating,
}

impl BackupSyncMode {
    fn as_sql_tag(&self) -> &'static str {
        match self {
            Self::Mirror => "mirror",
            Self::Accumulating => "accumulating",
        }
    }

    #[cfg_attr(not(test), allow(dead_code))]
    fn from_optional_sql_tag(tag: Option<String>) -> Result<Option<Self>, StorageError> {
        match tag.as_deref() {
            Some("mirror") => Ok(Some(Self::Mirror)),
            Some("accumulating") => Ok(Some(Self::Accumulating)),
            Some(other) => Err(StorageError::Database(format!(
                "invalid backup_mode tag: {other}"
            ))),
            None => Ok(None),
        }
    }
}

/// Session-scoped destination configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DestinationSession {
    pub destination_id: String,
    pub label: String,
    pub destination_type: DestinationType,
    pub rclone_remote_name: String,
    pub rclone_config_blob: String,
    pub bucket: String,
    pub path_prefix: String,
    pub is_primary: bool,
    pub backup_mode: Option<BackupSyncMode>,
}

/// Public destination session shape that excludes credential-bearing configuration blobs.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DestinationSessionPublic {
    pub destination_id: String,
    pub label: String,
    pub destination_type: DestinationType,
    pub rclone_remote_name: String,
    pub bucket: String,
    pub path_prefix: String,
    pub is_primary: bool,
    pub backup_mode: Option<BackupSyncMode>,
}

impl From<&DestinationSession> for DestinationSessionPublic {
    fn from(value: &DestinationSession) -> Self {
        Self {
            destination_id: value.destination_id.clone(),
            label: value.label.clone(),
            destination_type: value.destination_type.clone(),
            rclone_remote_name: value.rclone_remote_name.clone(),
            bucket: value.bucket.clone(),
            path_prefix: value.path_prefix.clone(),
            is_primary: value.is_primary,
            backup_mode: value.backup_mode.clone(),
        }
    }
}

/// Inserts one destination session row.
pub async fn insert_destination_session(
    store: &SqlCipherMetadataStore,
    session: &DestinationSession,
) -> Result<(), StorageError> {
    let mut session = session.clone();
    if session.destination_type == DestinationType::Cloud {
        session.rclone_config_blob = validate_single_remote_stanza(
            &session.rclone_config_blob,
            &session.rclone_remote_name,
        )?;
    }
    store
        .insert_destination_session(
            session.destination_id,
            session.label,
            session.destination_type.as_sql_tag().to_owned(),
            session.rclone_remote_name,
            session.rclone_config_blob,
            session.bucket,
            session.path_prefix,
            session.is_primary,
            session
                .backup_mode
                .as_ref()
                .map(BackupSyncMode::as_sql_tag)
                .map(str::to_owned),
        )
        .await
}

/// Lists all destination sessions in created-at order.
#[cfg_attr(not(test), allow(dead_code))]
pub async fn list_destination_sessions(
    store: &SqlCipherMetadataStore,
) -> Result<Vec<DestinationSession>, StorageError> {
    let rows = store.list_destination_sessions().await?;
    rows.into_iter().map(destination_session_from_row).collect()
}

/// Returns the primary destination session when present.
#[cfg_attr(not(test), allow(dead_code))]
pub async fn get_primary_destination(
    store: &SqlCipherMetadataStore,
) -> Result<Option<DestinationSession>, StorageError> {
    store
        .get_primary_destination()
        .await?
        .map(destination_session_from_row)
        .transpose()
}

/// Deletes a destination session by ID.
pub async fn delete_destination_session(
    store: &SqlCipherMetadataStore,
    destination_id: &str,
) -> Result<(), StorageError> {
    store
        .delete_destination_session(destination_id.to_owned())
        .await
}

/// Builds a session-lived `rclone.conf` from all stored destination sessions.
#[cfg_attr(not(test), allow(dead_code))]
pub async fn build_session_rclone_conf(
    store: &SqlCipherMetadataStore,
    output_path: &Path,
) -> Result<(), CloudTransportError> {
    if let Some(parent) = output_path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let sessions = list_destination_sessions(store)
        .await
        .map_err(map_storage_error)?;
    let mut concatenated = Zeroizing::new(String::new());
    for session in sessions.iter() {
        if session.destination_type != DestinationType::Cloud {
            continue;
        }
        if !concatenated.is_empty() && !concatenated.ends_with('\n') {
            concatenated.push('\n');
        }
        let validated_blob =
            validate_single_remote_stanza(&session.rclone_config_blob, &session.rclone_remote_name)
                .map_err(map_storage_error)?;
        let validated_blob = Zeroizing::new(validated_blob);
        concatenated.push_str(&validated_blob);
        if !concatenated.ends_with('\n') {
            concatenated.push('\n');
        }
    }

    staging::write_owner_only(output_path, concatenated.as_bytes())
        .await
        .map_err(map_storage_error)?;
    Ok(())
}

/// Overwrites and removes a session-lived `rclone.conf`.
pub async fn destroy_session_rclone_conf(path: &Path) -> Result<(), CloudTransportError> {
    let path_buf = path.to_path_buf();
    let metadata = match tokio::fs::metadata(&path_buf).await {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(CloudTransportError::IoError(error)),
    };

    let size = metadata.len() as usize;
    let mut file = tokio::fs::OpenOptions::new()
        .write(true)
        .open(&path_buf)
        .await?;
    if size > 0 {
        let zeros = vec![0u8; size];
        file.write_all(&zeros).await?;
    }
    file.flush().await?;
    file.sync_all().await?;
    drop(file);

    match tokio::fs::remove_file(&path_buf).await {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(CloudTransportError::IoError(error)),
    }
}

#[cfg_attr(not(test), allow(dead_code))]
fn map_storage_error(error: StorageError) -> CloudTransportError {
    CloudTransportError::Other(error.to_string())
}

fn destination_session_from_row(
    row: DestinationSessionRow,
) -> Result<DestinationSession, StorageError> {
    let destination_type = DestinationType::from_sql_tag(&row.destination_type)?;
    let backup_mode = BackupSyncMode::from_optional_sql_tag(row.backup_mode)?;
    Ok(DestinationSession {
        destination_id: row.destination_id,
        label: row.label,
        destination_type,
        rclone_remote_name: row.rclone_remote_name,
        rclone_config_blob: row.rclone_config_blob,
        bucket: row.bucket,
        path_prefix: row.path_prefix,
        is_primary: row.is_primary,
        backup_mode,
    })
}

fn stanza_header_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(r"(?m)^\s*\[([^\]\r\n]+)\]\s*$").expect("stanza-header regex should compile")
    })
}

fn validate_single_remote_stanza(
    rclone_config_blob: &str,
    expected_remote_name: &str,
) -> Result<String, StorageError> {
    if rclone_config_blob.trim().is_empty() {
        return Err(StorageError::ConstraintViolation(
            "rclone_config_blob must contain exactly one remote stanza".to_owned(),
        ));
    }

    let mut headers = stanza_header_regex().captures_iter(rclone_config_blob);
    let first = headers.next().ok_or_else(|| {
        StorageError::ConstraintViolation(
            "rclone_config_blob must contain exactly one remote stanza header".to_owned(),
        )
    })?;

    if headers.next().is_some() {
        return Err(StorageError::ConstraintViolation(
            "rclone_config_blob must not contain multiple remote stanzas".to_owned(),
        ));
    }

    let header_name = first
        .get(1)
        .map(|capture| capture.as_str().trim())
        .unwrap_or_default();
    if header_name != expected_remote_name {
        return Err(StorageError::ConstraintViolation(format!(
            "rclone_config_blob stanza header '{header_name}' must match rclone_remote_name '{expected_remote_name}'"
        )));
    }

    let mut normalised = rclone_config_blob.trim_end().to_owned();
    normalised.push('\n');
    Ok(normalised)
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;
    use uuid::Uuid;

    use super::{
        BackupSyncMode, DestinationSession, DestinationType, build_session_rclone_conf,
        delete_destination_session, destroy_session_rclone_conf, get_primary_destination,
        insert_destination_session, list_destination_sessions,
    };
    use crate::storage::cloud::CloudTransportError;
    use crate::storage::error::StorageError;
    use crate::storage::sqlcipher::SqlCipherMetadataStore;

    fn sample_session(id: &str, is_primary: bool) -> DestinationSession {
        DestinationSession {
            destination_id: id.to_owned(),
            label: format!("dest-{id}"),
            destination_type: DestinationType::Cloud,
            rclone_remote_name: format!("remote-{id}"),
            rclone_config_blob: format!("[remote-{id}]\ntype = s3\n"),
            bucket: "bucket".to_owned(),
            path_prefix: "vault".to_owned(),
            is_primary,
            backup_mode: Some(BackupSyncMode::Mirror),
        }
    }

    async fn create_store() -> (
        tempfile::TempDir,
        std::path::PathBuf,
        SqlCipherMetadataStore,
    ) {
        let directory = tempdir().expect("tempdir should be created");
        let db_path = directory.path().join("manifest.db");
        let store =
            SqlCipherMetadataStore::create(&db_path, &[9u8; 32], Uuid::new_v4(), 4_194_304, false)
                .await
                .expect("store should be created");
        (directory, db_path, store)
    }

    #[tokio::test]
    async fn test_destination_session_insert_and_list_round_trip() {
        let (_directory, _db_path, store) = create_store().await;
        insert_destination_session(&store, &sample_session("one", true))
            .await
            .expect("insert should succeed");

        let sessions = list_destination_sessions(&store)
            .await
            .expect("list should succeed");
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].destination_id, "one");
        assert_eq!(sessions[0].backup_mode, Some(BackupSyncMode::Mirror));
    }

    #[tokio::test]
    async fn test_destination_session_type_and_backup_mode_tags_round_trip() {
        let (_directory, _db_path, store) = create_store().await;
        let mut session = sample_session("two", false);
        session.destination_type = DestinationType::ExternalDrive;
        session.backup_mode = Some(BackupSyncMode::Accumulating);
        insert_destination_session(&store, &session)
            .await
            .expect("insert should succeed");

        let sessions = list_destination_sessions(&store).await.unwrap();
        assert_eq!(sessions[0].destination_type, DestinationType::ExternalDrive);
        assert_eq!(sessions[0].backup_mode, Some(BackupSyncMode::Accumulating));
    }

    #[tokio::test]
    async fn test_destination_session_duplicate_primary_is_rejected() {
        let (_directory, _db_path, store) = create_store().await;
        insert_destination_session(&store, &sample_session("one", true))
            .await
            .expect("first insert should succeed");

        let result = insert_destination_session(&store, &sample_session("two", true)).await;
        assert!(matches!(result, Err(StorageError::ConstraintViolation(_))));
    }

    #[tokio::test]
    async fn test_destination_session_get_primary_returns_expected_row() {
        let (_directory, _db_path, store) = create_store().await;
        insert_destination_session(&store, &sample_session("one", true))
            .await
            .expect("insert should succeed");

        let primary = get_primary_destination(&store)
            .await
            .expect("get should succeed")
            .expect("primary should exist");
        assert_eq!(primary.destination_id, "one");
        assert_eq!(primary.destination_type, DestinationType::Cloud);
    }

    #[tokio::test]
    async fn test_destination_session_delete_is_idempotent() {
        let (_directory, _db_path, store) = create_store().await;
        delete_destination_session(&store, "missing")
            .await
            .expect("delete should be idempotent");
    }

    #[tokio::test]
    async fn test_destination_session_sqlcipher_encryption_blocks_plain_sqlite_reads() {
        let (_directory, db_path, store) = create_store().await;
        insert_destination_session(&store, &sample_session("one", true))
            .await
            .expect("insert should succeed");

        let raw_conn = rusqlite::Connection::open(db_path).expect("raw sqlite open should succeed");
        let result: Result<String, rusqlite::Error> = raw_conn.query_row(
            "SELECT rclone_config_blob FROM destination_sessions LIMIT 1",
            [],
            |row| row.get(0),
        );
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_session_rclone_conf_build_and_destroy_round_trip() {
        let (directory, _db_path, store) = create_store().await;
        insert_destination_session(&store, &sample_session("one", true))
            .await
            .expect("insert should succeed");

        let config_path = directory.path().join("rclone-session.conf");
        build_session_rclone_conf(&store, &config_path)
            .await
            .expect("build should succeed");
        assert!(config_path.exists());

        destroy_session_rclone_conf(&config_path)
            .await
            .expect("destroy should succeed");
        assert!(!config_path.exists());
    }

    #[tokio::test]
    async fn test_destination_session_insert_rejects_multiple_stanzas() {
        let (_directory, _db_path, store) = create_store().await;
        let mut session = sample_session("one", true);
        session.rclone_config_blob = "[remote-one]\ntype = s3\n[other]\ntype = s3\n".to_owned();

        let result = insert_destination_session(&store, &session).await;
        assert!(matches!(result, Err(StorageError::ConstraintViolation(_))));
    }

    #[tokio::test]
    async fn test_destination_session_insert_rejects_header_mismatch() {
        let (_directory, _db_path, store) = create_store().await;
        let mut session = sample_session("one", true);
        session.rclone_config_blob = "[wrong]\ntype = s3\n".to_owned();

        let result = insert_destination_session(&store, &session).await;
        assert!(matches!(result, Err(StorageError::ConstraintViolation(_))));
    }

    #[tokio::test]
    async fn test_build_session_rclone_conf_rejects_malformed_persisted_blob() {
        let (directory, _db_path, store) = create_store().await;
        store
            .insert_destination_session(
                "dest-malformed".to_owned(),
                "malformed".to_owned(),
                "cloud".to_owned(),
                "remote-malformed".to_owned(),
                "[wrong]\ntype = s3\n".to_owned(),
                "bucket".to_owned(),
                "vault".to_owned(),
                false,
                None,
            )
            .await
            .expect("manual insert should succeed");

        let output = directory.path().join("session.conf");
        let result = build_session_rclone_conf(&store, &output).await;
        assert!(matches!(result, Err(CloudTransportError::Other(_))));
    }
}
