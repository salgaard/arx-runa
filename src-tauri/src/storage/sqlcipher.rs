//! SQLCipher-backed `MetadataStore` implementation.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use rusqlite::{Connection, OpenFlags, OptionalExtension, params};
use secrecy::SecretBox;
use thiserror::Error;
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::auth::types::user::{AuthUser, AuthUserStore};
use crate::crypto::SqlcipherKey;
use crate::storage::error::StorageError;
use crate::storage::metadata_store::MetadataStore;
use crate::storage::schema::{
    apply_canonical_schema, apply_epoch_v2_migration, seed_manifest_meta, validate_manifest_meta,
    validate_schema_integrity, verify_sqlcipher_key,
};
use crate::storage::types::{ChunkRecord, EpochBlobRecord, EpochBufferEntry, Node, NodeId, NodeType, SyncChunkRecord};
use crate::storage::validation::{
    immutable_meta_key_violation, is_immutable_manifest_meta_key, parse_chunk_size_bytes,
    validate_blob_name_uuid_v4, validate_chunk_target_node,
    validate_immutable_meta_matches_expected, validate_size_padded_matches_chunk_size,
};

/// Errors produced while opening and keying a SQLCipher connection.
#[derive(Debug, Error)]
pub enum SqlcipherOpenError {
    /// Opening the SQLite database file failed.
    #[error("failed to open SQLCipher database: {0}")]
    Open(String),
    /// SQLCipher rejected the provided key bytes.
    #[error("SQLCipher key rejected")]
    KeyRejected,
}

/// Production metadata store backed by a SQLCipher manifest database.
#[derive(Debug)]
pub struct SqlCipherMetadataStore {
    /// Shared connection guarded for serialized SQL access.
    conn: Arc<Mutex<Connection>>,
}

/// Raw destination-session row shape returned by SQLCipher helper methods.
#[derive(Debug, Clone)]
pub(crate) struct DestinationSessionRow {
    pub destination_id: String,
    pub label: String,
    pub destination_type: String,
    pub rclone_remote_name: String,
    pub rclone_config_blob: String,
    pub bucket: String,
    pub path_prefix: String,
    pub is_primary: bool,
    pub backup_mode: Option<String>,
}

