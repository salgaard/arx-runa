---
applyTo: "src-tauri/src/storage/**"
---

# Storage module — scoped instructions

These rules apply to all files under `src-tauri/src/storage/`.

## Manifest database (SQLCipher)
- The manifest is a SQLCipher database keyed with `sqlcipher_key` (HKDF-derived)
  — never `master_key`, never unencrypted
- Schema:
  - `nodes`: node_id, parent_id, node_type, name, created_at, modified_at,
    size_bytes — names stored as plaintext inside SQLCipher (the DB is the
    encryption layer; double-encrypting names adds complexity with no benefit)
  - `chunks`: chunk_id, node_id, chunk_index, blob_name, size_padded,
    blake3_checksum
  - `manifest_meta`: key-value store — schema_version, vault_id,
    snapshot_counter, last_synced_at
- ON DELETE CASCADE: deleting a node must cascade to its chunk rows
- snapshot_counter: monotonic integer, increment on every cloud push

## Chunking
- Fixed-size uniform padding only — no Content-Defined Chunking (CDC)
  CDC leaks file size as a side channel; fixed chunks do not
- All chunks for a given file must be the same padded size
- chunk_index is 0-based, stored in both the manifest and as AAD on the
  encrypted blob
- Blob names are random UUID v4 — no relation to file identity or content

## BLAKE3 integrity check
- Compute BLAKE3 over the ENCRYPTED blob (nonce + ciphertext + tag) after
  encryption; store in manifest
- Before decrypting any chunk: fetch BLAKE3 from manifest, recompute over
  the downloaded blob, compare. Fail if mismatch — do not attempt decryption
- BLAKE3 is an operational integrity check, not a security feature
  (AEAD tag handles authenticity)

## Cloud manifest backup
- Encrypted with `manifest_key` (HKDF-derived) — not `chunk_key`
- Vault header (unencrypted JSON: vault_id, schema_version, argon2_salt,
  argon2_params) MUST be uploaded before the manifest blob
  A new device needs the salt to derive keys before it can decrypt the manifest
- Snapshot model: atomic full export of SQLCipher DB after each batch
  No incremental diffs at this scope

## File deletion
- Immediate: delete chunk blobs from cloud, remove node + chunk rows from
  manifest in a single transaction
- ON DELETE CASCADE handles chunk row cleanup automatically
- Do not defer or batch deletes — blobs should be gone before the UI confirms

## Streaming I/O
- Never read a complete file into a `Vec<u8>` — stream via `BufReader`/
  `BufWriter` through the encrypt/decrypt pipeline
- Use `tokio::io` async I/O — never block the Tauri UI thread with sync I/O

## Trait boundary
- Define `MetadataStore` trait for manifest operations
- Define `CloudTransport` trait for Rclone operations
- Code depends on traits, not concrete types — enables mock-based testing
  without a live Rclone backend or real SQLCipher DB

## Required tests
- SQLCipher DB opened with wrong key returns error
- node insertion -> query -> deletion cycle is consistent and CASCADE works
- snapshot_counter increments on each export call
- BLAKE3 mismatch before decryption returns error, does not proceed
- Chunk blobs use UUID v4 names (no sequential, no filename-derived names)
- All chunks for a file have identical padded sizes
