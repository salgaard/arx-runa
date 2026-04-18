---
title: "Phase 3.2 — Encrypt and Decrypt Pipelines"
created: "2026-04-18T00:00:00Z"
status: approved
roadmap-phase: 3
sub-phase: "3.2"
design-document: "docs/architecture/designs/chunking-and-manifest/design.md"
sub-phase-roadmap: "docs/architecture/designs/chunking-and-manifest/sub-phases/roadmap.md"
governance-sync-required: true
tags: [storage, phase-3, pipeline, encrypt, decrypt, chunking, streaming, zeroize, blake3]
---

# Plan: Phase 3.2 — Encrypt and Decrypt Pipelines

## 1. Goal

Land streaming `encrypt_file` and `decrypt_file` under `src-tauri/src/storage/`, wire the per-file key lifecycle into an `upload_file` / `download_file` orchestration layer, and enforce the hybrid-routing gate against the `epoch_buffer_enabled` vault flag — completing the local encrypt/decrypt cycle that Phase 3.3 (staging + recovery) and Phase 4 (cloud sync) consume.

## 2. Context

**Roadmap**: Phase 3 — Storage: Chunking and Manifest (`docs/roadmap.md:64-69`). Phase 3.2 is the middle sub-phase; depends strictly on 3.1, and 3.3 depends on 3.2.

**Sub-phase roadmap**: `docs/architecture/designs/chunking-and-manifest/sub-phases/roadmap.md` (implementation order `3.1 → 3.2 → 3.3`). Security review **required** (roadmap line 97: streaming invariant, zeroization, BLAKE3 pre-decrypt). Estimated scope: ~200 lines production + ~200 lines tests.

**Sub-phase document**: `docs/architecture/designs/chunking-and-manifest/sub-phases/3.2-encrypt-decrypt-pipelines.md` (8 deliverables + validation checkpoint).

**Parent design sections used** (canonical per Contract Anchor `design.md#contract-surface`):

- `design.md:17-48` — Contract Surface (interface / data / invariant / dependency).
- `design.md:51-95` — Chunk Size (hybrid auto-routing when `epoch_buffer_enabled = true`).
- `design.md:100-118` — Padding Scheme (zero-pad encrypt, truncate-via-`size_bytes` decrypt, 0-byte file semantics).
- `design.md:301-371` — Encrypt Pipeline (canonical public API, encrypt flow, decrypt flow, streaming invariant).
- `design.md:375-411` — File Key Lifecycle (new upload flow ordering: encrypt-before-tx; file access; file deletion).
- `docs/architecture/design-invariants.md` §1 (chunk AAD), §2 (nonce policy), §4 (`chunk_size_bytes`), §7 (zero-trace persistence), §8 (hybrid epoch routing).

**Existing state** (branch `development`, HEAD `79c6527` — "phase 3.1 implemented"):

- `src-tauri/src/storage/mod.rs` re-exports `StorageError`, `MetadataStore`, `SqlCipherMetadataStore`, `BlobName`, `ChunkRecord`, `Node`, `NodeId`, `NodeType`. No `pipeline` or `vault_ops` modules yet.
- `src-tauri/src/storage/error.rs` — `StorageError` variants: `Database`, `NotFound`, `ChecksumMismatch`, `Io`, `WrongKey`, `ConstraintViolation` (`#[non_exhaustive]`). No `From<CryptoError>`.
- `src-tauri/src/storage/metadata_store.rs` — trait with `get_meta`, `get_chunks`, `insert_node`, `insert_chunks`, `set_meta` (rejects immutable keys), etc.
- `src-tauri/src/storage/validation.rs` — `parse_chunk_size_bytes`, `validate_blob_name_uuid_v4`, `validate_size_padded_matches_chunk_size`, `validate_chunk_target_node`.
- `src-tauri/src/storage/types/chunk_record.rs` — `ChunkRecord { chunk_id: Uuid, node_id: NodeId, chunk_index: u32, blob_name: String, size_padded: u64, blake3_checksum: [u8; 32] }`.
- `src-tauri/src/storage/types/node.rs` — `Node { ..., file_key_wrapped: Option<[u8; 72]> }` (wrapped wire format bytes, matches `WrappedFileKey`).
- `src-tauri/src/crypto/encrypt_chunk.rs` — `encrypt_chunk(plaintext: Vec<u8>, &FileKey, &FileId, ChunkIndex) -> Result<Vec<u8>, CryptoError>` (owns and overwrites plaintext in place; returns wire blob).
- `src-tauri/src/crypto/decrypt_chunk.rs` — `decrypt_chunk(VerifiedBlob, &FileKey, &FileId, ChunkIndex) -> Result<Vec<u8>, CryptoError>` (accepts only `VerifiedBlob`).
- `src-tauri/src/crypto/checksum.rs` — `verify_checksum(Vec<u8>, &Blake3Hash) -> Result<VerifiedBlob, CryptoError>` (`VerifiedBlob::into_inner` is `pub(crate)` — only callable from within the `crypto` module).
- `src-tauri/src/crypto/generate_file_key.rs` — `generate_file_key() -> FileKey` (CSPRNG fills `SecretBox` buffer directly, no stack locals).
- `src-tauri/src/crypto/wrap_key.rs` — `wrap_file_key(&FileKey, &KeyEncryptionKey) -> Result<WrappedFileKey, CryptoError>`, `unwrap_file_key(&WrappedFileKey, &KeyEncryptionKey) -> Result<FileKey, CryptoError>`. `WrappedFileKey(pub [u8; 72])`.
- `Cargo.toml` already pins `tokio`, `async-trait`, `uuid`, `thiserror`, `zeroize`, `proptest`. `tempfile` is present for crypto tests. **No new dependencies required.**
- `.claude/rules/storage.md` mentions streaming invariant and `verify_checksum → VerifiedBlob → decrypt_chunk` flow, but has no anchor for `storage::pipeline` location, plaintext zeroization at pipeline scope, or hybrid-routing gate residency.

**Pending architectural decisions** relevant to Phase 3.2:

- `design.md` "Open Decisions" (lines 545–550): upload-order randomisation is Phase 4; video EXIF stripping is deferred; maximum file size is non-blocking. No open decisions gate Phase 3.2.

## 3. Design Concerns / Open Questions

### DC-1 — `storage/pipeline.rs` single file violates one-concern-per-file

| Field | Content |
|---|---|
| Concern | Sub-phase deliverable 1/2 places both `encrypt_file` and `decrypt_file` in `src-tauri/src/storage/pipeline.rs`. `.claude/rules/rust.md` Structure says "One concern per file (e.g., `encrypt_chunk.rs`, `key_source.rs`)". Existing convention in `src-tauri/src/crypto/` has separate `encrypt_chunk.rs` / `decrypt_chunk.rs`. |
| Source | `3.2-encrypt-decrypt-pipelines.md:12-13` vs `.claude/rules/rust.md:3-5`. |
| Impact | A single-file `pipeline.rs` drifts from the established `one-concern-per-file` layout that Phase 3.1 already resolved for `metadata_store.rs` / `sqlcipher.rs` / `mock.rs`. |
| Classification | Non-blocking. |
| Resolution | Create `src-tauri/src/storage/pipeline/mod.rs` (re-exports only), `pipeline/encrypt_file.rs`, `pipeline/decrypt_file.rs`, `pipeline/aad_context.rs` (shared AAD / path helpers if needed). The sub-phase contract (`storage::encrypt_file`, `storage::decrypt_file` reachable via `crate::storage::*`) is preserved. |

