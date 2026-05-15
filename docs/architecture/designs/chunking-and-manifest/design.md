# Arx Runa — Chunking and Manifest Design

> Status: Design complete. Implementation live.
> Last updated: 2026-05-15

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

- Storage API surface is `encrypt_file` / `encrypt_bytes` / `decrypt_file` / `decrypt_epoch_file` plus the `MetadataStore` trait methods (`insert_node`, `insert_chunks`, `insert_file_with_chunks`, `get_chunks`, `delete_node`, `list_pending_deletions`, `mark_deletion_complete`, `increment_snapshot_counter`, and epoch-routing operations).
- `ChunkRecord` is the canonical per-chunk contract between encryption and metadata persistence.
- File upload/access/delete flows are transaction-backed and define how chunk/blob records are created, read, and removed.

### Data contract

- Core manifest tables are `nodes`, `chunks`, `manifest_meta`, and `pending_deletions`. Standalone chunks have `UNIQUE(node_id, chunk_index)` and a partial unique index on `blob_name WHERE blob_name IS NOT NULL`. Epoch-buffered chunks carry `epoch_blob_id`, `byte_offset`, and `byte_length` instead of `blob_name`.
- `nodes.file_key_wrapped` is stored once per file; `chunks` stores `chunk_index`, `size_padded`, `blake3_checksum`, and either `blob_name` (standalone) or `epoch_blob_id` + `byte_offset` + `byte_length` (epoch).
- `manifest_meta.chunk_size_bytes`, `manifest_meta.epoch_buffer_enabled`, and `manifest_meta.snapshot_counter` are canonical metadata keys consumed by later phases.
- `manifest_meta` mutability policy: `schema_version`, `vault_id`, `snapshot_counter`, `chunk_size_bytes`, and `epoch_buffer_enabled` are immutable via `set_meta`; `snapshot_counter` advances only through `increment_snapshot_counter`.

### Invariant contract

- `chunk_size_bytes` is immutable per vault; every chunk is padded to that exact size and reassembly truncates via `nodes.size_bytes`.
- Routing mode is stable per vault: with `epoch_buffer_enabled = false`, all files follow standalone chunk uploads; with `epoch_buffer_enabled = true`, files smaller than `chunk_size_bytes` are routed to epoch buffering while files `>= chunk_size_bytes` remain immediate standalone uploads.
- Chunk cryptographic context is fixed: `AAD = file_id || chunk_index`; BLAKE3 verification occurs before decrypt.
- Streaming invariant holds at most one chunk plaintext buffer in memory; node deletion cascades chunk-row deletion.
- Hierarchy invariant: parent targets for insert/move must be directories, self-parent is forbidden, and move operations must not introduce cycles.
- SQLCipher key handling invariant: metadata-open/create applies SQLCipher keys from protected wrappers (no by-value raw stack copies in the open/create/keying flow).
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

- **Larger chunk size** → wider blob-count inference range → stronger privacy. An adversary correlating upload timing learns only that a file falls within a range of ±chunk_size_bytes. At 64 MiB, a 5 MiB document and a 60 MiB document both produce one blob — indistinguishable.
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

Every file's last chunk is zero-padded to `chunk_size_bytes`. The overhead depends on `file_size mod chunk_size_bytes`:

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

For files larger than one chunk the maximum waste is < chunk_size_bytes (constant, not proportional to file size).

Per-chunk crypto overhead: 24 bytes (nonce) + 16 bytes (Poly1305 tag) = 40 bytes. Negligible at any chunk size in the valid range.
The table applies directly to standalone mode and to the large-file path when hybrid routing is enabled.
<!-- CITE: Breaking and Fixing Content-Defined Chunking — https://eprint.iacr.org/2025/558.pdf — supports fixed-size chunking for metadata privacy over CDC -->

---

## Padding Scheme

**Zero-pad to `chunk_size_bytes`, truncate on reassembly using `size_bytes` from manifest.**

### Encrypt path

Each chunk's plaintext is zero-filled to exactly `chunk_size_bytes` bytes before encryption. If the file's last segment is shorter than `chunk_size_bytes`, the remaining bytes are filled with `0x00`.

### Decrypt path

On reassembly, the file's `size_bytes` from the `nodes` table determines where to truncate the last chunk's decrypted output. All preceding chunks are written in full (`chunk_size_bytes` bytes each); the last chunk is truncated to `size_bytes - (chunk_count - 1) * chunk_size_bytes`.

### Security property

The cloud sees N identically sized encrypted blobs. It cannot distinguish content from padding because the padding is encrypted by XChaCha20-Poly1305. The `size_bytes` field is inside the SQLCipher database (encrypted with `sqlcipher_key`). The cloud learns only the total number of blobs in the vault — not which blobs belong to which file, nor individual file sizes.

### 0-byte files

A 0-byte file has no chunks. The `nodes` row exists with `size_bytes = 0` and `file_key_wrapped` (still generated — needed if the file is later updated or shared). On decrypt, `size_bytes = 0` means no chunks to fetch.

---

