# Arx Runa — Chunking and Manifest Design

> Status: Design complete. Implementation target: Phase 3.
> Last updated: 2026-04-08

---

## Goals

- Files are split into fixed-size chunks, uniformly padded, and encrypted individually
- The cloud sees only opaque, identically sized blobs with random UUID v4 names — no file size, filename, or structure information leaks
- The local SQLCipher manifest database tracks the mapping between virtual filesystem entries and encrypted blobs
- All file I/O is streaming — no complete file is ever loaded into a single buffer
- Chunk encryption uses per-file `file_key` values (from Phase 1 design)

---

## Contract Surface

### Interface contract

- Storage API surface is `encrypt_file` / `decrypt_file` plus the `MetadataStore` trait methods (`insert_node`, `insert_chunks`, `get_chunks`, `delete_node`, `increment_snapshot_counter`, and related metadata operations).
- `ChunkRecord` is the canonical per-chunk contract between encryption and metadata persistence.
- File upload/access/delete flows are transaction-backed and define how chunk/blob records are created, read, and removed.

### Data contract

- Canonical manifest tables are `nodes`, `chunks`, and `manifest_meta` with UUID identifiers and `UNIQUE(node_id, chunk_index)`.
- `nodes.file_key_wrapped` is stored once per file; `chunks` stores `blob_name`, `chunk_index`, `size_padded`, and `blake3_checksum`.
- `manifest_meta.chunk_size_bytes`, `manifest_meta.epoch_buffer_enabled`, and `manifest_meta.snapshot_counter` are canonical metadata keys consumed by later phases.

### Invariant contract

- `chunk_size_bytes` is immutable per vault; every chunk is padded to that exact size and reassembly truncates via `nodes.size_bytes`.
- Routing mode is stable per vault: with `epoch_buffer_enabled = false`, all files follow standalone chunk uploads; with `epoch_buffer_enabled = true`, files smaller than `chunk_size_bytes` are routed to epoch buffering while files `>= chunk_size_bytes` remain immediate standalone uploads.
- Chunk cryptographic context is fixed: `AAD = file_id || chunk_index`; BLAKE3 verification occurs before decrypt.
- Streaming invariant holds at most one chunk plaintext buffer in memory; node deletion cascades chunk-row deletion.
- Cross-phase invariant reference: [docs/architecture/design-invariants.md](../../design-invariants.md).

### Dependency contract

- Depends on Phase 1 crypto contracts (`encrypt_chunk`, `decrypt_chunk`, file-key wrapping, BLAKE3 checksums, UUID blob naming).
- Provides manifest + staging contracts consumed by cloud synchronisation push/pull, conflict checks, and garbage collection.
- Depends on SQLCipher (`rusqlite`) and `async-trait` for production (`SqlCipherMetadataStore`) and test (`MockMetadataStore`) implementations.

---

## Chunk Size

**Chunk size is set once at vault creation and is immutable thereafter.**

Valid range: 131,072 bytes (128 KiB) to 67,108,864 bytes (64 MiB). Default: 4,194,304 bytes (4 MiB).

Chunk size is a **privacy vs. storage efficiency dial**, not a performance parameter:

- **Larger chunk size** → wider blob-count inference range → stronger privacy. An adversary correlating upload timing learns only that a file falls within a range of ±chunk_size. At 64 MiB, a 5 MiB document and a 60 MiB document both produce one blob — indistinguishable.
- **Smaller chunk size** → narrower inference range → lower storage overhead. A 512 KiB chunk size reduces padding waste for small files at the cost of giving the adversary a tighter size estimate.

Chunk size is immutable after creation because changing it would require downloading, re-encrypting, and re-uploading every blob in the vault — equivalent to recreating the vault. All blobs within a vault are identically sized, preserving the anonymity set.

The chosen `chunk_size_bytes` is stored in `manifest_meta` and validated on every vault open.

### Hybrid auto-routing when `epoch_buffer_enabled = true`

`epoch_buffer_enabled` is an opt-in vault setting (default off) that controls small-file routing:

- Files with `size_bytes < chunk_size_bytes` are staged into an epoch buffer and packed before upload.
- Files with `size_bytes >= chunk_size_bytes` bypass epoch buffering and use immediate standalone chunk upload.
- The trailing partial chunk of large files is not deferred to epoch buffering; it is padded and uploaded in the same immediate upload path.

This applies Approach 7 from `docs/research/padding-overhead-reduction.md` while preserving immediate large-file backup behavior.

### Quantified padding waste (at default 4 MiB)

Every file's last chunk is zero-padded to `chunk_size_bytes`. The overhead depends on `file_size mod chunk_size`:

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

