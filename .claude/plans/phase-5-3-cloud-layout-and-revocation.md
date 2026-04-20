---
title: "Phase 5.3 — Cloud Layout and Revocation"
created: "2026-04-20T16:10:00Z"
status: approved
roadmap-phase: 5
sub-phase: "5.3"
design-document: docs/architecture/designs/file-sharing/design.md
sub-phase-roadmap: docs/architecture/designs/file-sharing/sub-phases/roadmap.md
governance-sync-required: true
tags: [sharing, cloud, revocation, re-encryption, security-critical]
---

## 1. Goal

Implement owner-side share creation (blob copy into `shared/<file_share_id>/`), recipient-side blob fetch and decrypt, default revocation by blob deletion, and the strong revocation path that rotates `file_key` + `file_share_id` via file re-encryption.

## 2. Context

- Sub-phase scope (roadmap Phase 5.3): `~150` LoC production, `~100` LoC tests; final sub-phase of Phase 5.
- Depends on Phase 5.1 (identity/contacts), Phase 5.2 (`sharing::packages::create_share_package` / `import_share_package`, `received_shares` table, `SharingStore` CRUD for contacts + received shares), and Phase 4 (`CloudTransport`, `MockCloudTransport`, path-based blob API, `staging` directory, `validate_remote_path`).
- Parent design sections: §Cloud Storage Layout (lines 191-215), §Revocation (lines 217-245), §Threat Model Additions (lines 419-435), §Database Schema (lines 363-382).
- Cross-phase invariants #1 (chunk AAD `file_id || chunk_index`), #5 (remote path allowlist), #7 (zero-trace persistence), #10 (`pending_deletions` durability — does **not** apply to `shared/` namespace per Section 3 Concern 4), #11 (share-package key-handling), #12 (revocation semantics), #13 (vault identity read-only from sharing).
- Rule anchors: `.claude/rules/sharing.md`, `.claude/rules/storage.md`, `.claude/rules/crypto.md`, `.claude/rules/rust.md`.
- `shares` table and `received_shares` canonical DDL already landed in `src-tauri/src/storage/schema.rs` (Phase 5.2 GS-003). No further schema work needed.
- Security-reviewer agent review is required per sub-phase §Security Review.

## 3. Design Concerns / Open Questions

| Concern | Source | Impact | Classification | Resolution | Documentation updates |
|---|---|---|---|---|---|
| 1. Sub-phase does not state **where** outgoing `shares` CRUD lives. Rule `.claude/rules/storage.md` mandates "`contacts` CRUD lives in `storage::sharing` behind the `SharingStore` trait", implying the `shares` table should follow the same pattern. | Sub-phase deliverables 3, 4, 6 vs. rule `.claude/rules/storage.md` (Traits) and `.claude/rules/sharing.md` (Trait boundaries). | Implementer might add `shares` methods to `MetadataStore`, violating sharing trait boundary rule. | Non-blocking | Extend `SharingStore` with `insert_share`, `get_share`, `list_shares_by_file`, `list_active_shares_by_file`, `list_active_shares_by_file_share_id`, `set_share_revoked_at`. Implement on `SqlCipherMetadataStore` in `src-tauri/src/storage/sharing.rs`. Do not touch `MetadataStore`. | Governance sync action GS-001 adds rule line about `shares` CRUD placement. |
| 2. Re-encryption mutates `nodes.file_key_wrapped` and `chunks.blob_name` — operations **not exposed** by `MetadataStore` (no `update_node_key`, no `replace_chunks`). Sub-phase deliverable 5 assumes this is possible. | Sub-phase deliverable 5 vs. `MetadataStore` surface (`src-tauri/src/storage/metadata_store.rs`). | Without a mutation path, strong revocation cannot update the manifest; sub-phase is un-implementable as written. | Non-blocking | Add a SQLCipher-specific helper `SqlCipherMetadataStore::replace_file_key_and_chunks(file_id, new_wrapped_key, new_chunks) -> Result<(), StorageError>` in a single SQL transaction (UPDATE nodes, DELETE chunks WHERE node_id=?, INSERT new chunks, enqueue OLD `blob_name` into `pending_deletions`). This mirrors the `rollback_snapshot_counter` / `list_sync_chunks` exception pattern permitted by `.claude/rules/storage.md`. Not added to `MetadataStore`. | Governance sync action GS-002 adds rule line about the helper in `.claude/rules/storage.md`. |
| 3. `file_share_id` reuse across multiple recipients of the same file. Sub-phase deliverable 2 says "generate a UUID v4 at share creation time" but design §Cloud Storage Layout (line 203) says "All recipients of the same file share the same set of blobs" under a single `file_share_id`. Sub-phase contradicts the design for the multi-recipient path. | Sub-phase deliverable 2 vs. design §Cloud Storage Layout. | Always-generate produces duplicate blob copies under different `file_share_id`s, wasting cloud storage and breaking the "one copy per file" invariant (design §Decisions Made — "one copy per file"). | Non-blocking | On share creation, query `SharingStore::list_active_shares_by_file(file_id)`: if an active (`revoked_at IS NULL`) share exists, reuse its `file_share_id` and **skip** the blob copy step. Only generate a new `file_share_id` + copy blobs when no active share exists. | None (design is authoritative; sub-phase text is accurate in spirit: `file_share_id` is generated on the **first** share of a file). |
| 4. Partial-failure semantics for revocation blob deletion. Sub-phase acceptance criterion: "partial deletion (network failure mid-revocation) must leave the system in a state that can be retried." Design invariant #10 (`pending_deletions`) covers only the `vault/` namespace; the queue schema has only `blob_name` (no path/prefix), so it cannot carry `shared/<file_share_id>/<uuid>.blob` entries without a schema change. | Sub-phase deliverable 4 vs. `pending_deletions` schema (`src-tauri/src/storage/schema.rs`) and rule `.claude/rules/storage.md` Deletion. | Extending `pending_deletions` to `shared/` would require schema change and is out of 5.3 scope. Omitting retry semantics violates the acceptance criterion. | Non-blocking | Phase 5.3 revocation runs a sequential `CloudTransport::delete_blob` loop; on the **first** failure, return `SharingError::RevocationPartial` carrying the index of the failed blob. Caller retries by calling `revoke_share` again: the function is idempotent (`delete_blob` on missing path is a no-op per `CloudTransport` contract). `revoked_at` is set **only after** all deletes succeed. A durable queue for shared-blob deletion is deferred to a future phase. | Governance sync action GS-003 adds a short note to `.claude/rules/sharing.md` documenting the sequential delete + retry contract. |
| 5. Recipient-side `CloudTransport` construction from `received_shares.cloud_endpoint`. The owner's `cloud_endpoint` JSON is an arbitrary provider descriptor (`provider`, `bucket`, `region`, `endpoint`, `path_prefix`), but `RcloneTransport` is constructed from a session-scoped `rclone.conf` that the recipient does not have for the owner's bucket. | Sub-phase deliverable 7 vs. `storage::cloud::rclone` construction path. | Recipient cannot fetch real blobs via Rclone in this sub-phase without solving dynamic remote registration; tests pass with `MockCloudTransport` but the production path is unfinished. | Non-blocking | Phase 5.3 defines the recipient-side fetch function as generic over `dyn CloudTransport`, verified with `MockCloudTransport` in tests. Production wiring (dynamic Rclone remote registration for foreign buckets) is called out explicitly in sub-phase Handoff Notes and in §7 Documentation impact as deferred to Phase 6 UI integration. | Noted in Section 7. |
| 6. Download receipts (design §Download Receipts, lines 248-292) are **not** in sub-phase 5.3 deliverables. The sub-phase completion list does not mention receipts either. | Sub-phase Deliverables §1-8 vs. design §Download Receipts. | If receipts are expected in Phase 5.3 and omitted, Phase 5 ships incomplete. Re-reading design: "Implementation target: Phase 5 (optional enhancement alongside core file sharing)" — receipts are optional and this sub-phase excludes them. | Non-blocking | Phase 5.3 does **not** implement receipts. Explicitly flagged in Assumption §4.8 and Handoff Notes. A separate plan entry can cover receipts later. | None. |
| 7. Re-encryption touches `storage::vault_ops` concerns (decrypt + re-encrypt chunks) and `sharing::` concerns (rotate `file_share_id`, issue new packages, mark old shares revoked). SRP-wise these are different layers. | Sub-phase deliverable 5. | Putting vault re-encryption inside `sharing::` would leak vault concerns into the sharing module; putting sharing orchestration into `storage::vault_ops` would do the reverse. | Non-blocking | Split: `storage::vault_ops::reencrypt_file(file_id, metadata_store, cloud, kek, staging)` does read-blob → decrypt → re-encrypt-under-new-key → upload → `replace_file_key_and_chunks` (vault layer). `sharing::revocation::strong_revoke_share(share_id, ...)` calls `reencrypt_file`, then deletes the old shared folder, re-issues packages for remaining recipients (callers write the `.vgshare` bytes out-of-band — this function returns the list of new wire blobs), and marks old shares revoked. | None — this is structural, no rule text change needed beyond Section 5 placement. |
| 8. Temporary ciphertext file location for owner-side blob copy. Sub-phase line 67: "temporary local ciphertext files" but does not name a directory. Staging directory (Phase 3) is the established pattern for transient vault artefacts. | Sub-phase Implementation Notes. | Writing to an arbitrary temp directory could leak into non-vault paths; using the system temp dir is acceptable but inconsistent with staging convention. | Non-blocking | Use the vault's staging directory (obtained via `storage::staging` module; same directory as upload/download staging). Temporary file name: `shared-copy-<uuid>.blob`, deleted on upload success/failure. | None. |

