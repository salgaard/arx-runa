---
title: "Phase 5.1 — Sharing Module Surface for X25519 Identity and Contacts"
created: "2026-04-20T00:00:00Z"
status: implemented
roadmap-phase: 5
sub-phase: "5.1"
design-document: docs/architecture/designs/file-sharing/design.md
sub-phase-roadmap: docs/architecture/designs/file-sharing/sub-phases/roadmap.md
governance-sync-required: true
tags: [phase-5, sharing, identity, contacts, sqlcipher]
---

# Plan: Phase 5.1 — Sharing Module Surface for X25519 Identity and Contacts

## 1. Goal

Introduce a new `sharing` module that exposes the already-generated X25519 identity and provides contact CRUD against the existing SQLCipher `contacts` table, via a `SharingStore` trait implemented on `SqlCipherMetadataStore`.

## 2. Context

**Sub-phase**: 5.1 of the file-sharing design. Depends on Phase 1.3 (`wrap_file_key`), Phase 2.4 (X25519 keypair creation during `create_vault`), and Phase 3.1 (SQLCipher schema including `contacts`, `shares`, `received_shares`, `vault_identity`).

**Sub-roadmap deliverables** (from `docs/architecture/designs/file-sharing/sub-phases/5.1-identity-and-contacts.md`):
1. X25519 keypair generation in `src-tauri/src/sharing/identity.rs`.
2. Wrap the X25519 private key with `key_encryption_key` via `wrap_file_key` and store in SQLCipher.
3. Public key export: 32-byte binary file + base64 QR data.
4. Contact import with `display_name` (required), `email` (optional), `public_key` (32 bytes) into `contacts` table.
5. Fingerprint display: first 16 lowercase hex chars of `SHA-256(public_key)`.
6. `SharingStore` trait in `src-tauri/src/sharing/store.rs` with methods `get_own_public_key`, `insert_contact`, `get_contact`, `list_contacts`, `delete_contact`.
7. Tests covering keypair shape, wrap/unwrap round-trip, export file size, contact CRUD round-trip, list, delete, fingerprint format.

**State today** (verified, not assumed):
- `x25519-dalek` v2 with `static_secrets` is already declared in `src-tauri/Cargo.toml`.
- `create_vault` (`src-tauri/src/auth/ceremonies/create.rs:123-147`) already generates the X25519 keypair via `StaticSecret::random_from_rng(OsRng)`, wraps the private key with `wrap_with_session_kek` (thin wrapper over `crypto::wrap_file_key`), and inserts `(1, public_key_bytes, wrapped_private_key)` into `vault_identity`.
- `change_password` and `rotate_key_file` already re-wrap the `vault_identity.wrapped_private_key` during key rotation and preserve the public key bytes (rotation test at `src-tauri/src/auth/ceremonies/rotate_key_file.rs:323`).
- `recover_vault` restores the full SQLCipher DB from the manifest backup; `vault_identity` travels inside the encrypted backup blob, so no explicit recovery-path code is required.
- `storage/schema.rs` already defines `contacts`, `shares`, `received_shares`, and `vault_identity` tables (see CS-007 below).
- `base64` v0.22 and `sha2` v0.11 are already dependencies. No new Cargo entries are needed for 5.1.
- No `src-tauri/src/sharing/` module exists yet.
- The global invariant ledger (`docs/architecture/design-invariants.md`) already carries invariant 11 ("Share package/import key-handling contract") but does not yet record a separate invariant for the identity row.

**Pending architectural decisions**: none tracked for this sub-phase. Open decisions in the parent design (enterprise key distribution, fingerprint UX placement, folder sharing UX) are deferred by the sub-roadmap.

## 3. Design Concerns / Open Questions

