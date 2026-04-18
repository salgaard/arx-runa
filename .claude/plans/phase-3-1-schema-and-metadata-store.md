---
title: "Phase 3.1 — SQLCipher Schema and MetadataStore Trait"
created: "2026-04-18T00:00:00Z"
status: approved
roadmap-phase: 3
sub-phase: "3.1"
design-document: "docs/architecture/designs/chunking-and-manifest/design.md"
sub-phase-roadmap: "docs/architecture/designs/chunking-and-manifest/sub-phases/roadmap.md"
governance-sync-required: true
tags: [storage, phase-3, sqlcipher, metadata-store, schema, manifest, pending-deletions]
---

# Plan: Phase 3.1 — SQLCipher Schema and MetadataStore Trait

## 1. Goal

Replace the Phase 2.4 stub schema with the canonical chunking-and-manifest schema and land the `MetadataStore` trait, `SqlCipherMetadataStore`, and `MockMetadataStore` implementations — establishing the manifest foundation consumed by the Phase 3.2 encrypt/decrypt pipelines.

## 2. Context

**Roadmap**: Phase 3 — Storage: Chunking and Manifest (`docs/roadmap.md` lines 64–69). Phase 3.1 is the first sub-phase; Phases 3.2 and 3.3 depend on it strictly.

**Sub-phase roadmap**: `docs/architecture/designs/chunking-and-manifest/sub-phases/roadmap.md` (implementation order 3.1 → 3.2 → 3.3). Security review **required** for Phase 3.1 (SQLCipher keying correctness, CASCADE deletion, UNIQUE constraints) per the roadmap's "Security Review Checkpoints" section (line 96). Estimated scope: ~250 lines production + ~150 lines tests.

**Sub-phase document**: `docs/architecture/designs/chunking-and-manifest/sub-phases/3.1-schema-and-metadata-store.md` (deliverables 1–8, acceptance criteria).

**Parent design sections used** (authoritative per the Contract Anchor at `design.md#contract-surface`):

- `docs/architecture/designs/chunking-and-manifest/design.md` lines 17–45 — Contract Surface (interface / data / invariant / dependency contracts — binding).
- Same file lines 119–242 — Manifest Database Schema (canonical DDL for `nodes`, `chunks`, `manifest_meta`, `pending_deletions`, `destination_sessions`, `contacts`, `shares`, `received_shares`; `file_key_wrapped` placement rationale; `UNIQUE` constraints).
- Same file lines 298–333 — Public API (`ChunkRecord` struct definition; consumed in Phase 3.1 as a shared type declaration, even though `encrypt_file` / `decrypt_file` land in Phase 3.2).
- Same file lines 372–410 — File Key Lifecycle (informs the CASCADE + `pending_deletions` transactional semantics of `delete_node`).
- Same file lines 461–527 — `MetadataStore` trait canonical method signatures (13 methods total, including `rename_node` and `move_node`).
- `docs/architecture/design-invariants.md` §4 (`chunk_size_bytes` immutability and configurable range) and §10 (durable cloud-deletion retry via `pending_deletions`). Both must be upheld by the schema + trait.

**Existing state** (branch `development`, HEAD = `39258af`):

- `src-tauri/src/storage/mod.rs` declares `pub mod cloud; pub mod error; pub mod types;` only — no `schema`, `sqlcipher`, `mock`, or `metadata_store` modules yet.
- `src-tauri/src/storage/error.rs` defines `StorageError` as an **empty** `#[non_exhaustive]` thiserror enum ("Variants added in implementation phases.").
- `src-tauri/src/storage/types/mod.rs` exports `BlobName` (`String` newtype). No `Node`, `ChunkRecord`, `NodeId` yet.
- `src-tauri/src/storage/cloud/mod.rs` hosts the Phase 2.4 forward declaration of `CloudTransport` and `CloudTransportError` (Phase 4.1/4.3 extends). Untouched by Phase 3.1.
- `src-tauri/src/auth/ceremonies/mod.rs` lines 35–46 define `VAULT_STUB_SCHEMA` — `CREATE TABLE _phase_stub`, an **integer-keyed** stub `nodes (id INTEGER PRIMARY KEY, file_key_wrapped BLOB NOT NULL)`, and `vault_identity (id, public_key, wrapped_private_key)`. Applied in `create.rs` line 131 via `conn.execute_batch(VAULT_STUB_SCHEMA)`.
- Ceremony rewrap loops reference the stub `nodes` schema as integer keys: `change_password.rs` line 134 (`SELECT id, file_key_wrapped FROM nodes`), line 151 (`UPDATE nodes ... WHERE id = ?`), plus test fixtures on lines 297 / 341 / 385 / 430 / 626 / 657. `rotate_key_file.rs` lines 142, 159 do the same.
- `src-tauri/src/auth/ceremonies/helpers.rs` provides `open_sqlcipher` (lines 193–225) — uses `ffi::sqlite3_key` with the raw 32-byte key path; returns `AuthenticationError::InvalidCredentials` on `sqlite3_key` failure (wrong-key path). Reusable by Phase 3.1 via a thin storage-local wrapper, or inlined separately; either is acceptable.
- `src-tauri/Cargo.toml` already pins `rusqlite = { version = "0.39", features = ["bundled-sqlcipher-vendored-openssl"] }`, `async-trait = "0.1"`, `uuid = { version = "1", features = ["v4", "serde"] }`, `thiserror = "2"`, `tokio = { version = "1", features = ["macros", "rt-multi-thread", "fs", "io-util", "sync", "time"] }`, `hex`, `base64`. **No new Cargo dependencies required.**
- `.claude/rules/storage.md` "Manifest (SQLCipher)" section lists only `nodes`, `chunks`, `manifest_meta`. It **does not** mention `pending_deletions`, the `destination_sessions` Phase 4 placeholder, the Phase 5 sharing tables, nor the `MetadataStore` method coverage (`rename_node`, `move_node`). The "Deletion" section describes "read blob names, delete node row (CASCADE), commit, then delete blobs" but **omits** the same-transaction `pending_deletions` enqueue required by design-invariant §10.
- `.github/instructions/storage.instructions.md` mirrors `.claude/rules/storage.md` and is kept in sync via `/copilot-sync`.