## Manifest Database Schema

### Live schema (SQLCipher, keyed with `sqlcipher_key`) — schema_version 9

New vaults are created with the canonical schema below. Existing vaults are migrated through versions 1–9 automatically on open. Column annotations marked `[v2]`–`[v9]` were introduced by migrations.

```sql
-- Core tables

CREATE TABLE nodes (
    node_id          TEXT PRIMARY KEY,     -- UUID v4
    parent_id        TEXT REFERENCES nodes(node_id) ON DELETE CASCADE,
    node_type        TEXT NOT NULL
                         CHECK (node_type IN ('file', 'directory')),
    name             TEXT NOT NULL,        -- plaintext (SQLCipher is the encryption layer)
    created_at       INTEGER NOT NULL,     -- Unix timestamp
    modified_at      INTEGER NOT NULL,     -- Unix timestamp
    size_bytes       INTEGER NOT NULL,     -- original file size (0 for directories)
    file_key_wrapped BLOB                  -- NULL for directories, NOT NULL for files
                         CHECK ((node_type = 'file'      AND file_key_wrapped IS NOT NULL)
                             OR (node_type = 'directory' AND file_key_wrapped IS NULL))
);

-- chunks: either standalone (blob_name set) or epoch-buffered (epoch_blob_id set).
-- Exactly one mode per row enforced by the CHECK constraint.
CREATE TABLE chunks (
    chunk_id         TEXT PRIMARY KEY,     -- UUID v4
    node_id          TEXT NOT NULL REFERENCES nodes(node_id) ON DELETE CASCADE,
    chunk_index      INTEGER NOT NULL,     -- 0-based
    blob_name        TEXT,                 -- [v2] nullable; NULL for epoch chunks
    size_padded      INTEGER NOT NULL,     -- equals chunk_size_bytes (standalone) or epoch blob size (epoch)
    blake3_checksum  BLOB NOT NULL,        -- 32 bytes, over encrypted blob
    epoch_blob_id    TEXT REFERENCES epoch_blobs(epoch_blob_id), -- [v2] NULL for standalone chunks
    byte_offset      INTEGER,             -- [v2] byte offset within epoch blob; NULL for standalone
    byte_length      INTEGER,             -- [v2] byte length within epoch blob; NULL for standalone
    UNIQUE(node_id, chunk_index),
    CHECK (
        (blob_name IS NOT NULL AND epoch_blob_id IS NULL
             AND byte_offset IS NULL AND byte_length IS NULL) OR
        (blob_name IS NULL AND epoch_blob_id IS NOT NULL
             AND byte_offset IS NOT NULL AND byte_length IS NOT NULL)
    )
);
-- Partial unique index (replaces the old UNIQUE(blob_name) table constraint):
CREATE UNIQUE INDEX idx_chunks_blob_name ON chunks(blob_name) WHERE blob_name IS NOT NULL;

CREATE TABLE manifest_meta (
    key              TEXT PRIMARY KEY,
    value            TEXT NOT NULL
);
-- Rows (schema_version increments with each migration):
-- ('schema_version', '9')
-- ('vault_id', '<uuid>')
-- ('snapshot_counter', '0')
-- ('chunk_size_bytes', '4194304')   -- immutable; validated on every open
-- ('epoch_buffer_enabled', 'false') -- user opt-in at vault creation
-- 'last_synced_at' not seeded; set on first successful push

CREATE TABLE pending_deletions (
    blob_name        TEXT PRIMARY KEY,     -- UUID v4 blob name queued for cloud deletion
    queued_at        INTEGER NOT NULL      -- Unix timestamp
);

-- Epoch buffering tables [v2]

CREATE TABLE epoch_blobs (
    epoch_blob_id    TEXT PRIMARY KEY,     -- UUID v4
    blob_name        TEXT NOT NULL UNIQUE, -- UUID v4, the actual staged file name
    file_key_wrapped BLOB NOT NULL,        -- epoch blob's own file_key, wrapped with KEK
    size_padded      INTEGER NOT NULL,     -- total padded blob size
    blake3_checksum  BLOB NOT NULL         -- 32 bytes, over the encrypted epoch blob
);

CREATE TABLE epoch_buffer (
    entry_id    TEXT PRIMARY KEY,          -- UUID v4
    node_id     TEXT NOT NULL REFERENCES nodes(node_id) ON DELETE CASCADE,
    plaintext   BLOB NOT NULL,             -- in-RAM plaintext; never written to disk unencrypted
    size_bytes  INTEGER NOT NULL,
    queued_at   INTEGER NOT NULL           -- Unix timestamp
);

-- Phase 2.4/5 identity table

CREATE TABLE vault_identity (
    id                   INTEGER PRIMARY KEY CHECK (id = 1),
    public_key           BLOB NOT NULL UNIQUE,
    wrapped_private_key  BLOB NOT NULL
);

-- Phase 4 destination sessions

CREATE TABLE destination_sessions (
    destination_id     TEXT PRIMARY KEY,
    label              TEXT NOT NULL,
    destination_type   TEXT NOT NULL
                           CHECK (destination_type IN ('cloud', 'external_drive', 'local_path')),
    rclone_remote_name TEXT NOT NULL,
    rclone_config_blob TEXT NOT NULL,      -- encrypted Rclone config section
    bucket             TEXT NOT NULL DEFAULT '',
    path_prefix        TEXT NOT NULL DEFAULT '',
    is_primary         INTEGER NOT NULL DEFAULT 0 CHECK (is_primary IN (0, 1)),
    backup_mode        TEXT
                           CHECK (backup_mode IS NULL OR backup_mode IN ('mirror', 'accumulating')),
    created_at         INTEGER NOT NULL,
    device_id          TEXT               -- [v7] device identifier for this destination
);

-- Phase 5 sharing tables

CREATE TABLE contacts (
    contact_id   TEXT PRIMARY KEY,
    display_name TEXT NOT NULL,
    email        TEXT,
    public_key   BLOB NOT NULL,
    created_at   INTEGER NOT NULL
);

CREATE TABLE shares (
    share_id                   TEXT PRIMARY KEY,
    file_id                    TEXT NOT NULL REFERENCES nodes(node_id) ON DELETE CASCADE, -- [v9] CASCADE added
    contact_id                 TEXT NOT NULL REFERENCES contacts(contact_id),
    file_share_id              TEXT NOT NULL,
    cloud_path                 TEXT NOT NULL,
    created_at                 INTEGER NOT NULL,
    expires_at                 INTEGER,
    revoked_at                 INTEGER,
    download_key_id            TEXT,      -- [v3]
    receipt_requested          INTEGER NOT NULL DEFAULT 0, -- [v3]
    receipt_received_at        INTEGER,   -- [v3]
    import_receipt_received_at INTEGER,   -- [v4]
    download_folder_id         TEXT       -- [v8] Drive folder ID for revocation
);

CREATE TABLE received_shares (
    share_id          TEXT PRIMARY KEY,
    sender_contact_id TEXT REFERENCES contacts(contact_id),
    sender_public_key BLOB NOT NULL,      -- X25519 public key, 32 bytes
    file_id           TEXT NOT NULL,      -- file node identifier (UUID v4)
    file_name         TEXT NOT NULL,
    file_key_wrapped  BLOB NOT NULL,
    chunk_count       INTEGER NOT NULL,
    chunk_size        INTEGER NOT NULL,
    chunk_uuids       TEXT NOT NULL CHECK (json_valid(chunk_uuids)),
    cloud_endpoint    TEXT NOT NULL CHECK (json_valid(cloud_endpoint)),
    expires_at        INTEGER,
    imported_at       INTEGER NOT NULL
);

-- Phase 7 backup tracking tables [v5, v6]

CREATE TABLE backup_upload_failures (
    blob_name      TEXT NOT NULL,
    destination_id TEXT NOT NULL,
    failed_at      INTEGER NOT NULL,
    retry_count    INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (blob_name, destination_id)
);

CREATE TABLE pending_backup (
    blob_name      TEXT NOT NULL,
    destination_id TEXT NOT NULL,
    PRIMARY KEY (blob_name, destination_id)
);

-- GDrive sharing config [v8]

CREATE TABLE sharing_config (
    config_id      TEXT PRIMARY KEY,
    destination_id TEXT,
    provider       TEXT NOT NULL CHECK (provider IN ('gdrive')),
    config_json    TEXT NOT NULL CHECK (json_valid(config_json)),
    created_at     INTEGER NOT NULL,
    updated_at     INTEGER NOT NULL
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
- `CREATE UNIQUE INDEX idx_chunks_blob_name ON chunks(blob_name) WHERE blob_name IS NOT NULL` enforces global blob-name uniqueness for standalone chunks (UUID v4 collisions are improbable but must still fail safely); epoch chunk rows have `blob_name = NULL` and are excluded from this index

---

## Pre-Encryption Processing: EXIF Stripping

### Purpose

Media files (JPEG, PNG, video containers) may contain EXIF, XMP, or IPTC metadata that reveals sensitive information: GPS coordinates, camera model, timestamps, lens settings, and software versions. This metadata is encrypted along with the file content, but stripping it before encryption reduces the risk surface if a file is later exported or shared outside Arx Runa.

### Behaviour

EXIF stripping is an optional pre-processing step that runs in RAM before the encrypt pipeline. It is enabled by default for supported image types.

**Supported file types** (detected by magic bytes, not file extension):

| Format | Segments/chunks stripped |
|--------|--------------------------|
| JPEG (`FF D8`) | APP1 (0xE1 — EXIF/XMP), APP2 (0xE2 — XMP extended/ICC), APP13 (0xED — IPTC) |
| PNG (`89 50 4E 47…`) | `eXIf`, `tEXt`, `iTXt`, `zTXt` chunks |

All other segments/chunks (JPEG APP0/SOF/SOS/compressed bitstream, PNG IHDR/IDAT/IEND/PLTE, etc.) are preserved so the output is a valid, viewable image.

**Unsupported types** (including `video/mp4`, `video/quicktime`, and TIFF) pass through to the encrypt pipeline unmodified.

> **Note — MP4/QuickTime**: The `moov` atom (GPS coordinates, all file-level metadata) is at the end of typical device recordings. A streaming single-pass read cannot reach it without reading the entire file, so MP4/QuickTime stripping is excluded to preserve the streaming invariant. Users needing GPS removed from video should use an external tool before upload.

### Flow

```
1. Read entire file into RAM (Vec<u8>)
2. Check magic bytes via is_image_magic()
3. If recognised: strip_exif() rewrites the byte stream in RAM, returning cleaned bytes
4. Pass the (possibly modified) bytes to encrypt_bytes() which chunks and encrypts them
5. Original file on disk is never modified
```

Note: loading the whole file into RAM is a deliberate exception to the per-chunk streaming invariant, scoped only to EXIF-eligible files. The encrypt pipeline's `encrypt_bytes` entry point handles in-RAM chunking.

### Implementation

The EXIF stripper (`storage::pipeline::exif`) is a hand-written byte-level parser — no external EXIF crates are used.

**JPEG**: iterates the segment stream starting after the SOI marker (`FF D8`). Each segment is identified by its marker byte; segments with marker `0xE1` (APP1), `0xE2` (APP2), or `0xED` (APP13) are omitted from the output. All other segments are copied verbatim. After the SOS marker (`0xDA`), the compressed bitstream is copied in full without further parsing.

**PNG**: iterates the chunk stream after the 8-byte signature. Each chunk's type field is read; chunks of type `eXIf`, `tEXt`, `iTXt`, or `zTXt` are omitted. All other chunks (IHDR, IDAT, PLTE, IEND, etc.) are copied verbatim including their CRC fields.

### Security property

Stripping occurs in RAM. The original file on disk is never modified by Arx Runa. The stripped content is what enters the encrypt pipeline and is stored in the cloud. If the user later exports the file from Arx Runa, the exported copy will not contain EXIF metadata.

### Scope

Implemented in Phase 3 alongside the encrypt pipeline (`storage::pipeline::exif`). JPEG and PNG are supported. Video and TIFF pass through unmodified.

---

## Encrypt Pipeline

### Public API

```rust
/// A chunk record — both the output of encrypt_file/encrypt_bytes and the type
/// loaded from MetadataStore::get_chunks for decryption.
///
/// Standalone chunks: blob_name is set, epoch fields are None.
/// Epoch chunks: blob_name is None, epoch_blob_id/byte_offset/byte_length are set.
struct ChunkRecord {
    chunk_id:        Uuid,
    node_id:         NodeId,
    chunk_index:     u32,
    blob_name:       Option<String>,    // UUID v4; None for epoch chunks
    size_padded:     u64,               // chunk_size_bytes (standalone) or epoch blob size (epoch)
    blake3_checksum: [u8; 32],
    epoch_blob_id:   Option<Uuid>,      // set for epoch chunks only
    byte_offset:     Option<u64>,       // byte offset within epoch blob
    byte_length:     Option<u64>,       // byte length within epoch blob
    // blob_path is intentionally absent: the staging path is derived at the
    // call site as staging_directory/<blob_name>.blob and is not persisted.
}

