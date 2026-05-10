# Epoch Buffer Feature — Full Implementation

## Prior fix subsumed
The earlier plan to "remove the epoch guard" is now superseded. Rather than dropping the guard, we replace it with the real epoch implementation. The upload bug is fixed as a side-effect.

---

## Design summary

Epoch buffering is opt-in at vault creation (`epoch_buffer_enabled`, default `false`). When enabled:
- Small files (`file_size < chunk_size_bytes`, typically 4 MiB) are staged in a local SQLCipher-encrypted table instead of being uploaded immediately
- When the buffer reaches `chunk_size_bytes` total bytes, all staged plaintexts are packed into one fixed-size blob, encrypted with a new epoch key, and uploaded
- Vault lock always triggers a flush of any remaining buffered data
- Restore: epoch-packed files are fetched by looking up the epoch blob, decrypting it, and slicing out the file's byte range

Large files (`>= chunk_size_bytes`) always use the Immediate path — unchanged from current behavior.

---

## DB schema additions (schema_version 1 → 2)

Per `docs/research/padding-overhead-reduction.md §Restore mechanics`, epoch data is stored as nullable columns on the **existing `chunks` table** plus two new tables.

Because `blob_name TEXT NOT NULL` cannot be relaxed via `ALTER TABLE ADD COLUMN`, the migration **recreates the `chunks` table** in-place (SQLite table-rename pattern). All other tables are unchanged.

### New tables (via `CREATE TABLE IF NOT EXISTS`)

```sql
-- Plaintext staging for small files awaiting epoch flush (SQLCipher = at-rest encryption)
CREATE TABLE epoch_buffer (
    entry_id    TEXT PRIMARY KEY,
    node_id     TEXT NOT NULL REFERENCES nodes(node_id) ON DELETE CASCADE,
    plaintext   BLOB NOT NULL,
    size_bytes  INTEGER NOT NULL,
    queued_at   INTEGER NOT NULL
);

-- Metadata for each flushed epoch blob
CREATE TABLE epoch_blobs (
    epoch_blob_id    TEXT PRIMARY KEY,   -- UUID v4; used as AEAD AAD binding
    blob_name        TEXT NOT NULL UNIQUE,  -- UUID v4; cloud storage name
    file_key_wrapped BLOB NOT NULL,      -- epoch blob key wrapped with KEK
    size_padded      INTEGER NOT NULL,   -- always equals chunk_size_bytes + 40
    blake3_checksum  BLOB NOT NULL       -- 32 bytes, over encrypted blob
);
```

### Altered `chunks` table (recreated)

```sql
-- chunks_new: same as chunks + nullable blob_name + epoch columns
CREATE TABLE chunks_new (
    chunk_id         TEXT PRIMARY KEY,
    node_id          TEXT NOT NULL REFERENCES nodes(node_id) ON DELETE CASCADE,
    chunk_index      INTEGER NOT NULL,
    blob_name        TEXT,          -- NULL for epoch-packed; NOT NULL for standalone
    size_padded      INTEGER NOT NULL,
    blake3_checksum  BLOB NOT NULL,
    epoch_blob_id    TEXT REFERENCES epoch_blobs(epoch_blob_id),  -- NULL for standalone
    byte_offset      INTEGER,       -- byte start within epoch blob plaintext; NULL for standalone
    byte_length      INTEGER,       -- byte count; NULL for standalone
    UNIQUE(node_id, chunk_index),
    CHECK (
        (blob_name IS NOT NULL AND epoch_blob_id IS NULL
             AND byte_offset IS NULL AND byte_length IS NULL) OR
        (blob_name IS NULL AND epoch_blob_id IS NOT NULL
             AND byte_offset IS NOT NULL AND byte_length IS NOT NULL)
    )
);
CREATE UNIQUE INDEX idx_chunks_blob_name ON chunks_new(blob_name) WHERE blob_name IS NOT NULL;
-- copy existing standalone rows
INSERT INTO chunks_new (chunk_id, node_id, chunk_index, blob_name, size_padded, blake3_checksum)
    SELECT chunk_id, node_id, chunk_index, blob_name, size_padded, blake3_checksum FROM chunks;
DROP TABLE chunks;
ALTER TABLE chunks_new RENAME TO chunks;
```