**Pending architectural decisions** relevant to Phase 3.1:

- All design-level decisions in `design.md` "Decisions Made" (lines 547–566) are closed. No open `Open Decisions` rows (lines 537–541) gate Phase 3.1 — "upload order randomisation" is Phase 4, "maximum file size" is non-blocking, "video metadata stripping" is Phase 3.2/6.

## 3. Design Concerns / Open Questions

### DC-1 — Phase 2.4 stub schema conflicts with canonical `nodes` schema

| Field | Content |
|---|---|
| Concern | Phase 2.4's `VAULT_STUB_SCHEMA` defines `nodes (id INTEGER PRIMARY KEY, file_key_wrapped BLOB NOT NULL)` while the canonical Phase 3.1 schema uses `nodes (node_id TEXT PRIMARY KEY, …, file_key_wrapped BLOB …)` with `file_key_wrapped` nullable for directories. Existing rewrap loops in `change_password.rs` / `rotate_key_file.rs` and their tests use integer `id`. |
| Source | `src-tauri/src/auth/ceremonies/mod.rs:35-46`, `change_password.rs:134,151,297,341,385,430,626,657`, `rotate_key_file.rs:142,159`, `helpers.rs:325` vs canonical DDL at `design.md:123-149` and `CHECK` invariant at `design.md:135-136`. |
| Impact | If Phase 3.1 only creates the canonical schema without updating the stub/ceremony SQL, ceremony tests break. If Phase 3.1 leaves the stub in place, the schema violates the canonical contract. |
| Classification | Non-blocking. |
| Resolution | Phase 3.1 replaces `VAULT_STUB_SCHEMA` with a new `CANONICAL_SCHEMA` constant exposed by `src-tauri/src/storage/schema.rs`, deletes `CREATE TABLE _phase_stub`, keeps `CREATE TABLE vault_identity` (still owned by auth ceremonies; Phase 5 will formalise it), and rewrites the rewrap loops to `SELECT node_id, file_key_wrapped FROM nodes WHERE file_key_wrapped IS NOT NULL` / `UPDATE ... WHERE node_id = ?` with `String` identifiers. Test fixtures switch from integer literals to deterministic UUID strings. See Approach step 8. |

### DC-2 — Sub-phase module placement conflicts with project rule "`mod.rs` re-exports only"

| Field | Content |
|---|---|
| Concern | Deliverables 3 and 5 state that `Node`, `ChunkRecord`, and the `MetadataStore` trait live in `src-tauri/src/storage/mod.rs`. `.claude/rules/rust.md` Structure section requires "mod.rs (re-exports only) + error.rs + types/ subfolder" and "Newtypes in `types/`". |
| Source | `3.1-schema-and-metadata-store.md:14-16` vs `.claude/rules/rust.md:3-7`. |
| Impact | Placing definitions in `mod.rs` drifts from established project layout (e.g. `crypto/types/mod.rs`, `auth/types/mod.rs`). If left in `mod.rs`, future `rust-reviewer` runs will flag it. |
| Classification | Non-blocking. |
| Resolution | Define the trait in `src-tauri/src/storage/metadata_store.rs` (one concern per file), `Node` in `src-tauri/src/storage/types/node.rs`, `ChunkRecord` in `src-tauri/src/storage/types/chunk_record.rs`, and a `NodeId` newtype wrapper in `src-tauri/src/storage/types/node_id.rs`. `mod.rs` re-exports them. The sub-phase contract (all types reachable from `crate::storage::*`) is preserved; only the internal file layout differs. |

### DC-3 — SQLCipher wrong-key detection requires a sentinel query, not just `sqlite3_key`

