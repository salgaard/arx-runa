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

-- Phase 5 canonical (see docs/architecture/designs/file-sharing/design.md §Database Schema)
-- Sharing tables:
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
    sender_public_key    BLOB NOT NULL,       -- X25519 public key, 32 bytes
    file_id              TEXT NOT NULL,       -- file node identifier (UUID v4)
    file_name            TEXT NOT NULL,
    file_key_wrapped     BLOB NOT NULL,
    chunk_count          INTEGER NOT NULL,
    chunk_size           INTEGER NOT NULL,
    chunk_uuids          TEXT NOT NULL
                             CHECK (json_valid(chunk_uuids)),
    cloud_endpoint       TEXT NOT NULL
                             CHECK (json_valid(cloud_endpoint)),
    expires_at           INTEGER,
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

/// Migrates a schema-version-1 manifest to schema-version-2.
///
/// Idempotent: if `schema_version` is already `'2'`, commits immediately and returns `Ok`.
/// Must be called before `validate_manifest_meta` on any vault opened from disk.
///
/// Migration steps (single `BEGIN IMMEDIATE` transaction):
/// 1. Create `epoch_blobs` table.
/// 2. Recreate `chunks` with nullable epoch-blob foreign-key columns and a CHECK constraint
///    ensuring that a chunk belongs to exactly one of: a standalone blob or an epoch blob.
/// 3. Create `epoch_buffer` table.
/// 4. Bump `schema_version` to `'2'`.
pub(crate) fn apply_epoch_v2_migration(conn: &Connection) -> Result<(), StorageError> {
    conn.execute_batch("BEGIN IMMEDIATE")
        .map_err(StorageError::from_rusqlite)?;

    let version = conn
        .query_row(
            "SELECT value FROM manifest_meta WHERE key = 'schema_version'",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(StorageError::from_rusqlite)?
        .ok_or_else(|| {
            StorageError::Database("missing manifest_meta key: schema_version".to_owned())
        })?;

    if version == "2" || version == "3" || version == "4" || version == "5" {
        conn.execute_batch("COMMIT")
            .map_err(StorageError::from_rusqlite)?;
        return Ok(());
    }

    conn.execute_batch(
        "
        CREATE TABLE epoch_blobs (
            epoch_blob_id    TEXT PRIMARY KEY,
            blob_name        TEXT NOT NULL UNIQUE,
            file_key_wrapped BLOB NOT NULL,
            size_padded      INTEGER NOT NULL,
            blake3_checksum  BLOB NOT NULL
        );

        CREATE TABLE chunks_new (
            chunk_id         TEXT PRIMARY KEY,
            node_id          TEXT NOT NULL REFERENCES nodes(node_id) ON DELETE CASCADE,
            chunk_index      INTEGER NOT NULL,
            blob_name        TEXT,
            size_padded      INTEGER NOT NULL,
            blake3_checksum  BLOB NOT NULL,
            epoch_blob_id    TEXT REFERENCES epoch_blobs(epoch_blob_id),
            byte_offset      INTEGER,
            byte_length      INTEGER,
            UNIQUE(node_id, chunk_index),
            CHECK (
                (blob_name IS NOT NULL AND epoch_blob_id IS NULL
                     AND byte_offset IS NULL AND byte_length IS NULL) OR
                (blob_name IS NULL AND epoch_blob_id IS NOT NULL
                     AND byte_offset IS NOT NULL AND byte_length IS NOT NULL)
            )
        );
        CREATE UNIQUE INDEX idx_chunks_blob_name ON chunks_new(blob_name) WHERE blob_name IS NOT NULL;
        INSERT INTO chunks_new (chunk_id, node_id, chunk_index, blob_name, size_padded, blake3_checksum)
            SELECT chunk_id, node_id, chunk_index, blob_name, size_padded, blake3_checksum FROM chunks;
        DROP TABLE chunks;
        ALTER TABLE chunks_new RENAME TO chunks;

        CREATE TABLE IF NOT EXISTS epoch_buffer (
            entry_id    TEXT PRIMARY KEY,
            node_id     TEXT NOT NULL REFERENCES nodes(node_id) ON DELETE CASCADE,
            plaintext   BLOB NOT NULL,
            size_bytes  INTEGER NOT NULL,
            queued_at   INTEGER NOT NULL
        );

        UPDATE manifest_meta SET value = '2' WHERE key = 'schema_version';
        ",
    )
    .map_err(StorageError::from_rusqlite)?;

    conn.execute_batch("COMMIT")
        .map_err(StorageError::from_rusqlite)?;

    Ok(())
}

/// Migrates a schema-version-2 manifest to schema-version-3.
///
/// Idempotent: if `schema_version` is already `'3'`, commits immediately and returns `Ok`.
/// Must be called after `apply_epoch_v2_migration` on any vault opened from disk.
///
/// Migration steps (single `BEGIN IMMEDIATE` transaction):
/// 1. Add `download_key_id` column to `shares` (nullable TEXT).
/// 2. Add `receipt_requested` column to `shares` (INTEGER NOT NULL DEFAULT 0).
/// 3. Add `receipt_received_at` column to `shares` (nullable INTEGER).
/// 4. Bump `schema_version` to `'3'`.
pub(crate) fn apply_sharing_v3_migration(conn: &Connection) -> Result<(), StorageError> {
    conn.execute_batch("BEGIN IMMEDIATE")
        .map_err(StorageError::from_rusqlite)?;

    let version = conn
        .query_row(
            "SELECT value FROM manifest_meta WHERE key = 'schema_version'",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(StorageError::from_rusqlite)?
        .ok_or_else(|| {
            StorageError::Database("missing manifest_meta key: schema_version".to_owned())
        })?;

    if version == "3" || version == "4" || version == "5" {
        conn.execute_batch("COMMIT")
            .map_err(StorageError::from_rusqlite)?;
        return Ok(());
    }

    conn.execute_batch(
        "
        ALTER TABLE shares ADD COLUMN download_key_id TEXT;
        ALTER TABLE shares ADD COLUMN receipt_requested INTEGER NOT NULL DEFAULT 0;
        ALTER TABLE shares ADD COLUMN receipt_received_at INTEGER;
        UPDATE manifest_meta SET value = '3' WHERE key = 'schema_version';
        ",
    )
    .map_err(StorageError::from_rusqlite)?;

    conn.execute_batch("COMMIT")
        .map_err(StorageError::from_rusqlite)?;

    Ok(())
}

/// Adds the `import_receipt_received_at` column introduced in sharing v4.
///
/// Idempotent: if `schema_version` is already `'4'`, commits immediately and returns.
pub(crate) fn apply_sharing_v4_migration(conn: &Connection) -> Result<(), StorageError> {
    conn.execute_batch("BEGIN IMMEDIATE")
        .map_err(StorageError::from_rusqlite)?;

    let version = conn
        .query_row(
            "SELECT value FROM manifest_meta WHERE key = 'schema_version'",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(StorageError::from_rusqlite)?
        .ok_or_else(|| {
            StorageError::Database("missing manifest_meta key: schema_version".to_owned())
        })?;

    if version == "4" || version == "5" {
        conn.execute_batch("COMMIT")
            .map_err(StorageError::from_rusqlite)?;
        return Ok(());
    }

    conn.execute_batch(
        "
        ALTER TABLE shares ADD COLUMN import_receipt_received_at INTEGER;
        UPDATE manifest_meta SET value = '4' WHERE key = 'schema_version';
        ",
    )
    .map_err(StorageError::from_rusqlite)?;

    conn.execute_batch("COMMIT")
        .map_err(StorageError::from_rusqlite)?;

    Ok(())
}

/// Adds the `backup_upload_failures` tracking table introduced in Phase 7.
///
/// Idempotent: if `schema_version` is already `'5'`, commits immediately and returns.
/// Must be called after `apply_sharing_v4_migration` on any vault opened from disk.
///
/// Migration steps (single `BEGIN IMMEDIATE` transaction):
/// 1. Create `backup_upload_failures` table.
/// 2. Bump `schema_version` to `'5'`.
pub(crate) fn apply_backup_v5_migration(conn: &Connection) -> Result<(), StorageError> {
    conn.execute_batch("BEGIN IMMEDIATE")
        .map_err(StorageError::from_rusqlite)?;

    let version = conn
        .query_row(
            "SELECT value FROM manifest_meta WHERE key = 'schema_version'",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(StorageError::from_rusqlite)?
        .ok_or_else(|| {
            StorageError::Database("missing manifest_meta key: schema_version".to_owned())
        })?;

    if version == "5" {
        conn.execute_batch("COMMIT")
            .map_err(StorageError::from_rusqlite)?;
        return Ok(());
    }

    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS backup_upload_failures (
            blob_name       TEXT NOT NULL,
            destination_id  TEXT NOT NULL,
            failed_at       INTEGER NOT NULL,
            retry_count     INTEGER NOT NULL DEFAULT 0,
            PRIMARY KEY (blob_name, destination_id)
        );
        UPDATE manifest_meta SET value = '5' WHERE key = 'schema_version';
        ",
    )
    .map_err(StorageError::from_rusqlite)?;

    conn.execute_batch("COMMIT")
        .map_err(StorageError::from_rusqlite)?;

    Ok(())
}

/// Verifies the active SQLCipher key by reading `sqlite_master`.
pub(crate) fn verify_sqlcipher_key(conn: &Connection) -> Result<(), StorageError> {
    conn.query_row("SELECT count(*) FROM sqlite_master", [], |_row| Ok(()))
        .map_err(StorageError::from_rusqlite)
}

/// Runs low-level SQLite integrity checks for schema and foreign keys.
pub(crate) fn validate_schema_integrity(conn: &Connection) -> Result<(), StorageError> {
    let integrity_result = conn
        .query_row("PRAGMA integrity_check(1)", [], |row| {
            row.get::<_, String>(0)
        })
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
    let epoch_buffer_enabled = if epoch_buffer_enabled {
        "true"
    } else {
        "false"
    };
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
    if schema_version != "1"
        && schema_version != "2"
        && schema_version != "3"
        && schema_version != "4"
        && schema_version != "5"
    {
        return Err(StorageError::Database(
            "invalid schema_version: expected 1, 2, 3, 4, or 5".to_owned(),
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
    use rusqlite::{Connection, OptionalExtension, params};
    use uuid::Uuid;

    use super::{
        apply_canonical_schema, apply_epoch_v2_migration, apply_sharing_v3_migration,
        apply_sharing_v4_migration, seed_manifest_meta, validate_manifest_meta,
        validate_schema_integrity,
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

        assert!(
            matches!(result, Err(StorageError::Database(message)) if message.contains("foreign key"))
        );
    }

    /// Verifies out-of-range chunk sizes are rejected.
    #[test]
    fn test_validate_manifest_meta_rejects_out_of_range_chunk_size() {
        let conn = Connection::open_in_memory().expect("in-memory connection should open");
        apply_canonical_schema(&conn).expect("schema should apply");
        seed_manifest_meta(&conn, Uuid::new_v4(), 131_071, false).expect("seed should succeed");

        let result = validate_manifest_meta(&conn);

        assert!(
            matches!(result, Err(StorageError::Database(message)) if message.contains("chunk_size_bytes"))
        );
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

        assert!(
            matches!(result, Err(StorageError::Database(message)) if message.contains("epoch_buffer_enabled"))
        );
    }

    /// Verifies unsupported schema versions are rejected.
    #[test]
    fn test_validate_manifest_meta_rejects_invalid_schema_version() {
        let conn = Connection::open_in_memory().expect("in-memory connection should open");
        apply_canonical_schema(&conn).expect("schema should apply");
        seed_manifest_meta(&conn, Uuid::new_v4(), 4_194_304, false).expect("seed should succeed");
        conn.execute(
            "UPDATE manifest_meta SET value = '6' WHERE key = 'schema_version'",
            [],
        )
        .expect("test setup should update schema_version");

        let result = validate_manifest_meta(&conn);

        assert!(
            matches!(result, Err(StorageError::Database(message)) if message.contains("schema_version"))
        );
    }

    /// Verifies non-UUID vault identifiers are rejected.
    #[test]
    fn test_validate_manifest_meta_rejects_invalid_vault_id() {
        let conn = Connection::open_in_memory().expect("in-memory connection should open");
        apply_canonical_schema(&conn).expect("schema should apply");
        seed_manifest_meta(&conn, Uuid::new_v4(), 4_194_304, false).expect("seed should succeed");
        conn.execute(
            "UPDATE manifest_meta SET value = 'not-a-uuid' WHERE key = 'vault_id'",
            [],
        )
        .expect("test setup should update vault_id");

        let result = validate_manifest_meta(&conn);

        assert!(
            matches!(result, Err(StorageError::Database(message)) if message.contains("vault_id"))
        );
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

        assert!(
            matches!(result, Err(StorageError::Database(message)) if message.contains("snapshot_counter"))
        );
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

        assert!(
            matches!(result, Err(StorageError::Database(message)) if message.contains("missing manifest_meta key: vault_id"))
        );
    }

    /// Verifies that `apply_epoch_v2_migration` migrates a version-1 schema to version 2.
    #[test]
    fn test_apply_epoch_v2_migration_upgrades_version_1_to_2() {
        let conn = Connection::open_in_memory().expect("in-memory connection should open");
        apply_canonical_schema(&conn).expect("schema should apply");
        seed_manifest_meta(&conn, Uuid::new_v4(), 4_194_304, false).expect("seed should succeed");

        apply_epoch_v2_migration(&conn).expect("migration should succeed");

        let version: String = conn
            .query_row(
                "SELECT value FROM manifest_meta WHERE key = 'schema_version'",
                [],
                |row| row.get(0),
            )
            .expect("schema_version should be readable");
        assert_eq!(version, "2");

        let result = validate_manifest_meta(&conn);
        assert!(result.is_ok());
    }

    /// Verifies that `apply_epoch_v2_migration` is idempotent when already at version 2.
    #[test]
    fn test_apply_epoch_v2_migration_is_idempotent_when_already_at_version_2() {
        let conn = Connection::open_in_memory().expect("in-memory connection should open");
        apply_canonical_schema(&conn).expect("schema should apply");
        seed_manifest_meta(&conn, Uuid::new_v4(), 4_194_304, false).expect("seed should succeed");
        apply_epoch_v2_migration(&conn).expect("first migration should succeed");

        let result = apply_epoch_v2_migration(&conn);

        assert!(result.is_ok());
    }

    /// Verifies that existing chunk rows survive the version-2 migration.
    #[test]
    fn test_apply_epoch_v2_migration_preserves_existing_chunk_rows() {
        let conn = Connection::open_in_memory().expect("in-memory connection should open");
        apply_canonical_schema(&conn).expect("schema should apply");
        seed_manifest_meta(&conn, Uuid::new_v4(), 4_194_304, false).expect("seed should succeed");

        let node_id = Uuid::new_v4().hyphenated().to_string();
        conn.execute(
            "INSERT INTO nodes (node_id, parent_id, node_type, name, created_at, modified_at, size_bytes, file_key_wrapped)
             VALUES (?1, NULL, 'file', 'test.txt', 1, 1, 42, ?2)",
            params![node_id, vec![0u8; 72]],
        )
        .expect("node should insert");

        let chunk_id = Uuid::new_v4().hyphenated().to_string();
        let blob_name = Uuid::new_v4().hyphenated().to_string();
        conn.execute(
            "INSERT INTO chunks (chunk_id, node_id, chunk_index, blob_name, size_padded, blake3_checksum)
             VALUES (?1, ?2, 0, ?3, 4194304, ?4)",
            params![chunk_id, node_id, blob_name, vec![1u8; 32]],
        )
        .expect("chunk should insert");

        apply_epoch_v2_migration(&conn).expect("migration should succeed");

        let count: i64 = conn
            .query_row("SELECT count(*) FROM chunks", [], |row| row.get(0))
            .expect("chunk count should be readable");
        assert_eq!(count, 1);
    }

    /// Verifies that `apply_sharing_v3_migration` migrates a version-2 schema to version 3.
    #[test]
    fn test_apply_sharing_v3_migration_upgrades_version_2_to_3() {
        let conn = Connection::open_in_memory().expect("in-memory connection should open");
        apply_canonical_schema(&conn).expect("schema should apply");
        seed_manifest_meta(&conn, Uuid::new_v4(), 4_194_304, false).expect("seed should succeed");
        apply_epoch_v2_migration(&conn).expect("v2 migration should succeed");

        apply_sharing_v3_migration(&conn).expect("v3 migration should succeed");

        let version: String = conn
            .query_row(
                "SELECT value FROM manifest_meta WHERE key = 'schema_version'",
                [],
                |row| row.get(0),
            )
            .expect("schema_version should be readable");
        assert_eq!(version, "3");

        let result = validate_manifest_meta(&conn);
        assert!(result.is_ok());
    }

    /// Verifies that `apply_sharing_v3_migration` is idempotent when already at version 3.
    #[test]
    fn test_apply_sharing_v3_migration_is_idempotent_when_already_at_version_3() {
        let conn = Connection::open_in_memory().expect("in-memory connection should open");
        apply_canonical_schema(&conn).expect("schema should apply");
        seed_manifest_meta(&conn, Uuid::new_v4(), 4_194_304, false).expect("seed should succeed");
        apply_epoch_v2_migration(&conn).expect("v2 migration should succeed");
        apply_sharing_v3_migration(&conn).expect("first v3 migration should succeed");

        let result = apply_sharing_v3_migration(&conn);

        assert!(result.is_ok());
    }

    /// Verifies that the new shares columns exist after v3 migration and default correctly.
    #[test]
    fn test_apply_sharing_v3_migration_adds_shares_columns_with_defaults() {
        let conn = Connection::open_in_memory().expect("in-memory connection should open");
        apply_canonical_schema(&conn).expect("schema should apply");
        seed_manifest_meta(&conn, Uuid::new_v4(), 4_194_304, false).expect("seed should succeed");
        apply_epoch_v2_migration(&conn).expect("v2 migration should succeed");
        apply_sharing_v3_migration(&conn).expect("v3 migration should succeed");

        conn.execute_batch(
            "INSERT INTO contacts (contact_id, display_name, public_key, created_at)
             VALUES ('00000000-0000-4000-8000-000000000001', 'Test', X'0000000000000000000000000000000000000000000000000000000000000000', 1);
             INSERT INTO nodes (node_id, parent_id, node_type, name, created_at, modified_at, size_bytes, file_key_wrapped)
             VALUES ('00000000-0000-4000-8000-000000000002', NULL, 'file', 'test.txt', 1, 1, 0, X'0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000');
             INSERT INTO shares (share_id, file_id, contact_id, file_share_id, cloud_path, created_at)
             VALUES ('00000000-0000-4000-8000-000000000003',
                     '00000000-0000-4000-8000-000000000002',
                     '00000000-0000-4000-8000-000000000001',
                     'fsid-1', 'shared/fsid-1/', 1000);",
        )
        .expect("test row should insert");

        let (download_key_id, receipt_requested, receipt_received_at): (Option<String>, i64, Option<i64>) = conn
            .query_row(
                "SELECT download_key_id, receipt_requested, receipt_received_at FROM shares WHERE share_id = '00000000-0000-4000-8000-000000000003'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .expect("row should be readable");

        assert_eq!(download_key_id, None);
        assert_eq!(receipt_requested, 0);
        assert_eq!(receipt_received_at, None);
    }

    /// Verifies that `apply_sharing_v4_migration` adds `import_receipt_received_at` and
    /// that `validate_manifest_meta` accepts schema version `'4'`.
    #[test]
    fn test_apply_sharing_v4_migration_upgrades_version_3_to_4() {
        let conn = Connection::open_in_memory().expect("in-memory connection should open");
        apply_canonical_schema(&conn).expect("schema should apply");
        seed_manifest_meta(&conn, Uuid::new_v4(), 4_194_304, false).expect("seed should succeed");
        apply_epoch_v2_migration(&conn).expect("v2 migration should succeed");
        apply_sharing_v3_migration(&conn).expect("v3 migration should succeed");

        apply_sharing_v4_migration(&conn).expect("v4 migration should succeed");

        let version: String = conn
            .query_row(
                "SELECT value FROM manifest_meta WHERE key = 'schema_version'",
                [],
                |row| row.get(0),
            )
            .expect("schema_version should be readable");
        assert_eq!(version, "4");

        // validate_manifest_meta must accept v4.
        let result = validate_manifest_meta(&conn);
        assert!(
            result.is_ok(),
            "validate_manifest_meta should accept version 4"
        );

        // The new column must exist and be NULL by default.
        let col: Option<i64> = conn
            .query_row(
                "SELECT import_receipt_received_at FROM shares LIMIT 1",
                [],
                |row| row.get(0),
            )
            .optional()
            .expect("query should succeed")
            .unwrap_or(None);
        assert_eq!(col, None);
    }

    /// Verifies that `apply_epoch_v2_migration` is idempotent when already at version 4.
    #[test]
    fn test_apply_epoch_v2_migration_is_idempotent_when_already_at_version_4() {
        let conn = Connection::open_in_memory().expect("in-memory connection should open");
        apply_canonical_schema(&conn).expect("schema should apply");
        seed_manifest_meta(&conn, Uuid::new_v4(), 4_194_304, false).expect("seed should succeed");
        apply_epoch_v2_migration(&conn).expect("v2 migration should succeed");
        apply_sharing_v3_migration(&conn).expect("v3 migration should succeed");
        apply_sharing_v4_migration(&conn).expect("v4 migration should succeed");

        let result = apply_epoch_v2_migration(&conn);
        assert!(
            result.is_ok(),
            "v2 migration should be idempotent when vault is at v4"
        );
    }

    /// Verifies that `apply_sharing_v3_migration` is idempotent when already at version 4.
    #[test]
    fn test_apply_sharing_v3_migration_is_idempotent_when_already_at_version_4() {
        let conn = Connection::open_in_memory().expect("in-memory connection should open");
        apply_canonical_schema(&conn).expect("schema should apply");
        seed_manifest_meta(&conn, Uuid::new_v4(), 4_194_304, false).expect("seed should succeed");
        apply_epoch_v2_migration(&conn).expect("v2 migration should succeed");
        apply_sharing_v3_migration(&conn).expect("v3 migration should succeed");
        apply_sharing_v4_migration(&conn).expect("v4 migration should succeed");

        let result = apply_sharing_v3_migration(&conn);
        assert!(
            result.is_ok(),
            "v3 migration should be idempotent when vault is at v4"
        );
    }

    /// Verifies that `apply_sharing_v4_migration` is idempotent when already at version 4.
    #[test]
    fn test_apply_sharing_v4_migration_is_idempotent_when_already_at_version_4() {
        let conn = Connection::open_in_memory().expect("in-memory connection should open");
        apply_canonical_schema(&conn).expect("schema should apply");
        seed_manifest_meta(&conn, Uuid::new_v4(), 4_194_304, false).expect("seed should succeed");
        apply_epoch_v2_migration(&conn).expect("v2 migration should succeed");
        apply_sharing_v3_migration(&conn).expect("v3 migration should succeed");
        apply_sharing_v4_migration(&conn).expect("first v4 migration should succeed");

        let result = apply_sharing_v4_migration(&conn);
        assert!(result.is_ok(), "second v4 migration should be idempotent");
    }
}