/// Encrypts a file into padded, encrypted chunks in the staging directory.
/// Returns ChunkRecords; does NOT write to MetadataStore — that is the caller's responsibility.
async fn encrypt_file(
    source: &Path,
    file_id: Uuid,
    file_key: &FileKey,
    metadata_store: &dyn MetadataStore,
    staging_directory: &Path,
    progress: Option<&(dyn Fn(u64, u64) + Send + Sync)>,
) -> Result<Vec<ChunkRecord>, StorageError>;

/// Encrypts in-RAM bytes into padded, encrypted chunks in the staging directory.
/// Used after EXIF stripping when the file has already been loaded into RAM.
/// Returns ChunkRecords; does NOT write to MetadataStore.
async fn encrypt_bytes(
    source_bytes: Vec<u8>,
    file_id: Uuid,
    file_key: &FileKey,
    metadata_store: &dyn MetadataStore,
    staging_directory: &Path,
    progress: Option<&(dyn Fn(u64, u64) + Send + Sync)>,
) -> Result<Vec<ChunkRecord>, StorageError>;

/// Decrypts standalone chunks and reassembles the original file.
/// Output is written to a temp file and atomically renamed to destination.
async fn decrypt_file(
    destination: &Path,
    file_id: Uuid,
    file_key: &FileKey,
    file_size: u64,
    chunks: &[ChunkRecord],
    blob_directory: &Path,
    metadata_store: &dyn MetadataStore,
    progress: Option<&(dyn Fn(u64, u64) + Send + Sync)>,
) -> Result<(), StorageError>;