impl SqlCipherMetadataStore {
    /// Opens an existing SQLCipher metadata store.
    pub async fn open(path: &Path, sqlcipher_key: &[u8; 32]) -> Result<Self, StorageError> {
        let path = path.to_path_buf();
        let sqlcipher_key = protected_sqlcipher_key_from_slice(sqlcipher_key);
        let conn = tokio::task::spawn_blocking(move || -> Result<Connection, StorageError> {
            let conn = open_keyed_connection(&path, &sqlcipher_key)?;
            apply_epoch_v2_migration(&conn)?;
            validate_schema_integrity(&conn)?;
            validate_manifest_meta(&conn)?;
            Ok(conn)
        })
        .await
        .map_err(|error| StorageError::Database(error.to_string()))??;

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Creates or opens a SQLCipher metadata store and seeds required metadata.
    pub async fn create(
        path: &Path,
        sqlcipher_key: &[u8; 32],
        vault_id: Uuid,
        chunk_size_bytes: u64,
        epoch_buffer_enabled: bool,
    ) -> Result<Self, StorageError> {
        let path = path.to_path_buf();
        let sqlcipher_key = protected_sqlcipher_key_from_slice(sqlcipher_key);
        let conn = tokio::task::spawn_blocking(move || -> Result<Connection, StorageError> {
            let conn = open_keyed_connection(&path, &sqlcipher_key)?;
            apply_canonical_schema(&conn)?;
            seed_manifest_meta(&conn, vault_id, chunk_size_bytes, epoch_buffer_enabled)?;
            apply_epoch_v2_migration(&conn)?;
            validate_schema_integrity(&conn)?;
            validate_manifest_meta(&conn)?;
            validate_create_immutable_meta_matches(
                &conn,
                vault_id,
                chunk_size_bytes,
                epoch_buffer_enabled,
            )?;
            Ok(conn)
        })
        .await
        .map_err(|error| StorageError::Database(error.to_string()))??;

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Enumerates every `chunks.blob_name` currently stored in the manifest.
    ///
    /// Intentionally SQLCipher-specific; this method is not exposed on
    /// [`MetadataStore`].
    pub(crate) async fn list_all_blob_names(&self) -> Result<HashSet<String>, StorageError> {
        self.with_connection_blocking(move |conn| {
            let mut statement = conn
                .prepare("SELECT blob_name FROM chunks WHERE blob_name IS NOT NULL
                UNION
                SELECT blob_name FROM epoch_blobs")
                .map_err(StorageError::from_rusqlite)?;
            let rows = statement
                .query_map([], |row| row.get::<_, String>(0))
                .map_err(StorageError::from_rusqlite)?;
            let mut names = HashSet::new();
            for row in rows {
                names.insert(row.map_err(StorageError::from_rusqlite)?);
            }
            Ok(names)
        })
        .await
    }

    /// Lists chunk records used by cloud push/pull synchronisation.
    ///
    /// Intentionally SQLCipher-specific; this method is not exposed on
    /// [`MetadataStore`].
    pub(crate) async fn list_sync_chunks(&self) -> Result<Vec<SyncChunkRecord>, StorageError> {
        self.with_connection_blocking(move |conn| {
            let mut statement = conn
                .prepare("SELECT blob_name, blake3_checksum FROM chunks WHERE blob_name IS NOT NULL
                UNION
                SELECT blob_name, blake3_checksum FROM epoch_blobs
                ORDER BY blob_name ASC")
                .map_err(StorageError::from_rusqlite)?;
            let rows = statement
                .query_map([], |row| {
                    let blob_name: String = row.get(0)?;
                    let checksum_bytes: Vec<u8> = row.get(1)?;
                    let blake3_checksum: [u8; 32] = checksum_bytes.try_into().map_err(|_| {
                        rusqlite::Error::FromSqlConversionFailure(
                            1,
                            rusqlite::types::Type::Blob,
                            Box::new(std::io::Error::new(
                                std::io::ErrorKind::InvalidData,
                                "expected 32-byte checksum",
                            )),
                        )
                    })?;
                    Ok(SyncChunkRecord {
                        blob_name,
                        blake3_checksum,
                    })
                })
                .map_err(StorageError::from_rusqlite)?;
            let mut records = Vec::new();
            for row in rows {
                records.push(row.map_err(StorageError::from_rusqlite)?);
            }
            Ok(records)
        })
        .await
    }

    /// Rolls back `snapshot_counter` to `previous_value` when and only when the
    /// currently stored value is `previous_value + 1`.
    ///
    /// Intentionally SQLCipher-specific; this method is not exposed on
    /// [`MetadataStore`].
    pub(crate) async fn rollback_snapshot_counter(
        &self,
        previous_value: u64,
    ) -> Result<(), StorageError> {
        self.with_connection_blocking(move |conn| {
            let tx = conn.transaction().map_err(StorageError::from_rusqlite)?;
            let current_value = tx
                .query_row(
                    "SELECT value FROM manifest_meta WHERE key = 'snapshot_counter'",
                    [],
                    |row| row.get::<_, String>(0),
                )
                .optional()
                .map_err(StorageError::from_rusqlite)?
                .ok_or(StorageError::NotFound)?;
            let current_value = current_value.parse::<u64>().map_err(|_| {
                StorageError::Database(
                    "invalid snapshot_counter: not an unsigned integer".to_owned(),
                )
            })?;
            let expected_current = previous_value.checked_add(1).ok_or_else(|| {
                StorageError::Database("invalid snapshot_counter: overflow".to_owned())
            })?;
            if current_value != expected_current {
                return Err(StorageError::Database(
                    "snapshot_counter rollback precondition violated".to_owned(),
                ));
            }
            tx.execute(
                "UPDATE manifest_meta SET value = ?1 WHERE key = 'snapshot_counter'",
                params![previous_value.to_string()],
            )
            .map_err(StorageError::from_rusqlite)?;
            tx.commit().map_err(StorageError::from_rusqlite)?;
            Ok(())
        })
        .await
    }

    /// Inserts one destination session row.
    ///
    /// Intentionally SQLCipher-specific; this method is not exposed on
    /// [`MetadataStore`].
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn insert_destination_session(
        &self,
        destination_id: String,
        label: String,
        destination_type: String,
        rclone_remote_name: String,
        rclone_config_blob: String,
        bucket: String,
        path_prefix: String,
        is_primary: bool,
        backup_mode: Option<String>,
    ) -> Result<(), StorageError> {
        let created_at = unix_timestamp_now()?;
        self.with_connection_blocking(move |conn| {
            let tx = conn.transaction().map_err(StorageError::from_rusqlite)?;
            if is_primary {
                let existing_primary: Option<String> = tx
                    .query_row(
                        "SELECT destination_id FROM destination_sessions WHERE is_primary = 1 LIMIT 1",
                        [],
                        |row| row.get(0),
                    )
                    .optional()
                    .map_err(StorageError::from_rusqlite)?;

                if let Some(existing_primary) = existing_primary
                    && existing_primary != destination_id
                {
                    return Err(StorageError::ConstraintViolation(
                        "only one primary destination is allowed".to_owned(),
                    ));
                }
            }
            tx.execute(
                "INSERT INTO destination_sessions (
                    destination_id, label, destination_type, rclone_remote_name, rclone_config_blob,
                    bucket, path_prefix, is_primary, backup_mode, created_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    destination_id,
                    label,
                    destination_type,
                    rclone_remote_name,
                    rclone_config_blob,
                    bucket,
                    path_prefix,
                    if is_primary { 1 } else { 0 },
                    backup_mode,
                    created_at,
                ],
            )
            .map_err(StorageError::from_rusqlite)?;

            tx.commit().map_err(StorageError::from_rusqlite)?;
            Ok(())
        })
        .await
    }

    /// Lists all destination sessions in created-at order.
    ///
    /// Intentionally SQLCipher-specific; this method is not exposed on
    /// [`MetadataStore`].
    pub(crate) async fn list_destination_sessions(
        &self,
    ) -> Result<Vec<DestinationSessionRow>, StorageError> {
        self.with_connection_blocking(move |conn| {
            let mut statement = conn
                .prepare(
                    "SELECT destination_id, label, destination_type, rclone_remote_name,
                            rclone_config_blob, bucket, path_prefix, is_primary, backup_mode
                     FROM destination_sessions
                     ORDER BY created_at ASC, destination_id ASC",
                )
                .map_err(StorageError::from_rusqlite)?;
            let rows = statement
                .query_map([], |row| {
                    Ok(DestinationSessionRow {
                        destination_id: row.get(0)?,
                        label: row.get(1)?,
                        destination_type: row.get(2)?,
                        rclone_remote_name: row.get(3)?,
                        rclone_config_blob: row.get(4)?,
                        bucket: row.get(5)?,
                        path_prefix: row.get(6)?,
                        is_primary: row.get::<_, i64>(7)? == 1,
                        backup_mode: row.get(8)?,
                    })
                })
                .map_err(StorageError::from_rusqlite)?;

            let mut sessions = Vec::new();
            for row in rows {
                sessions.push(row.map_err(StorageError::from_rusqlite)?);
            }
            Ok(sessions)
        })
        .await
    }

    /// Returns the primary destination session when present.
    ///
    /// Intentionally SQLCipher-specific; this method is not exposed on
    /// [`MetadataStore`].
    pub(crate) async fn get_primary_destination(
        &self,
    ) -> Result<Option<DestinationSessionRow>, StorageError> {
        self.with_connection_blocking(move |conn| {
            conn.query_row(
                "SELECT destination_id, label, destination_type, rclone_remote_name,
                        rclone_config_blob, bucket, path_prefix, is_primary, backup_mode
                 FROM destination_sessions
                 WHERE is_primary = 1
                 LIMIT 1",
                [],
                |row| {
                    Ok(DestinationSessionRow {
                        destination_id: row.get(0)?,
                        label: row.get(1)?,
                        destination_type: row.get(2)?,
                        rclone_remote_name: row.get(3)?,
                        rclone_config_blob: row.get(4)?,
                        bucket: row.get(5)?,
                        path_prefix: row.get(6)?,
                        is_primary: row.get::<_, i64>(7)? == 1,
                        backup_mode: row.get(8)?,
                    })
                },
            )
            .optional()
            .map_err(StorageError::from_rusqlite)
        })
        .await
    }

    /// Deletes a destination session by ID.
    ///
    /// Intentionally SQLCipher-specific; this method is not exposed on
    /// [`MetadataStore`].
    pub(crate) async fn delete_destination_session(
        &self,
        destination_id: String,
    ) -> Result<(), StorageError> {
        self.with_connection_blocking(move |conn| {
            conn.execute(
                "DELETE FROM destination_sessions WHERE destination_id = ?1",
                params![destination_id],
            )
            .map_err(StorageError::from_rusqlite)?;
            Ok(())
        })
        .await
    }

    /// Deletes `manifest_meta.last_synced_at`.
    ///
    /// Intentionally SQLCipher-specific; this method is not exposed on
    /// [`MetadataStore`].
    pub(crate) async fn clear_last_synced_at(&self) -> Result<(), StorageError> {
        self.with_connection_blocking(move |conn| {
            let tx = conn.transaction().map_err(StorageError::from_rusqlite)?;
            tx.execute("DELETE FROM manifest_meta WHERE key = 'last_synced_at'", [])
                .map_err(StorageError::from_rusqlite)?;
            tx.commit().map_err(StorageError::from_rusqlite)?;
            Ok(())
        })
        .await
    }

    /// Replaces a file's wrapped key and all chunk rows in a single atomic transaction,
    /// queueing old blobs for pending deletion.
    ///
    /// Intentionally SQLCipher-specific; this method is not exposed on [`MetadataStore`].
    pub(crate) async fn replace_file_key_and_chunks(
        &self,
        file_id: Uuid,
        new_file_key_wrapped: [u8; 72],
        new_chunks: Vec<ChunkRecord>,
        queued_at: i64,
    ) -> Result<(), StorageError> {
        self.with_connection_blocking(move |conn| {
            let tx = conn.transaction().map_err(StorageError::from_rusqlite)?;
            let file_id_text = file_id.hyphenated().to_string();

            let mut stmt = tx
                .prepare("SELECT blob_name FROM chunks WHERE node_id = ?1")
                .map_err(StorageError::from_rusqlite)?;
            let old_blob_names: Vec<String> =
                stmt.query_map(params![file_id_text.clone()], |row| row.get(0))
                    .map_err(StorageError::from_rusqlite)?
                    .map(|r| r.map_err(StorageError::from_rusqlite))
                    .collect::<Result<Vec<_>, _>>()?;
            drop(stmt);

            for blob_name in &old_blob_names {
                tx.execute(
                    "INSERT OR IGNORE INTO pending_deletions (blob_name, queued_at) VALUES (?1, ?2)",
                    params![blob_name, queued_at],
                )
                .map_err(StorageError::from_rusqlite)?;
            }

            tx.execute(
                "DELETE FROM chunks WHERE node_id = ?1",
                params![file_id_text.clone()],
            )
            .map_err(StorageError::from_rusqlite)?;

            let wrapped_blob = new_file_key_wrapped.to_vec();
            let rows_updated = tx
                .execute(
                    "UPDATE nodes SET file_key_wrapped = ?1 WHERE node_id = ?2",
                    params![wrapped_blob, file_id_text.clone()],
                )
                .map_err(StorageError::from_rusqlite)?;
            if rows_updated == 0 {
                return Err(StorageError::NotFound);
            }

            for chunk in &new_chunks {
                let chunk_id_text = chunk.chunk_id.hyphenated().to_string();
                let node_id_text = chunk.node_id.to_string();
                let checksum_blob = chunk.blake3_checksum.to_vec();
                tx.execute(
                    "INSERT INTO chunks (chunk_id, node_id, chunk_index, blob_name, size_padded, blake3_checksum) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![
                        chunk_id_text,
                        node_id_text,
                        chunk.chunk_index as i64,
                        chunk.blob_name.clone(),
                        chunk.size_padded as i64,
                        checksum_blob,
                    ],
                )
                .map_err(StorageError::from_rusqlite)?;
            }

            tx.commit().map_err(StorageError::from_rusqlite)?;
            Ok(())
        })
        .await
    }

    #[cfg(test)]
    pub(crate) async fn drop_manifest_meta_table_for_tests(&self) -> Result<(), StorageError> {
        self.with_connection_blocking(move |conn| {
            conn.execute_batch("DROP TABLE manifest_meta;")
                .map_err(StorageError::from_rusqlite)?;
            Ok(())
        })
        .await
    }

    /// Executes a blocking SQLite closure without stalling the async runtime.
    pub(crate) async fn with_connection_blocking<T, F>(
        &self,
        operation: F,
    ) -> Result<T, StorageError>
    where
        F: FnOnce(&mut Connection) -> Result<T, StorageError> + Send + 'static,
        T: Send + 'static,
    {
        let conn = Arc::clone(&self.conn);
        tokio::task::spawn_blocking(move || {
            let mut guard = conn.blocking_lock();
            operation(&mut guard)
        })
        .await
        .map_err(|error| StorageError::Database(error.to_string()))?
    }
}

/// Copies borrowed SQLCipher key bytes into protected heap storage.
fn protected_sqlcipher_key_from_slice(bytes: &[u8; 32]) -> SqlcipherKey {
    let mut boxed = Box::new([0u8; 32]);
    boxed.copy_from_slice(bytes);
    SqlcipherKey::from_secret_box(SecretBox::new(boxed))
}

/// Opens and keys a connection, verifies key correctness, and enables FK checks.
fn open_keyed_connection(
    path: &PathBuf,
    sqlcipher_key: &SqlcipherKey,
) -> Result<Connection, StorageError> {
    let conn = Connection::open(path).map_err(StorageError::from_rusqlite)?;
    apply_sqlcipher_key(&conn, sqlcipher_key)?;
    verify_sqlcipher_key(&conn)?;
    conn.execute_batch("PRAGMA foreign_keys = ON;")
        .map_err(StorageError::from_rusqlite)?;
    Ok(conn)
}

/// Applies a raw 32-byte SQLCipher key via `sqlite3_key`.
fn apply_sqlcipher_key(
    conn: &Connection,
    sqlcipher_key: &SqlcipherKey,
) -> Result<(), StorageError> {
    let rc = sqlcipher_key.with_exposed(|key_bytes| {
        // SAFETY: `conn` is open for this thread and `sqlcipher_key` points to
        // a valid 32-byte buffer for the duration of this FFI call.
        unsafe {
            rusqlite::ffi::sqlite3_key(
                conn.handle(),
                key_bytes.as_ptr().cast(),
                key_bytes.len() as i32,
            )
        }
    });
    if rc != rusqlite::ffi::SQLITE_OK {
        let message = {
            // SAFETY: SQLite owns the returned NUL-terminated error string and
            // the connection handle remains valid while `conn` is alive.
            unsafe {
                let message_ptr = rusqlite::ffi::sqlite3_errmsg(conn.handle());
                std::ffi::CStr::from_ptr(message_ptr)
                    .to_string_lossy()
                    .into_owned()
            }
        };
        return Err(StorageError::Database(message));
    }
    Ok(())
}

/// Opens a SQLCipher database and applies a raw 32-byte key.
pub(crate) fn open_sqlcipher(
    path: &Path,
    sqlcipher_key: &SqlcipherKey,
) -> Result<Connection, SqlcipherOpenError> {
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_WRITE)
        .map_err(|error| SqlcipherOpenError::Open(error.to_string()))?;
    let rc = sqlcipher_key.with_exposed(|key_bytes| {
        // SAFETY: `conn` is open for this thread and `sqlcipher_key` points to
        // a valid 32-byte buffer for the duration of the call.
        unsafe {
            rusqlite::ffi::sqlite3_key(
                conn.handle(),
                key_bytes.as_ptr().cast(),
                key_bytes.len() as i32,
            )
        }
    });
    if rc != rusqlite::ffi::SQLITE_OK {
        return Err(SqlcipherOpenError::KeyRejected);
    }
    verify_sqlcipher_key(&conn).map_err(|_| SqlcipherOpenError::KeyRejected)?;
    conn.execute_batch("PRAGMA foreign_keys = ON;")
        .map_err(|error| SqlcipherOpenError::Open(error.to_string()))?;
    Ok(conn)
}

/// Reads `snapshot_counter` and optional `last_synced_at` from a SQLCipher DB.
pub(crate) fn read_snapshot_state_from_database(
    path: &Path,
    sqlcipher_key: &SqlcipherKey,
) -> Result<(u64, Option<i64>), StorageError> {
    let conn = open_sqlcipher(path, sqlcipher_key)
        .map_err(|error| StorageError::Database(error.to_string()))?;
    let snapshot_counter_raw = conn
        .query_row(
            "SELECT value FROM manifest_meta WHERE key = 'snapshot_counter'",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(StorageError::from_rusqlite)?
        .ok_or_else(|| {
            StorageError::Database("missing manifest_meta key: snapshot_counter".to_owned())
        })?;
    let snapshot_counter = snapshot_counter_raw.parse::<u64>().map_err(|_| {
        StorageError::Database("invalid snapshot_counter: not an unsigned integer".to_owned())
    })?;
    let last_synced_at = conn
        .query_row(
            "SELECT value FROM manifest_meta WHERE key = 'last_synced_at'",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(StorageError::from_rusqlite)?
        .and_then(|value| value.parse::<i64>().ok());
    Ok((snapshot_counter, last_synced_at))
}

fn manifest_meta_value(conn: &Connection, key: &str) -> Result<Option<String>, StorageError> {
    conn.query_row(
        "SELECT value FROM manifest_meta WHERE key = ?1",
        params![key],
        |row| row.get::<_, String>(0),
    )
    .optional()
    .map_err(StorageError::from_rusqlite)
}

fn validate_create_immutable_meta_matches(
    conn: &Connection,
    vault_id: Uuid,
    chunk_size_bytes: u64,
    epoch_buffer_enabled: bool,
) -> Result<(), StorageError> {
    let expected_values = [
        ("vault_id", vault_id.hyphenated().to_string()),
        ("chunk_size_bytes", chunk_size_bytes.to_string()),
        (
            "epoch_buffer_enabled",
            if epoch_buffer_enabled {
                "true".to_owned()
            } else {
                "false".to_owned()
            },
        ),
    ];

    for (key, expected) in expected_values {
        let actual = manifest_meta_value(conn, key)?
            .ok_or_else(|| StorageError::Database(format!("missing manifest_meta key: {key}")))?;
        validate_immutable_meta_matches_expected(key, &expected, &actual)?;
    }

    Ok(())
}

fn ensure_parent_is_directory_for_insert(
    tx: &rusqlite::Transaction<'_>,
    node: &Node,
) -> Result<(), StorageError> {
    if let Some(parent_id) = node.parent_id.map(|id| *id.as_uuid()) {
        if parent_id == *node.node_id.as_uuid() {
            return Err(StorageError::ConstraintViolation(
                "node cannot be its own parent".to_owned(),
            ));
        }

        let parent_type = tx
            .query_row(
                "SELECT node_type FROM nodes WHERE node_id = ?1",
                params![parent_id.hyphenated().to_string()],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(StorageError::from_rusqlite)?;
        match parent_type.as_deref() {
            Some("directory") => {}
            Some("file") => {
                return Err(StorageError::ConstraintViolation(
                    "parent must be a directory".to_owned(),
                ));
            }
            Some(_) => {
                return Err(StorageError::Database(
                    "invalid node_type stored for parent".to_owned(),
                ));
            }
            None => {
                return Err(StorageError::ConstraintViolation(
                    "missing parent node_id".to_owned(),
                ));
            }
        }
    }

    Ok(())
}

fn ensure_move_respects_hierarchy(
    tx: &rusqlite::Transaction<'_>,
    node_id: Uuid,
    new_parent_id: Option<Uuid>,
) -> Result<(), StorageError> {
    let node_exists = tx
        .query_row(
            "SELECT 1 FROM nodes WHERE node_id = ?1",
            params![node_id.hyphenated().to_string()],
            |_row| Ok(()),
        )
        .optional()
        .map_err(StorageError::from_rusqlite)?
        .is_some();
    if !node_exists {
        return Err(StorageError::NotFound);
    }

    if let Some(parent_id) = new_parent_id {
        if parent_id == node_id {
            return Err(StorageError::ConstraintViolation(
                "node cannot be its own parent".to_owned(),
            ));
        }

        let parent_type = tx
            .query_row(
                "SELECT node_type FROM nodes WHERE node_id = ?1",
                params![parent_id.hyphenated().to_string()],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(StorageError::from_rusqlite)?;
        match parent_type.as_deref() {
            Some("directory") => {}
            Some("file") => {
                return Err(StorageError::ConstraintViolation(
                    "parent must be a directory".to_owned(),
                ));
            }
            Some(_) => {
                return Err(StorageError::Database(
                    "invalid node_type stored for parent".to_owned(),
                ));
            }
            None => {
                return Err(StorageError::ConstraintViolation(
                    "missing parent node_id".to_owned(),
                ));
            }
        }

        let creates_cycle = tx
            .query_row(
                "WITH RECURSIVE subtree(node_id) AS (
                     SELECT node_id FROM nodes WHERE parent_id = ?1
                     UNION ALL
                     SELECT n.node_id
                     FROM nodes n
                     INNER JOIN subtree s ON n.parent_id = s.node_id
                 )
                 SELECT 1 FROM subtree WHERE node_id = ?2 LIMIT 1",
                params![
                    node_id.hyphenated().to_string(),
                    parent_id.hyphenated().to_string()
                ],
                |_row| Ok(()),
            )
            .optional()
            .map_err(StorageError::from_rusqlite)?
            .is_some();
        if creates_cycle {
            return Err(StorageError::ConstraintViolation(
                "move would create a cycle".to_owned(),
            ));
        }
    }

    Ok(())
}

/// Converts a SQL row into a `Node`.
fn read_node(row: &rusqlite::Row<'_>) -> Result<Node, rusqlite::Error> {
    let node_id_text: String = row.get(0)?;
    let parent_id_text: Option<String> = row.get(1)?;
    let node_type_text: String = row.get(2)?;
    let name: String = row.get(3)?;
    let created_at: i64 = row.get(4)?;
    let modified_at: i64 = row.get(5)?;
    let size_bytes_i64: i64 = row.get(6)?;
    let file_key_wrapped_vec: Option<Vec<u8>> = row.get(7)?;

    let node_id = Uuid::parse_str(&node_id_text).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(error))
    })?;
    let parent_id = match parent_id_text {
        Some(value) => Some(Uuid::parse_str(&value).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                1,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?),
        None => None,
    };
    let node_type = NodeType::try_from(node_type_text.as_str()).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            2,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, error)),
        )
    })?;
    let size_bytes = u64::try_from(size_bytes_i64).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            6,
            rusqlite::types::Type::Integer,
            Box::new(error),
        )
    })?;
    let file_key_wrapped = match file_key_wrapped_vec {
        Some(value) => Some(value.try_into().map_err(|_| {
            rusqlite::Error::FromSqlConversionFailure(
                7,
                rusqlite::types::Type::Blob,
                Box::new(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "expected 72 bytes for file_key_wrapped",
                )),
            )
        })?),
        None => None,
    };

    Ok(Node {
        node_id: NodeId::new(node_id),
        parent_id: parent_id.map(NodeId::new),
        node_type,
        name,
        created_at,
        modified_at,
        size_bytes,
        file_key_wrapped,
    })
}

