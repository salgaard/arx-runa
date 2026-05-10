---
applyTo: "src-tauri/src/storage/**"
---

# Storage

> Design: `docs/architecture/designs/chunking-and-manifest/design.md`

## Manifest (SQLCipher)
- Keyed with `sqlcipher_key` — never `master_key`, never unencrypted
- Canonical tables: `nodes`, `chunks`, `manifest_meta`, `pending_deletions`; forward-declared: `destination_sessions` (Phase 4), `contacts`/`shares`/`received_shares` (Phase 5); `ON DELETE CASCADE` for node → chunks

## Chunking
- Chunk size immutable per vault (set at creation): 128 KiB–64 MiB, default 4 MiB
- `epoch_buffer_enabled`: opt-in per vault (default `false`); when enabled: files `< chunk_size_bytes` staged and packed; files `>= chunk_size_bytes` immediate standalone upload (including trailing partial chunks)
- Zero-pad each chunk to `chunk_size_bytes` (no CDC — leaks size info); `chunk_index` 0-based (used as AAD); blob names: random UUID v4

## Pipeline
- `storage::pipeline::{encrypt_file,decrypt_file}` owns streaming chunk transforms; `storage::vault_ops::{upload_file,download_file}` owns orchestration
- Plaintext buffers must be `Zeroizing<Vec<u8>>`; decrypt flow: `verify_checksum` before `decrypt_chunk` (`VerifiedBlob` enforces this — skipping is compile error)
- Hybrid routing decision: `storage::vault_ops::routing::decide`
- `upload_file`/`download_file`: `progress: Option<&(dyn Fn(u64, u64) + Send + Sync)>` (bytes_processed, bytes_total); `push_vault`/`pull_vault`: `progress: Option<&(dyn Fn(u32, u32, Option<&str>) + Send + Sync)>` (files_processed, files_total, current_file_name); storage must never depend on `tauri::` — `Channel<T>` is wrapped into `dyn Fn` at IPC layer

## Cloud backup
- Manifest encrypted with `manifest_key`; singleton blob (no AAD); vault header stays plaintext JSON at cloud root
- Backup blob path: `manifest/manifest-backup.blob` (constant `storage::cloud::manifest_backup::MANIFEST_BACKUP_BLOB_NAME`)
- Push: upload manifest backup, then vault header idempotently; `snapshot_counter` increments each push
- `rollback_snapshot_counter`: SQLCipher-specific on `SqlCipherMetadataStore` (not `MetadataStore`); push-only after manifest-upload failure; enforce current counter = previous+1 before rollback
- `list_sync_chunks`: SQLCipher-specific on `SqlCipherMetadataStore`; returns alphabetical `(blob_name, blake3_checksum)` pairs
- Fisher-Yates upload-order randomisation: `rand::rngs::SysRng` in production; deterministic seeding only under `#[cfg(test)]`
- `RcloneTransport`: bundled sidecar via `tokio::process::Command`; remote paths pass `^[a-zA-Z0-9._/-]+$` (reject `..` and leading `/`); stderr strips lines matching `token|key|secret|password|credential|auth`

## EXIF stripping
- Optional pre-encrypt; enabled by default for `image/jpeg`, `image/png`, `image/tiff` (magic bytes, not extension); strips EXIF/XMP/IPTC in RAM — disk file never modified; MP4/QuickTime excluded; unsupported containers pass through

## Deletion & Staging
- Transaction order: read blob names → enqueue `pending_deletions` → delete node (CASCADE removes chunks) → commit → delete local staging blobs
- Orphan blobs cleaned on startup; `storage::vault_ops::delete_file` is the orchestration entry point
- Staging path: `dirs::data_dir().join("arx-runa").join("staging")`; orphan cleanup runs at vault open only
- `cleanup_orphaned_blobs`: delete only files where extension=`.blob`, stem=UUID v4, stem absent from `chunks.blob_name`; `ErrorKind::NotFound` on delete = success
- `list_all_blob_names`: SQLCipher-specific, must not be added to `MetadataStore`

## I/O, Errors, Traits
- Stream via `BufReader`/`BufWriter`; async only (`tokio::io`)
- `StorageError::from_crypto`: checksum mismatches → `ChecksumMismatch`; all other crypto failures → `Database`
- `MetadataStore` for manifest; `CloudTransport` for Rclone (`&str` cloud-root-relative, forward slashes; `BlobName` for chunk filenames)
- Not on `MetadataStore`: sharing/session/cloud-config CRUD — use `SharingStore` or SQLCipher-specific accessors
- SQLCipher-only: `rollback_snapshot_counter`, `list_sync_chunks`, `list_all_blob_names`, `replace_file_key_and_chunks` (single transaction; enqueues old blob_names into `pending_deletions`)
