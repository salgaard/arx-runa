---
title: "Flow B — AEAD Encrypt/Decrypt & Chunk Pipeline"
date: "2026-05-20"
reviewer: "claude-sonnet-4-6"
invariants: [1, 2, 4]
status: complete
---

# Flow B Security Review — AEAD Encrypt/Decrypt & Chunk Pipeline

**Reviewed**: 2026-05-20  
**Invariants in scope**: 1 (AAD = `file_id || chunk_index`), 2 (CSPRNG nonce), 4 (chunk_size validation)  
**Starting symbols**: `encrypt_chunk`, `decrypt_chunk`, `generate_nonce`, `flush_epoch_buffer` / `flush_one_blob`

---

## Findings

No critical or high findings. One medium observation on code style and one low informational note recorded below.

---

### [FLOW-B-001] `std::mem::take` on inner Vec rather than on the Zeroizing wrapper
**Severity**: low  
**Invariant**: storage rule — plaintext buffers must be `Zeroizing<Vec<u8>>`  
**Location**: `src-tauri/src/storage/vault_ops/epoch_flush.rs:122`  
**Observation**: The current code reads:
```rust
let chunk = Zeroizing::new(std::mem::take(packed.as_mut()));
```
`packed.as_mut()` dereferences through `DerefMut` to yield `&mut Vec<u8>` (the inner buffer). `std::mem::take` then extracts the bare `Vec<u8>`, leaving `packed` with an empty `Vec<u8>`, and the raw `Vec<u8>` is immediately passed to `Zeroizing::new()` in the same expression. There is no named bare binding; the plaintext bytes never sit in an unprotected variable.  
**Violation**: None — the fix is in place. Plaintext stays `Zeroizing`-wrapped through `encrypt_chunk`. However, the idiom is subtly fragile: a reader could be tempted to split the expression into two statements, at which point the plaintext would briefly be an unwrapped `Vec<u8>`. The cleaner and safer idiom is `std::mem::take(&mut packed)`, which takes the `Zeroizing<Vec<u8>>` as a whole and replaces `packed` with `Zeroizing::new(Vec::new())` — no inner unwrap occurs.  
**Recommendation**: Replace with `let chunk = std::mem::take(&mut packed);` to eliminate the unwrap/rewrap pattern and make the safety property self-evident.  
**Test coverage**: none for this specific expression — correctness is covered indirectly by the epoch flush integration tests.

---

### [FLOW-B-002] Manifest backup AAD differs from chunk AAD format — note only
**Severity**: low (informational)  
**Invariant**: 1 (design sanity cross-check)  
**Location**: `src-tauri/src/storage/cloud/manifest_backup.rs:68`  
**Observation**: `encrypt_manifest_backup` uses `vault_id.as_bytes()` (16 bytes) as AAD rather than the `file_id || chunk_index` format mandated by Invariant 1 for chunk AEAD. AAD is non-empty and binds the ciphertext to vault identity. `decrypt_manifest_backup` reconstructs the same AAD identically.  
**Violation**: None. Invariant 1 is explicitly scoped to file chunk AEAD. The manifest is a single non-chunked blob; `chunk_index` has no meaning here. Using `vault_id` as AAD is semantically correct and consistent between encrypt and decrypt. Recorded only because the "every AEAD call" wording in the checklist could be misread to require the chunk format universally.  
**Recommendation**: No code change needed. Consider adding a one-line comment to `encrypt_manifest_backup` stating the AAD scope explicitly (e.g., `// AAD binds ciphertext to vault identity; not a chunk so chunk_index is inapplicable`).  
**Test coverage**: covered by `test_upload_manifest_backup_encrypt_upload_download_decrypt_round_trip`.

---

## Invariants Confirmed — No Findings

### Invariant 1 — AAD construction

**`build_chunk_aad` (`src-tauri/src/crypto/types/mod.rs:397`)**: returns exactly 20 bytes — `file_id` (16 bytes via `copy_from_slice`) followed by `chunk_index.to_be_bytes()` (4 bytes big-endian). Both `encrypt_chunk` and `decrypt_chunk` call this shared function, guaranteeing identical AAD on both sides.