/// Decrypts a file whose chunks are packed into an epoch blob.
async fn decrypt_epoch_file(
    destination: &Path,
    chunk: &ChunkRecord,
    kek: &KeyEncryptionKey,
    blob_directory: &Path,
    metadata_store: &dyn MetadataStore,
    progress: Option<&(dyn Fn(u64, u64) + Send + Sync)>,
) -> Result<(), StorageError>;
```

### Encrypt flow (per chunk)

```
0. Read `chunk_size_bytes` once from `manifest_meta` via MetadataStore
1. Allocate Zeroizing<Vec<u8>> pre-filled with 0x00 bytes (chunk_size_bytes length)
2. BufReader reads up to `chunk_size_bytes` bytes into the buffer (trailing bytes remain 0x00)
3. Generate AAD = file_id (16 bytes) || chunk_index (u32 big-endian, 4 bytes)
4. encrypt_chunk(padded_buffer, file_key, file_id, chunk_index)
   → wire_blob = [24B nonce | ciphertext | 16B Poly1305 tag]
5. blake3_checksum = blake3::hash(wire_blob)
6. blob_name = Uuid::new_v4()
7. Write wire_blob to staging_directory/<blob_name>.blob via BufWriter
8. Zeroize padded_buffer
9. Return ChunkRecord
```

If any chunk write fails, all blobs staged so far for this file are cleaned up before the error is returned. MetadataStore is not touched — the caller (vault_ops) is responsible for the node/chunk insert transaction.

### Decrypt flow (per chunk)

```
Pre-flight: validate chunks slice — length must equal expected chunk count,
            indices must be contiguous starting at 0 with no gaps or duplicates.
