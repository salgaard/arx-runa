---
paths:
  - "src-tauri/src/storage/**"
---

# Storage module — rules

**Design specification**: `docs/architecture/designs/chunking-and-manifest/design.md` — last verified against design dated 2026-04-08

## Manifest (SQLCipher)
- Keyed with `sqlcipher_key` — never `master_key`, never unencrypted
- Tables: `nodes`, `chunks`, `manifest_meta` — see design docs for schema
- Canonical tables: `nodes`, `chunks`, `manifest_meta`, `pending_deletions`. Forward-declared: `destination_sessions` (Phase 4), `contacts` / `shares` / `received_shares` (Phase 5). See design docs for DDL.
- `ON DELETE CASCADE` for node → chunks

## Chunking
- Chunk size is immutable per vault (set at creation): 128 KiB-64 MiB, default 4 MiB
- `epoch_buffer_enabled` is opt-in per vault (default `false`)
- Hybrid routing when enabled: files `< chunk_size_bytes` are staged and packed; files `>= chunk_size_bytes` use immediate standalone chunk upload (including trailing partial chunks)
- Zero-pad each chunk to `chunk_size_bytes` (no CDC — leaks size info)
- `chunk_index` 0-based, stored in manifest and used as AAD
- Blob names: random UUID v4 — no relation to file identity
- See design doc for padding waste analysis table

## BLAKE3
- Checksum over encrypted blob (nonce + ciphertext + tag)
- `verify_checksum` returns `VerifiedBlob`; pass to `decrypt_chunk` — the type system enforces check-before-decrypt order (skipping is a compile error)

## Cloud backup
- Manifest encrypted with `manifest_key`
- Manifest backup is a singleton blob (no AAD); vault header stays plaintext JSON at cloud root
- Push flow uploads manifest backup, then uploads vault header idempotently on every push
- Snapshot model: atomic full export, `snapshot_counter` increments each push

## EXIF stripping
- Optional pre-processing before the encrypt pipeline; enabled by default for `image/jpeg`, `image/png`, `image/tiff`, and can be disabled (detected by magic bytes, not extension)
- Strips EXIF, XMP, IPTC segments in RAM — original file on disk is never modified
- MP4/QuickTime excluded: `moov` atom at end-of-file breaks the streaming invariant
- Non-media types and unsupported containers pass through unmodified

## Deletion
- Transaction order: read blob names, enqueue into `pending_deletions` inside the same transaction, delete node row (CASCADE removes chunk rows), commit, then delete local staging blobs
- If blob deletion is interrupted, orphan encrypted blobs are cleaned on startup

## I/O
- Stream via `BufReader`/`BufWriter` — never load full file
- Async only (`tokio::io`)

## Traits
- `MetadataStore` for manifest, `CloudTransport` for Rclone
- `MetadataStore` Phase 3.1 surface: `insert_node`, `insert_chunks`, `get_node`, `list_children`, `get_chunks`, `rename_node`, `move_node`, `delete_node`, `list_pending_deletions`, `mark_deletion_complete`, `get_meta`, `set_meta`, `increment_snapshot_counter`
