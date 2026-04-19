---
title: "Phase 4.4 — Manifest Cloud Backup"
created: "2026-04-19T17:34:12+02:00"
status: implemented
roadmap-phase: 4
sub-phase: "4.4"
design-document: "docs/architecture/designs/cloud-synchronisation/design.md"
sub-phase-roadmap: "docs/architecture/designs/cloud-synchronisation/sub-phases/roadmap.md"
governance-sync-required: true
tags: [storage, cloud, manifest-backup, sqlcipher, vacuum-into, phase-4]
---

# Plan: Phase 4.4 — Manifest Cloud Backup

## 1. Goal

Land `upload_manifest_backup` and `download_manifest_backup` at the cloud-transport boundary under `src-tauri/src/storage/cloud/manifest_backup.rs`: export the SQLCipher manifest via `VACUUM INTO` to a consistent snapshot, XChaCha20-Poly1305-encrypt the snapshot under `manifest_key`, push to the canonical cloud path `manifest/manifest-backup.blob`, and pair it with a download path that writes a ready-to-open SQLCipher DB to disk and verifies its integrity before returning success. Retire the Phase 2.4 SQL-text recovery stub and route the existing `recover_vault` / `recover_with_phrase` ceremonies through the new helper, removing the `MANIFEST_BACKUP_BLOB_NAME = "manifest-backup.enc"` placeholder constant from `src-tauri/src/auth/ceremonies/mod.rs`.

## 2. Context

**Sub-phase position.** 4.4 is the fourth unit of the cloud-sync roadmap (4.1 → 4.2 → 4.3 → **4.4** → 4.5). Dependencies met: Phase 4.1 (`CloudTransport` trait + `MockCloudTransport`), Phase 4.2 (`RcloneTransport`), Phase 4.3 (`upload_vault_header` / `download_vault_header`). Phase 4.5 (push/pull flows and conflict detection) directly consumes the manifest-backup helpers to implement the manifest-is-canonical conflict check. Roadmap allotment: ~150 production + ~100 test lines; security review **required**.

**Canonical design sections** (`docs/architecture/designs/cloud-synchronisation/design.md`):
- `#manifest-cloud-backup` (lines 794–838) — purpose, 8-step upload flow, 8-step download flow.
- `#encryption-scheme` (lines 800–811) — wire format `[24B nonce | ciphertext | 16B tag]`, no AAD (singleton blob), `manifest_key = hkdf(master_key, info=b"arx-runa-manifest-backup")`.
- `#cloud-storage-layout` (lines 49–71) — canonical path `manifest/manifest-backup.blob`.
- `#contract-surface` (lines 19–46) — `CloudTransport` interface, `manifest/manifest-backup.blob` canonical layout, AEAD-under-`manifest_key` invariant.
- `docs/architecture/design-invariants.md` — streaming-rule exception for manifest (<10 MiB) documented in `#encryption-scheme`.

**Sub-phase source.** `docs/architecture/designs/cloud-synchronisation/sub-phases/4.4-manifest-backup.md` — deliverables 1–3 (upload, download, tests), implementation notes (`VACUUM INTO` or `Connection::backup_to_path`, <10 MiB in-memory exception, staging-directory temp files, no partial-parse on decrypt failure), security-review gate.

**Existing code state as of 2026-04-19.**
- `src-tauri/src/storage/cloud/manifest_backup.rs` is the Phase 2.4 stub: only `encrypt_manifest_backup(plaintext: Vec<u8>, manifest_key: &[u8; 32]) -> Result<Vec<u8>, CryptoError>` and `decrypt_manifest_backup(wire: &[u8], manifest_key: &[u8; 32]) -> Result<Zeroizing<Vec<u8>>, CryptoError>`, both `pub(crate)`. Module comment (`manifest_backup.rs:1-11`) explicitly states: "Phase 4.4 will own the full manifest-backup schema, streaming, and versioning." Wire format, XChaCha20-Poly1305 cipher, 24-byte CSPRNG nonce, and no-AAD choice all match the design and must be preserved.
- `src-tauri/src/storage/cloud/mod.rs:11` already re-exports `pub mod manifest_backup`. The module re-exports `VAULT_HEADER_BLOB_NAME`, `VaultHeaderSyncError`, `upload_vault_header`, `download_vault_header` from `vault_header_io` (Phase 4.3). No manifest-backup constants or sync error type is re-exported yet.
- `src-tauri/src/auth/ceremonies/mod.rs:29` owns `pub(super) const MANIFEST_BACKUP_BLOB_NAME: &str = "manifest-backup.enc";` — **drifts from canonical design** (`manifest/manifest-backup.blob`). Phase 4.3 plan explicitly deferred this constant's relocation to Phase 4.4 (see `.claude/plans/phase-4-3-vault-header.md:261, 373`).
- `src-tauri/src/auth/ceremonies/recover_vault.rs:6, 29, 102` inlines the download + decrypt path: `download_blob(MANIFEST_BACKUP_BLOB_NAME, staging_dir/"recover-vault-manifest-backup.enc") → tokio::fs::read → decrypt_manifest_backup → import_manifest_sql_atomic`. All failures map to `AuthenticationError::VaultHeaderInvalid` (transport) or `AuthenticationError::InvalidCredentials` (decrypt/import).
- `src-tauri/src/auth/ceremonies/recover_with_phrase.rs:4, 32, 95` uses the same pattern with `recover-with-phrase-manifest-backup.enc` as its local staging name.
- `src-tauri/src/auth/ceremonies/helpers.rs:132-162` defines `import_manifest_sql_atomic(vault_db_path, sqlcipher_key, manifest_sql_plaintext)` which **treats the decrypted bytes as UTF-8 SQL text** (`std::str::from_utf8(...)` + `conn.execute_batch(sql_text)` on a freshly-opened SQLCipher DB). This is a Phase 2.4 stub that produces a valid SQLCipher DB but does not match the design's intent: the download flow step 5 writes "`manifest_buffer` to the Arx Runa data directory as the local SQLCipher DB" — i.e., the plaintext **is** the DB file bytes.
- `src-tauri/src/auth/ceremonies/test_support.rs:155-182` seeds the mock cloud using `b"CREATE TABLE IF NOT EXISTS imported_stub (id INTEGER);"` as the fake SQL payload. Two call sites: `upload_manifest_backup_for(vault)` (used by the recovery round-trip tests) and `upload_manifest_backup_payload_for(vault, payload)` (used to force `b"not valid sql"` → `InvalidCredentials` in `test_recover_vault_failed_import_cleans_temp_and_keeps_destination_absent`).
- `src-tauri/src/auth/session/keys.rs:17, 50-65` already derives `manifest_key: SecureBytes<32>` via HKDF-SHA256 with `HKDF_INFO_MANIFEST_BACKUP = b"arx-runa-manifest-backup"` (from `src-tauri/src/crypto/hkdf.rs`). No derivation work is required in 4.4.
- `src-tauri/src/storage/staging.rs:13-17` provides `default_staging_directory()` → `dirs::data_dir().join("arx-runa").join("staging")` and `write_owner_only(&Path, &[u8])` with 0o600 semantics on Unix. `src-tauri/src/auth/staging.rs` wraps its own `staging_directory()` under `dirs::config_dir()/arx-runa/` for ceremony-owned metadata. Design note (4.4-manifest-backup.md:64) says: "Temp files in Arx Runa staging directory" — ambiguous between `storage::staging` and `auth::staging`; see C-9 for resolution.
- `src-tauri/src/storage/schema.rs` owns the canonical SQLCipher DDL (`nodes`, `chunks`, `manifest_meta`, `pending_deletions`). `VACUUM INTO` against an existing SQLCipher `MetadataStore` connection produces a fully-populated DB with the same DDL and the source key.
- `.claude/rules/storage.md` Cloud-backup section already declares: "Manifest encrypted with `manifest_key`", "Manifest backup is a singleton blob (no AAD); vault header stays plaintext JSON at cloud root", and "Push flow uploads manifest backup, then uploads vault header idempotently on every push." No rule edits are anticipated against `.claude/rules/storage.md` for 4.4 proper.
- `.claude/rules/auth.md` Ceremonies section pins the forward declaration: "`master_key` is bound as `Zeroizing<[u8; 32]>` inside ceremony-local scope and must not escape the function body." New 4.4 ceremony changes must continue to respect this invariant — `manifest_key` is already derived inside `SessionKeys` and passed by borrowed reference.
- `.claude/rules/crypto.md` Singleton-blob section already declares: "Singleton blobs follow design-specific AAD rules (…manifest backup uses no AAD)." Aligned; no edit.
- `docs/guides/glossary.md:17, 51, 59, 102` already describes the canonical `manifest/manifest-backup.blob` path; the drift is isolated to the Phase 2.4 constant.

