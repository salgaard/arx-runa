---
title: "Phase 4.3 — Vault Header Upload and Download"
created: "2026-04-19T12:00:00Z"
status: implemented
roadmap-phase: 4
sub-phase: "4.3"
design-document: "docs/architecture/designs/cloud-synchronisation/design.md"
sub-phase-roadmap: "docs/architecture/designs/cloud-synchronisation/sub-phases/roadmap.md"
governance-sync-required: true
tags: [storage, cloud, vault-header, serialization, phase-4]
---

# Plan: Phase 4.3 — Vault Header Upload and Download

## 1. Goal

Land a dedicated `upload_vault_header` / `download_vault_header` pair at the cloud-transport boundary that owns the plaintext-JSON staging, upload, download, and post-download validation for `<cloud_root>/vault-header.json`, and route existing Phase 2.4 ceremonies through the new helpers so the sub-phase's bootstrap-retry plumbing is funnelled through a single cloud-sync function.

## 2. Context

**Sub-phase position.** 4.3 is the third unit of the cloud-sync roadmap (4.1 → 4.2 → **4.3** → 4.4 → 4.5). Dependencies met: Phase 4.1 (`CloudTransport` trait + `MockCloudTransport`), Phase 4.2 (`RcloneTransport`). Phase 4.4 (manifest backup) and 4.5 (push/pull) depend on this sub-phase.

**Canonical design sections** (`docs/architecture/designs/cloud-synchronisation/design.md`):
- `#vault-header` (lines 610–737) — struct shape, tier wire-format examples.
- `#upload-flow` (lines 739–746) — 4-step upload path.
- `#download-and-parse-flow` (lines 748–776) — 6-step download + validation sequence.
- `#security-analysis-of-plaintext-fields` (lines 778–790).
- `#local-vault-parameter-cache` (lines 293–308) — `local-vault-params.json` policy (structure + existing-device anchor check).

**Existing code state as of 2026-04-19.**
- `src-tauri/src/storage/cloud/vault_header.rs` already defines `VaultHeader`, `Argon2ParamsJson`, `RecoverySlot`, `TrustedVaultHeaderAnchor`, `VaultHeaderTrustPolicy`, `VaultHeaderError`, `validate()`, `validate_structure()`, `validate_trust_policy(policy)`. Module-level comment (`vault_header.rs:1-8`) states: "Phase 4.3 will adopt this struct as-is, add richer validation, and wire the startup retry path for `pending-vault-header.json`."
- `src-tauri/src/storage/cloud/mod.rs` re-exports `pub mod vault_header` and the `CloudTransport` trait + `CloudTransportError`.
- `src-tauri/src/auth/ceremonies/create.rs:157-205`, `change_password.rs`, `rotate_key_file.rs`, `setup_recovery.rs` currently inline `serde_json::to_vec_pretty → staging::write_owner_only(STAGING_FILE_NAME) → cloud_transport.upload_blob(staging_path, VAULT_HEADER_BLOB_NAME) → staging::remove_if_exists` and bail on any error.
- `src-tauri/src/auth/ceremonies/recover_with_phrase.rs:28-62` inlines the download side: `cloud_transport.download_blob("vault-header.json", temp_path) → tokio::fs::read → serde_json::from_slice → validate_structure → validate_trust_policy(Bootstrap)`.
- `src-tauri/src/auth/ceremonies/mod.rs:27-31` owns the constants `VAULT_HEADER_BLOB_NAME = "vault-header.json"`, `STAGING_FILE_NAME = "pending-vault-header.json"`.
- `src-tauri/src/auth/staging.rs` provides `staging_directory()` under `dirs::config_dir()/arx-runa/`, `write_owner_only`, `write_owner_only_new`, `remove_if_exists`. The module-level comment (`auth/staging.rs:1-16`) says Phase 4.3 owns both the startup retry path and the Windows DACL tightening.
- `src-tauri/src/storage/cloud/remote_path.rs` already validates paths against `^[a-zA-Z0-9._/-]+$`; `"vault-header.json"` passes.
- No `local-vault-params.json` reader/writer exists anywhere in the tree (`Grep "local-vault-params"` returns zero matches).
- `.claude/rules/storage.md` Cloud-backup section already declares: "vault header stays plaintext JSON at cloud root" and "Push flow uploads manifest backup, then uploads vault header idempotently on every push."
- `.claude/rules/auth.md` Ceremonies section pins Phase 4.3 forward declarations: `VaultHeader` originates in Phase 2.4 and is extended by Phase 4.3; startup retry loop is Phase 4.3 territory.

**No pending architectural decisions** apply to this sub-phase; Contract Surface is canonical.

**Security review.** Sub-phase declares "Not required (plaintext by design)". Verified: deliverables touch `src-tauri/src/storage/cloud/` only for plaintext JSON bootstrap metadata. No key material, no crypto primitives, no IPC surface. `TrustedVaultHeaderAnchor` already exists; no new trust boundary. Agree with the sub-phase — security review is not required this run.

## 3. Design Concerns / Open Questions