### DC-2 — Sub-phase upload-order contradicts parent design's "encrypt-before-transaction" decision

| Field | Content |
|---|---|
| Concern | Deliverable 3 text orders steps as "begin transaction → insert node row → encrypt all chunks → insert chunk rows → commit". Parent design "New file upload" (`design.md:377-388`) and Decisions Made row (`design.md:565` — "Upload transaction scope: Encrypt chunks before opening SQLCipher write transaction") order them as "encrypt all chunks → begin transaction → insert node row → insert chunk rows → commit". The parent design is canonical. |
| Source | `3.2-encrypt-decrypt-pipelines.md:14` vs `design.md:377-388, 565`. |
| Impact | If the pipeline begins the SQLCipher write-transaction before encrypting, the write-lock is held while encrypting potentially gigabytes of chunks — starves concurrent readers and blocks other manifest mutations. |
| Classification | Non-blocking. |
| Resolution | Follow parent-design ordering: generate + wrap `file_key` → encrypt all chunks (writing staging blobs outside any tx) → begin SQLCipher tx → `insert_node` → `insert_chunks` → commit → zeroize `file_key`. |

### DC-3 — Hybrid routing gate has no epoch-buffering backend in Phase 3.2

| Field | Content |
|---|---|
| Concern | Deliverable 6 requires "when `epoch_buffer_enabled` is enabled, files smaller than `chunk_size_bytes` route to epoch buffering". Epoch buffering packing infrastructure is a Phase 4 responsibility (`design.md:73` — "Approach 7 from `docs/research/padding-overhead-reduction.md`" — no Phase 3 module exists). The acceptance criterion ("Hybrid routing is enforced when epoch buffering is enabled") forces a choice. |
| Source | `3.2-encrypt-decrypt-pipelines.md:17, 41` vs absent `src-tauri/src/storage/epoch_buffer/` module. |
| Impact | Without a definition, an implementer must either (a) silently fall back to standalone encrypt for small files (breaks invariant §8), (b) panic, or (c) return an error — none of which is stated. |
| Classification | Non-blocking. |
| Resolution | Introduce `enum RouteDecision { Immediate, EpochBuffer }` in `src-tauri/src/storage/vault_ops/routing.rs`. `upload_file` reads `chunk_size_bytes` and `epoch_buffer_enabled` via `MetadataStore::get_meta`, computes the decision, dispatches: `Immediate` → `pipeline::encrypt_file`; `EpochBuffer` → returns `StorageError::ConstraintViolation("epoch buffering not yet available; deferred to Phase 4")` (no `Unsupported` variant added to keep the error surface stable). Tests cover both branches (small+enabled → error, large+enabled → immediate path, any-size+disabled → immediate path). Phase 4 replaces the error return with the actual packing call. |

### DC-4 — `CryptoError` → `StorageError` mapping is undefined

| Field | Content |
|---|---|
| Concern | Pipeline must call `encrypt_chunk` (returns `CryptoError::EncryptionFailed`), `verify_checksum` (returns `CryptoError::ChecksumMismatch`), and `decrypt_chunk` (returns `CryptoError::{DecryptionFailed, InvalidBlobFormat, ChecksumMismatch}`). `storage::StorageError` has no `From<CryptoError>`. |
| Source | `src-tauri/src/crypto/error.rs:11-39` vs `src-tauri/src/storage/error.rs:9-28`. |
| Impact | Each pipeline call site would otherwise invent ad-hoc mapping. |
| Classification | Non-blocking. |
| Resolution | Add `pub(crate) fn from_crypto(error: CryptoError) -> StorageError` to `storage/error.rs`. Mapping: `ChecksumMismatch → StorageError::ChecksumMismatch`; every other variant → `StorageError::Database(error.to_string())` (AEAD failures are non-recoverable and the user-visible action is identical). No new `StorageError` variants. Add a `From<CryptoError>` impl that delegates to the helper for ergonomic `?` propagation. |

### DC-5 — Chunk-slice soundness checks are under-specified for `decrypt_file`

| Field | Content |
|---|---|
| Concern | Implementation Notes say "`decrypt_file` must sort `chunks` by `chunk_index` before processing — do not assume the caller provides them in order". No requirement to detect missing, duplicated, or out-of-range indices. A caller (or corrupt DB) could supply `[0, 2]` with `file_size = 3 * chunk_size_bytes`; silent last-chunk truncation would then produce a quietly-wrong plaintext. |
| Source | `3.2-encrypt-decrypt-pipelines.md:60` vs no explicit invariant in parent design. |
| Impact | Silent data corruption on malformed inputs. |
| Classification | Non-blocking. |
| Resolution | Before decrypt loop: compute `expected_count = ceil(file_size / chunk_size_bytes)` (0 for `file_size == 0`); verify `chunks.len() == expected_count`; sort by `chunk_index`; verify indices equal `0..expected_count`. Violations → `StorageError::ConstraintViolation("chunk list is malformed: missing or duplicate chunk_index")`. |

### DC-6 — `chunk_size_bytes` source-of-truth differs between encrypt and decrypt paths

| Field | Content |
|---|---|
| Concern | Encrypt path reads `chunk_size_bytes` from `manifest_meta` (design decision `design.md:567`). Decrypt path has two candidates: (a) read from `manifest_meta` again, or (b) derive from `ChunkRecord.size_padded` (which the Phase 3.1 validator already constrains to equal `chunk_size_bytes`). The canonical design decrypt flow (`design.md:354-367`) uses `chunk_size_bytes` explicitly in the truncation formula without saying where it comes from. |
| Source | `design.md:362-365` vs `3.2-encrypt-decrypt-pipelines.md` (silent). |
| Impact | Mixed sources risk silent divergence if `manifest_meta` and a persisted `ChunkRecord` disagree. |
| Classification | Non-blocking. |
| Resolution | Both pipeline entry points read `chunk_size_bytes` once, at the top, via `MetadataStore::get_meta("chunk_size_bytes")` → `parse_chunk_size_bytes`. `ChunkRecord.size_padded` is used only as a local sanity assertion (must equal `chunk_size_bytes`); mismatch → `StorageError::ConstraintViolation("chunk size_padded mismatches vault chunk_size_bytes")`. |

### DC-7 — Plaintext zeroization on async cancellation is implicit

| Field | Content |
|---|---|
| Concern | Implementation Notes say "use a guard pattern or explicit cleanup in error branches". Async cancellation (Tauri command aborts, future dropped) is neither a success nor an error path — if plaintext lives in a bare `Vec<u8>`, cancellation leaves it un-zeroized on the heap until re-allocated. |
| Source | `3.2-encrypt-decrypt-pipelines.md:56` vs `design-invariants.md §7` (zero-trace persistence). |
| Impact | Plaintext leakage window across cancellation. |
| Classification | Non-blocking. |
| Resolution | Wrap every plaintext buffer in `zeroize::Zeroizing<Vec<u8>>`. `Drop` zeroizes on every exit path — success, `?` error, panic, async cancel. `encrypt_chunk` consumes an owned `Vec<u8>`; the pipeline passes `std::mem::take`d plaintext out of the `Zeroizing` guard only at the moment of the AEAD call, then discards the guard. Document the guard pattern in code-level comments (short, non-prose). |