| # | Concern | Source | Impact | Classification | Resolution | Documentation updates |
|---|---------|--------|--------|----------------|------------|-----------------------|
| C-1 | Deliverables 1 and 2 describe keypair generation and wrapping that already landed in Phase 2.4 (`create_vault` → `vault_identity`). A literal reading would lead an implementer to create a duplicate generation path in `sharing::identity`. | `5.1-identity-and-contacts.md` Deliverables §1–§2 vs. `create.rs:123-147` and `schema.rs:110` | Duplicate keypair generation would break the single-identity-per-vault invariant: `vault_identity.id` is `CHECK (id = 1)`, so duplicate insert errors out, but if a new path ran after creation it would clobber the already-wrapped private key or orphan the public key in the `contacts` table. Also wastes implementer time. | Non-blocking | Treat 5.1 as an exposure layer. The sharing module **reads** `vault_identity` and exports the public key / computes fingerprints; it does **not** generate, insert, or mutate the identity row. Generation stays exclusively in `auth::ceremonies::create::create_vault`. Record as Assumption A-1. | Update `docs/architecture/designs/file-sharing/sub-phases/5.1-identity-and-contacts.md` after implementation to note the generation delegation to Phase 2.4. Add an invariant line to `docs/architecture/design-invariants.md` stating the single-owner vault_identity rule. Required this run. |
| C-2 | SharingStore method surface does not include access to `wrapped_private_key`, but Phase 5.2 (HPKE Open) will need it. | `5.1-identity-and-contacts.md` Deliverable §6 | If we omit the hook now, 5.2 will force an API change to SharingStore right after it lands. | Non-blocking | Expose `get_own_public_key` in 5.1 as specified; defer `get_own_wrapped_private_key` to 5.2 where the HPKE-open code actually consumes it. 5.2 will extend the trait without needing to retrofit storage access patterns. Record as Assumption A-2. | Note deferral in Section 7. No doc change required this run. |
| C-3 | `contacts` schema has no UNIQUE constraint on `public_key` or `display_name`. The sub-phase does not specify whether re-importing the same public key must reject, upsert, or produce a second contact row. | `design.md` Database Schema §contacts vs. Deliverables §4 | Ambiguous behaviour on duplicate imports. A "second Alice" contact row with a different `display_name` but the same key is quietly accepted today — users may treat them as different identities. | Non-blocking | `insert_contact` maps `contact_id` primary-key collision to `ConstraintViolation`. For duplicate `public_key` with a different `contact_id`, accept the row (current schema semantics) but surface this in a tracked follow-up (C-3 carry-over). Record as Assumption A-3. | Add an "Open: contact deduplication by public_key" entry to the parent design's `Open Decisions` table for 5.3 or 6 to resolve. Deferred — rationale: design decision not yet made, should not block code landing. |
| C-4 | SharingStore connection ownership. The implementation note says "both [MetadataStore and SharingStore] can share the same SQLCipher connection," but the concrete strategy is unspecified. `SqlCipherMetadataStore` currently owns `Arc<Mutex<Connection>>` privately. | `5.1-identity-and-contacts.md` Implementation Notes | A separate `SqlCipherSharingStore` struct would require a second keyed connection (double-open races against SQLCipher) or an exported `Arc<Mutex<Connection>>` (leaks internals). | Non-blocking | Implement `SharingStore` as a second trait on the existing `SqlCipherMetadataStore` type, with the trait defined in `sharing::store` and the impl block in a new `storage::sharing` file. Same connection, same `with_connection_blocking` helper, no new open path. Record as Assumption A-4. | None. Follows existing pattern for `destination_session` helpers. |
| C-5 | Public-key export file encoding. Deliverable §3 says "small binary file" and acceptance criteria require "exactly 32 bytes", while the same deliverable requires "QR code data (base64-encoded public key bytes)". A reader could conflate the two. | `5.1-identity-and-contacts.md` Deliverables §3 / Acceptance criteria | Risk of emitting base64-wrapped file instead of raw 32 bytes, breaking recipient-side import. | Non-blocking | Two distinct outputs: `export_public_key_bytes()` returns `[u8; 32]` (raw, for file export) and `public_key_qr_string()` returns base64 standard-alphabet string without padding removed. Record as Assumption A-5. | None. |
| C-6 | No explicit "does the sub-phase touch security-critical code" claim is made, but 5.1 clearly does: it handles the owner X25519 public key and delegates private-key wrapping. The sub-phase correctly calls for `security-reviewer`; verify invocation is honoured. | `5.1-identity-and-contacts.md` Security Review §1 | A missed security review on identity material could let a bug escape (e.g., fingerprint over wrong bytes, log-leak of public key). | Non-blocking | Section 6b lists `src-tauri/src/sharing/identity.rs` and the storage `SharingStore` impl as security-sensitive; `/implement-plan` must route to `security-reviewer`. No deviation. | None. |
| C-7 | Sub-phase states the private key must be "held in `zeroize::Zeroizing<[u8; 32]>`". Phase 5.1 does not unwrap the private key (no HPKE-open until 5.2), so there is no actual unwrap site in 5.1. | `5.1-identity-and-contacts.md` Security Review §1 | None in 5.1. 5.2 will own the zeroization contract when it introduces HPKE Open. | Non-blocking | Do not introduce a speculative unwrap helper in 5.1. Tests may cover the unwrap round-trip using existing `crypto::unwrap_file_key` directly. Record as Assumption A-6. | Restate in 5.2 plan. |

## 4. Assumptions

