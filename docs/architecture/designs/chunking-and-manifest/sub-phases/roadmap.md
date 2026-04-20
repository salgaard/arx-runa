# Chunking and Manifest — Sub-Phase Roadmap

**Parent design**: [`design.md`](../design.md)  
**Contract anchor**: [`design.md#contract-surface`](../design.md#contract-surface) is canonical for schema/pipeline contracts; sub-phases should reference it rather than duplicate contract payloads.  
**Created**: 2026-04-04  
**Status**: Draft  
**Implementation order**: 3.1 → 3.2 → 3.3 (strict dependencies)

---

## Overview

This sub-phase roadmap decomposes the chunking and manifest design (378 lines) into 3 independently testable implementation units, enabling incremental validation of the local storage layer before cloud synchronisation (Phase 4) depends on it.

**Total sub-phases**: 3

**Rationale for decomposition**:
-  **Size**: Exceeds ~100-150 lines (378 lines total)
-  **Trait boundaries**: `MetadataStore` trait → `MockMetadataStore` → `SqlCipherMetadataStore` → encrypt/decrypt pipelines
-  **Integration breadth**: Touches crypto module (Phase 1), auth module (Phase 2), introduces SQLCipher, staging directory, and file I/O pipelines
-  **Error surface**: Defines multiple distinct `StorageError` variants requiring separate test coverage
-  **Multi-step flows**: Encrypt pipeline, decrypt pipeline, file key lifecycle, error recovery, orphan cleanup

**Implementation strategy**: Build foundational schema and trait with mock → implement streaming encrypt/decrypt pipelines → add staging directory management and error recovery

---

## Dependency Graph

```
3.1 (SQLCipher schema + MetadataStore trait)
 ↓
3.2 (Encrypt/decrypt pipelines)
 ↓
3.3 (Staging directory + error recovery)
```

**Legend**:
- `→` strict dependency (must complete predecessor before starting)

---

## Sub-Phases

1. **[Phase 3.1: SQLCipher Schema and MetadataStore Trait](3.1-schema-and-metadata-store.md)**
   - SQLCipher schema creation (`nodes`, `chunks`, `manifest_meta`, sharing tables)
   - `MetadataStore` trait with 11 async methods (including `rename_node`, `move_node`)
   - `SqlCipherMetadataStore` and `MockMetadataStore` implementations
   - `Node`, `ChunkRecord`, and `StorageError` types
   - **Estimated**: ~250 lines production code, ~150 lines tests

2. **[Phase 3.2: Encrypt and Decrypt Pipelines](3.2-encrypt-decrypt-pipelines.md)**
    - `encrypt_file` — streaming, zero-pad, AEAD encrypt, BLAKE3 checksum, UUID blob naming
    - `decrypt_file` — BLAKE3 verify before decrypt, truncate last chunk, zeroize plaintext
    - Hybrid routing gate for `epoch_buffer_enabled` (small files buffered, large files immediate)
    - File key lifecycle integration (generate → wrap → store → use → zeroize)
    - Property-based tests for arbitrary file sizes
   - **Estimated**: ~200 lines production code, ~200 lines tests

3. **[Phase 3.3: Staging Directory and Error Recovery](3.3-staging-and-error-recovery.md)**
   - Staging directory creation and path management (Windows and Linux)
   - Orphan blob cleanup on startup
   - SQLCipher transaction wrapping for all manifest mutations
   - File deletion flow and crash recovery for all failure modes
   - **Estimated**: ~120 lines production code, ~100 lines tests

---

## Testing Strategy

### Per-Sub-Phase Testing
Each sub-phase includes its own test suite. Tests must pass before proceeding to the next sub-phase.

**Test types**:
- **Unit tests**: Core functionality in isolation (schema creation, trait methods, error mapping)
- **Mock-based tests**: Use `MockMetadataStore` for dependencies not yet backed by SQLCipher (Phase 3.2)
- **Property-based tests**: Use `proptest` for arbitrary file sizes in the encrypt/decrypt round-trip (Phase 3.2)
- **Integration tests**: Once all sub-phases complete, full vault operation (insert file, encrypt, stage, delete with orphan cleanup)

### Regression Testing
After completing each sub-phase, run:
```bash
cargo test storage        # All storage module tests must pass
cargo clippy -- -D warnings  # No new warnings
```

### Manual Testing Checklist
- Phase 3.1: Open manifest with wrong `sqlcipher_key` — confirm rejection, not corruption
- Phase 3.2: Encrypt a multi-chunk file; inspect staging blobs with a hex editor — confirm no plaintext
- Phase 3.3: Kill the process mid-encrypt; restart — confirm orphan blobs are deleted and no partial manifest entry exists

---

## Security Review Checkpoints

- **Phase 3.1**: Requires `security-reviewer` agent review (SQLCipher keying correctness, CASCADE deletion, UNIQUE constraint)
- **Phase 3.2**: Requires `security-reviewer` agent review (streaming invariant, zeroization of plaintext buffers, BLAKE3 pre-decrypt verification)
- **Phase 3.3**: No security review required (staging blobs are already AEAD ciphertext)

---

## Notes

- **SQLCipher keying**: The `sqlcipher_key` must be applied via `PRAGMA key` immediately after opening the connection, before any schema or data access. Failure to do so opens the database unencrypted.
- **Sharing tables**: `contacts`, `shares`, and `received_shares` are created in Phase 3.1 for schema completeness but are not populated until Phase 5. No application logic for these tables is written in Phase 3.
- **`file_key_wrapped` location**: Stored in the `nodes` table (per-file), not the `chunks` table. This eliminates N redundant copies for multi-chunk files; CASCADE deletion still removes it correctly.
- **0-byte files**: A valid edge case — the `nodes` row is inserted with `size_bytes = 0` and a generated (but unused) `file_key_wrapped`. No chunk rows exist. The encrypt pipeline must handle this without error.
- **Streaming invariant**: At no point should more than one chunk's worth of plaintext reside in memory. This invariant must hold even for single-chunk files.
- **Phase 3.2 deferred branch**: when `epoch_buffer_enabled = true` and `file_size < chunk_size_bytes`, upload returns a documented deferral error until Phase 4 implements epoch-buffer packing.

---

## References

- **Parent design**: `docs/architecture/designs/chunking-and-manifest/design.md`
- **Roadmap entry**: `docs/roadmap.md` Phase 3
- **Related phases**: Phase 1 (cryptographic primitives — `encrypt_chunk`, `decrypt_chunk`, BLAKE3, key wrapping), Phase 2 (authentication — `sqlcipher_key` from `SessionKeys`), Phase 4 (cloud synchronisation — consumes staging blobs)