0. Read `chunk_size_bytes` once from `manifest_meta` via MetadataStore
1. Resolve blob path: check pending/<blob_name>.blob, then cache/<blob_name>.blob,
   then staging_directory/<blob_name>.blob (flat fallback)
2. Check file size via fs::metadata — must equal chunk_size_bytes + 40; fail before read
3. Read wire_blob from resolved path via BufReader (read_exact)
4. verify_checksum(wire_blob, expected_blake3) → VerifiedBlob
   (type enforces checksum-before-decrypt at compile time; mismatch → ChecksumMismatch)
5. decrypt_chunk(verified_blob, file_key, file_id, chunk_index)
   → padded_plaintext (chunk_size_bytes bytes)
6. If this is the last chunk:
   bytes_to_write = file_size - (chunk_index * chunk_size_bytes)
   Write only bytes_to_write bytes to destination tmp file via BufWriter
7. Else: write full chunk_size_bytes bytes to destination tmp file
8. Zeroize padded_plaintext
After all chunks: atomically rename <dest>.arx-runa-decrypt-<uuid>.tmp → destination
```

### Streaming invariant

At no point is more than one chunk's worth of plaintext in memory simultaneously. The `BufReader` reads `chunk_size_bytes` bytes, the chunk is encrypted, the plaintext buffer is zeroed, and the next chunk is read.

**Exception — EXIF stripping**: when the magic bytes indicate JPEG or PNG, the entire file is loaded into RAM before chunking (`encrypt_bytes` path). This is a deliberate, scoped relaxation of the invariant for small-to-medium image files.

---

## File Key Lifecycle

### New file upload

Steps 1–3 are in `storage::pipeline` (encrypt_file / encrypt_bytes). Steps 4–8 are in `storage::vault_ops` (upload_file). The pipeline returns `Vec<ChunkRecord>` and does not touch MetadataStore; the orchestration layer owns the transaction.

```
1. Generate file_key (random 256-bit via CSPRNG)
2. Wrap: file_key_wrapped = encrypt(file_key, key_encryption_key)
3. encrypt_file / encrypt_bytes → Vec<ChunkRecord> (blobs written to staging)
   — if any chunk fails, staged blobs for this file are cleaned up before returning
