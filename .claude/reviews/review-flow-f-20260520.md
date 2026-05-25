---
title: "Flow F — Zero-Knowledge Boundary Review"
date: "2026-05-20"
flow: F
status: complete
---

# Flow F — Zero-Knowledge Boundary Review

**Core question**: Does the cloud ever receive anything other than opaque encrypted blobs?

---

## Upload path trace: staged file → rclone

```
push_vault (sync.rs:483)
  ├─ list_sync_chunks() → chunk.blob_name (UUID v4, validated)
  ├─ pending_blob_path(staging_dir, &blob_name) → local path
  ├─ drive_blob_uploads(upload_blobs, ...) → upload_blob_task per blob
  │     └─ build_blob_remote_path(blob_name) → "vault/{uuid}.blob"
  │           └─ validate_remote_path(path) [allowlist: a-zA-Z0-9._/-; no .., no /, no control chars]
  │     └─ cloud_transport.upload_blob(&local_path, &remote_path)  [rclone]
  ├─ upload_manifest_backup(vault_db_path, sqlcipher_key, manifest_key, vault_id, ...)
  │     └─ VACUUM INTO export (SQLCipher-encrypted, same key)
  │     └─ read into Zeroizing<Vec<u8>>, delete export file
  │     └─ encrypt_manifest_backup(plaintext, manifest_key, vault_id)
  │           → XChaCha20-Poly1305, AAD = vault_id.as_bytes() (16 bytes)
  │     └─ cloud_transport.upload_blob(..., "manifest/manifest-backup.blob")
  ├─ upload_vault_header(vault_header, ...)
  │     └─ strips key_file_blake3 and name from clone
  │     └─ serialise_pretty_json → owner-only staging file
  │     └─ cloud_transport.upload_blob(..., VAULT_HEADER_BLOB_NAME)
  └─ drain_pending_deletions()
        └─ pending blob names are UUIDs (same pool as upload blobs)
```

Blob names are generated in `encrypt_file_inner` as `Uuid::new_v4()` — random, not derived from filenames or content.

---

## Findings

### [FLOW-F-001] `strip_exif` is dead code — EXIF never stripped before encryption
**Severity**: medium
**Invariant**: ZK metadata (Flow F check: "strip_exif called unconditionally for JPEG/PNG before AEAD")
**Location**: `src-tauri/src/storage/pipeline/exif.rs:16`
**Observation**: `strip_exif` and `strip_jpeg_metadata` / `strip_png_metadata` are fully implemented in `exif.rs` and have zero references across the entire codebase (`find_references` on `strip_exif` returns 0 hits). `encrypt_file_inner` reads the source file in raw chunks and calls `encrypt_chunk` directly — no EXIF-stripping step exists anywhere in the production pipeline.
**Violation**: JPEG and PNG files are encrypted with full EXIF metadata (GPS coordinates, device serial, timestamps) intact inside the ciphertext blob. The ZK cloud boundary holds (cloud sees only ciphertext), but the design intent stated in the review plan is not met. More practically: when a file is shared via HPKE share packages (Flow H), the recipient decrypts the blob and receives full EXIF, including geolocation data the user may not have intended to share. Additionally, `exif.rs` dead code silently fails any future "is EXIF stripping in place?" audit.
**Recommendation**: Call `strip_exif` in `encrypt_file_inner` (and `encrypt_bytes`) before the chunk loop. Read the full source into memory first (or apply to the first chunk buffer if memory pressure is a concern), run `strip_exif`, then proceed with chunked AEAD. For `encrypt_file`, this requires a full read-before-chunk path for JPEG/PNG, which is a moderate change; document TIFF/HEIC/MP4 exclusion as an accepted limitation in the threat model.
**Test coverage**: none (existing `exif.rs` tests exercise the stripping functions in isolation; no integration test confirms `encrypt_file` strips EXIF before AEAD).

---

## Confirmed passes

### ZK core — every byte sent to cloud is ciphertext
**Invariant**: ZK core
**Result**: PASS

`encrypt_file_inner` (`encrypt_file.rs:48`) produces encrypted blobs in staging via `encrypt_chunk` (AEAD). `upload_blob_task` reads only the staged `.blob` files and passes them to `cloud_transport.upload_blob`. There is no code path in `push_vault` or `upload_blob_task` that reads plaintext source files or passes them to rclone. The manifest export is SQLCipher-encrypted on disk (VACUUM INTO reuses the vault sqlcipher key) before being read into `Zeroizing<Vec<u8>>` and then XChaCha20-Poly1305 encrypted; the SQLCipher export file is deleted before the encrypted wire is uploaded. All three cloud-facing upload calls — blob, manifest backup, vault header — are verified.

### Blob names are opaque random UUIDs
**Invariant**: ZK metadata
**Result**: PASS

`encrypt_file_inner:93` — `let blob_name = Uuid::new_v4().hyphenated().to_string();` — each blob gets a fresh random UUID, not derived from the source filename, path, or content. Before upload, `push_vault:542` validates every name through `validate_blob_name_uuid_v4` (must parse as canonical UUID v4). `build_blob_remote_path` constructs `vault/{uuid}.blob` and re-validates through `validate_remote_path` (allowlist: `[a-zA-Z0-9._/-]`; rejects `..`, leading `/`, control characters). No filename-to-blobname mapping is sent to cloud.

### Blob names not derived from plaintext content (no convergent encryption)
**Invariant**: ZK metadata
**Result**: PASS

