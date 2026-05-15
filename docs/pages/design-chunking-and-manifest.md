# Chunking and Manifest

Files are split into fixed-size, padded, individually encrypted chunks. The cloud receives only opaque, uniformly sized blobs with random UUID names — no file size, filename, or structure information is ever visible to the cloud provider. A local SQLCipher manifest tracks the mapping between the virtual filesystem and encrypted blobs.

---

## Goals

- Fixed-size chunks, uniformly padded, encrypted individually with per-file keys
- Cloud sees only opaque, identically sized blobs with random UUID v4 names
- SQLCipher manifest database tracks virtual filesystem entries → encrypted blobs
- Streaming I/O — no complete file is ever loaded into a single buffer
- EXIF metadata stripped from media files before encryption (in RAM, original file untouched)

---

## Contract Surface

### Interface

Storage API: `encrypt_file`, `decrypt_file`, and the `MetadataStore` trait (`insert_node`, `insert_chunks`, `get_chunks`, `delete_node`, `list_pending_deletions`, `mark_deletion_complete`, `increment_snapshot_counter`, and related).

`ChunkRecord` is the canonical per-chunk contract between encryption and metadata persistence.

### Data

Canonical manifest tables: `nodes`, `chunks`, `manifest_meta`, `pending_deletions`.

- `nodes.file_key_wrapped` — one wrapped file key per file
- `chunks` — `blob_name`, `chunk_index`, `size_padded`, `blake3_checksum`
- `manifest_meta` — `chunk_size_bytes` (immutable), `epoch_buffer_enabled`, `snapshot_counter`

### Invariants

- `chunk_size_bytes` is immutable per vault; every chunk is padded to that exact size; reassembly truncates via `nodes.size_bytes`.
- Streaming: at most one chunk's plaintext buffer in memory at a time.
- BLAKE3 verification occurs before decrypt on every chunk.
- Node deletion cascades chunk-row deletion via SQL `CASCADE`.

### Dependencies

Depends on Phase 1 crypto (`encrypt_chunk`, `decrypt_chunk`, file key wrapping, BLAKE3, UUID blob naming). Provides manifest and staging contracts consumed by cloud synchronisation.

---

## Chunk Size

**Set once at vault creation; immutable thereafter.**

Default: 4 MiB (4 194 304 bytes). Valid range: 128 KiB – 64 MiB.

Chunk size is a **privacy vs. storage efficiency** dial:

- **Larger** → wider blob-count inference range → stronger size privacy. At 64 MiB a 5 MiB document and a 60 MiB document both produce one blob — indistinguishable.
- **Smaller** → narrower inference range → lower storage overhead from padding.

Changing chunk size after creation would require re-encrypting every blob — equivalent to recreating the vault.

### Padding overhead (at default 4 MiB)

| File size | Chunks | Padded total | Waste |
|-----------|--------|-------------|-------|
| 1 byte | 1 | 4 MiB | ~100% |
| 1 MiB | 1 | 4 MiB | 75% |
| 4 MiB | 1 | 4 MiB | 0% |
| 10 MiB | 3 | 12 MiB | 17% |
| 100 MiB | 25 | 100 MiB | 0% |

Per-chunk crypto overhead: 40 bytes (24-byte nonce + 16-byte tag). Negligible at any chunk size.

---

## Manifest Database Schema

SQLCipher database keyed with `sqlcipher_key`:

```sql
CREATE TABLE nodes (
    node_id          TEXT PRIMARY KEY,     -- UUID v4
    parent_id        TEXT REFERENCES nodes(node_id) ON DELETE CASCADE,
    node_type        TEXT NOT NULL CHECK (node_type IN ('file', 'directory')),
    name             TEXT NOT NULL,
    created_at       INTEGER NOT NULL,
    modified_at      INTEGER NOT NULL,
    size_bytes       INTEGER NOT NULL,
    file_key_wrapped BLOB
        CHECK ((node_type = 'file'      AND file_key_wrapped IS NOT NULL)
            OR (node_type = 'directory' AND file_key_wrapped IS NULL))
);

CREATE TABLE chunks (
    chunk_id         TEXT PRIMARY KEY,
    node_id          TEXT NOT NULL REFERENCES nodes(node_id) ON DELETE CASCADE,
    chunk_index      INTEGER NOT NULL,
    blob_name        TEXT NOT NULL,        -- UUID v4, no relation to file identity
    size_padded      INTEGER NOT NULL,
    blake3_checksum  BLOB NOT NULL,        -- 32 bytes, over encrypted blob
    UNIQUE(node_id, chunk_index),
    UNIQUE(blob_name)
);

CREATE TABLE manifest_meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
    -- chunk_size_bytes: immutable after creation
    -- snapshot_counter: monotonic, advances via increment_snapshot_counter()
    -- epoch_buffer_enabled: opt-in small-file routing
);

CREATE TABLE pending_deletions (
    blob_name  TEXT PRIMARY KEY,
    queued_at  INTEGER NOT NULL
);
```

The tree structure is purely virtual — it exists only in SQLCipher. The cloud sees a flat namespace of UUID-named blobs.

---

## EXIF Stripping

Media files (JPEG, PNG, TIFF) may contain metadata that reveals sensitive information: GPS coordinates, camera model, timestamps. Arx Runa strips this metadata in RAM before the encrypt pipeline.

**Supported types** (detected by magic bytes, not file extension):