/// Converts a SQL row into a `ChunkRecord`.
fn read_chunk(row: &rusqlite::Row<'_>) -> Result<ChunkRecord, rusqlite::Error> {
    let chunk_id_text: String = row.get(0)?;
    let node_id_text: String = row.get(1)?;
    let chunk_index_i64: i64 = row.get(2)?;
    let blob_name: String = row.get(3)?;
    let size_padded_i64: i64 = row.get(4)?;
    let checksum_vec: Vec<u8> = row.get(5)?;
    let epoch_blob_id_text: Option<String> = row.get(6)?;
    let byte_offset_i64: Option<i64> = row.get(7)?;
    let byte_length_i64: Option<i64> = row.get(8)?;

    let chunk_id = Uuid::parse_str(&chunk_id_text).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(error))
    })?;
    let node_id = Uuid::parse_str(&node_id_text).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(1, rusqlite::types::Type::Text, Box::new(error))
    })?;
    let chunk_index = u32::try_from(chunk_index_i64).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            2,
            rusqlite::types::Type::Integer,
            Box::new(error),
        )
    })?;
    let size_padded = u64::try_from(size_padded_i64).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            4,
            rusqlite::types::Type::Integer,
            Box::new(error),
        )
    })?;
    let blake3_checksum: [u8; 32] = checksum_vec.try_into().map_err(|_| {
        rusqlite::Error::FromSqlConversionFailure(
            5,
            rusqlite::types::Type::Blob,
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "expected 32-byte checksum",
            )),
        )
    })?;
    let epoch_blob_id = epoch_blob_id_text
        .map(|text| {
            Uuid::parse_str(&text).map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(
                    6,
                    rusqlite::types::Type::Text,
                    Box::new(error),
                )
            })
        })
        .transpose()?;
    let byte_offset = byte_offset_i64.map(|v| v as u64);
    let byte_length = byte_length_i64.map(|v| v as u64);

    Ok(ChunkRecord {
        chunk_id,
        node_id: NodeId::new(node_id),
        chunk_index,
        blob_name,
        size_padded,
        blake3_checksum,
        epoch_blob_id,
        byte_offset,
        byte_length,
    })
}

/// Returns current Unix timestamp in seconds.
fn unix_timestamp_now() -> Result<i64, StorageError> {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| StorageError::Database(error.to_string()))?;
    i64::try_from(duration.as_secs()).map_err(|error| StorageError::Database(error.to_string()))
}