`Uuid::new_v4()` is CSPRNG-seeded random. BLAKE3 checksums are computed over ciphertext and stored locally for integrity, not used to name or deduplicate blobs. Two identical files produce distinct blob names on every upload.

### Manifest (file tree, names, sizes, timestamps) is encrypted before upload
**Invariant**: ZK metadata
**Result**: PASS

`upload_manifest_backup` (`manifest_backup.rs:115`): exports SQLCipher-encrypted DB via VACUUM INTO, reads into `Zeroizing<Vec<u8>>`, deletes local export, calls `encrypt_manifest_backup` (XChaCha20-Poly1305 with AAD = `vault_id.as_bytes()`), writes encrypted wire to staging, uploads to fixed path `manifest/manifest-backup.blob`. The SQLite manifest schema (filenames, directory tree, timestamps) is never sent in plaintext.

### Vault header content — no filenames, directory structure, or user identity
**Invariant**: Invariant 9 cross-check
**Result**: PASS

`VaultHeader` fields (`vault_header.rs:37`): `vault_id`, `schema_version`, `tier`, `argon2_salt`, `argon2_params`, `recovery_slots`, and optionally `key_file_blake3`/`name`. `upload_vault_header` zeroes out `key_file_blake3` (key-file fingerprint — ZK correlation leak) and `name` (human-readable vault name — ZK metadata leak) from the clone before serialising. The cloud copy contains only Argon2 params, the vault UUID, the wrapped key slot(s), and the recovery slots. No filenames, directory paths, or user identity fields are present. The `tier` field (1 = password only, 2 = password + key file) is intentionally public — accepted by design.

### `pending_deletions` use opaque blob names
**Invariant**: ZK metadata
**Result**: PASS

`drain_pending_deletions` operates on blob names fetched from `pending_deletions` table, which stores the same UUID v4 blob names used at encryption time. The cloud-facing delete call receives `vault/{uuid}.blob` paths — no original filenames.

### Staging directory contains only ciphertext
**Invariant**: Invariant 7 + ZK
**Result**: PASS

`encrypt_file_inner` writes only encrypted blobs (`write_blob_file` appends `.blob` extension). The manifest export is SQLCipher-encrypted (not plaintext). The vault header staging file is plaintext JSON, but it contains no filenames/keys (see vault header check above); it is written owner-only and deleted after upload.

### Chunk blobs are fixed-size (file size not precisely leaked)
**Invariant**: ZK / threat model
**Result**: PASS (with accepted limitation)

`encrypt_file_inner:72` allocates `vec![0u8; chunk_size_usize]` for every chunk including the last, so the AEAD input is always exactly `chunk_size_bytes`. All blobs uploaded to cloud are the same size (`chunk_size_bytes + AEAD overhead`). Exact file size is not recoverable from individual blob sizes. Approximate file size is recoverable from blob count × chunk_size (ceiling), which is listed as an accepted limitation in the plan.

### Remote path validation — path traversal and injection blocked
**Invariant**: Invariant 5 cross-check
**Result**: PASS

`validate_remote_path` (`remote_path.rs:26`) rejects: empty string, leading `/`, any `..` substring, control characters, any character outside `[a-zA-Z0-9._/-]`. Applied at two layers: once in `build_blob_remote_path` for chunk blobs, and once in `rclone.rs` at the transport layer for every `upload_blob` / `download_blob` / `delete_blob` / `list_blobs` call. Shared blob paths (containing `/`) bypass `validate_blob_name_uuid_v4` but still pass through `validate_remote_path`.

### Upload order randomised (access pattern hardening)
**Invariant**: ZK / threat model (design sanity)
**Result**: PASS

`push_vault:554` — `fisher_yates_shuffle_with_system_rng(&mut upload_blobs)` using `rand::rngs::SysRng` before the upload loop. Cloud provider cannot infer directory traversal order or file relationship from upload sequence.

---

## Accepted limitations confirmed

| Limitation | Status |
|---|---|
| Blob count on cloud reveals approximate file count | Accepted — not mitigated by design |
| Blob sizes fixed at chunk_size; count × chunk_size leaks approximate file size (±chunk_size) | Mitigated — fixed-size padding; residual within one chunk |
| Cloud provider sees upload/download timestamps | Accepted |
| `manifest/manifest-backup.blob` at a fixed path allows adversary to detect Arx Runa vault presence | Accepted by design |
| Vault `tier` field (1/2) in plaintext vault header reveals whether key file is in use | Accepted — needed for client to choose derivation path |
| TIFF, HEIC, MP4/QuickTime not stripped by `strip_exif` | Listed as accepted limitation in plan; however `strip_exif` is currently dead code so even JPEG/PNG are not stripped — see [FLOW-F-001] |

---

## Summary

| Severity | Count |
|---|---|
| Critical | 0 |
| High | 0 |
| Medium | 1 |
| Low | 0 |

**Invariants fully confirmed with no findings**: ZK core (every cloud byte is ciphertext), ZK metadata (opaque blob names, no convergent encryption, manifest encrypted, vault header stripped, pending_deletions opaque), Invariant 5 path validation cross-check, Invariant 9 vault header content cross-check.

**One finding**: [FLOW-F-001] `strip_exif` is dead code — JPEG/PNG files are encrypted with EXIF intact. The ZK cloud boundary holds, but the design intent is not met and EXIF accompanies shared files to recipients.

**Follow-up recommended**: Yes — fix [FLOW-F-001] by wiring `strip_exif` into `encrypt_file_inner` / `encrypt_bytes` before the chunk loop, and add an integration test that round-trips a JPEG through `encrypt_file` and verifies no EXIF survives in the decrypted output.