**No pending architectural decisions** apply to this sub-phase; `## Contract Surface` in the design is canonical and Contract Surface has not drifted in this area since Phase 4.3.

**Security review.** Sub-phase declares **Required**. Roadmap-level security review checkpoint confirms: "Phase 4.4: Requires `security-reviewer` agent review (manifest encryption, zeroization)." Scope for the reviewer: (a) new nonce generation and AEAD invocation path, (b) `manifest_key` handling and `Zeroizing` discipline in upload/download, (c) SQLCipher `VACUUM INTO` snapshot correctness, (d) staging-file cleanup (success + error + cancellation), (e) the absence of AAD (singleton invariant), (f) owner-only permissions on Unix for the local SQLCipher DB file produced by `download_manifest_backup`.

## 3. Design Concerns / Open Questions

| Concern | Source | Impact | Classification | Resolution | Documentation updates |
|---|---|---|---|---|---|
| **C-1 Cloud blob path drift: Phase 2.4 constant `MANIFEST_BACKUP_BLOB_NAME = "manifest-backup.enc"` contradicts canonical `manifest/manifest-backup.blob`.** | `src-tauri/src/auth/ceremonies/mod.rs:29` vs. `design.md:29, 55, 822, 829` | Until the constant moves to the canonical path, any cloud deployed with Phase 2.4 writes the backup to the wrong remote path. Not user-visible yet (no external callers), but Phase 4.5 push/pull will use the canonical path and immediately diverge. | Non-blocking | Introduce `MANIFEST_BACKUP_BLOB_NAME: &str = "manifest/manifest-backup.blob"` **inside `storage/cloud/manifest_backup.rs`** as a `pub const`; re-export from `storage::cloud`. Delete the `auth/ceremonies/mod.rs:29` declaration. Update `recover_vault.rs:6`, `recover_with_phrase.rs:4`, and `test_support.rs` imports to use the cloud-module constant. | Update `src-tauri/src/storage/cloud/manifest_backup.rs:1-11` module comment to remove the "Phase 4.4 will own…" forward-declaration language and describe the current state. Update `src-tauri/src/auth/ceremonies/mod.rs:26-33` doc comment to drop the `MANIFEST_BACKUP_BLOB_NAME` line. |
| **C-2 Plaintext format mismatch: Phase 2.4 stub encrypts/decrypts UTF-8 SQL text; design step 5 of the download flow says the plaintext *is* the SQLCipher DB file bytes.** | `src-tauri/src/auth/ceremonies/helpers.rs:132-162` (`import_manifest_sql_atomic`) vs. `design.md:820, 834`; sub-phase implementation note "Use `rusqlite::Connection::backup_to_path` or `VACUUM INTO` for export" (4.4-manifest-backup.md:61) | Keeping the SQL-text path past Phase 4.4 locks callers into a stub that cannot carry real manifest content (foreign keys, indices, binary blobs). Phase 4.5 push/pull requires real DB bytes to round-trip. | Non-blocking | Upload side: call SQLCipher `VACUUM INTO '<staging_dir>/manifest-export.db'` against the caller-provided connection (or a freshly-opened read-only connection on the vault DB path), read the resulting `.db` file into a `Zeroizing<Vec<u8>>`, encrypt via `encrypt_manifest_backup`, upload, delete both temp files. Download side: after `decrypt_manifest_backup`, write the plaintext bytes directly to `<destination_db_path>` via `staging::write_owner_only`-style helper, then open SQLCipher with `sqlcipher_key` and run `PRAGMA cipher_integrity_check` (or at minimum `SELECT COUNT(*) FROM sqlite_master`) to verify integrity; on failure, delete the destination file and return a decrypt/integrity error. Retire `import_manifest_sql_atomic`. Migrate `test_support::upload_manifest_backup_payload_for` to use real VACUUM-INTO bytes (see C-5). | Update the module comment at `src-tauri/src/auth/ceremonies/helpers.rs:130-131` when `import_manifest_sql_atomic` is removed, or delete the function entirely with its doc comment. |
| **C-3 Constant location violates SRP: `MANIFEST_BACKUP_BLOB_NAME` lives in `auth/ceremonies/mod.rs` but its owner concern is cloud layout, not auth.** | `src-tauri/src/auth/ceremonies/mod.rs:29` | Auth module re-exports a cloud-layout constant; mirrors the pre-4.3 `VAULT_HEADER_BLOB_NAME` drift that 4.3 resolved by relocating the constant to `storage::cloud::vault_header_io`. | Non-blocking | Relocation tracked by C-1. `storage::cloud::manifest_backup` owns the constant; `auth::ceremonies` imports it like any other client. Same treatment as Phase 4.3's `VAULT_HEADER_BLOB_NAME` relocation. | See C-1 doc updates; no further. |
| **C-4 Sub-phase deliverables list the two functions but do not list refactoring existing ceremonies to call them.** | `4.4-manifest-backup.md:11-29` (deliverables 1–3) vs. `src-tauri/src/auth/ceremonies/recover_vault.rs:100-125`, `recover_with_phrase.rs:95-117` | Without refactor, 4.4 adds a second code path next to the inlined Phase 2.4 download; duplication guarantees divergence when Phase 4.5 adds the push side. Symmetric to Phase 4.3's ceremony-refactor decision. | Non-blocking | In-scope refactor: migrate `recover_vault` and `recover_with_phrase` to call `download_manifest_backup(cloud_transport, staging_dir, manifest_key, destination_db_path, sqlcipher_key)` (or equivalent signature — see CS-001). Behaviour preserved: download-transport failure → `AuthenticationError::VaultHeaderInvalid`; decrypt/integrity failure → `AuthenticationError::InvalidCredentials`; staging and destination-file cleanup on error. | None. |
| **C-5 Test helper `upload_manifest_backup_payload_for` uses a fake SQL payload; if C-2 migrates the real format, the `"not valid sql"` corruption test still needs to exercise the integrity-check branch.** | `src-tauri/src/auth/ceremonies/test_support.rs:155-182`, `recover_vault.rs:330-357` | Dropping real VACUUM output in place of stub SQL bytes silently changes what `test_recover_vault_failed_import_cleans_temp_and_keeps_destination_absent` exercises. | Non-blocking | Replace the fake payload path: `upload_manifest_backup_for(vault)` produces a real VACUUM-INTO export of `vault.vault_db_path` and encrypts it via the new upload helper (or its primitives) into the mock cloud. `upload_manifest_backup_payload_for(vault, payload)` keeps a direct-bytes variant for corruption testing — the corrupted bytes exercise the integrity-check branch in `download_manifest_backup` (which must now return `CryptoError::DecryptionFailed` **or** a new `ManifestBackupSyncError::IntegrityCheckFailed` variant — see CS-002). The recover-vault test's expected outcome (`InvalidCredentials`) is preserved by the ceremony's error mapping. | None. |
| **C-6 `import_manifest_sql_atomic` becomes dead code once the download path writes DB bytes directly.** | `src-tauri/src/auth/ceremonies/helpers.rs:132-162` | Leaving the function behind invites future callers to re-adopt the SQL-text stub. | Non-blocking | Delete `import_manifest_sql_atomic` when the last caller (`recover_vault.rs`, `recover_with_phrase.rs`) migrates to `download_manifest_backup`. Also delete its direct callers' dead `sqlcipher_key` / `plaintext` handling once the helper owns it. | None (function deletion is self-documenting). |
| **C-7 Security-reviewer is required for 4.4 per the roadmap; sub-phase confirms.** | `4.4-manifest-backup.md:69-75`, `sub-phases/roadmap.md:112` | Skipping security review violates the sub-phase's explicit gate. | Non-blocking | `/implement-plan` must spawn `security-reviewer` on the resulting diff. Scope captured in Section 2 "Security review" paragraph and echoed in Section 6. | None. |
| **C-8 `VACUUM INTO` concurrency: sub-phase does not address whether the export runs against the active `MetadataStore` connection or a second connection on the same DB file.** | `4.4-manifest-backup.md:61` | If `upload_manifest_backup` opens a second SQLCipher connection on a DB already held open by `MetadataStore`, SQLite file-locking semantics apply: `VACUUM INTO` acquires a reserved lock; concurrent writes on the same file will serialize. Crash safety is preserved, but API shape matters — does the helper take an open `&Connection`, a `&SqlCipherMetadataStore`, a path + `SqlcipherKey`, or does it hold ownership of a short-lived connection? | Non-blocking | Assumption A-1: `upload_manifest_backup` takes the **vault DB path + `SqlcipherKey` (borrowed)** and opens a fresh `rusqlite::Connection` via the existing `open_sqlcipher(path, key)` helper (to be relocated out of `auth::ceremonies::helpers` — see A-4). This keeps the helper trait-independent (no `MetadataStore` coupling), lets Phase 4.5 call it from the push flow without owning a persistent connection, and relies on SQLCipher's internal file locking to serialize against any concurrent writer. `VACUUM INTO` is a read-only-at-target operation; it does not invalidate the existing open connection. | None (internal implementation detail). |
| **C-9 Temp-file location ambiguity: sub-phase says "Arx Runa staging directory" but the repo has two (`storage::staging::default_staging_directory` under `dirs::data_dir()` vs `auth::staging::staging_directory` under `dirs::config_dir()`).** | `4.4-manifest-backup.md:64`, `src-tauri/src/storage/staging.rs:13-17`, `src-tauri/src/auth/staging.rs` | Picking `auth::staging` couples manifest-backup plumbing to auth's config-dir staging (same as ceremony metadata); picking `storage::staging` keeps all chunk- and manifest-scoped temp files under a single directory and aligns with the design's "staging directory (from Phase 3)" prose. | Non-blocking | Assumption A-2: temp files for `upload_manifest_backup` / `download_manifest_backup` live under **`storage::staging::default_staging_directory()`**; the helpers accept a `staging_dir: &Path` parameter (matching `upload_vault_header`'s 4.3 shape). Ceremony callers pass the result of `storage::staging::default_staging_directory()` (creating the directory via `storage::staging::ensure_staging_directory` before the call). The filenames are `manifest-export.db` (upload VACUUM target), `manifest-backup-staging.blob` (upload ciphertext staging), and `manifest-backup-download.blob` (download ciphertext staging); all three are removed on every exit path. | None. |

No blocking concerns. `status: draft` in frontmatter; the Phase 4.4 charter is explicitly to clean up the Phase 2.4 stubs, and no Contract Surface contradictions exist.

## 4. Assumptions

- **A-1 Connection model for `VACUUM INTO`.** `upload_manifest_backup` takes the vault DB path and a `SqlcipherKey` (borrowed), opens its own short-lived SQLCipher connection via the relocated `open_sqlcipher` helper (see A-4), runs `VACUUM INTO '<staging_dir>/manifest-export.db'`, closes the connection, and reads the resulting `.db` file into a `Zeroizing<Vec<u8>>`. No coupling to `MetadataStore`; SQLCipher's file-locking serializes against a concurrent writer.
- **A-2 Staging directory.** Temp files live under `storage::staging::default_staging_directory()` with filenames `manifest-export.db`, `manifest-backup-staging.blob`, `manifest-backup-download.blob`. All three are removed on every exit path (success and error). Owner-only 0o600 permissions on Unix via the existing `storage::staging::write_owner_only` helper (or equivalent for the VACUUM-INTO target — `VACUUM INTO` creates the file itself, so a post-creation `std::fs::set_permissions(..., 0o600)` step is needed on Unix).
- **A-3 Integrity check.** After `decrypt_manifest_backup`, the download path writes plaintext bytes to `<destination_db_path>` and opens a SQLCipher connection with `sqlcipher_key` via the relocated `open_sqlcipher` helper. Integrity is verified by `SELECT COUNT(*) FROM sqlite_master` (a minimally invasive probe that forces SQLCipher to read and decrypt the root page). `PRAGMA cipher_integrity_check` is considered but treated as optional — the root-page read is sufficient to reject wrong-key or truncated DBs, and `cipher_integrity_check` can cost seconds on a large DB. On integrity failure, the destination DB file is deleted and the helper returns a dedicated error variant.
- **A-4 Helper relocation for `open_sqlcipher`.** The existing `src-tauri/src/auth/ceremonies/helpers.rs:192-223` function `open_sqlcipher` is relocated to a crate-internal module accessible from both `auth::ceremonies` and `storage::cloud::manifest_backup`. Target path: `src-tauri/src/storage/sqlcipher/open.rs` (the storage-side SQLCipher module), re-exported as `crate::storage::sqlcipher::open_sqlcipher`. The ceremonies module re-imports it from there. This avoids introducing a cycle (`storage/cloud` must not depend on `auth`).
- **A-5 Error boundary.** The helpers return a new `ManifestBackupSyncError` (thiserror, `#[non_exhaustive]`) with variants `{ StagingIo(String), Vacuum(String), ExportRead(String), CryptoFailed, Transport(#[from] CloudTransportError), IntegrityCheckFailed, DbPersistIo(String) }`. Ceremonies map all variants to `AuthenticationError::InvalidCredentials` on decrypt/integrity failure and `AuthenticationError::VaultHeaderInvalid` on transport/IO/vacuum failure, preserving the current Phase 2.4 error contract exactly. No new `AuthenticationError` variants required.
- **A-6 Plaintext buffer lifetime.** The full-DB plaintext buffer is wrapped in `Zeroizing<Vec<u8>>` throughout — both during upload (after `VACUUM INTO` + read) and during download (after decrypt, before write-to-disk). Zeroize happens on drop after the write-to-disk syscall returns. The wire-format buffer on the way to/from cloud is **not** secret (ciphertext), so `Vec<u8>` is acceptable there.
- **A-7 Nonce handling.** No new nonce-generation code; `encrypt_manifest_backup` (Phase 2.4) already uses `crate::crypto::nonce::generate_nonce()` (CSPRNG). Phase 4.4 does not regenerate or cache nonces — every upload produces a fresh 24-byte nonce.
- **A-8 Idempotent upload.** `upload_manifest_backup` overwrites `manifest/manifest-backup.blob` on every call (matches `CloudTransport::upload_blob` semantics and the design's "single file overwritten on each push"). No special handling for first-push.
- **A-9 Destination DB path pre-check.** `download_manifest_backup` requires the destination path not to exist before it writes (matches `recover_vault`'s existing `precheck_recovery_destination` semantics). The helper itself enforces this to keep the callers thin.
- **A-10 Manifest-backup constant visibility.** `MANIFEST_BACKUP_BLOB_NAME` becomes `pub const` in `storage::cloud::manifest_backup` and is re-exported from `storage::cloud`. No `pub(crate)` or `pub(super)` downgrade — it is the canonical cloud-layout constant, peer of `VAULT_HEADER_BLOB_NAME`.

## 5. Approach

### CONTRACT_SNIPPETS

**CS-001** — `upload_manifest_backup` / `download_manifest_backup` signatures (new, `storage::cloud::manifest_backup`):

```rust
/// Exports the vault's SQLCipher manifest via `VACUUM INTO`, encrypts the
/// snapshot under `manifest_key`, and uploads it to the canonical cloud
/// path `manifest/manifest-backup.blob`. Stage files are cleaned on every
/// exit path.
pub async fn upload_manifest_backup(
    vault_db_path: &Path,
    sqlcipher_key: &SqlcipherKey,
    manifest_key: &[u8; 32],
    cloud_transport: &dyn CloudTransport,
    staging_dir: &Path,
) -> Result<(), ManifestBackupSyncError>;

/// Downloads `manifest/manifest-backup.blob`, decrypts under `manifest_key`,
/// writes the decrypted bytes to `destination_db_path` as a SQLCipher DB,
/// and verifies integrity by opening the DB with `sqlcipher_key` and reading
/// `sqlite_master`. On any failure the destination file is removed.
pub async fn download_manifest_backup(
    cloud_transport: &dyn CloudTransport,
    staging_dir: &Path,
    manifest_key: &[u8; 32],
    destination_db_path: &Path,
    sqlcipher_key: &SqlcipherKey,
) -> Result<(), ManifestBackupSyncError>;
```

**CS-002** — `ManifestBackupSyncError` (new, `storage::cloud::manifest_backup`):

```rust
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ManifestBackupSyncError {
    #[error("manifest-backup staging I/O failed: {0}")]
    StagingIo(String),
    #[error("manifest VACUUM INTO failed: {0}")]
    Vacuum(String),
    #[error("manifest export read failed: {0}")]
    ExportRead(String),
    #[error("manifest-backup cryptographic operation failed")]
    CryptoFailed,
    #[error("manifest-backup cloud transport failed: {0}")]
    Transport(#[from] CloudTransportError),
    #[error("manifest-backup integrity check failed")]
    IntegrityCheckFailed,
    #[error("manifest destination DB persist I/O failed: {0}")]
    DbPersistIo(String),
}
```

**CS-003** — Canonical cloud-path constants (relocate + promote):

```rust
// storage/cloud/manifest_backup.rs
pub const MANIFEST_BACKUP_BLOB_NAME: &str = "manifest/manifest-backup.blob";
pub(crate) const MANIFEST_BACKUP_UPLOAD_STAGING_FILE_NAME: &str = "manifest-backup-staging.blob";
pub(crate) const MANIFEST_BACKUP_DOWNLOAD_STAGING_FILE_NAME: &str = "manifest-backup-download.blob";
pub(crate) const MANIFEST_EXPORT_FILE_NAME: &str = "manifest-export.db";
```

Re-exported from `storage::cloud`:

```rust
// storage/cloud/mod.rs (additions to existing re-export block)
pub use manifest_backup::{
    MANIFEST_BACKUP_BLOB_NAME, ManifestBackupSyncError,
    download_manifest_backup, upload_manifest_backup,
};
```

**CS-004** — Relocated `open_sqlcipher` helper (move out of `auth::ceremonies::helpers`):

```rust
// storage/sqlcipher/open.rs (new file)
pub fn open_sqlcipher(path: &Path, sqlcipher_key: &[u8; 32]) -> Result<Connection, SqlcipherOpenError>;

#[derive(Debug, thiserror::Error)]
pub enum SqlcipherOpenError {
    #[error("failed to open SQLCipher database: {0}")]
    Open(String),
    #[error("SQLCipher key rejected")]
    KeyRejected,
}
```

Re-exported as `crate::storage::sqlcipher::open_sqlcipher`; `auth::ceremonies::helpers::open_sqlcipher` is removed and its ceremony callers re-import from the new path. Error translation stays at the call sites: ceremonies map `SqlcipherOpenError::KeyRejected → AuthenticationError::InvalidCredentials`, `Open → AuthenticationError::VaultHeaderInvalid`.

**CS-005** — Retained primitives from Phase 2.4 (unchanged; `manifest_backup.rs`):

```rust
pub(crate) fn encrypt_manifest_backup(plaintext: Vec<u8>, manifest_key: &[u8; 32]) -> Result<Vec<u8>, CryptoError>;
pub(crate) fn decrypt_manifest_backup(wire: &[u8], manifest_key: &[u8; 32]) -> Result<Zeroizing<Vec<u8>>, CryptoError>;
```

These are the inner AEAD primitives; the public `upload_manifest_backup` and `download_manifest_backup` call them.

### Implementation steps

**Step 1 — Relocate `open_sqlcipher` (prerequisite for Step 2).** Create `src-tauri/src/storage/sqlcipher/open.rs` with CS-004. Introduce a `mod open;` / `pub use open::{open_sqlcipher, SqlcipherOpenError};` in `src-tauri/src/storage/sqlcipher/mod.rs` (or create the module re-export file if the `sqlcipher` folder does not exist yet — confirm via `Grep "pub mod sqlcipher"` on `storage/mod.rs` before proceeding). Delete `auth::ceremonies::helpers::open_sqlcipher` (lines 192-223). Update callers in `auth::ceremonies::helpers::import_manifest_sql_atomic` (will be deleted in Step 4), `auth::ceremonies::helpers::verify_credentials_via_identity_row`, and any other ceremony call sites to re-import from `crate::storage::sqlcipher::open_sqlcipher`. Keep the SAFETY comments on the `unsafe { sqlite3_key(...) }` block verbatim.

**Step 2 — Add `ManifestBackupSyncError` and cloud-layout constants.** In `src-tauri/src/storage/cloud/manifest_backup.rs`, add CS-002 at module top. Add CS-003 constants immediately after. Update the module-level comment (`manifest_backup.rs:1-11`) to drop the "Phase 4.4 will own…" forward declaration and describe the full surface.

**Step 3 — Implement `upload_manifest_backup`.** In `src-tauri/src/storage/cloud/manifest_backup.rs`, add CS-001 signature with this body sequence:
1. Ensure `staging_dir` exists (delegate to `storage::staging::ensure_staging_directory`).
2. Compute `export_path = staging_dir.join(MANIFEST_EXPORT_FILE_NAME)`; best-effort remove any prior export file (tolerate `NotFound`).
3. Inside `tokio::task::spawn_blocking`: open a fresh SQLCipher connection on `vault_db_path` via `open_sqlcipher`, execute `VACUUM INTO 'export_path'` via `conn.execute(...)` with single-quoted path escaping. Handle all failures → `ManifestBackupSyncError::Vacuum`. Drop the connection.
4. Set `0o600` permissions on `export_path` on Unix via `std::fs::set_permissions` inside the blocking task.
5. Read the export file into `plaintext: Zeroizing<Vec<u8>>` via `tokio::fs::read` → `Zeroizing::new(...)`. Handle I/O errors → `ManifestBackupSyncError::ExportRead`. Delete `export_path` before proceeding (tolerate missing).
6. Call `encrypt_manifest_backup((*plaintext).clone(), manifest_key)` → `wire: Vec<u8>`. Handle `CryptoError` → `ManifestBackupSyncError::CryptoFailed`. Drop the `plaintext` binding immediately after the call (Zeroize on drop).
7. Write `wire` to `staging_dir.join(MANIFEST_BACKUP_UPLOAD_STAGING_FILE_NAME)` via `storage::staging::write_owner_only`. Handle I/O → `ManifestBackupSyncError::StagingIo`.
8. `cloud_transport.upload_blob(&staging_path, MANIFEST_BACKUP_BLOB_NAME).await?` → `ManifestBackupSyncError::Transport` on failure.
9. Remove the upload staging file (tolerate `NotFound`; log at `warn!` if unexpected). On transport failure path, also remove the staging file before returning.
10. Return `Ok(())`.

**Step 4 — Implement `download_manifest_backup` and retire `import_manifest_sql_atomic`.** In `src-tauri/src/storage/cloud/manifest_backup.rs`, add CS-001 download signature with this body sequence:
1. Ensure `staging_dir` exists.
2. Pre-check `destination_db_path` does not exist → `ManifestBackupSyncError::DbPersistIo("destination exists")` if it does.
3. Compute `download_staging = staging_dir.join(MANIFEST_BACKUP_DOWNLOAD_STAGING_FILE_NAME)`; best-effort remove any prior file.
4. `cloud_transport.download_blob(MANIFEST_BACKUP_BLOB_NAME, &download_staging).await` → `Transport` on failure. On failure path, also remove the staging file before returning.
5. `let wire = tokio::fs::read(&download_staging).await?;` → `StagingIo` on failure. Remove `download_staging` immediately after the read (tolerate `NotFound`).
6. `let plaintext: Zeroizing<Vec<u8>> = decrypt_manifest_backup(&wire, manifest_key)?;` → `CryptoFailed`.
7. Ensure `destination_db_path.parent()` exists (`tokio::fs::create_dir_all`).
8. Inside `tokio::task::spawn_blocking`: atomically write `plaintext.as_slice()` to `destination_db_path` via `tempfile::NamedTempFile::new_in(parent) + write_all + sync_all + persist_noclobber`. Apply `0o600` perms on Unix. Errors → `DbPersistIo`.
9. Drop `plaintext` (Zeroize on drop).
10. Open the just-written DB with `open_sqlcipher(destination_db_path, sqlcipher_key.expose())`. On failure → remove `destination_db_path` (tolerate `NotFound`) → return `IntegrityCheckFailed`.
11. Run `conn.query_row("SELECT COUNT(*) FROM sqlite_master", [], |row| row.get::<_, i64>(0))`. On failure → remove `destination_db_path` → return `IntegrityCheckFailed`.
12. Drop the connection. Return `Ok(())`.

Delete `auth::ceremonies::helpers::import_manifest_sql_atomic` and its imports (`use rusqlite::Connection; use rusqlite::ffi;` may still be needed for `verify_credentials_via_identity_row` — keep them).

**Step 5 — Re-export from `storage::cloud`.** In `src-tauri/src/storage/cloud/mod.rs`, add the CS-003 `pub use manifest_backup::{…}` block next to the existing `pub use vault_header_io::{…}` block.

**Step 6 — Remove the drifted constant from `auth::ceremonies`.** In `src-tauri/src/auth/ceremonies/mod.rs`, delete line 29 (`pub(super) const MANIFEST_BACKUP_BLOB_NAME …`). Update the module-level doc comment (lines 1-12) if it references the constant (it does not currently). Leave `VAULT_HEADER_BLOB_NAME` alone — Phase 4.3 already canonicalised that.

**Step 7 — Refactor `recover_vault` to call `download_manifest_backup`.** In `src-tauri/src/auth/ceremonies/recover_vault.rs`:
- Replace `use super::{MANIFEST_BACKUP_BLOB_NAME, VAULT_HEADER_BLOB_NAME};` with `use crate::storage::cloud::{MANIFEST_BACKUP_BLOB_NAME, VAULT_HEADER_BLOB_NAME, ManifestBackupSyncError, download_manifest_backup};`.
- Remove the inlined download block (lines 100-118): `staging::write_owner_only`, `cloud_transport.download_blob`, `tokio::fs::read`, `decrypt_manifest_backup` and the `backup_download_path` variable.
- Replace the `import_manifest_sql_atomic(...)` call (lines 120-125) with a `download_manifest_backup(cloud_transport, &storage_staging_dir, session_keys.manifest_key.expose(), &request.vault_db_path, &sqlcipher_key).await` call. Introduce `let storage_staging_dir = storage::staging::default_staging_directory().map_err(|_| AuthenticationError::VaultHeaderInvalid)?; storage::staging::ensure_staging_directory(&storage_staging_dir).await.map_err(|_| AuthenticationError::VaultHeaderInvalid)?;` before the call. Map `ManifestBackupSyncError` variants as documented in A-5. The `precheck_recovery_destination` call (line 25) remains — it runs before any cloud work for early rejection.
- Delete the now-unused `backup_download_path` variable and its cleanup calls.

**Step 8 — Refactor `recover_with_phrase` to call `download_manifest_backup`.** In `src-tauri/src/auth/ceremonies/recover_with_phrase.rs`, apply the equivalent changes to Step 7 (same `MANIFEST_BACKUP_BLOB_NAME` import relocation, same inlined-download removal, same `download_manifest_backup` call substitution, same error mapping).

**Step 9 — Update `test_support`.** In `src-tauri/src/auth/ceremonies/test_support.rs`:
- Update the import at line ~15 to `use crate::storage::cloud::{MANIFEST_BACKUP_BLOB_NAME, ...};`.
- Rewrite `upload_manifest_backup_for(vault)` to call the new `upload_manifest_backup` with a per-vault `storage::staging::default_staging_directory()` override (use a `tempfile::tempdir` per call to isolate tests). The source DB is `vault.vault_db_path`; the mock cloud is `vault.cloud`; the `manifest_key` is re-derived from the known test password + `vault.header` (mirror the existing derivation block at lines 164-169).
- Keep `upload_manifest_backup_payload_for(vault, payload: &[u8])` for corruption tests but change its implementation to write a caller-provided raw byte buffer through `encrypt_manifest_backup` + `upload_blob` directly (bypassing VACUUM-INTO). This preserves the `b"not valid sql"` test case as an integrity-check failure driver. Document the bypass with a `#[doc(hidden)]` or comment explaining the test-only shape.

**Step 10 — Write tests.** Append to the `#[cfg(test)] mod tests` block in `src-tauri/src/storage/cloud/manifest_backup.rs`:
- `test_upload_manifest_backup_writes_canonical_remote_path_and_cleans_staging`.
- `test_upload_manifest_backup_transport_failure_removes_staging_file`.
- `test_upload_manifest_backup_encrypt_upload_download_decrypt_round_trip` (mirrors the design check-list item `design.md:1273` — the canonical test named there).
- `test_download_manifest_backup_wrong_key_returns_integrity_check_failed_and_removes_destination`.
- `test_download_manifest_backup_truncated_wire_returns_crypto_failed`.
- `test_download_manifest_backup_destination_exists_returns_db_persist_io_without_touching_cloud`.
- `test_download_manifest_backup_transport_not_found_returns_transport_error_and_no_partial_files`.
- Seed data helper: construct a real SQLCipher DB via `SqlCipherMetadataStore::create` (mirrors existing `storage::staging` tests) so round-trip tests exercise a real DB (not the Phase 2.4 SQL-text stub). Use `MockCloudTransport` for transport.
- Keep the existing `encrypt_manifest_backup` / `decrypt_manifest_backup` primitive tests intact — they stay useful.

**Step 11 — Verify ceremony tests still pass.** After Step 7/8/9, run `cargo test --workspace --all-targets --all-features` and specifically verify:
- `test_recover_vault_reconstructs_session_from_cloud_header_and_manifest_backup` still passes.
- `test_recover_vault_failed_import_cleans_temp_and_keeps_destination_absent` still passes with the `payload_for` bypass (error mapping preserves `AuthenticationError::InvalidCredentials`).
- `test_recover_vault_manifest_backup_missing_returns_vault_header_invalid` still passes.
- Equivalent `recover_with_phrase` tests still pass.

**Step 12 — Governance sync.** See Section 8.

**Step 13 — Security review.** Spawn `security-reviewer` on the diff. Scope per Section 2 "Security review" paragraph. Address findings before declaring the sub-phase complete.

## 6. Review focus areas

### 6a — Rust change surface

Anticipated files under `src-tauri/**/*.rs`:

- `src-tauri/src/storage/cloud/manifest_backup.rs` (add CS-001, CS-002, CS-003; keep CS-005; update module comment).
- `src-tauri/src/storage/cloud/mod.rs` (add `pub use manifest_backup::{…}` re-export block).
- `src-tauri/src/storage/sqlcipher/open.rs` (new — CS-004).
- `src-tauri/src/storage/sqlcipher/mod.rs` (register the new module + re-export — *create file if absent*).
- `src-tauri/src/auth/ceremonies/mod.rs` (delete `MANIFEST_BACKUP_BLOB_NAME`).
- `src-tauri/src/auth/ceremonies/recover_vault.rs` (migrate to `download_manifest_backup`).
- `src-tauri/src/auth/ceremonies/recover_with_phrase.rs` (migrate to `download_manifest_backup`).
- `src-tauri/src/auth/ceremonies/helpers.rs` (delete `import_manifest_sql_atomic`; delete `open_sqlcipher`; re-import from `crate::storage::sqlcipher::open_sqlcipher` in `verify_credentials_via_identity_row` and `rekey_sqlcipher`).
- `src-tauri/src/auth/ceremonies/test_support.rs` (migrate to `upload_manifest_backup`; keep a direct-bytes bypass variant).

### 6b — Security-sensitive paths

All three sensitive subtrees are touched. Reviewer scope per path:

- `src-tauri/src/storage/cloud/manifest_backup.rs` — nonce generation via `generate_nonce()` stays CSPRNG; AAD remains empty (singleton-blob invariant); `Zeroizing<Vec<u8>>` discipline on plaintext; wire-format boundary `[24B nonce | ciphertext | 16B tag]` preserved.
- `src-tauri/src/storage/sqlcipher/open.rs` — `unsafe { ffi::sqlite3_key(...) }` SAFETY comment preserved verbatim from the relocated source; key-slice lifetime guaranteed by the `&[u8; 32]` borrow; no logging of key bytes.
- `src-tauri/src/auth/ceremonies/recover_vault.rs`, `recover_with_phrase.rs` — `manifest_key` never escapes ceremony scope (passed by borrowed `&[u8; 32]` to the new helper); `master_key: Zeroizing<[u8; 32]>` binding unchanged (still dropped at end-of-scope per `.claude/rules/auth.md` ceremonies invariant); destination-DB file on failure must be removed by the helper (download side), not left behind to leak schema metadata.
- `src-tauri/src/storage/staging.rs` (no code change, but the module's owner-only-permissions contract is relied on for the upload staging file and the VACUUM-INTO export file).

### 6c — Architecture risk areas

- **`storage/cloud/manifest_backup.rs` SRP.** The file now owns: primitive AEAD (CS-005), wire-format constants, `ManifestBackupSyncError`, upload orchestration, download orchestration, VACUUM-INTO plumbing, integrity-check plumbing. This matches Phase 4.3's `vault_header_io.rs` shape (single file, ~400 LOC) — acceptable. Reviewer should confirm the file does not exceed ~500 LOC and that no helper becomes a hidden second concern (e.g., a full "SQLCipher export orchestrator"). If VACUUM-INTO plumbing grows beyond ~40 LOC, extract into `storage/cloud/manifest_backup/export.rs` and keep the public surface in `manifest_backup.rs` via `mod export;`.
- **Dependency direction.** `storage/cloud/manifest_backup` → `storage/sqlcipher::open_sqlcipher` → `rusqlite::ffi`. No cycle. `auth::ceremonies::*` depends on `storage::cloud::{manifest_backup, vault_header_io}` (already the direction in Phase 4.3). Reviewer should confirm no `storage::cloud` module imports from `auth::*`.
- **Re-export hygiene.** `storage::cloud::mod.rs` must re-export only the public surface: `MANIFEST_BACKUP_BLOB_NAME`, `ManifestBackupSyncError`, `upload_manifest_backup`, `download_manifest_backup`. Staging-file constants stay `pub(crate)`.
- **Module visibility discipline.** `encrypt_manifest_backup` / `decrypt_manifest_backup` stay `pub(crate)` — they are the inner primitives; external callers must go through the orchestration functions. Reviewer should flag any attempt to promote them to `pub`.
- **Drift-check anchor.** Any sensitive-file touched outside Section 6b triggers a Plan Deviation.

### 6d — Testing requirements

**Validation checkpoint from sub-roadmap** (`4.4-manifest-backup.md:33-48`):
- Automated: `cargo test storage::cloud::manifest_backup`.
- Manual: upload to real cloud, verify blob is encrypted (not plaintext SQLite), download + decrypt + open SQLCipher.
- Acceptance criteria: encrypted before upload, decryption produces valid SQLCipher DB, zeroization of manifest buffer occurs after use.

**Tests to write** (see Step 10 for exact names):
1. Round-trip with real SQLCipher DB seed and `MockCloudTransport`.
2. Wrong `manifest_key` → `IntegrityCheckFailed` (plaintext decrypts to garbage bytes that do not open as a valid SQLCipher DB with `sqlcipher_key`).
3. Wrong `sqlcipher_key` (correct `manifest_key`) → `IntegrityCheckFailed` (destination file written, open fails).
4. Truncated ciphertext → `CryptoFailed`.
5. Corrupted tag → `CryptoFailed`.
6. Upload-transport failure → `Transport` + staging file removed.
7. Download-transport `NotFound` → `Transport(NotFound)` + no destination file written.
8. Destination DB exists → `DbPersistIo` returned before cloud transport is invoked (defence-in-depth check).
9. Staging I/O failure on upload (simulate via `staging_dir = <read-only path>`) → `StagingIo`.
10. Ceremony-level: `test_recover_vault_reconstructs_session_from_cloud_header_and_manifest_backup` continues to pass after migration (round-trip through real VACUUM-INTO path).
11. Ceremony-level: `test_recover_vault_failed_import_cleans_temp_and_keeps_destination_absent` continues to pass — the test's `b"not valid sql"` payload path now flows through `upload_manifest_backup_payload_for`'s direct-bytes bypass and is caught as `IntegrityCheckFailed` → mapped to `AuthenticationError::InvalidCredentials`.
12. Edge case from Step 2 C-9: staging directory auto-creation works on first use.

**Boundary cases that matter** (from Step 2 adversarial review):
- `VACUUM INTO` file path containing a single quote (SQLite escaping) — the implementation must reject or escape. Since the staging directory path is controlled by the application, this is a sanity check; flag in review if path interpolation is raw-concatenated.
- Partial-write atomicity on download: a crash between `tokio::fs::write` and the integrity-check must leave no observable destination DB. `NamedTempFile + persist_noclobber` gives this.
- Concurrent two-device push: the helper is not responsible for the `snapshot_counter` race — that is Phase 4.5's conflict-detection responsibility. 4.4 tests must not assume conflict handling.

## 7. Documentation impact

- **Required this run** — `src-tauri/src/storage/cloud/manifest_backup.rs:1-11` module comment: remove the "Phase 4.4 will own the full manifest-backup schema, streaming, and versioning" forward declaration; describe the current surface (primitives + orchestration + cloud-layout constant).
- **Required this run** — `src-tauri/src/auth/ceremonies/helpers.rs` module-level comment (if it mentions manifest-SQL import): update when `import_manifest_sql_atomic` is deleted. Current comment (`helpers.rs:1`) is just "Internal helpers for ceremony flows." — no change needed.
- **Required this run** — `src-tauri/src/auth/ceremonies/mod.rs:1-12` module doc comment: no textual reference to `MANIFEST_BACKUP_BLOB_NAME` today, but the constants block comment on line 26 must be edited to drop the deleted line.
- **Required this run** — `.claude/rules/storage.md` Cloud-backup section: confirm wording still accurate after the helper relocation. Current text: "Push flow uploads manifest backup, then uploads vault header idempotently on every push." — still true. Add one line noting the canonical path: "Manifest backup blob path is `manifest/manifest-backup.blob` (constant owned by `storage::cloud::manifest_backup::MANIFEST_BACKUP_BLOB_NAME`)." This closes the same anchor pattern as Phase 4.3's `VAULT_HEADER_BLOB_NAME` reference.
- **Required this run** — `.claude/rules/auth.md` Ceremonies section: the forward-declaration bullet (line 58) mentions `VaultHeader` and `CloudTransport` forward declarations; add a parallel line: "`MANIFEST_BACKUP_BLOB_NAME` originates in `storage::cloud::manifest_backup` in Phase 4.4; do not re-declare in `auth::ceremonies`."
- **Deferred/optional** — `docs/architecture/designs/cloud-synchronisation/design.md` line 815 footnote: the existing step-list prose is already correct. No textual update required unless the reviewer requests clarification of the integrity-check step (design says step 7: "Open SQLCipher DB with sqlcipher_key to verify integrity" — aligned with implementation). Rationale for deferral: the design is canonical and already specifies the flow verbatim; adding implementation prose here would drift.
- **Deferred/optional** — Add a Phase 4.4 "last verified against design dated 2026-04-08" freshness stamp to `.claude/rules/storage.md` top matter (`storage.md:8`). Rationale for deferral: Phase 4.4 does not change the design, only implements the section that was already verified on 2026-04-08.
- **Deferred/optional** — `docs/architecture/designs/cloud-synchronisation/sub-phases/4.4-manifest-backup.md` implementation note about "Use `rusqlite::Connection::backup_to_path` or `VACUUM INTO`": narrow to `VACUUM INTO` now that the approach is selected. Rationale for deferral: sub-phase docs intentionally leave implementation flexibility to the plan; not load-bearing.

## 8. Governance sync actions (pre-implementation)

| Action ID | Reason / linked concern | Target files | Required edit | Verification |
|---|---|---|---|---|
| **G-1** | C-1 / C-3: constant moves out of `auth::ceremonies`. | `src-tauri/src/auth/ceremonies/mod.rs` | Delete line 29 (`pub(super) const MANIFEST_BACKUP_BLOB_NAME: &str = "manifest-backup.enc";`) in Step 6. Ensure no remaining callers via `Grep "MANIFEST_BACKUP_BLOB_NAME"` in `src-tauri/src/auth/` before deletion. | `cargo check --workspace --all-features` — compilation must pass. `Grep "manifest-backup\.enc"` returns zero matches under `src-tauri/`. |
| **G-2** | C-1 documentation: update module comment. | `src-tauri/src/storage/cloud/manifest_backup.rs:1-11` | Rewrite module-level `//!` block: remove "Phase 4.4 will own…" forward declaration; describe current responsibilities (primitives, orchestration, constants, error type); note cross-reference to `docs/architecture/designs/cloud-synchronisation/design.md#manifest-cloud-backup`. | Manual diff review; module still compiles. |
| **G-3** | C-2 / C-6: `import_manifest_sql_atomic` retirement. | `src-tauri/src/auth/ceremonies/helpers.rs:132-162` | Delete the function and its three local imports (`rusqlite::Connection`, `rusqlite::ffi`, `std::str::from_utf8`) if and only if no other caller references them after Step 7/8 — confirm with `Grep`. `Connection` and `ffi` are still needed by `verify_credentials_via_identity_row` and `rekey_sqlcipher`; keep them. | Compilation passes. `Grep "import_manifest_sql_atomic"` returns zero matches. |
| **G-4** | Update `.claude/rules/storage.md` Cloud-backup anchor. | `.claude/rules/storage.md` (Cloud backup section, around line 40) | Add one bullet: `- Manifest backup blob path is \`manifest/manifest-backup.blob\` (constant owned by \`storage::cloud::manifest_backup::MANIFEST_BACKUP_BLOB_NAME\`).` | `Grep "manifest/manifest-backup\.blob"` shows the new rule line. |
| **G-5** | Update `.claude/rules/auth.md` Ceremonies forward-declaration. | `.claude/rules/auth.md` (Ceremonies section, around line 58) | Append to the forward-declarations bullet: `\`MANIFEST_BACKUP_BLOB_NAME\` originates in \`storage::cloud::manifest_backup\` in Phase 4.4; do not re-declare in \`auth::ceremonies\`.` | `Grep "MANIFEST_BACKUP_BLOB_NAME"` inside `.claude/rules/auth.md` returns the new line. |
| **G-6** | Run `/copilot-sync` after rule edits. | N/A | After G-4 and G-5 land, run `/copilot-sync` to propagate governance edits to the derived copilot/instruction files. | `/copilot-sync` completes without error and reports updated instruction files. |

## 9. Handoff Notes for Implementer

Working directory is `C:\Users\chris\source\repos\arx-runa`. Plan is self-contained — you do **not** need to re-read the sub-phase doc once this plan is loaded; CS-001 through CS-005 plus Sections 4–5 cover all signatures, file paths, and ordering decisions. Execute Steps 1–13 in order: Step 1 (relocate `open_sqlcipher`) is a hard prerequisite for Steps 3–4, and Steps 7–9 (ceremony + test-support migrations) must wait until Steps 2–5 (new helper surface) compile. Do not touch `VAULT_HEADER_BLOB_NAME` — Phase 4.3 owns it. Do not re-derive `manifest_key`; it is already in `SessionKeys` (`src-tauri/src/auth/session/keys.rs:17`) and is expected to be borrowed as `&[u8; 32]` via `.expose()`. Traps: (a) SQLCipher `VACUUM INTO` needs the destination path escaped in the SQL string — prefer `conn.execute("VACUUM INTO ?1", [export_path.to_string_lossy().as_ref()])` style if `rusqlite` supports bound parameters for this statement; otherwise validate the staging-directory path does not contain `'` before string interpolation. (b) On Windows, `std::fs::set_permissions(0o600)` is a no-op — the 0o600 discipline is Unix-only; the `cfg(unix)` guards in `storage::staging::write_owner_only` are the template to follow. (c) Test suite command per user preference: `cargo test --workspace --all-targets --all-features` (full run, not narrow). (d) After implementation, spawn `security-reviewer` per C-7 / Step 13 — the sub-phase gate is mandatory. (e) The `test_recover_vault_failed_import_cleans_temp_and_keeps_destination_absent` test relies on `upload_manifest_backup_payload_for`'s direct-bytes bypass path — keep that variant in `test_support`; do not collapse it into the real-VACUUM helper.

## Implementation Log

- **Date**: 2026-04-19T18:18:00+02:00
- **Run ID**: `phase-4-4-manifest-backup-20260419-174221`
- **Track**: `full`
- **Branch**: `development`
- **Execution mode**: Orchestrator-led implementation with delegated review/test agents

| Approach step | Agent | Agent ID | Outcome |
|---|---|---|---|
| Structured context build | plan-context-builder | `plan-digest-builder` | `PLAN_DIGEST` produced |
| Structured context build | rules-extractor | `rules-index-builder` | `RULES_INDEX` produced |
| Structured context build | design-extractor | `design-index-builder-1` | `DESIGN_INDEX` produced |
| Structured context build | shard-planner | `shard-map-builder-1` | `SHARD_MAP` + `SHARD_DIGEST_SUMMARY[]` produced |
| Rust review (cycle 2/3) | rust-reviewer | `phase44-rust-review-r3` | No actionable findings |
| Security review (cycle 3) | security-reviewer | `phase44-security-review-r4` | No security findings |
| Architecture review (cycle 3) | architecture-reviewer | `phase44-architecture-review-r3` | No structural findings |
| Cross-shard review | cross-shard-reviewer | `phase44-cross-shard-review`, `phase44-cross-shard-review-r2` | No cross-shard findings |
| Findings quality gate | finding-classifier | `phase44-finding-classifier` | `CLASSIFIED_FINDINGS` with empty actionable set |
| Test expansion/audit | test-writer | `phase44-test-writer` | Additional adversarial/guard tests added |

- **Files changed**:
  - `.claude/plans/phase-4-4-manifest-backup.md`
  - `.claude/rules/auth.md`
  - `.claude/rules/storage.md`
  - `.github/instructions/auth.instructions.md`
  - `.github/instructions/storage.instructions.md`
  - `.github/instructions/tauri.instructions.md`
  - `docs/architecture/designs/cloud-synchronisation/sub-phases/4.4-manifest-backup.md`
  - `src-tauri/src/auth/ceremonies/helpers.rs`
  - `src-tauri/src/auth/ceremonies/mod.rs`
  - `src-tauri/src/auth/ceremonies/recover_vault.rs`
  - `src-tauri/src/auth/ceremonies/recover_with_phrase.rs`
  - `src-tauri/src/auth/ceremonies/test_support.rs`
  - `src-tauri/src/storage/cloud/manifest_backup.rs`
  - `src-tauri/src/storage/cloud/mod.rs`
  - `src-tauri/src/storage/sqlcipher.rs`
  - `.claude/runs/phase-4-4-manifest-backup-20260419-174221/run-state.json`
  - `.claude/runs/phase-4-4-manifest-backup-20260419-174221/cycle-3.json`

- **Formatting check**: `cargo fmt --all -- --check` passed (after applying `cargo fmt --all`)
- **Clippy results**: `cargo clippy --workspace --all-targets --all-features -- -D warnings` passed
- **Test results**: `cargo test --workspace --all-targets --all-features` passed (`495 passed; 0 failed; 1 ignored` in `arx-runa-tauri` library tests)
- **Release build**: `cargo build --workspace --release` passed
- **Rust review**: No actionable findings in final cycle
- **Architecture review**: No structural findings in final cycle
- **Security review**: No security findings in final cycle
- **Cross-shard review**: 2 invocations, no findings
- **Findings quality gate**: `ACTIONABLE_NOW=0`, `INTENTIONAL_DECISION=0`, `DEFERRED_BY_PLAN=0`, `INSUFFICIENT_EVIDENCE=0`
- **Finding overrides**: None
- **Design challenge outcomes**: None
- **Governance sync**: 6 actions executed; `/copilot-sync` completed and updated derived instruction files (`auth`, `storage`, `tauri`)
- **Sub-phase decisions sync**: `docs/architecture/designs/cloud-synchronisation/sub-phases/4.4-manifest-backup.md` updated with 5 implementation decisions
- **Deviations from plan**:
  - Implemented SQLCipher opener in `src-tauri/src/storage/sqlcipher.rs` (existing module file) instead of introducing a new `src-tauri/src/storage/sqlcipher/open.rs` module path.
  - Retained existing ceremony-local `open_sqlcipher` helper for unchanged ceremony flows outside this sub-phase scope.
- **Documentation flagged**:
  - Deferred/optional — `docs/architecture/designs/cloud-synchronisation/design.md` line 815 footnote clarification remains deferred (design already canonical for this flow).
  - Deferred/optional — freshness stamp update in `.claude/rules/storage.md` remains deferred.
  - Deferred/optional — narrowing sub-phase implementation note (`backup_to_path` vs `VACUUM INTO`) remains deferred.
- **Run state path**: `.claude/runs/phase-4-4-manifest-backup-20260419-174221/`