### DC-8 — `vault_ops.rs` single file violates one-concern-per-file

| Field | Content |
|---|---|
| Concern | Deliverable 3 locates upload lifecycle integration in `src-tauri/src/storage/vault_ops.rs`. Deliverable 4 adds the symmetric decrypt/download flow. Same rule drift as DC-1. |
| Source | `3.2-encrypt-decrypt-pipelines.md:14-15` vs `.claude/rules/rust.md:3-5`. |
| Impact | Two mixed concerns in one file, grows further in later phases. |
| Classification | Non-blocking. |
| Resolution | Create `src-tauri/src/storage/vault_ops/mod.rs` (re-exports only), `vault_ops/upload_file.rs`, `vault_ops/download_file.rs`, `vault_ops/routing.rs` (hybrid-routing `RouteDecision`). |

### DC-9 — Partial-failure cleanup during encrypt is under-specified

| Field | Content |
|---|---|
| Concern | If chunk `k` fails after chunks `0..k` have been written to staging, Phase 3.2 must not leave plaintext in memory and must not leave the caller to reason about partial state. Design states "Orphaned blobs in staging are cleaned up on next startup" (Phase 3.3 territory). |
| Source | `design.md:384-411` vs `3.2-encrypt-decrypt-pipelines.md` (silent). |
| Impact | Best-effort behavior on error is implementer-dependent. |
| Classification | Non-blocking. |
| Resolution | On error mid-encrypt, `encrypt_file` drops accumulated `ChunkRecord`s and attempts best-effort `tokio::fs::remove_file` for each staging blob already written; failures in cleanup are logged (Phase 6 telemetry stub — for now, ignored) and do not shadow the original error. Manifest is untouched (no tx has opened). Phase 3.3 orphan cleanup remains the durable guarantee. |

### DC-10 — Governance rule file `storage.md` lacks a 3.2 pipeline anchor

| Field | Content |
|---|---|
| Concern | `.claude/rules/storage.md` covers chunking / BLAKE3 / deletion / I/O but has no pipeline-level rule: plaintext `Zeroizing<Vec<u8>>` guard, `pipeline::encrypt_file` + `pipeline::decrypt_file` residency, hybrid-routing gate placement in `vault_ops::routing`. Copilot instructions `.github/instructions/storage.instructions.md` mirror this gap. |
| Source | `.claude/rules/storage.md` vs deliverables 1–6. |
| Impact | Future reviewers lack an anchor for "where pipeline code lives" / "how plaintext is protected". |
| Classification | Non-blocking. |
| Resolution | Add a "Pipeline" section to `storage.md` (see §8 governance sync). Run `/copilot-sync` to mirror to `.github/instructions/`. |

## 4. Assumptions

1. **No `StorageError::Unsupported` variant added** — the "epoch buffering not available" branch re-uses `ConstraintViolation` with a distinctive message (DC-3). A future Phase 4 replacement removes the branch entirely.
2. **`ChunkRecord.size_padded` is trusted as a post-validation invariant** — Phase 3.1 validators already assert `size_padded == chunk_size_bytes` at insert time, but the pipeline re-asserts locally (DC-6).
3. **Pipeline functions live at `crate::storage::encrypt_file` / `crate::storage::decrypt_file` re-exports** — callers consume the canonical design API surface even though the internal file layout is `pipeline/encrypt_file.rs` etc. (DC-1).
4. **Upload / download orchestration is re-exported as `crate::storage::{upload_file, download_file}`** — public API for Phase 3.3 (deletion flow) and Phase 4 (cloud sync) to invoke (DC-8).
5. **`epoch_buffer_enabled` defaults to `false`** in every Phase 3 test vault (Phase 3.1 seeds this); tests that exercise the epoch branch explicitly override via SQLCipher direct write (bypassing `set_meta`, which rejects immutable keys — same technique Phase 3.1 tests use).
6. **Plaintext buffers are heap-allocated `Vec<u8>`** of size `chunk_size_bytes`; `tokio::io::BufReader::read_exact` or `read_to_end` drives I/O. No `mmap`. No stack-resident plaintext. Streaming invariant: at most one such buffer alive per call (excluding the transient move into `encrypt_chunk`).
7. **BLAKE3 checksum bytes are stored as `[u8; 32]` in `ChunkRecord`** and converted to `crypto::types::Blake3Hash` at the verify boundary (existing type already `pub` via `crypto::Blake3Hash`).
8. **`FileId` used by `encrypt_chunk` / `decrypt_chunk` is the `crypto::types::FileId`** constructed via `FileId::from_uuid(node.node_id.as_uuid())` at the pipeline boundary.
9. **`KeyEncryptionKey` is supplied by the caller** (session-scoped; already provided by Phase 2.2 via `SessionKeys`) — `upload_file` / `download_file` take `&KeyEncryptionKey` parameter. Phase 3.2 does not define how sessions source it; that is a Phase 2 / Phase 6 concern already resolved.
10. **Error messages never include plaintext content** — mapping from `CryptoError` to `StorageError::Database(string)` forwards only `CryptoError`'s own `Display`, which is static (AAD mismatch, tag length, etc.).
11. **Tests use `tempfile::TempDir`** for source / staging / destination paths — already used in Phase 3.1 tests and matches crypto test pattern.

## 5. Approach

### `CONTRACT_SNIPPETS`

**CS-001** — `encrypt_file` canonical public signature (`design.md:319-325`):
```rust
pub async fn encrypt_file(
    source: &Path,
    file_id: Uuid,
    file_key: &FileKey,
    metadata_store: &dyn MetadataStore,
    staging_directory: &Path,
) -> Result<Vec<ChunkRecord>, StorageError>;
```

**CS-002** — `decrypt_file` canonical public signature (`design.md:328-335`):
```rust
pub async fn decrypt_file(
    destination: &Path,
    file_id: Uuid,
    file_key: &FileKey,
    file_size: u64,
    chunks: &[ChunkRecord],
    blob_directory: &Path,
) -> Result<(), StorageError>;
```

**CS-003** — Encrypt per-chunk flow (`design.md:340-351`):
```
0. Read chunk_size_bytes once from manifest_meta via MetadataStore
1. BufReader reads up to chunk_size_bytes bytes from source file
2. If bytes_read < chunk_size_bytes: zero-pad buffer to chunk_size_bytes
3. AAD = file_id (16B) || chunk_index (u32 BE, 4B)  -- built inside encrypt_chunk
4. wire_blob = encrypt_chunk(padded_buffer, file_key, file_id, chunk_index)
   -> [24B nonce | ciphertext | 16B Poly1305 tag]
5. blake3_checksum = blake3::hash(wire_blob)  -- via compute_checksum
6. blob_name = Uuid::new_v4()
7. Write wire_blob to staging_directory/<blob_name>.blob via BufWriter
8. Zeroize padded_buffer  -- done by Zeroizing<Vec<u8>> Drop, plus encrypt_chunk overwrite
9. Return ChunkRecord
```

