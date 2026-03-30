---
timestamp: "2026-03-29T21:10:03+0200"
type: decision
report-sections:
  - method
  - analysis
tags: [chunking, padding, manifest, sqlcipher, pipeline, file-key]
source: agent
commit: "5d71df7"
---

## Chunking and Manifest Database — Key Design Decisions

## Context

Phase 3 of VoidGate implements the storage layer: fixed-size chunking, uniform padding, per-file key lifecycle, SQLCipher manifest database, and the encrypt/decrypt pipelines. Two significant design questions were resolved: the chunk size (a pending decision since the roadmap was written) and the correct location for `file_key_wrapped` in the schema.

## Substance

### Chunk size: 4 MiB

A fixed chunk size of 4 MiB (4,194,304 bytes) was chosen over 8 MiB. The decision was made by quantifying padding waste:

| File size | Padded total (4 MiB) | Waste |
|-----------|---------------------|-------|
| 1 byte | 4 MiB | ~100% |
| 1 MiB | 4 MiB | 75% |
| 4 MiB | 4 MiB | 0% |
| 10 MiB | 12 MiB | 17% |
| 100 MiB | 100 MiB | 0% |

For files larger than one chunk, maximum waste is constant at < 4 MiB. Average waste per file ≈ 2 MiB. With 8 MiB chunks, average waste doubles to ≈ 4 MiB. Additional advantages of 4 MiB: lower per-chunk memory buffer, finer upload resume granularity. Per-chunk crypto overhead (40 bytes nonce + tag) is negligible at < 0.001% of chunk size.
<!-- CITE: Breaking and Fixing Content-Defined Chunking — https://eprint.iacr.org/2025/558.pdf — supports fixed-size over CDC for metadata privacy -->

### Padding scheme: zero-fill, truncate via manifest

Each chunk is zero-padded to exactly `chunk_size` before encryption. On reassembly, the file's `size_bytes` from the `nodes` table determines where to truncate the last chunk. This is simple, unambiguous, and requires no special padding encoding (PKCS7 cannot encode padding larger than 255 bytes, making it unsuitable for 4 MiB chunks). The cloud sees uniformly sized blobs and cannot distinguish content from padding because the padding is encrypted.

### `file_key_wrapped` moved from chunks table to nodes table

The previous schema placed `file_key_wrapped` in the `chunks` table (per chunk). Since every chunk of the same file uses the same `file_key`, this creates N redundant copies for a file with N chunks. Moving `file_key_wrapped` to the `nodes` table (per file) eliminates the redundancy. CASCADE deletion remains correct: deleting a node deletes the node row (including `file_key_wrapped`) and cascades to all chunk rows.

The correct schema:
- `nodes`: node_id, parent_id, node_type, name, created_at, modified_at, size_bytes, **file_key_wrapped**
- `chunks`: chunk_id, node_id, chunk_index, blob_name, size_padded, blake3_checksum

### File key lifecycle

A `file_key` is generated per file at creation time (CSPRNG, 256-bit). It is immediately wrapped with `key_encryption_key` and stored in the `nodes` table. The unwrapped `file_key` exists in memory only during encrypt and decrypt operations, and is zeroized immediately after use. This means the `key_encryption_key` is the only long-lived key in the session that grants access to file data — a single authenticated session gives access to all vault files, not just one.

### Streaming invariant

At no point is more than one chunk's worth of plaintext in memory. The `BufReader` reads `chunk_size` bytes, the chunk is encrypted, the plaintext buffer is zeroized, and the next chunk is read. This ensures that even very large files do not require proportional RAM.

### Error recovery via transactions

All manifest mutations (insert node + chunks, delete node) are wrapped in SQLCipher transactions. If a crash occurs mid-encrypt, the transaction is not committed and no partial state exists in the manifest. Orphaned staging blobs (written but never referenced by a manifest entry) are cleaned up on next startup.

## Alternatives considered

### Content-Defined Chunking (CDC)

Variable-size chunks based on file content (e.g., Rabin fingerprinting). Rejected: CDC leaks file size as a side channel because the number and size of chunks correlates with file content. Two papers from 2025 specifically demonstrate this attack against backup services using CDC.
<!-- CITE: Chunking Attacks on File Backup Services using Content-Defined Chunking — https://eprint.iacr.org/2025/532.pdf -->
<!-- CITE: Breaking and Fixing Content-Defined Chunking — https://eprint.iacr.org/2025/558.pdf -->

### 8 MiB chunk size

Double the padding waste (average ~4 MiB per file vs ~2 MiB), higher per-chunk memory, coarser resume granularity. No compensating benefit for VoidGate's workload profile.

### PKCS7-style padding

Cannot encode padding values larger than 255 bytes. With 4 MiB chunks and a 1-byte file, 4,194,303 bytes of padding are needed — far beyond PKCS7's range. Zero-fill with `size_bytes` from the manifest is the correct approach.

### `file_key_wrapped` per chunk

N redundant copies of the same value. No benefit: CASCADE deletion works correctly with `file_key_wrapped` on the `nodes` table.

## Implications

- The chunk size decision is resolved — no longer a pending architectural decision
- The storage rules, roadmap, and sharing design are updated to reflect `file_key_wrapped` on the `nodes` table
- Phase 4 (cloud sync) consumes the staging directory written by the encrypt pipeline
- The quantified padding waste analysis provides concrete data for the bachelor report's Analysis section (sub-question 3)

## References

<!-- SOURCE: Breaking and Fixing Content-Defined Chunking — https://eprint.iacr.org/2025/558.pdf — analyses security vulnerabilities in CDC implementations; supports fixed-size chunking for metadata privacy -->
<!-- SOURCE: Chunking Attacks on File Backup Services using Content-Defined Chunking — https://eprint.iacr.org/2025/532.pdf — demonstrates that CDC leaks file size information as a side channel -->
