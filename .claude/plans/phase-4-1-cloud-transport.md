---
title: "Phase 4.1 — CloudTransport Trait and Mock Implementation"
created: "2026-04-18T20:53:37Z"
status: implemented
roadmap-phase: 4
sub-phase: "4.1"
design-document: "docs/architecture/designs/cloud-synchronisation/design.md"
sub-phase-roadmap: "docs/architecture/designs/cloud-synchronisation/sub-phases/roadmap.md"
governance-sync-required: true
tags: [storage, cloud, trait, phase-4, mock]
---

# Plan: Phase 4.1 — CloudTransport Trait and Mock Implementation

## 1. Goal

Replace the Phase 2.4 forward-declared `CloudTransport` trait, `CloudTransportError`
enum, and `MockCloudTransport` under `src-tauri/src/storage/cloud/` with the canonical
4-method path-based surface defined in the cloud-sync design Contract Surface,
introduce the new `CloudEndpoint` struct, and keep all existing Phase 2.4 ceremony
callers compiling.

## 2. Context

Phase 4.1 is the first implementation unit of the cloud-synchronisation roadmap
(4.1 → 4.2 → 4.3 → 4.4 → 4.5). It depends on Phase 1 (crypto primitives) and Phase 3
(chunking/manifest), both of which are implemented. It has no other Phase 4
prerequisites.

What exists today (2026-04-18):

- `src-tauri/src/storage/cloud/mod.rs` — forward-declared `CloudTransport` trait with
  two methods: `upload_blob(&BlobName, &[u8])` and `download_blob(&BlobName) -> Vec<u8>`,
  and a minimal error enum `CloudTransportError { NotFound, IoError(String), Other(String) }`
  (currently `#[non_exhaustive]`). The module doc-comment explicitly says Phase 4.1
  will expand error variants and add `delete_blob` / `list_blobs`.
- `src-tauri/src/storage/cloud/mock.rs` — `MockCloudTransport` backed by
  `Arc<tokio::sync::Mutex<HashMap<String, Vec<u8>>>>` with inline tests for upload
  round-trip, not-found, and overwrite.
- `src-tauri/src/storage/cloud/vault_header.rs` — already defines `VaultHeader`,
  `Argon2ParamsJson`, `RecoverySlot`, `TrustedVaultHeaderAnchor`,
  `VaultHeaderTrustPolicy`, and `VaultHeaderError`. Phase 4.3 owns further work.
- `src-tauri/src/storage/cloud/manifest_backup.rs` — `encrypt_manifest_backup` /
  `decrypt_manifest_backup` helpers used by Phase 2.4 `recover_vault` and
  `setup_recovery`. Phase 4.4 owns the upload/download flow.
- Call sites that invoke `upload_blob` / `download_blob` today (9 files):
  `src-tauri/src/auth/ceremonies/{create.rs, change_password.rs, rotate_key_file.rs,
  recover_vault.rs, setup_recovery.rs, recover_with_phrase.rs, test_support.rs}`
  and the mock / trait definitions themselves. All callers currently pass
  `&BlobName` and in-memory `&[u8]` or receive `Vec<u8>`.

Deliverables required by the sub-phase (2026-04-02 roadmap, `4.1-cloud-transport.md`):

1. New trait in `src-tauri/src/storage/cloud/mod.rs` with `upload_blob`,
   `download_blob`, `delete_blob`, `list_blobs`.
2. `CloudTransportError` enum with `NotFound`, `AuthenticationFailed`, `Timeout`,
   `IoError`, `RcloneProcessFailed`, `Other`.
3. `CloudEndpoint` struct with `provider`, `bucket`, `region`, `endpoint`,
   `path_prefix` fields, serde-serialisable to JSON.
4. `MockTransport` (a.k.a. `MockCloudTransport`) backed by an in-memory
   `HashMap<String, Vec<u8>>`.
5. Full test suite for mock — upload/download round-trip, delete, list-with-prefix,
   NotFound on missing download, idempotent upload overwrite, idempotent delete on
   missing path.

No pending architectural decisions listed in `docs/roadmap.md` for Phase 4. The
sub-phase cites the Contract Surface as canonical.

## 3. Design Concerns / Open Questions

