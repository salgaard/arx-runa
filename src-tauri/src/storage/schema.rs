//! Canonical SQLCipher schema and manifest-meta helpers.

use rusqlite::{Connection, OptionalExtension, params};
use uuid::Uuid;

use crate::storage::error::StorageError;
use crate::storage::validation::parse_chunk_size_bytes;

/// Canonical SQL schema for Phase 3 manifest storage.
pub(crate) const CANONICAL_SCHEMA: &str = r#"
-- Phase 3 core
CREATE TABLE nodes (
    node_id          TEXT PRIMARY KEY,     -- UUID v4
    parent_id        TEXT REFERENCES nodes(node_id) ON DELETE CASCADE,
    node_type        TEXT NOT NULL         -- 'file' or 'directory'
                         CHECK (node_type IN ('file', 'directory')),
    name             TEXT NOT NULL,        -- plaintext (SQLCipher is the encryption layer)
    created_at       INTEGER NOT NULL,     -- Unix timestamp
    modified_at      INTEGER NOT NULL,     -- Unix timestamp
    size_bytes       INTEGER NOT NULL,     -- original file size (0 for directories)
    file_key_wrapped BLOB                  -- file_key encrypted with key_encryption_key
                                           -- NULL for directories, NOT NULL for files
                         CHECK ((node_type = 'file'      AND file_key_wrapped IS NOT NULL)
                             OR (node_type = 'directory' AND file_key_wrapped IS NULL))
);

CREATE TABLE chunks (
    chunk_id         TEXT PRIMARY KEY,     -- UUID v4
    node_id          TEXT NOT NULL REFERENCES nodes(node_id) ON DELETE CASCADE,
    chunk_index      INTEGER NOT NULL,     -- 0-based
    blob_name        TEXT NOT NULL,        -- UUID v4, no relation to file identity
    size_padded      INTEGER NOT NULL,     -- always equals configured chunk_size_bytes
                                           -- default chunk_size_bytes is 4 MiB (4194304)
    blake3_checksum  BLOB NOT NULL,        -- 32 bytes, over encrypted blob
    UNIQUE(node_id, chunk_index),
    UNIQUE(blob_name)
);

CREATE TABLE manifest_meta (
    key              TEXT PRIMARY KEY,
    value            TEXT NOT NULL
);
-- Initial rows:
-- ('schema_version', '1')
-- ('vault_id', '<uuid>')
-- ('snapshot_counter', '0')
-- last_synced_at is not seeded; set on first successful push
-- ('chunk_size_bytes', '4194304')   -- immutable; validated on every open
-- ('epoch_buffer_enabled', 'false') -- user opt-in at vault creation

-- Cloud deletion durability queue:
CREATE TABLE pending_deletions (
    blob_name        TEXT PRIMARY KEY,      -- UUID v4 blob name queued for cloud deletion
    queued_at        INTEGER NOT NULL       -- Unix timestamp
);

-- Phase 4 placeholder
-- Destination sessions (Phase 4 multi-destination, included here for schema completeness):
CREATE TABLE destination_sessions (
    destination_id   TEXT PRIMARY KEY,          -- UUID v4
    label            TEXT NOT NULL,             -- human-readable name
    destination_type TEXT NOT NULL              -- 'cloud', 'external_drive', 'local_path'
                         CHECK (destination_type IN ('cloud', 'external_drive', 'local_path')),
    rclone_remote_name TEXT NOT NULL,           -- remote name in the session-lived rclone.conf
    rclone_config_blob TEXT NOT NULL,           -- encrypted Rclone config section (credentials)
    bucket           TEXT NOT NULL DEFAULT '',  -- bucket/container; empty for local paths
    path_prefix      TEXT NOT NULL DEFAULT '',  -- path prefix within the destination
    is_primary       INTEGER NOT NULL DEFAULT 0 CHECK (is_primary IN (0, 1)),
    backup_mode      TEXT                       -- 'mirror' | 'accumulating' | NULL (primary)
                         CHECK (backup_mode IS NULL OR backup_mode IN ('mirror', 'accumulating')),
    created_at       INTEGER NOT NULL
);
-- Constraint: exactly one primary destination per vault (enforced in application logic).