| Concern | Source | Impact | Classification | Resolution | Documentation updates |
|---|---|---|---|---|---|
| **C-1 Local-vault-params.json cache persistence is referenced by design.md's download-parse flow (step 3d/step 5) but is not listed among the sub-phase deliverables.** | `4.3-vault-header.md:12-18` (deliverables 1–7) vs. `design.md:293-308, 757-775` and `design-invariants.md#9` | Without cache read/write, `VaultHeaderTrustPolicy::ExistingDevice` can be constructed at call sites but never populated from disk. Existing-device downgrade resistance remains theoretical; callers default to `Bootstrap` forever. | Non-blocking | Keep `download_vault_header` agnostic: accept an optional `&TrustedVaultHeaderAnchor` and delegate policy choice to the caller. Do **not** persist or read `local-vault-params.json` in this run; defer the cache read/write module to a follow-up sub-phase task that the push/pull flow (4.5) will consume. | None this run; note deferral in Section 7. |
| **C-2 Sub-phase deliverables do not list refactoring existing ceremonies to call the new `upload_vault_header`/`download_vault_header`.** | `4.3-vault-header.md:14-16` (deliverables 4–5) vs. `auth/ceremonies/create.rs:157-205`, `recover_with_phrase.rs:28-62` | If helpers are added but ceremonies keep inlining upload/download logic, the 4.3 API has no in-tree caller, duplication lingers, and Phase 4.5 will introduce a third upload path. | Non-blocking | In-scope refactor: migrate `create_vault`, `change_password`, `rotate_key_file`, `setup_recovery` to call `upload_vault_header`; migrate `recover_with_phrase` to call `download_vault_header`. Behaviour preserved exactly (same staging filename, same error mapping to `AuthenticationError::VaultHeaderInvalid`, same cleanup semantics). | None. |
| **C-3 `vault_header.rs:1-7` module comment and `.claude/rules/auth.md` ("startup retry loop is Phase 4.3 territory") direct 4.3 to wire the `pending-vault-header.json` startup retry path, which is not in the sub-phase deliverable list.** | `vault_header.rs:1-8`, `auth/staging.rs:1-16`, `.claude/rules/auth.md#ceremonies`, vs. `4.3-vault-header.md:10-18` | Crash between ceremony stage-write and cloud upload leaves an orphan `pending-vault-header.json`. Without retry, user must re-run ceremony or manually cleanup. | Non-blocking | Defer the startup retry to a follow-up task tracked under Phase 4.5 (push flow) where header publication is already idempotent per push. Sub-phase deliverables bound this run; document the deferral and update the `auth/staging.rs` module comment accordingly to point to the follow-up task. | Update `auth/staging.rs:1-16` module comment to reflect the defer-to-4.5 decision. Flag in Section 7 as required this run. |
| **C-4 Sub-phase uses `Argon2Params` as the struct name; code already ships `Argon2ParamsJson` (to disambiguate from the runtime `argon2::Params`).** | `4.3-vault-header.md:12` vs. `storage/cloud/vault_header.rs:18-26` | Cosmetic. Risk of spec drift in docs. | Non-blocking | Keep the existing `Argon2ParamsJson` name; do not rename. The disambiguation against `argon2::Params` is the whole point of the `Json` suffix. | None this run; the sub-phase's wording is descriptive, not prescriptive. |
| **C-5 Sub-phase deliverable 6 lists Argon2 "minimums" but does not restate the `memory_cost >= 19456, time_cost >= 2, parallelism >= 1` bootstrap floor or the warn-below-Arx-defaults hook.** | `4.3-vault-header.md:17` vs. `design.md:762-765` | Implementer could forget the "warn if below Arx defaults" UX hook. | Non-blocking | Assumption (Section 4 A-3): the bootstrap warning is a log-only `tracing::warn!` in `download_vault_header` when params are below `65536/3/4`; no UI surface yet (Phase 6 UX concern). Floors are already enforced in `validate_argon2_params` (`vault_header.rs:237-245`). | None this run. |
| **C-6 `download_vault_header` must co-exist with the already-inlined download path in `recover_with_phrase` that treats download failure as `AuthenticationError::VaultHeaderInvalid`.** | `recover_with_phrase.rs:35-58` | Error mapping at the boundary: new helper returns its own error type; callers must translate. | Non-blocking | Return `VaultHeaderSyncError` (thiserror) from helpers; `recover_with_phrase` maps all variants to `AuthenticationError::VaultHeaderInvalid` exactly as today. No new `AuthenticationError` variants required. | None. |
| **C-7 `download_vault_header` writes its temp file under the caller-provided staging directory; design.md says "Arx Runa staging directory (from Phase 3)".** | `4.3-vault-header.md:53`, `design.md:742-746` | The implementation note mentions "Arx Runa staging directory (from Phase 3), not system temp." Auth ceremonies currently use `dirs::config_dir()/arx-runa/` for `pending-vault-header.json`, not the Phase 3 `staging::default_staging_directory` (`dirs::data_dir().join("arx-runa").join("staging")`). | Non-blocking | Helpers take `staging_dir: &Path` as a parameter; callers pass the directory they already use. Ceremonies keep `dirs::config_dir()/arx-runa/` (the header is public JSON, not a chunk blob, and Phase 2.4 ceremonies already write there). Phase 4.5 push/pull callers may choose `storage::staging::default_staging_directory()` for consistency with blob staging; this sub-phase does not pre-empt that decision. Assumption A-2 pins this. | None. |

