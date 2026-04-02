# VoidGate — Chunking and Manifest Design

> Status: Design complete. Implementation target: Phase 3.
> Last updated: 2026-03-29

---

## Goals

- Files are split into fixed-size chunks, uniformly padded, and encrypted individually
- The cloud sees only opaque, identically sized blobs with random UUID v4 names — no file size, filename, or structure information leaks
- The local SQLCipher manifest database tracks the mapping between virtual filesystem entries and encrypted blobs
- All file I/O is streaming — no complete file is ever loaded into a single buffer
- Chunk encryption uses per-file `file_key` values (from Phase 1 design)

---

## Chunk Size: 4 MiB

**Decision**: 4 MiB (4,194,304 bytes) fixed chunk size.

### Quantified padding waste analysis

Every file's last chunk is zero-padded to 4 MiB. The overhead depends on `file_size mod chunk_size`:

| File size | Chunks | Padded total | Waste | Waste % |
|-----------|--------|-------------|-------|---------|
| 1 byte | 1 | 4 MiB | ~4 MiB | ~100% |
| 100 KiB | 1 | 4 MiB | ~3.9 MiB | ~97% |
| 1 MiB | 1 | 4 MiB | 3 MiB | 75% |
| 3 MiB | 1 | 4 MiB | 1 MiB | 25% |
| 4 MiB | 1 | 4 MiB | 0 | 0% |
| 4 MiB + 1 | 2 | 8 MiB | ~4 MiB | ~50% |
| 10 MiB | 3 | 12 MiB | 2 MiB | 17% |
| 100 MiB | 25 | 100 MiB | 0 | 0% |
| 1 GiB | 256 | 1 GiB | 0 | 0% |

For files larger than one chunk, the maximum waste is < 4 MiB (constant, not proportional to file size). For a vault with many files, the average waste per file converges to ~2 MiB.

**Rationale for 4 MiB over 8 MiB**: Half the padding waste per file (2 MiB average vs 4 MiB), lower memory per chunk buffer during encrypt/decrypt, finer-grained resume on interrupted transfers.

Per-chunk crypto overhead: 24 bytes (nonce) + 16 bytes (Poly1305 tag) = 40 bytes. For a 4 MiB chunk, this is 0.001% — negligible.
<!-- CITE: Breaking and Fixing Content-Defined Chunking — https://eprint.iacr.org/2025/558.pdf — supports fixed-size chunking for metadata privacy over CDC -->

---

## Padding Scheme

**Zero-pad to `chunk_size`, truncate on reassembly using `size_bytes` from manifest.**

### Encrypt path

Each chunk's plaintext is zero-filled to exactly `chunk_size` bytes before encryption. If the file's last segment is shorter than `chunk_size`, the remaining bytes are filled with `0x00`.

### Decrypt path

On reassembly, the file's `size_bytes` from the `nodes` table determines where to truncate the last chunk's decrypted output. All preceding chunks are written in full (`chunk_size` bytes each); the last chunk is truncated to `size_bytes - (chunk_count - 1) * chunk_size`.

### Security property

The cloud sees N identically sized encrypted blobs. It cannot distinguish content from padding because the padding is encrypted by XChaCha20-Poly1305. The `size_bytes` field is inside the SQLCipher database (encrypted with `sqlcipher_key`). The cloud learns only the total number of blobs in the vault — not which blobs belong to which file, nor individual file sizes.

### 0-byte files

A 0-byte file has no chunks. The `nodes` row exists with `size_bytes = 0` and `file_key_wrapped` (still generated — needed if the file is later updated or shared). On decrypt, `size_bytes = 0` means no chunks to fetch.

---

## Manifest Database Schema

### Updated schema (SQLCipher, keyed with `sqlcipher_key`)

```sql
CREATE TABLE nodes (
    node_id          TEXT PRIMARY KEY,     -- UUID v4
    parent_id        TEXT REFERENCES nodes(node_id) ON DELETE CASCADE,
    node_type        TEXT NOT NULL,        -- 'file' or 'directory'
    name             TEXT NOT NULL,        -- plaintext (SQLCipher is the encryption layer)
    created_at       INTEGER NOT NULL,     -- Unix timestamp
    modified_at      INTEGER NOT NULL,     -- Unix timestamp
    size_bytes       INTEGER NOT NULL,     -- original file size (0 for directories)
    file_key_wrapped BLOB                  -- file_key encrypted with key_encryption_key
                                           -- NULL for directories, NOT NULL for files
);

CREATE TABLE chunks (
    chunk_id         TEXT PRIMARY KEY,     -- UUID v4
    node_id          TEXT NOT NULL REFERENCES nodes(node_id) ON DELETE CASCADE,
    chunk_index      INTEGER NOT NULL,     -- 0-based
    blob_name        TEXT NOT NULL,        -- UUID v4, no relation to file identity
    size_padded      INTEGER NOT NULL,     -- always = chunk_size (4 MiB)
    blake3_checksum  BLOB NOT NULL,        -- 32 bytes, over encrypted blob
    UNIQUE(node_id, chunk_index)
);

CREATE TABLE manifest_meta (
    key              TEXT PRIMARY KEY,
    value            TEXT NOT NULL
);
-- Initial rows:
-- ('schema_version', '1')
-- ('vault_id', '<uuid>')
-- ('snapshot_counter', '0')
-- ('last_synced_at', NULL)

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
    revoked_at       INTEGER
);

CREATE TABLE received_shares (
    share_id             TEXT PRIMARY KEY,
    sender_contact_id    TEXT REFERENCES contacts(contact_id),
    file_name            TEXT NOT NULL,
    file_key_wrapped     BLOB NOT NULL,
    chunk_count          INTEGER NOT NULL,
    chunk_size           INTEGER NOT NULL,
    chunk_uuids          TEXT NOT NULL,
    cloud_endpoint       TEXT NOT NULL,
    imported_at          INTEGER NOT NULL
);
```

