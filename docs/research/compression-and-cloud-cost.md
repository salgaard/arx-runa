# Arx Runa: Compression Feasibility and Cloud Storage Cost

> **Document type**: Exploration / feasibility research
> **Status**: Living document
> **Last updated**: 2026-04-06

This document researches whether adding data compression to Arx Runa's encrypt-and-upload pipeline would be beneficial, and analyses the cloud storage cost implications of Arx Runa's current fixed-size chunking and zero-padding design.

---

## Table of Contents

1. [Compression and Encryption: The Ordering Rule](#compression-and-encryption-the-ordering-rule)
2. [Does Encryption Compress Data?](#does-encryption-compress-data)
3. [Why Image and Video Formats Are Already Compressed](#why-image-and-video-formats-are-already-compressed)
4. [CRIME/BREACH: Do Compression Oracle Attacks Apply?](#crimebreach-do-compression-oracle-attacks-apply)
5. [The Fixed-Size Padding Problem](#the-fixed-size-padding-problem)
6. [File Type Reality: Who Actually Benefits?](#file-type-reality-who-actually-benefits)
7. [The Blob-Count Privacy Leak](#the-blob-count-privacy-leak)
8. [Architecture Options Evaluated](#architecture-options-evaluated)
9. [Bin-Packing: Storing Multiple Files Per Chunk](#bin-packing-storing-multiple-files-per-chunk)
10. [Decision: Position 1 — No Compression](#decision-position-1--no-compression)
11. [Cloud Storage Cost Analysis](#cloud-storage-cost-analysis)
12. [Real Cost Drivers in Arx Runa](#real-cost-drivers-in-arx-runa)
13. [Padding Overhead: Real Numbers](#padding-overhead-real-numbers)
14. [Cloud Provider Comparison](#cloud-provider-comparison)
15. [Recommendation](#recommendation)
16. [Decisions](#decisions)
17. [Open Questions](#open-questions)
18. [Sources](#sources)

---

## Compression and Encryption: The Ordering Rule

Compression and encryption are fundamentally opposed operations:

| Goal | Compression | Encryption |
|---|---|---|
| What it does | Removes patterns and redundancy | Maximises apparent randomness |
| Output entropy | Lower than input | As high as possible |
| Compressible after? | — | No — output is indistinguishable from random bytes |

If compression is to provide any benefit, it **must** happen before encryption. Compressing after encryption yields no savings — and may slightly expand the data due to compression headers added to already-incompressible input.

This ordering rule is unambiguous and universally agreed upon in the literature. The interesting question is not whether to compress before encryption, but **whether Arx Runa's specific design makes it beneficial or safe to do so**.

---

## Does Encryption Compress Data?

No. A common misconception is that encryption "shrinks" files. It does not.

XChaCha20-Poly1305, as used in Arx Runa, adds a fixed overhead of **40 bytes per chunk**:
- 24-byte nonce
- 16-byte Poly1305 authentication tag

For a 4 MiB chunk, this represents 0.001% overhead — completely negligible for cost purposes. The ciphertext is not smaller than the plaintext; it is the same size plus 40 bytes, and it is incompressible.

**Corollary**: files stored in an Arx Runa vault cannot be further compressed by the cloud provider or any downstream system. The cloud stores exactly what Arx Runa uploads.

---

## Why Image and Video Formats Are Already Compressed

This is not an assumption — it is specified by the formats themselves. Compression is applied by the camera hardware or encoder **before the file is written to storage**. By the time Arx Runa receives a file, the entropy has already been maximised.

### HEIC (iPhone default since iOS 11)

When an iPhone shutter is pressed, the image passes through this pipeline in hardware before it hits storage:

```
Sensor data
  → Demosaicing (raw sensor → RGB pixels)
  → Colour space conversion (RGB → YCbCr: separates brightness from colour)
  → Chroma subsampling (colour resolution halved — human eyes resolve brightness better than colour)
  → HEVC intra-frame encoding:
      → Block partitioning (image split into variable-size prediction blocks)
      → Intra prediction (neighbouring block values used to predict current block; only the error residual is stored)
      → Transform coding (residuals converted to frequency domain — fine spatial detail becomes near-zero coefficients)
      → Quantization (high-frequency coefficients discarded — this is the lossy step)
      → CABAC entropy coding (Context-Based Adaptive Binary Arithmetic Coding — near the theoretical entropy limit)
  → Written as .HEIC file
```

**CABAC** is the critical step for compression purposes. It is a mathematically near-optimal entropy coder that eliminates almost all remaining statistical redundancy in the byte stream. After CABAC, the output looks statistically random — no general-purpose compressor can find patterns to exploit.

HEIC achieves approximately **half the file size of equivalent-quality JPEG**, which is why Apple switched in iOS 11. A typical iPhone HEIC photo is 1.5–4 MB.

### JPEG

The same general structure applies, with older technology:

1. **DCT** (Discrete Cosine Transform) on 8×8 pixel blocks converts spatial data to frequency coefficients
2. **Quantization** discards high-frequency detail (lossy step)
3. **Huffman entropy coding** compresses the quantized coefficients

Huffman coding is less efficient than CABAC but still produces near-incompressible output. JPEG files are fully entropy-coded when written to disk.

### MP4 / MOV video (H.264 / H.265)

Video codecs apply all the same techniques as image codecs, plus **inter-frame prediction**: consecutive frames are compared, and only the pixel-level differences between frames are stored. A video of someone speaking stores one full reference frame, then tiny motion patches where the mouth moved. The residuals are DCT-transformed, quantized, and CABAC-coded. The resulting byte stream is near-maximum entropy.

### Why zstd fails on these formats

Compression algorithms like zstd work by finding repeating byte patterns and replacing them with shorter codes. A HEIC, JPEG, or MP4 file has already had all repeating patterns eliminated by entropy coding. zstd scans the file, finds nothing to reference, and returns the data essentially unchanged — sometimes adding a few bytes for the compression frame header it wrote before giving up.

This behaviour is not specific to media files. **Any entropy-coded output is incompressible**: ZIP archives, encrypted files, MP3/AAC audio, compiled binaries, and HTTPS traffic all behave the same way for the same reason.

---

## CRIME/BREACH: Do Compression Oracle Attacks Apply?

**No** — but the reasoning matters.

CRIME (CVE-2012-4929) and BREACH are compression oracle attacks that work by:
1. Injecting attacker-controlled data into the **same compression context** as a secret
2. Observing the **output size** after compression + encryption
3. Inferring when injected data matches the secret (size decreases slightly)

This requires the attacker to mix chosen plaintext with a secret in a single compressed+encrypted stream, and to observe sizes. Both conditions are absent in Arx Runa:

- Each file is compressed and encrypted independently — no attacker input is mixed into the stream
- All blobs are fixed-size (4 MiB + 40 bytes) — an attacker observing the cloud sees no size variation

Classical compression oracle attacks are not applicable to offline file-at-rest storage with fixed-size blobs.

There is, however, a subtler concern: if compression changes the **number of blobs** uploaded for a file, this becomes a weak side-channel. See [The Blob-Count Privacy Leak](#the-blob-count-privacy-leak).

---

## The Fixed-Size Padding Problem

Arx Runa's current pipeline:

```
file → split into 4 MiB segments → zero-pad last segment to 4 MiB → encrypt → upload
```

Every blob on the cloud is exactly **4 MiB + 40 bytes**. This is intentional: the cloud learns nothing about individual file sizes from blob dimensions.

If compression is added, there are three architectural options:

### Option A — Compress per-chunk, pad back to 4 MiB

```
file → split → compress chunk → pad to 4 MiB → encrypt → upload
```

A 4 MiB chunk compresses to 1.5 MiB, then is zero-padded back to 4 MiB before encryption. The cloud receives an identical 4 MiB + 40 byte blob.

**Result**: zero storage or bandwidth savings. Pure CPU overhead. This option is pointless.

### Option B — Compress whole file before chunking

```
file → compress → split into 4 MiB chunks → pad last chunk → encrypt → upload
```

A 100 MiB text file compresses to ~25 MiB → 7 chunks instead of 25. Real storage and bandwidth savings for compressible content.

**Privacy concern**: the cloud observes that 7 blobs were added rather than 25. Since blob size is fixed, the blob count directly encodes the compressed size of the file. This reveals content compressibility class — text/source code (compressible) vs photos/videos (incompressible) — without revealing content. See [The Blob-Count Privacy Leak](#the-blob-count-privacy-leak).

**Practical impact for media files**: since JPEG, HEIC, and MP4 files compress to ~0%, Option B produces exactly the same blob count as no compression for those file types. The blob-count leak is only relevant for genuinely compressible content.

### Option C — Compress per-chunk, variable-size blobs (no padding)

Compress each chunk, upload blobs of varying sizes. Breaks the fixed-size blob property entirely. Blob sizes now reveal compression ratios directly, enabling content fingerprinting. This is the worst option for Arx Runa's privacy model and is rejected outright.

---

## File Type Reality: Who Actually Benefits?

The benefit of compression depends entirely on the content stored. Personal cloud storage is dominated by files that are already compressed at the codec or format level — and therefore incompressible.

| File type | Examples | Why | Compression benefit |
|---|---|---|---|
| iPhone photos | HEIC | HEVC + CABAC entropy coding | ~0% |
| JPEG photos | .jpg | DCT + Huffman entropy coding | ~0% |
| Videos | MP4, MOV, MKV | H.264/H.265 + inter-frame prediction + CABAC | ~0% |
| Office documents | DOCX, XLSX, PPTX | ZIP containers internally | 0–5% |
| PDFs (text-heavy) | Reports, contracts | Uncompressed text content | 20–50% |
| Plain text | .txt, .md, .csv, .json | No entropy coding applied | 60–80% |
| Source code | .rs, .py, .js, .ts | No entropy coding applied | 60–80% |
| RAW camera images | CR2, NEF, ARW | Unprocessed sensor data — not entropy-coded | 30–60% |
| Archives | ZIP, RAR, 7z | Already compressed | 0% (may expand) |

**Conclusion**: for a media-centric vault (photos + videos), compression provides essentially zero storage benefit because the dominant file formats already apply near-optimal entropy coding. For a developer or professional vault (source code, documents, text logs), savings of 50–80% on compressible content are real.

---

## The Blob-Count Privacy Leak

Arx Runa's fixed-size chunk design eliminates **intra-chunk size** as a metadata side-channel. However, Option B (compress whole file before chunking) reintroduces a **per-file blob count** signal.

An adversary with access to the cloud storage (which is explicitly in Arx Runa's threat model — the cloud provider is untrusted) can observe:

- How many blobs are present in the vault over time
- How many blobs are added or removed during each sync operation

With compression enabled (Option B), the blob count for a file becomes a function of its compressed size:

```
blobs_added = ceil(compressed_size / 4 MiB)
```

This encodes:
- **Content compressibility class**: 7 blobs for a 100 MiB file → highly compressible → text or source code. 25 blobs for the same file → incompressible → photo or video.
- **Approximate original size range**: blob count × 4 MiB / typical compression ratio gives an order-of-magnitude estimate.

This is a weaker leak than revealing exact file sizes, but it is real metadata that Arx Runa's current design does not leak at all. The 2019 PETs paper on PURBs (Padded Uniform Random Blobs) formally characterises exactly this class of metadata leakage from encrypted file sizes and counts.

**Without compression**, the cloud learns nothing beyond the total blob count in the vault — it cannot distinguish a 100 MiB text file from a 100 MiB video file. **With compression (Option B)**, it can.

**Note for media-only vaults**: since HEIC/JPEG/MP4 compress to ~0%, enabling Option B on a photo/video vault produces the same blob counts as no compression. The privacy leak is only triggered by genuinely compressible content.

---

## Architecture Options Evaluated

| Option | Storage savings | CPU cost | Privacy impact | Verdict |
|---|---|---|---|---|
| No compression (current) | None | None | None | **Consistent with threat model** |
| Compress per-chunk + pad (Option A) | None | Wasted | None | Pointless |
| Compress whole file before chunking (Option B) | Real, for compressible files | Low (zstd level 3) | Weak blob-count leak | Trade-off; see below |
| Variable-size blobs (Option C) | Real | Low | Breaks privacy model | Rejected |
| Compress manifest backup only | Small (manifest is tiny) | Negligible | None | Safe future improvement |

**Cryptomator**, the most comparable open-source competitor, does not compress either. Community requests for compression have been open for years without implementation, likely because of the complexity and the privacy trade-off.

---

## Bin-Packing: Storing Multiple Files Per Chunk

A natural question arising from the padding overhead analysis: if a 2.5 MB photo wastes 1.6 MiB of padding in its lone chunk, could multiple small files share a single chunk to eliminate that waste?

### How it would work

```
Chunk N: [file_A: 1.2 MiB | file_B: 0.8 MiB | file_C: 1.7 MiB | padding: 0.3 MiB]
```

The manifest would need to track byte offsets within each chunk in addition to the chunk UUID — a schema change, but not a large one. The cloud would still receive fixed-size 4 MiB blobs, preserving the privacy property.

### The update and delete problem

Bin-packing is straightforward for write-once content. For a mutable vault it introduces **write amplification**:

- **Delete file_B from chunk N**: decrypt chunk N, remove file_B's bytes, repack remaining files with new padding, re-encrypt, upload new blob, delete old blob. One small file deletion triggers a full 4 MiB re-encryption and two cloud operations.
- **Update file_A**: same — the entire chunk must be re-encrypted and re-uploaded.
- **Multi-chunk files**: files larger than 4 MiB cannot be bin-packed at all and are unaffected.

Write amplification is the same reason most databases avoid packing arbitrary data into pages. It becomes especially costly over Rclone to a remote cloud backend where each re-upload consumes bandwidth.

### Privacy implications

Bin-packing preserves the fixed-size blob property — the cloud still sees only uniform 4 MiB blobs. However, the manifest must record file-to-chunk mappings at byte-offset granularity. The manifest is stored in SQLCipher (encrypted), so this is not a cloud-visible leak. Privacy impact is neutral.

### Practical alternative

The same storage efficiency can be achieved today without implementation cost: users with many small files can archive them (tar, ZIP) before storing in Arx Runa. The archive is a single vault entry — one or a few full 4 MiB chunks, minimal padding waste, no write amplification, no schema changes. The trade-off is that individual files within the archive cannot be retrieved without decrypting and unpacking the whole archive.

### Summary

| | Bin-packing | Archive before storing |
|---|---|---|
| Storage efficiency | High | High |
| Delete performance | Expensive (re-encrypt chunk) | Must re-archive and re-upload |
| Update performance | Expensive | Must re-archive and re-upload |
| Implementation complexity | Significant (schema + packing logic) | None (user responsibility) |
| Privacy | Neutral (fixed-size blobs preserved) | Neutral |

Bin-packing is a viable future feature for read-heavy, rarely-mutated content (e.g., archival photo storage). It is not recommended for general-purpose mutable vault content.

---

## Decision: Position 1 — No Compression

**Arx Runa will not add compression to the file encryption pipeline.**

### Rationale

1. **The dominant file type gets no benefit.** Photos and videos are already entropy-coded at the format level (HEVC/CABAC for HEIC, DCT/Huffman for JPEG, H.264/H.265 for video). There is no redundancy left for a general-purpose compressor to exploit. Adding compression for ~0% savings on the majority of vault content is not justified.

2. **Compression before chunking undermines the fixed-size blob guarantee.** The blob count becomes a function of content compressibility, leaking content-type metadata to the cloud provider. This is inconsistent with Arx Runa's paranoid threat model, which treats the cloud as untrusted.

3. **The design is internally consistent.** The fixed-size chunk + zero-padding scheme was chosen specifically to prevent file size inference. Adding compression before chunking partially reverts this decision.

4. **Users who need compression can compress first.** A user storing a text archive can ZIP or tar.gz it before placing it in the vault. Arx Runa stores the compressed archive without any privacy regression — the blob count reflects the compressed size, which was the user's deliberate choice.

### Future consideration

An opt-in vault-level compression flag (Option B) could be offered as a **documented trade-off feature** in a future release, clearly labelled as trading some metadata privacy for storage cost savings. This is appropriate for developer vaults with text-heavy content where the user explicitly accepts the trade-off. It must not be the default.

Compressing the **manifest backup** (a single small blob — see `cloud-synchronisation` design) is a safe, zero-privacy-cost improvement worth implementing separately, as previously noted in that design document.

---

## Cloud Storage Cost Analysis

The 40-byte per-chunk crypto overhead is negligible. The real cost drivers are:

### 1. Zero-padding overhead (dominant for small files)

Every file occupies at least one full 4 MiB chunk regardless of actual size. The last chunk of every file is zero-padded. For large files this waste is negligible; for small files it dominates.

### 2. No compression (secondary, text-heavy vaults only)

For photo/video vaults: no additional cost vs a hypothetical compressed alternative, because HEIC/JPEG/MP4 are already at their entropy limit.

For developer/document vaults: uncompressed text files use 3–5× more cloud storage than they would with compression. At $0.006/GB/month (Backblaze B2), 100 GB of source code that could compress to 25 GB costs an extra $0.45/month — modest in absolute terms.

---

## Padding Overhead: Real Numbers

**Assumptions**: chunk size = 4 MiB = 4,194,304 bytes. Each file's last chunk is zero-padded to 4 MiB. Crypto overhead (40 bytes/chunk) omitted — it is negligible.

### Per-file overhead

| File type | Typical size | Chunks | Storage used | Padding wasted | Overhead |
|---|---|---|---|---|---|
| iPhone HEIC photo | 2.5 MB | 1 | 4 MiB | 1.62 MiB | **68%** |
| Android JPEG photo | 5 MB | 2 | 8 MiB | 3.24 MiB | **65%** |
| DSLR JPEG photo | 12 MB | 3 | 12 MiB | 0.43 MiB | **3.5%** |
| RAW image (CR2) | 25 MB | 7 | 28 MiB | 2.96 MiB | **12%** |
| 1-min 1080p video | 150 MB | 36 | 144 MiB | 0.85 MiB | **0.6%** |
| 10-min 4K video | 1.5 GB | 366 | 1,464 MiB | 1.24 MiB | **~0%** |
| Small document | 50 KB | 1 | 4 MiB | 3.95 MiB | **99%** |

The pattern: padding overhead is worst for files smaller than one chunk (< 4 MiB). For large video files, it is completely negligible — a 1.5 GB video wastes under 2 MiB regardless of its exact size.

### Concrete vault estimates

**Vault A — 10,000 iPhone HEIC photos (avg 2.5 MB each)**

| | Value |
|---|---|
| Actual content | 25 GB |
| Chunks needed | 10,000 × 1 = 10,000 |
| Cloud storage used | 10,000 × 4 MiB = **40 GiB** |
| Padding overhead | **+60%** |
| Cost on Backblaze B2 | $0.24/month |
| Cost on Cloudflare R2 | $0.60/month |

**Vault B — 500 videos (avg 300 MB each)**

| | Value |
|---|---|
| Actual content | 150 GB |
| Chunks per video | ceil(300 MB / 4 MiB) = 72 |
| Cloud storage used | 500 × 72 × 4 MiB ≈ **~144 GiB** |
| Padding overhead | **~4%** |
| Cost on Backblaze B2 | $0.87/month |
| Cost on Cloudflare R2 | $2.16/month |

**Vault C — Mixed: 5,000 photos (2.5 MB) + 100 videos (300 MB)**

| | Value |
|---|---|
| Actual content | ~12.5 GB photos + 30 GB video = 42.5 GB |
| Cloud storage used | ~20 GiB photos + ~28.5 GiB video = **~48.5 GiB** |
| Blended overhead | **~14%** |
| Cost on Backblaze B2 | $0.29/month |
| Cost on Cloudflare R2 | $0.73/month |

**Key insight**: for video-heavy vaults, padding overhead is negligible and cloud costs are low. For photo-only vaults with many small HEIC files, overhead reaches 60%+ — though the absolute cost on B2 is still well under $1/month for typical vault sizes.

---

## Cloud Provider Comparison

Arx Runa's BYOC model via Rclone supports all major cloud backends. Cost varies significantly by provider.

### Object storage (S3-compatible) — pay per GB

| Provider | Storage/GB/month | Egress/GB | Free tier | Best for |
|---|---|---|---|---|
| Backblaze B2 | **$0.006** | $0.01 ($0 via Cloudflare CDN) | 10 GB | Storage-heavy, cost-sensitive |
| Cloudflare R2 | $0.015 | **$0** | 10 GB | High-download, egress-sensitive |
| Wasabi | $0.0068 | $0 | None | Flat-rate alternative |
| AWS S3 | $0.023 | $0.09 | 5 GB (12 months) | Enterprise / existing AWS users |

### Consumer cloud (subscription) — pay per tier

| Provider | Free tier | 100 GB | 1–2 TB | Notes |
|---|---|---|---|---|
| Google Drive | **15 GB** | $2.99/month | $9.99/month (2 TB) | Shared with Gmail/Photos |
| OneDrive | 5 GB | $1.99/month | $6.99/month (1 TB, with Microsoft 365) | Bundled value |
| Dropbox | 2 GB | — | $9.99/month (2 TB) | Limited free tier |

**Key insight**: for pure cloud cost minimisation, Backblaze B2 (with Cloudflare CDN for free egress) is the cheapest at well under $1/month for typical personal vaults. Consumer plans (Google Drive, OneDrive) cost more per GB but include large storage tiers that make them attractive if the user already pays for them.

---

## Recommendation

Arx Runa will not add compression to the file encryption pipeline. The dominant file types stored in personal vaults (photos, video) are already entropy-coded at the format level — compression before chunking yields zero savings while leaking content-type metadata to the cloud via blob count. The fixed-size blob guarantee must be preserved. See [Decision: Position 1 — No Compression](#decision-position-1--no-compression) for the full rationale.

For text-heavy developer vaults, an **opt-in vault-level compression flag** is a viable future release feature, clearly documented as trading metadata privacy for storage savings. It must not be the default.

---

## Decisions

> Choices made during this research session. Updated as the session progresses.

| Decision | Alternatives considered | Rationale |
|---|---|---|
| **No compression in the file encryption pipeline** | Compress per-chunk + re-pad to fixed size (Option A), compress whole file before chunking (Option B), variable-size blobs (Option C) | Dominant file types are at their entropy limit; Option B leaks content-type via blob count; Option C breaks the fixed-size blob guarantee entirely |
| **Variable-size blobs (Option C) rejected outright** | Option A, Option B, no compression | Directly exposes compression ratios as blob sizes — the worst possible privacy regression |
| **Opt-in compression flag deferred to a future release, not default** | Always-on compression, never allow it | Users with text-heavy vaults should be able to accept the documented metadata privacy trade-off; must not affect media vaults |

---

## Open Questions

1. **Small file warning**: should Arx Runa display an estimated cloud storage size (including padding overhead) before upload, so users with many small files can decide to archive them first?

2. **Manifest compression**: the cloud-sync design deferred compressing the manifest backup with zstd. This is safe and should be revisited — the manifest is all structured text and compresses very well.

3. **Opt-in compression flag**: if implemented in a future release, how should the privacy trade-off be communicated to the user? A vault creation option labelled "Reduce storage (trades some metadata privacy)" seems appropriate.

4. **Bin-packing for archival vaults**: for read-heavy, rarely-mutated content (photo archives, document archives), bin-packing multiple small files into single chunks would eliminate the 60%+ photo overhead. Worth designing as an opt-in vault mode.

5. **Cost estimator UI**: a simple "estimated monthly cost" display based on configured cloud backend and current vault size would help users choose a provider and understand the padding overhead.

6. **Storj / decentralised backends**: some rclone backends (Storj, Filebase) offer free tiers and decentralised storage. Worth evaluating for cost and privacy properties.

---

## Sources

| Source | Topic | URL |
|---|---|---|
| **Kelsey (FSE 2002)** | Compression and Information Leakage of Plaintext — foundational analysis of compression side-channel leakage | [IACR archive (FSE 2002)](https://www.iacr.org/archive/fse2002/23650264/23650264.pdf) |
| **RFC 9325 — Recommendations for Secure Use of TLS/DTLS** | Current best-practice guidance for TLS, including historical attack classes and conservative configuration guidance | https://www.rfc-editor.org/rfc/rfc9325 |
| **BREACH Attack Site (Rizzo & Duong)** | Primary timeline and technical summaries for CRIME and BREACH compression side-channel attacks | [breachattack.com](https://breachattack.com/) |
| **PETs 2019 — Reducing Metadata Leakage with PURBs** | Formal treatment of size/count metadata leakage from encrypted files | [petsymposium.org/popets/2019/popets-2019-0056.pdf](https://petsymposium.org/popets/2019/popets-2019-0056.pdf) |
| **Cornell — MiniCrypt** | Reconciling encryption and compression for big data stores | [cs.cornell.edu/~ragarwal/pubs/minicrypt.pdf](https://www.cs.cornell.edu/~ragarwal/pubs/minicrypt.pdf) |
| **Cryptomator — GitHub Discussion #2295** | Community request for compression; not implemented | [github.com/cryptomator/cryptomator/discussions/2295](https://github.com/cryptomator/cryptomator/discussions/2295) |
| **RFC 8878 — Zstandard Compression and the `application/zstd` Media Type** | Normative zstd format definition and framing details | https://www.rfc-editor.org/rfc/rfc8878 |
| **Nokia Tech — HEIF Technical** | HEIF format specification and compression internals | [nokiatech.github.io/heif/technical.html](https://nokiatech.github.io/heif/technical.html) |
| **ITU-T T.81 (JPEG standard text mirror)** | JPEG baseline coding process (DCT, quantization, entropy coding) | https://www.w3.org/Graphics/JPEG/itu-t81.pdf |
| **ITU-T H.265 / HEVC** | HEVC coding tools and standard evolution used by HEIF ecosystems | https://www.itu.int/rec/T-REC-H.265 |
| **Backblaze** | B2 storage pricing vs AWS S3, GCP, Azure | [backblaze.com/cloud-storage/pricing](https://www.backblaze.com/cloud-storage/pricing) |
| **Cloudflare R2 documentation** | Official Cloudflare R2 storage and operation pricing | https://developers.cloudflare.com/r2/pricing/ |
| **Dropbox plans** | Official Dropbox storage plan pricing | https://www.dropbox.com/plans |
| **Google One plans** | Official Google Drive consumer storage plan pricing | https://one.google.com/about/plans |
| **Microsoft OneDrive plans** | Official OneDrive storage plan pricing | https://www.microsoft.com/en-us/microsoft-365/onedrive/compare-onedrive-plans |