#[async_trait]
impl MetadataStore for SqlCipherMetadataStore {
    /// Inserts a single node row in a transaction.
    async fn insert_node(&self, node: &Node) -> Result<(), StorageError> {
        let node = node.clone();
        self.with_connection_blocking(move |conn| {
            let tx = conn.transaction().map_err(StorageError::from_rusqlite)?;
            ensure_parent_is_directory_for_insert(&tx, &node)?;
            tx.execute(
                "INSERT INTO nodes (node_id, parent_id, node_type, name, created_at, modified_at, size_bytes, file_key_wrapped)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    node.node_id.to_string(),
                    node.parent_id.map(|id| id.to_string()),
                    node.node_type.as_ref(),
                    node.name,
                    node.created_at,
                    node.modified_at,
                    i64::try_from(node.size_bytes).map_err(|error| StorageError::Database(error.to_string()))?,
                    node.file_key_wrapped.map(|bytes| bytes.to_vec())
                ],
            )
            .map_err(StorageError::from_rusqlite)?;
            tx.commit().map_err(StorageError::from_rusqlite)?;
            Ok(())
        })
        .await
    }

    /// Inserts chunk rows in a single transaction.
    async fn insert_chunks(&self, chunks: &[ChunkRecord]) -> Result<(), StorageError> {
        let chunks = chunks.to_vec();
        self.with_connection_blocking(move |conn| {
            let tx = conn.transaction().map_err(StorageError::from_rusqlite)?;
            let chunk_size_text = tx
                .query_row(
                    "SELECT value FROM manifest_meta WHERE key = 'chunk_size_bytes'",
                    [],
                    |row| row.get::<_, String>(0),
                )
                .optional()
                .map_err(StorageError::from_rusqlite)?
                .ok_or_else(|| {
                    StorageError::Database("missing manifest_meta key: chunk_size_bytes".to_owned())
                })?;
            let chunk_size_bytes = parse_chunk_size_bytes(&chunk_size_text)?;
            for chunk in chunks {
                validate_blob_name_uuid_v4(&chunk.blob_name)?;
                validate_size_padded_matches_chunk_size(chunk.size_padded, chunk_size_bytes)?;
                let node_type = tx
                    .query_row(
                        "SELECT node_type FROM nodes WHERE node_id = ?1",
                        params![chunk.node_id.to_string()],
                        |row| row.get::<_, String>(0),
                    )
                    .optional()
                    .map_err(StorageError::from_rusqlite)?
                    .map(|value| {
                        NodeType::try_from(value.as_str()).map_err(|_| {
                            StorageError::Database("invalid node_type stored for chunk node".to_owned())
                        })
                    })
                    .transpose()?;
                validate_chunk_target_node(node_type)?;
                tx.execute(
                    "INSERT INTO chunks (chunk_id, node_id, chunk_index, blob_name, size_padded, blake3_checksum)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![
                        chunk.chunk_id.hyphenated().to_string(),
                        chunk.node_id.to_string(),
                        i64::from(chunk.chunk_index),
                        chunk.blob_name,
                        i64::try_from(chunk.size_padded).map_err(|error| StorageError::Database(error.to_string()))?,
                        chunk.blake3_checksum.to_vec()
                    ],
                )
                .map_err(StorageError::from_rusqlite)?;
            }
            tx.commit().map_err(StorageError::from_rusqlite)?;
            Ok(())
        })
        .await
    }

    /// Inserts a file node and all chunk rows in one transaction.
    async fn insert_file_with_chunks(
        &self,
        node: &Node,
        chunks: &[ChunkRecord],
    ) -> Result<(), StorageError> {
        let node = node.clone();
        let chunks = chunks.to_vec();
        self.with_connection_blocking(move |conn| {
            let tx = conn.transaction().map_err(StorageError::from_rusqlite)?;
            ensure_parent_is_directory_for_insert(&tx, &node)?;
            tx.execute(
                "INSERT INTO nodes (node_id, parent_id, node_type, name, created_at, modified_at, size_bytes, file_key_wrapped)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    node.node_id.to_string(),
                    node.parent_id.map(|id| id.to_string()),
                    node.node_type.as_ref(),
                    node.name,
                    node.created_at,
                    node.modified_at,
                    i64::try_from(node.size_bytes)
                        .map_err(|error| StorageError::Database(error.to_string()))?,
                    node.file_key_wrapped.map(|bytes| bytes.to_vec())
                ],
            )
            .map_err(StorageError::from_rusqlite)?;

            let chunk_size_text = tx
                .query_row(
                    "SELECT value FROM manifest_meta WHERE key = 'chunk_size_bytes'",
                    [],
                    |row| row.get::<_, String>(0),
                )
                .optional()
                .map_err(StorageError::from_rusqlite)?
                .ok_or_else(|| {
                    StorageError::Database("missing manifest_meta key: chunk_size_bytes".to_owned())
                })?;
            let chunk_size_bytes = parse_chunk_size_bytes(&chunk_size_text)?;

            for chunk in chunks {
                if chunk.node_id != node.node_id {
                    return Err(StorageError::ConstraintViolation(
                        "chunk node_id does not match inserted file node".to_owned(),
                    ));
                }
                validate_blob_name_uuid_v4(&chunk.blob_name)?;
                validate_size_padded_matches_chunk_size(chunk.size_padded, chunk_size_bytes)?;
                validate_chunk_target_node(Some(node.node_type))?;
                tx.execute(
                    "INSERT INTO chunks (chunk_id, node_id, chunk_index, blob_name, size_padded, blake3_checksum)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![
                        chunk.chunk_id.hyphenated().to_string(),
                        chunk.node_id.to_string(),
                        i64::from(chunk.chunk_index),
                        chunk.blob_name,
                        i64::try_from(chunk.size_padded)
                            .map_err(|error| StorageError::Database(error.to_string()))?,
                        chunk.blake3_checksum.to_vec()
                    ],
                )
                .map_err(StorageError::from_rusqlite)?;
            }

            tx.commit().map_err(StorageError::from_rusqlite)?;
            Ok(())
        })
        .await
    }

    /// Retrieves one node by ID.
    async fn get_node(&self, node_id: Uuid) -> Result<Node, StorageError> {
        self.with_connection_blocking(move |conn| {
            conn.query_row(
                "SELECT node_id, parent_id, node_type, name, created_at, modified_at, size_bytes, file_key_wrapped
                 FROM nodes
                 WHERE node_id = ?1",
                params![node_id.hyphenated().to_string()],
                read_node,
            )
            .optional()
            .map_err(StorageError::from_rusqlite)?
            .ok_or(StorageError::NotFound)
        })
        .await
    }

    /// Lists direct children for a parent node.
    async fn list_children(&self, parent_id: Uuid) -> Result<Vec<Node>, StorageError> {
        self.with_connection_blocking(move |conn| {
            let mut statement = conn
                .prepare(
                    "SELECT node_id, parent_id, node_type, name, created_at, modified_at, size_bytes, file_key_wrapped
                     FROM nodes
                     WHERE parent_id = ?1
                     ORDER BY name ASC",
                )
                .map_err(StorageError::from_rusqlite)?;
            let rows = statement
                .query_map(params![parent_id.hyphenated().to_string()], read_node)
                .map_err(StorageError::from_rusqlite)?;
            let mut nodes = Vec::new();
            for row in rows {
                nodes.push(row.map_err(StorageError::from_rusqlite)?);
            }
            Ok(nodes)
        })
        .await
    }

    /// Retrieves all chunks for a node ordered by `chunk_index`.
    async fn get_chunks(&self, node_id: Uuid) -> Result<Vec<ChunkRecord>, StorageError> {
        self.with_connection_blocking(move |conn| {
            let mut statement = conn
                .prepare(
                    "SELECT c.chunk_id, c.node_id, c.chunk_index,
                            COALESCE(c.blob_name, eb.blob_name) AS blob_name,
                            COALESCE(c.size_padded, eb.size_padded) AS size_padded,
                            COALESCE(c.blake3_checksum, eb.blake3_checksum) AS blake3_checksum,
                            c.epoch_blob_id, c.byte_offset, c.byte_length
                     FROM chunks c
                     LEFT JOIN epoch_blobs eb ON c.epoch_blob_id = eb.epoch_blob_id
                     WHERE c.node_id = ?1
                     ORDER BY c.chunk_index ASC",
                )
                .map_err(StorageError::from_rusqlite)?;
            let rows = statement
                .query_map(params![node_id.hyphenated().to_string()], read_chunk)
                .map_err(StorageError::from_rusqlite)?;
            let mut chunks = Vec::new();
            for row in rows {
                chunks.push(row.map_err(StorageError::from_rusqlite)?);
            }
            Ok(chunks)
        })
        .await
    }

    /// Updates node name and modified time in one transaction.
    async fn rename_node(
        &self,
        node_id: Uuid,
        new_name: &str,
        modified_at: i64,
    ) -> Result<(), StorageError> {
        let new_name = new_name.to_owned();
        self.with_connection_blocking(move |conn| {
            let tx = conn.transaction().map_err(StorageError::from_rusqlite)?;
            let affected = tx
                .execute(
                    "UPDATE nodes SET name = ?1, modified_at = ?2 WHERE node_id = ?3",
                    params![new_name, modified_at, node_id.hyphenated().to_string()],
                )
                .map_err(StorageError::from_rusqlite)?;
            if affected == 0 {
                return Err(StorageError::NotFound);
            }
            tx.commit().map_err(StorageError::from_rusqlite)?;
            Ok(())
        })
        .await
    }

    /// Updates node parent and modified time in one transaction.
    async fn move_node(
        &self,
        node_id: Uuid,
        new_parent_id: Option<Uuid>,
        modified_at: i64,
    ) -> Result<(), StorageError> {
        self.with_connection_blocking(move |conn| {
            let tx = conn.transaction().map_err(StorageError::from_rusqlite)?;
            ensure_move_respects_hierarchy(&tx, node_id, new_parent_id)?;
            let affected = tx
                .execute(
                    "UPDATE nodes SET parent_id = ?1, modified_at = ?2 WHERE node_id = ?3",
                    params![
                        new_parent_id.map(|id| id.hyphenated().to_string()),
                        modified_at,
                        node_id.hyphenated().to_string()
                    ],
                )
                .map_err(StorageError::from_rusqlite)?;
            debug_assert_ne!(affected, 0);
            tx.commit().map_err(StorageError::from_rusqlite)?;
            Ok(())
        })
        .await
    }

    /// Deletes a node after enqueuing its blob names into `pending_deletions`.
    async fn delete_node(&self, node_id: Uuid) -> Result<(), StorageError> {
        self.with_connection_blocking(move |conn| {
            let tx = conn.transaction().map_err(StorageError::from_rusqlite)?;
            let queued_at = unix_timestamp_now()?;
            tx.execute(
                "WITH RECURSIVE subtree(node_id) AS (
                     SELECT node_id FROM nodes WHERE node_id = ?1
                     UNION ALL
                     SELECT n.node_id
                     FROM nodes n
                     INNER JOIN subtree s ON n.parent_id = s.node_id
                 )
                 INSERT OR IGNORE INTO pending_deletions (blob_name, queued_at)
                 SELECT c.blob_name, ?2
                 FROM chunks c
                 INNER JOIN subtree s ON c.node_id = s.node_id",
                params![node_id.hyphenated().to_string(), queued_at],
            )
            .map_err(StorageError::from_rusqlite)?;

            let affected = tx
                .execute(
                    "DELETE FROM nodes WHERE node_id = ?1",
                    params![node_id.hyphenated().to_string()],
                )
                .map_err(StorageError::from_rusqlite)?;
            if affected == 0 {
                return Err(StorageError::NotFound);
            }
            tx.commit().map_err(StorageError::from_rusqlite)?;
            Ok(())
        })
        .await
    }

    /// Lists queued blob names up to `limit` entries.
    async fn list_pending_deletions(&self, limit: usize) -> Result<Vec<String>, StorageError> {
        self.with_connection_blocking(move |conn| {
            let limit = i64::try_from(limit).map_err(|error| StorageError::Database(error.to_string()))?;
            let mut statement = conn
                .prepare(
                    "SELECT blob_name FROM pending_deletions ORDER BY queued_at ASC, blob_name ASC LIMIT ?1",
                )
                .map_err(StorageError::from_rusqlite)?;
            let rows = statement
                .query_map(params![limit], |row| row.get::<_, String>(0))
                .map_err(StorageError::from_rusqlite)?;
            let mut names = Vec::new();
            for row in rows {
                names.push(row.map_err(StorageError::from_rusqlite)?);
            }
            Ok(names)
        })
        .await
    }

    /// Removes a blob name from the pending-deletions queue.
    async fn mark_deletion_complete(&self, blob_name: &str) -> Result<(), StorageError> {
        let blob_name = blob_name.to_owned();
        self.with_connection_blocking(move |conn| {
            let tx = conn.transaction().map_err(StorageError::from_rusqlite)?;
            tx.execute(
                "DELETE FROM pending_deletions WHERE blob_name = ?1",
                params![blob_name],
            )
            .map_err(StorageError::from_rusqlite)?;
            tx.commit().map_err(StorageError::from_rusqlite)?;
            Ok(())
        })
        .await
    }

    /// Gets a manifest-meta value by key.
    async fn get_meta(&self, key: &str) -> Result<Option<String>, StorageError> {
        let key = key.to_owned();
        self.with_connection_blocking(move |conn| {
            conn.query_row(
                "SELECT value FROM manifest_meta WHERE key = ?1",
                params![key],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(StorageError::from_rusqlite)
        })
        .await
    }

    /// Sets or updates a manifest-meta entry in one transaction.
    async fn set_meta(&self, key: &str, value: &str) -> Result<(), StorageError> {
        if is_immutable_manifest_meta_key(key) {
            return Err(immutable_meta_key_violation(key));
        }

        let key = key.to_owned();
        let value = value.to_owned();
        self.with_connection_blocking(move |conn| {
            let tx = conn.transaction().map_err(StorageError::from_rusqlite)?;
            tx.execute(
                "INSERT INTO manifest_meta (key, value) VALUES (?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                params![key, value],
            )
            .map_err(StorageError::from_rusqlite)?;
            tx.commit().map_err(StorageError::from_rusqlite)?;
            Ok(())
        })
        .await
    }

    /// Atomically increments and returns `snapshot_counter`.
    async fn increment_snapshot_counter(&self) -> Result<u64, StorageError> {
        self.with_connection_blocking(move |conn| {
            let tx = conn.transaction().map_err(StorageError::from_rusqlite)?;
            let current_value = tx
                .query_row(
                    "SELECT value FROM manifest_meta WHERE key = 'snapshot_counter'",
                    [],
                    |row| row.get::<_, String>(0),
                )
                .optional()
                .map_err(StorageError::from_rusqlite)?
                .ok_or(StorageError::NotFound)?;
            let current_parsed = current_value.parse::<u64>().map_err(|_| {
                StorageError::Database(
                    "invalid snapshot_counter: not an unsigned integer".to_owned(),
                )
            })?;
            let next_value = current_parsed.checked_add(1).ok_or_else(|| {
                StorageError::Database("invalid snapshot_counter: overflow".to_owned())
            })?;
            tx.execute(
                "UPDATE manifest_meta SET value = ?1 WHERE key = 'snapshot_counter'",
                params![next_value.to_string()],
            )
            .map_err(StorageError::from_rusqlite)?;
            tx.commit().map_err(StorageError::from_rusqlite)?;
            Ok(next_value)
        })
        .await
    }

    /// Inserts a file node row without any associated chunk rows.
    async fn insert_file_node_only(&self, node: &Node) -> Result<(), StorageError> {
        let node = node.clone();
        self.with_connection_blocking(move |conn| {
            let tx = conn.transaction().map_err(StorageError::from_rusqlite)?;
            ensure_parent_is_directory_for_insert(&tx, &node)?;
            tx.execute(
                "INSERT INTO nodes (node_id, parent_id, node_type, name, created_at, modified_at, size_bytes, file_key_wrapped)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    node.node_id.to_string(),
                    node.parent_id.map(|id| id.to_string()),
                    node.node_type.as_ref(),
                    node.name,
                    node.created_at,
                    node.modified_at,
                    i64::try_from(node.size_bytes).map_err(|error| StorageError::Database(error.to_string()))?,
                    node.file_key_wrapped.map(|bytes| bytes.to_vec())
                ],
            )
            .map_err(StorageError::from_rusqlite)?;
            tx.commit().map_err(StorageError::from_rusqlite)?;
            Ok(())
        })
        .await
    }

    /// Stages a plaintext entry in the epoch buffer for the given node.
    async fn stage_epoch_entry(
        &self,
        node_id: Uuid,
        plaintext: Vec<u8>,
    ) -> Result<(), StorageError> {
        self.with_connection_blocking(move |conn| {
            let entry_id = Uuid::new_v4().hyphenated().to_string();
            let node_id_text = node_id.hyphenated().to_string();
            let size_bytes = plaintext.len() as i64;
            let queued_at = unix_timestamp_now()?;
            conn.execute(
                "INSERT INTO epoch_buffer (entry_id, node_id, plaintext, size_bytes, queued_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![entry_id, node_id_text, plaintext, size_bytes, queued_at],
            )
            .map_err(StorageError::from_rusqlite)?;
            Ok(())
        })
        .await
    }

    /// Returns the total number of bytes currently staged in the epoch buffer.
    async fn get_epoch_buffer_total_bytes(&self) -> Result<u64, StorageError> {
        self.with_connection_blocking(move |conn| {
            let total: i64 = conn
                .query_row(
                    "SELECT COALESCE(SUM(size_bytes), 0) FROM epoch_buffer",
                    [],
                    |row| row.get(0),
                )
                .map_err(StorageError::from_rusqlite)?;
            Ok(total as u64)
        })
        .await
    }

    /// Returns all entries currently staged in the epoch buffer.
    async fn get_epoch_buffer_entries(&self) -> Result<Vec<EpochBufferEntry>, StorageError> {
        self.with_connection_blocking(move |conn| {
            let mut statement = conn
                .prepare(
                    "SELECT entry_id, node_id, plaintext, size_bytes FROM epoch_buffer ORDER BY queued_at ASC",
                )
                .map_err(StorageError::from_rusqlite)?;
            let rows = statement
                .query_map([], |row| {
                    let entry_id_text: String = row.get(0)?;
                    let node_id_text: String = row.get(1)?;
                    let plaintext: Vec<u8> = row.get(2)?;
                    let size_bytes_i64: i64 = row.get(3)?;
                    Ok((entry_id_text, node_id_text, plaintext, size_bytes_i64))
                })
                .map_err(StorageError::from_rusqlite)?;
            let mut entries = Vec::new();
            for row in rows {
                let (entry_id_text, node_id_text, plaintext, size_bytes_i64) =
                    row.map_err(StorageError::from_rusqlite)?;
                let entry_id = Uuid::parse_str(&entry_id_text).map_err(|error| {
                    StorageError::Database(format!("invalid entry_id uuid: {error}"))
                })?;
                let node_id = Uuid::parse_str(&node_id_text).map_err(|error| {
                    StorageError::Database(format!("invalid node_id uuid: {error}"))
                })?;
                entries.push(EpochBufferEntry {
                    entry_id,
                    node_id,
                    plaintext,
                    size_bytes: size_bytes_i64 as u64,
                });
            }
            Ok(entries)
        })
        .await
    }

    /// Atomically inserts an epoch blob record, chunk rows, and clears the buffer.
    async fn commit_epoch_flush(
        &self,
        record: &EpochBlobRecord,
        extents: &[(Uuid, u32, u64, u64)],
    ) -> Result<(), StorageError> {
        let record = record.clone();
        let extents = extents.to_vec();
        self.with_connection_blocking(move |conn| {
            let tx = conn.transaction().map_err(StorageError::from_rusqlite)?;
            let epoch_blob_id_text = record.epoch_blob_id.hyphenated().to_string();
            tx.execute(
                "INSERT INTO epoch_blobs (epoch_blob_id, blob_name, file_key_wrapped, size_padded, blake3_checksum)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    epoch_blob_id_text,
                    record.blob_name,
                    record.file_key_wrapped,
                    i64::try_from(record.size_padded).map_err(|error| StorageError::Database(error.to_string()))?,
                    record.blake3_checksum.to_vec()
                ],
            )
            .map_err(StorageError::from_rusqlite)?;

            for (node_id, chunk_index, byte_offset, byte_length) in &extents {
                let chunk_id = Uuid::new_v4().hyphenated().to_string();
                let node_id_text = node_id.hyphenated().to_string();
                let size_padded_i64 = i64::try_from(record.size_padded)
                    .map_err(|error| StorageError::Database(error.to_string()))?;
                let byte_offset_i64 = *byte_offset as i64;
                let byte_length_i64 = *byte_length as i64;
                tx.execute(
                    "INSERT INTO chunks (chunk_id, node_id, chunk_index, blob_name, size_padded, blake3_checksum, epoch_blob_id, byte_offset, byte_length)
                     VALUES (?1, ?2, ?3, NULL, ?4, ?5, ?6, ?7, ?8)",
                    params![
                        chunk_id,
                        node_id_text,
                        i64::from(*chunk_index),
                        size_padded_i64,
                        record.blake3_checksum.to_vec(),
                        epoch_blob_id_text,
                        byte_offset_i64,
                        byte_length_i64
                    ],
                )
                .map_err(StorageError::from_rusqlite)?;
            }

            tx.execute("DELETE FROM epoch_buffer", [])
                .map_err(StorageError::from_rusqlite)?;
            tx.commit().map_err(StorageError::from_rusqlite)?;
            Ok(())
        })
        .await
    }

    /// Retrieves an epoch blob record by identifier.
    async fn get_epoch_blob(&self, epoch_blob_id: Uuid) -> Result<EpochBlobRecord, StorageError> {
        self.with_connection_blocking(move |conn| {
            let epoch_blob_id_text = epoch_blob_id.hyphenated().to_string();
            conn.query_row(
                "SELECT epoch_blob_id, blob_name, file_key_wrapped, size_padded, blake3_checksum
                 FROM epoch_blobs WHERE epoch_blob_id = ?1",
                params![epoch_blob_id_text],
                |row| {
                    let epoch_blob_id_str: String = row.get(0)?;
                    let blob_name: String = row.get(1)?;
                    let file_key_wrapped: Vec<u8> = row.get(2)?;
                    let size_padded_i64: i64 = row.get(3)?;
                    let checksum_vec: Vec<u8> = row.get(4)?;
                    Ok((epoch_blob_id_str, blob_name, file_key_wrapped, size_padded_i64, checksum_vec))
                },
            )
            .optional()
            .map_err(StorageError::from_rusqlite)?
            .ok_or(StorageError::NotFound)
            .and_then(|(epoch_blob_id_str, blob_name, file_key_wrapped, size_padded_i64, checksum_vec)| {
                let epoch_blob_id = Uuid::parse_str(&epoch_blob_id_str).map_err(|error| {
                    StorageError::Database(format!("invalid epoch_blob_id uuid: {error}"))
                })?;
                let size_padded = size_padded_i64 as u64;
                let blake3_checksum: [u8; 32] = checksum_vec.try_into().map_err(|_| {
                    StorageError::Database("expected 32-byte checksum in epoch_blobs".to_owned())
                })?;
                Ok(EpochBlobRecord {
                    epoch_blob_id,
                    blob_name,
                    file_key_wrapped,
                    size_padded,
                    blake3_checksum,
                })
            })
        })
        .await
    }
}