**No AEAD call site omits AAD**:
- `encrypt_chunk` (`src-tauri/src/crypto/encrypt_chunk.rs:30`): passes `build_chunk_aad` (20 bytes).
- `encrypt_manifest_backup` (`manifest_backup.rs:68`): passes `vault_id.as_bytes()` (16 bytes). See FLOW-B-002.
- `flush_one_blob` (`epoch_flush.rs:94`): calls `encrypt_chunk` with `FileId::from_uuid(epoch_blob_id)` and `ChunkIndex::new(0)`. The epoch blob's own UUID is used as FileId — not any constituent file's node UUID. This is intentional and documented by an inline comment in the code.

### Invariant 2 — CSPRNG nonce

`generate_nonce` (`src-tauri/src/crypto/nonce.rs:6`) generates 24 bytes via `rand::rng().random::<[u8; 24]>()`. `rand::rng()` in rand 0.9+ returns a thread-local CSPRNG backed by the platform's secure random source. No counter, no KDF-derived nonce, no reuse mechanism. Tests verify 24-byte length, non-zero output, and uniqueness over 1 000 samples.

### Invariant 4 — chunk_size validation

`parse_chunk_size_bytes` (`src-tauri/src/storage/validation.rs:55`) enforces `131_072..=67_108_864` (128 KiB – 64 MiB inclusive). `read_chunk_size_bytes` wraps this parse-and-validate step and is called:

- `encrypt_file_inner` (`encrypt_file.rs:55`) — file-based path
- `encrypt_bytes` (`encrypt_file.rs:170`) — in-memory path (EXIF-stripped images)
- `upload_file` IPC handler (`upload_file.rs:78`) — additional call before routing decision
- `file_commands::flush_epoch_buffer` (`file_commands.rs:594`) — before passing to `storage::vault_ops::flush_epoch_buffer`

No hardcoded chunk size bypasses the stored value in any production path. Test helpers (`FixedMetaStore`, `4_194_304` literal in tests) are `#[cfg(test)]`-only.

### Design sanity — BLAKE3 over ciphertext

In all three encryption paths:
- `encrypt_file_inner`: `compute_checksum(&wire_blob)` where `wire_blob = encrypt_chunk(plaintext, ...)` ✓
- `encrypt_bytes`: same pattern ✓
- `flush_one_blob`: `compute_checksum(&encrypted)` where `encrypted = encrypt_chunk(chunk, ...)` ✓

BLAKE3 is always computed over the ciphertext wire blob, never over plaintext.

### Storage rule — epoch buffer plaintext stays `Zeroizing`-wrapped

**At collection stage**: `EpochBufferEntry.plaintext` is typed as `Zeroizing<Vec<u8>>` (confirmed in struct definition at `src-tauri/src/storage/types/epoch_buffer_entry.rs:6`). `entry.plaintext.clone()` therefore produces `Zeroizing<Vec<u8>>`; the accumulation buffer is `Vec<(Uuid, Zeroizing<Vec<u8>>)>`.

**At pack stage** (`flush_one_blob`): each `plaintext` slice is appended into `packed: Zeroizing<Vec<u8>>` via `extend_from_slice`. The packed buffer is `Zeroizing`-wrapped throughout.

**At encrypt stage**: `let chunk = Zeroizing::new(std::mem::take(packed.as_mut()))` — the plaintext is re-wrapped immediately (see FLOW-B-001 for the style note). No bare `Vec<u8>` binding holds plaintext at any point. `chunk` is consumed by `encrypt_chunk` which takes ownership of the `Zeroizing<Vec<u8>>`.

### EXIF stripping happens before AEAD

`strip_exif_if_image` (`upload_file.rs:21`) detects JPEG/PNG by magic bytes and strips EXIF/XMP/IPTC in RAM before any encryption call. Both the epoch-buffer path and the immediate-encrypt path branch on the `Option<Vec<u8>>` returned by this function before entering the pipeline. There is no flag or caller that can bypass stripping for supported formats.

---

## Summary

| Severity | Count |
|---|---|
| Critical | 0 |
| High | 0 |
| Medium | 0 |
| Low | 2 (both informational) |

**Invariants fully confirmed**: 1, 2, 4.  
**Design sanity checks passed**: BLAKE3 ordering, epoch blob AAD intent, `std::mem::take` fix in place.  
**Storage rule**: plaintext `Zeroizing` wrapper is maintained through the entire epoch flush pipeline.

**Follow-up fix session recommended**: No — neither finding requires a correctness fix. FLOW-B-001 (idiom improvement) can be addressed in a routine cleanup PR.