## 4. Assumptions

- **A-1 Staging filename for header downloads.** `download_vault_header` writes to `<staging_dir>/pending-vault-header-download.json` (distinct from the upload-side `pending-vault-header.json` to avoid colliding with a concurrent ceremony staging write) and deletes it on every exit (success or error). The filename is an internal detail; no on-disk contract is introduced.
- **A-2 Header staging directory.** The helper accepts `staging_dir: &Path` chosen by the caller. Ceremony callers pass `crate::auth::staging::staging_directory().await?` (i.e., `dirs::config_dir()/arx-runa/`). No change to the existing `STAGING_FILE_NAME = "pending-vault-header.json"` constant for the upload side.
- **A-3 Below-default warn logs at debug-plus level.** When `download_vault_header` observes primary or recovery-slot Argon2 params below the Arx defaults (`memory_cost < 65536 || time_cost < 3 || parallelism < 4`) during bootstrap mode, it emits `tracing::warn!` with `schema_version`, observed params, and the defaults — no blob contents, no key material. No error is returned.
- **A-4 Schema version supported set.** "Supported" = equal to `VaultHeader::SCHEMA_VERSION` (currently `1`). `validate_structure` already enforces this; no change needed.
- **A-5 Error type naming.** New `VaultHeaderSyncError` enum lives in `vault_header_io.rs` and re-exports from `storage::cloud`. Variants map 1:1 to the failure modes in Section 5 (`CS-007`).
- **A-6 Test layout.** Unit tests for the new helpers live alongside the new module (`#[cfg(test)] mod tests`) and use `MockCloudTransport` + `tempfile::tempdir()`; no integration tests against `RcloneTransport` in this sub-phase (Phase 4.5 owns the real-cloud round-trip).
- **A-7 Ceremony refactor keeps existing error mapping.** Every call site that today maps `upload/download_blob` errors to `AuthenticationError::VaultHeaderInvalid` continues to do so via `.map_err(|_| AuthenticationError::VaultHeaderInvalid)` on `VaultHeaderSyncError`.
- **A-8 No new IPC surface.** Phase 4.3 does not add any Tauri command; all callers are internal Rust flows.
- **A-9 Public API surface.** `upload_vault_header` and `download_vault_header` are `pub fn` at `storage::cloud::vault_header_io`; they are re-exported by `storage::cloud::mod.rs` as `pub use vault_header_io::{upload_vault_header, download_vault_header, VaultHeaderSyncError};`.

## 5. Approach

### CONTRACT_SNIPPETS

**CS-001** — Already-in-tree `CloudTransport` trait (`src-tauri/src/storage/cloud/mod.rs:51-73`, unchanged this run):

```rust
#[async_trait]
pub trait CloudTransport: Send + Sync {
    async fn upload_blob(&self, local_path: &Path, remote_path: &str)
        -> Result<(), CloudTransportError>;
    async fn download_blob(&self, remote_path: &str, local_path: &Path)
        -> Result<(), CloudTransportError>;
    async fn delete_blob(&self, remote_path: &str) -> Result<(), CloudTransportError>;
    async fn list_blobs(&self, remote_prefix: &str) -> Result<Vec<String>, CloudTransportError>;
}
```

**CS-002** — Already-in-tree `VaultHeader`, `Argon2ParamsJson`, `RecoverySlot` (`src-tauri/src/storage/cloud/vault_header.rs:17-59`, unchanged this run):

```rust
pub struct Argon2ParamsJson { pub memory_cost: u32, pub time_cost: u32, pub parallelism: u32 }
pub struct RecoverySlot {
    pub method: String,
    pub argon2_salt: String,
    pub argon2_params: Argon2ParamsJson,
    pub wrapped_master_key: String,
}
pub struct VaultHeader {
    pub vault_id: String,
    pub schema_version: u32,
    pub tier: u8,
    pub argon2_salt: String,
    pub argon2_params: Argon2ParamsJson,
    pub key_file_blake3: Option<String>,
    #[serde(default)] pub recovery_slots: Vec<RecoverySlot>,
}
```

**CS-003** — Already-in-tree `VaultHeaderTrustPolicy` and `TrustedVaultHeaderAnchor` (`vault_header.rs:61-82`):

```rust
pub struct TrustedVaultHeaderAnchor {
    pub vault_id: String,
    pub argon2_salt: String,
    pub argon2_params: Argon2ParamsJson,
}
pub enum VaultHeaderTrustPolicy<'a> {
    Bootstrap,
    ExistingDevice { trusted_anchor: &'a TrustedVaultHeaderAnchor },
}
```

**CS-004** — Already-in-tree `VaultHeaderError` (`vault_header.rs:172-226`, unchanged). No new variants.

**CS-005** — New `upload_vault_header` signature (added this run, `src-tauri/src/storage/cloud/vault_header_io.rs`):

```rust
/// Serialises `header` to pretty JSON, stages it with owner-only permissions,
/// uploads it to `<cloud_root>/vault-header.json`, and removes the staging file.
///
/// Temp file lives at `staging_dir.join("pending-vault-header.json")`. Overwrites
/// any prior staging file via `write_owner_only` semantics. On any error, the
/// staging file is best-effort removed before returning.
pub async fn upload_vault_header(
    header: &VaultHeader,
    cloud_transport: &dyn CloudTransport,
    staging_dir: &Path,
) -> Result<(), VaultHeaderSyncError>;
```