/// AuthUserStore trait implementation for SqlCipherMetadataStore.
///
/// Converts `AuthUser` adapter types into storage schema mutations, managing
/// the boundary between auth ceremonies and storage implementation.
#[async_trait]
impl AuthUserStore for SqlCipherMetadataStore {
    /// Create a new vault user entry (post-authentication).
    ///
    /// This is called after `create_vault` successfully uploads the vault header
    /// and initializes the manifest database. For now, this is a placeholder
    /// that validates the vault exists (via manifest_meta) without storing
    /// additional user metadata.
    ///
    /// # Future enhancement
    /// When a dedicated user metadata table is added to the schema, this method
    /// will persist the vault ID, salt, and key file hash for faster authentication.
    async fn create_vault_user(&self, _user: AuthUser) -> Result<(), StorageError> {
        // Current implementation: vault metadata is stored in manifest_meta during
        // create_vault ceremony and vault_identity row is created in the ceremony itself.
        // AuthUser creation is validated implicitly by the ceremony's successful
        // database initialization.
        Ok(())
    }

    /// Update password hash and salt (after re-authentication).
    ///
    /// Called during `change_password` ceremony to persist the new salt.
    /// For now, this is a placeholder as the salt is stored in the vault header
    /// (cloud-stored and managed separately).
    ///
    /// # Future enhancement
    /// When user metadata is stored in the manifest database, this will update
    /// the salt for faster future authentications.
    async fn update_password(
        &self,
        _vault_id: &crate::crypto::VaultId,
        _salt: [u8; 16],
    ) -> Result<(), StorageError> {
        // Current implementation: salt is maintained in vault_header (cloud storage)
        // This method is available for future use when user metadata tables are added.
        Ok(())
    }

    /// Rotate key file hash (post-authentication).
    ///
    /// Called during `rotate_key_file` ceremony to update the stored USB key file
    /// hash. For now, this is a placeholder as key_file_blake3 is stored in the
    /// vault header (cloud-stored and managed separately).
    ///
    /// # Future enhancement
    /// When user metadata is stored in the manifest database, this will update
    /// the key file hash for Tier 2 vaults.
    async fn rotate_key_file(
        &self,
        _vault_id: &crate::crypto::VaultId,
        _key_file_hash: [u8; 32],
    ) -> Result<(), StorageError> {
        // Current implementation: key_file_blake3 is maintained in vault_header (cloud storage)
        // This method is available for future use when user metadata tables are added.
        Ok(())
    }