## 4. Assumptions

1. All outgoing `shares` CRUD is added to the existing `SharingStore` trait in `src-tauri/src/sharing/store.rs` and implemented on `SqlCipherMetadataStore` in `src-tauri/src/storage/sharing.rs`. No new trait, no `MetadataStore` growth.
2. A new orchestration function `sharing::cloud::create_share(file_id, contact_id, expires_at, owner_cloud_endpoint, metadata, sharing, cloud, kek, staging_dir) -> Result<CreateShareOutput, SharingError>` composes: (a) resolve or create `file_share_id` per Concern 3, (b) copy blobs (skip if reusing existing `file_share_id`), (c) create `.vgshare` bytes via `packages::create_share_package`, (d) insert `shares` row. Returns `{ share_id, file_share_id, wire_bytes }`.
3. Blob copy path: for each `blob_name` in `metadata.get_chunks(file_id)`, call `cloud.download_blob("vault/<uuid>.blob", &staging_dir.join(format!("shared-copy-{uuid}.blob")))` → `cloud.upload_blob(&temp_path, &format!("shared/<file_share_id>/{uuid}.blob"))` → `tokio::fs::remove_file(&temp_path)`. No plaintext is ever materialised; the blob is opaque ciphertext throughout.
4. Revocation (`sharing::revocation::revoke_share(share_id, ...)`): load the share, load sibling active shares for the same `file_share_id`; if this is the **only** remaining active share, sequentially delete every `shared/<file_share_id>/<uuid>.blob` via `CloudTransport::delete_blob` (and the folder placeholder if present). Set `revoked_at = now` only after all deletes succeed. If other active shares exist, only set `revoked_at` for the target share row (default cooperative path per design §Revocation Single recipient). The strong path is a separate explicit call (§Assumption 6).
5. Revocation partial failure returns `SharingError::RevocationPartial { failed_index }` without mutating the `shares` row; re-invocation is idempotent because `delete_blob` on a missing path is a no-op per `CloudTransport` contract.
6. Strong revocation (`sharing::revocation::strong_revoke_share(share_id, ...)`): (a) call `storage::vault_ops::reencrypt_file` which generates new `file_key`, decrypts each existing blob, re-encrypts under new `file_key` and new blob UUIDs, uploads to `vault/<new_uuid>.blob`, and calls `SqlCipherMetadataStore::replace_file_key_and_chunks` transactionally (new wrapped key + new chunk rows + enqueue old `blob_name`s into `pending_deletions`); (b) copy the re-encrypted blobs into a **new** `shared/<new_file_share_id>/`; (c) issue fresh `.vgshare` packages for each still-active recipient (returned to caller to send out-of-band); (d) delete all blobs under the old `shared/<old_file_share_id>/` via sequential `delete_blob`; (e) mark every share row tied to the old `file_share_id` as `revoked_at = now` and insert new active rows for each re-issued recipient.
7. `storage::vault_ops::reencrypt_file` reuses existing `crypto::decrypt_chunk` + `crypto::encrypt_chunk` so the chunk AAD stays `file_id || chunk_index` (invariant #1 preserved); the `file_id` does not change during re-encryption.
8. Download receipts (design §Download Receipts) are **not** implemented in Phase 5.3.
9. Recipient fetch (`sharing::cloud::fetch_received_share_to_local(share_id, cloud, kek, metadata, destination_dir)`): (a) load `received_shares` row, (b) parse `chunk_uuids`, (c) for each UUID download `shared/<file_share_id>/<uuid>.blob` into staging, (d) `verify_checksum` then `decrypt_chunk` using unwrapped `file_key`, (e) append plaintext to the destination file, (f) zeroize the `FileKey` after the last chunk. The `file_share_id` is extracted from `cloud_endpoint.path_prefix` (format `shared/<file_share_id>/`). The caller-supplied `cloud` is a `&dyn CloudTransport`; tests use `MockCloudTransport`, production uses a to-be-wired Rclone remote (deferred per Concern 5).
10. Recipient cannot compute BLAKE3 checksums from the share package alone because `chunks.blake3_checksum` is not in the package — it's owner-only manifest data. Therefore recipient-side `decrypt_chunk` must be callable **without** a pre-verified `VerifiedBlob`. Resolution: recipient-side fetch computes `compute_checksum` on the downloaded blob and immediately calls `verify_checksum` against the just-computed checksum (trivially succeeds) — this preserves the `VerifiedBlob` compile-time barrier without fabricating an external checksum claim. This is documented in-line as the authorised exception: the AEAD tag is the actual integrity check for recipient-side fetch; BLAKE3 is a redundant owner-side integrity layer that does not apply cross-vault.
11. Shared-cloud path strings (`shared/<uuid>/<uuid>.blob`) pass the existing `validate_remote_path` allowlist (regex `^[a-zA-Z0-9._/-]+$`), because UUID v4 hyphenated form and `shared/` are all within the allowlist. No new path validator is needed.
12. `ShareRecord` domain type (outgoing share) is new; lives in `src-tauri/src/sharing/store.rs` alongside `Contact` and `ReceivedShare`.
13. `file_share_id` and `share_id` are both `String` (UUID v4 hyphenated) at the storage boundary, mirroring how `share_id` is represented for `ReceivedShare`. Storage-layer v4 validation uses the existing `is_uuid_v4_string` helper in `src-tauri/src/storage/sharing.rs`.
14. New error variants (per CS-007) are added to `SharingError` with one display-message test each (rule `.claude/rules/rust.md` Testing).
15. No new HPKE `info` string is introduced in Phase 5.3 (receipts deferred per §4.8).
16. Strong revocation returns a structured `StrongRevocationOutput { new_file_share_id, reissued_packages: Vec<(ContactId, Vec<u8>)> }` so the caller (UI / IPC layer in Phase 6) can deliver new `.vgshare` bytes out-of-band.
17. `created_at` / `revoked_at` / `expires_at` timestamps are injected by the caller as `i64` Unix seconds (same pattern as Phase 5.1/5.2 functions that take `now_unix_seconds`). No direct clock reads inside sharing.
18. Tests use `MockCloudTransport` exclusively; no Rclone process is spawned in the automated suite.

## 5. Approach

### `CONTRACT_SNIPPETS`

**CS-001 — `ShareRecord` domain struct** (new, in `src-tauri/src/sharing/store.rs`)
```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShareRecord {
    pub share_id: String,          // UUID v4 hyphenated
    pub file_id: String,           // UUID v4 hyphenated (nodes.node_id)
    pub contact_id: ContactId,
    pub file_share_id: String,     // UUID v4 hyphenated
    pub cloud_path: String,        // "shared/<file_share_id>/"
    pub created_at: i64,           // Unix seconds
    pub expires_at: Option<i64>,   // Unix seconds, NULL = no expiry
    pub revoked_at: Option<i64>,   // NULL = active
}
```

**CS-002 — `SharingStore` trait additions**
```rust
async fn insert_share(&self, share: &ShareRecord) -> Result<(), SharingError>;
async fn get_share(&self, share_id: &str) -> Result<ShareRecord, SharingError>;
async fn list_shares_by_file(&self, file_id: &str) -> Result<Vec<ShareRecord>, SharingError>;
async fn list_active_shares_by_file(&self, file_id: &str) -> Result<Vec<ShareRecord>, SharingError>;
async fn list_active_shares_by_file_share_id(&self, file_share_id: &str) -> Result<Vec<ShareRecord>, SharingError>;
async fn set_share_revoked_at(&self, share_id: &str, revoked_at: i64) -> Result<(), SharingError>;
```

**CS-003 — New `SharingError` variants** (added to `src-tauri/src/sharing/error.rs`)
```rust
/// A cloud transport operation underpinning share creation, revocation, or fetch failed.
#[error("sharing cloud operation failed: {0}")]
CloudOperation(String),
/// Revocation blob deletion stopped mid-loop; caller retries by re-invoking `revoke_share`.
#[error("revocation partial: failed at blob index {failed_index}")]
RevocationPartial { failed_index: usize },
/// The requested outgoing share row does not exist.
#[error("share not found")]
ShareNotFound,
/// A share cannot be revoked because it is already revoked.
#[error("share already revoked")]
ShareAlreadyRevoked,
/// Strong revocation attempted on a file with no re-encryption candidates.
#[error("no active shares to rotate for file_share_id")]
NoActiveSharesForRotation,
```

**CS-004 — `sharing::cloud` module API** (new file `src-tauri/src/sharing/cloud.rs`)
```rust
/// Return value of [`create_share`]: identifiers and the `.vgshare` wire bytes.
pub struct CreateShareOutput {
    pub share_id: String,
    pub file_share_id: String,
    pub wire_bytes: Vec<u8>,
}

/// Orchestrates blob copy (if needed) + share-package creation + `shares` row insert.
pub async fn create_share(
    file_id: Uuid,
    contact_id: ContactId,
    expires_at: Option<i64>,
    owner_cloud_endpoint: CloudEndpoint,
    now_unix_seconds: i64,
    metadata_store: &dyn MetadataStore,
    sharing_store: &dyn SharingStore,
    cloud: &dyn CloudTransport,
    key_encryption_key: &KeyEncryptionKey,
    staging_dir: &Path,
) -> Result<CreateShareOutput, SharingError>;

/// Downloads and decrypts a received share to a local destination file.
pub async fn fetch_received_share_to_local(
    share_id: &str,
    destination_file: &Path,
    sharing_store: &dyn SharingStore,
    cloud: &dyn CloudTransport,
    key_encryption_key: &KeyEncryptionKey,
    staging_dir: &Path,
) -> Result<(), SharingError>;
```

**CS-005 — `sharing::revocation` module API** (new file `src-tauri/src/sharing/revocation.rs`)
```rust
/// Output of [`strong_revoke_share`] — caller delivers `reissued_packages` out-of-band.
pub struct StrongRevocationOutput {
    pub new_file_share_id: String,
    pub reissued_packages: Vec<(ContactId, Vec<u8>)>,  // (recipient, new .vgshare bytes)
}

/// Default cooperative revocation. If this is the last active share for the
/// `file_share_id`, deletes every shared blob; otherwise only flags the share.
pub async fn revoke_share(
    share_id: &str,
    now_unix_seconds: i64,
    sharing_store: &dyn SharingStore,
    cloud: &dyn CloudTransport,
) -> Result<(), SharingError>;

/// Strong revocation: rotate `file_key`, re-encrypt vault blobs, republish under
/// a fresh `file_share_id`, re-issue packages for the remaining active
/// recipients, and delete the old shared folder.
pub async fn strong_revoke_share(
    revoked_share_id: &str,
    owner_cloud_endpoint: CloudEndpoint,
    now_unix_seconds: i64,
    metadata_store: &dyn MetadataStore,
    sharing_store: &dyn SharingStore,
    cloud: &dyn CloudTransport,
    key_encryption_key: &KeyEncryptionKey,
    staging_dir: &Path,
) -> Result<StrongRevocationOutput, SharingError>;
```

**CS-006 — `SqlCipherMetadataStore::replace_file_key_and_chunks` helper** (new, `src-tauri/src/storage/sqlcipher.rs`; SQLCipher-specific, **not** added to `MetadataStore` trait)
```rust
pub(crate) async fn replace_file_key_and_chunks(
    &self,
    file_id: Uuid,
    new_wrapped_file_key: [u8; 72],
    new_chunks: &[ChunkRecord],
) -> Result<(), StorageError>;
// Single transaction:
//   1. SELECT blob_name FROM chunks WHERE node_id = ?1
//   2. INSERT OR IGNORE INTO pending_deletions (blob_name, queued_at) ... for each old blob_name
//   3. DELETE FROM chunks WHERE node_id = ?1
//   4. UPDATE nodes SET file_key_wrapped = ?2 WHERE node_id = ?1
//   5. INSERT new chunks
```

**CS-007 — `storage::vault_ops::reencrypt_file` signature** (new file `src-tauri/src/storage/vault_ops/reencrypt_file.rs`)
```rust
/// Re-encrypts every chunk of an existing vault file under a fresh `file_key`
/// and rewrites the manifest. Old `blob_name`s are enqueued into
/// `pending_deletions` via the SQLCipher helper [CS-006].
///
/// Returns the new chunk records so sharing-layer callers can copy the new
/// blobs into a fresh `shared/<file_share_id>/` folder.
pub async fn reencrypt_file(
    file_id: Uuid,
    now_unix_seconds: i64,
    sqlcipher_store: &SqlCipherMetadataStore,
    cloud: &dyn CloudTransport,
    key_encryption_key: &KeyEncryptionKey,
    staging_dir: &Path,
) -> Result<Vec<ChunkRecord>, StorageError>;
```

**CS-008 — Shared blob path formatting**
```
shared/<file_share_id>/<blob_uuid>.blob
```
`file_share_id` is the hyphenated UUID v4 string; `blob_uuid` is the existing (or newly generated during re-encryption) chunk `blob_name`.

### Steps

**S-1 — `SharingError` variants** (CS-003). Add five variants to `src-tauri/src/sharing/error.rs` with `#[cfg(test)]` display-message tests (one per variant, rule `.claude/rules/rust.md`).

**S-2 — `ShareRecord` + `SharingStore` trait extension** (CS-001, CS-002). Add the struct and the six trait methods to `src-tauri/src/sharing/store.rs`.

**S-3 — SQLCipher `shares` CRUD** in `src-tauri/src/storage/sharing.rs`. Implement the six new `SharingStore` methods against the existing `shares` table DDL (already in `schema.rs`). SQL:
- Insert: `INSERT INTO shares (share_id, file_id, contact_id, file_share_id, cloud_path, created_at, expires_at, revoked_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL)`.
- Get: single-row `SELECT ... WHERE share_id = ?1`, map via new `map_share_row` helper; NotFound → `SharingError::ShareNotFound`.
- `list_shares_by_file`: all rows for a file_id ordered by `created_at DESC, share_id ASC`.
- `list_active_shares_by_file`: `WHERE file_id = ?1 AND revoked_at IS NULL`.
- `list_active_shares_by_file_share_id`: `WHERE file_share_id = ?1 AND revoked_at IS NULL`.
- `set_share_revoked_at`: `UPDATE shares SET revoked_at = ?2 WHERE share_id = ?1 AND revoked_at IS NULL`; 0 rows affected and `revoked_at IS NOT NULL` → `ShareAlreadyRevoked`; 0 rows affected and row missing → `ShareNotFound`.
- All SQL runs inside `with_connection_blocking`; use `is_uuid_v4_string` (existing helper) to reject non-UUID-v4 `share_id` / `file_share_id` strings before SQL.
- Map `StorageError::ConstraintViolation` to `SharingError::ConstraintViolation` for duplicate `share_id`.

**S-4 — SQLCipher re-encryption helper** (CS-006). Add `replace_file_key_and_chunks` to `SqlCipherMetadataStore` in `src-tauri/src/storage/sqlcipher.rs`, following the same `with_connection_blocking` pattern as `rollback_snapshot_counter` / `list_sync_chunks`. Use a single transaction (`BEGIN; …; COMMIT;`). Queue old `blob_name`s with `queued_at = now_unix_seconds` (parameter to the helper).

**S-5 — Vault re-encryption entry point** (CS-007). Create `src-tauri/src/storage/vault_ops/reencrypt_file.rs` and wire it into `src-tauri/src/storage/vault_ops/mod.rs` (add `mod reencrypt_file; pub use reencrypt_file::reencrypt_file;`).
- Read existing `(Node, Vec<ChunkRecord>)` via `MetadataStore::get_node` + `get_chunks`.
- Unwrap old `file_key` via `crypto::unwrap_file_key`.
- Generate new `FileKey` via `crypto::generate_file_key`; wrap via `crypto::wrap_file_key`.
- Loop per chunk: download `vault/<old_blob_name>.blob` into staging → `compute_checksum` + `verify_checksum` → `decrypt_chunk(VerifiedBlob, &old_key, &FileId(node_id_bytes), ChunkIndex(i))` (plaintext held in `Zeroizing<Vec<u8>>`) → `encrypt_chunk(plaintext, &new_key, &FileId, ChunkIndex(i))` → new blob UUID → upload to `vault/<new_blob_name>.blob` → stage deletion old file → compute new `blake3_checksum`.
- After loop, call `SqlCipherMetadataStore::replace_file_key_and_chunks`.
- Return the new `Vec<ChunkRecord>`.
- AAD binding: `file_id || chunk_index` uses the **original** `file_id` (invariant #1); the re-encryption never changes `file_id`.

**S-6 — `sharing::cloud::create_share`** (CS-004). Create `src-tauri/src/sharing/cloud.rs`.
- Look up `list_active_shares_by_file(file_id)`:
  - If any row exists, reuse `file_share_id = rows[0].file_share_id` and **skip** blob copy.
  - Else generate `file_share_id = Uuid::new_v4().hyphenated().to_string()` and perform the blob copy loop per Assumption §4.3 (temporary file in `staging_dir`; path validated by `validate_remote_path` during `CloudTransport` use).
- Compose `cloud_endpoint_json = serde_json::to_value(&CloudEndpoint { path_prefix: format!("shared/{file_share_id}/"), ..owner_cloud_endpoint.clone() })?`.
- Call `packages::create_share_package(file_id, recipient_public_key, expires_at, cloud_endpoint_json, metadata_store, sharing_store, kek)`; recipient public key is read from `sharing_store.get_contact(contact_id)?.public_key`.
- Build and insert a `ShareRecord` via `SharingStore::insert_share`.
- Return `CreateShareOutput`.
- Cloud failures map to `SharingError::CloudOperation(stringified)`; storage failures map via existing patterns.

**S-7 — `sharing::cloud::fetch_received_share_to_local`** (CS-004). Same file. Per Assumption §4.9 + §4.10:
- Load `ReceivedShare` via `SharingStore::get_received_share`.
- Parse `file_share_id` from `cloud_endpoint["path_prefix"]` (trim prefix `shared/` and trailing `/`); reject otherwise with `SharingError::MalformedSharePackage`.
- Unwrap `file_key` via `crypto::unwrap_file_key(&WrappedFileKey(row.file_key_wrapped), kek)`; `FileId` = 16-byte big-endian UUID derived from `file_id` or constructed from a stored per-receipt `file_id`? The `received_shares` row does **not** currently store `file_id` — **sub-sub-concern**: but `SharePackagePayload` carries `file_id`, which is placed into the `received_shares` row... actually it is **not**: re-read `src-tauri/src/sharing/store.rs` `ReceivedShare` — it has no `file_id` column. The AAD for recipient-side decrypt therefore cannot be reconstructed. **Resolution in S-7**: invariant #1 requires AAD `file_id || chunk_index`. Recipient-side decryption needs `file_id` bytes identical to the sender's. Add a new column `file_id TEXT NOT NULL` to the `received_shares` DDL **and** domain struct, populated from `payload.file_id` on import. Because this also touches Phase 5.2 code, it is recorded as **Governance sync action GS-004** and a schema diff. Tests in Phase 5.2 are updated to populate/verify `file_id`.
- Actually this is a schema gap discovered during planning — see §3 row added (finalised below) and §8 GS-004.
- For each `(chunk_index, uuid)` in `received_share.chunk_uuids.iter().enumerate()`:
  - Download `shared/<file_share_id>/<uuid>.blob` into staging file.
  - Read file → `compute_checksum` → `verify_checksum` (trivially passes) → `decrypt_chunk(verified, &file_key, &FileId(file_id_bytes), ChunkIndex(chunk_index as u32))`.
  - `tokio::io::AsyncWriteExt::write_all` plaintext to destination file (`BufWriter`, streaming; rule `.claude/rules/rust.md` I/O).
  - `tokio::fs::remove_file(staging_blob)`.
- Drop `FileKey` (ZeroizeOnDrop handles zeroization).

**S-8 — Add `file_id` to `received_shares` row and schema** (discovered in S-7). Extend CS-007 of Phase 5.2 to add column `file_id TEXT NOT NULL`. Update `src-tauri/src/storage/schema.rs`, `ReceivedShare` struct in `src-tauri/src/sharing/store.rs`, `import_share_package` in `src-tauri/src/sharing/packages.rs` (populate from `payload.file_id`), and `src-tauri/src/storage/sharing.rs` (INSERT + SELECT + map). Update Phase 5.2 round-trip tests to verify `file_id` round-trips. This is a **corrective schema change** enabling Phase 5.3 recipient decrypt; recorded as GS-004.

**S-9 — `sharing::revocation::revoke_share`** (CS-005). Create `src-tauri/src/sharing/revocation.rs`.
- Load target share. If `revoked_at.is_some()` → `ShareAlreadyRevoked`.
- `siblings = sharing_store.list_active_shares_by_file_share_id(share.file_share_id)`. If `siblings.len() > 1` (target + others) → cooperative path: call `set_share_revoked_at(share_id, now_unix_seconds)` and return.
- Else single-active path: reconstruct `chunk_uuids` for this file via `metadata_store.get_chunks(file_id_parsed_from_share.file_id)`; for each chunk, call `cloud.delete_blob(format!("shared/{file_share_id}/{blob_name}.blob"))` sequentially. On first error, return `SharingError::RevocationPartial { failed_index }` **without** mutating the row.
- After the loop completes successfully, call `set_share_revoked_at(share_id, now_unix_seconds)`.

**S-10 — `sharing::revocation::strong_revoke_share`** (CS-005). Same file.
- Load target share; `file_id = Uuid::parse_str(&share.file_id)?`.
- Fetch `remaining_active = list_active_shares_by_file_share_id(share.file_share_id)`.
- Call `storage::vault_ops::reencrypt_file(file_id, now_unix_seconds, sqlcipher_store, cloud, kek, staging_dir) -> Vec<ChunkRecord>` (the concrete type is needed because `replace_file_key_and_chunks` is SQLCipher-specific; pass `&SqlCipherMetadataStore`, not `&dyn MetadataStore`).
- `new_file_share_id = Uuid::new_v4().hyphenated().to_string()`; perform blob copy loop into `shared/<new_file_share_id>/` (same logic as `create_share` blob copy step, against the new chunk records).
- For each still-active recipient in `remaining_active` (excluding the one being revoked): call `packages::create_share_package(...)` to produce fresh `.vgshare` bytes; insert a new `shares` row for that recipient under the new `file_share_id`.
- Sequentially delete every blob under `shared/<share.file_share_id>/` (old prefix) via `delete_blob`. Partial failure → `RevocationPartial`.
- `set_share_revoked_at(old_share_id, now_unix_seconds)` for every row in `remaining_active` **and** the originally-targeted share.
- Return `StrongRevocationOutput { new_file_share_id, reissued_packages }`.

**S-11 — Module surface updates** (`src-tauri/src/sharing/mod.rs`).
```rust
#[allow(dead_code)] // TODO(phase-6): remove when Tauri commands consume cloud
mod cloud;
#[allow(dead_code)] // TODO(phase-6): remove when Tauri commands consume revocation
mod revocation;
#[allow(unused_imports)]
pub(crate) use cloud::{create_share, fetch_received_share_to_local, CreateShareOutput};
#[allow(unused_imports)]
pub(crate) use revocation::{revoke_share, strong_revoke_share, StrongRevocationOutput};
pub use store::ShareRecord;
```

**S-12 — Tests** — (~`100` LoC per sub-phase estimate). Per file:
- `sharing/cloud.rs` (`#[cfg(test)] mod tests`):
  - Blob copy produces `shared/<file_share_id>/<uuid>.blob` for every chunk UUID (via `MockCloudTransport::list_blobs`).
  - Second share of same file reuses `file_share_id` and does not re-copy blobs (check list_blobs before and after).
  - `create_share` inserts `shares` row with correct `file_share_id`, `cloud_path`, `revoked_at = NULL`.
  - `fetch_received_share_to_local` produces byte-identical plaintext across a create→import→copy→fetch round trip (uses `MockCloudTransport` shared between owner and recipient).
- `sharing/revocation.rs`:
  - Cooperative path: multiple active shares → target share is the only one `revoked_at` is set for; other shares untouched; blobs remain in cloud (verified via `list_blobs`).
  - Last-active path: deletes every shared blob; blobs absent from cloud; `revoked_at` set on target share.
  - Partial failure: inject `CloudTransportErrorKind::Timeout` on second blob via `MockCloudTransport::inject_failure`; assert `RevocationPartial { failed_index: 1 }`; assert `revoked_at` is still NULL on the share row.
  - Retry: after clearing the failure, calling `revoke_share` a second time completes and the already-deleted blob (first one) is a no-op (idempotent).
  - Strong revocation: old blobs absent; new blobs present under new `file_share_id`; `reissued_packages` count equals `remaining_active.len() - 1` (excludes revoked recipient); attempting to decrypt the old package with the old `file_key` against the new blobs fails (AEAD auth failure); the target share and all old-`file_share_id` siblings are `revoked_at`.
- `storage/sharing.rs`:
  - `insert_share` → `get_share` round-trip preserves fields.
  - `list_active_shares_by_file` excludes revoked rows.
  - `set_share_revoked_at` on an already-revoked row → `ShareAlreadyRevoked`.
  - `set_share_revoked_at` on a missing `share_id` → `ShareNotFound`.
  - Duplicate `share_id` insert → `ConstraintViolation`.
- `storage/sqlcipher.rs`:
  - `replace_file_key_and_chunks` updates `nodes.file_key_wrapped`, deletes old chunks, inserts new chunks, and enqueues old `blob_name`s into `pending_deletions` — all inside one transaction (assert via `list_pending_deletions`).
- `storage/vault_ops/reencrypt_file.rs`:
  - After `reencrypt_file`, decrypting a `vault/<new_blob_name>.blob` with the **old** `file_key` fails (acceptance criterion); decrypting with the new `file_key` succeeds and recovers original plaintext.

## 6. Review focus areas

### 6a. Rust change surface

- `src-tauri/src/sharing/cloud.rs` *(new)*
- `src-tauri/src/sharing/revocation.rs` *(new)*
- `src-tauri/src/sharing/store.rs` *(add `ShareRecord` + 6 `SharingStore` methods; add `file_id` to `ReceivedShare`)*
- `src-tauri/src/sharing/error.rs` *(5 new variants + display tests)*
- `src-tauri/src/sharing/mod.rs` *(module + re-export updates)*
- `src-tauri/src/sharing/packages.rs` *(populate new `file_id` on import)*
- `src-tauri/src/storage/sharing.rs` *(shares CRUD + `received_shares` `file_id` wiring)*
- `src-tauri/src/storage/schema.rs` *(add `file_id TEXT NOT NULL` to `received_shares`)*
- `src-tauri/src/storage/sqlcipher.rs` *(`replace_file_key_and_chunks` helper)*
- `src-tauri/src/storage/vault_ops/reencrypt_file.rs` *(new)*
- `src-tauri/src/storage/vault_ops/mod.rs` *(re-export)*

### 6b. Security-sensitive paths

- `src-tauri/src/sharing/cloud.rs` — staging file cleanup on all failure branches; no plaintext materialisation during blob copy (only opaque ciphertext passes through the staging file); on recipient-side decrypt the unwrapped `FileKey` must live only inside a tight scope and drop (ZeroizeOnDrop) before function return; plaintext chunks held in `Zeroizing<Vec<u8>>`; destination file write uses `BufWriter` streaming (no whole-file buffer).
- `src-tauri/src/sharing/revocation.rs` — `set_share_revoked_at` executed **only** after all blob deletes succeed (atomicity-by-ordering); strong path re-encryption uses fresh CSPRNG `file_key` (verified via test that old-key decrypt fails on new blobs); old `file_key` variable is confined to `reencrypt_file` scope and zeroized on drop.
- `src-tauri/src/storage/vault_ops/reencrypt_file.rs` — chunk AAD remains `file_id || chunk_index` (invariant #1); new nonces per chunk are random 24-byte CSPRNG (existing `encrypt_chunk` behaviour); plaintext buffers are `Zeroizing<Vec<u8>>`; BLAKE3 checksum over new ciphertext is computed before the manifest replace (rule `.claude/rules/storage.md` BLAKE3).
- `src-tauri/src/sharing/store.rs` / `storage/sharing.rs` — `ShareRecord` and `ReceivedShare` do not embed secrets; `Debug` emission must not print `public_key` bytes (existing `X25519PublicKey` redaction already enforced).
- `src-tauri/src/storage/sqlcipher.rs::replace_file_key_and_chunks` — single transaction semantics verified by integrity test; old `file_key_wrapped` not lingering anywhere after UPDATE.

### 6c. Architecture risk areas

- **Trait boundary discipline** — `shares` CRUD must go on `SharingStore` only, never `MetadataStore` (rule `.claude/rules/sharing.md` Trait boundaries, rule `.claude/rules/storage.md` Traits). Verify imports in `src-tauri/src/storage/sharing.rs` and absence of new methods on `MetadataStore` in `src-tauri/src/storage/metadata_store.rs`.
- **SQLCipher-specific helper exception** — `replace_file_key_and_chunks` is a sibling of `rollback_snapshot_counter` / `list_sync_chunks` (rule `.claude/rules/storage.md` Cloud backup / Traits). Must remain on `SqlCipherMetadataStore`, not on `MetadataStore`. Document in GS-002.
- **Module visibility** — `sharing::cloud` and `sharing::revocation` are crate-internal during 5.3 (Tauri wiring arrives in Phase 6). Re-exports gated under `#[allow(dead_code)]` / `#[allow(unused_imports)]` per existing Phase 5.2 pattern.
- **SRP split** — vault-level decrypt/re-encrypt belongs in `storage::vault_ops::reencrypt_file`; sharing-level rotation orchestration belongs in `sharing::revocation::strong_revoke_share`. Do not merge.
- **Dependency direction** — `sharing` may import from `crypto`, `storage` (including concrete `SqlCipherMetadataStore` for the re-encryption call); `storage::vault_ops::reencrypt_file` **may not** import from `sharing`. Verify.
- **Path allowlist** — every cloud path constructed in `sharing::cloud` and `sharing::revocation` (`shared/<uuid>/<uuid>.blob`) must pass `validate_remote_path` at the `CloudTransport` boundary (rule `.claude/rules/storage.md` Cloud backup). Tests confirm with `MockCloudTransport` (which does not enforce the allowlist), so add an explicit unit test feeding a constructed path through `storage::cloud::remote_path::validate_remote_path` to assert acceptance.

### 6d. Testing requirements

**Validation-checkpoint commands (sub-phase §Validation Checkpoint):**
```bash
cargo test sharing::cloud
cargo test sharing::revocation
cargo test --workspace --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
```

**Boundary cases and acceptance criteria** (sub-phase §Deliverables 8 + §Acceptance criteria):
- Blob copy path structure correct for N chunks (N = 1 and N = 3 cases).
- `shares` insert fields verified (file_share_id, cloud_path, revoked_at=NULL).
- Revocation last-active path: every listed `chunk_uuids` blob is removed; `revoked_at` set exactly once.
- Partial-failure revocation: `revoked_at` unchanged; error is `RevocationPartial { failed_index }`.
- Cooperative (sibling-present) revocation: cloud blobs untouched; only target share flagged.
- Re-encryption flow: new blobs in `vault/<new_uuid>.blob`; old blobs in `pending_deletions`; old-key decrypt fails; new-key decrypt recovers plaintext; `nodes.file_key_wrapped` updated.
- Strong revocation: new `shared/<new_file_share_id>/` populated, old folder gone, `reissued_packages.len() == remaining_active_recipients_excluding_revoked`, all old shares revoked.
- Recipient fetch round-trip: create→import→fetch produces byte-identical plaintext as the original file.
- Recipient fetch never persists raw `file_key` (verified by scope review; `FileKey` is `ZeroizeOnDrop` and held inside the fetch function).
- Strong-revoked share package: attempting to decrypt new blobs with the old package's `file_key` yields an AEAD auth error (not a silent mis-decrypt).

## 7. Documentation impact

- **Required this run**:
  - None (design.md is already canonical; schema.rs and rule files are edited through governance sync actions GS-001/GS-002/GS-003/GS-004; sub-phase design-doc content is unchanged).
- **Deferred/optional**:
  - `docs/architecture/designs/file-sharing/diagrams/file-sharing-flow.md`: add cloud-layout + revocation sub-diagrams covering `create_share` and `strong_revoke_share`. Rationale: can be drawn once the full owner-side flow is implemented; deferred to avoid partial diagrams. Revisit at Phase 6 UI wiring.
  - `docs/threat-model/` (mentioned in sub-phase completion list): fold the threat-model additions in design lines 419-435 into a dedicated doc. No `docs/threat-model/` directory exists today; the sub-phase treats this as a Phase 5 completion deliverable, not a 5.3 deliverable. Defer until the file-sharing work is fully merged.
  - `docs/roadmap.md` Phase 5 completion marker: mark after 5.3 lands and integration tests pass. Deferred — not a 5.3 code deliverable.
  - Recipient-side Rclone remote registration for foreign buckets (Concern 5): add a design note describing the UX and config flow. Deferred to Phase 6.

## 8. Governance sync actions (pre-implementation)

| Action ID | Reason / linked concern | Target files | Required edit | Verification |
|---|---|---|---|---|
| GS-001 | Concern 1: lock outgoing `shares` CRUD onto the `SharingStore` trait. | `C:\Users\chris\source\repos\arx-runa\.claude\rules\storage.md` (Traits section) | Append a bullet: "`shares` CRUD lives in `storage::sharing` behind the `SharingStore` trait in `sharing::store`, mirroring the `contacts` + `received_shares` pattern; it must not be added to `MetadataStore`." | Grep for the new bullet; run `/copilot-sync` (GS-005). |
| GS-002 | Concern 2: document the re-encryption SQLCipher-specific helper as an authorised exception to `MetadataStore`. | `C:\Users\chris\source\repos\arx-runa\.claude\rules\storage.md` (Traits / Cloud backup section) | Append a bullet: "`replace_file_key_and_chunks` is a SQLCipher-specific helper on `SqlCipherMetadataStore` (not on `MetadataStore`); used by sharing re-encryption and must run in a single transaction that enqueues old `blob_name`s into `pending_deletions`." | Grep for the new bullet; `cargo check` after wiring; `/copilot-sync` (GS-005). |
| GS-003 | Concern 4: record the revocation sequential-delete + retry contract in the sharing rules. | `C:\Users\chris\source\repos\arx-runa\.claude\rules\sharing.md` (new "## Revocation contract" section) | Add section with two bullets: "Revocation blob deletion runs as a sequential loop; on failure, `SharingError::RevocationPartial { failed_index }` is returned with `shares.revoked_at` unchanged so the operation can be retried." "Strong revocation rotates `file_key` and `file_share_id` atomically at the manifest layer (`replace_file_key_and_chunks`) before shared-folder cleanup." | Grep for the new section header; `/copilot-sync` (GS-005). |
| GS-004 | S-8 schema gap: `received_shares` needs `file_id` to support recipient-side AAD reconstruction during `decrypt_chunk`. | `C:\Users\chris\source\repos\arx-runa\src-tauri\src\storage\schema.rs`, `C:\Users\chris\source\repos\arx-runa\src-tauri\src\sharing\store.rs`, `C:\Users\chris\source\repos\arx-runa\src-tauri\src\sharing\packages.rs`, `C:\Users\chris\source\repos\arx-runa\src-tauri\src\storage\sharing.rs`, `C:\Users\chris\source\repos\arx-runa\docs\architecture\designs\file-sharing\design.md` §Database Schema | Add `file_id TEXT NOT NULL` column to `received_shares` DDL. Add `file_id: String` field to `ReceivedShare` struct. Populate it from `payload.file_id` in `import_share_package`. Update INSERT/SELECT/mapping in `storage::sharing`. Update design doc `received_shares` table to list `file_id TEXT NOT NULL` (place after `share_id` / `sender_contact_id`). Update Phase 5.2 round-trip tests to assert `received.file_id == payload.file_id`. | Run `cargo test --workspace --all-targets --all-features` — Phase 5.2 round-trip tests must pass with the new field. Grep `received_shares` in `schema.rs` to confirm the new column. |
| GS-005 | Fan out rule edits from GS-001/GS-002/GS-003 to Copilot instruction mirrors. | N/A | Run `/copilot-sync` **after** GS-001–GS-003 complete and **before** Rust code. | `.github/instructions/sharing.instructions.md` and `.github/instructions/storage.instructions.md` diffs match the new rules. |

## 9. Handoff Notes for Implementer

Working directory: `C:\Users\chris\source\repos\arx-runa`. Execute governance-sync actions in order: GS-001 → GS-002 → GS-003 → GS-004 (schema + code touch points) → GS-005 (`/copilot-sync`). Then implement Section 5 Steps in order S-1 → S-2 → S-3 → S-4 → S-5 → S-6 → S-7 → S-8 (if not already covered by GS-004) → S-9 → S-10 → S-11 → S-12 (tests interleaved per file). The plan is self-contained — do not re-read the sub-phase unless contract ambiguity arises. Platform traps: staging directory path uses `storage::staging` helpers which already handle Windows/macOS/Linux parity; do not introduce new path-manipulation primitives. `MockCloudTransport` (feature `test-utils`) is required in tests; import via `use crate::storage::cloud::mock::MockCloudTransport;`. Never log `blob_name`s, `file_share_id`, `file_key`, or `cloud_endpoint` contents in any path. The strong-revocation test must verify that decrypting a new blob with the old `FileKey` fails with an AEAD auth error — this is the acceptance-criterion test for re-encryption correctness. Recipient-side `CloudTransport` construction against the owner's foreign bucket is deferred to Phase 6 UI wiring; tests exercise only `MockCloudTransport`. Invoke `security-reviewer` agent after `cargo test --workspace --all-targets --all-features` and `cargo clippy --all-targets --all-features -- -D warnings` both pass.