-- Phase 5 placeholder
-- Sharing tables (Phase 5, included here for schema completeness):
CREATE TABLE contacts (
    contact_id       TEXT PRIMARY KEY,
    display_name     TEXT NOT NULL,
    email            TEXT,
    public_key       BLOB NOT NULL,
    created_at       INTEGER NOT NULL
);

CREATE TABLE shares (
    share_id         TEXT PRIMARY KEY,
    file_id          TEXT NOT NULL REFERENCES nodes(node_id),
    contact_id       TEXT NOT NULL REFERENCES contacts(contact_id),
    file_share_id    TEXT NOT NULL,
    cloud_path       TEXT NOT NULL,
    created_at       INTEGER NOT NULL,
    expires_at       INTEGER,             -- NULL = no expiration (Unix timestamp)
    revoked_at       INTEGER
);

CREATE TABLE received_shares (
    share_id             TEXT PRIMARY KEY,
    sender_contact_id    TEXT REFERENCES contacts(contact_id),
    file_name            TEXT NOT NULL,
    file_key_wrapped     BLOB NOT NULL,
    chunk_count          INTEGER NOT NULL,
    chunk_size           INTEGER NOT NULL,
    chunk_uuids          TEXT NOT NULL      -- JSON array of UUID v4 blob names, e.g. ["uuid1","uuid2"]
                             CHECK (json_valid(chunk_uuids)),
    cloud_endpoint       TEXT NOT NULL,
    imported_at          INTEGER NOT NULL
);

-- Phase 2.4 / 5 identity table
CREATE TABLE vault_identity (
    id                   INTEGER PRIMARY KEY CHECK (id = 1),
    public_key           BLOB NOT NULL UNIQUE,
    wrapped_private_key  BLOB NOT NULL
);
"#;

/// Applies the canonical manifest schema to a SQLCipher connection.
pub(crate) fn apply_canonical_schema(conn: &Connection) -> Result<(), StorageError> {
    conn.execute_batch(CANONICAL_SCHEMA)
        .map_err(StorageError::from_rusqlite)
}

/// Verifies the active SQLCipher key by reading `sqlite_master`.
pub(crate) fn verify_sqlcipher_key(conn: &Connection) -> Result<(), StorageError> {
    conn.query_row("SELECT count(*) FROM sqlite_master", [], |_row| Ok(()))
        .map_err(StorageError::from_rusqlite)
}

/// Runs low-level SQLite integrity checks for schema and foreign keys.
pub(crate) fn validate_schema_integrity(conn: &Connection) -> Result<(), StorageError> {
    let integrity_result = conn
        .query_row("PRAGMA integrity_check(1)", [], |row| row.get::<_, String>(0))
        .map_err(StorageError::from_rusqlite)?;
    if integrity_result != "ok" {
        return Err(StorageError::Database(format!(
            "schema integrity check failed: {integrity_result}"
        )));
    }

    let foreign_key_violation = conn
        .query_row("PRAGMA foreign_key_check", [], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, i64>(3)?,
            ))
        })
        .optional()
        .map_err(StorageError::from_rusqlite)?;
    if let Some((table, rowid, parent, fkid)) = foreign_key_violation {
        return Err(StorageError::Database(format!(
            "schema foreign key check failed: table={table}, rowid={rowid}, parent={}, fkid={fkid}",
            parent.unwrap_or_else(|| "unknown".to_owned())
        )));
    }

    Ok(())
}

/// Seeds required `manifest_meta` rows for first-open vault initialization.
pub(crate) fn seed_manifest_meta(
    conn: &Connection,
    vault_id: Uuid,
    chunk_size_bytes: u64,
    epoch_buffer_enabled: bool,
) -> Result<(), StorageError> {
    let epoch_buffer_enabled = if epoch_buffer_enabled { "true" } else { "false" };
    let chunk_size_bytes = chunk_size_bytes.to_string();
    let vault_id = vault_id.hyphenated().to_string();
    conn.execute(
        "INSERT OR IGNORE INTO manifest_meta (key, value) VALUES ('schema_version', '1')",
        [],
    )
    .map_err(StorageError::from_rusqlite)?;
    conn.execute(
        "INSERT OR IGNORE INTO manifest_meta (key, value) VALUES ('snapshot_counter', '0')",
        [],
    )
    .map_err(StorageError::from_rusqlite)?;
    conn.execute(
        "INSERT OR IGNORE INTO manifest_meta (key, value) VALUES ('vault_id', ?1)",
        params![vault_id],
    )
    .map_err(StorageError::from_rusqlite)?;
    conn.execute(
        "INSERT OR IGNORE INTO manifest_meta (key, value) VALUES ('chunk_size_bytes', ?1)",
        params![chunk_size_bytes],
    )
    .map_err(StorageError::from_rusqlite)?;
    conn.execute(
        "INSERT OR IGNORE INTO manifest_meta (key, value) VALUES ('epoch_buffer_enabled', ?1)",
        params![epoch_buffer_enabled],
    )
    .map_err(StorageError::from_rusqlite)?;
    Ok(())
}