### `validate_manifest_meta` update

Currently rejects `schema_version != '1'`. Must be updated to accept `'1'` (pre-migration) and `'2'` (post-migration). Migration runs in `SqlCipherMetadataStore::open()` before validation, so after the first open on an existing vault, the version becomes `'2'`.

### Flush-to-cloud path

Per `docs/research/padding-overhead-reduction.md §Integration with Cloud Sync Design`:
> epoch flush writes encrypted blobs to the **staging directory** as `.blob` files; the existing cloud push flow (Phase 4) handles upload without changes.

`flush_epoch_buffer()` does **not** upload directly. It writes to `staging_dir` and calls `commit_epoch_flush()`. The push flow picks up staged blobs as normal.

---

## Crypto

Epoch blobs use the same AEAD primitive as standalone chunks (`encrypt_chunk` / `decrypt_chunk`).  
AAD binding: `FileId::from_uuid(epoch_blob_id)` + `ChunkIndex::new(0)`.  
Each epoch blob gets its own randomly generated `FileKey`.

No changes to the crypto module.

---

## Type changes

### `ChunkRecord` — add three nullable epoch fields

```rust
pub struct ChunkRecord {
    // existing fields unchanged
    pub chunk_id: Uuid,
    pub node_id: NodeId,
    pub chunk_index: u32,
    pub blob_name: String,       // epoch chunks: populated from epoch_blobs.blob_name
    pub size_padded: u64,        // epoch chunks: epoch_blobs.size_padded
    pub blake3_checksum: [u8; 32],  // epoch chunks: epoch_blobs.blake3_checksum
    // new — all None for standalone chunks, all Some for epoch-packed chunks
    pub epoch_blob_id: Option<Uuid>,
    pub byte_offset: Option<u64>,
    pub byte_length: Option<u64>,
}
```

For epoch chunks, `blob_name` holds the **epoch blob's** cloud UUID (from `epoch_blobs.blob_name`), allowing existing blob-fetch code in the download path to work without modification.

### New types in `src-tauri/src/storage/types/`

- `EpochBufferEntry { entry_id: Uuid, node_id: Uuid, plaintext: Vec<u8>, size_bytes: u64 }`
- `EpochBlobRecord { epoch_blob_id: Uuid, blob_name: String, file_key_wrapped: Vec<u8>, size_padded: u64, blake3_checksum: [u8; 32] }`

---

## MetadataStore trait additions

```rust
// Insert node row without chunks (epoch upload path)
async fn insert_file_node_only(&self, node: &Node) -> Result<(), StorageError>;

// Append plaintext to the epoch buffer
async fn stage_epoch_entry(&self, node_id: Uuid, plaintext: Vec<u8>) -> Result<(), StorageError>;

// Sum of staged plaintext bytes
async fn get_epoch_buffer_total_bytes(&self) -> Result<u64, StorageError>;

// All staged entries (plaintext included)
async fn get_epoch_buffer_entries(&self) -> Result<Vec<EpochBufferEntry>, StorageError>;

// Atomic: insert epoch_blobs row + epoch_chunk_extents rows + clear flushed epoch_buffer entries
// extents: Vec<(node_id, chunk_index, byte_offset, byte_length)>
async fn commit_epoch_flush(
    &self,
    record: &EpochBlobRecord,
    extents: &[(Uuid, u32, u64, u64)],
) -> Result<(), StorageError>;

// Look up epoch blob metadata (for restore)
async fn get_epoch_blob(&self, epoch_blob_id: Uuid) -> Result<EpochBlobRecord, StorageError>;
```

`get_chunks()` — signature unchanged, but implementation returns a UNION of `chunks` + `epoch_chunk_extents JOIN epoch_blobs`. Epoch rows have `epoch_blob_id/byte_offset/byte_length = Some(...)`.

---

## Files changed

### Schema + migration
- `src-tauri/src/storage/schema.rs` — add 3 new tables to `CANONICAL_SCHEMA`; add `apply_epoch_v2_migration(conn)` fn

