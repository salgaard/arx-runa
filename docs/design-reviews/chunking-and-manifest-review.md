# Arx Runa: Chunking and Manifest — Critical Review

> **Document type**: Exploration / feasibility research
> **Status**: Concluded
> **Last updated**: 2026-04-08

Critical review of `docs/architecture/designs/chunking-and-manifest/design.md` against
academic literature, production systems, and implementation correctness.
Each design decision is re-examined for correctness, completeness, and
missed opportunities.

For the canonical design, see `docs/architecture/designs/chunking-and-manifest/design.md`.

---

## Table of Contents

1. [The Problem](#the-problem)
2. [MetadataStore Async Trait Object Safety](#metadatastore-async-trait-object-safety)
3. [EXIF Stripping and the Streaming Claim](#exif-stripping-and-the-streaming-claim)
4. [Schema Integrity Constraints](#schema-integrity-constraints)
5. [ChunkRecord: Transient vs Persistent Fields](#chunkrecord-transient-vs-persistent-fields)
6. [received_shares.chunk_uuids: Unstructured Field](#received_shareschunk_uuids-unstructured-field)
7. [File Rename and Move Operations](#file-rename-and-move-operations)
8. [size_padded: Always-Constant Column](#size_padded-always-constant-column)
9. [Upload Order Randomisation](#upload-order-randomisation)
10. [Recommendation](#recommendation)
11. [Decisions](#decisions)
12. [Open Questions](#open-questions)
13. [Sources](#sources)

---

## The Problem

Phase 3 of Arx Runa implements the local half of the storage layer: files are split into fixed-size chunks, each chunk is encrypted and written to a staging directory, and a SQLCipher manifest database records the mapping between virtual filesystem entries and encrypted blobs. This design review examines whether that specification is correct, complete, and free of implementation traps before the code is written.

The review covers eight topics: one async trait correctness issue analogous to the DeviceMonitor bug found in the auth review, a correctness problem with the EXIF stripping streaming claim, two schema integrity gaps, a struct design inconsistency in `ChunkRecord`, an unstructured field in `received_shares`, a missing file-mutation operation, and a low-value column in the chunks schema.

---

## MetadataStore Async Trait Object Safety

The design defines `MetadataStore` with `async fn` methods and specifies two concrete implementations — `SqlCipherMetadataStore` for production and `MockMetadataStore` for testing. The `Send + Sync` supertrait bounds and the test/production duality make runtime dispatch the clearly intended pattern: a call site should hold a `Box<dyn MetadataStore>` and switch implementations without changing the surrounding code.

**The problem**: `async fn` in a trait desugars to a return-position `impl Trait` (RPITIT), where the concrete future type is opaque and only known at monomorphisation time. Any trait with an opaque return type is not dyn-compatible. The vtable cannot hold a function pointer for a type whose size is unknown at compile time. This limitation was present before Rust 1.75 and was not resolved by the 1.75 stabilisation of `async fn` in traits — that stabilisation enabled `async fn` in trait definitions for generic contexts, but did not make them dyn-safe. Attempting to write `Box<dyn MetadataStore>` with the trait as specified produces:

```
error[E0038]: the trait `MetadataStore` cannot be made into an object
```

This is the same issue found in the `DeviceMonitor` trait during the authentication design review (see `docs/research/authentication-and-session-management-review.md`).

**Alternatives evaluated:**

| Pattern | dyn-safe | Ergonomics | Notes |
|---------|----------|------------|-------|
| `#[async_trait]` macro | Yes | Add one attribute to trait and each impl | Transforms `async fn` → `fn → Pin<Box<dyn Future + Send>>` at compile time; standard Rust ecosystem pattern |
| Explicit boxed futures | Yes | Verbose — each of the 8 methods requires manual `Pin<Box<dyn Future<Output=...> + Send + '_>>` return | Equivalent result to `#[async_trait]`, no new dependency, but significantly harder to read |
| Generic `<S: MetadataStore>` at call site | No dyn | Propagates the generic through every containing struct | Avoids the issue structurally but complicates the entire storage module for what is fundamentally a test seam |
| Associated future types | Yes | Each impl must name its concrete future types | Works but requires a separate associated type per method — extremely verbose for 8 methods |

The `DeviceMonitor` fix used explicit boxed returns because the trait had a single method returning a stream — boxing at the return was clean. `MetadataStore` has eight methods; explicit boxing on each is tedious and obscures intent. The `#[async_trait]` macro produces a trait definition that looks exactly like the original, requires only the addition of an attribute, and generates the correct boxed-future code at compile time.

The `async-trait` crate is actively maintained, widely used in the Rust ecosystem, and contains no `unsafe` code. It requires a one-line addition to `src-tauri/Cargo.toml`:

```toml
async-trait = "0.1"
```

The fix on the trait definition and both `impl` blocks:

```rust
use async_trait::async_trait;

#[async_trait]
trait MetadataStore: Send + Sync {
    async fn insert_node(&self, node: &Node) -> Result<(), StorageError>;
    async fn insert_chunks(&self, chunks: &[ChunkRecord]) -> Result<(), StorageError>;
    // ... all other methods unchanged
}

#[async_trait]
impl MetadataStore for SqlCipherMetadataStore { ... }

#[async_trait]
impl MetadataStore for MockMetadataStore { ... }
```

**Status: Fixed. `#[async_trait]` added to `MetadataStore` trait and both `impl` blocks in the design. `async-trait = "0.1"` noted as a required dependency.**

---

## EXIF Stripping and the Streaming Claim

The design includes an optional pre-encryption step that strips EXIF, XMP, and IPTC metadata from media files before they enter the encrypt pipeline. The purpose is **export privacy**: because stripping happens before encryption, the blob stored in the cloud is the metadata-free version, and any file later exported or shared from Arx Runa will not carry GPS coordinates, timestamps, or camera identifiers. This is distinct from protecting the cloud from seeing EXIF — encryption already handles that. The feature protects users from accidentally re-exposing metadata when they take a file out of the system.

The design lists four supported MIME types: `image/jpeg`, `image/png`, `image/tiff`, and `video/mp4` / `video/quicktime`. For the image types, stripping is streaming-compatible: JPEG EXIF lives in the APP1 segment near the start of the file, PNG eXIf chunks appear in the header region, and TIFF metadata is in the leading IFD structure. All are reachable within the first read buffer. For MP4 and QuickTime, the situation is fundamentally different.

**The moov atom placement problem**: In the ISO 14496-12 base media file format (the container used by both MP4 and QuickTime), all file-level metadata — including GPS coordinates, location tags, and camera information — lives in the `moov` atom. The standard does not mandate where `moov` appears in the byte stream. During real-time recording on Android (MediaRecorder API) and iOS, the encoder writes compressed media samples sequentially into the `mdat` atom and finalises `moov` only when recording stops. This produces the layout `[ftyp][mdat][moov]` — moov at the end of the file. The widely-known "fast start" optimisation (`ffmpeg -movflags +faststart`) relocates moov to the front for HTTP streaming, but files recorded directly to device storage almost universally have moov at the end. Reading only the first 4 MiB buffer of a 1 GiB device recording reads only `ftyp` and the opening bytes of `mdat` — moov, and any GPS metadata within it, is never reached.

**What this means in practice**: Stripping as specified would silently do nothing on typical smartphone recordings. The file would pass through the encrypt pipeline with GPS coordinates intact. There is no breach — the GPS data is encrypted — but the export privacy guarantee ("the exported copy will not contain EXIF metadata") would not hold for video files.

**Alternatives evaluated:**

| Approach | Streaming? | Correctness | Notes |
|----------|-----------|-------------|-------|
| Read full video into RAM, parse + rewrite moov | No | Correct for all valid MP4s | A 4K video file can be several GiB — incompatible with the streaming invariant |
| Two-pass: seek to moov offset, patch in place | Partial | Correct | Requires seekable source, two I/O passes; significant complexity |
| `mp4` crate for moov rewriting | No | Correct | Pure-Rust option but still requires full file read |
| Exclude MP4/QuickTime; document limitation | Yes | Partial — image types still stripped | Honest about the limitation; video stripping deferred to a future non-streaming step |

The EXIF stripping feature is an optional privacy enhancement scoped to Phase 3 or Phase 6. Its core value is eliminating GPS metadata from exported images. Deferring video stripping avoids breaking the streaming invariant and avoids silently failing (which is worse than not attempting). Users who need GPS stripped from video can use an external tool before upload.

**Verdict: MP4 and QuickTime removed from the supported EXIF stripping targets. JPEG, PNG, and TIFF remain. The limitation is documented explicitly in the design. Video metadata stripping is an open question for a future non-streaming pipeline.**

---

## Schema Integrity Constraints

The `nodes` table has two invariants described in prose — valid `node_type` values and the `file_key_wrapped` nullability rule — but neither is enforced by the DDL. Without schema-level enforcement, corrupt rows can enter the database silently: a bug in a calling function, a partially-written migration, or a test that inserts a minimal fixture can produce a file row with `file_key_wrapped = NULL` or a node with an unrecognised type string. These are not caught until a decrypt attempt or a type-dispatch at runtime, at which point the error is distant from the insertion site and harder to diagnose.

SQLite CHECK constraints are evaluated at INSERT and UPDATE time and are zero-cost at read time. They are the correct mechanism for this invariant — no trade-off is involved.

**What the design chose (original)**: `node_type TEXT NOT NULL` with no valid-value constraint; `file_key_wrapped BLOB` with nullability described only in the Node types prose table.

**Fix**: two CHECK constraints added to the `nodes` DDL:

```sql
node_type        TEXT NOT NULL CHECK (node_type IN ('file', 'directory')),
file_key_wrapped BLOB
    CHECK ((node_type = 'file'      AND file_key_wrapped IS NOT NULL)
        OR (node_type = 'directory' AND file_key_wrapped IS NULL))
```

SQLite supports cross-column CHECK constraints within a single table, so the `file_key_wrapped` check can reference `node_type` in the same row. Any insertion of a file row missing its wrapped key, or a directory row carrying one, produces `CHECK constraint failed` at the write site — the invariant is enforced at the database level regardless of which code path performs the insert.

**Status: Fixed. Both CHECK constraints added to the `nodes` DDL in the design.**

---

## ChunkRecord: Transient vs Persistent Fields

`ChunkRecord` serves two roles in the design: the return type of `encrypt_file` (capturing the result of writing a chunk to staging) and the type returned by `MetadataStore::get_chunks` (loading chunk metadata from SQLite for decryption). Six fields are defined; five map directly to columns in the `chunks` schema. The sixth — `blob_path: PathBuf`, described as "path in staging directory" — does not appear in the schema and cannot be populated when loading from the database.

The `decrypt_file` function does not use `blob_path`. It locates blobs via `blob_directory/<blob_name>.blob` using its `blob_directory: &Path` parameter. When `get_chunks` returns records for decryption, `blob_path` will be empty on every record — a struct field that is permanently meaningless in one of the two usage contexts.

The implementation trap this creates is concrete: an implementer writing `SqlCipherMetadataStore::get_chunks` will find a field they cannot populate from the SELECT result. The natural response is `PathBuf::new()` or `PathBuf::default()`, which silently introduces a plausible-looking field with a dangerously wrong value. Any future code that reads `blob_path` from a database-loaded record will silently use an empty path.

| Option | Approach | Trade-off |
|--------|----------|-----------|
| Remove `blob_path` | Five-field struct matching the schema exactly; derive path at call site | Phase 4 upload uses `staging_dir/<blob_name>.blob` — no information lost |
| Split into `StagedChunkRecord` / `ChunkRecord` | Type-level distinction; `encrypt_file` returns the staged variant | More types; a conversion step before `insert_chunks`; cleaner semantics |

Splitting into two structs is architecturally cleaner, but the only place `blob_path` is used is the narrow window between `encrypt_file` returning and `insert_chunks` being called. Phase 4 can construct the staging path from `blob_name` at that point. Removing the field is simpler and eliminates the confusion entirely.

**Status: Fixed. `blob_path` removed from `ChunkRecord`. The staging path is derived as `staging_directory/<blob_name>.blob` at the call site.**

---

## received_shares.chunk_uuids: Unstructured Field

The `received_shares` table stores everything needed to fetch and decrypt a file shared by another user. The `chunk_uuids TEXT NOT NULL` column holds the list of blob names for the shared chunks, but the DDL specifies no format, no comment, and no constraint. The share package JSON (Phase 5 design) contains `chunk_uuids` as a JSON array, but the schema is silent on whether that format carries through to the database column. Two independently written code paths — one importing a share package, one fetching shared blobs — could disagree on the serialisation format without any compile-time or database-level signal.

**What the design chose (original)**: bare `TEXT NOT NULL` with no format specification.

| Approach | Enforcement | Notes |
|----------|-------------|-------|
| JSON array + `json_valid()` CHECK | SQLite built-in; rejects malformed JSON at INSERT/UPDATE | Consistent with share package format; `serde_json` round-trips cleanly |
| Normalised `received_share_chunks` table | Schema-level FK | More relational; complicates atomic share import and delete |
| Comment only | None | Documents intent without enforcement |

SQLite's `json_valid()` function has been available since 3.38.0 (February 2022) and is present in all SQLCipher versions the project will target. A `CHECK (json_valid(chunk_uuids))` constraint rejects any non-JSON value at write time with no read-time cost.

**Status: Fixed. `chunk_uuids` defined as a JSON array of UUID v4 blob name strings, enforced with `CHECK (json_valid(chunk_uuids))`, documented with a comment.**

---

## File Rename and Move Operations

The `MetadataStore` trait covers create, read, and delete but omits update entirely. There is no method for renaming a file or directory (changing the `name` column) or moving a node to a different parent (changing `parent_id`). Without these, the Phase 6 UI cannot expose rename or move without bypassing the `MetadataStore` abstraction and writing directly to SQLite — defeating the purpose of having a trait.

Both operations are straightforward `UPDATE` statements on the `nodes` table. The question is whether they appear as two distinct methods or a single general `update_node` with an optional-field patch struct.

| Approach | Trait surface | Notes |
|----------|--------------|-------|
| `rename_node` + `move_node` | Two focused methods | Consistent with named-operation style (`insert_node`, `delete_node`); test cases are explicit |
| `update_node(NodePatch)` | Single method | More flexible; adds a `NodePatch` type for two fields |
| Defer to Phase 6 | No change | Leaves the trait incomplete during Phase 3; Phase 6 would need to extend the abstraction boundary anyway |

The trait consistently uses named, intent-specific methods. Adding two focused methods maintains that pattern and keeps `MockMetadataStore` easy to implement and verify.

**Status: Fixed. `rename_node` and `move_node` added to the `MetadataStore` trait.**

---

## EXIF Crate Versions

The design names two Rust crates for the EXIF stripping implementation: `kamadak-exif` for parsing and `img-parts` for segment-level rewriting. Both were verified against current crates.io state.

`kamadak-exif` (0.6.1, November 2024): pure-Rust EXIF parser supporting JPEG, PNG, TIFF, HEIF, and WebP. **Read-only** — parses and exposes EXIF fields but provides no API for stripping or rewriting. This is the correct role for it in the design: detect whether EXIF is present and which fields exist.

`img-parts` (0.4.0, February 2026): low-level image container library with segment/chunk-level manipulation for JPEG, PNG, and RIFF. Supports removing APP1 (EXIF) and APP13 (IPTC) segments from JPEG, and eXIf chunk removal from PNG (PNG Third Edition 2025 format, which this version handles). Both crates are actively maintained.

The two-crate approach is correct as written: `kamadak-exif` is not a stripping library and was never intended to be; `img-parts` handles the actual byte-level rewriting for JPEG and PNG. No changes required.

**Note: No design change. Crate versions verified; both maintained; approach correct.**

---

## size_padded: Always-Constant Column

The `chunks` table includes `size_padded INTEGER NOT NULL`, described in the design as "always = chunk_size (4 MiB)". With a fixed 4 MiB chunk size, every row in the table carries the same value — the column stores no information that cannot be derived from the global constant.

The argument for keeping it is forward compatibility: the padding overhead reduction research (`padding-overhead-reduction.md`, Approach 3 — Tiered Fixed Chunk Sizes) identifies variable chunk sizes as a viable future optimisation. If Arx Runa ever supports tiered sizes (e.g. 256 KiB for small files, 4 MiB for large), `size_padded` would already be in the schema with no migration needed. The storage cost is 8 bytes per row — negligible.

The column is kept with a comment clarifying its current invariant and future intent. No structural change.

**Note: No design change. Comment added to clarify the column is forward-compatible with variable chunk sizes.**

---

## Upload Order Randomisation

The design notes upload order randomisation as an "Extension point, not blocking" in Open Decisions, without stating why it matters. This is worth documenting because the reason is non-obvious and directly relevant to the threat model.

When blobs are uploaded sequentially (chunk 0, chunk 1, chunk 2, …), a passive observer watching the cloud transport layer sees a burst of uploads with timestamps that are tightly correlated. Even though blob names are random UUIDs, the upload timestamps reveal which blobs belong to the same file: they arrive as a group, in order. For multi-file operations (a vault sync uploading many files), sequential upload also reveals chunk boundaries within files — the timing gaps between files are larger than the gaps within a file.

Fisher-Yates shuffle of the blob upload queue — already called out in the Phase 4 design — eliminates temporal correlation: all blobs from a sync operation arrive in a random order, and an observer cannot determine which blobs are chunks of the same file. This is a meaningful privacy enhancement that costs only a single shuffle pass before the upload loop.

The Phase 3 design does not need to implement upload order (that is Phase 4), but the Open Decisions entry should explain the security rationale so the Phase 4 implementer understands why it is not optional.

**Note: No structural change. Security rationale added to the Open Decisions entry for upload order randomisation.**

---

## Recommendation

The chunking and manifest design is structurally sound. The core choices — 4 MiB fixed chunks, zero-pad with `size_bytes` truncation, SQLCipher manifest, per-file `file_key_wrapped` on the `nodes` table, staging directory with transaction-backed error recovery — are all well-reasoned and consistent with the prior cryptographic and authentication designs.

Six changes were made. Two were bugs that would have caused compile failures or silent incorrect behaviour: the `MetadataStore` async trait was not dyn-safe (requiring `#[async_trait]`), and the EXIF streaming claim for MP4/QuickTime was physically incorrect (moov atom at end-of-file on device recordings). Four were gaps: schema CHECK constraints that enforce invariants at the database level rather than relying on application code; removal of a transient `blob_path` field from a struct used in both staging and database contexts; a format specification and `json_valid()` constraint on an unstructured `TEXT` column; and two missing `MetadataStore` methods needed for rename and move.

The design is ready for Phase 3 implementation with these changes applied.

| # | Finding | Severity | Resolution |
|---|---------|----------|------------|
| 1 | `MetadataStore` async trait not dyn-safe | Bug | Fixed — `#[async_trait]` added |
| 2 | EXIF streaming incorrect for MP4/QuickTime | Bug | Fixed — MP4/QuickTime removed; limitation documented |
| 3 | No schema CHECK constraints on `nodes` | Gap | Fixed — `node_type` and `file_key_wrapped` CHECKs added |
| 4 | `ChunkRecord.blob_path` transient field | Gap | Fixed — field removed; path derived at call site |
| 5 | `received_shares.chunk_uuids` unstructured | Gap | Fixed — JSON array format + `json_valid()` CHECK |
| 6 | No rename/move in `MetadataStore` | Gap | Fixed — `rename_node` and `move_node` added |
| 7 | EXIF crate versions | Note | No change — both crates current and correct |
| 8 | `size_padded` always-constant | Improvement | No change — kept with clarifying comment |
| 9 | Upload order security rationale undocumented | Note | Rationale added to Open Decisions entry |

---

## Decisions

| Decision | Alternatives considered | Rationale |
|---|---|---|
| `MetadataStore` dyn-safety fix | `#[async_trait]` macro vs explicit boxed futures vs generics | `#[async_trait]` chosen: 8 async methods make explicit boxing impractical; macro produces identical-looking trait, widely adopted |
| MP4/QuickTime EXIF stripping | Drop support vs full-file read vs two-pass seek | Dropped: moov atom at end-of-file on device recordings; streaming-incompatible. JPEG/PNG/TIFF remain. Video stripping deferred |
| Schema CHECK constraints | Enforce in DDL vs prose-only | DDL enforcement: SQLite CHECKs catch corrupt rows at write time; zero read-time cost; cross-column constraint covers file_key_wrapped nullability invariant |
| `ChunkRecord.blob_path` | Remove vs split into two structs | Removed: only used in a narrow window between encrypt_file and insert_chunks; Phase 4 derives the staging path from blob_name |
| `received_shares.chunk_uuids` format | JSON array + CHECK vs normalised table vs comment-only | JSON array with `json_valid()` CHECK: consistent with share package format, enforced at write time, no structural change |
| Rename/move operations | Two focused methods vs `update_node(NodePatch)` vs defer | Two methods chosen: consistent with named-operation style; test cases are explicit; no new types needed |

---

## Open Questions

---

## Sources

| Source | Topic | URL |
|---|---|---|
| Rust RFC 3498 — async fn in traits | dyn-safety limitation of RPITIT | https://rust-lang.github.io/rfcs/3498-async-fn-in-traits.html |
| `async-trait` crate | `#[async_trait]` macro for dyn-safe async traits | https://crates.io/crates/async-trait |
| ISO/IEC 14496-12 (MPEG-4 Part 12) | moov atom placement in MP4/QuickTime containers | https://www.iso.org/standard/83102.html |
| FFmpeg `-movflags +faststart` | moov relocation for streaming-optimised MP4 | https://ffmpeg.org/ffmpeg-formats.html |
| `kamadak-exif` crate 0.6.1 | EXIF parser (read-only) for JPEG/PNG/TIFF | https://crates.io/crates/kamadak-exif |
| `img-parts` crate 0.4.0 | JPEG/PNG segment-level rewriting for EXIF stripping | https://crates.io/crates/img-parts |
| SQLite `json_valid()` function | JSON validation in CHECK constraints (since SQLite 3.38.0) | https://www.sqlite.org/json1.html |
| SQLite CHECK constraints | Cross-column CHECK constraint support | https://www.sqlite.org/lang_createtable.html |