| # | Concern | Source | Impact | Classification | Resolution | Documentation updates |
|---|---------|--------|--------|----------------|------------|-----------------------|
| 1 | **Signature contradiction between canonical Contract Surface and forward-declared trait.** Canonical trait is path-based: `upload_blob(&Path, &str)` and `download_blob(&str, &Path)`. Existing in-code trait is bytes-based: `upload_blob(&BlobName, &[u8])` and `download_blob(&BlobName) -> Vec<u8>`. | `design.md` §"CloudTransport Trait" lines 88–131 vs `src-tauri/src/storage/cloud/mod.rs:44–50` | All 22+ ceremony call sites across 9 files must migrate to write-to-staging-then-upload / download-to-tempfile-then-read, enlarging Phase 4.1 scope by ~300–500 lines beyond the sub-phase's "~150 prod + ~100 test lines" estimate. | **Resolved (was Blocking)** | **Decision (2026-04-18): option (A) — adopt canonical path-based signatures and migrate all 9 ceremony files.** Rationale: design's "lets Rclone manage I/O internally" is load-bearing for Phase 4.5's per-chunk uploads; ceremonies already write `pending-vault-header.json` to staging for retry semantics so upload migration is argument-swap; tempfile overhead on downloads is acceptable. | None — design already canonical; no design-doc edits. |
| 2 | **`IoError` variant shape mismatch.** Canonical: `IoError(#[from] std::io::Error)`. Current: `IoError(String)`. Two call sites stringify `std::io::Error` into `Other`/`IoError(msg)` today. | `design.md` line 150 vs `mod.rs:30–31` | Changing the variant to `std::io::Error` makes the enum non-`Clone`/non-`PartialEq` unless `#[derive(Debug, Error)]` only (currently the case). All sites that build `CloudTransportError::IoError(String)` must move to `?`-propagation via `#[from]`. | **Non-blocking** if #1 is resolved as (A) or (C); contained to the trait crate. | Adopt canonical form `IoError(#[from] std::io::Error)`. Remove any sites that synthesise `IoError` from arbitrary strings; they become `Other(String)`. | None beyond the trait file. |
| 3 | **Missing `RcloneProcessFailed` semantics for mock.** `RcloneProcessFailed { exit_code, stderr_sanitised }` is Rclone-specific and cannot occur in `MockCloudTransport`. | `design.md` lines 154–158 | Phase 4.1 tests must still be able to construct and match every variant (per `rules/rust.md`: "Every `thiserror` variant must have a test that triggers it"). Mock cannot naturally emit it. | **Non-blocking** | Add a `#[cfg(any(test, feature = "test-utils"))]` failure-injection hook on `MockCloudTransport` (e.g., `inject_failure(path, CloudTransportError)`) that lets tests trigger any variant on demand, matching the design's `failure_paths` concept (design.md lines 1234–1240). | None. Covered in Approach step 5. |
| 4 | **`BlobName` vs `&str` for remote paths.** `.claude/rules/rust.md` mandates newtypes; `BlobName` already exists. Canonical design uses bare `&str`, explicitly to prevent OS-specific separator injection from `PathBuf`. | `design.md` line 170 ("relative paths as `&str`") vs `rules/rust.md` §Structure | If #1 resolves (A), remote paths become `&str`. The `BlobName` newtype (currently holding UUID blob names like `"<uuid>.blob"`) is still needed for chunks under `vault/` but is no longer the trait input type. The trait remains `&str`-keyed; callers compose `format!("vault/{}.blob", blob_name)` or pass literals like `"vault-header.json"`. | **Non-blocking** | Accept `&str` on the trait per canonical design. Keep `BlobName` where it's used today (manifest `chunks.blob_name`, staging filenames). Update `.claude/rules/storage.md` to note the path-vs-blob-name distinction. | Governance sync action in §8 adds a note to `rules/storage.md`. |
| 5 | **Mock naming drift.** Sub-phase deliverable uses `MockTransport`; design testing section uses `MockCloudTransport`; current code uses `MockCloudTransport`. | `4.1-cloud-transport.md` deliverable 4 vs `design.md` line 1234 vs `storage/cloud/mock.rs:22` | Two names in active sources. | **Non-blocking** | Keep `MockCloudTransport` (more descriptive, matches design testing section and existing code). | Note in sub-phase plan only — no design file edits needed. |
| 6 | **Security review claim.** Sub-phase declares "Not required" for 4.1. This file lives under `src-tauri/src/storage/` which CLAUDE guidance flags as security-sensitive. | `4.1-cloud-transport.md` §Security Review | 4.1 produces only a trait surface + in-memory mock + endpoint struct. No real cloud interaction, no credentials, no plaintext persistence. The error enum *shape* does matter (stderr sanitisation, AuthenticationFailed without message) because Phase 4.2 builds on it. | **Non-blocking** | Accept "not required" for the mock/trait introduction. The error-variant shape review happens in Phase 4.2 when `RcloneTransport` constructs the variants from real subprocess output. | None. |
| 7 | **`CloudEndpoint` file location not stated.** Sub-phase lists it in deliverable 3 but does not pin a path. | `4.1-cloud-transport.md` deliverable 3 | Implementer must pick. | **Non-blocking** | Place in `src-tauri/src/storage/cloud/endpoint.rs` with one type, re-exported from `src-tauri/src/storage/cloud/mod.rs`. Matches "one concern per file" rule. | None. |
| 8 | **`list_blobs` prefix semantics.** Design says the mock filters by prefix. Unclear whether `list_blobs("vault/")` should include paths equal to `"vault/"` if stored, or exclude directory-like entries. | `design.md` lines 124–130, 452–454 | Mock behaviour must match Rclone `lsjson --files-only --recursive` semantics for later phases. | **Non-blocking** | Mock returns all stored keys that `starts_with(remote_prefix)` and are not equal to `remote_prefix` itself. No "directories" exist in the HashMap-backed mock, so the filter reduces to `starts_with` + non-empty suffix. Document in `/// ` comment. | None. |