**CS-006** — New `download_vault_header` signature:

```rust
/// Downloads `<cloud_root>/vault-header.json`, deserialises, and validates
/// under `policy`. Writes a temp file at
/// `staging_dir.join("pending-vault-header-download.json")`, then removes it
/// on every exit path (success or error).
///
/// Under `VaultHeaderTrustPolicy::Bootstrap`, emits a `tracing::warn!` when
/// Argon2 params are below Arx defaults (65536/3/4). Under
/// `VaultHeaderTrustPolicy::ExistingDevice`, the header must match the
/// anchor's `vault_id`, `argon2_salt`, and `argon2_params` exactly.
pub async fn download_vault_header(
    cloud_transport: &dyn CloudTransport,
    staging_dir: &Path,
    policy: VaultHeaderTrustPolicy<'_>,
) -> Result<VaultHeader, VaultHeaderSyncError>;
```

**CS-007** — New error enum `VaultHeaderSyncError` (added this run):

```rust
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum VaultHeaderSyncError {
    #[error("vault header serialisation failed")]
    SerialiseFailed,
    #[error("vault header staging I/O failed: {0}")]
    StagingIo(String),
    #[error("vault header cloud transport failed: {0}")]
    Transport(#[from] CloudTransportError),
    #[error("vault header JSON decode failed")]
    DeserialiseFailed,
    #[error("vault header validation failed: {0}")]
    Validation(#[from] VaultHeaderError),
}
```

**CS-008** — Cloud blob path constant (unchanged, sourced from `auth/ceremonies/mod.rs:27` today; may move to `vault_header_io.rs` as part of the refactor):

```rust
pub const VAULT_HEADER_BLOB_NAME: &str = "vault-header.json";
```

### Step-by-step implementation

**Step 1 — Create `src-tauri/src/storage/cloud/vault_header_io.rs`** (maps deliverables 4, 5 and partially 6).

Implement `upload_vault_header` per CS-005:
1. `serde_json::to_vec_pretty(header)` → `SerialiseFailed` on error.
2. `let staging_path = staging_dir.join("pending-vault-header.json");`
3. Write owner-only: use `tokio::fs::write` with a `cfg!(unix)` mode-0o600 post-set via `std::os::unix::fs::PermissionsExt`; on Windows use `tokio::fs::write` (DACL tightening is C-3 follow-up). Wrap I/O errors in `StagingIo(error.to_string())`.
4. `cloud_transport.upload_blob(&staging_path, VAULT_HEADER_BLOB_NAME).await?;` — `Transport` variant auto-converts.
5. `let _ = tokio::fs::remove_file(&staging_path).await;` — best-effort, warn-log non-`NotFound` errors via `tracing::warn!`.
6. On any error after staging write, best-effort remove staging file, then return the error.

Implement `download_vault_header` per CS-006:
1. `let temp_path = staging_dir.join("pending-vault-header-download.json");`
2. `cloud_transport.download_blob(VAULT_HEADER_BLOB_NAME, &temp_path).await?;` — `Transport` variant bubbles (includes `NotFound`).
3. `let bytes = tokio::fs::read(&temp_path).await.map_err(|error| StagingIo(error.to_string()))?;`
4. `let _ = tokio::fs::remove_file(&temp_path).await;` — best-effort regardless of downstream result.
5. `let header: VaultHeader = serde_json::from_slice(&bytes).map_err(|_| DeserialiseFailed)?;`
6. `header.validate_trust_policy(policy)?;` — returns `Validation(VaultHeaderError)` via `#[from]`.
7. If `policy == Bootstrap` and (primary or any recovery-slot) Argon2 params are below `memory_cost=65536 || time_cost=3 || parallelism=4`, emit `tracing::warn!(schema_version=header.schema_version, primary_memory_cost=header.argon2_params.memory_cost, ...)`. No error.
8. Return `Ok(header)`.

Emit `VaultHeaderSyncError` per CS-007 in the same file; `#[from]` impls cover `CloudTransportError` and `VaultHeaderError`.

Move or mirror the constant `VAULT_HEADER_BLOB_NAME` into `vault_header_io.rs` as the canonical source; keep the existing `auth/ceremonies/mod.rs` constant or have it re-export (Step 3 decides).

**Step 2 — Register the new module and exports** (`src-tauri/src/storage/cloud/mod.rs`).

Add:

```rust
pub mod vault_header_io;
pub use vault_header_io::{
    VaultHeaderSyncError, VAULT_HEADER_BLOB_NAME, download_vault_header, upload_vault_header,
};
```

(Grouped with the existing `pub mod vault_header;` at `mod.rs:17`.)

**Step 3 — Refactor existing ceremonies to use the helpers** (maps C-2).

For each of the ceremony files that currently upload the header inline — `src-tauri/src/auth/ceremonies/create.rs`, `change_password.rs`, `rotate_key_file.rs`, `setup_recovery.rs` — replace the sequence:

```
serde_json::to_vec_pretty(&header)
  → staging::write_owner_only(STAGING_FILE_NAME path, &json_bytes)
  → cloud_transport.upload_blob(staging_path, VAULT_HEADER_BLOB_NAME)
  → staging::remove_if_exists(staging_path)
```

with:

```rust
let staging_dir = staging::staging_directory().await?;
crate::storage::cloud::upload_vault_header(&header, cloud_transport, &staging_dir)
    .await
    .map_err(|_| AuthenticationError::VaultHeaderInvalid)?;
```

Keep the existing rollback logic (`rollback_after_header_publish_failure` in `create.rs:219-237` and siblings) unchanged — it handles vault-DB / key-file cleanup that the helper does not own.

For `recover_with_phrase.rs:28-58`, replace the inline download block with:

```rust
let staging_dir = staging::staging_directory().await?;
let header = crate::storage::cloud::download_vault_header(
    cloud_transport,
    &staging_dir,
    VaultHeaderTrustPolicy::Bootstrap,
)
.await
.map_err(|_| AuthenticationError::VaultHeaderInvalid)?;
```

Keep the subsequent `Uuid::parse_str` → `VaultId::from_uuid` and `NoRecoverySlot` check unchanged.

Remove the now-unused constants `STAGING_FILE_NAME` and `VAULT_HEADER_BLOB_NAME` in `auth/ceremonies/mod.rs:27-31` **if** no remaining callers reference them (expected: none remain after Step 3; verify with `Grep "STAGING_FILE_NAME"` and `Grep "VAULT_HEADER_BLOB_NAME"` within `src-tauri/src/auth`). If `MANIFEST_BACKUP_BLOB_NAME` remains used by `recover_with_phrase.rs`, leave it; Phase 4.4 owns that constant's relocation.

**Step 4 — Update `auth/staging.rs` module comment** (maps C-3 documentation update).

Edit the Phase 4.3 paragraph in `src-tauri/src/auth/staging.rs:1-16` so it reads: "The startup retry loop for `pending-vault-header.json` is deferred to Phase 4.5 push flow (header upload is idempotent on every push)." Do **not** remove the Windows DACL TODO — that stays.

**Step 5 — Unit tests in `vault_header_io.rs`** (maps deliverable 7).

Each test uses `MockCloudTransport` + `tempfile::tempdir()`. Test names follow `test_<unit>_<scenario>_<expected_outcome>`:

- `test_upload_vault_header_stores_plaintext_json_at_expected_remote_path` — build tier-1 header, upload, `download_blob` raw bytes, assert UTF-8 JSON and `serde_json::from_slice::<VaultHeader>` round-trips.
- `test_upload_vault_header_removes_staging_file_on_success` — upload, assert `staging_dir.join("pending-vault-header.json")` does not exist.
- `test_upload_vault_header_removes_staging_file_on_transport_failure` — inject `RcloneProcessFailed` via mock; call upload; assert staging file is gone and return is `Err(VaultHeaderSyncError::Transport(_))`.
- `test_upload_vault_header_rejects_serialise_failure` — construct a header with invalid UTF-8 is impossible (all fields are `String`); skip or cover via a custom serialiser that forces `SerialiseFailed` (lower priority; retain only if cheap).
- `test_download_vault_header_round_trip_preserves_tier1_header_fields` — upload a tier-1 header, download under `Bootstrap`, assert struct equality.
- `test_download_vault_header_round_trip_preserves_tier2_with_recovery_slot` — include one BIP-39 recovery slot with valid 32-byte salt + 72-byte wrapped blob, verify struct-equality after round-trip.
- `test_download_vault_header_rejects_malformed_json` — pre-seed mock with `b"not json"`, assert `Err(DeserialiseFailed)` and temp file removed.
- `test_download_vault_header_rejects_structurally_invalid_header` — upload a header with `argon2_salt` base64 of 16 bytes; assert `Err(Validation(VaultHeaderError::SaltWrongLength))`.
- `test_download_vault_header_rejects_argon2_params_below_floor` — upload with `memory_cost = 19_455`; assert `Err(Validation(VaultHeaderError::Argon2ParamsBelowMinimum))`.
- `test_download_vault_header_rejects_recovery_slot_wrapped_blob_wrong_length` — recovery slot with 40-byte wrapped blob; assert `Err(Validation(VaultHeaderError::RecoverySlotBlobWrongLength))`.
- `test_download_vault_header_rejects_existing_device_anchor_mismatch` — upload a valid header; invoke with `ExistingDevice { trusted_anchor }` where `vault_id` differs; assert `Err(Validation(VaultHeaderError::TrustedVaultIdMismatch))`.
- `test_download_vault_header_removes_temp_file_on_success_and_failure` — parametric; assert `pending-vault-header-download.json` is absent after both a `NotFound` and a happy-path download.
- `test_download_vault_header_surfaces_transport_not_found` — empty mock; assert `Err(Transport(CloudTransportError::NotFound))`.
- `test_download_vault_header_bootstrap_accepts_below_arx_defaults_without_error` — set `memory_cost=19_456, time_cost=2, parallelism=1`; assert `Ok(_)` returned (warn log is non-observable in unit tests without a subscriber, so assert `Ok` path only).
- `test_upload_vault_header_overwrites_previous_remote_blob_idempotently` — upload twice with different recovery-slot counts, download, assert the second state wins.