- **A-1** — Phase 5.1 does **not** generate, insert, or mutate the `vault_identity` row. Identity generation stays in `auth::ceremonies::create::create_vault` (already implemented). The sharing module consumes a read-only view (`get_own_public_key`).
- **A-2** — `SharingStore` in 5.1 returns only the 32-byte public key. Access to `wrapped_private_key` is deferred to 5.2 and will be added as a new method (tentatively `get_own_wrapped_private_key`) at that time.
- **A-3** — Duplicate-`public_key` contact inserts are **accepted** (current schema allows it). `contact_id` collisions map to `SharingError::ConstraintViolation`. Deduplication policy is tracked as an open design question for a future sub-phase.
- **A-4** — `SqlCipherMetadataStore` implements `SharingStore` directly; the trait lives in `sharing::store` and the impl lives in `storage::sharing`. Both traits share the same `Arc<Mutex<Connection>>` via the existing `with_connection_blocking` helper.
- **A-5** — Public-key export produces two distinct outputs: raw 32-byte slice for file export and standard-padded base64 string for QR data. No wrapper framing, no magic bytes, no length prefix.
- **A-6** — No private-key unwrap occurs in 5.1. The zeroization obligation stated in the sub-phase security review applies to 5.2's HPKE-open site, not to 5.1 code.
- **A-7** — `SharingError` is a new `thiserror` enum in `src-tauri/src/sharing/error.rs`. Storage-to-sharing conversion is performed at the `storage::sharing` adapter boundary and does not leak raw `rusqlite` error messages verbatim to callers.
- **A-8** — Contacts are identified by `ContactId` (UUID v4 newtype). `insert_contact` expects the caller to generate the UUID; the store does not auto-generate. `created_at` is the caller-provided Unix timestamp, matching the Phase 3 `Node::created_at` convention, to keep timestamps deterministic in tests.
- **A-9** — `display_name` is validated as non-empty after trim; `email` is an optional nullable `TEXT` and is not syntactically validated (display-only per design lines 55–56).
- **A-10** — `x25519-dalek` v2 `StaticSecret` / `PublicKey` are `#[cfg(test)]`-only usage inside the sharing tests. Production sharing code does **not** import `x25519-dalek` in 5.1 (no keypair handling yet).
- **A-11** — The sharing module is registered in `src-tauri/src/lib.rs` (new `pub mod sharing;` entry) but no Tauri IPC commands are added in 5.1. The frontend wiring is Phase 6.

## 5. Approach

### CONTRACT_SNIPPETS

**CS-001** — `SharingError` enum (new, `src-tauri/src/sharing/error.rs`):

```rust
use thiserror::Error;

#[non_exhaustive]
#[derive(Debug, Error)]
pub enum SharingError {
    #[error("identity not initialised: vault_identity row missing")]
    IdentityMissing,
    #[error("contact not found")]
    ContactNotFound,
    #[error("contact constraint violation: {0}")]
    ConstraintViolation(String),
    #[error("invalid X25519 public key length: expected 32 bytes, got {0}")]
    InvalidPublicKeyLength(usize),
    #[error("invalid contact identifier: {0}")]
    InvalidContactId(String),
    #[error("display name must not be empty")]
    EmptyDisplayName,
    #[error("sharing storage backend error: {0}")]
    Backend(String),
}
```

**CS-002** — Newtypes (new, `src-tauri/src/sharing/types/mod.rs`):

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ContactId([u8; 16]);

impl ContactId {
    pub fn new(bytes: [u8; 16]) -> Self;
    pub fn from_uuid(uuid: uuid::Uuid) -> Self;
    pub fn to_uuid(&self) -> uuid::Uuid;
    pub fn as_bytes(&self) -> &[u8; 16];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct X25519PublicKey([u8; 32]);

impl X25519PublicKey {
    pub fn new(bytes: [u8; 32]) -> Self;
    pub fn as_bytes(&self) -> &[u8; 32];
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisplayName(String);

impl DisplayName {
    pub fn new(value: &str) -> Result<Self, crate::sharing::error::SharingError>;
    pub fn as_str(&self) -> &str;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fingerprint([u8; 8]);

impl Fingerprint {
    pub fn to_hex_lowercase(&self) -> String;
    pub fn as_bytes(&self) -> &[u8; 8];
}
```

**CS-003** — `Contact` struct (new, `src-tauri/src/sharing/store.rs`):

```rust
#[derive(Debug, Clone)]
pub struct Contact {
    pub contact_id: crate::sharing::types::ContactId,
    pub display_name: crate::sharing::types::DisplayName,
    pub email: Option<String>,
    pub public_key: crate::sharing::types::X25519PublicKey,
    pub created_at: i64,
}
```

**CS-004** — `SharingStore` trait (new, `src-tauri/src/sharing/store.rs`):

```rust
use async_trait::async_trait;

#[async_trait]
pub trait SharingStore: Send + Sync {
    async fn get_own_public_key(
        &self,
    ) -> Result<crate::sharing::types::X25519PublicKey, crate::sharing::error::SharingError>;