### Key change: `file_key_wrapped` moved to `nodes`

The previous design stored `file_key_wrapped` per-chunk in the `chunks` table. This is redundant: every chunk of the same file uses the same `file_key`. Storing it per-chunk creates N copies for a file with N chunks.

Moving `file_key_wrapped` to the `nodes` table:
- One copy per file instead of N
- CASCADE still works: deleting a node deletes the node row (including `file_key_wrapped`) and cascades to all chunk rows
- Directories have `file_key_wrapped = NULL`

### Node types

| Type | Description | `file_key_wrapped` | Has chunks |
|------|-------------|--------------------|------------|
| `file` | A file with associated encrypted chunks | NOT NULL | Yes |
| `directory` | A folder containing other nodes via `parent_id` | NULL | No |

The root directory has `parent_id = NULL`. The tree is purely virtual — it exists only in SQLCipher. The cloud sees flat blobs.

### Unique constraints

- `UNIQUE(node_id, chunk_index)` prevents duplicate chunk indices for the same file
- `blob_name` is UUID v4 — collisions are astronomically unlikely but should be unique in practice

---

## Encrypt Pipeline

### Public API

```rust
/// Result of encrypting a single chunk.
struct ChunkRecord {
    chunk_id: Uuid,
    chunk_index: u32,
    blob_name: String,        // UUID v4
    size_padded: u64,         // always chunk_size
    blake3_checksum: [u8; 32],
    blob_path: PathBuf,       // path in staging directory
}

/// Encrypts a file into padded, encrypted chunks in the staging directory.
async fn encrypt_file(
    source: &Path,
    file_id: Uuid,
    file_key: &FileKey,
    chunk_size: usize,
    staging_directory: &Path,
) -> Result<Vec<ChunkRecord>, StorageError>;

/// Decrypts chunks and reassembles the original file.
async fn decrypt_file(
    destination: &Path,
    file_id: Uuid,
    file_key: &FileKey,
    file_size: u64,
    chunks: &[ChunkRecord],
    blob_directory: &Path,
) -> Result<(), StorageError>;
```

### Encrypt flow (per chunk)

```
1. BufReader reads up to chunk_size bytes from source file
2. If bytes_read < chunk_size: zero-pad buffer to chunk_size
3. Generate AAD = file_id (16 bytes) || chunk_index (u32 big-endian, 4 bytes)
4. encrypt_chunk(padded_buffer, file_key, file_id, chunk_index)
   → wire_blob = [24B nonce | ciphertext | 16B Poly1305 tag]
5. blake3_checksum = blake3::hash(wire_blob)
6. blob_name = Uuid::new_v4()
7. Write wire_blob to staging_directory/<blob_name>.blob via BufWriter
8. Zeroize padded_buffer
9. Return ChunkRecord
```

### Decrypt flow (per chunk)

```
1. Read wire_blob from blob_directory/<blob_name>.blob via BufReader
2. Verify: blake3::hash(wire_blob) == expected blake3_checksum
   If mismatch → return ChecksumMismatch error, do NOT attempt decryption
3. decrypt_chunk(wire_blob, file_key, file_id, chunk_index)
   → padded_plaintext (chunk_size bytes)
4. If this is the last chunk:
   bytes_to_write = file_size - (chunk_index * chunk_size)
   Write only bytes_to_write bytes to destination via BufWriter
5. Else: write full chunk_size bytes to destination
6. Zeroize padded_plaintext
```

### Streaming invariant

At no point is more than one chunk's worth of plaintext in memory simultaneously. The `BufReader` reads `chunk_size` bytes, the chunk is encrypted, the plaintext buffer is zeroed, and the next chunk is read.

---

## File Key Lifecycle

### New file upload

```
1. Generate file_key (random 256-bit via CSPRNG)
2. Wrap: file_key_wrapped = encrypt(file_key, key_encryption_key)
3. Begin SQLCipher transaction
4. Insert node row (node_id, name, size_bytes, file_key_wrapped, ...)
5. Encrypt all chunks → ChunkRecords (blobs written to staging)
6. Insert chunk rows for all ChunkRecords
7. Commit transaction
8. Zeroize file_key
```