For files larger than one chunk the maximum waste is < chunk_size (constant, not proportional to file size).

Per-chunk crypto overhead: 24 bytes (nonce) + 16 bytes (Poly1305 tag) = 40 bytes. Negligible at any chunk size in the valid range.
The table applies directly to standalone mode and to the large-file path when hybrid routing is enabled.
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
-- last_synced_at is not seeded; set on first successful push
-- ('chunk_size_bytes', '4194304')   -- immutable; validated on every open
-- ('epoch_buffer_enabled', 'false') -- user opt-in at vault creation

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

## Pre-Encryption Processing: EXIF Stripping

### Purpose

Media files (JPEG, PNG, TIFF, video containers) may contain EXIF, XMP, or IPTC metadata that reveals sensitive information: GPS coordinates, camera model, timestamps, lens settings, and software versions. This metadata is encrypted along with the file content, but stripping it before encryption reduces the risk surface if a file is later exported or shared outside Arx Runa.

### Behaviour

EXIF stripping is an optional pre-processing step that runs in RAM before the encrypt pipeline. It is enabled by default for media file types and can be disabled per vault in settings.

**Supported file types** (detected by magic bytes, not file extension):

| MIME type | Metadata formats stripped |
|-----------|--------------------------|
| `image/jpeg` | EXIF, XMP, IPTC |
| `image/png` | eXIf chunk, XMP (tEXt/iTXt) |
| `image/tiff` | EXIF, XMP, IPTC |

**Unsupported types** (including `video/mp4` and `video/quicktime`) pass through to the encrypt pipeline unmodified.

> **Note — MP4/QuickTime**: The `moov` atom, which contains GPS coordinates and all file-level metadata, is placed at the **end** of the file in typical device recordings (`[ftyp][mdat][moov]`). A streaming single-pass read cannot reach moov without reading the entire file. MP4/QuickTime metadata stripping is therefore excluded from this pipeline to preserve the streaming invariant. Users who need GPS removed from video files should use an external tool (e.g. `ffmpeg -movflags +faststart` followed by ExifTool) before upload. Video stripping is an open question for a future non-streaming pre-processing step.

### Flow

```
1. Read file into chunk-sized buffer (streaming, same as encrypt pipeline)
2. If first buffer contains a recognised media magic byte sequence:
   a. Parse metadata segments from the buffer
   b. Remove EXIF, XMP, and IPTC segments
   c. Rewrite the file header in-place in the buffer
3. Pass the (possibly modified) buffer to the encrypt pipeline
4. Original file on disk is never modified
```

### Implementation

The `kamadak-exif` crate provides EXIF parsing. For rewriting JPEG files without EXIF segments, the `img-parts` crate can split a JPEG into segments and reassemble without the APP1 (EXIF) and APP13 (IPTC) segments.

<!-- CITE: kamadak-exif crate — https://crates.io/crates/kamadak-exif -->
<!-- CITE: img-parts crate — https://crates.io/crates/img-parts -->

### Security property

Stripping occurs in RAM. The original file on disk is never modified by Arx Runa. The stripped content is what enters the encrypt pipeline and is stored in the cloud. If the user later exports the file from Arx Runa, the exported copy will not contain EXIF metadata.

### Scope

Implementation target: Phase 3 (alongside the encrypt pipeline) or Phase 6 (as a UI toggle). Not blocking for the core encrypt/decrypt cycle.

---

## Encrypt Pipeline

### Public API

```rust
/// A chunk record — both the output of encrypt_file and the type loaded
/// from MetadataStore::get_chunks for decryption.
struct ChunkRecord {
    chunk_id:        Uuid,
    chunk_index:     u32,
    blob_name:       String,       // UUID v4; no relation to file identity
    size_padded:     u64,          // always chunk_size
    blake3_checksum: [u8; 32],
    // blob_path is intentionally absent: the staging path is derived at the
    // call site as staging_directory/<blob_name>.blob and is not persisted.
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

A subdirectory of the Arx Runa application data directory:
- Windows: `%APPDATA%/arx-runa/staging/`
- Linux: `~/.local/share/arx-runa/staging/`

### Lifecycle

1. **Write**: blobs are created during `encrypt_file`, named `<uuid>.blob`
2. **Upload**: Phase 4 (cloud sync) reads from staging and uploads via Rclone
3. **Delete**: after confirmed upload, the staging copy is deleted
4. **Cleanup**: on startup, Arx Runa scans the staging directory for blobs not referenced by any `chunks.blob_name` in the manifest → delete them (orphans from interrupted operations). Global `chunks.blob_name` enumeration is performed via a SQLCipher-specific query helper in the storage implementation, not via the `MetadataStore` trait.

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
use async_trait::async_trait;

/// Abstraction over the manifest database for testability.
#[async_trait]
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

    /// Renames a node. Updates modified_at to the provided Unix timestamp.
    async fn rename_node(
        &self,
        node_id: Uuid,
        new_name: &str,
        modified_at: i64,
    ) -> Result<(), StorageError>;

    /// Moves a node to a new parent directory. Updates modified_at.
    /// Pass None for new_parent_id to move to the root.
    async fn move_node(
        &self,
        node_id: Uuid,
        new_parent_id: Option<Uuid>,
        modified_at: i64,
    ) -> Result<(), StorageError>;

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

// Both concrete implementations require the attribute:
// #[async_trait]
// impl MetadataStore for SqlCipherMetadataStore { ... }
//
// #[async_trait]
// impl MetadataStore for MockMetadataStore { ... }
```