    /// Look up user by vault ID (for session establishment).
    ///
    /// Retrieves minimal user data needed to validate credentials during authentication.
    /// For now, this reads the vault ID from manifest_meta to validate the vault exists.
    ///
    /// # Errors
    /// - `StorageError::NotFound` if vault_id is not found in manifest_meta
    ///
    /// # Future enhancement
    /// When user metadata is stored in the manifest database, this will return
    /// the full AuthUser (vault_id, salt, key_file_hash) for authentication.
    async fn get_user(&self, vault_id: &crate::crypto::VaultId) -> Result<AuthUser, StorageError> {
        let vault_id_string = vault_id.to_uuid().hyphenated().to_string();
        let vault_id_copy = *vault_id;
        self.with_connection_blocking(move |conn| {
            conn.query_row(
                "SELECT value FROM manifest_meta WHERE key = 'vault_id'",
                [],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(StorageError::from_rusqlite)?
            .ok_or(StorageError::NotFound)
            .and_then(|stored_vault_id| {
                if stored_vault_id == vault_id_string {
                    Ok(AuthUser {
                        vault_id: vault_id_copy,
                        salt: [0u8; 16], // Placeholder: from vault_header in practice
                        key_file_hash: None, // Placeholder: from vault_header in practice
                    })
                } else {
                    Err(StorageError::NotFound)
                }
            })
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use rusqlite::params;
    use tempfile::tempdir;
    use uuid::Uuid;

    use super::SqlCipherMetadataStore;
    use crate::storage::MetadataStore;
    use crate::storage::error::StorageError;
    use crate::storage::types::{ChunkRecord, Node, NodeType};

    fn directory_node(node_id: Uuid, parent_id: Option<Uuid>, name: &str) -> Node {
        Node::new(
            node_id,
            parent_id,
            NodeType::Directory,
            name.to_owned(),
            1,
            1,
            0,
            None,
        )
    }

    fn file_node(node_id: Uuid, parent_id: Option<Uuid>, name: &str) -> Node {
        Node::new(
            node_id,
            parent_id,
            NodeType::File,
            name.to_owned(),
            1,
            1,
            42,
            Some([9; 72]),
        )
    }

    fn chunk_for(node_id: Uuid, chunk_index: u32, blob_name: &str) -> ChunkRecord {
        ChunkRecord {
            chunk_id: Uuid::new_v4(),
            node_id: node_id.into(),
            chunk_index,
            blob_name: blob_name.to_owned(),
            size_padded: 4_194_304,
            blake3_checksum: [7; 32],
            epoch_blob_id: None,
            byte_offset: None,
            byte_length: None,
        }
    }

    #[tokio::test]
    async fn test_open_wrong_key_returns_wrong_key() {
        let temp = tempdir().expect("tempdir should be created");
        let db_path = temp.path().join("manifest.db");
        let create_key = [5; 32];
        let wrong_key = [6; 32];
        SqlCipherMetadataStore::create(&db_path, &create_key, Uuid::new_v4(), 4_194_304, false)
            .await
            .expect("store should be created");

        let result = SqlCipherMetadataStore::open(&db_path, &wrong_key).await;

        assert!(matches!(result, Err(StorageError::WrongKey)));
    }

    #[tokio::test]
    async fn test_open_sqlcipher_missing_path_returns_open_error() {
        let temp = tempdir().expect("tempdir should be created");
        let db_path = temp.path().join("missing.db");
        let sqlcipher_key = crate::crypto::SqlcipherKey::from_bytes([5u8; 32]);

        let result = super::open_sqlcipher(&db_path, &sqlcipher_key);

        assert!(matches!(result, Err(super::SqlcipherOpenError::Open(_))));
    }

    #[tokio::test]
    async fn test_open_sqlcipher_wrong_key_returns_key_rejected() {
        let temp = tempdir().expect("tempdir should be created");
        let db_path = temp.path().join("manifest.db");
        let create_key = [7u8; 32];
        let wrong_key = [8u8; 32];
        let wrong_sqlcipher_key = crate::crypto::SqlcipherKey::from_bytes(wrong_key);
        SqlCipherMetadataStore::create(&db_path, &create_key, Uuid::new_v4(), 4_194_304, false)
            .await
            .expect("store should be created");

        let result = super::open_sqlcipher(&db_path, &wrong_sqlcipher_key);

        assert!(matches!(
            result,
            Err(super::SqlcipherOpenError::KeyRejected)
        ));
    }

    #[tokio::test]
    async fn test_delete_node_enqueues_pending_deletions_for_entire_subtree() {
        let temp = tempdir().expect("tempdir should be created");
        let db_path = temp.path().join("manifest.db");
        let key = [5; 32];
        let store =
            SqlCipherMetadataStore::create(&db_path, &key, Uuid::new_v4(), 4_194_304, false)
                .await
                .expect("store should be created");

        let root_id = Uuid::new_v4();
        let child_dir_id = Uuid::new_v4();
        let grandchild_dir_id = Uuid::new_v4();
        let child_file_id = Uuid::new_v4();
        let grandchild_file_id = Uuid::new_v4();
        let sibling_file_id = Uuid::new_v4();

        store
            .insert_node(&directory_node(root_id, None, "root"))
            .await
            .expect("root should insert");
        store
            .insert_node(&directory_node(child_dir_id, Some(root_id), "child"))
            .await
            .expect("child dir should insert");
        store
            .insert_node(&directory_node(
                grandchild_dir_id,
                Some(child_dir_id),
                "grandchild",
            ))
            .await
            .expect("grandchild dir should insert");
        store
            .insert_node(&file_node(child_file_id, Some(child_dir_id), "child.txt"))
            .await
            .expect("child file should insert");
        store
            .insert_node(&file_node(
                grandchild_file_id,
                Some(grandchild_dir_id),
                "grandchild.txt",
            ))
            .await
            .expect("grandchild file should insert");
        store
            .insert_node(&file_node(sibling_file_id, Some(root_id), "sibling.txt"))
            .await
            .expect("sibling file should insert");

        store
            .insert_chunks(&[
                chunk_for(child_file_id, 0, "11111111-1111-4111-8111-111111111111"),
                chunk_for(
                    grandchild_file_id,
                    0,
                    "22222222-2222-4222-8222-222222222222",
                ),
                chunk_for(sibling_file_id, 0, "33333333-3333-4333-8333-333333333333"),
            ])
            .await
            .expect("chunks should insert");

        store
            .delete_node(child_dir_id)
            .await
            .expect("subtree delete should succeed");

        assert!(matches!(
            store.get_node(child_dir_id).await,
            Err(StorageError::NotFound)
        ));
        assert!(matches!(
            store.get_node(grandchild_dir_id).await,
            Err(StorageError::NotFound)
        ));
        assert!(matches!(
            store.get_node(child_file_id).await,
            Err(StorageError::NotFound)
        ));
        assert!(matches!(
            store.get_node(grandchild_file_id).await,
            Err(StorageError::NotFound)
        ));
        assert!(store.get_node(sibling_file_id).await.is_ok());

        let pending = store
            .list_pending_deletions(10)
            .await
            .expect("pending deletions should list");
        assert_eq!(
            pending,
            vec![
                "11111111-1111-4111-8111-111111111111".to_owned(),
                "22222222-2222-4222-8222-222222222222".to_owned(),
            ]
        );
    }

    #[tokio::test]
    async fn test_insert_node_rejects_file_parent_constraint_violation() {
        let temp = tempdir().expect("tempdir should be created");
        let db_path = temp.path().join("manifest.db");
        let key = [5; 32];
        let store =
            SqlCipherMetadataStore::create(&db_path, &key, Uuid::new_v4(), 4_194_304, false)
                .await
                .expect("store should be created");

        let file_parent_id = Uuid::new_v4();
        let child_id = Uuid::new_v4();

        store
            .insert_node(&file_node(file_parent_id, None, "file-parent.txt"))
            .await
            .expect("file parent should insert");

        let result = store
            .insert_node(&file_node(child_id, Some(file_parent_id), "child.txt"))
            .await;

        assert!(matches!(
            result,
            Err(StorageError::ConstraintViolation(message)) if message.contains("directory")
        ));
    }

    #[tokio::test]
    async fn test_insert_node_rejects_self_parent_constraint_violation() {
        let temp = tempdir().expect("tempdir should be created");
        let db_path = temp.path().join("manifest.db");
        let key = [5; 32];
        let store =
            SqlCipherMetadataStore::create(&db_path, &key, Uuid::new_v4(), 4_194_304, false)
                .await
                .expect("store should be created");
        let node_id = Uuid::new_v4();

        let result = store
            .insert_node(&directory_node(node_id, Some(node_id), "self-parent"))
            .await;

        assert!(matches!(
            result,
            Err(StorageError::ConstraintViolation(message)) if message.contains("own parent")
        ));
    }

    #[tokio::test]
    async fn test_insert_node_file_without_wrapped_key_returns_constraint_violation() {
        let temp = tempdir().expect("tempdir should be created");
        let db_path = temp.path().join("manifest.db");
        let key = [5; 32];
        let store =
            SqlCipherMetadataStore::create(&db_path, &key, Uuid::new_v4(), 4_194_304, false)
                .await
                .expect("store should be created");
        let file_without_key = Node::new(
            Uuid::new_v4(),
            None,
            NodeType::File,
            "missing-key.txt".to_owned(),
            1,
            1,
            42,
            None,
        );

        let result = store.insert_node(&file_without_key).await;

        assert!(matches!(
            result,
            Err(StorageError::ConstraintViolation(message)) if message.contains("file_key_wrapped")
        ));
    }

    #[tokio::test]
    async fn test_insert_node_directory_with_wrapped_key_returns_constraint_violation() {
        let temp = tempdir().expect("tempdir should be created");
        let db_path = temp.path().join("manifest.db");
        let key = [5; 32];
        let store =
            SqlCipherMetadataStore::create(&db_path, &key, Uuid::new_v4(), 4_194_304, false)
                .await
                .expect("store should be created");
        let directory_with_key = Node::new(
            Uuid::new_v4(),
            None,
            NodeType::Directory,
            "bad-dir".to_owned(),
            1,
            1,
            0,
            Some([4; 72]),
        );

        let result = store.insert_node(&directory_with_key).await;

        assert!(matches!(
            result,
            Err(StorageError::ConstraintViolation(message)) if message.contains("file_key_wrapped")
        ));
    }

    #[tokio::test]
    async fn test_insert_chunks_rejects_invalid_blob_name_constraint_violation() {
        let temp = tempdir().expect("tempdir should be created");
        let db_path = temp.path().join("manifest.db");
        let key = [5; 32];
        let store =
            SqlCipherMetadataStore::create(&db_path, &key, Uuid::new_v4(), 4_194_304, false)
                .await
                .expect("store should be created");
        let file_id = Uuid::new_v4();
        store
            .insert_node(&file_node(file_id, None, "file.txt"))
            .await
            .expect("file should insert");

        let result = store
            .insert_chunks(&[chunk_for(file_id, 0, "not-a-uuid")])
            .await;
        assert!(matches!(
            result,
            Err(StorageError::ConstraintViolation(message)) if message.contains("blob_name")
        ));
    }

    #[tokio::test]
    async fn test_insert_chunks_rejects_non_v4_blob_name_constraint_violation() {
        let temp = tempdir().expect("tempdir should be created");
        let db_path = temp.path().join("manifest.db");
        let key = [5; 32];
        let store =
            SqlCipherMetadataStore::create(&db_path, &key, Uuid::new_v4(), 4_194_304, false)
                .await
                .expect("store should be created");
        let file_id = Uuid::new_v4();
        store
            .insert_node(&file_node(file_id, None, "file.txt"))
            .await
            .expect("file should insert");

        let result = store
            .insert_chunks(&[chunk_for(
                file_id,
                0,
                "f81d4fae-7dec-11d0-a765-00a0c91e6bf6",
            )])
            .await;
        assert!(matches!(
            result,
            Err(StorageError::ConstraintViolation(message)) if message.contains("UUID v4")
        ));
    }

    #[tokio::test]
    async fn test_insert_chunks_rejects_missing_node_constraint_violation() {
        let temp = tempdir().expect("tempdir should be created");
        let db_path = temp.path().join("manifest.db");
        let key = [5; 32];
        let store =
            SqlCipherMetadataStore::create(&db_path, &key, Uuid::new_v4(), 4_194_304, false)
                .await
                .expect("store should be created");

        let missing_node_id = Uuid::new_v4();
        let result = store
            .insert_chunks(&[chunk_for(
                missing_node_id,
                0,
                "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
            )])
            .await;
        assert!(matches!(
            result,
            Err(StorageError::ConstraintViolation(message)) if message.contains("missing")
        ));
    }

    #[tokio::test]
    async fn test_insert_chunks_rejects_directory_node_constraint_violation() {
        let temp = tempdir().expect("tempdir should be created");
        let db_path = temp.path().join("manifest.db");
        let key = [5; 32];
        let store =
            SqlCipherMetadataStore::create(&db_path, &key, Uuid::new_v4(), 4_194_304, false)
                .await
                .expect("store should be created");
        let directory_id = Uuid::new_v4();
        store
            .insert_node(&directory_node(directory_id, None, "dir"))
            .await
            .expect("directory should insert");

        let result = store
            .insert_chunks(&[chunk_for(
                directory_id,
                0,
                "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb",
            )])
            .await;
        assert!(matches!(
            result,
            Err(StorageError::ConstraintViolation(message)) if message.contains("directory")
        ));
    }

    #[tokio::test]
    async fn test_insert_chunks_rejects_size_padded_mismatch_constraint_violation() {
        let temp = tempdir().expect("tempdir should be created");
        let db_path = temp.path().join("manifest.db");
        let key = [5; 32];
        let store =
            SqlCipherMetadataStore::create(&db_path, &key, Uuid::new_v4(), 4_194_304, false)
                .await
                .expect("store should be created");
        let file_id = Uuid::new_v4();
        store
            .insert_node(&file_node(file_id, None, "file.txt"))
            .await
            .expect("file should insert");
        let mut chunk = chunk_for(file_id, 0, "cccccccc-cccc-4ccc-8ccc-cccccccccccc");
        chunk.size_padded = 2048;

        let result = store.insert_chunks(&[chunk]).await;
        assert!(matches!(
            result,
            Err(StorageError::ConstraintViolation(message)) if message.contains("size_padded")
        ));
    }

    #[tokio::test]
    async fn test_insert_file_with_chunks_rejects_bad_chunk_and_leaves_no_partial_manifest_entry() {
        let temp = tempdir().expect("tempdir should be created");
        let db_path = temp.path().join("manifest.db");
        let key = [5; 32];
        let store =
            SqlCipherMetadataStore::create(&db_path, &key, Uuid::new_v4(), 4_194_304, false)
                .await
                .expect("store should be created");
        let file_id = Uuid::new_v4();
        let mut bad_chunk = chunk_for(file_id, 1, "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb");
        bad_chunk.size_padded = 1234;

        let result = store
            .insert_file_with_chunks(
                &file_node(file_id, None, "partial.txt"),
                &[
                    chunk_for(file_id, 0, "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa"),
                    bad_chunk,
                ],
            )
            .await;

        assert!(matches!(
            result,
            Err(StorageError::ConstraintViolation(message)) if message.contains("size_padded")
        ));
        assert!(matches!(
            store.get_node(file_id).await,
            Err(StorageError::NotFound)
        ));
    }

    #[tokio::test]
    async fn test_insert_file_with_chunks_chunk_node_mismatch_rolls_back_manifest_changes() {
        let temp = tempdir().expect("tempdir should be created");
        let db_path = temp.path().join("manifest.db");
        let key = [5; 32];
        let store =
            SqlCipherMetadataStore::create(&db_path, &key, Uuid::new_v4(), 4_194_304, false)
                .await
                .expect("store should be created");
        let file_id = Uuid::new_v4();
        let other_file_id = Uuid::new_v4();

        let result = store
            .insert_file_with_chunks(
                &file_node(file_id, None, "mismatch.txt"),
                &[
                    chunk_for(file_id, 0, "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa"),
                    chunk_for(other_file_id, 1, "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb"),
                ],
            )
            .await;

        assert!(matches!(
            result,
            Err(StorageError::ConstraintViolation(message))
                if message == "chunk node_id does not match inserted file node"
        ));
        assert!(matches!(
            store.get_node(file_id).await,
            Err(StorageError::NotFound)
        ));
        assert!(
            store
                .get_chunks(file_id)
                .await
                .expect("chunks query should succeed")
                .is_empty()
        );
        assert!(
            store
                .list_pending_deletions(10)
                .await
                .expect("pending deletions query should succeed")
                .is_empty()
        );
    }

    #[tokio::test]
    async fn test_delete_node_transaction_commits_pending_deletions_and_cascades() {
        let temp = tempdir().expect("tempdir should be created");
        let db_path = temp.path().join("manifest.db");
        let key = [5; 32];
        let store =
            SqlCipherMetadataStore::create(&db_path, &key, Uuid::new_v4(), 4_194_304, false)
                .await
                .expect("store should be created");
        let file_id = Uuid::new_v4();
        let expected_blob_names = vec![
            "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa".to_owned(),
            "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb".to_owned(),
        ];
        store
            .insert_file_with_chunks(
                &file_node(file_id, None, "delete-me.txt"),
                &[
                    chunk_for(file_id, 0, &expected_blob_names[0]),
                    chunk_for(file_id, 1, &expected_blob_names[1]),
                ],
            )
            .await
            .expect("file and chunks should insert");

        store
            .delete_node(file_id)
            .await
            .expect("delete should succeed");

        assert!(matches!(
            store.get_node(file_id).await,
            Err(StorageError::NotFound)
        ));
        assert_eq!(
            store
                .get_chunks(file_id)
                .await
                .expect("chunks should load after delete"),
            Vec::<ChunkRecord>::new()
        );
        assert_eq!(
            store
                .list_pending_deletions(10)
                .await
                .expect("pending deletions should load"),
            expected_blob_names
        );
    }

    #[tokio::test]
    async fn test_delete_node_missing_node_keeps_pending_deletions_unchanged() {
        let temp = tempdir().expect("tempdir should be created");
        let db_path = temp.path().join("manifest.db");
        let key = [5; 32];
        let store =
            SqlCipherMetadataStore::create(&db_path, &key, Uuid::new_v4(), 4_194_304, false)
                .await
                .expect("store should be created");
        let seeded_blob = "dddddddd-dddd-4ddd-8ddd-dddddddddddd".to_owned();
        let seeded_blob_for_insert = seeded_blob.clone();
        store
            .with_connection_blocking(move |conn| {
                conn.execute(
                    "INSERT INTO pending_deletions (blob_name, queued_at) VALUES (?1, ?2)",
                    params![seeded_blob_for_insert, 5i64],
                )
                .map_err(StorageError::from_rusqlite)?;
                Ok(())
            })
            .await
            .expect("pending deletions should seed");

        let result = store.delete_node(Uuid::new_v4()).await;

        assert!(matches!(result, Err(StorageError::NotFound)));
        assert_eq!(
            store
                .list_pending_deletions(10)
                .await
                .expect("pending deletions should load"),
            vec![seeded_blob]
        );
    }

    #[tokio::test]
    async fn test_create_existing_database_returns_database_error() {
        let temp = tempdir().expect("tempdir should be created");
        let db_path = temp.path().join("manifest.db");
        let key = [5; 32];
        let initial_vault_id = Uuid::new_v4();
        SqlCipherMetadataStore::create(&db_path, &key, initial_vault_id, 4_194_304, false)
            .await
            .expect("initial store should be created");

        let result =
            SqlCipherMetadataStore::create(&db_path, &key, Uuid::new_v4(), 8_388_608, true).await;
        assert!(matches!(
            result,
            Err(StorageError::Database(message)) if message.contains("already exists")
        ));
    }

    #[tokio::test]
    async fn test_move_node_rejects_cycle_constraint_violation() {
        let temp = tempdir().expect("tempdir should be created");
        let db_path = temp.path().join("manifest.db");
        let key = [5; 32];
        let store =
            SqlCipherMetadataStore::create(&db_path, &key, Uuid::new_v4(), 4_194_304, false)
                .await
                .expect("store should be created");

        let root_id = Uuid::new_v4();
        let child_id = Uuid::new_v4();
        let grandchild_id = Uuid::new_v4();
        store
            .insert_node(&directory_node(root_id, None, "root"))
            .await
            .expect("root should insert");
        store
            .insert_node(&directory_node(child_id, Some(root_id), "child"))
            .await
            .expect("child should insert");
        store
            .insert_node(&directory_node(grandchild_id, Some(child_id), "grandchild"))
            .await
            .expect("grandchild should insert");

        let result = store.move_node(root_id, Some(grandchild_id), 2).await;

        assert!(matches!(
            result,
            Err(StorageError::ConstraintViolation(message)) if message.contains("cycle")
        ));
    }

    #[tokio::test]
    async fn test_move_node_rejects_file_parent_constraint_violation() {
        let temp = tempdir().expect("tempdir should be created");
        let db_path = temp.path().join("manifest.db");
        let key = [5; 32];
        let store =
            SqlCipherMetadataStore::create(&db_path, &key, Uuid::new_v4(), 4_194_304, false)
                .await
                .expect("store should be created");

        let root_id = Uuid::new_v4();
        let destination_file_id = Uuid::new_v4();
        let moving_id = Uuid::new_v4();
        store
            .insert_node(&directory_node(root_id, None, "root"))
            .await
            .expect("root should insert");
        store
            .insert_node(&file_node(
                destination_file_id,
                Some(root_id),
                "destination.txt",
            ))
            .await
            .expect("destination file should insert");
        store
            .insert_node(&directory_node(moving_id, Some(root_id), "moving"))
            .await
            .expect("moving node should insert");

        let result = store
            .move_node(moving_id, Some(destination_file_id), 3)
            .await;
        assert!(matches!(
            result,
            Err(StorageError::ConstraintViolation(message)) if message.contains("directory")
        ));
    }

    #[tokio::test]
    async fn test_list_children_parent_with_unsorted_names_returns_name_sorted_children() {
        let temp = tempdir().expect("tempdir should be created");
        let db_path = temp.path().join("manifest.db");
        let key = [5; 32];
        let store =
            SqlCipherMetadataStore::create(&db_path, &key, Uuid::new_v4(), 4_194_304, false)
                .await
                .expect("store should be created");
        let root_id = Uuid::new_v4();
        let child_a_id = Uuid::new_v4();
        let child_b_id = Uuid::new_v4();
        let grandchild_id = Uuid::new_v4();
        store
            .insert_node(&directory_node(root_id, None, "root"))
            .await
            .expect("root should insert");
        store
            .insert_node(&file_node(child_b_id, Some(root_id), "b-file"))
            .await
            .expect("child b should insert");
        store
            .insert_node(&directory_node(child_a_id, Some(root_id), "a-dir"))
            .await
            .expect("child a should insert");
        store
            .insert_node(&file_node(grandchild_id, Some(child_a_id), "nested"))
            .await
            .expect("nested should insert");

        let children = store
            .list_children(root_id)
            .await
            .expect("children should list");
        let names = children
            .into_iter()
            .map(|node| node.name)
            .collect::<Vec<_>>();

        assert_eq!(names, vec!["a-dir".to_owned(), "b-file".to_owned()]);
    }

    #[tokio::test]
    async fn test_rename_node_existing_node_updates_name_and_modified_at() {
        let temp = tempdir().expect("tempdir should be created");
        let db_path = temp.path().join("manifest.db");
        let key = [5; 32];
        let store =
            SqlCipherMetadataStore::create(&db_path, &key, Uuid::new_v4(), 4_194_304, false)
                .await
                .expect("store should be created");
        let node_id = Uuid::new_v4();
        store
            .insert_node(&file_node(node_id, None, "before.txt"))
            .await
            .expect("node should insert");

        store
            .rename_node(node_id, "after.txt", 77)
            .await
            .expect("rename should succeed");

        let renamed = store.get_node(node_id).await.expect("node should load");
        assert_eq!(renamed.name, "after.txt");
        assert_eq!(renamed.modified_at, 77);
    }

    #[tokio::test]
    async fn test_move_node_existing_node_updates_parent_and_modified_at() {
        let temp = tempdir().expect("tempdir should be created");
        let db_path = temp.path().join("manifest.db");
        let key = [5; 32];
        let store =
            SqlCipherMetadataStore::create(&db_path, &key, Uuid::new_v4(), 4_194_304, false)
                .await
                .expect("store should be created");
        let root_id = Uuid::new_v4();
        let parent_a_id = Uuid::new_v4();
        let parent_b_id = Uuid::new_v4();
        let moving_id = Uuid::new_v4();
        store
            .insert_node(&directory_node(root_id, None, "root"))
            .await
            .expect("root should insert");
        store
            .insert_node(&directory_node(parent_a_id, Some(root_id), "a"))
            .await
            .expect("parent a should insert");
        store
            .insert_node(&directory_node(parent_b_id, Some(root_id), "b"))
            .await
            .expect("parent b should insert");
        store
            .insert_node(&file_node(moving_id, Some(parent_a_id), "moving.txt"))
            .await
            .expect("moving should insert");

        store
            .move_node(moving_id, Some(parent_b_id), 88)
            .await
            .expect("move should succeed");

        let moved = store.get_node(moving_id).await.expect("node should load");
        assert_eq!(moved.parent_id.map(|id| *id.as_uuid()), Some(parent_b_id));
        assert_eq!(moved.modified_at, 88);
    }

    #[tokio::test]
    async fn test_insert_node_zero_byte_file_accepts_and_round_trips() {
        let temp = tempdir().expect("tempdir should be created");
        let db_path = temp.path().join("manifest.db");
        let key = [5; 32];
        let store =
            SqlCipherMetadataStore::create(&db_path, &key, Uuid::new_v4(), 4_194_304, false)
                .await
                .expect("store should be created");
        let node_id = Uuid::new_v4();
        let zero_byte_file = Node::new(
            node_id,
            None,
            NodeType::File,
            "empty.txt".to_owned(),
            1,
            1,
            0,
            Some([3; 72]),
        );

        store
            .insert_node(&zero_byte_file)
            .await
            .expect("zero-byte file should insert");

        let stored = store.get_node(node_id).await.expect("file should load");
        assert_eq!(stored.size_bytes, 0);
        assert!(matches!(stored.node_type, NodeType::File));
    }

    #[tokio::test]
    async fn test_set_meta_rejects_immutable_keys_and_allows_mutable_key() {
        let temp = tempdir().expect("tempdir should be created");
        let db_path = temp.path().join("manifest.db");
        let key = [5; 32];
        let store =
            SqlCipherMetadataStore::create(&db_path, &key, Uuid::new_v4(), 4_194_304, false)
                .await
                .expect("store should be created");

        let immutable_result = store.set_meta("chunk_size_bytes", "8192").await;
        assert!(matches!(
            immutable_result,
            Err(StorageError::ConstraintViolation(message)) if message.contains("immutable")
        ));

        store
            .set_meta("last_synced_at", "123")
            .await
            .expect("mutable key should succeed");
        assert_eq!(
            store
                .get_meta("last_synced_at")
                .await
                .expect("value should be readable"),
            Some("123".to_owned())
        );
    }

    #[tokio::test]
    async fn test_set_meta_rejects_snapshot_counter_key() {
        let temp = tempdir().expect("tempdir should be created");
        let db_path = temp.path().join("manifest.db");
        let key = [5; 32];
        let store =
            SqlCipherMetadataStore::create(&db_path, &key, Uuid::new_v4(), 4_194_304, false)
                .await
                .expect("store should be created");

        let result = store.set_meta("snapshot_counter", "99").await;
        assert!(matches!(
            result,
            Err(StorageError::ConstraintViolation(message)) if message.contains("snapshot_counter")
        ));
    }

    #[tokio::test]
    async fn test_increment_snapshot_counter_reports_invalid_value() {
        let temp = tempdir().expect("tempdir should be created");
        let db_path = temp.path().join("manifest.db");
        let key = [5; 32];
        let store =
            SqlCipherMetadataStore::create(&db_path, &key, Uuid::new_v4(), 4_194_304, false)
                .await
                .expect("store should be created");

        store
            .with_connection_blocking(move |conn| {
                conn.execute(
                    "UPDATE manifest_meta SET value = 'invalid' WHERE key = 'snapshot_counter'",
                    [],
                )
                .map_err(StorageError::from_rusqlite)?;
                Ok(())
            })
            .await
            .expect("test setup should update snapshot_counter");

        let result = store.increment_snapshot_counter().await;
        assert!(matches!(
            result,
            Err(StorageError::Database(message)) if message.contains("snapshot_counter")
        ));
    }

    #[tokio::test]
    async fn test_list_sync_chunks_returns_empty_when_no_rows() {
        let temp = tempdir().expect("tempdir should be created");
        let db_path = temp.path().join("manifest.db");
        let key = [5; 32];
        let store =
            SqlCipherMetadataStore::create(&db_path, &key, Uuid::new_v4(), 4_194_304, false)
                .await
                .expect("store should be created");

        let rows = store
            .list_sync_chunks()
            .await
            .expect("list_sync_chunks should succeed");

        assert!(rows.is_empty());
    }

    #[tokio::test]
    async fn test_list_sync_chunks_returns_alphabetical_order() {
        let temp = tempdir().expect("tempdir should be created");
        let db_path = temp.path().join("manifest.db");
        let key = [5; 32];
        let store =
            SqlCipherMetadataStore::create(&db_path, &key, Uuid::new_v4(), 4_194_304, false)
                .await
                .expect("store should be created");
        let file_id = Uuid::new_v4();
        store
            .insert_node(&file_node(file_id, None, "file.txt"))
            .await
            .expect("file should insert");
        store
            .insert_chunks(&[
                chunk_for(file_id, 0, "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb"),
                chunk_for(file_id, 1, "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa"),
            ])
            .await
            .expect("chunks should insert");

        let rows = store
            .list_sync_chunks()
            .await
            .expect("list_sync_chunks should succeed");

        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].blob_name, "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa");
        assert_eq!(rows[1].blob_name, "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb");
    }

    #[tokio::test]
    async fn test_list_sync_chunks_includes_blake3_checksum_bytes() {
        let temp = tempdir().expect("tempdir should be created");
        let db_path = temp.path().join("manifest.db");
        let key = [5; 32];
        let store =
            SqlCipherMetadataStore::create(&db_path, &key, Uuid::new_v4(), 4_194_304, false)
                .await
                .expect("store should be created");
        let file_id = Uuid::new_v4();
        store
            .insert_node(&file_node(file_id, None, "file.txt"))
            .await
            .expect("file should insert");
        let mut chunk = chunk_for(file_id, 0, "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa");
        chunk.blake3_checksum = [0xAB; 32];
        store
            .insert_chunks(&[chunk])
            .await
            .expect("chunk should insert");

        let rows = store
            .list_sync_chunks()
            .await
            .expect("list_sync_chunks should succeed");

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].blake3_checksum, [0xAB; 32]);
    }