**CS-004** — Decrypt per-chunk flow (`design.md:356-367`):
```
1. Read wire_blob from blob_directory/<blob_name>.blob via BufReader
2. verify_checksum(wire_blob, &Blake3Hash(chunk.blake3_checksum)) -> VerifiedBlob
   On CryptoError::ChecksumMismatch -> StorageError::ChecksumMismatch, do NOT call decrypt_chunk
3. padded_plaintext = decrypt_chunk(verified_blob, file_key, file_id, chunk_index)
4. If last chunk: bytes_to_write = file_size - (chunk_index * chunk_size_bytes)
   Else: bytes_to_write = chunk_size_bytes
5. BufWriter writes bytes_to_write bytes to destination
6. Zeroize padded_plaintext  -- via Zeroizing<Vec<u8>> Drop
```

**CS-005** — New file upload canonical ordering (`design.md:377-388`, Decisions Made `design.md:565`):
```
1. generate_file_key()                                 -- FileKey (CSPRNG via SecretBox)
2. wrap_file_key(&file_key, &kek)                      -- WrappedFileKey
3. encrypt_file(source, file_id, &file_key, ..., staging) -> Vec<ChunkRecord>   [NO TX OPEN]
4. BEGIN TRANSACTION
5. MetadataStore::insert_node(&node { file_key_wrapped: Some(wrapped.0), ... })
6. MetadataStore::insert_chunks(&chunks)
7. COMMIT
8. file_key drops -- SecretBox zeroizes
```

**CS-006** — Hybrid routing (`design-invariants.md §8`; `design.md:66-75`):
```
RouteDecision::Immediate   iff  epoch_buffer_enabled == false
                                OR file_size >= chunk_size_bytes
RouteDecision::EpochBuffer iff  epoch_buffer_enabled == true
                                AND file_size < chunk_size_bytes
```

**CS-007** — `StorageError::from_crypto` (new helper, DC-4):
```rust
// in src-tauri/src/storage/error.rs
pub(crate) fn from_crypto(error: crate::crypto::CryptoError) -> Self {
    use crate::crypto::CryptoError;
    match error {
        CryptoError::ChecksumMismatch => Self::ChecksumMismatch,
        other => Self::Database(other.to_string()),
    }
}

impl From<crate::crypto::CryptoError> for StorageError {
    fn from(error: crate::crypto::CryptoError) -> Self { Self::from_crypto(error) }
}
```

---

### Step 1 — Add `CryptoError` mapping to `StorageError`

**File**: `src-tauri/src/storage/error.rs`.
Apply `CS-007`. Add `#[cfg(test)]` tests covering: `ChecksumMismatch` round-trip via `from_crypto`; non-checksum variants (`DecryptionFailed`, `EncryptionFailed`, `InvalidBlobFormat`, `KeyWrapFailed`, `KeyUnwrapFailed`, `KeyDerivationFailed`) all map to `Database` and preserve the `Display` text. Resolves DC-4.

---

### Step 2 — Create `storage::pipeline` module

**Files**:
- `src-tauri/src/storage/pipeline/mod.rs` — re-exports `encrypt_file`, `decrypt_file`.
- `src-tauri/src/storage/pipeline/encrypt_file.rs` — implements `CS-001` per `CS-003` + `CS-005` ordering.
- `src-tauri/src/storage/pipeline/decrypt_file.rs` — implements `CS-002` per `CS-004` + `CS-005` decrypt path.
- `src-tauri/src/storage/pipeline/chunk_size.rs` — `pub(crate) async fn read_chunk_size_bytes(store: &dyn MetadataStore) -> Result<u64, StorageError>` (calls `store.get_meta("chunk_size_bytes")` + `parse_chunk_size_bytes`). Resolves DC-6.

Wire in `storage/mod.rs`: `pub mod pipeline;` and `pub use pipeline::{decrypt_file, encrypt_file};` (matches sub-phase public surface). Resolves DC-1.

---

### Step 3 — Implement `pipeline::encrypt_file` per `CS-001` / `CS-003`

**File**: `src-tauri/src/storage/pipeline/encrypt_file.rs`. Per-chunk loop:

1. `let chunk_size_bytes = read_chunk_size_bytes(metadata_store).await?;` (usize cast with `try_into` — range already validated).
2. `let file = tokio::fs::File::open(source).await.map_err(|e| StorageError::Io(e.to_string()))?;`
3. `let mut reader = tokio::io::BufReader::new(file);`
4. Loop with `chunk_index: u32 = 0`:
   a. `let mut plaintext: Zeroizing<Vec<u8>> = Zeroizing::new(vec![0u8; chunk_size_bytes as usize]);`
   b. `let bytes_read = reader.read(&mut plaintext).await.map_err(|e| StorageError::Io(e.to_string()))?;` — use `tokio::io::AsyncReadExt::read_buf` or a loop filling until EOF / full.
   c. If `bytes_read == 0 && chunk_index == 0`: return `Ok(Vec::new())` (0-byte file — no blobs).
   d. If `bytes_read == 0`: break (EOF cleanly on chunk boundary).
   e. If `bytes_read < chunk_size_bytes as usize`: `plaintext[bytes_read..].fill(0)` (zero-pad — but `Zeroizing` buffer was already zero-initialized; explicit fill is belt-and-braces and documents intent).
   f. `let owned = std::mem::take::<Vec<u8>>(&mut plaintext);` — move out; `Zeroizing` now holds empty Vec. `encrypt_chunk` consumes `owned`, overwrites in place, drops.
   g. `let wire_blob = encrypt_chunk(owned, file_key, &FileId::from_uuid(file_id), ChunkIndex::new(chunk_index))?;`
   h. `let checksum = compute_checksum(&wire_blob);`
   i. `let blob_uuid = Uuid::new_v4(); let blob_name = blob_uuid.hyphenated().to_string();`
   j. `let blob_path = staging_directory.join(format!("{blob_name}.blob"));`
   k. Write via `tokio::io::BufWriter`: `writer.write_all(&wire_blob).await?; writer.flush().await?; writer.into_inner().sync_all().await?;` (note: `sync_all` is optional — Phase 3.3 crash-recovery relies on orphan scan, but matches design intent of durable staging writes. Document: `sync_all` chosen for crash recovery correctness at modest perf cost; revisit if measurable bottleneck).
   l. `records.push(ChunkRecord { chunk_id: Uuid::new_v4(), node_id: ..., chunk_index, blob_name, size_padded: chunk_size_bytes, blake3_checksum: checksum.0 });`
   m. `chunk_index = chunk_index.checked_add(1).ok_or_else(|| StorageError::ConstraintViolation("chunk_index overflow".to_owned()))?;` (u32 max = 2^32; `design.md:548` notes 16 PiB at 4 MiB chunk is impractical but overflow-safe.)
   n. On short read `bytes_read < chunk_size_bytes`: this was the last chunk — break after pushing.