| Field | Content |
|---|---|
| Concern | `ffi::sqlite3_key` returns `SQLITE_OK` even when the supplied key is wrong — SQLCipher delays key verification until the first page read. A wrong key surfaces as "file is not a database" / `SQLITE_NOTADB` on the next statement, not at keying time. The sub-phase requires `StorageError::WrongKey` for wrong-key rejection (acceptance criterion, lines 36–40) but design docs and sub-phase don't prescribe the detection mechanism. |
| Source | `3.1-schema-and-metadata-store.md:36-40, 64` (acceptance criterion and security review bullet) vs `helpers.rs:198-223` (existing impl maps any `sqlite3_key` error to `InvalidCredentials` and does not probe). |
| Impact | Without a probe step, a wrong key would surface later as `StorageError::Database` (generic) rather than `WrongKey`, defeating the test in deliverable 8. |
| Classification | Non-blocking. |
| Resolution | After `sqlite3_key` returns `SQLITE_OK`, Phase 3.1 runs `SELECT count(*) FROM sqlite_master` as a sentinel. Any `rusqlite::Error::SqliteFailure` whose extended error code is `SQLITE_NOTADB` maps to `StorageError::WrongKey`; other errors map to `StorageError::Database`. A helper `verify_sqlcipher_key(&Connection) -> Result<(), StorageError>` in `schema.rs` encapsulates this. |

### DC-4 — `async_trait` + `rusqlite::Connection` requires explicit off-runtime offload; sub-phase only notes the `Mock` side

| Field | Content |
|---|---|
| Concern | `MetadataStore: Send + Sync` + `#[async_trait]` means `SqlCipherMetadataStore` methods are `async`. `rusqlite::Connection` is blocking and `!Send` when borrowed. The sub-phase's Implementation Notes (line 55) prescribe `Arc<Mutex<…>>` for `MockMetadataStore` but say nothing about the sqlcipher path. |
| Source | `3.1-schema-and-metadata-store.md:55` vs existing pattern in `auth/ceremonies/create.rs:129-142` (`tokio::task::spawn_blocking` around rusqlite). |
| Impact | A naïve `async fn insert_node` that calls `conn.execute` directly on the async runtime will block the reactor; clippy's `blocking_ops_in_async_context` won't catch all cases and nothing else will flag it. |
| Classification | Non-blocking. |
| Resolution | `SqlCipherMetadataStore` wraps the connection in `Arc<tokio::sync::Mutex<rusqlite::Connection>>`. Each `async fn` clones the `Arc`, acquires the lock, and runs the synchronous SQLite work inside `tokio::task::spawn_blocking`. The lock is released inside the blocking closure before returning. A single private helper `async fn with_connection_blocking<T>(&self, f: FnOnce(&mut Connection) -> Result<T, StorageError>) -> Result<T, StorageError>` keeps boilerplate out of the trait impls. |

### DC-5 — `StorageError::Io` and `StorageError::ConstraintViolation` trigger surfaces are thin in 3.1's scope

| Field | Content |
|---|---|
| Concern | `.claude/rules/rust.md` requires that **every** `thiserror` variant have a test that triggers it. `StorageError::Io` is really a Phase 3.2/3.3 concern (file I/O on staging blobs). `StorageError::ConstraintViolation` has a natural trigger (duplicate `UNIQUE(node_id, chunk_index)`) but `Io` does not. |
| Source | `.claude/rules/rust.md:29` ("Every thiserror variant must have a test that triggers it"); sub-phase deliverable 4 lists both variants. |
| Impact | Adding `Io` without a test breaks the project rule; omitting it defers the variant to Phase 3.2 and reshapes the enum between phases. |
| Classification | Non-blocking. |
| Resolution | Trigger `StorageError::Io` in 3.1 by pointing `SqlCipherMetadataStore::open` at a path whose parent directory does not exist (and cannot be created by SQLite), mapping the resulting `rusqlite::Error::SqliteFailure { code: SQLITE_CANTOPEN, .. }` to `StorageError::Io`. This keeps the variant covered without adding file-blob code. If that mapping feels forced, the alternative is to keep `Io` out of the enum until Phase 3.2 — implementer picks one during implementation; preferred is the former. |

### DC-6 — Governance rule `.claude/rules/storage.md` is stale on `pending_deletions`, `destination_sessions`, sharing tables, and `MetadataStore` surface

| Field | Content |
|---|---|
| Concern | The rule file doesn't list `pending_deletions`, omits the same-transaction enqueue requirement from invariant §10, and predates the Phase 3.1 `MetadataStore` method surface (it lacks `rename_node`, `move_node`, `list_pending_deletions`, `mark_deletion_complete`, snapshot-counter operations). |
| Source | `.claude/rules/storage.md` "Manifest" and "Deletion" sections vs `design.md:123-218, 461-527` and `design-invariants.md` §10. |
| Impact | Future reviews might miss drift between plan/rule/design. `/copilot-sync` will propagate whatever state the rule is in to `.github/instructions/storage.instructions.md`. |
| Classification | Non-blocking governance drift → Section 8 action. |
| Resolution | Add pre-implementation rule edit (Section 8 Action G-1) followed by `/copilot-sync`. |

## 4. Assumptions