## 4. Assumptions

Concrete choices the plan makes where the sub-phase is silent. Correct these
before `/implement-plan` if wrong.

1. **Resolution #1 is option (A)** — adopt canonical path-based signatures and
   migrate the 9 ceremony call sites in Phase 4.1. Confirmed by user 2026-04-18.
   Rationale: the canonical design is ground truth per CLAUDE.md and the
   sub-phase Contract Anchor; path-based signatures are load-bearing for Phase
   4.5's chunk uploads.
2. The ceremony migration for `upload_blob` reuses the already-existing
   `pending-vault-header.json` staging write in `create.rs`, `change_password.rs`,
   `rotate_key_file.rs`, `setup_recovery.rs`, `recover_with_phrase.rs`. For
   `manifest-backup.enc` uploads in `test_support.rs`, callers write the encrypted
   wire bytes to a fresh tempfile in the staging directory, upload by path, then
   remove the tempfile.
3. For `download_blob`, callers download into a tempfile under
   `dirs::config_dir()/arx-runa/` (already owner-protected by the staging module),
   read + parse, then `remove_if_exists`. Failed reads are treated as I/O errors,
   not `NotFound`.
4. `MockCloudTransport` continues to be gated behind
   `#[cfg(any(test, feature = "test-utils"))]`, unchanged from current practice.
5. `CloudEndpoint` derives `Debug, Clone, Serialize, Deserialize, PartialEq, Eq`.
   `Eq` is added because unit tests must compare endpoints after round-trip
   serialisation.
6. The new `CloudTransportError` is `#[derive(Debug, Error)]` (no `Clone`,
   no `PartialEq`) because `std::io::Error` is not `Clone` and the design does not
   require equality.
7. Remote paths inside the mock are not validated against the
   `^[a-zA-Z0-9._/-]+$` allowlist — that validation is Phase 4.2's
   `RcloneTransport` responsibility. The mock stores arbitrary byte-level keys
   to permit tests that probe boundary encodings.
8. The staging temp path used for ceremony uploads/downloads is
   `dirs::config_dir() / "arx-runa" / STAGING_FILE_NAME` (existing constant),
   reused with a per-ceremony suffix where needed to avoid collisions in
   parallel ceremony execution (none today, but future-proof).
9. `#[non_exhaustive]` is **removed** from `CloudTransportError` because the
   canonical design enumerates the complete set. New variants become breaking
   changes — acceptable per Contract Surface semantics.

## 5. Approach

### `CONTRACT_SNIPPETS` (inline once; reference by ID thereafter)

**CS-001 — Canonical `CloudTransport` trait.** Exact text from
`design.md` §"CloudTransport Trait":
```rust
#[async_trait]
pub trait CloudTransport: Send + Sync {
    async fn upload_blob(&self, local_path: &Path, remote_path: &str) -> Result<(), CloudTransportError>;
    async fn download_blob(&self, remote_path: &str, local_path: &Path) -> Result<(), CloudTransportError>;
    async fn delete_blob(&self, remote_path: &str) -> Result<(), CloudTransportError>;
    async fn list_blobs(&self, remote_prefix: &str) -> Result<Vec<String>, CloudTransportError>;
}
```

**CS-002 — Canonical `CloudTransportError` enum.** Exact text from `design.md`:
```rust
#[derive(thiserror::Error, Debug)]
pub enum CloudTransportError {
    #[error("blob not found at remote path")]
    NotFound,
    #[error("cloud transport authentication failed")]
    AuthenticationFailed,
    #[error("cloud transport operation timed out")]
    Timeout,
    #[error("cloud transport local I/O error")]
    IoError(#[from] std::io::Error),
    #[error("rclone process failed with exit code {exit_code}")]
    RcloneProcessFailed { exit_code: i32, stderr_sanitised: String },
    #[error("cloud transport error: {0}")]
    Other(String),
}
```

**CS-003 — `CloudEndpoint` struct.** Exact text from `design.md`:
```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CloudEndpoint {
    pub provider: String,
    pub bucket: String,
    pub region: String,
    pub endpoint: String,
    pub path_prefix: String,
}
```
(`Eq` added per Assumption 5.)