Ceremony-side behaviour is covered by the existing tests in `create.rs`, `change_password.rs`, etc.; verify they still pass after Step 3 without modification.

**Step 6 — Rust checks.** Run per `.claude/rules/rust.md`:

```
cargo fmt --manifest-path src-tauri/Cargo.toml
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-targets --all-features
```

## 6. Review focus areas

### 6a. Rust change surface
- `src-tauri/src/storage/cloud/vault_header_io.rs` (new).
- `src-tauri/src/storage/cloud/mod.rs` (module registration + re-exports).
- `src-tauri/src/auth/ceremonies/mod.rs` (delete/relocate `VAULT_HEADER_BLOB_NAME` and `STAGING_FILE_NAME` constants if unused after refactor).
- `src-tauri/src/auth/ceremonies/create.rs`.
- `src-tauri/src/auth/ceremonies/change_password.rs`.
- `src-tauri/src/auth/ceremonies/rotate_key_file.rs`.
- `src-tauri/src/auth/ceremonies/setup_recovery.rs`.
- `src-tauri/src/auth/ceremonies/recover_with_phrase.rs`.
- `src-tauri/src/auth/staging.rs` (module comment update only).

### 6b. Security-sensitive paths
- `src-tauri/src/storage/cloud/vault_header_io.rs` — public plaintext JSON at cloud root. Security concerns: (a) staging file permissions must match existing owner-only mode on Unix; (b) temp path cleanup must be best-effort and never leak key material to logs; (c) `tracing::warn!` must emit structured fields only (schema version, numeric Argon2 params) — no JSON bytes, no header fields like `vault_id` or `key_file_blake3` in the warn line (even though both are safe per design, conservative logging discipline keeps surface narrow); (d) all error variants must avoid including blob content in `Display`.
- `src-tauri/src/auth/ceremonies/*` — refactor must preserve the existing error-mapping boundary (`AuthenticationError::VaultHeaderInvalid`) exactly; no new error distinguishability that would enable oracle attacks. `recover_with_phrase` must keep returning `VaultHeaderInvalid` (not `NoRecoverySlot`) for download/validation failures.

### 6c. Architecture risk areas
- **Module boundary discipline.** `vault_header_io.rs` must depend only on `storage::cloud::{CloudTransport, CloudTransportError}`, `storage::cloud::vault_header::*`, `serde_json`, `tokio::fs`, `tracing`. It must NOT depend on `auth::*` (ceremonies call it, not the other way around).
- **One-concern-per-file rule.** `vault_header.rs` stays struct + validation; `vault_header_io.rs` owns sync helpers. Do not collapse these into one file.
- **Re-export discipline.** Ceremonies import via `crate::storage::cloud::{upload_vault_header, download_vault_header, VAULT_HEADER_BLOB_NAME}` — not `crate::storage::cloud::vault_header_io::...`. Verify `pub use` in `mod.rs`.
- **Error surface minimality.** No new variants added to `CloudTransportError`, `VaultHeaderError`, or `AuthenticationError`. New errors live in `VaultHeaderSyncError` only.
- **No blocking I/O on Tauri thread.** Helpers are `async fn`; use `tokio::fs` exclusively per `.claude/rules/rust.md#io`.

### 6d. Testing requirements

Per sub-phase Validation Checkpoint (`4.3-vault-header.md:22-37`):

```
cargo test storage::cloud::vault_header
cargo test storage::cloud::vault_header_io
```

Required test coverage:
- Upload → download round-trip preserves all fields (tier 1, tier 2, with and without recovery slots).
- Validation rejects malformed headers (undersized salt, wrong-length BLAKE3, below-minimum Argon2, wrong-length wrapped recovery blob).
- Recovery slots preserved across round-trip (method, salt, params, wrapped_master_key).
- Transport failure paths do not leak temp files.
- Existing-device anchor mismatch surfaces `VaultHeaderError::Trusted*Mismatch`.
- Pre-existing `#[cfg(test)]` modules in `vault_header.rs` continue to pass unchanged.
- Ceremony tests (`create.rs`, `change_password.rs`, `rotate_key_file.rs`, `setup_recovery.rs`, `recover_with_phrase.rs`) continue to pass after the refactor.

Manual verification (integration tests against real cloud remain Phase 4.5 scope):
- Not required this run per the sub-phase's "Acceptance criteria" — automated tests are sufficient for 4.3 sign-off.

## 7. Documentation impact

- **Required this run.**
  - `src-tauri/src/auth/staging.rs:1-16` — module-level comment: update Phase 4.3 reference to defer startup retry to Phase 4.5 (see C-3). This is a code-comment change, not a `docs/` change.
- **Deferred / follow-up.**
  - `docs/architecture/designs/cloud-synchronisation/sub-phases/4.3-vault-header.md` — no change; sub-phase deliverables remain accurate after this run. (Optional future: add a line noting `local-vault-params.json` persistence is tracked by Phase 4.5.)
  - `docs/architecture/designs/cloud-synchronisation/design.md#local-vault-parameter-cache` — unchanged; persistence implementation deferred (C-1). Rationale: keep the canonical design intact; the cache is already specified, only implementation is deferred.
  - `.claude/rules/storage.md` Cloud-backup section — no change required; existing wording ("vault header stays plaintext JSON at cloud root" / "Push flow uploads … vault header idempotently on every push") still holds.
  - `.claude/rules/auth.md` "Forward declarations" line — no change this run. Phase 4.5 will revise when the startup retry is implemented.