4. Begin SQLCipher transaction (vault_ops)
5. MetadataStore::insert_file_with_chunks(node, &chunks) — atomically inserts node + all chunk rows
6. Commit transaction
7. Zeroize file_key
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
3. Insert blob_names into `pending_deletions`
4. Delete node row (CASCADE deletes chunk rows and file_key_wrapped)
5. Commit transaction
6. Delete blob files from local staging if present (best-effort)
7. Phase 4 sync drains `pending_deletions` for cloud deletion; row removed only after confirmed cloud delete
```

---

## Staging Directory

### Location

A subdirectory of the Arx Runa application data directory:
- Windows: `%APPDATA%/arx-runa/staging/`
- Linux: `~/.local/share/arx-runa/staging/`
- macOS: `~/Library/Application Support/arx-runa/staging/`

### Lifecycle

1. **Write**: blobs are created during `encrypt_file` / `encrypt_bytes`, named `<uuid>.blob`
2. **Upload**: Phase 4 (cloud sync) reads from staging and uploads via Rclone
3. **Delete**: after confirmed upload, the staging copy is deleted
4. **Cleanup**: on startup, Arx Runa scans the staging directory for blobs not referenced by any `chunks.blob_name` in the manifest → delete them (orphans from interrupted operations). Global `chunks.blob_name` enumeration is performed via a SQLCipher-specific query helper in the storage implementation, not via the `MetadataStore` trait.
5. **Cloud-delete queue drain**: Phase 4 sync drains `pending_deletions` and removes queue rows only after confirmed cloud delete.

**Blob resolution order** (used by `decrypt_file`): `pending/<blob_name>.blob` → `cache/<blob_name>.blob` → `<staging_root>/<blob_name>.blob` (flat fallback). The `pending/` and `cache/` subdirectories are managed by the sync layer.

### Security

Staging blobs are encrypted (AEAD ciphertext). Leaving them on disk does not expose plaintext. The staging directory is local storage, not synced to cloud — the cloud transport layer handles uploads separately.

---

## Epoch Buffering

Epoch buffering (`epoch_buffer_enabled = true`) is an opt-in vault setting that packs multiple small files into a single encrypted blob before uploading. This reduces both storage overhead and the number of cloud API calls for vaults containing many small files.

### Routing decision

Hybrid auto-routing is decided by `storage::vault_ops::routing::decide`:

- Files with `size_bytes < chunk_size_bytes` → epoch buffer path
- Files with `size_bytes >= chunk_size_bytes` → standalone path (same as when epoch buffering is disabled)

The trailing partial chunk of large files is not deferred to epoch buffering.

### Epoch buffer path (upload)

```
1. Generate file_key; wrap to file_key_wrapped
2. Load file into RAM (already small — less than chunk_size_bytes)
3. MetadataStore::insert_file_node_and_stage_epoch_entry(node, plaintext)
   — inserts node row AND epoch_buffer entry in one transaction
4. Zeroize file_key (stored as file_key_wrapped in nodes row)
5. Check if epoch_buffer total bytes >= chunk_size_bytes (flush trigger)
6. If flush triggered: call epoch_flush (see below)
```

### Epoch flush

```
1. MetadataStore::get_epoch_buffer_entries() → Vec<EpochBufferEntry>
2. Generate epoch_file_key; concatenate all plaintexts → single buffer
3. Zero-pad buffer to chunk_size_bytes
4. encrypt_chunk(buffer, epoch_file_key, epoch_blob_id, 0)
   → wire_blob = [24B nonce | ciphertext | 16B tag]
5. blake3_checksum = blake3::hash(wire_blob)
6. Write wire_blob to staging as <epoch_blob_name>.blob
7. Wrap epoch_file_key → epoch_file_key_wrapped
8. MetadataStore::commit_epoch_flush(EpochBlobRecord, extents)
   — atomically: insert epoch_blobs row, insert chunk rows with byte_offset/byte_length,
     clear epoch_buffer entries
```

### Epoch decrypt path

`decrypt_epoch_file` handles a file whose chunk record has `epoch_blob_id` set:

```
1. MetadataStore::get_epoch_blob(epoch_blob_id) → EpochBlobRecord
2. Unwrap epoch_file_key from EpochBlobRecord.file_key_wrapped
3. Resolve blob path (same pending/cache/flat resolution as standalone)
4. verify_checksum + decrypt_chunk → padded_plaintext
5. Slice out byte_offset..byte_offset+byte_length from padded_plaintext
6. Write slice to destination (atomic rename)
7. Zeroize plaintext buffer
```

### Schema additions

- `epoch_blobs`: one row per packed blob — stores `blob_name`, `file_key_wrapped`, `size_padded`, `blake3_checksum`
- `epoch_buffer`: staging queue — stores plaintext bytes in-DB (BLOB), cleared on flush
- `chunks`: epoch chunk rows have `blob_name = NULL`, `epoch_blob_id` set, `byte_offset` and `byte_length` set

---

## Error Recovery

### Crash during encrypt

- The SQLCipher transaction (steps 4-7 in "New file upload") has not committed
- No manifest state exists for the partial file
- Orphaned blobs in staging are cleaned up on next startup

### Crash during decrypt

- Partial output file may exist at the destination
- On retry, the decrypt operation overwrites the destination file from the beginning
- No manifest state changes during decrypt (it's read-only)

### Crash during delete

- If the transaction committed (node row deleted): chunk rows are gone via CASCADE; blob files may still exist in cloud, but queued `pending_deletions` rows guarantee retry on next sync/startup
- If the transaction did not commit: file still exists in manifest, no data loss

### Transaction model

All manifest mutations (insert, update, delete) are wrapped in SQLCipher transactions. The manifest is never in a partially updated state.

---

## File Deletion (MVP)

The current design supports **file deletion only**. Directory targets are rejected with `ConstraintViolation` error.

Rationale: Directory deletion requires cascading deletion of all children and their associated blobs. The current implementation focuses on per-file operations for MVP scope.

**Phase 7+ Enhancement**: A dedicated `delete_directory` operation would handle:
- Recursive child enumeration
- Cascade blob deletion from manifest
- Atomic transaction ensuring consistency

---

## MetadataStore Trait

```rust
use async_trait::async_trait;

/// Abstraction over manifest metadata persistence.
#[async_trait]
pub trait MetadataStore: Send + Sync {

    // --- Core node/chunk methods ---