    #[tokio::test]
    async fn test_rollback_snapshot_counter_restores_previous_value() {
        let temp = tempdir().expect("tempdir should be created");
        let db_path = temp.path().join("manifest.db");
        let key = [5; 32];
        let store =
            SqlCipherMetadataStore::create(&db_path, &key, Uuid::new_v4(), 4_194_304, false)
                .await
                .expect("store should be created");
        let previous_value = store
            .increment_snapshot_counter()
            .await
            .expect("first increment should succeed");
        let _ = store
            .increment_snapshot_counter()
            .await
            .expect("second increment should succeed");

        store
            .rollback_snapshot_counter(previous_value)
            .await
            .expect("rollback should succeed");

        assert_eq!(
            store
                .get_meta("snapshot_counter")
                .await
                .expect("meta should load"),
            Some(previous_value.to_string())
        );
    }

    #[tokio::test]
    async fn test_rollback_snapshot_counter_rejects_when_precondition_violated() {
        let temp = tempdir().expect("tempdir should be created");
        let db_path = temp.path().join("manifest.db");
        let key = [5; 32];
        let store =
            SqlCipherMetadataStore::create(&db_path, &key, Uuid::new_v4(), 4_194_304, false)
                .await
                .expect("store should be created");
        let _ = store
            .increment_snapshot_counter()
            .await
            .expect("increment should succeed");

        let result = store.rollback_snapshot_counter(3).await;

        assert!(matches!(
            result,
            Err(StorageError::Database(message)) if message.contains("precondition")
        ));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_rollback_snapshot_counter_concurrent_with_increment_fails_one_side() {
        let temp = tempdir().expect("tempdir should be created");
        let db_path = temp.path().join("manifest.db");
        let key = [5; 32];
        let store = Arc::new(
            SqlCipherMetadataStore::create(&db_path, &key, Uuid::new_v4(), 4_194_304, false)
                .await
                .expect("store should be created"),
        );
        let _ = store
            .increment_snapshot_counter()
            .await
            .expect("first increment should succeed");
        let _ = store
            .increment_snapshot_counter()
            .await
            .expect("second increment should succeed");

        let rollback_store = Arc::clone(&store);
        let increment_store = Arc::clone(&store);
        let rollback_task =
            tokio::spawn(async move { rollback_store.rollback_snapshot_counter(0).await });
        let increment_task =
            tokio::spawn(async move { increment_store.increment_snapshot_counter().await });

        let rollback_result = rollback_task.await.expect("rollback task should complete");
        let increment_result = increment_task
            .await
            .expect("increment task should complete");

        assert!(rollback_result.is_err());
        assert!(increment_result.is_ok());
    }

    #[tokio::test]
    async fn test_list_pending_deletions_limit_returns_only_requested_entries() {
        let temp = tempdir().expect("tempdir should be created");
        let db_path = temp.path().join("manifest.db");
        let key = [5; 32];
        let store =
            SqlCipherMetadataStore::create(&db_path, &key, Uuid::new_v4(), 4_194_304, false)
                .await
                .expect("store should be created");

        store
            .with_connection_blocking(move |conn| {
                conn.execute(
                    "INSERT INTO pending_deletions (blob_name, queued_at) VALUES (?1, ?2)",
                    params!["cccccccc-cccc-4ccc-8ccc-cccccccccccc", 20i64],
                )
                .map_err(StorageError::from_rusqlite)?;
                conn.execute(
                    "INSERT INTO pending_deletions (blob_name, queued_at) VALUES (?1, ?2)",
                    params!["bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb", 10i64],
                )
                .map_err(StorageError::from_rusqlite)?;
                conn.execute(
                    "INSERT INTO pending_deletions (blob_name, queued_at) VALUES (?1, ?2)",
                    params!["aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa", 10i64],
                )
                .map_err(StorageError::from_rusqlite)?;
                Ok(())
            })
            .await
            .expect("pending rows should seed");

        let pending = store
            .list_pending_deletions(2)
            .await
            .expect("pending rows should list");

        assert_eq!(
            pending,
            vec![
                "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa".to_owned(),
                "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb".to_owned(),
            ]
        );
    }

    #[tokio::test]
    async fn test_mark_deletion_complete_existing_blob_removes_only_target_blob() {
        let temp = tempdir().expect("tempdir should be created");
        let db_path = temp.path().join("manifest.db");
        let key = [5; 32];
        let store =
            SqlCipherMetadataStore::create(&db_path, &key, Uuid::new_v4(), 4_194_304, false)
                .await
                .expect("store should be created");

        store
            .with_connection_blocking(move |conn| {
                conn.execute(
                    "INSERT INTO pending_deletions (blob_name, queued_at) VALUES (?1, ?2)",
                    params!["aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa", 10i64],
                )
                .map_err(StorageError::from_rusqlite)?;
                conn.execute(
                    "INSERT INTO pending_deletions (blob_name, queued_at) VALUES (?1, ?2)",
                    params!["bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb", 10i64],
                )
                .map_err(StorageError::from_rusqlite)?;
                Ok(())
            })
            .await
            .expect("pending rows should seed");

        store
            .mark_deletion_complete("aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa")
            .await
            .expect("delete completion should succeed");
        let pending = store
            .list_pending_deletions(10)
            .await
            .expect("pending rows should list");

        assert_eq!(
            pending,
            vec!["bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb".to_owned()]
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_increment_snapshot_counter_concurrent_calls_return_unique_sequence() {
        let temp = tempdir().expect("tempdir should be created");
        let db_path = temp.path().join("manifest.db");
        let key = [5; 32];
        let store = Arc::new(
            SqlCipherMetadataStore::create(&db_path, &key, Uuid::new_v4(), 4_194_304, false)
                .await
                .expect("store should be created"),
        );
        let mut tasks = Vec::new();
        for _ in 0..16 {
            let store = Arc::clone(&store);
            tasks.push(tokio::spawn(async move {
                store
                    .increment_snapshot_counter()
                    .await
                    .expect("increment should succeed")
            }));
        }
        let mut values = Vec::new();
        for task in tasks {
            values.push(task.await.expect("task should complete"));
        }
        values.sort_unstable();

        assert_eq!(values, (1u64..=16).collect::<Vec<_>>());
        assert_eq!(
            store
                .get_meta("snapshot_counter")
                .await
                .expect("meta should load"),
            Some("16".to_owned())
        );
    }

    /// Verifies `replace_file_key_and_chunks` atomically replaces chunks and queues old blobs.
    #[tokio::test]
    async fn test_replace_file_key_and_chunks_atomically_replaces_chunks_and_queues_deletions() {
        let temp = tempdir().expect("tempdir should be created");
        let db_path = temp.path().join("manifest.db");
        let key = [5; 32];
        let store =
            SqlCipherMetadataStore::create(&db_path, &key, Uuid::new_v4(), 4_194_304, false)
                .await
                .expect("store should be created");

        let file_id = Uuid::new_v4();
        let old_blob_a = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
        let old_blob_b = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb";
        store
            .insert_file_with_chunks(
                &file_node(file_id, None, "file.txt"),
                &[
                    chunk_for(file_id, 0, old_blob_a),
                    chunk_for(file_id, 1, old_blob_b),
                ],
            )
            .await
            .expect("file and chunks should insert");

        let new_blob_name = format!("shared-copy-{}", Uuid::new_v4().hyphenated());
        let new_chunk = ChunkRecord {
            chunk_id: Uuid::new_v4(),
            node_id: file_id.into(),
            chunk_index: 0,
            blob_name: new_blob_name.clone(),
            size_padded: 4_194_304,
            blake3_checksum: [0xAB; 32],
            epoch_blob_id: None,
            byte_offset: None,
            byte_length: None,
        };
        let new_key: [u8; 72] = [0x55; 72];

        store
            .replace_file_key_and_chunks(file_id, new_key, vec![new_chunk], 9999)
            .await
            .expect("replace_file_key_and_chunks should succeed");

        let pending = store
            .list_pending_deletions(10)
            .await
            .expect("pending_deletions should load");
        assert!(pending.contains(&old_blob_a.to_owned()));
        assert!(pending.contains(&old_blob_b.to_owned()));

        let chunks = store.get_chunks(file_id).await.expect("chunks should load");
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].blob_name, new_blob_name);

        let node = store.get_node(file_id).await.expect("node should load");
        assert_eq!(node.file_key_wrapped, Some(new_key));
    }

    /// Verifies `replace_file_key_and_chunks` returns `NotFound` when the file node is absent.
    #[tokio::test]
    async fn test_replace_file_key_and_chunks_on_missing_node_returns_not_found() {
        let temp = tempdir().expect("tempdir should be created");
        let db_path = temp.path().join("manifest.db");
        let key = [5; 32];
        let store =
            SqlCipherMetadataStore::create(&db_path, &key, Uuid::new_v4(), 4_194_304, false)
                .await
                .expect("store should be created");

        let result = store
            .replace_file_key_and_chunks(Uuid::new_v4(), [0x11; 72], vec![], 1234)
            .await;

        assert!(matches!(result, Err(StorageError::NotFound)));
    }

    /// Verifies AuthUserStore::get_user returns vault when vault exists in manifest_meta.
    #[tokio::test]
    async fn test_auth_user_store_get_user_returns_vault_when_exists() {
        use crate::auth::AuthUserStore;

        let temp = tempdir().expect("tempdir should be created");
        let db_path = temp.path().join("manifest.db");
        let key = [5; 32];
        let vault_id_uuid = Uuid::new_v4();
        let vault_id = crate::crypto::VaultId::from_uuid(vault_id_uuid);
        let store = SqlCipherMetadataStore::create(&db_path, &key, vault_id_uuid, 4_194_304, false)
            .await
            .expect("store should be created");

        let result = store.get_user(&vault_id).await;

        assert!(result.is_ok());
        let auth_user = result.expect("get_user should succeed");
        assert_eq!(auth_user.vault_id, vault_id);
    }

    /// Verifies AuthUserStore::get_user returns NotFound when vault_id is wrong.
    #[tokio::test]
    async fn test_auth_user_store_get_user_returns_not_found_on_wrong_vault_id() {
        use crate::auth::AuthUserStore;

        let temp = tempdir().expect("tempdir should be created");
        let db_path = temp.path().join("manifest.db");
        let key = [5; 32];
        let vault_id_uuid = Uuid::new_v4();
        let wrong_vault_id = crate::crypto::VaultId::from_uuid(Uuid::new_v4());
        let store = SqlCipherMetadataStore::create(&db_path, &key, vault_id_uuid, 4_194_304, false)
            .await
            .expect("store should be created");

        let result = store.get_user(&wrong_vault_id).await;

        assert!(matches!(result, Err(StorageError::NotFound)));
    }

    /// Verifies AuthUserStore::create_vault_user succeeds (placeholder implementation).
    #[tokio::test]
    async fn test_auth_user_store_create_vault_user_succeeds() {
        use crate::auth::{AuthUser, AuthUserStore};

        let temp = tempdir().expect("tempdir should be created");
        let db_path = temp.path().join("manifest.db");
        let key = [5; 32];
        let vault_id_uuid = Uuid::new_v4();
        let vault_id = crate::crypto::VaultId::from_uuid(vault_id_uuid);
        let store = SqlCipherMetadataStore::create(&db_path, &key, vault_id_uuid, 4_194_304, false)
            .await
            .expect("store should be created");

        let auth_user = AuthUser {
            vault_id,
            salt: [0x11; 16],
            key_file_hash: Some([0x22; 32]),
        };

        let result = store.create_vault_user(auth_user).await;

        assert!(result.is_ok());
    }

    /// Verifies AuthUserStore::update_password succeeds (placeholder implementation).
    #[tokio::test]
    async fn test_auth_user_store_update_password_succeeds() {
        use crate::auth::AuthUserStore;

        let temp = tempdir().expect("tempdir should be created");
        let db_path = temp.path().join("manifest.db");
        let key = [5; 32];
        let vault_id_uuid = Uuid::new_v4();
        let vault_id = crate::crypto::VaultId::from_uuid(vault_id_uuid);
        let store = SqlCipherMetadataStore::create(&db_path, &key, vault_id_uuid, 4_194_304, false)
            .await
            .expect("store should be created");

        let result = store.update_password(&vault_id, [0x33; 16]).await;

        assert!(result.is_ok());
    }

    /// Verifies AuthUserStore::rotate_key_file succeeds (placeholder implementation).
    #[tokio::test]
    async fn test_auth_user_store_rotate_key_file_succeeds() {
        use crate::auth::AuthUserStore;

        let temp = tempdir().expect("tempdir should be created");
        let db_path = temp.path().join("manifest.db");
        let key = [5; 32];
        let vault_id_uuid = Uuid::new_v4();
        let vault_id = crate::crypto::VaultId::from_uuid(vault_id_uuid);
        let store = SqlCipherMetadataStore::create(&db_path, &key, vault_id_uuid, 4_194_304, false)
            .await
            .expect("store should be created");

        let result = store.rotate_key_file(&vault_id, [0x44; 32]).await;

        assert!(result.is_ok());
    }
}