1. The canonical DDL in `design.md` lines 123–218 is copied **verbatim** into `src-tauri/src/storage/schema.rs`. No schema rephrasing; any whitespace/formatting change is cosmetic only.
2. The SQLCipher file location is **whatever the caller passes** to `SqlCipherMetadataStore::open(path, key)`. Phase 3.1 does not choose the on-disk path. (Ceremonies already pass `request.vault_db_path`; Phase 3.1 keeps that contract.)
3. `vault_identity` table (introduced by Phase 2.4) is preserved by Phase 3.1 as an **ancillary** table appended after the canonical schema. It is not in the canonical contract but is still required by existing auth ceremonies and the Phase 5 sharing identity.
4. `sqlcipher_key` is the raw 32-byte value from `SessionKeys::sqlcipher_key.expose()` (already implemented in Phase 2.2). Phase 3.1 uses the `sqlite3_key` FFI path exactly like `auth/ceremonies/helpers.rs:open_sqlcipher`.
5. `PRAGMA foreign_keys = ON` is applied on every open to enforce CASCADE. SQLCipher defaults to OFF per connection; the design assumes CASCADE works and therefore the pragma must be explicit.
6. Seed rows for `manifest_meta` are inserted on first-open when the `manifest_meta` table is empty: `('schema_version', '1')`, `('vault_id', <caller-supplied UUID>)`, `('snapshot_counter', '0')`, `('chunk_size_bytes', <caller-supplied>)`, `('epoch_buffer_enabled', <caller-supplied>)`. `last_synced_at` is deliberately NOT seeded (design line 159).
7. `SqlCipherMetadataStore::open` is the single entry point for Phase 3.1. A separate `SqlCipherMetadataStore::create(path, key, vault_id, chunk_size_bytes, epoch_buffer_enabled)` method (or `open`'s `Option`-argument overload) is added to seed `manifest_meta` on first vault creation — the ceremony calls the create variant, subsequent opens call `open`. Exact naming left to implementer; two constructors recommended.
8. `chunk_size_bytes` validation on open: if the stored value is outside `[131_072, 67_108_864]` → `StorageError::Database("invalid chunk_size_bytes")`. `epoch_buffer_enabled` must be `"true"` or `"false"` (string-typed, per design lines 161–162); otherwise same error.
9. Test-only `MockMetadataStore` is NOT a compile-time public type of the `arx_runa_tauri_lib` crate. It lives behind `#[cfg(any(test, feature = "test-utils"))]` like `MockCloudTransport` in `storage/cloud/mock.rs`.
10. `increment_snapshot_counter` uses a single SQL statement `UPDATE manifest_meta SET value = CAST(CAST(value AS INTEGER) + 1 AS TEXT) WHERE key = 'snapshot_counter' RETURNING CAST(value AS INTEGER)` inside a transaction, returning the post-increment `u64`. SQLite 3.35+ supports `RETURNING`; rusqlite 0.39 binds it.

## 5. Approach

### CONTRACT_SNIPPETS

- **CS-001 — `MetadataStore` trait** (verbatim from `design.md:463-527`):

```rust
use async_trait::async_trait;

#[async_trait]
trait MetadataStore: Send + Sync {
    async fn insert_node(&self, node: &Node) -> Result<(), StorageError>;
    async fn insert_chunks(&self, chunks: &[ChunkRecord]) -> Result<(), StorageError>;
    async fn get_node(&self, node_id: Uuid) -> Result<Node, StorageError>;
    async fn list_children(&self, parent_id: Uuid) -> Result<Vec<Node>, StorageError>;
    async fn get_chunks(&self, node_id: Uuid) -> Result<Vec<ChunkRecord>, StorageError>;
    async fn rename_node(&self, node_id: Uuid, new_name: &str, modified_at: i64) -> Result<(), StorageError>;
    async fn move_node(&self, node_id: Uuid, new_parent_id: Option<Uuid>, modified_at: i64) -> Result<(), StorageError>;
    async fn delete_node(&self, node_id: Uuid) -> Result<(), StorageError>;
    async fn list_pending_deletions(&self, limit: usize) -> Result<Vec<String>, StorageError>;
    async fn mark_deletion_complete(&self, blob_name: &str) -> Result<(), StorageError>;
    async fn get_meta(&self, key: &str) -> Result<Option<String>, StorageError>;
    async fn set_meta(&self, key: &str, value: &str) -> Result<(), StorageError>;
    async fn increment_snapshot_counter(&self) -> Result<u64, StorageError>;
}
```

- **CS-002 — `ChunkRecord` struct** (verbatim from `design.md:305-313`):

```rust
struct ChunkRecord {
    chunk_id:        Uuid,
    chunk_index:     u32,
    blob_name:       String,       // UUID v4; no relation to file identity
    size_padded:     u64,          // always chunk_size_bytes
    blake3_checksum: [u8; 32],
}
```

- **CS-003 — `Node` struct** (derived from canonical `nodes` DDL at `design.md:124-137`):

```rust
struct Node {
    node_id:         Uuid,
    parent_id:       Option<Uuid>,
    node_type:       NodeType,              // file | directory
    name:            String,
    created_at:      i64,                   // unix seconds
    modified_at:     i64,                   // unix seconds
    size_bytes:      u64,                   // 0 for directories
    file_key_wrapped: Option<[u8; 72]>,     // Some for files, None for directories
}

enum NodeType { File, Directory }
```

- **CS-004 — `StorageError` enum** (derived from sub-phase deliverable 4):

```rust
#[non_exhaustive]
#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("database operation failed: {0}")]
    Database(String),
    #[error("record not found")]
    NotFound,
    #[error("blob checksum mismatch")]
    ChecksumMismatch,
    #[error("I/O operation failed: {0}")]
    Io(String),
    #[error("incorrect SQLCipher key for manifest database")]
    WrongKey,
    #[error("constraint violation: {0}")]
    ConstraintViolation(String),
}
```

- **CS-005 — Canonical `CREATE TABLE` DDL** — copy verbatim from `design.md:123-218` (the full block covering `nodes`, `chunks`, `manifest_meta`, `pending_deletions`, `destination_sessions`, `contacts`, `shares`, `received_shares`). Do not re-inline here; Phase 3.1 reads from the design anchor.

### Implementation steps

1. **Add error variants** — replace the placeholder `StorageError` body in `src-tauri/src/storage/error.rs` with **CS-004**. Add a `from_rusqlite` helper that maps `rusqlite::Error::SqliteFailure { code: SQLITE_NOTADB, .. }` → `WrongKey`, `code: SQLITE_CONSTRAINT` → `ConstraintViolation(msg)`, `code: SQLITE_CANTOPEN, .. }` → `Io(msg)`, everything else → `Database(msg)`.

2. **Add domain types**:
   - `src-tauri/src/storage/types/node_id.rs` — `NodeId(Uuid)` newtype with `fn new(Uuid) -> Self`, `fn as_uuid(&self) -> &Uuid`, `Display` as hyphenated UUID. Fulfils `rust.md` "Newtypes in `types/`".
   - `src-tauri/src/storage/types/node.rs` — **CS-003** + `NodeType` enum with `TryFrom<&str>` / `AsRef<str>` for the `'file' | 'directory'` round-trip.
   - `src-tauri/src/storage/types/chunk_record.rs` — **CS-002**.
   - Update `src-tauri/src/storage/types/mod.rs` to `pub mod`-declare and re-export the three new modules alongside existing `BlobName`.

3. **Add trait file** — `src-tauri/src/storage/metadata_store.rs`:
   - `use async_trait::async_trait;`
   - Declare `MetadataStore` per **CS-001** with `pub(crate)` visibility (match the pattern used by Phase 2.4 `CloudTransport`, which is `pub`; Phase 3.1 can use `pub` since Phase 3.2 will be an intra-crate consumer but external crates may eventually test against it via `test-utils`). Doc every method with a `///` comment covering: success shape, which error variants can be returned, transactional scope.

4. **Add schema module** — `src-tauri/src/storage/schema.rs`:
   - `pub(crate) const CANONICAL_SCHEMA: &str = "..."` — the verbatim DDL from `design.md:123-218`, plus an `ALTER TABLE`-free appended `CREATE TABLE vault_identity (id INTEGER PRIMARY KEY CHECK (id = 1), public_key BLOB NOT NULL UNIQUE, wrapped_private_key BLOB NOT NULL);`. Add inline comments marking cross-phase ownership boundaries: `-- Phase 3 core`, `-- Phase 4 placeholder`, `-- Phase 5 placeholder`, `-- Phase 2.4 / 5 identity table`.
   - `pub(crate) fn apply_canonical_schema(conn: &Connection) -> Result<(), StorageError>` — `conn.execute_batch(CANONICAL_SCHEMA)` + error mapping.
   - `pub(crate) fn verify_sqlcipher_key(conn: &Connection) -> Result<(), StorageError>` — runs `SELECT count(*) FROM sqlite_master`; maps `SQLITE_NOTADB` to `StorageError::WrongKey`.
   - `pub(crate) fn seed_manifest_meta(conn: &Connection, vault_id: Uuid, chunk_size_bytes: u64, epoch_buffer_enabled: bool) -> Result<(), StorageError>` — conditional INSERT based on `INSERT OR IGNORE`.
   - `pub(crate) fn validate_manifest_meta(conn: &Connection) -> Result<(), StorageError>` — reads `chunk_size_bytes` and `epoch_buffer_enabled`; enforces the `[128 KiB, 64 MiB]` range and the string-valued boolean.

5. **Add sqlcipher implementation** — `src-tauri/src/storage/sqlcipher.rs`:
   - `pub struct SqlCipherMetadataStore { conn: Arc<tokio::sync::Mutex<Connection>> }`.
   - `impl SqlCipherMetadataStore`:
     - `pub async fn open(path: &Path, sqlcipher_key: &[u8; 32]) -> Result<Self, StorageError>` — `spawn_blocking` around `Connection::open` + `sqlite3_key` FFI + `verify_sqlcipher_key` + `PRAGMA foreign_keys = ON` + `validate_manifest_meta`. FFI mirror of `helpers.rs:194-225`.
     - `pub async fn create(path: &Path, sqlcipher_key: &[u8; 32], vault_id: Uuid, chunk_size_bytes: u64, epoch_buffer_enabled: bool) -> Result<Self, StorageError>` — same as open but additionally `apply_canonical_schema` + `seed_manifest_meta` before returning.
     - Private `async fn with_connection_blocking<T, F>(&self, f: F) -> Result<T, StorageError>` where `F: FnOnce(&mut Connection) -> Result<T, StorageError> + Send + 'static, T: Send + 'static`.
   - `#[async_trait] impl MetadataStore for SqlCipherMetadataStore { ... }` — each method offloads to `with_connection_blocking`. All mutating methods open `BEGIN IMMEDIATE;` … `COMMIT;` (rollback on error). `delete_node` uses a single transaction that (a) reads blob names via `SELECT blob_name FROM chunks WHERE node_id = ?`, (b) inserts them into `pending_deletions (blob_name, queued_at)` using `INSERT OR IGNORE`, (c) `DELETE FROM nodes WHERE node_id = ?` (CASCADE removes chunk rows), (d) commits.
   - `increment_snapshot_counter` uses the `RETURNING` SQL from Assumption 10 inside a transaction.

6. **Add mock implementation** — `src-tauri/src/storage/mock.rs`, gated `#[cfg(any(test, feature = "test-utils"))]`:
   - `pub struct MockMetadataStore { inner: Arc<std::sync::Mutex<MockState>> }` with `MockState { nodes: HashMap<Uuid, Node>, chunks_by_node: HashMap<Uuid, Vec<ChunkRecord>>, meta: HashMap<String, String>, pending_deletions: Vec<(String, i64)> }`.
   - Default `impl Default for MockMetadataStore` pre-seeds `manifest_meta` like `SqlCipherMetadataStore::create`.
   - `#[async_trait] impl MetadataStore for MockMetadataStore { ... }` — in-process `HashMap` logic; simulates UNIQUE violations by returning `StorageError::ConstraintViolation` on duplicate `(node_id, chunk_index)` or `blob_name`.

7. **Wire module re-exports** — `src-tauri/src/storage/mod.rs`:
   - `pub mod cloud;`
   - `pub mod error;`
   - `pub mod types;`
   - `pub mod metadata_store;`
   - `pub mod schema;` (pub(crate) contents; declare `pub(crate) mod schema;` actually — no external consumer).
   - `pub mod sqlcipher;`
   - `#[cfg(any(test, feature = "test-utils"))] pub mod mock;`
   - Re-export `pub use error::StorageError; pub use metadata_store::MetadataStore; pub use sqlcipher::SqlCipherMetadataStore; pub use types::{BlobName, ChunkRecord, Node, NodeId, NodeType};`.

8. **Retire Phase 2.4 stub schema + update rewrap SQL**:
   - In `src-tauri/src/auth/ceremonies/mod.rs`: delete `VAULT_STUB_SCHEMA`. Replace its single use-site (`create.rs:131`) with `storage::schema::apply_canonical_schema(&conn)` + `storage::schema::seed_manifest_meta(&conn, vault_id, chunk_size_bytes, epoch_buffer_enabled)`. `create_vault` gains two parameters on its `CreateVaultRequest`: `chunk_size_bytes: u64` (default 4 MiB) and `epoch_buffer_enabled: bool` (default false). These ride along `request`; default values live in the type.
   - In `src-tauri/src/auth/ceremonies/change_password.rs` lines 134 / 151: change to `SELECT node_id, file_key_wrapped FROM nodes WHERE file_key_wrapped IS NOT NULL` and `UPDATE nodes SET file_key_wrapped = ? WHERE node_id = ?`. Row identifier becomes `String` (TEXT) rather than `i64`.
   - In `src-tauri/src/auth/ceremonies/rotate_key_file.rs` lines 142 / 159: identical changes.
   - Update test fixtures that insert into `nodes` (lines 297, 385, 626 in `change_password.rs`) to use a deterministic UUID string (`"00000000-0000-0000-0000-000000000001"`, `...0002`, `...0003`) and the canonical INSERT form `INSERT INTO nodes (node_id, parent_id, node_type, name, created_at, modified_at, size_bytes, file_key_wrapped) VALUES (?, NULL, 'file', 'fixture', 0, 0, 0, ?)`. Corresponding SELECTs switch to `WHERE node_id = ?`.

9. **Test matrix** — deliverable 8 plus `rust.md`'s "every variant has a triggering test" rule. Tests live next to source using `#[cfg(test)] mod tests`. Split across files:
   - `sqlcipher.rs` tests (`#[tokio::test]` with `tempfile::tempdir`): wrong-key rejection (triggers `StorageError::WrongKey`), open non-existent path (triggers `StorageError::Io`), open with correct key + schema applied + round-trip insert/get of a `Node` and `ChunkRecord`, `UNIQUE(node_id, chunk_index)` duplicate insert (triggers `ConstraintViolation`), `UNIQUE(blob_name)` duplicate insert (triggers `ConstraintViolation`), CASCADE on `delete_node` removes chunk rows and enqueues `pending_deletions`, `list_pending_deletions` honours `limit`, `mark_deletion_complete` drops the queued row, `increment_snapshot_counter` atomic under contention (two concurrent calls via `tokio::join!` yield distinct counter values), `list_children` returns only direct children (triggers path through `NotFound` for the 0-byte edge when combined with the next test), 0-byte file insert + retrieval (node with no chunks), `rename_node` updates `name` and `modified_at`, `move_node` updates `parent_id` (including `None` → root).
   - `mock.rs` tests: parallel coverage of insert/get round-trip, duplicate → `ConstraintViolation`, CASCADE semantics on `delete_node`, seeded `manifest_meta` reads, `increment_snapshot_counter` monotonic.
   - `schema.rs` tests: `validate_manifest_meta` rejects out-of-range `chunk_size_bytes` (triggers `StorageError::Database`), rejects non-boolean `epoch_buffer_enabled`, accepts valid seed.
   - `error.rs` tests: `from_rusqlite` mapping covers SQLITE_NOTADB, SQLITE_CONSTRAINT_UNIQUE, SQLITE_CANTOPEN, default → `Database`. The `ChecksumMismatch` variant is triggered in Phase 3.2; for 3.1 the rule is satisfied only if either (a) `ChecksumMismatch` is omitted from the enum until 3.2 OR (b) a unit test constructs the variant and asserts its `Display` (acceptable — the rule reads "a test that triggers it", and a variant construction + Display check is the minimum). **Preferred**: keep all five/six variants now, construct `ChecksumMismatch` in a lightweight unit test so Phase 3.2 doesn't reshape the enum.

## 6. Review focus areas

### 6a. Rust change surface

- `src-tauri/src/storage/error.rs` — rewrite body with **CS-004** + rusqlite mapping.
- `src-tauri/src/storage/mod.rs` — add `metadata_store`, `schema`, `sqlcipher`, `mock` (cfg-gated) module declarations and re-exports.
- `src-tauri/src/storage/metadata_store.rs` — **new**, hosts trait.
- `src-tauri/src/storage/schema.rs` — **new**, hosts canonical DDL constant + helper functions.
- `src-tauri/src/storage/sqlcipher.rs` — **new**, hosts `SqlCipherMetadataStore`.
- `src-tauri/src/storage/mock.rs` — **new** (cfg-gated), hosts `MockMetadataStore`.
- `src-tauri/src/storage/types/node.rs` — **new**, hosts `Node` + `NodeType`.
- `src-tauri/src/storage/types/node_id.rs` — **new**, hosts `NodeId` newtype.
- `src-tauri/src/storage/types/chunk_record.rs` — **new**, hosts `ChunkRecord`.
- `src-tauri/src/storage/types/mod.rs` — re-export additions.
- `src-tauri/src/auth/ceremonies/mod.rs` — delete `VAULT_STUB_SCHEMA`.
- `src-tauri/src/auth/ceremonies/create.rs` — use `storage::schema::apply_canonical_schema` + `seed_manifest_meta`.
- `src-tauri/src/auth/ceremonies/types.rs` — extend `CreateVaultRequest` with `chunk_size_bytes` + `epoch_buffer_enabled`.
- `src-tauri/src/auth/ceremonies/change_password.rs` + `rotate_key_file.rs` — rewrite rewrap SQL to use TEXT `node_id`; update test fixtures.
- `src-tauri/src/auth/ceremonies/helpers.rs` — if the `open_sqlcipher` + `rekey_sqlcipher` helpers are reused by `SqlCipherMetadataStore`, factor shared FFI code into `storage/sqlcipher.rs` as a private helper and have ceremonies call through it; otherwise leave duplicate FFI in place (implementer choice, prefer factoring to avoid divergence).

### 6b. Security-sensitive paths

- `src-tauri/src/storage/sqlcipher.rs` — **sqlcipher keying correctness**: raw 32-byte key via `sqlite3_key` FFI (no passphrase derivation), wrong-key detection via `SELECT count(*) FROM sqlite_master` → `SQLITE_NOTADB` → `StorageError::WrongKey`, `PRAGMA foreign_keys = ON` applied before any data op, `Connection` dropped promptly on wrong-key so no partial state leaks. No plaintext persisted; nothing in this file touches file-level secrets other than the 32-byte SQLCipher key that remains on stack only for the FFI call.
- `src-tauri/src/storage/schema.rs` — **schema integrity**: CHECK constraints enforced (`file_key_wrapped` null-iff-directory), UNIQUE constraints on `(node_id, chunk_index)` and `blob_name`, CASCADE on delete. Any accidental DDL drift from `design.md:123-218` breaks the zero-knowledge contract.
- `src-tauri/src/auth/ceremonies/helpers.rs` / `change_password.rs` / `rotate_key_file.rs` — the re-wrap SQL change **must not** accidentally leave directory rows (which will now have `file_key_wrapped = NULL`) in the SELECT set; the `WHERE file_key_wrapped IS NOT NULL` guard is load-bearing. Dropping it would cause the rewrap loop to panic on `.try_into::<[u8;72]>()` of a zero-length blob.

### 6c. Architecture risk areas

- `src-tauri/src/storage/mod.rs` — enforce "`mod.rs` re-exports only" after the churn in DC-2. No struct bodies or trait definitions in the file.
- `src-tauri/src/storage/sqlcipher.rs` — single responsibility: SQLCipher-backed `MetadataStore`. Do not inline schema DDL (that belongs in `schema.rs`) or trait definition (that belongs in `metadata_store.rs`). The `with_connection_blocking` helper must remain private.
- Dependency direction — Phase 3.1 depends on `src-tauri/src/memory` (indirectly via `sqlcipher_key` bytes) and must NOT depend on `src-tauri/src/auth`. `auth::ceremonies` calls **into** `storage::schema`, not the other way round.
- The `vault_identity` table's ownership is ambiguous — Phase 3.1 hosts the DDL but the table is used exclusively by `auth::ceremonies` and (later) Phase 5 sharing. Document this in the schema comment block; do **not** add `vault_identity`-specific methods to `MetadataStore` in 3.1.

### 6d. Testing requirements

Validation checkpoint from the sub-phase:

- `cargo test storage::metadata` passes locally (use `cargo test --workspace --all-targets --all-features` per user convention).
- Manual: opening the SQLCipher file without a key must show binary gibberish; opening with the right key must expose all canonical tables and seed rows.

Acceptance criteria (sub-phase lines 36–40):

- All 13 `MetadataStore` methods implemented on both `SqlCipherMetadataStore` and `MockMetadataStore`.
- Wrong-key rejection returns `StorageError::WrongKey` (not a panic, not a silent corruption).
- `delete_node` cascades to chunk rows and enqueues blob names into `pending_deletions` in the same transaction (invariant §10).
- `increment_snapshot_counter` is atomic within a transaction — the concurrent `tokio::join!` test must return two distinct values.

Boundary cases from Step 2:

- 0-byte file: `Node` with `size_bytes = 0` and `file_key_wrapped = Some(...)`, no chunk rows.
- Directory: `Node` with `file_key_wrapped = None`; DDL CHECK rejects `file_key_wrapped IS NOT NULL` for directories.
- Duplicate chunk index for the same file → `StorageError::ConstraintViolation`.
- Duplicate `blob_name` (even across files) → `StorageError::ConstraintViolation`.
- Non-existent path or unwritable parent directory → `StorageError::Io` (DC-5).

## 7. Documentation impact

- `docs/architecture/designs/chunking-and-manifest/sub-phases/3.1-schema-and-metadata-store.md` — append an "Implementation Notes addendum" after implementation noting: (a) trait + types live in dedicated files per `rust.md` Structure rule (resolves DC-2), (b) wrong-key detection uses a sentinel query (resolves DC-3), (c) `vault_identity` carried forward from Phase 2.4 (resolves DC-1). Keep the existing body unchanged.
- `docs/architecture/designs/chunking-and-manifest/design.md` — no structural edits expected. If implementer finds that the `DDL ↔ Rust Node struct` naming has a discrepancy, add a footnote rather than mutating the DDL.
- `docs/architecture/designs/chunking-and-manifest/diagrams/manifest-schema.md` — regenerate via `/diagram` after schema lands to pick up `pending_deletions` + `destination_sessions` if not already present.

## 8. Governance sync actions (pre-implementation)

| Action ID | Reason | Target files (absolute paths) | Required edit | Verification |
|---|---|---|---|---|
| **G-1** | DC-6 — rule omits `pending_deletions`, same-transaction enqueue, `destination_sessions`, sharing tables, and new trait methods | `C:\Users\chris\source\repos\arx-runa\.claude\rules\storage.md` | (a) "Manifest (SQLCipher)" → add bullet: "Canonical tables: `nodes`, `chunks`, `manifest_meta`, `pending_deletions`. Forward-declared: `destination_sessions` (Phase 4), `contacts` / `shares` / `received_shares` (Phase 5). See design docs for DDL." (b) "Deletion" → extend "Transaction order" bullet to "read blob names, **enqueue into `pending_deletions` inside the same transaction**, delete node row (CASCADE removes chunk rows), commit, then delete local staging blobs." (c) Add a new "Traits" subsection spelling out the 13 `MetadataStore` methods required as of Phase 3.1. | Re-read the file and confirm the three edits appear verbatim; re-verify design-invariant §10 is reflected. |
| **G-2** | Propagate G-1 to the GitHub Copilot mirror | `C:\Users\chris\source\repos\arx-runa\.github\instructions\storage.instructions.md` | Run `/copilot-sync` **after** G-1 — do not hand-edit. | `diff` between `storage.md` and `storage.instructions.md` contains only preamble differences. |

## 9. Handoff Notes for Implementer

Working directory: `C:\Users\chris\source\repos\arx-runa`. Execution order: (1) governance sync G-1 + `/copilot-sync` (G-2), (2) Approach steps 1–7 (new storage code in dependency order), (3) Approach step 8 (retire Phase 2.4 stub and migrate ceremony SQL in the same commit — the schema replacement is all-or-nothing), (4) Approach step 9 tests. The plan is self-contained: all DDL, trait signatures, and struct shapes are inlined via **CS-001**–**CS-005** or anchored to exact design-doc line ranges. Traps: (a) `PRAGMA foreign_keys = ON` is per-connection — forgetting it silently defeats CASCADE in tests even though SQLite "appears to work"; (b) rusqlite 0.39 `RETURNING` requires a `query_row` call, not `execute`, to retrieve the value; (c) the `CHECK ((node_type = 'file' AND file_key_wrapped IS NOT NULL) OR (node_type = 'directory' AND file_key_wrapped IS NULL))` constraint fires on INSERT — fixture code that omits `file_key_wrapped` for files must generate a deterministic 72-byte blob; (d) run `cargo test --workspace --all-targets --all-features` (not plain `cargo test`) to exercise the `test-utils`-gated `MockMetadataStore`; (e) this sub-phase is tagged `Security Review — Required` by the parent roadmap but per user preference `security-reviewer` / `test-writer` agents are not invoked automatically — run them only if the user explicitly requests.