5. On any `?` error after chunk 0 was staged, before return: best-effort `tokio::fs::remove_file` for each blob already written (DC-9). Implemented via a drop-guard struct holding `&staging_directory` and the current `Vec<String>` of blob_names; `drop` iterates and ignores errors. Success path calls `guard.disarm()`.
6. **`node_id` parameter**: Sub-phase `ChunkRecord` has a `node_id: NodeId` field; the canonical design API `encrypt_file` (CS-001) does not take a `node_id`. Two resolutions: (a) add `node_id: NodeId` as a parameter to `encrypt_file` (deviates from CS-001), or (b) `upload_file` caller populates `node_id` on each `ChunkRecord` before `insert_chunks`. **Choose (b)**: `encrypt_file` populates `node_id: NodeId::new(Uuid::nil())` as a placeholder and caller overwrites; document in code comment. Rationale: preserves CS-001; `upload_file` owns the `NodeId`. Add a post-fill helper `pub(crate) fn assign_node_id(records: &mut [ChunkRecord], node_id: NodeId)` in `pipeline/mod.rs`.

Resolves DC-1, DC-6, DC-7, DC-9.

---

### Step 4 — Implement `pipeline::decrypt_file` per `CS-002` / `CS-004`

**File**: `src-tauri/src/storage/pipeline/decrypt_file.rs`.

1. `let chunk_size_bytes = read_chunk_size_bytes(metadata_store).await?;` — wait, CS-002 does **not** take `metadata_store`. So either fetch-by-ChunkRecord (`chunks[0].size_padded`) or extend the signature. **Choose**: extend `decrypt_file` with an added `chunk_size_bytes: u64` parameter and document: upload / download orchestration layer reads it once and passes in. Minor deviation from CS-002 to keep the pipeline pure (no `MetadataStore` dependency in decrypt). Alternative acceptable: take `&dyn MetadataStore` symmetry. **Decision**: take `&dyn MetadataStore` to match CS-001 signature shape; read `chunk_size_bytes` via `read_chunk_size_bytes`. Document as an extension of CS-002; raise in §7 documentation impact.
2. `let expected_count = if file_size == 0 { 0 } else { file_size.div_ceil(chunk_size_bytes) as usize };` — note: `file_size == 0` means zero chunks, so the following validation becomes a no-op.
3. Sort + validate (DC-5): `let mut sorted: Vec<&ChunkRecord> = chunks.iter().collect(); sorted.sort_by_key(|c| c.chunk_index);` Validate `sorted.len() == expected_count`; for `i in 0..expected_count` assert `sorted[i].chunk_index == i as u32`; assert `sorted[i].size_padded == chunk_size_bytes`. On violation → `StorageError::ConstraintViolation(...)`.
4. `let file = tokio::fs::File::create(destination).await.map_err(...)?; let mut writer = tokio::io::BufWriter::new(file);`
5. For each chunk in sorted order:
   a. Build blob path: `blob_directory.join(format!("{}.blob", chunk.blob_name))`.
   b. `let encrypted = tokio::fs::read(&blob_path).await.map_err(|e| StorageError::Io(e.to_string()))?;` — blobs are `chunk_size_bytes + 40` bytes, well under memory budget.
   c. `let expected_hash = Blake3Hash(chunk.blake3_checksum);`
   d. `let verified = verify_checksum(encrypted, &expected_hash).map_err(StorageError::from)?;` — `ChecksumMismatch` propagates as `StorageError::ChecksumMismatch` before `decrypt_chunk` is called (meets acceptance criterion).
   e. `let padded_plaintext: Zeroizing<Vec<u8>> = Zeroizing::new(decrypt_chunk(verified, file_key, &FileId::from_uuid(file_id), ChunkIndex::new(chunk.chunk_index))?);`
   f. Compute `bytes_to_write`: if this is the last chunk, `bytes_to_write = file_size - (chunk.chunk_index as u64) * chunk_size_bytes;` else `bytes_to_write = chunk_size_bytes;`.
   g. `writer.write_all(&padded_plaintext[..bytes_to_write as usize]).await?;`
   h. `Zeroizing` drops at loop end.
6. `writer.flush().await?; writer.into_inner().sync_all().await?;` Return `Ok(())`.
7. Error path: destination may contain partial data — design `error-recovery: on retry the decrypt operation overwrites the destination file from the beginning`; consistent with `File::create` above. No cleanup.

Resolves DC-5, DC-6, DC-7.

---

### Step 5 — Create `storage::vault_ops` module with routing gate

**Files**:
- `src-tauri/src/storage/vault_ops/mod.rs` — re-exports `upload_file`, `download_file`, `RouteDecision`.
- `src-tauri/src/storage/vault_ops/routing.rs` — `enum RouteDecision { Immediate, EpochBuffer }`, `pub fn decide(file_size: u64, chunk_size_bytes: u64, epoch_enabled: bool) -> RouteDecision` (pure function, per `CS-006`).
- `src-tauri/src/storage/vault_ops/upload_file.rs` — orchestrates `CS-005`:
  1. Stat source via `tokio::fs::metadata` to obtain `file_size`.
  2. Read `chunk_size_bytes` + `epoch_buffer_enabled` via `MetadataStore::get_meta`.
  3. Call `RouteDecision::decide(...)`. If `EpochBuffer`: return `Err(StorageError::ConstraintViolation("epoch buffering not yet available; deferred to Phase 4".to_owned()))`. Document stub via short inline `// Phase 4 replaces this branch with epoch-buffer packing.`
  4. `let file_key = generate_file_key();`
  5. `let wrapped = wrap_file_key(&file_key, kek)?;`
  6. `let mut chunks = pipeline::encrypt_file(source, file_id, &file_key, metadata_store, staging_directory).await?;`
  7. `pipeline::assign_node_id(&mut chunks, node_id);`
  8. Build `Node` (file node with `file_key_wrapped: Some(wrapped.0)`, `size_bytes: file_size`, caller-supplied `name`, `parent_id`, timestamps).
  9. Under a single SQLCipher transaction (via `MetadataStore::insert_node` + `insert_chunks` back-to-back — both already tx-wrapped internally; see DC-2 note): `metadata_store.insert_node(&node).await?; metadata_store.insert_chunks(&chunks).await?;`. **Note**: the canonical design wraps both in a single tx; `MetadataStore` surface has two separate calls. For Phase 3.2, the two calls land in two back-to-back transactions (weaker atomicity); Phase 3.3 introduces a batched `apply_upload` helper or extends the trait. Document this as a scope boundary — the pipeline call cannot leak plaintext (file_key dropped, blobs opaque) even if `insert_chunks` fails after `insert_node`; the resulting orphan chunk rows are cleaned up by Phase 3.3 deletion flow.
  10. `file_key` drops; `Zeroize` zeroes.
  11. Return `node` or `ChunkRecord`s to caller (decide based on downstream needs — probably `Node`).
- `src-tauri/src/storage/vault_ops/download_file.rs` — orchestrates file access:
  1. `let node = metadata_store.get_node(node_id).await?;`
  2. If `node.node_type != NodeType::File`: `StorageError::ConstraintViolation("target is a directory")`.
  3. `let wrapped = WrappedFileKey(node.file_key_wrapped.ok_or_else(|| StorageError::ConstraintViolation("file node missing wrapped key".to_owned()))?);`
  4. `let file_key = unwrap_file_key(&wrapped, kek)?;`
  5. `let chunks = metadata_store.get_chunks(node_id).await?;`
  6. `pipeline::decrypt_file(destination, file_id_from_node, &file_key, node.size_bytes, &chunks, blob_directory, metadata_store).await?;`
  7. `file_key` drops.

Wire in `storage/mod.rs`: `pub mod vault_ops;` and `pub use vault_ops::{RouteDecision, download_file, upload_file};`.