## 8. Governance sync actions (pre-implementation)

| Action ID | Reason / linked concern | Target files | Required edit | Verification |
|---|---|---|---|---|
| **G-1** | C-3 — align module comment with the defer-to-4.5 decision | `src-tauri/src/auth/staging.rs:1-16` | Replace "Phase 4.3 startup retry path can consume" with "Phase 4.5 push flow will consume (header upload is idempotent per push)" and leave the Windows DACL follow-up TODO in place. | After edit, `rg "Phase 4\.3 startup retry" src-tauri` returns no matches. |
| **G-2** | Align `vault_header.rs` forward-declaration comment with the outcome of this sub-phase | `src-tauri/src/storage/cloud/vault_header.rs:1-8` | Replace the "Phase 4.3 will adopt this struct as-is, add richer validation, and wire the startup retry path" sentence with "Phase 4.3 added `vault_header_io::{upload_vault_header, download_vault_header}`; startup retry is deferred to Phase 4.5." Keep the `MasterKey` containment rule paragraph unchanged. | `rg "Phase 4\.3 will" src-tauri/src/storage/cloud/vault_header.rs` returns no matches. |

No `.claude/rules/*.md` edits are required, so `/copilot-sync` is not triggered.

## 9. Handoff Notes for Implementer

Working directory: `C:\Users\chris\source\repos\arx-runa`. The plan is self-contained — do not re-read the sub-phase unless resolving an edge case on the exact Download-and-parse validation order (then consult `docs/architecture/designs/cloud-synchronisation/design.md:748-776`).

Order of operations: (1) Step 1 (`vault_header_io.rs` + `VaultHeaderSyncError`), (2) Step 2 (`mod.rs` re-exports), (3) Step 5 tests green in isolation, (4) Step 3 ceremony refactor one file at a time — run `cargo test -p arx-runa auth::ceremonies` between each file, (5) Step 4 and Step 8 (G-1, G-2) module-comment edits, (6) Step 6 workspace-wide checks.

Traps:
- **Windows staging permissions.** `tokio::fs::write` does not set owner-only ACL on Windows; match the existing `auth::staging::write_owner_only_inner` pattern (Unix `0o600` + Windows default DACL) rather than introducing a separate helper. If you factor out a helper, keep it private to `vault_header_io` — do **not** import `crate::auth::staging::write_owner_only` from storage (would violate the storage→auth direction rule from `.claude/rules/rust.md`).
- **Test-only exports.** `MockCloudTransport` is under `#[cfg(any(test, feature = "test-utils"))]`; the new tests in `vault_header_io.rs` should import it via `use crate::storage::cloud::mock::MockCloudTransport;` inside a `#[cfg(test)] mod tests` block.
- **`tokio::fs::remove_file` on non-existent paths.** After the temp file is deleted, any retry must not propagate `NotFound` as an error; use `match ... Err(e) if e.kind() == ErrorKind::NotFound => Ok(())`.
- **`serde_json::to_vec_pretty` vs `to_vec`.** Ceremonies already use `to_vec_pretty`; keep that in `upload_vault_header` for parity with current on-disk output.
- **Constant ownership migration.** If you relocate `VAULT_HEADER_BLOB_NAME` from `auth/ceremonies/mod.rs` to `vault_header_io.rs`, every remaining reference in ceremonies must be re-imported from the new home; run `rg "VAULT_HEADER_BLOB_NAME" src-tauri/src` after the refactor.
- **Do not touch `MANIFEST_BACKUP_BLOB_NAME`** — Phase 4.4 owns it.
- **Do not introduce `local-vault-params.json` read/write** — C-1 defers it; resist the temptation even though `TrustedVaultHeaderAnchor` is already on hand.

## Implementation Log

- **Date**: 2026-04-19T16:59:55.0367953+02:00
- **Run ID**: `phase-4-3-vault-header-20260419-162254`
- **Track**: `full`
- **Branch**: `development`
- **Execution mode**: orchestrator fallback (coding steps implemented directly; reviewers/classifier/solver/test-writer delegated)

### Agent evidence