| MIME type | Metadata stripped |
|-----------|-----------------|
| `image/jpeg` | EXIF, XMP, IPTC |
| `image/png` | eXIf chunk, XMP |
| `image/tiff` | EXIF, XMP, IPTC |

MP4/QuickTime are not supported — the `moov` atom containing GPS metadata is at the end of the file in typical device recordings, incompatible with the streaming pipeline.

The original file on disk is never modified. The stripped content is what enters encryption and is stored in the cloud.

---

## Encrypt Pipeline

```rust
struct ChunkRecord {
    chunk_id:        Uuid,
    chunk_index:     u32,
    blob_name:       String,       // UUID v4
    size_padded:     u64,
    blake3_checksum: [u8; 32],
}

async fn encrypt_file(
    source: &Path,
    file_id: Uuid,
    file_key: &FileKey,
    metadata_store: &dyn MetadataStore,
    staging_directory: &Path,
) -> Result<Vec<ChunkRecord>, StorageError>;
```

Per chunk:
1. Read up to `chunk_size_bytes` from source
2. Zero-pad to `chunk_size_bytes` if last chunk
3. `encrypt_chunk(padded_buffer, file_key, file_id, chunk_index)` → wire blob
4. `blake3::hash(wire_blob)` → checksum
5. Write to `staging_directory/<blob_uuid>.blob`
6. Zeroize plaintext buffer

---

## Decrypt Pipeline

```rust
async fn decrypt_file(
    destination: &Path,
    file_id: Uuid,
    file_key: &FileKey,
    file_size: u64,
    chunks: &[ChunkRecord],
    blob_directory: &Path,
    metadata_store: &dyn MetadataStore,
) -> Result<(), StorageError>;
```

Per chunk:
1. Read wire blob from `blob_directory/<blob_name>.blob`
2. Verify `blake3::hash(wire_blob) == expected_checksum` — error if mismatch, never attempt decrypt
3. `decrypt_chunk(wire_blob, file_key, file_id, chunk_index)` → padded plaintext
4. Write full `chunk_size_bytes` for all chunks except last; truncate last chunk to `file_size mod chunk_size_bytes`
5. Zeroize plaintext buffer

---

## File Key Lifecycle

### New file upload

1. Generate `file_key` (random 256-bit CSPRNG)
2. Wrap: `file_key_wrapped = encrypt(file_key, key_encryption_key)`
3. Encrypt all chunks → `ChunkRecord` list (blobs written to staging)
4. SQLCipher transaction: insert `nodes` row + all `chunks` rows
5. Commit; zeroize `file_key`

Crash before commit leaves no manifest state. Orphaned staging blobs are cleaned up on next startup.

### File deletion

1. SQLCipher transaction: queue `blob_name`s into `pending_deletions`; delete `nodes` row (cascades `chunks` rows and `file_key_wrapped`)
2. Commit
3. Cloud sync drains `pending_deletions` — row removed only after confirmed cloud delete

---

## Staging Directory

Encrypted blobs are written here before upload and read here during download:

| OS | Path |
|----|------|
| Windows | `%APPDATA%/arx-runa/staging/` |
| Linux | `~/.local/share/arx-runa/staging/` |
| macOS | `~/Library/Application Support/arx-runa/staging/` |

Staging blobs are AEAD ciphertext — leaving them on disk does not expose plaintext. On startup, Arx Runa removes any staging blob not referenced by `chunks.blob_name` in the manifest.

---

## MetadataStore Trait

```rust
#[async_trait]
trait MetadataStore: Send + Sync {
    async fn insert_node(&self, node: &Node) -> Result<(), StorageError>;
    async fn insert_chunks(&self, chunks: &[ChunkRecord]) -> Result<(), StorageError>;
    async fn get_node(&self, node_id: Uuid) -> Result<Node, StorageError>;
    async fn list_children(&self, parent_id: Uuid) -> Result<Vec<Node>, StorageError>;
    async fn get_chunks(&self, node_id: Uuid) -> Result<Vec<ChunkRecord>, StorageError>;
    async fn rename_node(&self, node_id: Uuid, new_name: &str, modified_at: i64) -> Result<(), StorageError>;
    async fn move_node(&self, node_id: Uuid, new_parent_id: Option<Uuid>, modified_at: i64) -> Result<(), StorageError>;
    async fn delete_node(&self, node_id: Uuid) -> Result<(), StorageError>;
    async fn list_pending_deletions(&self, limit: usize) -> Result<Vec<String>, StorageError>;
    async fn mark_deletion_complete(&self, blob_name: &str) -> Result<(), StorageError>;
    async fn get_meta(&self, key: &str) -> Result<Option<String>, StorageError>;
    async fn set_meta(&self, key: &str, value: &str) -> Result<(), StorageError>;
    async fn increment_snapshot_counter(&self) -> Result<u64, StorageError>;
}
```

Implementations: `SqlCipherMetadataStore` (production) and `MockMetadataStore` (in-memory, for testing).

---

## Related Documents

- [Cryptographic Primitives](design-cryptographic-primitives.md) — `encrypt_chunk`, `decrypt_chunk`, file key wrapping
- [Authentication and Session Management](design-authentication.md) — `sqlcipher_key` and `key_encryption_key` origins
- [Cloud Synchronisation](design-cloud-synchronisation.md) — staging → cloud upload, `pending_deletions` drain
- [File Sharing](design-file-sharing.md) — `received_shares` table, per-file key sharing model