If the transaction fails (crash, I/O error), no manifest state exists for the partial file. Orphaned blobs in staging are cleaned up on next startup.

### File access (decrypt/download)

```
1. Read file_key_wrapped from nodes table
2. Unwrap: file_key = decrypt(file_key_wrapped, key_encryption_key)
3. Read chunk rows (ordered by chunk_index)
4. Decrypt each chunk using file_key
5. Zeroize file_key when done
```

### File deletion

```
1. Begin SQLCipher transaction
2. Read chunk rows → list of blob_names
3. Delete node row (CASCADE deletes chunk rows and file_key_wrapped)
4. Commit transaction
5. Delete blob files from staging and/or cloud
```

---

## Staging Directory

### Location

A subdirectory of the VoidGate application data directory:
- Windows: `%APPDATA%/voidgate/staging/`
- Linux: `~/.local/share/voidgate/staging/`

### Lifecycle

1. **Write**: blobs are created during `encrypt_file`, named `<uuid>.blob`
2. **Upload**: Phase 4 (cloud sync) reads from staging and uploads via Rclone
3. **Delete**: after confirmed upload, the staging copy is deleted
4. **Cleanup**: on startup, VoidGate scans the staging directory for blobs not referenced by any `chunks.blob_name` in the manifest → delete them (orphans from interrupted operations)

### Security

Staging blobs are encrypted (AEAD ciphertext). Leaving them on disk does not expose plaintext. The staging directory is local storage, not synced to cloud — the cloud transport layer handles uploads separately.

---

## Error Recovery

### Crash during encrypt

- The SQLCipher transaction (steps 3-7 in "New file upload") has not committed
- No manifest state exists for the partial file
- Orphaned blobs in staging are cleaned up on next startup

### Crash during decrypt

- Partial output file may exist at the destination
- On retry, the decrypt operation overwrites the destination file from the beginning
- No manifest state changes during decrypt (it's read-only)

### Crash during delete

- If the transaction committed (node row deleted): chunk rows are gone via CASCADE; blob files may still exist → cleaned up on next startup (orphan scan)
- If the transaction did not commit: file still exists in manifest, no data loss

### Transaction model

All manifest mutations (insert, update, delete) are wrapped in SQLCipher transactions. The manifest is never in a partially updated state.

---

## MetadataStore Trait

```rust
/// Abstraction over the manifest database for testability.
trait MetadataStore: Send + Sync {
    /// Inserts a new node (file or directory).
    async fn insert_node(&self, node: &Node) -> Result<(), StorageError>;

    /// Inserts chunk records for a file.
    async fn insert_chunks(&self, chunks: &[ChunkRecord]) -> Result<(), StorageError>;

    /// Retrieves a node by ID.
    async fn get_node(&self, node_id: Uuid) -> Result<Node, StorageError>;

    /// Lists children of a directory.
    async fn list_children(&self, parent_id: Uuid) -> Result<Vec<Node>, StorageError>;

    /// Retrieves all chunks for a file, ordered by chunk_index.
    async fn get_chunks(&self, node_id: Uuid) -> Result<Vec<ChunkRecord>, StorageError>;

    /// Deletes a node and cascades to chunks.
    async fn delete_node(&self, node_id: Uuid) -> Result<Vec<String>, StorageError>;
    // Returns list of blob_names for cloud deletion

    /// Reads manifest_meta value by key.
    async fn get_meta(&self, key: &str) -> Result<Option<String>, StorageError>;

    /// Sets manifest_meta value.
    async fn set_meta(&self, key: &str, value: &str) -> Result<(), StorageError>;

    /// Increments and returns the new snapshot_counter.
    async fn increment_snapshot_counter(&self) -> Result<u64, StorageError>;
}
```

Implementations:
- `SqlCipherMetadataStore` — production implementation backed by SQLCipher
- `MockMetadataStore` — in-memory HashMap for testing without a database

---

## Open Decisions

| Decision | Options | Status |
|----------|---------|--------|
| Upload order randomisation | Randomise blob upload order to mask which blobs belong to the same file vs. sequential for simplicity | Extension point, not blocking |
| Maximum file size | Implicit limit from chunk_index as u32 (2^32 chunks × 4 MiB = 16 PiB per file) — no practical limit needed | Not blocking |

---

## Decisions Made

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Chunk size | 4 MiB | Half the padding waste of 8 MiB, lower memory, finer resume granularity |
| Padding | Zero-pad to chunk_size, truncate via `size_bytes` on reassembly | Simple, unambiguous, cloud sees uniform blob sizes |
| `file_key_wrapped` location | `nodes` table (per-file) | Eliminates N redundant copies in chunks table; CASCADE still works |
| 0-byte files | Node row with no chunks, `size_bytes = 0` | Clean edge case, file_key still generated for future updates |
| Staging directory | App data subdirectory | Encrypted blobs safe on disk, cleaned up on startup |
| Error recovery | SQLCipher transactions + orphan blob cleanup | No partial manifest state, no data loss |