Resolves DC-3, DC-8.

Add a **note on scope boundary with Phase 3.3** in `vault_ops/upload_file.rs` comment at the tx-atomicity site: "Phase 3.3 introduces `MetadataStore::apply_upload(&Node, &[ChunkRecord])` to fold steps 9a–9b into one SQLCipher transaction."

---

### Step 6 — Wire up module declarations

**File**: `src-tauri/src/storage/mod.rs`. After existing re-exports, add:
```rust
pub mod pipeline;
pub mod vault_ops;

pub use pipeline::{decrypt_file, encrypt_file};
pub use vault_ops::{RouteDecision, download_file, upload_file};
```

---

### Step 7 — Test suite

All test files follow `cargo fmt` + `cargo clippy -D warnings`. Named `test_<unit>_<scenario>_<outcome>`. `unwrap()` / `expect()` permitted only inside `#[cfg(test)]`.

#### 7a — `pipeline/encrypt_file.rs` unit tests (inline `#[cfg(test)] mod tests`)
- `test_encrypt_file_zero_byte_returns_empty_vec_no_staging_files` — creates empty temp source, asserts `Vec::new()` and no files in staging.
- `test_encrypt_file_one_byte_produces_one_blob_of_chunk_size_plus_forty` — single-byte source; assert one ChunkRecord, staging file size == `chunk_size_bytes + 40`.
- `test_encrypt_file_exactly_chunk_size_produces_one_blob` — boundary: exactly `chunk_size_bytes`. Assert 1 chunk.
- `test_encrypt_file_chunk_size_plus_one_produces_two_blobs` — boundary.
- `test_encrypt_file_exact_multiple_three_chunks_produces_three_blobs` — boundary.
- `test_encrypt_file_short_read_last_chunk_zero_padded` — assert ciphertext payload at chunk_size_bytes - 1 onward corresponds to zero plaintext (tested via round-trip with decrypt).
- `test_encrypt_file_blob_names_are_uuid_v4` — validates each ChunkRecord `blob_name` parses as UUID v4 and staging file `<blob_name>.blob` exists.
- `test_encrypt_file_chunk_index_monotonic_from_zero` — asserts `chunk_index = 0, 1, 2, ...`.
- `test_encrypt_file_blake3_checksum_matches_blob_contents` — read each blob, compute BLAKE3, compare with stored checksum.
- `test_encrypt_file_error_cleans_up_partial_blobs` — inject `tokio::fs::File::open` failure on Nth chunk via a mock staging directory (or use non-writable directory for later chunks — simplest: write N-1 chunks successfully, then `set_readonly` on staging dir and fail on Nth). Assert on error: no blob files remain in staging. **Fallback**: use `mockall` or a simulated write failure via a full disk (`tempfile` on tiny tmpfs — not portable). **Simpler**: restructure to use a `BlobWriter` trait in production, mock in test. For Phase 3.2 scope, downgrade to a **documented manual test** and cover the guard logic in a `pipeline/cleanup_guard.rs` unit test (DC-9 guard struct tested in isolation).

#### 7b — `pipeline/decrypt_file.rs` unit tests
- `test_decrypt_file_round_trip_single_chunk_returns_original` — encrypt 1 MiB → decrypt → byte equality.
- `test_decrypt_file_round_trip_multi_chunk_returns_original` — encrypt 10 MiB → decrypt → byte equality with small `chunk_size_bytes` override.
- `test_decrypt_file_zero_byte_produces_zero_byte_output` — `file_size = 0`, chunks empty, destination file size == 0.
- `test_decrypt_file_last_chunk_truncated_to_file_size` — encrypt a non-multiple file, decrypt, assert output length == file_size.
- `test_decrypt_file_blake3_mismatch_returns_checksum_mismatch_without_calling_decrypt` — tamper one staging blob byte, assert `Err(StorageError::ChecksumMismatch)`. To prove `decrypt_chunk` was not called, tamper inside the nonce region (offset 0) or tag region — BLAKE3 covers all bytes, so any single-byte flip is caught by `verify_checksum` before `decrypt_chunk` can be invoked. Test asserts error type only (sufficient per type-level enforcement — `VerifiedBlob` is unconstructible on failure).
- `test_decrypt_file_malformed_chunk_list_gaps_returns_constraint_violation` — pass `[chunks[0], chunks[2]]` with `file_size` requiring 3 chunks. Assert `Err(StorageError::ConstraintViolation(...))`.
- `test_decrypt_file_malformed_chunk_list_duplicate_index_returns_constraint_violation` — two chunks with same `chunk_index`.
- `test_decrypt_file_unsorted_chunks_are_sorted_before_decrypt` — pass `[chunks[2], chunks[0], chunks[1]]`, assert byte-identical output.
- `test_decrypt_file_size_padded_mismatch_returns_constraint_violation` — synthesize ChunkRecord with wrong `size_padded`.

#### 7c — `vault_ops/routing.rs` unit tests
- `test_decide_epoch_disabled_small_file_returns_immediate`
- `test_decide_epoch_disabled_large_file_returns_immediate`
- `test_decide_epoch_enabled_small_file_returns_epoch_buffer`
- `test_decide_epoch_enabled_exactly_chunk_size_returns_immediate` (boundary — `>= chunk_size_bytes`).
- `test_decide_epoch_enabled_large_file_returns_immediate`.
- `test_decide_zero_byte_epoch_enabled_returns_epoch_buffer` (edge — 0 < chunk_size).
- `test_decide_zero_byte_epoch_disabled_returns_immediate`.

#### 7d — `vault_ops/upload_file.rs` + `download_file.rs` integration (test-only `MockMetadataStore`)
- `test_upload_download_round_trip_one_chunk` — end-to-end via `MockMetadataStore` + `tempfile::TempDir`.
- `test_upload_download_round_trip_multi_chunk`.
- `test_upload_file_with_epoch_enabled_small_file_returns_constraint_violation` — set `epoch_buffer_enabled = true` on mock store (direct map write since `set_meta` rejects it), upload < chunk_size_bytes file, assert the "epoch buffering not yet available" error.
- `test_upload_file_with_epoch_enabled_large_file_succeeds` — same epoch override, upload >= chunk_size_bytes, assert round-trip works.
- `test_upload_file_persists_wrapped_key_and_chunks` — inspect `MockMetadataStore` for stored node's `file_key_wrapped` and chunk count.
- `test_download_file_wrong_kek_fails_with_database_or_checksum_error` — upload with KEK_A, download with KEK_B, assert non-panic error (mapped from `CryptoError::DecryptionFailed`).

#### 7e — Streaming invariant observational test
`test_encrypt_file_streaming_invariant_peak_allocation_bounded` — use a `Zeroizing`-aware tracking allocator (`std::alloc::GlobalAlloc` wrapper) to assert peak heap growth during `encrypt_file` on a multi-chunk file stays within `chunk_size_bytes + overhead` (generous bound, e.g. `2 * chunk_size_bytes`). **Alternative if allocator test is too brittle on Windows**: structural test — grep the `encrypt_file` source for any `Vec::with_capacity(file_size)` or `read_to_end` call on the source, assert absence via `include_str!` match. **Choose structural test**; document in test comment.