    /// Inserts a node row into the manifest.
    /// Returns ConstraintViolation when primary/foreign/check constraints fail,
    /// or when the provided parent_id is not a directory (including self-parent).
    async fn insert_node(&self, node: &Node) -> Result<(), StorageError>;

    /// Inserts one or more chunk rows into the manifest.
    /// Returns ConstraintViolation for duplicate (node_id, chunk_index) or blob_name collisions.
    async fn insert_chunks(&self, chunks: &[ChunkRecord]) -> Result<(), StorageError>;

    /// Inserts a file node and all associated chunk rows atomically.
    /// Both inserts succeed or neither is persisted.
    async fn insert_file_with_chunks(
        &self,
        node: &Node,
        chunks: &[ChunkRecord],
    ) -> Result<(), StorageError>;

    /// Loads a node by identifier. Returns NotFound when no row matches.
    async fn get_node(&self, node_id: Uuid) -> Result<Node, StorageError>;

    /// Lists direct children for the provided parent node identifier.
    async fn list_children(&self, parent_id: Uuid) -> Result<Vec<Node>, StorageError>;

    /// Returns all chunk rows for a node ordered by chunk_index.
    async fn get_chunks(&self, node_id: Uuid) -> Result<Vec<ChunkRecord>, StorageError>;

    /// Renames a node and updates modified_at in one mutation.
    async fn rename_node(
        &self,
        node_id: Uuid,
        new_name: &str,
        modified_at: i64,
    ) -> Result<(), StorageError>;

    /// Moves a node to a new parent and updates modified_at in one mutation.
    /// new_parent_id = None moves the node to root.
    /// Returns ConstraintViolation when the move violates hierarchy rules.
    async fn move_node(
        &self,
        node_id: Uuid,
        new_parent_id: Option<Uuid>,
        modified_at: i64,
    ) -> Result<(), StorageError>;

    /// Deletes a node and its cascading chunk rows.
    /// Enqueues blob_name entries into pending_deletions in the same transaction.
    async fn delete_node(&self, node_id: Uuid) -> Result<(), StorageError>;

    /// Lists queued blob names from pending_deletions (at most limit entries).
    async fn list_pending_deletions(&self, limit: usize) -> Result<Vec<String>, StorageError>;

    /// Removes a blob name from pending_deletions after successful cloud delete.
    async fn mark_deletion_complete(&self, blob_name: &str) -> Result<(), StorageError>;

    /// Retrieves a manifest-meta value by key.
    async fn get_meta(&self, key: &str) -> Result<Option<String>, StorageError>;

    /// Sets or replaces a manifest-meta key/value pair.
    /// Returns ConstraintViolation for immutable keys: schema_version, vault_id,
    /// snapshot_counter, chunk_size_bytes, epoch_buffer_enabled.
    async fn set_meta(&self, key: &str, value: &str) -> Result<(), StorageError>;

    /// Atomically increments and returns snapshot_counter.
    /// This is the only supported mutation path for snapshot_counter.
    async fn increment_snapshot_counter(&self) -> Result<u64, StorageError>;

    // --- Epoch buffering methods ---

    /// Inserts a file node row without any associated chunk rows.
    /// Used by the epoch routing path before a flush produces chunk rows.
    async fn insert_file_node_only(&self, node: &Node) -> Result<(), StorageError>;

    /// Inserts a file node and stages its plaintext in the epoch buffer atomically.
    /// Crash between node insert and buffer entry cannot occur.
    async fn insert_file_node_and_stage_epoch_entry(
        &self,
        node: &Node,
        plaintext: Vec<u8>,
    ) -> Result<(), StorageError>;

    /// Stages a plaintext entry in the epoch buffer for the given node.
    async fn stage_epoch_entry(
        &self,
        node_id: Uuid,
        plaintext: Vec<u8>,
    ) -> Result<(), StorageError>;

    /// Returns the total number of bytes currently staged in the epoch buffer.
    async fn get_epoch_buffer_total_bytes(&self) -> Result<u64, StorageError>;

    /// Returns all entries currently staged in the epoch buffer.
    async fn get_epoch_buffer_entries(&self) -> Result<Vec<EpochBufferEntry>, StorageError>;

    /// Returns the node IDs of all entries currently staged in the epoch buffer.
    async fn get_epoch_buffer_node_ids(&self) -> Result<Vec<Uuid>, StorageError>;

    /// Returns the number of files currently staged in the epoch buffer.
    async fn get_epoch_buffer_count(&self) -> Result<u32, StorageError> {
        self.get_epoch_buffer_node_ids()
            .await
            .map(|ids| ids.len() as u32)
    }

    /// Atomically: insert epoch_blobs row, insert epoch chunk rows into chunks table,
    /// and clear the flushed epoch_buffer entries.
    /// extents: (node_id, chunk_index, byte_offset, byte_length)
    async fn commit_epoch_flush(
        &self,
        record: &EpochBlobRecord,
        extents: &[(Uuid, u32, u64, u64)],
    ) -> Result<(), StorageError>;