/// Validates immutable `manifest_meta` constraints required by design invariants.
pub(crate) fn validate_manifest_meta(conn: &Connection) -> Result<(), StorageError> {
    let schema_version = manifest_meta_value(conn, "schema_version")?;
    if schema_version != "1" {
        return Err(StorageError::Database(
            "invalid schema_version: expected 1".to_owned(),
        ));
    }

    let vault_id = manifest_meta_value(conn, "vault_id")?;
    Uuid::parse_str(&vault_id)
        .map_err(|_| StorageError::Database("invalid vault_id: not a UUID".to_owned()))?;

    let snapshot_counter = manifest_meta_value(conn, "snapshot_counter")?;
    snapshot_counter.parse::<u64>().map_err(|_| {
        StorageError::Database("invalid snapshot_counter: not an unsigned integer".to_owned())
    })?;

    let chunk_size_value = manifest_meta_value(conn, "chunk_size_bytes")?;
    parse_chunk_size_bytes(&chunk_size_value)?;

    let epoch_value = manifest_meta_value(conn, "epoch_buffer_enabled")?;
    if epoch_value != "true" && epoch_value != "false" {
        return Err(StorageError::Database(
            "invalid epoch_buffer_enabled: expected true|false".to_owned(),
        ));
    }

    Ok(())
}