#### 7f — Property-based tests (`pipeline/encrypt_file.rs` proptests module)
- `prop_encrypt_decrypt_round_trip_identity` — `plaintext in proptest::collection::vec(any::<u8>(), 0..=(20 * 1024 * 1024))`, `file_key_seed`, `file_id_seed`. Write to `TempDir`, encrypt → decrypt → assert byte equality. Use a **reduced `chunk_size_bytes`** in property tests (128 KiB — minimum valid) to keep runtime reasonable; still exercises multi-chunk paths at feasible sizes. Set `ProptestConfig::with_cases(16)` (proptest is slow with real I/O).
- `prop_encrypt_produces_blob_count_matching_ceil_div` — for arbitrary `file_size in 0..=(5 * 1024 * 1024)`, encrypt and assert `chunks.len() == ceil(file_size / chunk_size_bytes)`.
- `prop_encrypted_blobs_have_exact_size` — each staging blob file length equals `chunk_size_bytes + 40`.

#### 7g — `error.rs` additions
- `test_from_crypto_checksum_mismatch_maps_to_checksum_mismatch`.
- `test_from_crypto_decryption_failed_maps_to_database`.
- `test_from_crypto_encryption_failed_maps_to_database`.
- `test_from_crypto_invalid_blob_format_maps_to_database_preserves_display`.
- `test_from_crypto_key_wrap_failed_maps_to_database`.
- `test_from_crypto_key_unwrap_failed_maps_to_database`.
- `test_from_crypto_key_derivation_failed_maps_to_database`.
- `test_from_trait_delegates_to_from_crypto`.

---

## 6. Review focus areas

### 6a. Rust change surface (anticipated Rust files)

- `src-tauri/src/storage/error.rs` — add `from_crypto` helper + `From<CryptoError>` impl + tests.
- `src-tauri/src/storage/mod.rs` — new module declarations + re-exports.
- `src-tauri/src/storage/pipeline/mod.rs` — new.
- `src-tauri/src/storage/pipeline/encrypt_file.rs` — new.
- `src-tauri/src/storage/pipeline/decrypt_file.rs` — new.
- `src-tauri/src/storage/pipeline/chunk_size.rs` — new (shared helper).
- `src-tauri/src/storage/vault_ops/mod.rs` — new.
- `src-tauri/src/storage/vault_ops/upload_file.rs` — new.
- `src-tauri/src/storage/vault_ops/download_file.rs` — new.
- `src-tauri/src/storage/vault_ops/routing.rs` — new.

### 6b. Security-sensitive paths (anticipated files under `src-tauri/src/{crypto,auth,storage}/`)

- `src-tauri/src/storage/pipeline/encrypt_file.rs` — plaintext `Zeroizing<Vec<u8>>` guard on success + error + cancellation; AAD = `file_id || chunk_index` handled by `encrypt_chunk` (no duplication); nonce freshness from `encrypt_chunk`; blob-name UUID v4 freshness; no plaintext copy outside the pipeline-local buffer.
- `src-tauri/src/storage/pipeline/decrypt_file.rs` — BLAKE3 pre-decrypt via `VerifiedBlob` type boundary (skipping `verify_checksum` is a compile error — `VerifiedBlob` is only constructible via `verify_checksum`); plaintext `Zeroizing<Vec<u8>>`; output truncation uses `file_size` from caller-controlled parameter (caller must source from `manifest_meta`/`nodes.size_bytes`, not user input).
- `src-tauri/src/storage/vault_ops/upload_file.rs` — `FileKey` lifetime strictly local to the function (generated → wrapped → used → dropped); `KeyEncryptionKey` borrowed reference only; no `expose()` inside application logic (only inside `wrap_file_key` / `unwrap_file_key` / chunk AEAD, which already protect the exposed slice with RAII).
- `src-tauri/src/storage/vault_ops/download_file.rs` — `FileKey` lifetime strictly local; wrapped-key bytes come from `Node.file_key_wrapped` which is typed `Option<[u8; 72]>` (no plaintext key in the Node struct).
- `src-tauri/src/storage/error.rs` — `from_crypto` forwards only static `Display` text (no dynamic plaintext leakage through error messages).

Security-reviewer focus list:
1. Streaming invariant — confirm no code path accumulates > 1 chunk plaintext in memory (check `read_to_end`, `Vec::with_capacity(file_size)`, `collect::<Vec<_>>()` over plaintext iterators).
2. Zeroization — `Zeroizing<Vec<u8>>` wraps both encrypt and decrypt plaintext buffers; `std::mem::take` hand-off into `encrypt_chunk` must not leave a plaintext shadow.
3. BLAKE3 pre-decrypt — every `decrypt_chunk` call site is preceded by `verify_checksum` (type-level enforced, but still inspect for any `unsafe` or `transmute` shortcuts).
4. AAD binding — chunk-loop chunk_index is the stored `ChunkRecord.chunk_index` (not a positional iterator counter) — prevents AAD/position mismatch on out-of-order chunks.
5. File-key lifetime — `FileKey` is never `clone()`d; never stored in a struct field; always `drop`s at function exit.
6. Error messages — no plaintext bytes embedded in any `StorageError` variant.

### 6c. Architecture risk areas

- `src-tauri/src/storage/pipeline/` — concern isolation (encrypt vs decrypt vs size-source helper); check for accidental leaks of implementation helpers to `pub` (only `encrypt_file`, `decrypt_file` should be `pub`).
- `src-tauri/src/storage/vault_ops/` — orchestration layer boundary with `pipeline::` (unidirectional: `vault_ops` depends on `pipeline`, not vice versa). `routing.rs` is a pure function with no dependencies beyond primitives — check it does not reach into `MetadataStore` (decision data flows in as arguments).
- `src-tauri/src/storage/mod.rs` — re-export discipline per Phase 3.1 precedent (`mod.rs` = re-exports only). New modules must follow the same pattern.
- Dependency flow — `crypto` → `storage` (storage depends on crypto). No storage types leak into crypto signatures.
- `Node.file_key_wrapped: Option<[u8; 72]>` vs `WrappedFileKey([u8; 72])` — confirm both locations stay in sync if the wire format width changes.
- Abstraction debt — `vault_ops::upload_file` issues two separate `MetadataStore` mutation calls (non-atomic across the pair). Flag as a **deferred abstraction** — Phase 3.3 introduces `MetadataStore::apply_upload` to fold both into one transaction. Architecture-reviewer should confirm this is acknowledged in the plan (it is — see Approach Step 5 note) and not silently landed as permanent.

### 6d. Testing requirements

**Boundary cases** (from sub-phase + Step 2 `/plan` adversarial review):
- `file_size == 0` (zero chunks, zero staging files).
- `file_size == 1`.
- `file_size == chunk_size_bytes - 1`.
- `file_size == chunk_size_bytes`.
- `file_size == chunk_size_bytes + 1` (two chunks, second short).
- `file_size == 2 * chunk_size_bytes` (exact multiple).
- `file_size == 3 * chunk_size_bytes + (chunk_size_bytes / 2)` (non-multiple multi-chunk).
- Hybrid routing: `file_size < chunk_size_bytes`, `== chunk_size_bytes`, `> chunk_size_bytes`, with `epoch_buffer_enabled` both `true` and `false`.