### Types
- `src-tauri/src/storage/types/chunk_record.rs` — add 3 nullable fields
- `src-tauri/src/storage/types/epoch_buffer_entry.rs` — new file
- `src-tauri/src/storage/types/epoch_blob_record.rs` — new file
- `src-tauri/src/storage/types/mod.rs` — re-export new types

### Trait + implementations
- `src-tauri/src/storage/metadata_store.rs` — add 6 new trait methods
- `src-tauri/src/storage/sqlcipher.rs` — implement new methods; update `open()` to run migration and accept schema_version '1' or '2'; update `get_chunks()` to LEFT JOIN `epoch_blobs` on `chunks.epoch_blob_id`; populate epoch fields in returned `ChunkRecord`
- `src-tauri/src/storage/mock.rs` — implement new trait methods (in-memory)

### Upload
- `src-tauri/src/storage/vault_ops/upload_file.rs` — replace epoch guard with real epoch path; keep `read_epoch_buffer_enabled()` and routing logic; epoch path: read plaintext into memory, `insert_file_node_only`, `stage_epoch_entry`, if `get_epoch_buffer_total_bytes() >= chunk_size_bytes` → call `flush_epoch_buffer` synchronously; return Ok

### Flush
- `src-tauri/src/storage/vault_ops/epoch_flush.rs` — new file; `pub async fn flush_epoch_buffer(metadata_store, kek, staging_dir, chunk_size_bytes)`: get all entries, pack greedily into chunk_size_bytes blobs, zero-pad, encrypt with epoch_blob_id as AAD, write `.blob` to staging_dir (standard staging pattern — push flow handles upload), call `commit_epoch_flush`, repeat for overflow
- `src-tauri/src/storage/vault_ops/mod.rs` — re-export `flush_epoch_buffer`

### Download
- `src-tauri/src/storage/vault_ops/download_file.rs` — after `get_chunks()`, detect epoch path; call `download_epoch_file()` or existing `decrypt_file()` accordingly
- `src-tauri/src/storage/pipeline/decrypt_file.rs` — `decrypt_file` stays unchanged (standalone only); add `decrypt_epoch_file(destination, epoch_blob_id, epoch_key, file_size, chunk, blob_dir, metadata_store, progress)` alongside it

### Vault lock flush
- `src-tauri/src/ui/auth_commands.rs` — in `lock_session()`, before `*state.database.write().await = None`, flush epoch buffer if `epoch_buffer_enabled` and there is a pending buffer (best-effort; log warn on error, don't block lock)

### Tests
- `upload_file.rs` — rename `test_upload_file_with_epoch_enabled_small_file_returns_constraint_violation` → `test_upload_file_epoch_path_small_file_succeeds_and_stages`; assert Ok + no chunks in `chunks` table + 1 row in `epoch_buffer`
- `epoch_flush.rs` — unit tests: flush empty buffer is no-op; flush one small file produces 1 epoch blob + 1 extent row + cleared buffer; flush that fills exactly one blob; flush that requires two blobs
- `download_file.rs` — add `test_upload_download_epoch_round_trip`: upload small file with epoch enabled → flush → download → verify bytes match

---

## Flush trigger policy

The research doc (`docs/research/padding-overhead-reduction.md §Option 3`) **recommends** Adaptive Multi-Condition Flush (time threshold 300s + size threshold 50 MB + vault lock + sync-now button).

**This implementation uses Option 2 (simplified)** per user decision: **buffer-full + vault-lock only**.

Rationale for deferring Option 3: time-based flush requires a background timer in the Tauri session, adding significant scope (timer lifecycle, UI countdown indicator, "Sync Now" button). The Option 3 UI requirements are explicitly called out in the research doc (§Recommendation). Implementing Option 2 now and upgrading to Option 3 in Phase 7 is a documented, safe deferral — vault lock always guarantees the buffer is flushed, so no data loss occurs if the user maintains normal vault lock discipline.

**Documented trade-off**: an always-on vault that crashes before locking will lose staged small files. This is the known risk of Option 2 (research doc §Option 2 §Risk).

---

## Validation

`cargo test -p arx-runa-tauri-lib` — all tests pass.
