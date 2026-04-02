---
applyTo: "src-tauri/src/storage/**"
---

# Storage module — rules

**Design specification**: `docs/architecture/designs/chunking-and-manifest/design.md`

## Manifest (SQLCipher)
- Keyed with `sqlcipher_key` — never `master_key`, never unencrypted
- Tables: `nodes`, `chunks`, `manifest_meta` — see design docs for schema
- `ON DELETE CASCADE` for node → chunks

## Chunking
- Fixed 4 MiB chunks with zero-padding (no CDC — leaks size info)
- `chunk_index` 0-based, stored in manifest and used as AAD
- Blob names: random UUID v4 — no relation to file identity
- See design doc for padding waste analysis table

## BLAKE3
- Checksum over encrypted blob (nonce + ciphertext + tag)
- Verify before decryption — fail on mismatch, don't attempt decrypt

## Cloud backup
- Manifest encrypted with `manifest_key`
- Vault header (unencrypted) uploaded BEFORE manifest — salt needed first
- Snapshot model: atomic full export, `snapshot_counter` increments each push

## Deletion
- Immediate: delete blobs + remove rows in single transaction

## I/O
- Stream via `BufReader`/`BufWriter` — never load full file
- Async only (`tokio::io`)

## Traits
- `MetadataStore` for manifest, `CloudTransport` for Rclone