    /// Retrieves an epoch blob record by identifier. Returns NotFound when no row matches.
    async fn get_epoch_blob(&self, epoch_blob_id: Uuid) -> Result<EpochBlobRecord, StorageError>;
}
```

Implementations:
- `SqlCipherMetadataStore` — production implementation backed by SQLCipher
- `MockMetadataStore` — in-memory store for testing without a database

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
| Padding | Zero-pad to chunk_size_bytes, truncate via `size_bytes` on reassembly | Simple, unambiguous, cloud sees uniform blob sizes |
| `file_key_wrapped` location | `nodes` table (per-file) | Eliminates N redundant copies in chunks table; CASCADE still works |
| 0-byte files | Node row with no chunks, `size_bytes = 0` | Clean edge case, file_key still generated for future updates |
| Staging directory | App data subdirectory | Encrypted blobs safe on disk, cleaned up on startup |
| Error recovery | SQLCipher transactions + orphan blob cleanup | No partial manifest state, no data loss |
| `blob_name` uniqueness | Enforce `UNIQUE(blob_name)` in `chunks` DDL | Converts improbable UUID collision from silent overwrite risk into deterministic insert failure |
| Upload transaction scope | Encrypt chunks before opening SQLCipher write transaction | Avoids long write-lock hold while doing CPU/I/O-heavy encryption on large files |
| Chunk size source of truth | `encrypt_file` reads `chunk_size_bytes` from `manifest_meta` via MetadataStore | Prevents caller-provided chunk-size mismatch from silently corrupting reassembly |
| Cloud delete durability | `pending_deletions` queue table drained by Phase 4 sync | Survives crash between manifest delete commit and cloud blob deletion; guarantees retry |
| `MetadataStore` async dispatch | `#[async_trait]` macro | `async fn` in traits is not dyn-safe; `#[async_trait]` makes `Box<dyn MetadataStore>` compile while keeping the trait readable. Requires `async-trait = "0.1"` in `Cargo.toml` |
| MP4/QuickTime EXIF stripping | Drop vs full-file read vs two-pass seek | Dropped: moov atom at end-of-file on device recordings breaks streaming; video stripping deferred |
| EXIF implementation approach | Hand-written byte-level parser (`storage::pipeline::exif`) | No external EXIF crates; full-file RAM load for JPEG/PNG; avoids dependency on kamadak-exif/img-parts |
| Schema CHECK constraints | DDL enforcement vs prose-only | CHECK constraints added: catch corrupt node rows at write time; cross-column constraint enforces file_key_wrapped nullability invariant |
| `ChunkRecord.blob_path` | Remove vs split into two structs | Removed: only used between encrypt_file and insert_chunks; staging path derived from blob_name at call site |
| `received_shares.chunk_uuids` format | JSON array + CHECK vs normalised table vs comment-only | JSON array with `json_valid()` CHECK: consistent with share package format, enforced at write time |
| Rename/move in MetadataStore | Two focused methods vs `update_node(NodePatch)` vs defer | Two focused methods: `rename_node` and `move_node`; consistent naming with existing trait methods |

---

## Category C: Architectural Decisions (Finalized)

These decisions are intentional MVP scope limitations that will persist through Phase 6. Phase 7+ planning may reconsider them with explicit research.

| Decision | Status | Rationale | Notes |
|----------|--------|-----------|-------|
| **c-uuid-nodeid-migration** — NodeId at domain, Uuid at trait | ✅ Finalized | Type safety is provided at domain layer via `NodeId` wrapper; trait boundary uses `Uuid` for persistence contract abstraction. This is intentional architectural layering, not a gap to be filled. Avoids broad API churn across Phase 3–5 contracts. | Documented in [Deferred Items Inventory](../../deferred-items-inventory.md) Category C |
| **c-directory-deletion** — Files-only in Phase 6 | ✅ Finalized | Directory deletion requires recursive enumeration and cascade blob cleanup. MVP focuses on per-file operations. `delete_directory` is a Phase 7+ feature with separate IPC command + MetadataStore extension. | Documented in [Deferred Items Inventory](../../deferred-items-inventory.md) Category C; see `File Deletion (MVP)` section above |
| **c-inapp-file-viewer** — Backend ready, UI deferred | ✅ Finalized | `get_file_content` command is implemented with 50 MiB cap. Infrastructure is production-ready; in-app viewer UI is Phase 6.8+ feature. Future phases can add viewers (text, image, PDF) without backend changes. | Command registered in canonical surface; UI consumer deferred per [Deferred Items Inventory](../../deferred-items-inventory.md) Category D |

---

## Related Documents

- [Chunk Pipeline Diagram](diagrams/chunk-pipeline.md)
- [Manifest Schema Diagram](diagrams/manifest-schema.md)
- [Cryptographic Primitives](../cryptographic-primitives/design.md) — `encrypt_chunk`, `file_key`, BLAKE3
- [Cloud Synchronisation](../cloud-synchronisation/design.md) — blob upload/download
- Roadmap Phase 3 deliverables