Implementations:
- `SqlCipherMetadataStore` — production implementation backed by SQLCipher
- `MockMetadataStore` — in-memory HashMap for testing without a database

---

## Open Decisions

| Decision | Options | Status |
|----------|---------|--------|
| Upload order randomisation | Randomise blob upload order to mask which blobs belong to the same file vs. sequential for simplicity | Extension point for Phase 4. Security rationale: sequential upload leaks temporal correlation — an observer sees which blobs belong to the same file by their upload timestamps, even though blob names are random UUIDs. Fisher-Yates shuffle of the upload queue eliminates this signal. See Phase 4 design |
| Maximum file size | Implicit limit from chunk_index as u32 (2^32 chunks × `chunk_size_bytes`; default 4 MiB gives 16 PiB per file) — no practical limit needed | Not blocking |
| Video metadata stripping | MP4/QuickTime excluded from EXIF stripping pipeline (moov-at-end incompatible with streaming). A future non-streaming pre-processing step could handle this | Deferred |

---

## Decisions Made

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Default chunk size | 4 MiB (`chunk_size_bytes`, user-configurable at vault creation) | Half the padding waste of 8 MiB, lower memory, finer resume granularity |
| Epoch buffer implementation | Hybrid auto-routing (opt-in via `epoch_buffer_enabled`) | Small files benefit from packing and timing privacy, while large files remain immediately available in cloud |
| Padding | Zero-pad to chunk_size, truncate via `size_bytes` on reassembly | Simple, unambiguous, cloud sees uniform blob sizes |
| `file_key_wrapped` location | `nodes` table (per-file) | Eliminates N redundant copies in chunks table; CASCADE still works |
| 0-byte files | Node row with no chunks, `size_bytes = 0` | Clean edge case, file_key still generated for future updates |
| Staging directory | App data subdirectory | Encrypted blobs safe on disk, cleaned up on startup |
| Error recovery | SQLCipher transactions + orphan blob cleanup | No partial manifest state, no data loss |
| `MetadataStore` async dispatch | `#[async_trait]` macro | `async fn` in traits is not dyn-safe; `#[async_trait]` makes `Box<dyn MetadataStore>` compile while keeping the trait readable. Requires `async-trait = "0.1"` in `Cargo.toml` |
| MP4/QuickTime EXIF stripping | Drop vs full-file read vs two-pass seek | Dropped: moov atom at end-of-file on device recordings breaks streaming; video stripping deferred |
| Schema CHECK constraints | DDL enforcement vs prose-only | CHECK constraints added: catch corrupt node rows at write time; cross-column constraint enforces file_key_wrapped nullability invariant |
| `ChunkRecord.blob_path` | Remove vs split into two structs | Removed: only used between encrypt_file and insert_chunks; staging path derived from blob_name at call site |
| `received_shares.chunk_uuids` format | JSON array + CHECK vs normalised table vs comment-only | JSON array with `json_valid()` CHECK: consistent with share package format, enforced at write time |
| Rename/move in MetadataStore | Two focused methods vs `update_node(NodePatch)` vs defer | Two focused methods: `rename_node` and `move_node`; consistent naming with existing trait methods |

---

## Related Documents

- [Chunk Pipeline Diagram](diagrams/chunk-pipeline.md)
- [Manifest Schema Diagram](diagrams/manifest-schema.md)
- [Cryptographic Primitives](../cryptographic-primitives/design.md) — `encrypt_chunk`, `file_key`, BLAKE3
- [Cloud Synchronisation](../cloud-synchronisation/design.md) — blob upload/download
- Roadmap Phase 3 deliverables