**Adversarial cases**:
- BLAKE3 mismatch detected before `decrypt_chunk`.
- Malformed chunk list: gap, duplicate, out-of-order.
- Wrong KEK on download.
- `size_padded` divergence from `chunk_size_bytes`.
- Chunk-index overflow (contrived test via `MockMetadataStore`).
- Unicode file names (source + destination `&Path` round-trips).
- Platform path separators (Windows `\` and Unix `/` — `Path::join` handles both, test on all three targets in CI).

**Property tests**: `proptest` over `0..=20 MiB` with reduced `chunk_size_bytes = 131_072`, 16 cases per property to keep CI time bounded.

**Validation checkpoint** (from sub-phase):
- `cargo test storage::pipeline` passes (all unit + property).
- Manual: encrypt a 10 MiB file; inspect staging — confirm blob count matches `ceil(10 MiB / chunk_size_bytes)`, each file is exactly `chunk_size_bytes + 40` bytes.
- Manual: corrupt one byte in a staging blob; attempt decrypt — confirm `StorageError::ChecksumMismatch`.

**Acceptance criteria (from sub-phase)**:
- All chunk boundary test cases pass.
- BLAKE3 mismatch detected before `decrypt_chunk`.
- No plaintext remains in heap after `encrypt_file` / `decrypt_file` return (success or error).
- 0-byte file: `encrypt_file` returns `Ok(vec![])`, no staging files.
- Hybrid routing enforced.

## 7. Documentation impact

- `docs/architecture/designs/chunking-and-manifest/design.md` — **minor update**: `decrypt_file` signature in "Encrypt Pipeline → Public API" (lines 319–335) should add `&dyn MetadataStore` parameter (Approach Step 4) for chunk-size sourcing, OR the canonical signature stays and Step 4's `&dyn MetadataStore` parameter is documented as an implementation-private wrapper (`pub(crate) async fn decrypt_file_impl(..., &dyn MetadataStore)` called by `pub async fn decrypt_file(...) -> which reads chunk_size_bytes via a helper`). **Recommended**: keep the canonical signature; route `chunk_size_bytes` via `ChunkRecord.size_padded` (asserted-consistent) — avoid design-doc churn. Revise plan Approach Step 4 to match at implementation time; finalize in problem-solver handoff.
- `docs/architecture/designs/chunking-and-manifest/diagrams/chunk-pipeline.md` — add a routing-decision node for the hybrid gate (Phase 3.2 visibility). **Deferred**: if `/diagram` skill is not invoked, update after implementation.
- `docs/architecture/designs/chunking-and-manifest/sub-phases/roadmap.md` — if DC-3 epoch-buffer stub is accepted, add a "Phase 3.2 notes" row clarifying the deferred branch. Low priority; optional.

## 8. Governance sync actions (pre-implementation)

| Action ID | Reason / linked concern | Target files | Required edit | Verification |
|---|---|---|---|---|
| GA-001 | DC-10 — `storage.md` has no pipeline anchor | `.claude/rules/storage.md` | Add a new "Pipeline" section after the existing "Chunking" section: one line each for (a) `storage::pipeline::{encrypt_file,decrypt_file}` location and `vault_ops::{upload_file,download_file}` orchestration split, (b) plaintext wrapped in `Zeroizing<Vec<u8>>` for encrypt + decrypt path, (c) BLAKE3 pre-decrypt enforced by `VerifiedBlob` type (already present — cross-link from the new section), (d) hybrid routing gate location (`vault_ops::routing::decide`). | `.claude/rules/storage.md` `grep -n "Pipeline"` shows a new section; content ≤ 8 lines total. |
| GA-002 | DC-10 — copilot instruction mirror | `.github/instructions/storage.instructions.md` | Propagate GA-001 via `/copilot-sync`. | Diff between `.claude/rules/storage.md` and `.github/instructions/storage.instructions.md` shows semantic equivalence (skill-reported). |
| GA-003 | DC-4 — new error-mapping helper must be visible to rule readers | `.claude/rules/storage.md` | Add one line under "I/O" or a new "Errors" subsection: "`StorageError::from_crypto` centralizes `CryptoError → StorageError` mapping; checksum mismatches surface as `StorageError::ChecksumMismatch`, all other crypto failures as `StorageError::Database`." | Grep match for `from_crypto`. |

Run `/copilot-sync` after rule edits (GA-002 depends on GA-001/GA-003).

**Sub-phase design doc touchups** (Section 7 above) are not governance-sync — they are tracked under Documentation impact.

## 9. Handoff Notes for Implementer

Working directory: `C:\Users\chris\source\repos\arx-runa` (repo root). Implementation lives entirely under `src-tauri/src/storage/`; Phase 3.1 groundwork is already on `development` (HEAD `79c6527`).

**Order of operations** (strict): Step 1 (add `StorageError::from_crypto` + tests) → Step 2 (create `pipeline/` skeleton) → Step 3 (`encrypt_file`) → Step 4 (`decrypt_file`) → Step 5 (`vault_ops/` orchestration + routing) → Step 6 (wire into `mod.rs`) → Step 7 (tests: inline unit tests per file, then `vault_ops` integration tests via `MockMetadataStore`, then property tests). Run `cargo test --workspace --all-targets --all-features` after each step. Run `cargo clippy --workspace --all-targets --all-features -- -D warnings` before declaring any step complete. Run `cargo fmt --all` before commit.

**Self-contained status**: the plan inlines canonical signatures (`CS-001`, `CS-002`), per-chunk flows (`CS-003`, `CS-004`), upload ordering (`CS-005`), routing rule (`CS-006`), and the new error helper (`CS-007`). The sub-phase document does not need to be re-read for correctness, but keep it open for the Implementation Notes (AAD construction details, blob-naming conventions, `proptest` strategy).

**Known traps**:
- `encrypt_chunk` consumes `Vec<u8>` by value and overwrites in place — do **not** clone the plaintext before calling it. Use `std::mem::take` out of `Zeroizing<Vec<u8>>` (per Approach Step 3).
- `VerifiedBlob::into_inner` is `pub(crate)` within `crypto` — the pipeline must call `verify_checksum` and then pass the `VerifiedBlob` straight to `decrypt_chunk`; do not try to deconstruct.
- `size_padded` validation: Phase 3.1 mock and SQLCipher store already enforce `size_padded == chunk_size_bytes` at insert; the pipeline still re-asserts defensively (DC-6).
- `epoch_buffer_enabled = true` path returns `StorageError::ConstraintViolation` (DC-3 / Approach Step 5 item 3) — do not silently fall through to the Immediate path.
- Blob naming: `blob_name` field in `ChunkRecord` stores the bare UUID string (no `.blob` extension); `.blob` is appended only at path construction.
- Windows path round-trips: `tempfile::TempDir` handles cross-platform; avoid hardcoded `/` in tests.
- Property test wall time: reduce `chunk_size_bytes` to 128 KiB in proptests (via a test-only `MockMetadataStore` with custom `chunk_size_bytes` seed) and cap cases at 16.
- Governance sync (Section 8) runs **before** code so `.claude/rules/storage.md` stays authoritative while `rust-reviewer` runs during implementation; failure to run `/copilot-sync` leaves the GitHub copilot mirror drifted.

Status: `draft` (no blocking concerns). Proceed via `/implement-plan .claude/plans/phase-3-2-encrypt-decrypt-pipelines.md`.
