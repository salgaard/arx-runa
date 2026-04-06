# Arx Runa: Reducing Padding Overhead

> **Document type**: Exploration / feasibility research
> **Status**: Living document
> **Last updated**: April 2026

This document surveys all known techniques for reducing the per-file padding overhead caused by Arx Runa's fixed-size 4 MiB chunk design, and evaluates each against Arx Runa's privacy model and implementation constraints.

For background on the overhead problem itself, see `compression-and-cloud-cost.md`. For bin-packing specifically, see `bin-packing.md`.

---

## Table of Contents

1. [The Encryption and Upload Flow](#the-encryption-and-upload-flow)
2. [Why Chunking and Padding Exist](#why-chunking-and-padding-exist)
3. [The Problem, Restated](#the-problem-restated)
4. [The Privacy Constraint](#the-privacy-constraint)
5. [Approach 1 — Bin-Packing](#approach-1--bin-packing)
6. [Approach 2 — Padmé Padding](#approach-2--padmé-padding)
7. [Approach 3 — Tiered Fixed Chunk Sizes](#approach-3--tiered-fixed-chunk-sizes)
8. [Approach 4 — Smaller Uniform Chunk Size](#approach-4--smaller-uniform-chunk-size)
9. [Approach 5 — Content-Defined Chunking (CDC)](#approach-5--content-defined-chunking-cdc)
10. [Approach 6 — Epoch-Based Deferred Batching](#approach-6--epoch-based-deferred-batching)
11. [Approach 7 — Hybrid Auto-Routing (Small-File Epoch Buffering)](#approach-7--hybrid-auto-routing-small-file-epoch-buffering)
12. [Upload Jitter — Why It Does Not Work](#upload-jitter--why-it-does-not-work)
13. [Chunk Size as a Security Parameter](#chunk-size-as-a-security-parameter)
14. [Vault-Specific Chunk Size](#vault-specific-chunk-size)
15. [Comparative Summary](#comparative-summary)
16. [Recommendation](#recommendation)
17. [Open Questions](#open-questions)
18. [Sources](#sources)

---

## The Encryption and Upload Flow

Before discussing security properties, it is useful to be precise about the order of operations — because it is not always intuitive.

### Correct order: chunk → pad → encrypt → upload

```
File (plaintext)
  │
  ├─→ Chunk 1 [████████████████] 4 MiB real data               → encrypt → upload as blob UUID-1
  ├─→ Chunk 2 [████████████████] 4 MiB real data               → encrypt → upload as blob UUID-2
  └─→ Chunk 3 [████████░░░░░░░░] 2 MiB real data + 2 MiB zeros → encrypt → upload as blob UUID-3
```

Only the last chunk is padded. All preceding chunks are naturally full at exactly 4 MiB and require no padding.

For a small file (e.g. a 500 KB document), there is only one chunk and it is almost entirely padding:

```
File (plaintext)
  │
  └─→ Chunk 1 [█░░░░░░░░░░░░░░░] 500 KB real data + 3.5 MiB zeros → encrypt → upload as blob UUID-1
```

This single blob is indistinguishable from a blob containing 4 MiB of real data. The cloud cannot tell the difference — which is the privacy benefit. The cost is the 3.5 MiB of wasted cloud storage.

Encryption happens **last**, after chunking and padding. Each chunk is encrypted independently with its own random 24-byte nonce and produces its own 16-byte authentication tag. The AAD (Additional Authenticated Data) bound to each chunk is `file_id || chunk_index`, which cryptographically ties each blob to its position in the file — preventing an attacker from swapping or reordering chunks.

This design follows the standard approach for streaming authenticated encryption described by Adam Langley (*"Encrypting Streams"*, imperialviolet.org, 2014) and implemented in libraries such as libsodium SecretStream and Google Tink's Streaming AEAD. The core requirement, as Langley notes, is that a chunked encryption scheme must prevent: chunk reordering, chunk dropping from the start or end, and cross-stream chunk injection. Binding `chunk_index` into the AAD of each chunk satisfies all three.

Encrypting the whole file first and then chunking would not work: you would have one large ciphertext with no way to partially download or verify individual segments, and no way to bind each segment to its position.

### What a "blob" is

A blob (Binary Large Object) is the atomic unit of storage in object storage systems such as Amazon S3, Backblaze B2, and Cloudflare R2. Each blob is a named byte sequence stored and retrieved as a whole — the storage system does not interpret its contents. In Arx Runa's case, each encrypted chunk is stored as one blob: an object on the cloud backend with a random UUID v4 name, containing `nonce (24 bytes) || ciphertext (4 MiB) || Poly1305 tag (16 bytes)`. The cloud treats it as opaque binary data — it can store, retrieve, and delete it, but cannot read its contents.

### Does the cloud know which blobs belong to the same file?

**No — and this is a key privacy property.**

When Arx Runa uploads a 10 MiB file, it produces 3 blobs with random UUID names:

```
3f8a2b1c-4d5e-6f7a-8b9c-0d1e2f3a4b5c   (4 MiB + 40 bytes)
a9f3c2e1-7b8c-9d0e-1f2a-3b4c5d6e7f8a   (4 MiB + 40 bytes)
7b2d4f8a-1c2d-3e4f-5a6b-7c8d9e0f1a2b   (4 MiB + 40 bytes)
```

The cloud cannot distinguish "one 10 MiB file split into 3 chunks" from "three separate 4 MiB files" or "one 4 MiB file and one 8 MiB file" or any other combination. All blobs are identical in size and randomly named. The **manifest** — stored locally in an encrypted SQLCipher database — is the only record of which UUID belongs to which file and in which order. The cloud never sees the manifest.

Your intuition is correct: the cloud sees a bucket of N uniform, anonymous blobs. It cannot count files, identify file sizes precisely, or link blobs together — unless it can observe upload timing.

### The timing side-channel

The one exception is **upload timing**. If all 3 chunks of a file are uploaded in rapid succession as a burst, an adversary watching the upload log might infer those 3 blobs are related. This is a weak side-channel — it reveals approximate file size (from burst size), not content. Research on encrypted traffic analysis confirms that timing and volume patterns remain exploitable even when payload content is fully encrypted (*"The Inevitability of Side-Channel Leakage in Encrypted Traffic"*, arxiv 2602.14055). Epoch-based batching mitigates this by interleaving blobs from multiple files in a single upload batch, making grouping inference harder.

A natural response to timing leakage is to add random upload delays — jitter of a few seconds between blob uploads to blur burst boundaries. This approach is evaluated and rejected in the [Upload Jitter](#upload-jitter--why-it-does-not-work) section below.

---

## Why Chunking and Padding Exist

Before addressing the cost, it is worth being precise about what security property chunking and padding actually provide — because every trade-off in this document is a trade-off against this property.

### What an adversary sees without padding

Suppose Arx Runa encrypted files and uploaded them as variable-size blobs — no padding, no fixed chunk size. The ciphertext content is unreadable. But the cloud provider, or anyone who can observe the storage bucket, would see:

- A blob of 2,497,152 bytes
- A blob of 5,242,880 bytes
- A blob of 52,428,800 bytes

**File sizes are metadata.** They reveal information independently of content:

| What the adversary observes | What they can infer |
|---|---|
| Blob is ~2.5 MB | Almost certainly a smartphone photo (HEIC/JPEG size range) |
| Blob is ~50 KB | Small document, config file, or thumbnail |
| Blob is ~4 GB | Large video file or disk image |
| Blob matches a known file exactly | Can confirm whether a specific file is present — even without decrypting it |

The last point is the most serious: an adversary who has a copy of a target file (e.g., a known document or photo) can compute its size and compare it against observed blob sizes to confirm or deny its presence in the vault. This is a **membership inference attack** — no decryption required. The 2019 PURBs paper (Nikitin et al., EPFL) formally characterises this class of leakage, and the 2024 *Broken Cloud Storage* research demonstrated it as a practical attack against five major E2EE cloud storage providers.

### What fixed-size chunking and padding provides

Arx Runa splits files into 4 MiB chunks and zero-pads the last chunk to 4 MiB before encryption. Every blob uploaded to the cloud is exactly **4 MiB + 40 bytes**.

A key question: if a file is split into multiple chunks, does the number of chunks reveal the file size?

**The cloud cannot directly count blobs per file.** All blobs have random UUID names, are identical in size, and have no structural links between them. The manifest — the only record of which blobs belong to which file — is encrypted locally in SQLCipher and never sent to the cloud. An adversary watching the storage bucket sees a pool of anonymous, uniform-size objects. They cannot determine how many blobs any given file produced by inspecting storage alone.

The only mechanism by which the adversary can learn N for a specific file is **upload timing**: if a file's chunks are uploaded in a rapid burst, the adversary watching the upload log can group those blobs together and infer N. If uploads from multiple files are interleaved — as in epoch batching — the adversary cannot decompose the total blob count into per-file chunk counts.

If timing correlation succeeds, the inferred N gives a size range:

```
(N − 1) × chunk_size  <  file_size  ≤  N × chunk_size
```

For a 4 MiB chunk, this gives a size range of width 4 MiB per file. A 2.5 MB photo and a 3.9 MB photo are indistinguishable — both produce 1 blob. A 5 MB photo and a 7.9 MB photo are indistinguishable — both produce 2 blobs. The **exact size within that range** is hidden because the manifest (which stores `size_bytes`) is encrypted in SQLCipher and never visible to the cloud.

The precise security property is therefore: **file size is hidden to within ±chunk_size, conditional on the adversary successfully correlating upload bursts to individual files.** It is not zero leakage — it is bounded, timing-conditional leakage.

What the design provides:

1. **Exact file size is not inferrable**: the cloud cannot determine whether a file is 2.5 MB or 3.9 MB — both produce one identical blob. The encrypted manifest is the only record of the exact size.

2. **No file type inference from blob size**: all blobs are identical in size, so the adversary cannot use blob dimensions to distinguish photos from documents, videos from source code.

3. **Membership inference is substantially blocked**: an adversary who possesses a target file of known exact size S cannot confirm its presence from blob sizes (all identical) or from per-file blob count (which requires timing correlation to observe). Without the manifest, they cannot link any set of blobs to any specific file.

4. **Blob-to-file mapping is hidden**: the manifest, which records which blobs belong to which file, is encrypted. The adversary cannot determine which N blobs form a single file, or how many files a given set of blobs represents. Timing is the only exception: blobs uploaded in close succession may allow grouping inference (see "What padding does NOT protect" below).

5. **Combined with UUID blob names**: blobs are named with random UUID v4 identifiers. The cloud sees N identically-sized, randomly-named opaque objects with no structural relationships between them.

### What padding does NOT protect

Padding addresses size-based inference. It does not protect:

- **Access patterns**: which blobs are downloaded, when, and how often. If the adversary can observe downloads, they may infer which files are accessed even if they cannot read them. Arx Runa does not currently address access pattern leakage. Research on encrypted traffic analysis shows that size and timing patterns persist as side-channels even when content is fully encrypted (*"The Inevitability of Side-Channel Leakage in Encrypted Traffic"*, arxiv 2602.14055).
- **Blob count over time**: the total number of blobs in the vault grows as files are added. An adversary watching the vault over time can observe when files are added and removed, even if not which files.
- **Upload timing**: the timing of uploads may correlate with user activity (e.g., a burst of uploads after a trip may suggest photo backup).

These are out of scope for the padding design and are acknowledged in the threat model.

### Why this justifies the overhead cost

The padding overhead (up to 68% for small files) is the price of the bounded-leakage guarantee above. Every approach in this document reduces that price by accepting *more* leakage — either a narrower size range, or exact size in the worst case. Understanding precisely what the padding buys (file size hidden to within ±chunk_size, blob-to-file mapping hidden by the encrypted manifest) is necessary to evaluate whether any given trade-off is worth making.

---

## The Problem, Restated

Arx Runa pads every file's last chunk to exactly 4 MiB before encryption. All blobs are identical in size — the cloud cannot determine exact file sizes, only a size range of width ±chunk_size from the blob count. The exact size within that range is protected by the encrypted manifest. The privacy property is strong, but the storage cost is high for small files:

| File | Actual size | Stored | Overhead |
|---|---|---|---|
| iPhone HEIC photo | 2.5 MB | 4 MiB | **68%** |
| Android JPEG photo | 5 MB | 8 MiB | **65%** |
| Small document | 50 KB | 4 MiB | **99%** |
| 10-min 4K video | 1.5 GB | 1,464 MiB | **~0%** |

The overhead only matters for small files. Large files (videos, archives) waste at most 4 MiB per file regardless of total size — negligible at scale. The problem is concentrated in photo libraries and small document collections.

---

## The Privacy Constraint

The current design achieves **bounded size leakage**: all blobs are 4 MiB + 40 bytes, so the adversary can infer a file's size only to within a 4 MiB range from the blob count. The exact size within that range is hidden by the encrypted manifest. An adversary cannot infer file types from blob dimensions, and cannot determine exact file sizes.

Any approach that reduces padding overhead either narrows that range (smaller chunks leak a tighter size bucket) or widens it further (larger chunks leak a coarser one). The key question for each approach is: **how much additional leakage does it introduce, and is it acceptable given Arx Runa's threat model?**

Arx Runa's threat model treats the cloud provider as untrusted and adversarial. The relevant question is not "does this leak anything?" but "does this leak enough to enable a meaningful attack?"

---

## Approach 1 — Bin-Packing

Pack multiple small files into a single 4 MiB chunk before encryption.

```
Chunk: [file_A: 1.2 MiB | file_B: 0.8 MiB | file_C: 1.7 MiB | padding: 0.3 MiB]
```

**Storage savings**: high — approaches zero padding for large enough batches of small files.

**Privacy**: no additional leakage — all blobs remain the same fixed size. The existing bounded leakage (±chunk_size from blob count) is unchanged.

**Core problem**: write amplification. Deleting or updating one file in a packed chunk requires decrypting, repacking, and re-encrypting the entire chunk.

**Best fit**: write-once archival vaults (photo archives, document backups). Modelled on Facebook Haystack.

**Covered in detail**: `bin-packing.md`.

---

## Approach 2 — Padmé Padding

Padmé is a padding scheme developed at EPFL as part of the PURB (Padded Uniform Random Blobs) research. Rather than padding all files to one fixed size, Padmé pads each file to the nearest value in a mathematically defined set of sizes — chosen to minimise both information leakage and storage overhead.

### How Padmé works

Padmé represents the file length as a floating-point number and rounds the mantissa, producing a padded length that clusters files into size tiers. The tier boundaries are closer together at small sizes and further apart at large sizes, adapting to the actual distribution of file lengths in the wild.

The result:
- An adversary learns at most **O(log log M)** bits about the file's size (where M is the maximum possible size)
- This is the same asymptotic leakage as padding to the next power of two
- But the maximum overhead is only **12%** instead of up to 100% for power-of-two

### Visualisation — what the cloud sees

With Arx Runa's current fixed-size chunking, every blob is exactly 4 MiB + 40 bytes regardless of the actual file size. A 500 KB photo and a 3.9 MB photo both produce a single identical blob — the cloud learns only that the file is somewhere in the 0–4 MiB range:

```
Current design (fixed 4 MiB chunks)

  500 KB file:
  └─→ Blob [█░░░░░░░░░░░░░░░] 500 KB data + 3.5 MiB zeros  → 4 MiB + 40 B

  2.5 MB file:
  └─→ Blob [██████████░░░░░░] 2.5 MB data + 1.5 MiB zeros  → 4 MiB + 40 B

  3.9 MB file:
  └─→ Blob [███████████████░] 3.9 MB data + 0.1 MiB zeros  → 4 MiB + 40 B

  All three blobs are identical in size. The adversary learns: "file is 0–4 MiB".
  Overhead: up to 88% for the 500 KB file, 68% for the 2.5 MB file.
```

With Padmé, each file is padded to the nearest Padmé tier — a much smaller size gap — and then encrypted. Blobs are no longer all the same size, but they cluster into a mathematically defined set of sizes:

```
Padmé design (variable blobs, bounded leakage)

  500 KB file:  padded to ~560 KB
  └─→ Blob [████████████░░░] 500 KB data + ~60 KB zeros  → ~560 KB + 40 B

  2.5 MB file:  padded to ~2.8 MB
  └─→ Blob [█████████████░░] 2.5 MB data + ~300 KB zeros → ~2.8 MB + 40 B

  3.9 MB file:  padded to ~4.0 MB
  └─→ Blob [███████████████] 3.9 MB data + ~100 KB zeros → ~4.0 MB + 40 B

  Blobs vary in size, but only between Padmé tier boundaries.
  The adversary learns: "file is 470–560 KB", "2.3–2.8 MB", "3.7–4.0 MB".
  Overhead: ≤ 12% per file.
```

The key difference is the **width of the leakage window**. Under fixed chunking, that window is 4 MiB wide for every small file. Under Padmé, the window shrinks proportionally with the file — a 500 KB file leaks only a ~90 KB range rather than a 4 MiB range. The adversary gains slightly more precise size information per file, but Arx Runa wastes far less cloud storage.

### Overhead profile

| File size | Padmé padded to | Overhead |
|---|---|---|
| 50 KB | ~56 KB | ≤ 12% |
| 500 KB | ~560 KB | ≤ 12% |
| 2.5 MB | ~2.8 MB | ≤ 12% |
| 5 MB | ~5.6 MB | ≤ 12% |
| 300 MB | ~324 MB | ≤ 8% |
| 1.5 GB | ~1.55 GB | ≤ 3% |

In practice the average overhead across a realistic file corpus is approximately **3%** — measured against 848,000 real hard-drive user files.

### Real-world impact from EPFL research

Applied to real datasets, Padmé reduces the fraction of files uniquely identifiable by size from:

| Dataset | Without Padmé | With Padmé |
|---|---|---|
| 56k Ubuntu packages | 83% uniquely identifiable | 3% |
| 191k YouTube videos | 87% uniquely identifiable | 3% |
| 848k user files | 45% uniquely identifiable | 8% |

### Privacy trade-off for Arx Runa

The current Arx Runa design already has **bounded** size leakage — the blob count reveals file size to within ±4 MiB (one chunk). Padmé replaces that coarse-grained chunk-count leakage with a finer set of size tiers. For a 2.5 MB photo: currently the cloud learns "this file is 0–4 MB" (1 blob). With Padmé it learns "this file is somewhere in the 2.3–2.8 MB range" — slightly more precise, but with dramatically less wasted storage. Whether this is a net privacy improvement depends on the vault's content — for a vault of photos that are mostly 2–4 MB, Padmé reveals slightly more within that range, but the range is already implied by the blob count.

Whether this is acceptable depends on the threat model. For most Arx Runa use cases:
- Knowing a file is a "2.3–2.8 MB image" is not actionable without the content
- The gain is substantial: from 68% overhead per HEIC photo to ≤ 12%

### Rust implementation

A Rust crate implementing Padmé exists: [`jedisct1/rust-padme-padding`](https://github.com/jedisct1/rust-padme-padding). Arx Runa could adopt this directly with minimal integration effort. The padding function takes a plaintext length and returns the padded length; zero-filling the remainder is unchanged from the current implementation.

### Applied to Arx Runa

Instead of one fixed blob size, Arx Runa would produce blobs in a defined set of Padmé-determined sizes. The manifest stores the original `size_bytes` for truncation on decrypt (unchanged from current design). The cloud sees blobs of varying but clustered sizes — not exact file sizes.

For files larger than one chunk (4 MiB), the last chunk is Padmé-padded; full chunks remain at 4 MiB. This preserves the fixed-size property for all complete chunks and applies Padmé only to the last (partial) chunk of each file.

---

## Approach 3 — Tiered Fixed Chunk Sizes

Instead of one fixed chunk size (4 MiB), define multiple fixed sizes — for example 256 KB, 1 MiB, and 4 MiB. Each file is assigned to the smallest tier that keeps padding overhead below a threshold.

### Example tier assignment

| File size | Tier chosen | Storage used | Overhead |
|---|---|---|---|
| 50 KB | 256 KB | 256 KB | 80% |
| 200 KB | 256 KB | 256 KB | 22% |
| 500 KB | 1 MiB | 1 MiB | 50% |
| 900 KB | 1 MiB | 1 MiB | 11% |
| 2.5 MB | 4 MiB | 4 MiB | 38% |
| 5 MB | 4 MiB × 2 | 8 MiB | 38% |
| 300 MB | 4 MiB × 75 | 300 MiB | ~1% |

### Privacy leakage

The cloud sees blobs of three different fixed sizes. It learns which tier a file belongs to — a 3-bit leakage for a 3-tier system. For a 256 KB blob: "this file is between 0 and 256 KB." For a 1 MiB blob: "this file is between 256 KB and 1 MiB."

This is a coarser anonymity set than Padmé (which creates many tightly-spaced tiers) but the leakage is bounded and predictable. Within a tier, all blobs are identical — no finer-grained size information is revealed.

### Key properties

- **No write amplification**: tiers apply at write time; mutation of a file does not change the tier assignment for other files
- **Simple to implement**: the manifest already stores `size_padded`; the vault configuration adds a tier table
- **No new dependencies**: purely a configuration change and padding arithmetic
- **Cloud cost per API call**: blobs in the 256 KB tier produce ~16× more cloud objects for large files than the 4 MiB tier — but large files use the 4 MiB tier anyway

### Variant: power-of-two tiers

Use 64 KB, 128 KB, 256 KB, 512 KB, 1 MiB, 2 MiB, 4 MiB as tiers. Maximum overhead is always < 100% (worst case: 1 byte above a tier boundary). Privacy leakage: O(log log M) bits — same asymptotic as Padmé, but with up to 100% overhead vs Padmé's 12% maximum. Power-of-two is simpler to reason about but less efficient than Padmé.

---

## Approach 4 — Smaller Uniform Chunk Size

The simplest possible change: reduce the chunk size from 4 MiB to a smaller value. Average padding waste per file is `chunk_size / 2`.

### Overhead comparison

| Chunk size | Avg waste/file | iPhone HEIC overhead | 1 GiB file chunks |
|---|---|---|---|
| 4 MiB (current) | ~2 MiB | 68% | 256 |
| 1 MiB | ~512 KB | 20% | 1,024 |
| 512 KB | ~256 KB | 10% | 2,048 |
| 256 KB | ~128 KB | 5% | 4,096 |

### Privacy

No additional leakage — all blobs are still a uniform fixed size within the vault. The adversary can still infer file size to within ±chunk_size from blob count, but this is the same bounded leakage as the current design. Smaller chunk sizes narrow that range and therefore leak more precise size information (see the chunk-size section above).

### Trade-offs

Smaller chunks produce more cloud objects per large file, which has real costs:

**Upload**: More cloud API calls. Most providers charge per-operation (AWS S3: $0.005 per 1,000 PUTs; Backblaze B2: $0.004 per 1,000 uploads; Cloudflare R2: $0.0036 per million). For a 1 GiB video at 1 MiB chunks: 1,024 uploads vs 256 at 4 MiB — 4× more operations. In absolute cost this remains small, but it compounds across large libraries.

**Restore (download)**: Each blob requires a separate HTTP GET. Downloading a 1 GiB video means 1,024 individual requests at 1 MiB chunks vs 256 at 4 MiB. Even with parallelism, the round-trip overhead and connection setup cost accumulate. Restore latency for large files increases meaningfully at small chunk sizes.

**Manifest size**: More rows in the `chunks` table and more entries per `file_extents` record. For a vault of large video files, this can grow significantly.

**Crypto overhead**: More AEAD decrypt operations per file retrieval. Each blob requires its own nonce read and Poly1305 tag verification. Negligible per operation, but scales with blob count.

The impact is asymmetric: small files (one blob at any chunk size) see no difference in API cost from smaller chunks. Large files pay proportionally more. A 1 GiB video that costs 256 API calls at 4 MiB costs 4× more at 1 MiB — and this matters most on restore, where the user is waiting.

| Chunk size | HEIC overhead | 1 GiB video blobs | 1 GiB download requests |
|---|---|---|---|
| 4 MiB (current) | 68% | 256 | 256 |
| 1 MiB | 20% | 1,024 | 1,024 |
| 512 KB | 10% | 2,048 | 2,048 |

### Chunk size selection rationale

The current 4 MiB was chosen to balance padding waste against blob count for the anticipated workload. Reducing to 1 MiB meaningfully improves photo overhead (68% → 20%) while keeping large file blob counts at a reasonable 1,024 per GiB. Below 1 MiB, the restore penalty for large files grows substantially with diminishing privacy benefit — the bounded leakage range narrows, but the adversary already cannot observe per-file blob count without timing correlation.

---

## Approach 5 — Content-Defined Chunking (CDC)

CDC splits files at content-dependent boundaries using a rolling hash (Rabin fingerprinting, Gear hashing, FastCDC). Chunks are variable-size but cluster around a target average. Used by restic, Borg, Kopia, Bupstash, Duplicacy, and Tarsnap — essentially all encrypted backup tools that support deduplication.

### Why backup tools use CDC

CDC enables **cross-file deduplication**: if the same data block appears in two different files (or two versions of the same file), the same chunk boundary will be found and the chunk stored once. This is the primary motivation — deduplication ratios of 60–80% are common for backup workloads.

### The privacy problem

CDC variable chunk sizes leak information. A 2025 paper (*"Breaking and Fixing Content-Defined Chunking"*, Kien Truong) demonstrated:

- An adversary observing encrypted chunk sizes can **fingerprint specific files** without decrypting them
- The vector of chunk lengths for a file can uniquely identify it among a known set of candidate files
- This enables a **membership inference attack**: "is this specific file present in this backup?"

This attack was demonstrated concretely against Tarsnap, Borg, and Restic. All three are vulnerable to an adversary who can observe encrypted blob sizes.

**For Arx Runa's threat model — where the cloud provider is explicitly adversarial — CDC is unacceptable.** Arx Runa's fixed-size chunk design was chosen precisely to prevent this class of attack. CDC would revert that protection.

### Deduplication without CDC

Deduplication without content-defined chunk sizes is not generally possible — fixed-size chunks from two versions of a file will differ unless the file is identical. Arx Runa does not target deduplication as a design goal, which makes CDC's primary benefit irrelevant in addition to its privacy cost.

---

## Approach 6 — Epoch-Based Deferred Batching

Instead of encrypting and uploading each file immediately, buffer all writes within a time window (an "epoch") and flush the accumulated files as a batch of packed chunks at the end of the epoch.

### How it works

```
Epoch window (e.g., 30 minutes or user-triggered):
  file_A added → held in local staging buffer
  file_B added → held in local staging buffer
  file_C added → held in local staging buffer
  ...
  Epoch flush:
    Pack files into 4 MiB chunks, encrypt, upload batch
```

### Difference from bin-packing

Standard bin-packing packs files into chunks and uploads immediately, then must re-pack on mutation. Epoch batching is **append-only within an epoch**: files written during an epoch are packed together and sealed. Subsequent mutations either:
- Create a new version of the file in the next epoch (append-only, old version soft-deleted)
- Or trigger an immediate flush of the current epoch

No in-place mutation of sealed epochs occurs. This eliminates write amplification.

### Privacy

Blobs remain fixed-size (4 MiB + 40 bytes). Epoch batching does not increase size leakage — all blobs are still identical.

Epoch batching **eliminates the timing side-channel** for batched files. Without it, uploading a 500 KB file produces one blob immediately — the adversary can correlate that single-blob burst to a single small-file addition. With epoch batching, that file's data is mixed into a chunk with other files and the entire epoch flushes as one burst. The adversary cannot determine how many files were added, what their sizes are, or which blobs correspond to which files.

### Trade-offs

- Files are not immediately available in cloud until the epoch flushes. For Arx Runa's use case (sync, not real-time streaming), this is generally acceptable.
- Partially-filled staging chunks at epoch flush incur last-chunk padding — but this applies once per epoch, not once per file.
- Soft-deleted files accumulate until a compaction pass.
- The local staging buffer must be encrypted at rest and cleared on vault lock.

### Suitability

Best fit for bulk imports (e.g., importing a full photo library) where files are added in large batches. Less useful for individual file additions where the epoch window closes with a single file — yielding no packing benefit. Approach 7 addresses this limitation.

---

## Approach 7 — Hybrid Auto-Routing (Small-File Epoch Buffering)

A refinement of epoch batching that routes files automatically based on size: small files go to the epoch buffer, large files upload immediately. This eliminates the main weakness of pure epoch batching (large file delay) while preserving its full benefit for small files.

### How it works

The natural threshold is the chunk size itself. A file smaller than one chunk cannot fill any complete chunk — it only ever produces padding waste. Such files benefit maximally from packing and have no urgent upload requirement. Files larger than one chunk upload all their chunks immediately, including the trailing partial — which is zero-padded to a full fixed-size blob as in the current design.

```
file size < chunk_size
  → queue entire file in local epoch buffer
  → packed with other small files at epoch flush
  → uploaded as full fixed-size blobs

file size ≥ chunk_size
  → ALL chunks encrypted and uploaded immediately
  → trailing partial padded to chunk_size and uploaded as a standalone blob
  → no epoch involvement — file is fully backed up immediately
```

Trailing partials of large files are **not** queued in the epoch buffer. Doing so would create a backup-completeness problem: if no small files follow, the epoch may never fill, leaving the large file partially backed up with its last chunk stuck in the local buffer indefinitely. Uploading the trailing partial immediately as a standalone blob avoids this entirely — large file backup is always complete as soon as the upload finishes.

### Visualisation

```
Small file (500 KB):
  └─→ epoch buffer → [file_A: 500 KB | file_B: 800 KB | file_C: 1.2 MiB | pad: 1.5 MiB]
                                                                         → encrypt → blob UUID-X

Large file (10 MiB):
  ├─→ Chunk 1 [████████████████] 4 MiB real data               → encrypt → upload as blob UUID-1
  ├─→ Chunk 2 [████████████████] 4 MiB real data               → encrypt → upload as blob UUID-2
  └─→ Chunk 3 [██░░░░░░░░░░░░░░] 2 MiB real + 2 MiB padding   → encrypt → upload as blob UUID-3
  (all three blobs uploaded immediately — same as current design)
```

### Privacy

All blobs are fixed-size (4 MiB + 40 bytes) — the invariant is preserved.

Small files gain a stronger privacy property than the current design. The adversary watching uploads cannot determine how many small files were added in a given epoch, what any individual small file's size is, or which blobs correspond to which files.

Large files behave identically to the current design — bounded timing-conditional leakage from blob count. No regression, no new leakage.

### Benefits over pure epoch batching

| | Pure epoch batching | Hybrid auto-routing |
|---|---|---|
| Small files: padding waste | Near zero | Near zero |
| Small files: timing leakage | Eliminated | Eliminated |
| Large files: upload delay | Full delay until epoch flush | None — all chunks upload immediately |
| Large files: restore latency | Full delay | None — all chunks in cloud immediately |
| Large files: backup completeness risk | Yes — last chunk stuck in buffer | None |
| Epoch buffer size | All files | Small files only |
| Manifest complexity | Single-mode | Dual-mode (small files only) |
| Implementation complexity | Medium | Medium + size threshold check |

### Restore mechanics

The manifest must support two kinds of chunk location: **standalone** (current design) and **packed extent** (epoch or bin-packed blob). On restore, the client resolves each chunk differently depending on type.

Because large files upload all chunks immediately (including trailing partials), they never appear in epoch blobs. The manifest schema change and dual-mode lookup only apply to small files.

**Manifest schema change:**

```sql
-- existing columns
blob_id      TEXT     -- UUID of the cloud blob (NULL if packed in epoch blob)
chunk_index  INTEGER

-- added for small-file packed extents only
epoch_blob_id  TEXT     -- NULL for all large-file chunks; non-NULL for packed small files
byte_offset    INTEGER  -- byte start within the epoch blob
byte_length    INTEGER  -- byte count of this file's data
```

Large file chunks always have `epoch_blob_id = NULL` — they are standalone blobs. The dual-mode logic only triggers for small files.

**Restore flow for a large file — unchanged from current design:**

```
Restore large_file.mp4 (10 MiB, chunk_size = 4 MiB):

  Manifest:
    chunk 0 → standalone blob UUID-1
    chunk 1 → standalone blob UUID-2
    chunk 2 → standalone blob UUID-3  (trailing partial, padded)

  1. Fetch UUID-1 → decrypt → chunk 0
  2. Fetch UUID-2 → decrypt → chunk 1
  3. Fetch UUID-3 → decrypt → chunk 2 (truncate 2 MiB padding)
  4. Concatenate → truncate to 10_485_760 bytes → done
```

No epoch involvement. No byte offset arithmetic. Identical to current restore logic.

**Restore flow for a small file packed in the same epoch blob:**

```
Restore small_doc.pdf (800 KB):

  Manifest:
    chunk 0 → epoch blob UUID-E, offset=0, len=819_200

  1. Fetch UUID-E (4 MiB + 40 B) → decrypt → 4 MiB plaintext
     → slice [0 : 819_200] → 800 KB → chunk 0
  2. Truncate to 819_200 bytes → file restored
```

If both files are restored in the same session, UUID-E only needs to be downloaded and decrypted once. The client can cache decrypted epoch blobs in memory across the restore of multiple files.

**AAD for epoch blobs:**

Arx Runa binds each standalone chunk to `file_id || chunk_index` in the AEAD AAD. An epoch blob contains data from multiple files, so there is no single file-specific binding. Epoch blobs use their own `epoch_blob_id` as the AAD. Individual file data integrity within the blob is guaranteed by the manifest's byte offsets — the manifest is protected by SQLCipher and authenticated at the database level.

---

### Comparison with bin-packing

Hybrid auto-routing and bin-packing solve the same problem with the same manifest schema. The differences are in *when* chunks are uploaded and *what happens on mutation*.

**Where they converge:**

Both pack multiple files into one fixed-size blob. Both require the `byte_offset` / `byte_length` schema extension. Both restore via the same two-path lookup. The implementation complexity of the manifest layer is identical.

**Where hybrid auto-routing wins:**

| | Bin-packing (immediate) | Hybrid auto-routing (epoch) |
|---|---|---|
| Write amplification on update/delete | Yes — must re-encrypt entire chunk | No — soft-delete, new version in next epoch |
| Timing leakage for small files | Yes — pack uploads immediately | No — epoch flush hides individual additions |
| Large file upload delay | None | None (full chunks immediate) |
| Small file upload delay | None | Until epoch flush |
| Small file cloud availability | Immediate | Delayed |

The decisive advantage of hybrid auto-routing is **no write amplification**. Bin-packing's write amplification is not a minor implementation detail — it compounds: deleting one file from a 4 MiB pack containing 8 small files forces a full decrypt-repack-re-encrypt cycle for all 8. In a vault with frequent deletions or renames, this becomes expensive and difficult to reason about.

**Can bin-packing be modified to match or beat hybrid auto-routing?**

Yes — by adopting the same techniques:

1. **Soft-delete + compaction**: Instead of repacking on delete, mark the extent as deleted in the manifest and repack lazily during a periodic compaction pass. This eliminates write amplification. At this point bin-packing's mutation behaviour is identical to hybrid auto-routing's.

2. **Deferred flush**: Instead of uploading each completed pack immediately, accumulate packs into an epoch and flush as a batch. This eliminates timing leakage. At this point bin-packing's upload behaviour is identical to hybrid auto-routing's epoch flush.

Once both modifications are applied, the two approaches are functionally identical — hybrid auto-routing is simply bin-packing that adopts soft-delete and epoch flush from the start rather than as retrofits.

**Bin-packing's one remaining advantage:**

A bin-packing implementation that uploads completed packs immediately (without epoch delay) gives small files immediate cloud availability. This is the only meaningful trade-off: timing leakage in exchange for no upload delay. For a backup tool where small files (documents, configs) are rarely time-sensitive, this trade-off is not worth making. For a use case where the user expects files to appear in the cloud within seconds of being added, it matters.

**Conclusion:** Hybrid auto-routing with epoch-based flush is the strictly better design for Arx Runa's use case. It matches bin-packing on storage efficiency, beats it on write amplification, and beats it on timing privacy. Bin-packing can be retrofitted to match, but doing so requires adopting the same two mechanisms that define hybrid auto-routing — at which point the distinction is architectural framing, not substance.

### Trade-offs

- Small files are not immediately available in cloud until the epoch flushes. Acceptable for backup/sync use cases.
- The epoch buffer must be encrypted at rest and cleared on vault lock.
- Soft-deleted small files accumulate in epoch chunks until compaction.
- Restore requires dual-mode chunk lookup for small files (packed extent vs standalone); large file restore is unchanged.
- Epoch flush trigger must handle the case where the buffer has data but no more small files arrive — vault lock should always force a flush to guarantee all data is in the cloud.

---

## Upload Jitter — Why It Does Not Work

A natural response to timing leakage is to add random delays between blob uploads — e.g., sleeping 1–5 seconds between each upload to blur burst boundaries. This is simple to implement and intuitively appealing. It does not solve the problem.

### The adversary controls the clock

Arx Runa's threat model designates the **cloud provider as adversarial**. The cloud provider records millisecond-precision timestamps on every blob creation in their own storage logs — they have server-side visibility that the client cannot influence. Adding a 3-second delay between uploads does not remove those timestamps from the cloud's log. The adversary simply observes "3 blobs appeared at T+2s, T+5s, T+8s, then silence for 10 minutes" and still groups them as one file's upload.

Jitter is effective against a **network-level observer** (an ISP or passive eavesdropper watching connection traffic) who has coarser timing and cannot see inside the cloud's logs. For that threat, a few seconds of randomisation meaningfully blurs burst boundaries. But network-level observation is not Arx Runa's primary threat — the cloud provider is.

### Statistical de-correlation

Even against a weaker adversary, jitter alone is consistently broken by traffic analysis. Timing obfuscation has been studied extensively in the context of Tor traffic, website fingerprinting, and VoIP analysis. The conclusion is consistent: random delays reduce correlation confidence but do not eliminate it, especially when the adversary can observe many uploads over time and build a statistical model of the upload pattern.

### The right tool for the right adversary

| Technique | Effective against cloud provider | Effective against network observer | Cost |
|---|---|---|---|
| Jitter (random delay) | No | Partially | Adds latency to every upload |
| Epoch batching | Yes | Yes | Delays small files until epoch flush |
| Constant-rate upload (dummy traffic) | Yes | Yes | Continuous cloud storage and API cost |

Constant-rate uploading — sending dummy blobs at a fixed rate regardless of real activity — is the only timing defence that fully defeats the cloud provider. It is impractical for a consumer backup tool because it means paying for continuous uploads and cloud storage even when no files are being added.

Epoch batching addresses the timing problem at the right level: it eliminates per-file blob grouping by interleaving data before upload, rather than trying to obscure when individual blobs arrive.

---

## Chunk Size as a Security Parameter

All previous approaches treat chunk size as a fixed implementation detail and focus on reducing the padding waste it causes. This section examines chunk size itself as a tunable security parameter — because it directly controls how much information the blob count leaks about file sizes.

### What the chunk count leaks

The cloud cannot directly observe which blobs belong to which file — blobs are randomly named and the manifest is encrypted locally. The adversary can only infer N for a specific file by correlating upload timing: a burst of N blobs likely corresponds to one file's upload. If uploads are interleaved (epoch batching), this inference fails.

Assuming timing correlation succeeds, the adversary who observes N blobs for a single file can infer:

```
(N − 1) × chunk_size  <  file_size  ≤  N × chunk_size
```

A larger chunk size means a wider range — more uncertainty for the adversary. This is independent of cryptographic strength: XChaCha20-Poly1305 is equally strong at any chunk size. The only security dimension that chunk size affects is **file size inference from blob count**.

### Concrete comparison across chunk sizes

For a 2.5 MB iPhone HEIC photo:

| Chunk size | Blobs produced | What the adversary learns | Storage overhead |
|---|---|---|---|
| 256 KB | 10 | File is 2.25–2.5 MB — very precise | 7% |
| 512 KB | 5 | File is 2.0–2.5 MB | 14% |
| 1 MiB | 3 | File is 2.0–3.0 MB | 20% |
| 4 MiB (current) | 1 | File is 0–4 MB | 68% |
| 8 MiB | 1 | File is 0–8 MB | 84% |

**The counterintuitive result**: reducing chunk size to save storage gives the adversary more precise file size information. Smaller chunks = better storage efficiency, weaker privacy. Larger chunks = worse storage efficiency, stronger privacy. These are directly opposed.

### The security ceiling

Increasing chunk size beyond a certain point stops improving privacy. Once the chunk size comfortably exceeds the typical file size in a vault, almost every file produces exactly one blob — the adversary only learns "file is smaller than chunk_size." Further increasing the chunk size adds storage overhead without narrowing the adversary's uncertainty any further.

For a vault of iPhone photos (avg 2.5 MB), going from 4 MiB to 8 MiB chunks only improves privacy for the minority of photos between 4–8 MB. The majority already produce one 4 MiB blob, so the adversary's inference is unchanged. Going from 4 MiB to 16 MiB provides essentially no additional benefit while doubling the average padding waste per file.

For video files (hundreds of MB to several GB), chunk size barely affects privacy in either direction — the blob count is in the hundreds regardless, and the file size is already inferable to within one chunk.

### Chunk size is not cryptographic block size

It is important not to confuse storage chunk size with the block size of a cipher. XChaCha20-Poly1305 is a stream cipher — it has no internal block size constraint in the traditional sense. The 4 MiB chunk size is a storage and metadata design decision, not a cryptographic one. Any chunk size that is a multiple of the nonce+tag overhead is equally valid from a cryptographic standpoint.

---

## Vault-Specific Chunk Size

Since chunk size is a privacy vs. storage efficiency dial, it is a natural candidate for a vault-level configuration option set at vault creation time.

### Why vault-level, not file-level

Chunk size is a property of the vault's storage format, not of individual files. Making it per-file would:
- Require the manifest to store chunk_size per file (schema complexity)
- Produce a mix of differently-sized blobs in the same vault (reducing the anonymity set — the adversary could distinguish file types by blob size)
- Complicate encryption and retrieval logic

Per-vault chunk size keeps all blobs in a vault uniform, preserving the equal-size blob guarantee within that vault.

### Vault types and ideal chunk sizes

| Vault use case | Ideal chunk size | Reasoning |
|---|---|---|
| Sensitive documents, legal/medical | 8 MiB | Maximum size ambiguity; storage overhead acceptable for small document collections |
| Photo archive | 4 MiB | Photos are 2–5 MB; most produce 1 chunk; good privacy with moderate overhead |
| Video archive | 4 MiB | Videos produce hundreds of chunks regardless; chunk size has little effect |
| Developer secrets / config files | 512 KB | Files are tiny; user explicitly accepts size visibility for lower cloud cost |
| General purpose (default) | 4 MiB | Reasonable balance for mixed workloads |

### Changing chunk size after vault creation

Chunk size cannot be changed without re-encrypting every blob in the vault. The existing blobs are padded and encrypted to the old chunk size — there is no way to resize them in-place. A chunk size change would require:

1. Downloading and decrypting all existing blobs
2. Re-chunking and re-encrypting with the new chunk size
3. Uploading all new blobs
4. Deleting all old blobs
5. Rebuilding the manifest

This is equivalent to recreating the vault from scratch. The chunk size should therefore be treated as an immutable vault property chosen at creation time, stored in `manifest_meta`, and validated on every open.

### Precedent in existing systems

- **CryFS** allows users to set the block size at vault creation (default 32 KiB). It is a documented configuration option, though framed as a performance parameter rather than a privacy one.
- **Azure Storage client-side encryption v2.1** made the chunk size configurable from 16 bytes to 1 GiB, having previously used a fixed 4 MiB.
- **Borg backup** exposes chunker parameters (target chunk size, min/max size) as command-line options at repository creation.

None of these systems frame chunk size as a privacy dial — they expose it as a technical parameter. Arx Runa could be the first to present it explicitly as a privacy vs. storage trade-off, with human-readable preset names rather than raw byte counts.

### Proposed vault creation UX

```
Vault storage mode:

  ○ Standard   (4 MiB chunks)
      Recommended for most use cases. Balanced privacy and storage cost.

  ○ Paranoid   (8 MiB chunks)
      Maximum file size ambiguity. Higher cloud storage usage.
      Best for: sensitive documents, legal or medical records.

  ○ Efficient  (512 KB chunks)
      Lower storage overhead for small files. Cloud can infer approximate
      file sizes within a 512 KB range.
      Best for: developer vaults, config files, small documents.
```

The vault header records the chosen chunk size. All subsequent operations use it without exposing the raw value to the user again.

### Cross-vault considerations

If a user shares a file between vaults with different chunk sizes, the share package (Phase 5) must include the chunk size used for the shared file so the recipient can reconstruct it correctly. This is already implied by the share package design, which records `chunk_size` alongside `chunk_uuids`.

---

## Comparative Summary

| Approach | Storage savings (small files) | Timing leakage eliminated | Size leakage to cloud | Mutation cost | Complexity | Rust dependency |
|---|---|---|---|---|---|---|
| **Current** (4 MiB fixed) | None | No | ±4 MiB range (timing-conditional) | None | — | — |
| **Larger chunk size** (e.g. 8 MiB) | Negative | No | ±8 MiB range (coarser, better) | None | Minimal | None |
| **Smaller chunk size** (e.g. 1 MiB) | Medium | No | ±1 MiB range (tighter, worse) | None | Minimal | None |
| **Vault-specific chunk size** | Depends | No | Depends on chosen size | None | Low | None |
| **Bin-packing** | High (near zero) | No | ±chunk_size (unchanged) | High (write amplification) | High | None |
| **Padmé padding** | High (max 12%) | No | O(log log M) bits per file | None | Low | `rust-padme-padding` |
| **Tiered chunk sizes** | Medium | No | Size bucket per tier | None | Low | None |
| **CDC** | High (+ dedup) | No | **High** (fingerprinting) | None | Medium | Multiple crates |
| **Epoch batching** | High (amortised) | Yes (for batched files) | None | Low (soft delete) | Medium | None |
| **Hybrid auto-routing** (Approach 7) | High (small files) | Yes (small files) | None (small files); ±chunk_size (large files, timing-conditional) | Low (soft delete) | Medium | None |
| **Upload jitter** | None | No (against cloud provider) | No change | None | Minimal | None |

---

## Recommendation

There is no single best answer — the right approach depends on the workload and how much privacy trade-off is acceptable. Three paths stand out:

### Path A — Smaller uniform chunk size (safest, lowest effort)

Reduce the chunk size from 4 MiB to **1 MiB**. This requires changing one configuration constant and no architecture changes. Privacy is unchanged (all blobs still identical). iPhone HEIC photo overhead drops from 68% to 20%. Video overhead increases from 0.6% to ~2% (acceptable).

This is the correct first step — it addresses most of the practical overhead with essentially zero risk.

### Path B — Padmé padding on the last chunk (best storage efficiency with bounded leakage)

Apply Padmé padding to the last (partial) chunk of each file. All complete chunks remain at the configured chunk size. The Rust crate `jedisct1/rust-padme-padding` is available. Maximum overhead drops to 12%. The cloud learns size buckets, not exact sizes.

This is the theoretically optimal approach for storage efficiency vs. leakage. It is a good candidate as an opt-in vault setting, clearly documented as trading the current bounded ±chunk_size leakage for near-zero padding overhead with Padmé-bounded leakage.

### Path C — Hybrid auto-routing with epoch batching (Approach 7)

Implement the size-threshold routing from Approach 7: files smaller than chunk_size are automatically queued for epoch batching; larger files upload full chunks immediately with only trailing partials queued. This gives small files zero padding waste and eliminates timing correlation for them, while large files remain immediately available in cloud with no upload delay.

This is the most complete solution for photo-heavy vaults without any privacy trade-off. It requires a manifest schema extension (`epoch_blob_id`, `byte_offset`, `byte_length` on chunks) and a dual-mode restore path, but these are bounded in scope and do not affect the crypto layer.

Bin-packing (Approach 1) can be retrofitted to match by adopting soft-delete and epoch-based flush — at which point it is functionally identical to Approach 7. Starting with Approach 7 is preferable because it makes the right design choices from the beginning rather than as corrections to a mutable bin-packing design.

This addresses the most common high-overhead scenario (photo library import) without touching the general-purpose vault pipeline.

### What to avoid

**CDC** is incompatible with Arx Runa's threat model and should not be adopted regardless of the storage benefits. The fingerprinting attacks are concrete and published.

**Full bin-packing on mutable vaults** introduces write amplification that is disproportionate to the storage benefit for general-purpose use. Restrict it to explicit archival vault mode if implemented at all.

---

## Open Questions

1. **Chunk size for bachelor project**: should the chunk size be changed from 4 MiB before implementation begins, or locked in and revisited post-launch? Changing it later is a breaking format change.

2. **Padmé as opt-in**: if implemented, Padmé changes the blob size contract. Vault metadata must record whether Padmé is enabled. Blobs from Padmé vaults and fixed-size vaults cannot be mixed in the same cloud path.

3. **Epoch flush trigger**: for epoch batching, what triggers a flush? Time elapsed, total buffered size, user action, vault lock? Each has different UX and security implications (a longer epoch means more data at risk in the local staging buffer).

4. **Combination approaches**: Padmé + epoch batching could be combined. Padmé handles the last chunk of each epoch; epoch batching handles full chunks. This would achieve near-optimal storage efficiency with only bounded leakage and no write amplification.

5. **User communication**: how should storage overhead be communicated? A "vault storage efficiency" indicator (showing actual content size vs. cloud usage) would help users understand the trade-off they are accepting.

---

## Sources

| Source | Topic | URL |
|---|---|---|
| **ImperialViolet — Encrypting Streams (Adam Langley, 2014)** | Canonical reference on per-chunk AEAD streaming encryption — position binding via AAD, chunk reordering/truncation attacks | [imperialviolet.org/2014/06/27/streamingencryption.html](https://www.imperialviolet.org/2014/06/27/streamingencryption.html) |
| **Libsodium — SecretStream** | Practical streaming file encryption API — independent nonce per chunk, authentication tag per chunk, last-chunk marking | [libsodium.gitbook.io/doc/secret-key_cryptography/secretstream](https://libsodium.gitbook.io/doc/secret-key_cryptography/secretstream) |
| **Google Tink — Streaming AEAD** | Streaming AEAD standard — per-segment encryption enabling partial decrypt and verification | [developers.google.com/tink/streaming-aead](https://developers.google.com/tink/streaming-aead) |
| **Authenticated Encryption — Wikipedia** | AEAD overview — role of associated data in binding ciphertext to context | [en.wikipedia.org/wiki/Authenticated_encryption](https://en.wikipedia.org/wiki/Authenticated_encryption) |
| **Google Cloud — What is Blob Storage** | Definition of Binary Large Object (blob) in object storage | [cloud.google.com/discover/what-is-binary-large-object-storage](https://cloud.google.com/discover/what-is-binary-large-object-storage) |
| **Microsoft Azure — Introduction to Blob Storage** | Blob storage architecture — objects stored and retrieved as whole units, opaque to storage system | [learn.microsoft.com/en-us/azure/storage/blobs/storage-blobs-introduction](https://learn.microsoft.com/en-us/azure/storage/blobs/storage-blobs-introduction) |
| **brokencloudstorage.info** | End-to-End Encrypted Cloud Storage in the Wild: A Broken Ecosystem — real attacks on E2EE providers (Sync, pCloud, Icedrive, Seafile, Tresorit) exploiting metadata leakage | [brokencloudstorage.info](https://brokencloudstorage.info/) |
| **The Hacker News (2024)** | Coverage of the broken E2EE cloud storage research — file size and metadata enabling practical attacks | [thehackernews.com — broken E2EE cloud storage](https://thehackernews.com/2024/10/researchers-discover-severe-security.html) |
| **ACM ToS — Encrypted Deduplication Leakage** | Information Leakage in Encrypted Deduplication via Frequency Analysis — file volume patterns exploitable as a side-channel | [dl.acm.org/doi/fullHtml/10.1145/3365840](https://dl.acm.org/doi/fullHtml/10.1145/3365840) |
| **arxiv — Side-Channel Leakage in Encrypted Traffic** | The Inevitability of Side-Channel Leakage — size and access pattern leakage persists even with content encryption | [arxiv.org/html/2602.14055v1](https://arxiv.org/html/2602.14055v1) |
| **lbarman.ch** | Padmé: Efficiently hiding file sizes — overview and overhead formula | [lbarman.ch/blog/padme](https://lbarman.ch/blog/padme/) |
| **PETs 2019 — Nikitin et al. (EPFL)** | Reducing Metadata Leakage from Encrypted Files and Communication with PURBs — Padmé definition, O(log log M) leakage bound, real-world evaluation on 848k files. DOI: 10.2478/popets-2019-0056 | [bford.info/pub/sec/purb.pdf](https://bford.info/pub/sec/purb.pdf) |
| **PURB — Wikipedia** | PURB overview and Padmé description | [en.wikipedia.org/wiki/PURB_(cryptography)](https://en.wikipedia.org/wiki/PURB_(cryptography)) |
| **GitHub — jedisct1/rust-padme-padding** | Rust implementation of Padmé — directly usable in Arx Runa | [github.com/jedisct1/rust-padme-padding](https://github.com/jedisct1/rust-padme-padding) |
| **GitHub — jedisct1/go-padme-padding** | Go implementation of Padmé — reference for algorithm details | [github.com/jedisct1/go-padme-padding](https://github.com/jedisct1/go-padme-padding) |
| **Padmé — age issue #83** | Discussion of applying Padmé to the `age` encryption tool — implementation considerations | [github.com/FiloSottile/age/issues/83](https://github.com/FiloSottile/age/issues/83) |
| **ktruong.dev** | Breaking and Fixing Content-Defined Chunking — fingerprinting attacks on CDC backup systems | [blog.ktruong.dev/breaking-cdc](https://blog.ktruong.dev/breaking-cdc/) |
| **restic.net** | Introducing Content Defined Chunking — how CDC enables deduplication in restic | [restic.net/blog/2015-09-12/restic-foundation1-cdc](https://restic.net/blog/2015-09-12/restic-foundation1-cdc/) |
| **FastCDC — USENIX ATC 2016** | FastCDC: A Fast and Efficient Content-Defined Chunking Approach — algorithm details | [usenix.org/system/files/conference/atc16/atc16-paper-xia.pdf](https://www.usenix.org/system/files/conference/atc16/atc16-paper-xia.pdf) |
| **PETs 2019 — PURBs (petsymposium mirror)** | Open-access mirror of the canonical PURBs paper | [petsymposium.org/popets/2019/popets-2019-0056.pdf](https://petsymposium.org/popets/2019/popets-2019-0056.pdf) |
| **Springer — Obfuscation Padding Schemes** | Minimising Rényi Min-Entropy leakage via padding — theoretical foundations | [link.springer.com/chapter/10.1007/978-981-99-7032-2_5](https://link.springer.com/chapter/10.1007/978-981-99-7032-2_5) |
| **CryFS — How it works** | Configurable block size at vault creation — precedent for per-vault chunk size | [cryfs.org/howitworks](https://www.cryfs.org/howitworks) |
| **Cryptomator — Vault Cryptography** | Fixed 32 KiB chunk size design — comparison point for chunk size choices | [docs.cryptomator.org/security/vault](https://docs.cryptomator.org/security/vault/) |
| **Azure Storage — Client-Side Encryption v2** | Configurable chunk size (16 bytes to 1 GiB) in v2.1 — precedent for flexible chunk sizing | [learn.microsoft.com — client-side encryption](https://learn.microsoft.com/en-us/azure/storage/blobs/client-side-encryption) |
| **Borg — chunker parameter discussion** | Borg exposes chunk size as a repository creation parameter — prior art for per-vault configuration | [github.com/borgbackup/borg/issues/1085](https://github.com/borgbackup/borg/issues/1085) |

---

*This is a living document. Add implementation findings and empirical overhead measurements as they emerge.*