| Approach step | Agent | Agent ID | Outcome |
|---|---|---|---|
| Structured context build | plan-context-builder | `plan-digest-43` | `PLAN_DIGEST` produced |
| Structured context build | rules-extractor | `rules-index-43` | `RULES_INDEX` produced |
| Structured context build | design-extractor | `design-index-43-1` | `DESIGN_INDEX` produced |
| Structured context build | shard-planner | `shard-map-43-1` | `SHARD_MAP` + `SHARD_DIGEST_SUMMARY[]` produced |
| Review cycle 1 | rust-reviewer | `rust-review-43-retry` | Findings emitted |
| Review cycle 1 | security-reviewer | `security-review-43` | Findings emitted |
| Review cycle 1 | architecture-reviewer | `arch-review-43` | Findings emitted |
| Review cycle 1 | cross-shard-reviewer | `cross-shard-43` | Findings emitted |
| Findings gate cycle 1 | finding-classifier | `classify-43-c1` | `CLASSIFIED_FINDINGS` produced |
| Remediation synthesis cycle 1 | problem-solver | `solver-43-c1-retry` | `SOLUTION_PACK` produced |
| Review cycle 2 | rust-reviewer | `rust-review-43-c2` | `NO_ACTIONABLE_FINDINGS` |
| Review cycle 2 | security-reviewer | `security-review-43-c2-retry` | 1 warning emitted |
| Review cycle 2 | architecture-reviewer | `arch-review-43-c2` | `NO_STRUCTURAL_FINDINGS` |
| Review cycle 2 | cross-shard-reviewer | `cross-shard-43-c2` | `NO_CROSS_SHARD_FINDINGS` |
| Findings gate cycle 2 | finding-classifier | `classify-43-c2` | Remaining warning classified `DEFERRED_BY_PLAN` |
| Testing audit | test-writer | `test-writer-43` | Added adversarial temp-cleanup test in `vault_header_io.rs` |

### Files changed

- `.claude/plans/phase-4-3-vault-header.md`
- `docs/architecture/designs/cloud-synchronisation/sub-phases/4.3-vault-header.md`
- `.claude/runs/phase-4-3-vault-header-20260419-162254/run-state.json`
- `.claude/runs/phase-4-3-vault-header-20260419-162254/cycle-1.json`
- `.claude/runs/phase-4-3-vault-header-20260419-162254/cycle-2.json`
- `src-tauri/src/storage/cloud/vault_header_io.rs`
- `src-tauri/src/storage/cloud/mod.rs`
- `src-tauri/src/storage/cloud/vault_header.rs`
- `src-tauri/src/auth/staging.rs`
- `src-tauri/src/auth/ceremonies/helpers.rs`
- `src-tauri/src/auth/ceremonies/mod.rs`
- `src-tauri/src/auth/ceremonies/create.rs`
- `src-tauri/src/auth/ceremonies/change_password.rs`
- `src-tauri/src/auth/ceremonies/rotate_key_file.rs`
- `src-tauri/src/auth/ceremonies/setup_recovery.rs`
- `src-tauri/src/auth/ceremonies/recover_with_phrase.rs`

### Verification

- **Formatting check** (`cargo fmt --all -- --check`): clean.
- **Clippy** (`cargo clippy --workspace --all-targets --all-features -- -D warnings`): clean.
- **Tests** (`cargo test --workspace --all-targets --all-features`): clean (`479 passed, 0 failed, 1 ignored` in `arx-runa-tauri` test suite; workspace commands succeeded).
- **Release build** (`cargo build --workspace --release`): success.

### Review summaries

- **Rust review**: cycle 1 findings remediated; cycle 2 reported no actionable findings.
- **Architecture review**: cycle 1 findings addressed for in-scope 4.3 concerns; cycle 2 reported no structural findings.
- **Security review**: cycle 1 CRITICAL/HIGH items addressed for 4.3 scope; cycle 2 left one Windows ACL warning deferred by plan.
- **Cross-shard review**: cycle 1 raised consistency findings around publish/retention boundary; cycle 2 reported no cross-shard findings.

### Findings quality gate

- **Counts by disposition (all cycles)**: `ACTIONABLE_NOW=7`, `INTENTIONAL_DECISION=0`, `DEFERRED_BY_PLAN=2`, `INSUFFICIENT_EVIDENCE=3`.
- **Finding overrides**: None.
- **Design challenge outcomes**: None.

### Governance sync

- **Action count**: 2 (`G-1`, `G-2`).
- **Files updated**: `src-tauri/src/auth/staging.rs`, `src-tauri/src/storage/cloud/vault_header.rs`.
- **`/copilot-sync`**: not required (no `.claude/rules/*.md` changes).

### Sub-phase decisions sync

- **Doc path**: `docs/architecture/designs/cloud-synchronisation/sub-phases/4.3-vault-header.md`
- **Updates**: added `## Implementation Decisions` with 5 run decisions (including 4.5 deferrals).

### Deviations from plan

- Added shared ceremony mapping helper `map_vault_header_sync_error` to enforce uniform auth-boundary error translation.
- Kept `STAGING_FILE_NAME` as `#[cfg(test)]` in ceremony `mod.rs` because it is now test-only after helper migration.
- Added an adversarial transport-failure cleanup test for `download_vault_header` to tighten temp-file lifecycle coverage.

### Documentation flagged

- **Required this run**: `src-tauri/src/auth/staging.rs:1-16` module comment updated to defer startup retry to Phase 4.5.
- **Deferred / optional from Section 7**:
  - `docs/architecture/designs/cloud-synchronisation/sub-phases/4.3-vault-header.md` optional note about `local-vault-params.json` persistence tracking.
  - `docs/architecture/designs/cloud-synchronisation/design.md#local-vault-parameter-cache` implementation remains deferred.
  - `.claude/rules/storage.md` unchanged.
  - `.claude/rules/auth.md` forward-declarations line unchanged (planned for Phase 4.5).

### Run state path

- `.claude/runs/phase-4-3-vault-header-20260419-162254/`
