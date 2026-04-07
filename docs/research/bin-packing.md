# Arx Runa: Bin-Packing Small Files into Chunks

> **Document type**: Exploration / feasibility research
> **Status**: Living document
> **Last updated**: 2026-04-06

This document researches whether Arx Runa should support packing multiple small files into a single 4 MiB chunk to reduce the padding overhead identified in `compression-and-cloud-cost.md`.

---

## Table of Contents

1. [The Problem](#the-problem)
2. [Prior Art](#prior-art)
3. [How Bin-Packing Would Work in Arx Runa](#how-bin-packing-would-work-in-arx-runa)
4. [Privacy Analysis](#privacy-analysis)
5. [Write Amplification: The Core Trade-off](#write-amplification-the-core-trade-off)
6. [The RWS Trilemma](#the-rws-trilemma)
7. [When Bin-Packing Makes Sense](#when-bin-packing-makes-sense)
8. [Manifest Schema Changes Required](#manifest-schema-changes-required)
9. [Implementation Sketch](#implementation-sketch)
10. [Comparison with Existing Solutions](#comparison-with-existing-solutions)
11. [Recommendation](#recommendation)
12. [Decisions](#decisions)
13. [Open Questions](#open-questions)
14. [Sources](#sources)

---

## The Problem

Arx Runa uses fixed-size 4 MiB chunks. Every file occupies at least one full chunk regardless of actual size. The last chunk of every file is zero-padded to 4 MiB.

For files smaller than 4 MiB — the majority of photos taken on a smartphone — this wastes a significant fraction of every chunk:

| File type | Typical size | Storage used | Padding waste | Overhead |
|---|---|---|---|---|
| iPhone HEIC photo | 2.5 MB | 4 MiB | 1.62 MiB | **68%** |
| Android JPEG photo | 5 MB | 8 MiB | 3.24 MiB | **65%** |
| Small document | 50 KB | 4 MiB | 3.95 MiB | **99%** |

A vault of 10,000 iPhone photos stores 25 GB of real content but occupies 40 GiB of cloud space — a 60% overhead. This is the cost of the privacy guarantee: by keeping all blobs at a fixed size, Arx Runa prevents the cloud from inferring anything about individual file sizes.

The question is whether it is possible to eliminate the padding waste for small files without breaking the privacy model.

---

## Prior Art

### Facebook Haystack (2010)

The canonical production example of packing many small files into large storage objects is Facebook's **Haystack** system, described in the OSDI 2010 paper *"Finding a Needle in Haystack: Facebook's Photo Storage"* (Beaver et al.).

**The problem Haystack solved**: Facebook stores hundreds of billions of photos. A traditional filesystem stores each photo as a separate file with its own filesystem metadata (inode, directory entry, permissions). At Facebook's scale, this caused two problems:
1. Metadata overhead: each photo required multiple disk operations just to locate it
2. Storage efficiency: many small files waste space through filesystem block overhead

**The solution**: pack many photos (called **needles**) into very large files (called **haystacks**, 100 GB each). A compact in-memory index maps photo ID → (haystack file, byte offset, size). Reading a photo requires at most one disk seek.

**Key design properties**:
- **Write-once**: photos are appended to haystacks; existing needles are never modified in-place
- **Soft delete**: deleted photos are marked in the index but the bytes remain until a compaction pass
- **Compaction**: periodic background process rewrites haystacks, skipping deleted needles
- **In-memory index**: the index (offset + size per needle) is small enough to fit entirely in RAM

**Why this is directly relevant to Arx Runa**: Haystack demonstrates that packing many small files into large fixed-size containers is a proven, production-scale technique for exactly the use case Arx Runa targets — photo storage. The write-once model Haystack uses avoids the write amplification problem entirely.

### HDFS and the Small File Problem

The Hadoop Distributed File System (HDFS) is well-known to handle small files poorly. HDFS uses a default block size of 128 MB. Each file — regardless of size — consumes one full block of NameNode metadata (~150 bytes in memory per file). At millions of files, this saturates the NameNode.

Standard solutions in the Hadoop ecosystem:

| Solution | Approach | Trade-off |
|---|---|---|
| **HAR (Hadoop Archives)** | Pack many small files into a single HAR archive with an index | Read-only after creation |
| **SequenceFiles** | Key-value container format, keys = filenames | Requires application-level awareness |
| **Compaction** | Merge small files into target-size larger files | Periodic re-write required |

The HDFS small file problem is the same root issue as Arx Runa's padding overhead, scaled to petabytes. The solutions converge on the same pattern: aggregate into larger units, maintain an index, accept that mutation is expensive.

### IEEE Research: Optimal Packing for Encrypted KV Stores

A 2025 IEEE paper (*"Optimal Packing for Encrypted and Compressed Key-Value Stores With Pattern-Analysis Security"*) studies bin-packing specifically in the context of encrypted storage, where pack sizes must appear uniform to prevent pattern-analysis attacks.

Key finding: the pack length and access frequency distributions of packs must both appear uniform to adversaries. Existing algorithms minimise length differences but produce large variations in access frequency — creating a frequency-based side-channel. The proposed **DualPacking** algorithm optimises both dimensions simultaneously.

**Relevance to Arx Runa**: Arx Runa already solves the length problem (fixed-size blobs). DualPacking's insight about access frequency patterns is relevant if Arx Runa ever supports bin-packing: the order and timing of reads to a packed chunk reveals which files within it are accessed, not just the chunk itself.

### CMU PDL: Small File Performance in Object Storage

The CMU Parallel Data Lab paper *"Improving small file performance in object-based storage"* (2006) identifies the same overhead in object storage: small files cause high per-object metadata costs and poor storage utilisation. Proposed solutions include client-side aggregation (pack before upload) and server-side merging.

**Key insight**: aggregation at the client side (before upload) is simpler and more privacy-friendly than server-side merging, since the server never sees the individual files.

---

## How Bin-Packing Would Work in Arx Runa

### Concept

Instead of one file per chunk, multiple files are concatenated into a single 4 MiB chunk before encryption:

```
Chunk N (4 MiB, encrypted as one blob):
┌─────────────────────────────────────────────────────────────┐
│ file_A: 1.2 MiB │ file_B: 0.8 MiB │ file_C: 1.7 MiB │ PAD │
│ [1,258,291 bytes] [838,860 bytes]  [1,782,579 bytes] [315K] │
└─────────────────────────────────────────────────────────────┘
```

The blob uploaded to the cloud is identical to a non-packed blob: 4 MiB + 40 bytes (nonce + Poly1305 tag). The cloud sees nothing different.

The manifest must record, for each file, which chunk it lives in and at what byte offset:

```
file_A → chunk_id=X, byte_offset=0,         size=1,258,291
file_B → chunk_id=X, byte_offset=1,258,291,  size=838,860
file_C → chunk_id=X, byte_offset=2,097,151,  size=1,782,579
```

On retrieval: download chunk X, decrypt, slice out the relevant byte range, return to the caller. The padding bytes at the end are discarded.

### Packing algorithm

At upload time, Arx Runa maintains a **staging chunk** — a partially-filled 4 MiB buffer. Incoming small files are appended to the staging chunk until it is full (or a flush is triggered). When the staging chunk is full, it is encrypted and uploaded as a single blob.

Files larger than 4 MiB bypass the packing logic entirely and use the existing multi-chunk pipeline.

---

## Privacy Analysis

### Fixed-size blobs — preserved

All blobs remain 4 MiB + 40 bytes. The cloud cannot infer individual file sizes from blob dimensions. The key privacy property is fully preserved.

### Access pattern leak — new concern

With bin-packing, a single blob contains multiple files. When Arx Runa downloads a blob to retrieve file_B, it necessarily also downloads file_A and file_C (they are in the same encrypted blob). This is unavoidable — the blob is the atomic unit of decryption.

From the cloud's perspective: every access to a packed chunk looks the same as every other access — one blob download. The cloud cannot distinguish "user is reading file_B" from "user is reading file_C". This is actually **better** than per-file blobs, where each download reveals exactly which blob (and therefore approximately which file) is being accessed.

### Chunk boundary inference

An adversary watching blob access patterns over time could note that blob X is always fetched together with blob Y (because the same logical file spans multiple chunks). Bin-packing does not change this — files larger than 4 MiB still produce multiple sequential chunks with the same observable pattern.

For packed small files, the adversary cannot distinguish individual file accesses within a chunk. This is a privacy improvement, not a regression.

### Manifest security

The byte-offset information (which file lives at which offset in which chunk) is stored in the SQLCipher manifest database. The manifest is encrypted with `sqlcipher_key` (derived from `master_key`). The cloud never sees this mapping. Privacy impact is neutral.

**Summary: bin-packing does not weaken Arx Runa's privacy model, and may slightly improve it by coalescing file accesses.**

---

## Write Amplification: The Core Trade-off

The significant cost of bin-packing is **write amplification on mutation** — updating or deleting any file within a packed chunk requires re-processing the entire chunk.

### Delete scenario

User deletes file_B from chunk X (which also contains file_A and file_C):

```
1. Download blob X from cloud (4 MiB + egress cost)
2. Decrypt chunk X (CPU)
3. Remove file_B bytes, shift file_C left (or leave a gap)
4. Re-pad to 4 MiB
5. Generate new nonce, re-encrypt (CPU)
6. Upload new blob X' (4 MiB + ingress cost)
7. Delete old blob X from cloud
8. Update manifest: file_A and file_C now in chunk X', file_B removed
```

One logical delete of a small file triggers: 1 blob download + 1 blob upload + 2 cloud API calls + manifest update.

### Update scenario

User replaces file_A with a new version of different size:

If new size fits in the same chunk: same as delete + repack.

If new size is larger and doesn't fit: file_A must be moved out of chunk X into a new chunk (or split across chunks). file_B and file_C remain in chunk X with a larger gap, reducing efficiency.

### Amplification factor

For a chunk containing N files, the write amplification factor is N — one logical write causes N files' worth of data to be re-encrypted and re-uploaded. For N=8 files packed into one chunk, deleting one triggers reading and writing 8 files' worth of data.

This is the same trade-off that motivated Facebook Haystack's **write-once design**: Haystack avoids write amplification entirely by never modifying needles in-place. Deletes are soft (marked in the index); actual reclamation happens asynchronously during compaction.

---

## The RWS Trilemma

A fundamental result from storage system design is that it is impossible to simultaneously optimise for:

- **R** — Read amplification (number of reads per logical read)
- **W** — Write amplification (number of writes per logical write)
- **S** — Space amplification (storage used / logical data size)

*Pick two.* For Arx Runa:

| Strategy | Read amp | Write amp | Space amp | Notes |
|---|---|---|---|---|
| Current (1 file per chunk) | 1× | 1× | High (padding) | Space amp is the problem |
| Bin-packing (mutable) | 1× | High (N×) | Low | Solves space, costs write |
| Bin-packing (write-once) | 1× | 1× (append-only) | Low | Solves both — but no in-place delete |
| Bin-packing + compaction | 1× | Amortised 1× | Low | Complex; background re-pack pass needed |

The Haystack approach (write-once + async compaction) achieves good read, write, and space amplification by deferring deletions. This is only practical for archival workloads.

---

## When Bin-Packing Makes Sense

### Good fit: archival / write-once vaults

- Photo archives: upload photos, rarely delete or modify them
- Document archives: scan documents once, store permanently
- Backup snapshots: point-in-time backups are inherently write-once

For these workloads, the Haystack model applies directly: append new files to staging chunks, soft-delete with compaction on demand. Write amplification is not a concern because mutations are rare.

### Poor fit: general-purpose mutable vaults

- Active working documents: files updated frequently
- Developer secrets: rotated regularly
- Any vault where individual files are added and removed frequently

For these workloads, write amplification makes bin-packing expensive. The current one-file-per-chunk design is correct.

### Neutral: mixed vaults

A vault containing both archival photos (write-once) and active documents (mutable) could use bin-packing selectively — only for files below a size threshold and in a "frozen" state. This requires tracking per-file mutability, which adds complexity.

---

## Manifest Schema Changes Required

The current `chunks` table associates each chunk with a single `node_id`:

```sql
CREATE TABLE chunks (
    chunk_id        TEXT PRIMARY KEY,
    node_id         TEXT NOT NULL REFERENCES nodes(node_id) ON DELETE CASCADE,
    chunk_index     INTEGER NOT NULL,
    blob_name       TEXT NOT NULL,
    size_padded     INTEGER NOT NULL,
    blake3_checksum BLOB NOT NULL,
    UNIQUE(node_id, chunk_index)
);
```

Bin-packing requires chunks to reference multiple files. The schema must be restructured:

```sql
-- Chunks become independent of individual files
CREATE TABLE chunks (
    chunk_id        TEXT PRIMARY KEY,
    blob_name       TEXT NOT NULL UNIQUE,
    size_padded     INTEGER NOT NULL,  -- always chunk_size (4 MiB)
    blake3_checksum BLOB NOT NULL,
    is_packed       INTEGER NOT NULL DEFAULT 0  -- 0 = single-file, 1 = packed
);

-- New table: maps files to their byte ranges within chunks
CREATE TABLE file_extents (
    extent_id       TEXT PRIMARY KEY,
    node_id         TEXT NOT NULL REFERENCES nodes(node_id) ON DELETE CASCADE,
    chunk_id        TEXT NOT NULL REFERENCES chunks(chunk_id),
    chunk_index     INTEGER NOT NULL,   -- for multi-chunk files
    byte_offset     INTEGER NOT NULL,   -- offset within the decrypted chunk
    byte_length     INTEGER NOT NULL,   -- length of this file's data in this chunk
    UNIQUE(node_id, chunk_index)
);
```

This is a significant schema migration. The existing single-file-per-chunk path can be retained as `is_packed = 0` with `byte_offset = 0` and `byte_length = size_bytes`, making it backwards-compatible.

---

## Implementation Sketch

### Upload path (packed)

```rust
pub struct ChunkStager {
    buffer: Vec<u8>,          // staging buffer, up to chunk_size
    extents: Vec<StagedExtent>, // file_id + offset + length for each file packed so far
}

impl ChunkStager {
    /// Try to pack file data into the current staging chunk.
    /// Returns None if the file is too large to fit (caller uses standard chunking).
    pub fn try_pack(&mut self, file_id: Uuid, data: &[u8]) -> Option<Vec<StagedExtent>> {
        if data.len() > CHUNK_SIZE {
            return None; // too large — bypass packing
        }
        if self.buffer.len() + data.len() > CHUNK_SIZE {
            // Flush current chunk first, then start fresh
            let flushed = self.flush();
            self.buffer.extend_from_slice(data);
            self.extents.push(StagedExtent { file_id, offset: 0, length: data.len() });
            return Some(flushed);
        }
        let offset = self.buffer.len();
        self.buffer.extend_from_slice(data);
        self.extents.push(StagedExtent { file_id, offset, length: data.len() });
        None
    }

    /// Pad to chunk_size, encrypt, and return the sealed chunk + extent records.
    pub fn flush(&mut self) -> Vec<StagedExtent> { /* ... */ }
}
```

### Retrieval path (packed)

```rust
pub async fn read_packed_file(
    node_id: Uuid,
    manifest: &Manifest,
    cloud: &dyn CloudBackend,
    file_key: &FileKey,
) -> Result<Vec<u8>> {
    let extent = manifest.get_extent(node_id)?;
    let blob = cloud.download(&extent.blob_name).await?;
    let plaintext = decrypt_chunk(&blob, file_key, &extent.aad)?;
    Ok(plaintext[extent.byte_offset..extent.byte_offset + extent.byte_length].to_vec())
}
```

The caller receives only the relevant byte slice. file_A's bytes are never exposed when reading file_B, even though they share a chunk — they are sliced out in memory after decryption, before returning to the UI layer.

---

## Comparison with Existing Solutions

| System | Packing approach | Mutability | Privacy | Relevance |
|---|---|---|---|---|
| **Facebook Haystack** | Needles in 100 GB haystack files, in-memory index | Write-once + async compaction | N/A (no encryption) | Direct model for archival photo vaults |
| **HDFS HAR** | Hadoop Archive: read-only container | Read-only | N/A | Hadoop-specific; not general |
| **ZIP / tar archive** | User-managed before storing in Arx Runa | User must re-archive to update | Neutral | Available today, no implementation needed |
| **Arx Runa bin-packing (proposed)** | Transparent, manifest-tracked, fixed-size blobs | Mutable with write amplification or write-once | Fixed-size blobs preserved | The proposed feature |

---

## Recommendation

**Bin-packing should not be added to the general-purpose vault pipeline.** Write amplification makes it unsuitable for mutable content, and the current design is correct for the common case.

**Two practical paths forward:**

### Path 1 — User-level archiving (available today)

Users with many small files (photo archives, document collections) should be advised to archive them before storing in Arx Runa. A ZIP or tar.gz of 1,000 photos is a single vault entry with negligible padding overhead.

Arx Runa should surface this guidance in the UI when many small files are added at once — e.g., *"You are adding 500 files under 4 MiB each. Archiving them first would reduce cloud storage by approximately X GB."*

### Path 2 — Opt-in archival vault mode (future feature)

Model after Facebook Haystack: an **archival vault mode** where:
- Files are write-once (no in-place update or delete)
- Bin-packing is applied transparently at upload time
- Soft-delete marks files as deleted in the manifest; bytes remain in the chunk
- A periodic **compaction** operation rewrites affected chunks, reclaiming space

This is appropriate for photo archives and document backups where the Haystack model fits naturally. It must be a distinct vault type, not a setting on a general-purpose vault.

### Conclusion

The padding overhead for small files is real but affordable in absolute terms (< $1/month for typical personal vaults on Backblaze B2). The right response is user education and optional archival mode — not transparently complicating the core mutable vault pipeline.

---

## Decisions

> Choices made during this research session. Updated as the session progresses.

| Decision | Alternatives considered | Rationale |
|---|---|---|
| **Bin-packing not added to the general-purpose vault pipeline** | Transparent bin-packing for all files, opt-in per-vault | Write amplification (N× re-encrypt and re-upload per mutation) makes it unsuitable for mutable content |
| **Two practical paths: user-level archiving (available today) and opt-in archival vault mode (future)** | No action, transparent bin-packing for all vaults | Archiving before storage is available today at zero implementation cost; archival vault mode modelled on Haystack is appropriate for write-once workloads only |

---

## Open Questions

1. **Compaction trigger**: in an archival vault mode, when should compaction run? On demand only, or automatically when deleted bytes exceed a threshold (e.g., > 20% of a chunk)?

2. **Partial chunk at vault close**: if a vault is closed mid-upload with a partially-filled staging chunk, should Arx Runa flush it (with padding) or hold it open for the next session?

3. **Cross-file key isolation**: in the current design, each file has its own `file_key`. In a packed chunk, multiple files share a single encrypted blob but have different `file_key` values. Decrypting the chunk requires the `file_key` of any one file in it — but AAD binding must still cover the correct (file_id, chunk_index) pair. This needs careful design to avoid weakening per-file key isolation.

4. **Chunk AAD in packed mode**: the current AAD construction is `file_id || chunk_index`. For packed chunks containing multiple files, AAD must bind to the chunk itself, not an individual file. A `chunk_id` AAD (`chunk_id || 0u64`) with per-file key derivation is one approach — but this requires the cryptographic primitives design to be updated.

5. **File sharing in packed chunks**: Arx Runa's sharing design (Phase 5) wraps a `file_key` per shared file. If the file lives in a packed chunk, sharing it requires the recipient to download and decrypt the entire chunk even to access one file. This may be acceptable for archival vaults where sharing is rare.

---

## Sources

| Source | Topic | URL |
|---|---|---|
| **Beaver et al. (OSDI 2010)** | Finding a Needle in Haystack: Facebook's Photo Storage — write-once bin-packing at scale | [usenix.org/legacy/event/osdi10/tech/full_papers/Beaver.pdf](https://www.usenix.org/legacy/event/osdi10/tech/full_papers/Beaver.pdf) |
| **Engineering at Meta** | Haystack blog post — efficient storage of billions of photos | [engineering.fb.com/2009/04/30/core-infra/needle-in-a-haystack](https://engineering.fb.com/2009/04/30/core-infra/needle-in-a-haystack-efficient-storage-of-billions-of-photos/) |
| **IEEE Xplore (2025)** | Optimal Packing for Encrypted and Compressed KV Stores With Pattern-Analysis Security | [ieeexplore.ieee.org/document/11214715](https://ieeexplore.ieee.org/document/11214715/) |
| **CMU PDL** | Improving small file performance in object-based storage | [pdl.cmu.edu/PDL-FTP/Storage/CMU-PDL-06-104.pdf](https://www.pdl.cmu.edu/PDL-FTP/Storage/CMU-PDL-06-104.pdf) |
| **AWS Blog** | Optimizing storage costs and query performance by compacting small objects | [aws.amazon.com/blogs/storage/optimizing-storage-costs-and-query-performance-by-compacting-small-objects](https://aws.amazon.com/blogs/storage/optimizing-storage-costs-and-query-performance-by-compacting-small-objects/) |
| **ScienceDirect** | An optimized approach for storing and accessing small files on cloud storage | [sciencedirect.com/science/article/abs/pii/S1084804512001610](https://www.sciencedirect.com/science/article/abs/pii/S1084804512001610) |
| **Wikipedia** | Bin packing problem — complexity, algorithms, approximation ratios | [en.wikipedia.org/wiki/Bin_packing_problem](https://en.wikipedia.org/wiki/Bin_packing_problem) |
| **Wikipedia** | Write amplification — definition and storage system impact | [en.wikipedia.org/wiki/Write_amplification](https://en.wikipedia.org/wiki/Write_amplification) |
| **Small Datum blog** | Read, write, and space amplification — pick 2 (RWS trilemma) | [smalldatum.blogspot.com/2015/11/read-write-space-amplification-pick-2](http://smalldatum.blogspot.com/2015/11/read-write-space-amplification-pick-2_23.html) |
| **Cloudera** | Small Files, Big Foils — HDFS small file problem and solutions | [cloudera.com/blog/technical/small-files-big-foils](https://www.cloudera.com/blog/technical/small-files-big-foils-addressing-the-associated-metadata-and-application-challenges.html) |