**CS-004 — Mock signature.** `MockCloudTransport` in
`src-tauri/src/storage/cloud/mock.rs` with the following state:
```rust
pub struct MockCloudTransport {
    blobs: Arc<tokio::sync::Mutex<HashMap<String, Vec<u8>>>>,
    #[cfg(any(test, feature = "test-utils"))]
    failure_paths: Arc<tokio::sync::Mutex<HashMap<String, CloudTransportErrorKind>>>,
}
```
Where `CloudTransportErrorKind` is a plain-data mirror of `CloudTransportError` used
only for failure injection (construction path).

---

### Implementation steps (absolute paths)

**Step 1 — Replace the trait + error enum in `C:\Users\chris\source\repos\arx-runa\src-tauri\src\storage\cloud\mod.rs`.**
- Delete the two-method `CloudTransport` and 3-variant `CloudTransportError`.
- Emit CS-002 and CS-001 verbatim. Remove `#[non_exhaustive]`.
- Update the module doc-comment: replace "Forward declaration for Phase 4.1 / Phase 4.3"
  with a single-line summary that this module owns the canonical `CloudTransport`
  trait, its error enum, and the `CloudEndpoint` descriptor; Phase 4.2 adds
  `RcloneTransport`.
- Re-export `CloudEndpoint` from step 2.

**Step 2 — Create `C:\Users\chris\source\repos\arx-runa\src-tauri\src\storage\cloud\endpoint.rs`.**
- Contain only CS-003 plus a doc-comment that points at the design's
  "Connection Descriptor" section.
- Export nothing else from this file.