    async fn insert_contact(
        &self,
        contact: &crate::sharing::store::Contact,
    ) -> Result<(), crate::sharing::error::SharingError>;

    async fn get_contact(
        &self,
        contact_id: crate::sharing::types::ContactId,
    ) -> Result<crate::sharing::store::Contact, crate::sharing::error::SharingError>;

    async fn list_contacts(
        &self,
    ) -> Result<Vec<crate::sharing::store::Contact>, crate::sharing::error::SharingError>;

    async fn delete_contact(
        &self,
        contact_id: crate::sharing::types::ContactId,
    ) -> Result<(), crate::sharing::error::SharingError>;
}
```

**CS-005** — Identity helpers (new, `src-tauri/src/sharing/identity.rs`):

```rust
use crate::sharing::types::{Fingerprint, X25519PublicKey};

pub fn export_public_key_bytes(public_key: &X25519PublicKey) -> [u8; 32];

pub fn public_key_qr_string(public_key: &X25519PublicKey) -> String;

pub fn compute_fingerprint(public_key: &X25519PublicKey) -> Fingerprint;
```

Fingerprint rule (CS-005 body): `let digest = sha2::Sha256::digest(public_key.as_bytes()); Fingerprint(<first 8 bytes of digest>)`. Renders as 16 lowercase hex characters via `Fingerprint::to_hex_lowercase`.

QR rule (CS-005 body): `base64::engine::general_purpose::STANDARD.encode(public_key.as_bytes())` — standard alphabet with padding, 44 ASCII characters.

**CS-006** — `From<StorageError> for SharingError` and rusqlite-error mapping (new, `src-tauri/src/sharing/error.rs`):

```rust
impl From<crate::storage::StorageError> for SharingError {
    fn from(error: crate::storage::StorageError) -> Self {
        match error {
            crate::storage::StorageError::NotFound => Self::ContactNotFound,
            crate::storage::StorageError::ConstraintViolation(message) => {
                Self::ConstraintViolation(message)
            }
            other => Self::Backend(other.to_string()),
        }
    }
}
```

**CS-007** — Existing SQL DDL (already in `src-tauri/src/storage/schema.rs`, ground truth):

```sql
CREATE TABLE contacts (
    contact_id       TEXT PRIMARY KEY,
    display_name     TEXT NOT NULL,
    email            TEXT,
    public_key       BLOB NOT NULL,
    created_at       INTEGER NOT NULL
);

CREATE TABLE vault_identity (
    id                   INTEGER PRIMARY KEY CHECK (id = 1),
    public_key           BLOB NOT NULL UNIQUE,
    wrapped_private_key  BLOB NOT NULL
);
```

No DDL changes in 5.1 — both tables exist as of Phase 2.4 / 3.1.

### Steps

Work below is scoped to **exposure only**. All identity generation / wrapping / persistence remains in `auth::ceremonies::create_vault`.

1. **Create the `sharing` module skeleton** at `src-tauri/src/sharing/` with files `mod.rs` (re-exports only), `error.rs` (CS-001 + CS-006), `types/mod.rs` with submodules `contact_id.rs`, `display_name.rs`, `fingerprint.rs`, `x25519_public_key.rs` (CS-002), `identity.rs` (CS-005), and `store.rs` (CS-003 + CS-004). Register the module in `src-tauri/src/lib.rs` via `pub mod sharing;`.

2. **Newtype wrappers** (CS-002): implement per `.claude/rules/rust.md` — one concern per file. `DisplayName::new` trims whitespace and returns `SharingError::EmptyDisplayName` on empty input. `X25519PublicKey::new` takes raw 32 bytes (no on-curve validation in 5.1 — that belongs to `x25519-dalek` at HPKE time in 5.2). Every newtype gets `///` doc comments.

3. **Fingerprint helpers** (CS-005) in `src-tauri/src/sharing/identity.rs`: implement `compute_fingerprint` using `sha2::Sha256::new()` + `Digest::finalize()`; copy the first 8 bytes into `Fingerprint`. `Fingerprint::to_hex_lowercase` uses `hex::encode` (already a dependency) and yields 16 characters. `public_key_qr_string` uses `base64::engine::general_purpose::STANDARD`. No other I/O in this file.

4. **`SharingStore` trait** (CS-004) in `src-tauri/src/sharing/store.rs`. Trait methods are `async` via `async_trait` to match `MetadataStore`. Include `Contact` struct (CS-003) in the same file per the existing `MetadataStore` pattern (trait + owned row struct colocated).

5. **`impl SharingStore for SqlCipherMetadataStore`** in a new file `src-tauri/src/storage/sharing.rs`. Register the module in `src-tauri/src/storage/mod.rs` as `pub mod sharing;`. Implementation:
   - `get_own_public_key`: `SELECT public_key FROM vault_identity WHERE id = 1`. Missing row → `SharingError::IdentityMissing`. Non-32-byte blob → `SharingError::InvalidPublicKeyLength`. Use the existing `with_connection_blocking` helper on `SqlCipherMetadataStore` (extend its visibility to `pub(crate)` if needed — inspect first; prefer adding a scoped `sharing`-only accessor on the struct).
   - `insert_contact`: `INSERT INTO contacts (contact_id, display_name, email, public_key, created_at) VALUES (?, ?, ?, ?, ?)`. Map rusqlite errors via `StorageError::from_rusqlite` + CS-006.
   - `get_contact`: `SELECT ... FROM contacts WHERE contact_id = ?`. `query_row` missing → `SharingError::ContactNotFound`.
   - `list_contacts`: `SELECT ... FROM contacts ORDER BY display_name ASC, contact_id ASC` (stable ordering for test assertions).
   - `delete_contact`: `DELETE FROM contacts WHERE contact_id = ?`. `rows_affected == 0` → `SharingError::ContactNotFound` (matches `MetadataStore::rename_node`'s NotFound convention).
   - All queries run inside `with_connection_blocking`.

6. **Re-exports** in `src-tauri/src/sharing/mod.rs` (following the `storage::mod` pattern): `pub use error::SharingError;`, `pub use store::{Contact, SharingStore};`, `pub use types::{ContactId, DisplayName, Fingerprint, X25519PublicKey};`, `pub use identity::{compute_fingerprint, export_public_key_bytes, public_key_qr_string};`.

7. **Tests** (target ~80 LOC production tests + colocated in each file per existing repo convention):
   - `types/contact_id.rs`: UUID round-trip; raw-bytes round-trip.
   - `types/display_name.rs`: empty / whitespace-only → `EmptyDisplayName`; trim preserved.
   - `types/fingerprint.rs`: `to_hex_lowercase` yields 16 chars, all lowercase, no separators.
   - `identity.rs`: `compute_fingerprint` against known test vector (SHA-256 of `[0u8; 32]` first 8 bytes = `66687aadf862bd77`); `export_public_key_bytes` returns 32 bytes matching input; `public_key_qr_string` decodes via base64 back to original 32 bytes.
   - `storage/sharing.rs`: open-in-memory SQLCipher (using the existing `SqlCipherMetadataStore::create` test helper), insert → get → list → delete round-trip; `get_own_public_key` after `create_vault` (or a direct `INSERT INTO vault_identity (1, ?, ?)` seed if calling `create_vault` from a storage test is inconvenient); missing identity row returns `IdentityMissing`; deleting a nonexistent contact returns `ContactNotFound`.
   - Ceremony-layer test (optional, in `create.rs`): assert that after `create_vault`, `SharingStore::get_own_public_key` returns the same 32 bytes that `PublicKey::from(&static_secret).to_bytes()` produced. This closes the loop between the two modules without duplicating generation logic. Gate behind the `ceremony_lock` pattern used by existing tests.

8. **Run the full test suite** per memory-recorded command: `cargo test --workspace --all-targets --all-features`, then `cargo clippy --workspace --all-targets --all-features -- -D warnings`, then `cargo fmt --check`.

## 6. Review focus areas

### 6a — Rust change surface (anticipated)
- `src-tauri/src/lib.rs` (one-line module registration).
- `src-tauri/src/sharing/mod.rs` (new).
- `src-tauri/src/sharing/error.rs` (new).
- `src-tauri/src/sharing/identity.rs` (new).
- `src-tauri/src/sharing/store.rs` (new).
- `src-tauri/src/sharing/types/mod.rs` (new).
- `src-tauri/src/sharing/types/contact_id.rs` (new).
- `src-tauri/src/sharing/types/display_name.rs` (new).
- `src-tauri/src/sharing/types/fingerprint.rs` (new).
- `src-tauri/src/sharing/types/x25519_public_key.rs` (new).
- `src-tauri/src/storage/mod.rs` (one-line module registration for new `sharing` submodule).
- `src-tauri/src/storage/sharing.rs` (new, `impl SharingStore for SqlCipherMetadataStore`).
- `src-tauri/src/storage/sqlcipher.rs` (possible visibility change on `with_connection_blocking` — minimal, see step 5).

### 6b — Security-sensitive paths
- `src-tauri/src/sharing/identity.rs` — fingerprint must be computed over the raw 32-byte public key (not base64, not hex, not QR string). No log statements emitting public-key bytes. No `Debug` impl leaking key material through formatting (public keys are not secret but follow the project's "keys do not appear in logs" convention per `.claude/rules/auth.md`).
- `src-tauri/src/storage/sharing.rs` — `get_own_public_key` must not read `wrapped_private_key` or expose it in any form in 5.1. SQL error messages must not be surfaced verbatim to callers (use `SharingError::Backend(String)` already sanitised via `StorageError`).
- `src-tauri/src/sharing/types/x25519_public_key.rs` — ensure `Debug` does not print the raw bytes (print only a fixed label or a fingerprint).
- No code path unwraps `vault_identity.wrapped_private_key` in 5.1 — any such unwrap is a Plan Deviation and must route back through planning for 5.2.

### 6c — Architecture risk areas
- **Module boundary (sharing ↔ storage)**: the `impl SharingStore for SqlCipherMetadataStore` lives under `storage::sharing`. Check that no `storage` code imports `sharing::identity` (one-way dependency only: `storage::sharing` → `sharing::*` for types and error). Check that `sharing::` does not import `storage::sqlcipher` or any storage-private details.
- **Trait coupling**: verify `MetadataStore` is **not** extended with sharing methods (explicit rule in `.claude/rules/storage.md`). `SharingStore` must be a standalone trait.
- **Visibility discipline**: `with_connection_blocking` (currently private on `SqlCipherMetadataStore`) should be promoted only as narrowly as needed (prefer adding dedicated `pub(crate)` sharing accessors to the struct, similar to how `list_sync_chunks` and `list_all_blob_names` are structured today).
- **Identity ownership**: a single source-of-truth for keypair creation (`auth::ceremonies::create_vault`). `sharing::` must be read-only with respect to `vault_identity`.
- **Dependency direction**: `sharing` may depend on `crypto` (for future types) and on `storage::StorageError` for `From`. `sharing` must not depend on `auth`.

### 6d — Testing requirements
- **Validation checkpoint from sub-roadmap**:
  - `cargo test sharing::identity` — all tests pass, no warnings.
  - `cargo test sharing::contacts` (achieved via `storage::sharing` and `sharing::store` tests).
  - Manual: public-key file is exactly 32 bytes; fingerprint display matches external SHA-256 truncation.
- **Edge cases from Step 2**:
  - Empty / whitespace-only `DisplayName` rejected.
  - Non-32-byte `vault_identity.public_key` blob → `InvalidPublicKeyLength` (not a panic).
  - Duplicate `contact_id` insert → `ConstraintViolation`.
  - `delete_contact` on nonexistent id → `ContactNotFound` (not a silent no-op).
  - `get_own_public_key` on vault without `vault_identity` row → `IdentityMissing`.
  - Fingerprint determinism: repeated invocation on same key yields identical 16-char lowercase hex output.
  - Base64 QR round-trip: decode yields original 32 bytes.
- **Adversarial coverage** (light for 5.1; heavier crypto coverage belongs to 5.2):
  - Fingerprint collision: two distinct public keys must produce different fingerprints (structural sanity, not statistical guarantee).
  - `email` field: `Some("")` versus `None` are stored distinctly and round-trip.
- **Coverage target**: ≥80% line coverage on new sharing files (these are security-adjacent even if 5.1 handles only public material).

## 7. Documentation impact

| Path | Change | Timing |
|------|--------|--------|
| `docs/architecture/designs/file-sharing/sub-phases/5.1-identity-and-contacts.md` | Amend Deliverables §1–§2 to note that keypair generation and private-key wrapping are delegated to Phase 2.4 (`create_vault`); 5.1 scope is the sharing-module exposure layer. Reference C-1 resolution. | **Required this run** (sub-phase must reflect code reality before handoff to 5.2). |
| `docs/architecture/design-invariants.md` | Add invariant §13 "Vault identity ownership": exactly one `vault_identity` row per vault (`id = 1` constraint); identity is created in `auth::ceremonies::create_vault` and is read-only from `sharing::`. | **Required this run** (locks the single-ownership contract before 5.2 extends access). |
| `docs/architecture/designs/file-sharing/design.md` — Open Decisions table | Add "Contact deduplication by `public_key`" row (status: Extension point for 5.3 / Phase 6). | **Deferred** (non-blocking design decision; rationale in C-3). |
| `docs/architecture/designs/file-sharing/sub-phases/roadmap.md` | Update Phase 5.1 summary line to "~60 LOC production + ~100 LOC tests" to reflect the exposure-only scope. | **Deferred** (cosmetic; re-evaluate once implementation lands). |
| `docs/architecture/designs/file-sharing/diagrams/file-sharing-flow.md` | No change in 5.1 (diagrams cover HPKE flow, which is 5.2 territory). | **Not applicable**. |

## 8. Governance sync actions (pre-implementation)

| Action ID | Reason / linked concern | Target files | Required edit | Verification |
|-----------|------------------------|--------------|---------------|--------------|
| GS-1 | `.claude/rules/` has no sharing module rule yet; 5.1 creates the module and sets its conventions for 5.2 / 5.3. | `C:\Users\chris\source\repos\arx-runa\.claude\rules\sharing.md` (new) | Create rule file stating: (i) Design specification path; (ii) `SharingStore` trait boundary — never extend `MetadataStore`; (iii) identity is read-only from `sharing::` (generation owned by `auth::ceremonies::create_vault`); (iv) fingerprint = first 8 bytes of `SHA-256(public_key)`, rendered as 16 lowercase hex chars; (v) no public-key bytes in logs or `Debug` output; (vi) contacts CRUD lives in `storage::sharing`, not `storage::sqlcipher` (mirrors the `destination_session` pattern). | File exists; content matches the listed bullets; file is referenced from `/copilot-sync` run. |
| GS-2 | `.claude/rules/storage.md` currently lists forward-declared Phase 5 tables but does not pin where their CRUD lives. | `C:\Users\chris\source\repos\arx-runa\.claude\rules\storage.md` | Append a bullet under the **Traits** section: "`contacts` CRUD lives in `storage::sharing` behind the `SharingStore` trait in `sharing::store`, not on `MetadataStore`. Mirrors the `destination_session` split." | Grep finds the new bullet under the Traits section. |
| GS-3 | `.claude/rules/auth.md` describes ceremony-owned material but does not name `vault_identity` as auth-owned. | `C:\Users\chris\source\repos\arx-runa\.claude\rules\auth.md` | Append a bullet under the **Ceremonies** section: "`vault_identity` row is written exactly once, during `create_vault`, and re-wrapped in place during `change_password` and `rotate_key_file`. Sharing-module code reads `vault_identity.public_key` only; it must never insert, update, or delete the row." | Grep finds the new bullet under the Ceremonies section. |
| GS-4 | After rule edits, Copilot instructions must be synchronised. | Run `/copilot-sync`. | Invoke the `copilot-sync` skill; it mirrors updated `.claude/rules/*.md` files to `.github/instructions/`. | `git status` shows updates under `.github/instructions/` (or a clean sync if already up to date). |

Run order: GS-1 → GS-2 → GS-3 → GS-4 (GS-4 must be last; it depends on the rule edits completing).

## 9. Handoff Notes for Implementer

Working directory: `C:\Users\chris\source\repos\arx-runa\`. This plan is self-contained; do not re-derive context from the sub-phase document without reading the C-1 resolution first — the sub-phase's Deliverables §1 and §2 look like "generate a keypair" but the keypair already exists at `src-tauri/src/auth/ceremonies/create.rs:123-147`. **Do not** add a second keypair generation path. Execute Section 8 governance actions (GS-1 … GS-4) before writing any code; both rule files and the Copilot mirror must be coherent before review. Then create the `sharing` module tree per Section 5 steps 1–6 and the storage impl per step 5. Ceremony-layer tests must acquire `ceremony_lock` to avoid interference with other auth tests; see `src-tauri/src/auth/ceremonies/test_support.rs` for the pattern. Platform note: no platform-gated code in 5.1 — all work is pure-Rust SQLCipher and crypto-adjacent. Validate with the user's preferred command: `cargo test --workspace --all-targets --all-features`. Security-reviewer invocation is mandatory per the sub-phase (see Section 6b for focus areas).

## Implementation Log

- **Date**: 2026-04-20T06:01:00+02:00
- **Run ID**: `phase-5-1-identity-and-contacts-20260420-055351`
- **Track**: `full`
- **Branch**: `development`
- **Execution mode**: Orchestrator direct implementation (fallback path used for coding steps in this run).

### Agent evidence

| Approach step | Agent | Agent ID | Outcome |
|---|---|---|---|
| Rust quality review cycle 1 | `rust-reviewer` | `phase51-rust-review2` | 1 MEDIUM finding |
| Security review cycle 1 | `security-reviewer` | `phase51-security-review2` | 1 WARNING finding |
| Rust quality re-review | `rust-reviewer` | `phase51-rust-review3` | No actionable findings |
| Security re-review | `security-reviewer` | `phase51-security-review3` | 1 NOTE finding (deferred by plan) |
| Architecture review cycle 1 | `architecture-reviewer` | `phase51-arch-review3` | 1 MEDIUM finding |
| Test expansion audit | `test-writer` | `phase51-test-writer` | Added adversarial tests in `storage::sharing` |

### Files changed

- `.claude/plans/phase-5-1-identity-and-contacts.md`
- `.claude/rules/auth.md`
- `.claude/rules/storage.md`
- `.claude/rules/sharing.md`
- `.github/instructions/auth.instructions.md`
- `.github/instructions/crypto.instructions.md`
- `.github/instructions/leptos.instructions.md`
- `.github/instructions/memory-protection.instructions.md`
- `.github/instructions/mermaid.instructions.md`
- `.github/instructions/research.instructions.md`
- `.github/instructions/rust.instructions.md`
- `.github/instructions/storage.instructions.md`
- `.github/instructions/tauri.instructions.md`
- `.github/instructions/sharing.instructions.md`
- `docs/architecture/design-invariants.md`
- `docs/architecture/designs/file-sharing/sub-phases/5.1-identity-and-contacts.md`
- `src-tauri/src/lib.rs`
- `src-tauri/src/storage/mod.rs`
- `src-tauri/src/storage/sqlcipher.rs`
- `src-tauri/src/storage/sharing.rs`
- `src-tauri/src/sharing/mod.rs`
- `src-tauri/src/sharing/error.rs`
- `src-tauri/src/sharing/identity.rs`
- `src-tauri/src/sharing/store.rs`
- `src-tauri/src/sharing/types/mod.rs`
- `src-tauri/src/sharing/types/contact_id.rs`
- `src-tauri/src/sharing/types/display_name.rs`
- `src-tauri/src/sharing/types/fingerprint.rs`
- `src-tauri/src/sharing/types/x25519_public_key.rs`

### Verification

- **Formatting check**: `cargo fmt --all -- --check` passed (after formatting once).
- **Clippy results**: `cargo clippy --workspace --all-targets --all-features -- -D warnings` passed.
- **Test results**: `cargo test --workspace --all-targets --all-features` passed (`557 passed; 0 failed; 1 ignored` in the main library run).
- **Release build**: `cargo build --workspace --release` passed.

### Review outcomes

- **Rust review**: MEDIUM issue for UUID v4 contract enforcement on `ContactId`; remediated.
- **Architecture review**: MEDIUM issue for dependency direction (`sharing::error` coupled to `StorageError`); remediated by local adapter mapping in `storage::sharing`.
- **Security review**: WARNING issue for unsanitized backend error propagation; remediated. One NOTE about `received_shares.sender_public_key` schema alignment remains deferred to later phase scope.
- **Cross-shard review**: N/A (single shard).
- **Findings quality gate**: `ACTIONABLE_NOW=3`, `DEFERRED_BY_PLAN=1`, `INTENTIONAL_DECISION=0`, `INSUFFICIENT_EVIDENCE=0`.
- **Finding overrides**: None.
- **Design challenge outcomes**: None.

### Governance sync

- **Actions executed**: 4 (`GS-1`..`GS-4`).
- **Files updated**: `.claude/rules/{sharing,storage,auth}.md` and mirrored `.github/instructions/*.instructions.md`.
- **copilot-sync outcome**: OK (no degraded state).

### Sub-phase decisions sync

- **Doc path**: `docs/architecture/designs/file-sharing/sub-phases/5.1-identity-and-contacts.md`
- **Decisions added/updated**: 5 bullets under `## Implementation Decisions`.

### Deviations from plan

- `SharingError` no longer implements `From<StorageError>` directly; storage-to-sharing conversion moved into `storage::sharing` to preserve boundary direction.
- Added explicit UUID v4 enforcement at storage boundary and decode paths to align with the contact ID contract.

### Documentation flagged

- Required this run and completed:
  - `docs/architecture/designs/file-sharing/sub-phases/5.1-identity-and-contacts.md`
  - `docs/architecture/design-invariants.md`
- Deferred by plan:
  - `docs/architecture/designs/file-sharing/design.md` Open Decisions row for contact deduplication policy.
  - `docs/architecture/designs/file-sharing/sub-phases/roadmap.md` scope-estimate cosmetic update.
- Not applicable:
  - `docs/architecture/designs/file-sharing/diagrams/file-sharing-flow.md`

### Run state path

- `.claude/runs/phase-5-1-identity-and-contacts-20260420-055351/`