fn manifest_meta_value(conn: &Connection, key: &str) -> Result<String, StorageError> {
    let value = conn
        .query_row(
            "SELECT value FROM manifest_meta WHERE key = ?1",
            params![key],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(StorageError::from_rusqlite)?;
    value.ok_or_else(|| StorageError::Database(format!("missing manifest_meta key: {key}")))
}

#[cfg(test)]
mod tests {
    use rusqlite::{Connection, params};
    use uuid::Uuid;

    use super::{
        apply_canonical_schema, seed_manifest_meta, validate_manifest_meta, validate_schema_integrity,
    };
    use crate::storage::error::StorageError;

    /// Verifies valid seeded manifest metadata passes validation.
    #[test]
    fn test_validate_manifest_meta_accepts_valid_seed() {
        let conn = Connection::open_in_memory().expect("in-memory connection should open");
        apply_canonical_schema(&conn).expect("schema should apply");
        seed_manifest_meta(&conn, Uuid::new_v4(), 4_194_304, false).expect("seed should succeed");

        let result = validate_manifest_meta(&conn);

        assert!(result.is_ok());
    }

    /// Verifies low-level schema checks pass for a valid schema.
    #[test]
    fn test_validate_schema_integrity_accepts_valid_schema() {
        let conn = Connection::open_in_memory().expect("in-memory connection should open");
        apply_canonical_schema(&conn).expect("schema should apply");

        let result = validate_schema_integrity(&conn);

        assert!(result.is_ok());
    }

    /// Verifies low-level schema checks reject foreign-key violations.
    #[test]
    fn test_validate_schema_integrity_rejects_foreign_key_violations() {
        let conn = Connection::open_in_memory().expect("in-memory connection should open");
        apply_canonical_schema(&conn).expect("schema should apply");
        conn.execute_batch("PRAGMA foreign_keys = OFF;")
            .expect("foreign key enforcement should be disabled for setup");
        conn.execute(
            "INSERT INTO chunks (chunk_id, node_id, chunk_index, blob_name, size_padded, blake3_checksum)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                Uuid::new_v4().hyphenated().to_string(),
                Uuid::new_v4().hyphenated().to_string(),
                0i64,
                Uuid::new_v4().hyphenated().to_string(),
                4_194_304i64,
                vec![7u8; 32]
            ],
        )
        .expect("invalid chunk row should insert when foreign keys are disabled");

        let result = validate_schema_integrity(&conn);

        assert!(matches!(result, Err(StorageError::Database(message)) if message.contains("foreign key")));
    }

    /// Verifies out-of-range chunk sizes are rejected.
    #[test]
    fn test_validate_manifest_meta_rejects_out_of_range_chunk_size() {
        let conn = Connection::open_in_memory().expect("in-memory connection should open");
        apply_canonical_schema(&conn).expect("schema should apply");
        seed_manifest_meta(&conn, Uuid::new_v4(), 131_071, false).expect("seed should succeed");

        let result = validate_manifest_meta(&conn);

        assert!(matches!(result, Err(StorageError::Database(message)) if message.contains("chunk_size_bytes")));
    }

    /// Verifies non-boolean epoch flag values are rejected.
    #[test]
    fn test_validate_manifest_meta_rejects_invalid_epoch_flag() {
        let conn = Connection::open_in_memory().expect("in-memory connection should open");
        apply_canonical_schema(&conn).expect("schema should apply");
        seed_manifest_meta(&conn, Uuid::new_v4(), 4_194_304, false).expect("seed should succeed");
        conn.execute(
            "UPDATE manifest_meta SET value = 'invalid' WHERE key = 'epoch_buffer_enabled'",
            [],
        )
        .expect("test setup should update epoch flag");

        let result = validate_manifest_meta(&conn);

        assert!(matches!(result, Err(StorageError::Database(message)) if message.contains("epoch_buffer_enabled")));
    }

    /// Verifies unsupported schema versions are rejected.
    #[test]
    fn test_validate_manifest_meta_rejects_invalid_schema_version() {
        let conn = Connection::open_in_memory().expect("in-memory connection should open");
        apply_canonical_schema(&conn).expect("schema should apply");
        seed_manifest_meta(&conn, Uuid::new_v4(), 4_194_304, false).expect("seed should succeed");
        conn.execute(
            "UPDATE manifest_meta SET value = '2' WHERE key = 'schema_version'",
            [],
        )
        .expect("test setup should update schema_version");

        let result = validate_manifest_meta(&conn);

        assert!(matches!(result, Err(StorageError::Database(message)) if message.contains("schema_version")));
    }

    /// Verifies non-UUID vault identifiers are rejected.
    #[test]
    fn test_validate_manifest_meta_rejects_invalid_vault_id() {
        let conn = Connection::open_in_memory().expect("in-memory connection should open");
        apply_canonical_schema(&conn).expect("schema should apply");
        seed_manifest_meta(&conn, Uuid::new_v4(), 4_194_304, false).expect("seed should succeed");
        conn.execute("UPDATE manifest_meta SET value = 'not-a-uuid' WHERE key = 'vault_id'", [])
            .expect("test setup should update vault_id");

        let result = validate_manifest_meta(&conn);

        assert!(matches!(result, Err(StorageError::Database(message)) if message.contains("vault_id")));
    }

    /// Verifies non-integer snapshot counters are rejected.
    #[test]
    fn test_validate_manifest_meta_rejects_invalid_snapshot_counter() {
        let conn = Connection::open_in_memory().expect("in-memory connection should open");
        apply_canonical_schema(&conn).expect("schema should apply");
        seed_manifest_meta(&conn, Uuid::new_v4(), 4_194_304, false).expect("seed should succeed");
        conn.execute(
            "UPDATE manifest_meta SET value = 'invalid' WHERE key = 'snapshot_counter'",
            [],
        )
        .expect("test setup should update snapshot_counter");

        let result = validate_manifest_meta(&conn);

        assert!(matches!(result, Err(StorageError::Database(message)) if message.contains("snapshot_counter")));
    }

    /// Verifies missing required manifest-meta keys are rejected.
    #[test]
    fn test_validate_manifest_meta_rejects_missing_required_key() {
        let conn = Connection::open_in_memory().expect("in-memory connection should open");
        apply_canonical_schema(&conn).expect("schema should apply");
        seed_manifest_meta(&conn, Uuid::new_v4(), 4_194_304, false).expect("seed should succeed");
        conn.execute("DELETE FROM manifest_meta WHERE key = 'vault_id'", [])
            .expect("test setup should remove vault_id");

        let result = validate_manifest_meta(&conn);

        assert!(matches!(result, Err(StorageError::Database(message)) if message.contains("missing manifest_meta key: vault_id")));
    }
}