**Step 3 — Rewrite `C:\Users\chris\source\repos\arx-runa\src-tauri\src\storage\cloud\mock.rs` to implement CS-001.**
- Keep the struct name `MockCloudTransport` (Assumption, #5).
- `upload_blob(local_path, remote_path)`: `tokio::fs::read(local_path).await?`,
  insert bytes under `remote_path` key. Map any `std::io::Error` via `?`
  to `CloudTransportError::IoError`.
- `download_blob(remote_path, local_path)`: `blobs.get(remote_path).cloned()`,
  return `CloudTransportError::NotFound` on miss; write to `local_path` via
  `tokio::fs::write` and propagate `IoError` on failure.
- `delete_blob(remote_path)`: `blobs.remove(remote_path)`; return `Ok(())`
  even when absent (idempotent per design).
- `list_blobs(remote_prefix)`: return every key that `starts_with(remote_prefix)`
  and is strictly longer than the prefix, sorted lexicographically for stable tests.
- Add `#[cfg(any(test, feature = "test-utils"))] fn inject_failure(&self, path: &str, kind: CloudTransportErrorKind)`
  plus a private helper `fn check_failure(path: &str) -> Result<(), CloudTransportError>`
  invoked at the start of each trait method. This satisfies the
  `rust.md` "Every thiserror variant must have a test that triggers it" rule
  (see concern #3 resolution).
- Drop helper methods `len()` / `is_empty()` if no longer needed by callers, or
  keep them under `#[cfg(any(test, feature = "test-utils"))]` — confirm by
  grepping callers during implementation.

**Step 4 — Update `C:\Users\chris\source\repos\arx-runa\src-tauri\src\storage\mod.rs` re-exports.**
- Add `pub use cloud::{CloudEndpoint, CloudTransport, CloudTransportError};` if
  needed by downstream modules (do not re-export test-only mock).

**Step 5 — Migrate ceremony call sites to the path-based API (Assumption 1, #2, #3).** Each of the following files must switch from `upload_blob(&BlobName, &[u8])` / `download_blob(&BlobName) -> Vec<u8>` to CS-001:

- `C:\Users\chris\source\repos\arx-runa\src-tauri\src\auth\ceremonies\create.rs`
  (line 172): already writes `pending-vault-header.json` via
  `staging::write_owner_only(&staging_path, &json_bytes)`. Change the upload call
  to `cloud_transport.upload_blob(&staging_path, VAULT_HEADER_BLOB_NAME).await`.
  The staging cleanup (`remove_if_exists(&staging_path)`) stays on the success
  path and is skipped on failure to preserve Phase 4.3 retry semantics.
- `C:\Users\chris\source\repos\arx-runa\src-tauri\src\auth\ceremonies\create.rs`
  (lines 382, 405 — test ceremony callers via mock): construct a temp file in the
  test temp dir, pass `&temp_path` to `download_blob`, read the file bytes back.
- `C:\Users\chris\source\repos\arx-runa\src-tauri\src\auth\ceremonies\change_password.rs`
  (line 232): same migration as `create.rs` upload.
- `C:\Users\chris\source\repos\arx-runa\src-tauri\src\auth\ceremonies\rotate_key_file.rs`
  (line 244): same migration as `create.rs` upload.
- `C:\Users\chris\source\repos\arx-runa\src-tauri\src\auth\ceremonies\setup_recovery.rs`
  (line 114 upload, line 232 test download): upload via staging path already
  available; test downloads mirror `create.rs` pattern.
- `C:\Users\chris\source\repos\arx-runa\src-tauri\src\auth\ceremonies\recover_with_phrase.rs`
  (lines 28, 91, 156, 287, 296, 365, 372, 487, 507): this file has the most call
  sites. Each `download_blob(&vault_header_blob_name())` becomes a download to a
  ceremony-local tempfile, followed by `tokio::fs::read` and
  `staging::remove_if_exists`. Each `upload_blob(&name, &bytes)` becomes a write
  to staging followed by `upload_blob(&staging_path, NAME)`.
- `C:\Users\chris\source\repos\arx-runa\src-tauri\src\auth\ceremonies\recover_vault.rs`
  (lines 27, 81, 147, 251, 260): same migration patterns.
- `C:\Users\chris\source\repos\arx-runa\src-tauri\src\auth\ceremonies\test_support.rs`
  (lines 70, 114, 168): in-test ceremonies — mirror real ceremony migration;
  add a small `tempdir` for download/upload paths.
- `C:\Users\chris\source\repos\arx-runa\src-tauri\src\auth\ceremonies\mod.rs`
  (lines 36–43): the helpers `vault_header_blob_name() -> BlobName` and
  `manifest_backup_blob_name() -> BlobName` are no longer used by CS-001 —
  delete them and replace call-site usages with the existing string constants
  `VAULT_HEADER_BLOB_NAME` and `MANIFEST_BACKUP_BLOB_NAME`. This simultaneously
  resolves concern #4 about the `BlobName`-vs-`&str` distinction.

**Step 6 — Mock test suite** in `C:\Users\chris\source\repos\arx-runa\src-tauri\src\storage\cloud\mock.rs` (under `#[cfg(test)] mod tests`). Replace the current 4 tests with the following 10, one per sub-phase acceptance bullet + one per `CloudTransportError` variant (per `rust.md` testing rule):

- `test_mock_upload_download_round_trip_preserves_bytes` — canonical success path.
- `test_mock_download_missing_path_returns_not_found` — covers `NotFound`.
- `test_mock_upload_overwrites_existing_blob_idempotently` — covers design idempotency claim.
- `test_mock_delete_removes_blob` — delete + subsequent download returns `NotFound`.
- `test_mock_delete_nonexistent_path_is_idempotent` — no error.
- `test_mock_list_blobs_filters_by_prefix` — populate `"vault/a.blob"`, `"vault/b.blob"`, `"manifest/x.blob"`; `list_blobs("vault/")` returns exactly `["vault/a.blob", "vault/b.blob"]`.
- `test_mock_list_blobs_empty_prefix_returns_all_paths` — sanity check.
- `test_mock_inject_authentication_failure_variant` — injection triggers `AuthenticationFailed` on the targeted path.
- `test_mock_inject_timeout_variant_on_upload` — triggers `Timeout`.
- `test_mock_inject_rclone_process_failed_variant_carries_exit_code_and_stderr` — triggers `RcloneProcessFailed { exit_code, stderr_sanitised }` and asserts the fields round-trip.
- `test_cloud_endpoint_serde_round_trip_preserves_all_fields` — covers deliverable 3 JSON serde.
- `test_cloud_endpoint_equality_differs_when_path_prefix_differs` — covers `Eq`.

`IoError(#[from] std::io::Error)` is covered indirectly by the round-trip tests
that pass bad paths (e.g., `"/does/not/exist/xyz"` for upload) — add one explicit
`test_mock_upload_bubbles_io_error_when_local_path_unreadable` using a
non-existent source file.

**Step 7 — Update rule stubs** per §8 governance actions.

**Step 8 — Verify the invariant test in `ceremonies/mod.rs:64` still passes** (`test_master_key_token_absent_from_session_and_header_type_names`) — no type renames affect it.

**Step 9 — Build + lint gate:**
```bash
cargo fmt
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-targets --all-features
```

## 6. Review focus areas

Guidance for `/implement-plan`; actual agent selection is driven by changed files.

### 6a. Rust change surface (anticipated)
- `C:\Users\chris\source\repos\arx-runa\src-tauri\src\storage\cloud\mod.rs` (rewrite)
- `C:\Users\chris\source\repos\arx-runa\src-tauri\src\storage\cloud\endpoint.rs` (new)
- `C:\Users\chris\source\repos\arx-runa\src-tauri\src\storage\cloud\mock.rs` (rewrite)
- `C:\Users\chris\source\repos\arx-runa\src-tauri\src\storage\mod.rs` (re-export update)
- `C:\Users\chris\source\repos\arx-runa\src-tauri\src\auth\ceremonies\mod.rs` (remove `BlobName` helpers)
- `C:\Users\chris\source\repos\arx-runa\src-tauri\src\auth\ceremonies\create.rs` (3 call sites)
- `C:\Users\chris\source\repos\arx-runa\src-tauri\src\auth\ceremonies\change_password.rs` (2+ call sites)
- `C:\Users\chris\source\repos\arx-runa\src-tauri\src\auth\ceremonies\rotate_key_file.rs` (2+ call sites)
- `C:\Users\chris\source\repos\arx-runa\src-tauri\src\auth\ceremonies\setup_recovery.rs` (2 call sites)
- `C:\Users\chris\source\repos\arx-runa\src-tauri\src\auth\ceremonies\recover_vault.rs` (5 call sites)
- `C:\Users\chris\source\repos\arx-runa\src-tauri\src\auth\ceremonies\recover_with_phrase.rs` (9 call sites)
- `C:\Users\chris\source\repos\arx-runa\src-tauri\src\auth\ceremonies\test_support.rs` (3 call sites)

### 6b. Security-sensitive paths (anticipated)
All files under `src-tauri/src/storage/cloud/` and `src-tauri/src/auth/ceremonies/`.
Specific concerns for this run:

- **Plaintext residue on disk.** The migration to path-based upload/download means
  ceremony code writes intermediate JSON to the staging directory (already the
  case for `pending-vault-header.json`) and, for downloads, writes fetched bytes
  to a staging tempfile. Verify that every new download tempfile is created with
  owner-only permissions via `staging::write_owner_only` or the equivalent helper
  *before* bytes land, and is passed through `staging::remove_if_exists` on both
  success and error branches. No `/tmp` fallback.
- **No plaintext manifest on disk at any point.** `recover_with_phrase` and
  `recover_vault` download the encrypted manifest backup to a tempfile (ciphertext
  only). Verify the tempfile is deleted on every exit branch; the decrypted
  buffer stays in `Zeroizing<Vec<u8>>` per existing `manifest_backup.rs` contract.
- **No new sensitive field in `CloudEndpoint`.** CS-003 contains no credentials —
  credentials are Phase 4.2 territory, stored encrypted in SQLCipher.
- **No logging of `remote_path` or blob contents.** Doc comment on `CloudTransport`
  says implementations MUST NOT log blob content; debug-level path logging is
  permitted but must not reach user-facing errors.

### 6c. Architecture risk areas
- **SRP in `storage::cloud::mod.rs`:** after this change the module file contains
  trait + error enum + re-exports only (no impls); `endpoint.rs` owns
  `CloudEndpoint`; `mock.rs` owns only the mock. Verify no helpers leak across.
- **Dependency direction:** `auth::ceremonies` depends on `storage::cloud` (existing);
  `storage::cloud` must not pull in `auth`. The removal of
  `vault_header_blob_name() -> BlobName` helpers tightens this boundary.
- **Visibility discipline:** `MockCloudTransport` must remain feature-gated
  (`#[cfg(any(test, feature = "test-utils"))]`). `CloudTransportErrorKind`
  (injection helper) must be test-only.
- **Abstraction debt:** the `#[non_exhaustive]` attribute is removed because the
  Contract Surface enumerates the full variant set. Confirm no downstream
  `match` statements rely on catch-all arms that would now warn unused.

### 6d. Testing requirements
- **Validation checkpoint from sub-roadmap:**
  `cargo test storage::cloud::mock_transport` must pass with 100% coverage on
  `MockCloudTransport`. (The module path in our tree is
  `storage::cloud::mock::tests` — adjust the command accordingly.)
- **Edge cases from Step 2 adversarial review:**
  - Every `CloudTransportError` variant triggered in a dedicated test
    (`rust.md` testing rule).
  - `CloudEndpoint` serde round-trip preserves every field including empty
    `region` and empty `endpoint` strings (design.md lines 194–199).
  - `list_blobs` returns paths in stable order across runs (tests rely on
    sorted output).
  - Idempotent upload: second upload of same `remote_path` with different
    bytes results in the later bytes being returned on download.
  - Idempotent delete: delete on absent path is a no-op.
- **Regression:** full `cargo test --workspace --all-targets --all-features`
  must stay green, especially `auth::ceremonies::*` tests that were rewired in
  Step 5.

## 7. Documentation impact

| Item | Path | Required this run? | Rationale |
|------|------|-------------------:|-----------|
| Update `storage/cloud/mod.rs` module-level doc-comment to drop "forward declaration" wording. | in-code doc-comment at the top of `mod.rs` | **Required** | Becomes misleading once this sub-phase lands. |
| `docs/architecture/designs/cloud-synchronisation/sub-phases/4.1-cloud-transport.md` | same file | **Deferred / optional** | No design changes required by the plan under Resolution (A); edit only if user picks Resolution (B) or (C) for Concern #1. |
| `docs/architecture/designs/cloud-synchronisation/design.md` | same file | **Deferred / optional** | Same — only edited under Resolution (B) or (C). |
| `docs/architecture/design-invariants.md` | same file | **Deferred** | No invariant added or changed by 4.1. |
| Diagrams under `docs/architecture/designs/cloud-synchronisation/diagrams/` | TBD | **Deferred / optional** | 4.1 introduces no new flow — existing diagrams remain accurate. |

## 8. Governance sync actions (pre-implementation)

| Action ID | Reason / linked concern | Target files | Required edit | Verification |
|-----------|-------------------------|--------------|---------------|--------------|
| GS-001 | Concern #4 (`BlobName` vs `&str`) — the `.claude/rules/storage.md` "Traits" section currently lists `CloudTransport` only in passing; after 4.1 the trait uses `&str` remote paths while `BlobName` remains for chunk blob names. | `C:\Users\chris\source\repos\arx-runa\.claude\rules\storage.md` under the "Traits" heading | Add a one-line note: "`CloudTransport` uses `&str` for cloud-root-relative paths (forward slashes only); `BlobName` is reserved for chunk blob filenames in the manifest and staging directory." | Grep for `CloudTransport` in `storage.md`; verify the note appears and does not duplicate existing guidance. |
| GS-002 | Concern #4 + broader Phase 4.1 trait shape — `.claude/rules/auth.md` says "Forward declarations: `CloudTransport` … originate in Phase 2.4 and are extended by Phase 4.1 / 4.3." After 4.1 lands, this wording is wrong: 4.1 *replaces* the forward declaration, not just extends it. | `C:\Users\chris\source\repos\arx-runa\.claude\rules\auth.md` under "Ceremonies" | Update the bullet to: "Forward declarations: `VaultHeader` (`src-tauri/src/storage/cloud/vault_header.rs`) originates in Phase 2.4 and is extended by Phase 4.3. `CloudTransport` (`src-tauri/src/storage/cloud/mod.rs`) is replaced by the canonical 4-method surface in Phase 4.1; ceremonies call it via staging-file semantics." | `grep -n CloudTransport .claude/rules/auth.md` returns the updated line. |
| GS-003 | After GS-001/GS-002, mirror the rule edits into GitHub Copilot instructions. | `.github/instructions/storage.instructions.md`, `.github/instructions/auth.instructions.md` | Run `/copilot-sync` after GS-001 and GS-002 land. | Post-sync diff is empty on a second `/copilot-sync` run. |

## 9. Handoff Notes for Implementer

Working directory: `C:\Users\chris\source\repos\arx-runa`. Execute steps in the
order listed in §5 — trait + error enum rewrite first (Step 1), then
`endpoint.rs` (Step 2), then the mock (Step 3), then ceremony migration (Step 5,
one file at a time so `cargo check` stays green between commits), then tests
(Step 6), then governance sync (Step 7 / §8), then the full verification gate
(Step 9). The only non-obvious trap is the scale of the ceremony migration:
nine files under `src-tauri/src/auth/ceremonies/` hold 22+ call sites — far more
than the sub-phase's "~150 prod + ~100 test lines" estimate implies. Keep the
existing `pending-vault-header.json` staging behaviour untouched; just redirect
the `upload_blob` call to use the staging path. For downloads in ceremonies,
introduce a new tempfile alongside `pending-vault-header.json` (different
filename suffix) and delete it on every exit branch — including error paths.

Platform note: staging paths resolve via `dirs::config_dir()` which returns
`%APPDATA%` on Windows, `~/.config` on Linux, `~/Library/Application Support` on
macOS. No platform-specific branches are introduced by this sub-phase. No new
feature flags; `test-utils` remains the only optional feature touched.

## Implementation Log

- **Date**: 2026-04-19T00:34:08.1170452+02:00
- **Run ID**: `phase-4-1-cloud-transport-20260418-234838`
- **Track**: `full`
- **Branch**: `development`
- **Execution mode**: delegated (`rust-implementer`) with orchestrator-managed remediation/review cycles

### Agent evidence

| Approach step | Agent | Agent ID | Outcome |
|---|---|---|---|
| Governance sync GS-001/GS-002 | orchestrator | N/A | Updated `.claude/rules/storage.md` and `.claude/rules/auth.md` |
| Governance sync GS-003 | copilot-sync skill + orchestrator | N/A | Synced instruction mirrors; second sync remained idempotent |
| Steps 1-6 implementation | rust-implementer | `rust-impl-4-1` | Completed |
| Remediation cycle fixes (CF-002/CF-003/CF-005) | rust-implementer | `rust-fix-cf002-3-5` | Completed |
| Remediation cycle fix (CF-008) | rust-implementer | `rust-fix-cf008` | Completed |
| Test expansion audit | test-writer | `test-writer-4-1` | Added focused mock transport tests |

### Files changed

- `.claude/plans/phase-4-1-cloud-transport.md`
- `.claude/rules/auth.md`
- `.claude/rules/storage.md`
- `.github/instructions/auth.instructions.md`
- `.github/instructions/crypto.instructions.md`
- `.github/instructions/leptos.instructions.md`
- `.github/instructions/memory-protection.instructions.md`
- `.github/instructions/mermaid.instructions.md`
- `.github/instructions/research.instructions.md`
- `.github/instructions/rust.instructions.md`
- `.github/instructions/storage.instructions.md`
- `.github/instructions/tauri.instructions.md`
- `docs/architecture/designs/cloud-synchronisation/sub-phases/4.1-cloud-transport.md`
- `src-tauri/src/storage/cloud/mod.rs`
- `src-tauri/src/storage/cloud/endpoint.rs`
- `src-tauri/src/storage/cloud/mock.rs`
- `src-tauri/src/storage/mod.rs`
- `src-tauri/src/auth/ceremonies/mod.rs`
- `src-tauri/src/auth/ceremonies/create.rs`
- `src-tauri/src/auth/ceremonies/change_password.rs`
- `src-tauri/src/auth/ceremonies/rotate_key_file.rs`
- `src-tauri/src/auth/ceremonies/setup_recovery.rs`
- `src-tauri/src/auth/ceremonies/recover_vault.rs`
- `src-tauri/src/auth/ceremonies/recover_with_phrase.rs`
- `src-tauri/src/auth/ceremonies/test_support.rs`

### Verification gates

- **Formatting check** (`cargo fmt --all -- --check`): clean after formatting pass.
- **Clippy** (`cargo clippy --workspace --all-targets --all-features -- -D warnings`): clean.
- **Tests** (`cargo test --workspace --all-targets --all-features`): passed (`388 passed; 0 failed; 1 ignored` in main tauri lib suite; workspace test command exited success).
- **Release build** (`cargo build --workspace --release`): success.

### Review outcomes

- **Rust review**: actionable findings resolved for in-scope items (CF-002, CF-003, CF-005, CF-008); later cycle no actionable findings in shard reviews.
- **Architecture review**: no structural findings in auth shard final cycle; storage-shard residual findings classified as deferred-by-plan/intentional decision per phase boundary.
- **Security review**: no CRITICAL findings; residual session-gate race concern classified deferred-by-plan (out-of-scope for 4.1).
- **Cross-shard review**: 2 invocations; no cross-shard contradictions found.

### Findings quality gate

- **ACTIONABLE_NOW**: 4 (`CF-002`, `CF-003`, `CF-005`, `CF-008`) — implemented
- **INTENTIONAL_DECISION**: 1 (`CF-014`)
- **DEFERRED_BY_PLAN**: 9 (`CF-001`, `CF-004`, `CF-006`, `CF-007`, `CF-009`, `CF-010`, `CF-011`, `CF-012`, `CF-013`)
- **INSUFFICIENT_EVIDENCE**: 0

### Finding overrides

- None.

### Design challenge outcomes

- **Rejected**: validated cloud path abstraction in 4.1 (deferred to 4.2 per assumption #7).
- **Rejected**: pending-header concurrency serialization hardening in 4.1 (deferred to 4.3).
- **Rejected**: ceremony SQLCipher rewrap/rekey dedup refactor in 4.1 (out of deliverable scope).
- No design-document updates were required from accepted challenges.

### Governance sync

- **Actions executed**: 3 (`GS-001`, `GS-002`, `GS-003`)
- **Files updated**: `.claude/rules/auth.md`, `.claude/rules/storage.md`, `.github/instructions/*.instructions.md` (auth, crypto, leptos, memory-protection, mermaid, research, rust, storage, tauri)
- **`/copilot-sync` outcome**: OK

### Sub-phase decisions sync

- **Doc path**: `docs/architecture/designs/cloud-synchronisation/sub-phases/4.1-cloud-transport.md`
- **Decisions added**: 4 bullets under `## Implementation Decisions`

### Deviations from plan

- Additional remediation cycles addressed review findings directly coupled to touched ceremony flows (cleanup/guard ordering) before closure.
- Security-scoped and architecture-wide concerns outside approved 4.1 boundaries were recorded and deferred by plan classification rather than broadened into cross-phase refactors.

### Documentation flagged

- `Update storage/cloud/mod.rs module-level doc-comment to drop "forward declaration" wording.` — **Required** — applied.
- `docs/architecture/designs/cloud-synchronisation/sub-phases/4.1-cloud-transport.md` — **Deferred / optional** in plan, but updated in this run for mandatory sub-phase implementation-decision sync.
- `docs/architecture/designs/cloud-synchronisation/design.md` — **Deferred / optional** — unchanged (no Resolution B/C selection).
- `docs/architecture/design-invariants.md` — **Deferred** — unchanged.
- `Diagrams under docs/architecture/designs/cloud-synchronisation/diagrams/` — **Deferred / optional** — unchanged.

### Run state path

- `.claude/runs/phase-4-1-cloud-transport-20260418-234838/`
