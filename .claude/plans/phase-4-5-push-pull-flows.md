---
title: "Phase 4.5 — Push/Pull Flows and Conflict Detection"
created: "2026-04-19T20:12:00+02:00"
status: approved
roadmap-phase: 4
sub-phase: "4.5"
design-document: "docs/architecture/designs/cloud-synchronisation/design.md"
sub-phase-roadmap: "docs/architecture/designs/cloud-synchronisation/sub-phases/roadmap.md"
governance-sync-required: true
tags: [storage, cloud, push, pull, conflict-detection, sync, phase-4]
---

# Plan: Phase 4.5 — Push/Pull Flows and Conflict Detection

## 1. Goal

Land the end-to-end cloud synchronisation surface at `src-tauri/src/storage/cloud/sync.rs`: `push_vault` (conflict-check → randomised parallel blob upload → `snapshot_counter` increment with rollback-on-failure → manifest backup upload → idempotent vault-header upload), `pull_vault` (vault-header + manifest download → authentication hook → parallel blob download with BLAKE3 verification → failure accumulation), and `delete_vault_from_cloud` (best-effort full cloud cleanup). Introduce the `SyncError` / `SyncConflict` error surface (Phase 6 Stub canonicalised here) and two SQLCipher-specific sync helpers on `SqlCipherMetadataStore` (`list_sync_chunks` and `rollback_snapshot_counter`) without extending the `MetadataStore` trait.

## 2. Context

**Sub-phase position.** 4.5 is the final unit of the cloud-sync roadmap (4.1 → 4.2 → 4.3 → 4.4 → **4.5**). Dependencies met as of 2026-04-19: `CloudTransport` + `MockCloudTransport` (4.1), `RcloneTransport` (4.2), `upload_vault_header` / `download_vault_header` (4.3), `upload_manifest_backup` / `download_manifest_backup` (4.4). Phase 3 provides `MetadataStore::increment_snapshot_counter` and the `chunks.blob_name` + `chunks.blake3_checksum` columns. Roadmap allotment: ~400 production + ~250 test lines; **security review required**.

**Canonical design sections** (`docs/architecture/designs/cloud-synchronisation/design.md`):
- `#push-flow` (lines 842–918) — 12-step push pipeline including conflict check, Fisher-Yates upload shuffle, `tokio::JoinSet` concurrency (default 4), snapshot-counter rollback on manifest-upload failure, idempotent vault-header upload.
- `#pull-flow` (lines 922–946) — 9-step pull pipeline with plaintext vault-header bootstrap, manifest decrypt + import, parallel blob download with BLAKE3 verification, per-blob failure accumulation.
- `#conflict-detection` (lines 950–972) — cloud vs. local `snapshot_counter` matrix (`==` safe / `>` needs pull / `<` stale-cloud abort / `NotFound` first push) and manual-resolution contract.
- `#cloud-garbage-collection` (lines 976–989) — best-effort semantics (orphan ciphertext harmless).
- `#vault-deletion-full-cloud-cleanup` (lines 993–1017) — list-then-delete vault/ blobs, manifest backup, vault-header; partial failure semantics.
- `#error-recovery` (lines 1071–1098) — interrupted push, failed manifest-upload rollback, interrupted pull, Rclone crash.
- `#progress-reporting-phase-6-stub` (lines 1144–1192) — `SyncProgress` / `SyncOperation` data model (frontend-facing; deferred to Phase 6 UI).
- `#conflict-representation-phase-6-stub` (lines 1196–1229) — `SyncConflict { local_counter, cloud_counter, local_last_synced, cloud_last_synced }`.
- `#security-analysis` (lines 1102–1140) — path sanitisation, manifest replay, Rclone trust boundary.
- `#contract-surface` (lines 19–46) — `CloudTransport`, cloud layout, `snapshot_counter` monotonicity, manifest-backup AEAD-under-`manifest_key`, vault-header plaintext.

**Sub-phase source.** `docs/architecture/designs/cloud-synchronisation/sub-phases/4.5-push-pull-flows.md` — deliverables 1–5 (push_vault, pull_vault, conflict tests, delete_vault_from_cloud, error-recovery tests), implementation notes (`tokio::task::JoinSet`, Fisher-Yates in-place, SQLCipher-specific sync helpers, BLAKE3 before decrypt, snapshot-counter rollback via transaction or explicit revert, best-effort delete), security-review gate.

**Existing code state as of 2026-04-19.**
- `src-tauri/src/storage/cloud/mod.rs:1-38` re-exports the 4.1–4.4 surface; no `sync` submodule declared. `sync_config.rs` is unrelated (it exposes `SyncConfig { max_concurrent=4, operation_timeout_seconds=300 }`, validated to 1..=16 / 60..=3600).
- `src-tauri/src/storage/cloud/manifest_backup.rs:113-173` owns `upload_manifest_backup(vault_db_path, sqlcipher_key, manifest_key, cloud_transport, staging_dir) -> Result<(), ManifestBackupSyncError>` and lines 176-265 own `download_manifest_backup(cloud_transport, staging_dir, manifest_key, destination_db_path, sqlcipher_key) -> Result<(), ManifestBackupSyncError>`. `MANIFEST_BACKUP_BLOB_NAME = "manifest/manifest-backup.blob"` is `pub const`. The download helper **persists to `destination_db_path` and refuses to overwrite existing files** (line 187-194 `destination exists` short-circuit) — suitable for new-device pull, but push's conflict check needs a throwaway destination that is deleted after reading the counter (see C-2).
- `src-tauri/src/storage/cloud/vault_header_io.rs:50-64, 1-27` owns `upload_vault_header(header, cloud_transport, staging_dir) -> Result<(), VaultHeaderSyncError>`, `download_vault_header`, and `VAULT_HEADER_BLOB_NAME = "vault-header.json"`. Upload is idempotent (overwrite) and accepts `&VaultHeader` by reference.
- `src-tauri/src/storage/metadata_store.rs:11-97` defines the Phase 3 trait surface; the last method is `increment_snapshot_counter(&self) -> Result<u64, StorageError>` (the canonical `snapshot_counter` mutation path). `set_meta` rejects the immutable keys `schema_version`, `vault_id`, `snapshot_counter`, `chunk_size_bytes`, `epoch_buffer_enabled` — so rollback cannot go through `set_meta`.
- `src-tauri/src/storage/sqlcipher.rs:98-117` establishes the SQLCipher-specific helper pattern via `pub(crate) async fn list_all_blob_names(&self) -> HashSet<String>`, explicitly documented as "not exposed on `MetadataStore`". Lines 137-145 expose `pub(crate) with_connection_mut` for further SQLCipher-specific mutations. `.claude/rules/storage.md` pins this pattern ("`list_all_blob_names` remains a SQLCipher-specific helper and must not be added to `MetadataStore`").
- `src-tauri/src/storage/types/chunk_record.rs:7-20` carries `blob_name: String` and `blake3_checksum: [u8; 32]` — the pair the pull flow needs.
- `src-tauri/src/crypto/checksum.rs:12-51` provides `compute_checksum(&[u8]) -> Blake3Hash` and `verify_checksum(Vec<u8>, &Blake3Hash) -> Result<VerifiedBlob, CryptoError>`; `VerifiedBlob::into_inner` is `pub(crate)` inside the crypto module. For pull-time BLAKE3 verification of a file on disk, `compute_checksum(&bytes) == expected` is the right primitive (no decrypt invocation here — pull stops at staging/blob placement; decryption is caller-side `decrypt_file`).
- `src-tauri/src/storage/cloud/mock.rs:30-139` ships `MockCloudTransport` with per-path one-shot failure injection (`inject_failure(path, CloudTransportErrorKind)`). List semantics: prefix-exact, sorted. Failure injection is `#[cfg(any(test, feature = "test-utils"))]`; the kind mirror mirrors every `CloudTransportError` variant.
- `src-tauri/src/storage/cloud/remote_path.rs` (Phase 4.1) houses the `^[a-zA-Z0-9._/-]+$` + reject-`..` + reject-leading-`/` allowlist that push/pull must route all dynamic path construction through. Blob names are validated as UUID v4 via `src-tauri/src/storage/validation.rs::validate_blob_name_uuid_v4`.
- `src-tauri/src/storage/staging.rs:13-56` provides `default_staging_directory()` (→ `dirs::data_dir().join("arx-runa/staging")`), `ensure_staging_directory`, and `write_owner_only`. The push and pull flows consume `&Path` to the caller-supplied staging directory (matches 4.3/4.4 shape).
- `src-tauri/src/storage/vault_ops/{upload_file,download_file,delete_file}.rs` are the Phase 3 orchestration entry points that stage blobs, insert chunk rows, and handle local blob lifecycle. `delete_file` (delete_file.rs:33-44) best-effort-removes local staging after `MetadataStore::delete_node` (which enqueues into `pending_deletions`); cloud deletion of the enqueued rows is **not** part of 4.5 (see C-5).
- `.claude/rules/storage.md` "Cloud backup" section already declares: "Push flow uploads manifest backup, then uploads vault header idempotently on every push." "Snapshot model: atomic full export, `snapshot_counter` increments each push." And the `MetadataStore` Phase 3.1 surface listing (line containing `increment_snapshot_counter`). `list_all_blob_names` is flagged as SQLCipher-specific, giving precedent for adding further sync helpers in the same namespace.
- `.claude/rules/storage.md` "Deletion" section declares "If blob deletion is interrupted, orphan encrypted blobs are cleaned on startup" — the push flow does not assume responsibility for draining `pending_deletions`; see C-5.
- `docs/architecture/design-invariants.md` #5 (vault path validation allowlist) and #10 (`pending_deletions` durable-deletion rule) apply to this sub-phase. #10 is out of scope operationally (drain happens at startup per storage.md); the allowlist applies to every dynamic remote-path construction in push/pull/delete.
- No `sync.rs` or `SyncError` exists yet. `CloudTransportError`, `ManifestBackupSyncError`, and `VaultHeaderSyncError` are the neighbouring error types; the new `SyncError` composes them via `#[from]` conversions.

**No pending architectural decisions** apply to this sub-phase; `## Contract Surface` in the design is canonical and has not drifted in this area since Phase 4.4.

**Security review.** Sub-phase declares **Required** (4.5-push-pull-flows.md:89-95). Roadmap confirms: "Phase 4.5: Requires `security-reviewer` agent review (conflict detection correctness, BLAKE3 verification)." Scope for the reviewer: (a) conflict-detection correctness — `cloud == local` / `cloud > local` / `cloud < local` / `NotFound` branches including decryption-failure-as-conflict; (b) snapshot-counter rollback atomicity after manifest-upload failure (no interleaving that could leave cloud or local in a "phantom-pushed" state); (c) Fisher-Yates shuffle RNG (CSPRNG required to preserve upload-order unlinkability claim in design security analysis); (d) BLAKE3 verification unambiguously gates any use of downloaded blobs; (e) path sanitisation through `remote_path` helper for every dynamic remote path (blob list, prefix list, delete); (f) best-effort delete semantics do not swallow authentication-level errors that hide an attacker-induced partial-deletion; (g) plaintext cloud-manifest exposure in the conflict-check probe (temp DB on disk must be deleted on every exit path with owner-only permissions on Unix).

## 3. Design Concerns / Open Questions

| Concern | Source | Impact | Classification | Resolution | Documentation updates |
|---|---|---|---|---|---|
| **C-1 `snapshot_counter` rollback has no sanctioned mutation path.** The `MetadataStore` trait forbids `set_meta` on `snapshot_counter` (metadata_store.rs:89-91), `increment_snapshot_counter` is the only mutation path, and design step 11 mandates rolling the counter back to its prior value on manifest-upload failure. | `sub-phases/4.5-push-pull-flows.md:24` (push deliverable "On manifest upload failure: roll back `snapshot_counter`"), `design.md:871-872, 1080-1085` (error-recovery section), `metadata_store.rs:85-96` | Without a rollback mechanism, a manifest-upload-failure path cannot re-establish the invariant `local.snapshot_counter == cloud.snapshot_counter` — the next retry's conflict check would falsely fire `cloud < local` and refuse to push (design step 3). | Non-blocking | Add `SqlCipherMetadataStore::rollback_snapshot_counter(previous_value: u64)` as a SQLCipher-specific helper (not on `MetadataStore`), mirroring the `list_all_blob_names` precedent (sqlcipher.rs:98-117). Implementation: `UPDATE manifest_meta SET value = ?1 WHERE key = 'snapshot_counter'` with a verification read inside the same transaction asserting the current stored value equals `previous_value + 1` (otherwise reject with `StorageError::Database("snapshot_counter rollback precondition violated")`). Push flow calls `rollback_snapshot_counter(previous_counter)` after a manifest-upload failure. See CS-004. | Update `.claude/rules/storage.md` "Cloud backup" section to add one bullet: `rollback_snapshot_counter` is a SQLCipher-specific helper on `SqlCipherMetadataStore`, not on `MetadataStore`; invoked only by the push flow after manifest-upload failure. |
| **C-2 Conflict-check probe requires a one-shot cloud-manifest decrypt without overwriting the live local DB.** `download_manifest_backup` (manifest_backup.rs:187-194) refuses to overwrite an existing `destination_db_path` and always persists to disk, so push cannot reuse it for the "read cloud counter" step. | `design.md:847-855` (push steps 1–4), `manifest_backup.rs:175-265` | The push flow must decrypt the cloud manifest, read `snapshot_counter` (and `last_synced_at` for the `SyncConflict` payload), and discard the temp DB — without touching the active local manifest. Misusing `download_manifest_backup`'s persistence would either require deleting the local DB first (catastrophic) or an ad-hoc temp path with manual cleanup. | Non-blocking | Treat the conflict-check probe as a distinct pipeline: call `download_manifest_backup` with `destination_db_path = staging_dir.join("manifest-backup-conflict-probe.db")`, pre-deleting any stale probe file; open the probe DB with `sqlcipher_key` via the existing crate-internal `storage::sqlcipher::open::open_sqlcipher` helper; `SELECT value FROM manifest_meta WHERE key = 'snapshot_counter'` and (optionally) `'last_synced_at'`; close the connection; delete the probe file on every exit path (success and error). Encapsulate in an internal helper `read_cloud_snapshot_state(cloud_transport, staging_dir, manifest_key, sqlcipher_key) -> Result<Option<CloudSnapshotState>, SyncError>` that returns `Ok(None)` when the cloud manifest is `CloudTransportError::NotFound` (first push). See CS-003. | None (internal implementation detail). |
| **C-3 Design step 2 says "If decryption fails: treat as conflict; abort" but the conflict `SyncError` payload requires a `cloud_counter` value that is unknown on decrypt failure.** | `design.md:850-851`, `design.md:1196-1215` | Returning `SyncError::Conflict(SyncConflict { cloud_counter: ??? })` is undefined when decryption fails. Using a sentinel (`u64::MAX`) would leak into the UI. | Non-blocking | Expose a distinct `SyncError::CloudManifestUnreadable { reason: ManifestBackupSyncError }` variant for cryptographic/integrity failures of the cloud manifest backup. Callers map both `CloudManifestUnreadable` and `Conflict` to the same user-facing "cannot sync — manual intervention" path in Phase 6, but the Rust boundary keeps them distinct to preserve truth. Design step 2 ("treat as conflict; abort") is preserved semantically — push aborts in both branches. Test plan exercises both outcomes separately. | Add one line to the design's `#conflict-detection` section note: "decryption/integrity failure of the cloud manifest backup surfaces as `SyncError::CloudManifestUnreadable`, semantically a conflict." Target: `docs/architecture/designs/cloud-synchronisation/design.md` at the end of `#conflict-detection`. Mark **deferred** (low-priority doc update); `/implement-plan` can log and skip. |
| **C-4 Sub-phase "collect all blob_names from chunks table with staging files" needs blob_name + blake3_checksum for pull, but only blob_name for push step 5, and the existing `list_all_blob_names` helper (sqlcipher.rs:102) returns a `HashSet<String>` with no checksum.** | `design.md:856-859, 933-936`, `sqlcipher.rs:98-117` | Push needs `Vec<String>` (to preserve iteration order for deterministic shuffle-then-enumerate testing); pull needs `Vec<(String, [u8; 32])>` for BLAKE3 verification. Reusing `list_all_blob_names` would force pull to re-query per-blob and defeat the design's "read all chunk rows" one-shot. | Non-blocking | Add `SqlCipherMetadataStore::list_sync_chunks(&self) -> Result<Vec<SyncChunkRecord>, StorageError>` where `SyncChunkRecord { blob_name: String, blake3_checksum: [u8; 32] }` (newtype lives in `storage::cloud::sync`, not in the generic `types/`, because it is sync-flow-specific and never persisted). SQL: `SELECT blob_name, blake3_checksum FROM chunks ORDER BY blob_name ASC`. Both push (`SELECT blob_name` column is subset) and pull consume the same result. Push filters to blobs whose staging file exists via `tokio::fs::metadata` before shuffle. `list_all_blob_names` stays as-is (different type, different purpose). See CS-005. | Update `.claude/rules/storage.md` "Cloud backup" section to add a bullet: `list_sync_chunks` is a SQLCipher-specific helper on `SqlCipherMetadataStore`, not on `MetadataStore`; returns `(blob_name, blake3_checksum)` pairs for push/pull flows. |
| **C-5 `pending_deletions` drain is not listed as a 4.5 deliverable even though push is the natural drain point.** | `sub-phases/4.5-push-pull-flows.md:11-41` vs. `design.md:976-989`, `.claude/rules/storage.md` Deletion section ("orphan encrypted blobs are cleaned on startup"), `design-invariants.md` #10 | Push leaves `pending_deletions` un-drained; deletes only resolve at vault startup (via a separate cleanup path that is currently absent — `storage::staging::cleanup_orphaned_blobs` targets local files only, not cloud rows). First-push flows will therefore never cloud-delete blobs that were deleted locally. | Non-blocking | **Out of scope for 4.5**. Sub-phase deliverables do not list cloud GC; storage rules already document startup-drain semantics; design's "Cloud Garbage Collection" section is non-blocking for the push/pull happy path and can be wired in a follow-up sub-phase (or folded into Phase 6 orchestration). Add an explicit handoff note in Section 9 so `/implement-plan` does not drift into implementing GC. | Add one TODO line to the design's `#cloud-garbage-collection` section: "Drain-on-push and drain-on-startup implementation is tracked separately from Phase 4.5." Target: `docs/architecture/designs/cloud-synchronisation/design.md`. Mark **deferred**. |
| **C-6 `delete_vault_from_cloud` scope is ambiguous: sub-phase says "full cleanup" but design's Vault Deletion section enumerates both cloud operations (steps 2–6) and local-only operations (steps 7–9).** | `sub-phases/4.5-push-pull-flows.md:40`, `design.md:997-1009` | Unclear whether the backend function tears down local SQLCipher, staging, and cloud-config (Phase 6 UX concern) or strictly the cloud-side (steps 2–6). | Non-blocking | Resolve by **function naming**: `delete_vault_from_cloud` covers cloud-side only (list vault/ → delete each blob → delete manifest backup → delete vault header, in that order). Local teardown is a Phase 6 UI-orchestration concern that will compose this with local-file deletion. Shared directory cleanup (design step 6) is deferred because Phase 5 sharing is not implemented. Function returns `Result<CloudDeletionReport, SyncError>` where `CloudDeletionReport { vault_blobs_deleted: usize, vault_blobs_failed: Vec<String>, manifest_backup_deleted: bool, vault_header_deleted: bool }` so partial-deletion state is observable. See CS-006. | None. |
| **C-7 Sub-phase requires Fisher-Yates shuffle but does not specify RNG source.** | `sub-phases/4.5-push-pull-flows.md:17, 81`, `design.md:878-885` (upload-order-randomisation security claim cites restic PR #5295 whose threat model requires unpredictable-to-adversary order) | If a weak RNG (e.g., `rand::thread_rng` seeded with low entropy, or a deterministic seed in tests leaking into production) is used, the unlinkability claim in the design's security analysis is vacuous. | Non-blocking | Use `rand::rngs::OsRng` (CSPRNG) as the Fisher-Yates RNG in production, passed as `&mut dyn RngCore` to the internal shuffle function so tests can inject a seeded RNG (`rand::rngs::StdRng::seed_from_u64`) via a `#[cfg(test)]`-only variant of `push_vault`. The production surface never exposes the seeded variant. This also supports the "randomises blob upload order" test (`test_push_flow_randomises_blob_upload_order`) which asserts order differs across runs and matches a deterministic seed when injected. | Update `.claude/rules/storage.md` "Cloud backup" section to add one bullet: Fisher-Yates upload-order shuffle uses `OsRng` (CSPRNG) in production; deterministic seeding is permitted only under `#[cfg(test)]`. |
| **C-8 Concurrency semantics on per-blob upload failure: design step 7c says "stop issuing new tasks, drain in-flight" but does not specify whether already-spawned tasks are aborted.** | `sub-phases/4.5-push-pull-flows.md:21`, `design.md:861-866` | Aborting in-flight tasks risks corrupting cloud state (an in-flight Rclone invocation that is mid-upload with `upload_blob`'s overwrite semantics). Draining lets them complete. | Non-blocking | Interpret "stop issuing new tasks, drain in-flight" as **no abort**: the spawn loop exits on first per-task error; the `JoinSet` is polled via `join_next().await` until empty, discarding success/failure of drained tasks (they are all idempotent — upload overwrites). Record the first failure as the returned `SyncError::PushUploadFailed { first_error: Box<CloudTransportError>, successful_uploads: Vec<String> }`. Do **not** call `JoinSet::abort_all()`. This matches design error-recovery rationale: "Blobs already uploaded to `vault/` are harmless (opaque ciphertext, random names)" and "Remaining staging blobs are uploaded on retry; `upload_blob` is idempotent." See CS-007. | None. |
| **C-9 Pull flow step 8b says "On mismatch: delete downloaded file, record error (do not abort)" — but the same decision for concurrent JoinSet tasks differs from push's stop-on-first-error.** | `design.md:938-943` | Pull is specified as accumulate-all-failures; push is stop-on-first. Symmetric-looking design, different semantics. | Non-blocking | Implement pull without the stop-on-first-error short-circuit: every spawned download runs to completion (or fails) independently, verification failures and transport failures both collect into `SyncError::PullIncomplete { verification_failures: Vec<String>, transport_failures: Vec<(String, CloudTransportError)> }`. Pull returns `Ok(())` only when both lists are empty. See CS-007. | None. |
| **C-10 First-push detection currently relies on `download_manifest_backup` returning `ManifestBackupSyncError::Transport(CloudTransportError::NotFound)`, but the conflict probe (C-2) adds a DB-open layer that could mask or wrap the `NotFound` surface.** | `design.md:850, 892-918`, `manifest_backup.rs:201-207` | If `NotFound` is wrapped into a less-specific variant by the probe helper, push would fall into the "unknown cloud state" branch and fail. Design demands first-push to skip the conflict check cleanly. | Non-blocking | `read_cloud_snapshot_state` (C-2) explicitly pattern-matches `ManifestBackupSyncError::Transport(CloudTransportError::NotFound)` before opening any DB and returns `Ok(None)`. All other variants bubble up, with decrypt/integrity failures surfacing as `SyncError::CloudManifestUnreadable` (C-3). Tested via `test_push_flow_first_push_with_no_cloud_manifest_skips_conflict_check`. | None. |

No blocking concerns. `status: draft` in frontmatter; all gaps are additive or naming-level and the Contract Surface has no contradictions.

## 4. Assumptions

- **A-1 Module layout.** Production code lives in a new `src-tauri/src/storage/cloud/sync.rs` (top-level sync module) with the public surface re-exported from `storage::cloud::mod.rs` as `{push_vault, pull_vault, delete_vault_from_cloud, SyncError, SyncConflict, CloudDeletionReport}`. Internal helpers (`read_cloud_snapshot_state`, `fisher_yates_shuffle`, `SyncChunkRecord`) stay `pub(crate)` within `sync.rs` to keep the cloud module boundary clean.
- **A-2 `push_vault` signature.**
  ```rust
  pub async fn push_vault(
      vault_db_path: &Path,
      sqlcipher_key: &SqlcipherKey,
      manifest_key: &ManifestKey,
      metadata_store: &SqlCipherMetadataStore,
      cloud_transport: &dyn CloudTransport,
      vault_header: &VaultHeader,
      staging_dir: &Path,
      sync_config: &SyncConfig,
  ) -> Result<PushReport, SyncError>;
  ```
  `PushReport { blobs_uploaded: usize, snapshot_counter_after: u64, duration_seconds: f64 }`. The parameter takes `&SqlCipherMetadataStore` (not `&dyn MetadataStore`) because the SQLCipher-specific helpers (`list_sync_chunks`, `rollback_snapshot_counter`) are required (C-1, C-4).
- **A-3 `pull_vault` signature.**
  ```rust
  pub async fn pull_vault(
      vault_db_path: &Path,
      sqlcipher_key: &SqlcipherKey,
      manifest_key: &ManifestKey,
      metadata_store_after_import: &SqlCipherMetadataStore,
      cloud_transport: &dyn CloudTransport,
      staging_dir: &Path,
      sync_config: &SyncConfig,
  ) -> Result<PullReport, SyncError>;
  ```
  `PullReport { blobs_downloaded: usize, blobs_skipped_present: usize, duration_seconds: f64 }`. The caller owns vault-header download, authentication, and `SessionKeys` derivation; `pull_vault` starts at "cloud manifest download + import + blob download + BLAKE3 verify" to keep the function single-responsibility. The design's step 1–3 (vault-header + authenticate + derive) are invoked by the Phase 6 UI shell, which composes `download_vault_header`, the auth ceremony (`unlock_vault` / `recover_vault`), and then `pull_vault`. This mirrors how the existing `recover_vault` ceremony is split.
- **A-4 `delete_vault_from_cloud` signature.**
  ```rust
  pub async fn delete_vault_from_cloud(
      cloud_transport: &dyn CloudTransport,
  ) -> Result<CloudDeletionReport, SyncError>;
  ```
  Cloud-side only. Caller (Phase 6 UI) owns local SQLCipher deletion and confirmation UX.
- **A-5 Conflict-probe staging file name.** `staging_dir/manifest-backup-conflict-probe.db` — constant defined locally in `sync.rs` as `const CONFLICT_PROBE_DB_FILE_NAME: &str = "manifest-backup-conflict-probe.db";`. Owner-only 0o600 on Unix via `std::fs::set_permissions` after `download_manifest_backup` creates the file (persistence is done by the existing helper; the probe deletes on every exit path).
- **A-6 Blob remote path construction.** Every dynamic remote path goes through `storage::cloud::remote_path::sanitise(&str) -> Result<String, ...>` (Phase 4.1). Blob names are pre-validated via `storage::validation::validate_blob_name_uuid_v4` before path assembly (defence in depth — the manifest could technically hold a malformed row). Path template: `format!("vault/{}.blob", blob_name)` after validation.
- **A-7 `last_synced_at` key.** Non-immutable (not in `set_meta`'s reject list); push uses `metadata_store.set_meta("last_synced_at", unix_timestamp.to_string()).await?` after counter increment and before manifest-backup upload. Timestamp source is `std::time::SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()`; `.unwrap_or_default()` is safe because pre-1970 clocks are out-of-scope.
- **A-8 BLAKE3 verification primitive.** Pull step 8b uses `crate::crypto::compute_checksum(&bytes)` where `bytes` is the full downloaded blob read via `tokio::fs::read`, then compares the returned `Blake3Hash` to the `blake3_checksum` column (`ChunkRecord::blake3_checksum` via `SyncChunkRecord`). `verify_checksum` is **not** used here because the pull flow does not decrypt — it stops at verified-blob-on-disk and the staged blob remains for subsequent `download_file` orchestration. This preserves the crypto module's check-before-decrypt invariant at the callers (every `decrypt_chunk` invocation continues to require a `VerifiedBlob`).
- **A-9 Filter "blobs not already present locally" (pull step 7).** Probe via `tokio::fs::try_exists(staging_dir.join(format!("{}.blob", blob_name))).await?`. Existence alone is sufficient — the subsequent BLAKE3 check on the would-be-download path is skipped for already-present blobs because `download_file` (the eventual decrypt consumer) runs its own `verify_checksum` before `decrypt_chunk`. Pull flow thus trusts locally-present blobs are intact-enough to defer to the decrypt-path verification.
- **A-10 `JoinSet` concurrency bound.** Respect `sync_config.max_concurrent`. The spawn loop acquires a semaphore permit (`tokio::sync::Semaphore::new(max_concurrent as usize)`) before spawning each upload/download task; the task releases on completion. This avoids a bursty `JoinSet` that spawns all tasks upfront and blocks Rclone subprocess fork on OS limits.
- **A-11 Sanity on `max_concurrent=0` impossible.** `SyncConfig::validate` already rejects `max_concurrent < 1`; `push_vault` / `pull_vault` require the caller to have validated the config. No defensive recheck.
- **A-12 Manifest-upload rollback is unconditional on failure.** If `upload_manifest_backup` returns `Err(_)`, push invokes `rollback_snapshot_counter(previous_counter)`. If rollback itself errors, the push returns `SyncError::RollbackFailed { manifest_error, rollback_error }` (both boxed). This is a hard-failure signal that the local DB is in an inconsistent state and a full manual recovery is required; the UI (Phase 6) surfaces this as a critical error.
- **A-13 Vault-header upload on every push is unconditional and best-effort-on-retry.** Push executes `upload_vault_header` after the manifest backup upload succeeds (design step 12, "idempotent"). A vault-header upload failure does not trigger a snapshot-counter rollback because the manifest is already canonically updated in the cloud — re-push will re-attempt the header (design rationale: "Cheap and safe to upload unconditionally on every push"). Error surfaces as `SyncError::VaultHeaderUploadFailed { source: VaultHeaderSyncError }`; the UI can present this as a "header desynced — retry push" with no data risk.
- **A-14 Test transport.** All unit and integration tests use `MockCloudTransport` (storage::cloud::mock). The push/pull happy path, conflict scenarios, and rollback scenarios are exercised via `inject_failure` on the canonical paths (`manifest/manifest-backup.blob`, `vault/<uuid>.blob`, `vault-header.json`). Fisher-Yates determinism test uses the `#[cfg(test)]`-only seeded-RNG variant (C-7).
- **A-15 Concurrent-call behaviour.** `push_vault` and `pull_vault` do not take a process-wide lock — Phase 6 UI is expected to serialize at the command level (one push or pull at a time per vault). No invariant is violated by concurrent invocation because each step uses its own SQLCipher connection and owner-semantics on the staging files; however simultaneous pushes against the same cloud endpoint could race on manifest-backup upload (last-writer-wins). Document the single-invocation assumption in Section 9 / module comment; do not over-engineer locking at the storage layer.
- **A-16 Idle-vault push.** Calling `push_vault` on a vault with no staged blobs and no pending counter changes still executes: (a) conflict-check (safe), (b) zero-blob shuffle (no-op), (c) counter increment (→ n+1), (d) manifest-upload (reflects n+1), (e) vault-header upload. This matches design step 9's unconditional increment — each call produces a new snapshot identity even when no blobs changed, which keeps replay detection (design security analysis) honest.
- **A-17 Pull-side manifest import into an already-initialised vault.** `metadata_store_after_import` is expected to be opened on the vault DB that was just overwritten by `download_manifest_backup`. The caller (Phase 6 UI / new-device bootstrap) sequence: `download_manifest_backup` → reopen `SqlCipherMetadataStore` on the destination path → pass to `pull_vault`. `pull_vault` does not itself open or overwrite the DB; it only reads `list_sync_chunks` and downloads blobs.

## 5. Approach

### CONTRACT_SNIPPETS

- **CS-001 `SyncError` enum** (new type, `thiserror::Error`, `#[non_exhaustive]`, in `sync.rs`):

  ```rust
  #[derive(Debug, thiserror::Error)]
  #[non_exhaustive]
  pub enum SyncError {
      #[error("snapshot_counter conflict with cloud")]
      Conflict(SyncConflict),

      #[error("cloud manifest backup could not be decrypted or verified: {reason}")]
      CloudManifestUnreadable { reason: ManifestBackupSyncError },

      #[error("push failed during blob upload: {first_error}")]
      PushUploadFailed {
          first_error: Box<CloudTransportError>,
          successful_uploads: Vec<String>,
      },

      #[error("push failed during manifest backup upload: {source}")]
      PushManifestBackupFailed { source: ManifestBackupSyncError },

      #[error("snapshot_counter rollback failed after manifest-upload error")]
      RollbackFailed {
          manifest_error: Box<ManifestBackupSyncError>,
          rollback_error: Box<StorageError>,
      },

      #[error("vault header upload failed: {source}")]
      VaultHeaderUploadFailed { source: VaultHeaderSyncError },

      #[error("pull completed with failures")]
      PullIncomplete {
          verification_failures: Vec<String>,
          transport_failures: Vec<(String, CloudTransportError)>,
      },

      #[error("cloud transport operation failed: {source}")]
      Transport {
          #[from]
          source: CloudTransportError,
      },

      #[error("manifest backup operation failed: {source}")]
      ManifestBackup {
          #[from]
          source: ManifestBackupSyncError,
      },

      #[error("storage error: {source}")]
      Storage {
          #[from]
          source: StorageError,
      },

      #[error("I/O error: {0}")]
      Io(#[from] std::io::Error),
  }
  ```

- **CS-002 `SyncConflict` struct** (canonical from design #conflict-representation-phase-6-stub):

  ```rust
  #[derive(Debug, Clone, serde::Serialize, PartialEq, Eq)]
  pub struct SyncConflict {
      pub local_counter: u64,
      pub cloud_counter: u64,
      pub local_last_synced: Option<i64>,
      pub cloud_last_synced: Option<i64>,
  }
  ```

- **CS-003 `CloudSnapshotState` (internal, `pub(crate)` in sync.rs):**

  ```rust
  pub(crate) struct CloudSnapshotState {
      pub snapshot_counter: u64,
      pub last_synced_at: Option<i64>,
  }

  pub(crate) async fn read_cloud_snapshot_state(
      cloud_transport: &dyn CloudTransport,
      staging_dir: &Path,
      manifest_key: &ManifestKey,
      sqlcipher_key: &SqlcipherKey,
  ) -> Result<Option<CloudSnapshotState>, SyncError>;
  ```

  Returns `Ok(None)` when `CloudTransportError::NotFound`; `Err(SyncError::CloudManifestUnreadable { .. })` on decrypt/integrity failure; `Ok(Some(..))` otherwise. Probe DB at `staging_dir.join("manifest-backup-conflict-probe.db")` is deleted on every exit path.

- **CS-004 `SqlCipherMetadataStore::rollback_snapshot_counter`** (new SQLCipher-specific helper, `pub(crate)`):

  ```rust
  impl SqlCipherMetadataStore {
      pub(crate) async fn rollback_snapshot_counter(
          &self,
          previous_value: u64,
      ) -> Result<(), StorageError> { /* ... */ }
  }
  ```

  SQL (inside one transaction):

  ```sql
  SELECT value FROM manifest_meta WHERE key = 'snapshot_counter';
  -- assert value == previous_value + 1
  UPDATE manifest_meta SET value = ?previous_value_str WHERE key = 'snapshot_counter';
  ```

  Returns `Err(StorageError::Database(..))` when the precondition (current == previous + 1) fails.

- **CS-005 `SqlCipherMetadataStore::list_sync_chunks`** (new SQLCipher-specific helper, `pub(crate)`):

  ```rust
  #[derive(Debug, Clone, PartialEq, Eq)]
  pub(crate) struct SyncChunkRecord {
      pub blob_name: String,
      pub blake3_checksum: [u8; 32],
  }

  impl SqlCipherMetadataStore {
      pub(crate) async fn list_sync_chunks(&self) -> Result<Vec<SyncChunkRecord>, StorageError> { /* ... */ }
  }
  ```

  SQL: `SELECT blob_name, blake3_checksum FROM chunks ORDER BY blob_name ASC`. Canonical ordering is alphabetical to keep the pre-shuffle input deterministic in tests; shuffle happens downstream.

- **CS-006 `CloudDeletionReport`:**

  ```rust
  #[derive(Debug, Clone)]
  pub struct CloudDeletionReport {
      pub vault_blobs_deleted: usize,
      pub vault_blobs_failed: Vec<String>,
      pub manifest_backup_deleted: bool,
      pub vault_header_deleted: bool,
  }
  ```

- **CS-007 Concurrency primitives (internal to sync.rs):**

  Push upload driver (per-blob semaphore + JoinSet + stop-on-first-error):

  ```rust
  async fn drive_blob_uploads(
      blobs: Vec<String>,
      cloud_transport: &dyn CloudTransport,
      staging_dir: &Path,
      max_concurrent: usize,
  ) -> Result<Vec<String>, (Box<CloudTransportError>, Vec<String>)>;
  ```

  Returns `Ok(successful_uploads)` or `Err((first_error, successful_uploads))`. Drains the JoinSet without `abort_all()` on error; deletes the local staging file per successful upload (design step 7b).

  Pull download driver (accumulate-all-failures):

  ```rust
  async fn drive_blob_downloads(
      chunks_to_fetch: Vec<SyncChunkRecord>,
      cloud_transport: &dyn CloudTransport,
      staging_dir: &Path,
      max_concurrent: usize,
  ) -> Result<usize, (Vec<String>, Vec<(String, CloudTransportError)>)>;
  ```

  Returns `Ok(count)` only when both failure lists are empty; otherwise `Err((verification_failures, transport_failures))`.

### Step-by-step implementation plan

1. **Create `src-tauri/src/storage/cloud/sync.rs`** with module-level doc comment referencing `design.md#push-flow` and `#pull-flow`. Declare `SyncError` [CS-001], `SyncConflict` [CS-002], `CloudSnapshotState` [CS-003], `CloudDeletionReport` [CS-006]. Register the module in `storage/cloud/mod.rs` (add `pub mod sync;` and re-export `pub use sync::{push_vault, pull_vault, delete_vault_from_cloud, SyncError, SyncConflict, CloudDeletionReport};`).

2. **Add SQLCipher helpers** in `src-tauri/src/storage/sqlcipher.rs`:
   - `list_sync_chunks` [CS-005] — placed alongside `list_all_blob_names` (sqlcipher.rs:98-117). Public `SyncChunkRecord` is declared in `sync.rs`; import from `crate::storage::cloud::sync::SyncChunkRecord` into `sqlcipher.rs`. (No cyclic dependency: `sqlcipher.rs` is deeper in the storage module than `cloud/sync.rs`, but both are descendants of `crate::storage`; `sqlcipher.rs` is already referenced from `cloud/manifest_backup.rs` which in turn is in the same `cloud/` subtree as `sync.rs`. If Rust module layering forces the newtype to live outside the cloud submodule, move `SyncChunkRecord` to `crate::storage::types::sync_chunk_record.rs` and re-export from `storage::cloud::sync` — decision at implementation time.) Add tests: `test_list_sync_chunks_returns_empty_when_no_rows`, `test_list_sync_chunks_returns_alphabetical_order`, `test_list_sync_chunks_includes_blake3_checksum_bytes`.
   - `rollback_snapshot_counter` [CS-004] — placed alongside `increment_snapshot_counter` (sqlcipher.rs:893). Tests: `test_rollback_snapshot_counter_restores_previous_value`, `test_rollback_snapshot_counter_rejects_when_precondition_violated`, `test_rollback_snapshot_counter_concurrent_with_increment_fails_one_side`.

3. **Implement `read_cloud_snapshot_state`** [CS-003] inside `sync.rs`. Pipeline: (a) `tokio::fs::remove_file` on any stale `CONFLICT_PROBE_DB_FILE_NAME`, tolerate `NotFound`; (b) call `download_manifest_backup(cloud_transport, staging_dir, manifest_key.expose(), &probe_path, sqlcipher_key)` — map `ManifestBackupSyncError::Transport(CloudTransportError::NotFound)` to `Ok(None)`, all other `ManifestBackupSyncError` variants to `Err(SyncError::CloudManifestUnreadable { reason })`; (c) open probe DB via `storage::sqlcipher::open::open_sqlcipher(&probe_path, sqlcipher_key.expose())` (relocated by Phase 4.4 — A-4 of phase-4-4 plan), read `snapshot_counter` and `last_synced_at` from `manifest_meta`, parse `u64` / `Option<i64>`; (d) drop connection; (e) `tokio::fs::remove_file(&probe_path)` unconditionally in a scopeguard-style cleanup that runs even on error. Tests: `test_read_cloud_snapshot_state_returns_none_on_not_found`, `test_read_cloud_snapshot_state_returns_state_on_present_manifest`, `test_read_cloud_snapshot_state_returns_unreadable_on_wrong_manifest_key`, `test_read_cloud_snapshot_state_deletes_probe_on_success_and_error`.

4. **Implement `fisher_yates_shuffle`** [C-7] inside `sync.rs`: `fn fisher_yates_shuffle<T, R: RngCore>(items: &mut [T], rng: &mut R)`. Production caller uses `rand::rngs::OsRng`. Tests: `test_fisher_yates_shuffle_preserves_length_and_elements`, `test_fisher_yates_shuffle_deterministic_with_seeded_rng` (seeded `StdRng`).

5. **Implement `drive_blob_uploads`** [CS-007, push half] inside `sync.rs`: (a) construct `Arc<Semaphore>(max_concurrent)`; (b) spawn each blob as `JoinSet::spawn` task that `acquire_owned().await`, calls `cloud_transport.upload_blob(&staging_path, &format!("vault/{blob}.blob"))`, then on success removes the staging file (design step 7b); (c) drain loop: `while let Some(join_result) = join_set.join_next().await { ... }` — on first inner `Err(CloudTransportError)` capture it and stop accepting new results via a flag (but keep draining); on inner `Ok(blob_name)` push to `successful_uploads`. Tests: `test_drive_blob_uploads_completes_all_when_no_errors`, `test_drive_blob_uploads_stops_spawning_on_first_error_but_drains`, `test_drive_blob_uploads_deletes_staging_file_after_each_success`, `test_drive_blob_uploads_respects_max_concurrent_semaphore`.

6. **Implement `drive_blob_downloads`** [CS-007, pull half] inside `sync.rs`: (a) same semaphore pattern; (b) for each `SyncChunkRecord`, spawn a task that downloads `vault/<blob>.blob` into `staging_dir/<blob>.blob`, then `tokio::fs::read` the file bytes, compute `blake3::hash` via `crate::crypto::compute_checksum`, compare to `blake3_checksum`; (c) on mismatch `tokio::fs::remove_file(&staging_path).await.ok(); verification_failures.push(blob_name)`; on transport error `transport_failures.push((blob_name, error))`. Tests: `test_drive_blob_downloads_completes_all_when_no_errors`, `test_drive_blob_downloads_records_verification_failure_and_deletes_file`, `test_drive_blob_downloads_records_transport_failure`, `test_drive_blob_downloads_continues_after_single_failure`.

7. **Implement `push_vault`** [A-2] inside `sync.rs`, composing the primitives above. Pipeline:
   1. Read local `snapshot_counter` via `metadata_store.get_meta("snapshot_counter")?.parse::<u64>()`. Read local `last_synced_at` (optional).
   2. Call `read_cloud_snapshot_state(...)`. Map result:
      - `Ok(None)` → skip conflict check (first push).
      - `Ok(Some(CloudSnapshotState { snapshot_counter: cloud, last_synced_at: cloud_ts }))`:
        - If `cloud != local` → `Err(SyncError::Conflict(SyncConflict { local, cloud, local_last_synced, cloud_last_synced: cloud_ts }))`.
        - Else continue.
      - `Err(SyncError::CloudManifestUnreadable { .. })` → bubble up.
   3. Call `metadata_store.list_sync_chunks()` → `Vec<SyncChunkRecord>`.
   4. Filter to blobs whose staging file exists: `tokio::fs::try_exists(staging_dir.join(format!("{}.blob", blob_name)))`.
   5. Shuffle `Vec<String>` via `fisher_yates_shuffle(..., OsRng)`.
   6. `drive_blob_uploads(...)`. On error: `Err(SyncError::PushUploadFailed { first_error, successful_uploads })`.
   7. `let new_counter = metadata_store.increment_snapshot_counter().await?;` (returns `local + 1`).
   8. `metadata_store.set_meta("last_synced_at", now_unix_seconds().to_string()).await?;`
   9. `upload_manifest_backup(vault_db_path, sqlcipher_key, manifest_key.expose(), cloud_transport, staging_dir).await`:
      - On `Err(source)` → `rollback_snapshot_counter(local)`. If rollback succeeds → `Err(SyncError::PushManifestBackupFailed { source })`. If rollback fails → `Err(SyncError::RollbackFailed { manifest_error: Box::new(source), rollback_error: Box::new(rollback_err) })`.
   10. `upload_vault_header(vault_header, cloud_transport, staging_dir).await` → on error `Err(SyncError::VaultHeaderUploadFailed { source })` (no rollback; A-13).
   11. Return `Ok(PushReport { blobs_uploaded, snapshot_counter_after: new_counter, duration_seconds })`.

8. **Implement `pull_vault`** [A-3] inside `sync.rs`:
   1. `metadata_store_after_import.list_sync_chunks()` → `Vec<SyncChunkRecord>`.
   2. Filter to blobs not already present at `staging_dir/<blob>.blob`.
   3. `drive_blob_downloads(...)`. On error → `Err(SyncError::PullIncomplete { verification_failures, transport_failures })`.
   4. Return `Ok(PullReport { blobs_downloaded, blobs_skipped_present, duration_seconds })`.

9. **Implement `delete_vault_from_cloud`** [A-4, CS-006] inside `sync.rs`:
   1. `let blobs = cloud_transport.list_blobs("vault/").await?;`
   2. For each blob in `blobs`: `cloud_transport.delete_blob(blob).await` — on error push `blob_name` to `vault_blobs_failed` (do not abort).
   3. `cloud_transport.delete_blob(MANIFEST_BACKUP_BLOB_NAME).await` — capture success/failure into `manifest_backup_deleted: bool`.
   4. `cloud_transport.delete_blob(VAULT_HEADER_BLOB_NAME).await` — capture into `vault_header_deleted`.
   5. Return `Ok(CloudDeletionReport { .. })`. Never return `Err(SyncError::..)` for per-blob best-effort failures; only bubble `list_blobs` transport failure as `Err(SyncError::Transport { source })`.

10. **Conflict-detection test suite** (sub-phase deliverable 3) in `sync.rs` `#[cfg(test)] mod tests`:
    - `test_push_vault_first_push_with_no_cloud_manifest_skips_conflict_check_and_succeeds`
    - `test_push_vault_aborts_when_cloud_counter_exceeds_local_with_conflict_error`
    - `test_push_vault_aborts_when_cloud_counter_below_local_with_conflict_error`
    - `test_push_vault_proceeds_when_cloud_counter_equals_local`
    - `test_push_vault_wrong_manifest_key_returns_cloud_manifest_unreadable`
    - `test_push_vault_corrupted_cloud_manifest_returns_cloud_manifest_unreadable`
    - `test_push_vault_sync_conflict_payload_carries_both_counters_and_timestamps`

11. **Error-recovery test suite** (sub-phase deliverable 5):
    - `test_push_vault_increments_snapshot_counter_only_after_successful_uploads`
    - `test_push_vault_rolls_back_snapshot_counter_on_manifest_upload_failure`
    - `test_push_vault_rollback_failure_surfaces_rollback_failed_variant`
    - `test_push_vault_vault_header_upload_failure_does_not_rollback_counter`
    - `test_push_vault_idempotent_on_retry_after_partial_upload_failure` (re-issue push after some blobs already in cloud; successful_uploads subset empty, retry completes)
    - `test_push_vault_concurrent_upload_failure_drains_without_abort` (inject failure on one blob, assert other uploads complete)
    - `test_pull_vault_continues_after_single_blob_verification_failure`
    - `test_pull_vault_rejects_blob_with_blake3_mismatch_and_records_in_report`
    - `test_pull_vault_continues_after_transport_failure_on_single_blob`
    - `test_pull_vault_skips_blobs_already_present_in_staging_directory`
    - `test_delete_vault_from_cloud_removes_all_vault_blobs_manifest_and_header`
    - `test_delete_vault_from_cloud_partial_failure_records_failed_blobs_and_still_attempts_manifest_and_header`
    - `test_delete_vault_from_cloud_list_blobs_failure_bubbles_transport_error`

12. **Integration test** (local Rclone remote, per design `#testing-strategy`) — new `tests/integration_cloud_sync.rs` or appended to existing cloud integration suite:
    - Full push → pull → decrypt round-trip with a 1 KiB file (1 chunk) and a 4 MiB + 1 byte file (2 chunks).
    - Assert: all canonical remote paths exist; cloud counter reflects post-push value; pull restores blobs; BLAKE3 verification passes; `decrypt_file` on the downloaded blobs reconstructs the original plaintext.

13. **Governance sync** (Section 8) — execute Action IDs G-1, G-2, G-3 before coding begins (only rule edits; ruleset must be consistent with the new helpers).

### Error-matrix summary

| Failure point | Local state change | Cloud state change | Variant |
|---|---|---|---|
| Conflict probe download (decrypt success, counter != local) | None | None | `SyncError::Conflict(SyncConflict { .. })` |
| Conflict probe download (decrypt/integrity failure) | None | None | `SyncError::CloudManifestUnreadable { .. }` |
| Per-blob upload (first failure) | Counter unchanged | Some blobs present, others absent | `SyncError::PushUploadFailed { .. }` |
| Manifest backup upload | Counter incremented then rolled back; `last_synced_at` updated (not rolled back — informational only) | Blobs present, manifest stale | `SyncError::PushManifestBackupFailed { .. }` |
| Rollback fails | Counter in indeterminate state | As above | `SyncError::RollbackFailed { .. }` |
| Vault header upload | Counter incremented, manifest current | Manifest current, header stale | `SyncError::VaultHeaderUploadFailed { .. }` |
| Pull per-blob verification / transport | None | None | `SyncError::PullIncomplete { .. }` |

## 6. Review focus areas

**6a. Rust change surface** — anticipated files under `src-tauri/**/*.rs`:
- `src-tauri/src/storage/cloud/sync.rs` (new, ~400 production + ~250 test LOC).
- `src-tauri/src/storage/cloud/mod.rs` (module registration + re-exports).
- `src-tauri/src/storage/sqlcipher.rs` (two new `pub(crate) async fn`s: `list_sync_chunks`, `rollback_snapshot_counter` + tests).
- `src-tauri/src/storage/mod.rs` (re-export `PushReport`, `PullReport`, `SyncError`, `SyncConflict`, `CloudDeletionReport` through `pub use cloud::{..}`).
- Possibly `src-tauri/src/storage/types/sync_chunk_record.rs` if module-cycle resolution demands relocation (see Approach step 2).
- Integration test file `src-tauri/tests/integration_cloud_sync.rs` (new, if no parent cloud-sync integration harness exists — confirm at implementation time).

**6b. Security-sensitive paths** — anticipated files under `src-tauri/src/{crypto,auth,storage}/`:
- `src-tauri/src/storage/cloud/sync.rs` — new file; all security concerns concentrate here. Reviewer checklist: (1) conflict-detection correctness matrix (`==`/`>`/`<`/`NotFound`/decrypt-fail) against design step 3; (2) snapshot-counter rollback precondition check prevents a concurrent increment from being silently undone; (3) Fisher-Yates RNG is `OsRng` in the production spawn path; (4) every dynamic remote path routes through `remote_path::sanitise` (`vault/<blob>.blob`, `vault/` prefix for delete); (5) conflict probe DB is deleted on every exit path and owner-only on Unix; (6) BLAKE3 verification uses `compute_checksum` on the full downloaded bytes — not on a partial read — before the caller can feed the file to `decrypt_chunk`; (7) best-effort delete does not swallow authentication errors in a way that hides a compromised endpoint (surface via `vault_blobs_failed`); (8) `ManifestKey::expose` / `SqlcipherKey::expose` are called only within the async task that owns them, never logged, never serialised.
- `src-tauri/src/storage/sqlcipher.rs` — reviewer checklist: rollback SQL precondition (`current == previous + 1`) is asserted inside the same transaction as the update; `list_sync_chunks` selects are bound parameters (no dynamic SQL string concatenation).

**6c. Architecture risk areas.**
- **SRP of `sync.rs`.** File owns three public flows (push / pull / delete) plus three internal drivers plus the error type. At ~400 production LOC this exceeds the "one concern per file" guideline in `.claude/rules/rust.md`. Mitigation: if the module exceeds ~500 LOC at review time, split into `sync/push.rs`, `sync/pull.rs`, `sync/delete.rs`, `sync/error.rs`, `sync/shuffle.rs`, `sync/concurrency.rs` with `sync/mod.rs` as re-exports. Decision at implementation time based on actual LOC.
- **Dependency direction.** `storage::cloud::sync` consumes `storage::sqlcipher::{SqlCipherMetadataStore, open_sqlcipher}`, `storage::cloud::{manifest_backup, vault_header_io, remote_path}`, `crypto::{ManifestKey, SqlcipherKey, compute_checksum}`. No upward dependency on `auth/` — the caller composes auth with sync.
- **Module visibility discipline.** `SyncChunkRecord` and `CloudSnapshotState` are `pub(crate)` (not public API surface). `SyncError`, `SyncConflict`, `PushReport`, `PullReport`, `CloudDeletionReport` are `pub`. Internal driver functions (`drive_blob_uploads`, `drive_blob_downloads`, `read_cloud_snapshot_state`, `fisher_yates_shuffle`) are `pub(crate)` to enable direct unit testing without a public surface.
- **Abstraction debt.** Resist introducing a `SyncOperation` trait or `PushDriver`/`PullDriver` types — the two flows are structurally similar but semantically divergent (stop-on-first-error vs accumulate-all). A trait would force a false-symmetry and hide the C-8/C-9 distinction.

**6d. Testing requirements.**
- Sub-phase validation checkpoint (4.5-push-pull-flows.md:47-66): `cargo test storage::cloud::sync` must pass. Manual verification: real cloud provider push/pull; conflict simulation (manual `snapshot_counter` edit in cloud manifest); new-device pull; vault deletion.
- Acceptance criteria: push flow completes for first + subsequent pushes; conflict detection catches both divergence directions; pull restores; BLAKE3 catches corruption; shuffle is randomised.
- Edge cases from Step 2 / Section 3:
  - First push with empty cloud (A-16).
  - First push with no staged blobs (zero-blob shuffle, counter still increments).
  - Push with `cloud_counter == local_counter`, zero blobs (degenerate success case).
  - Push with manifest-upload failure + rollback success (C-1).
  - Push with rollback failure (A-12).
  - Push with vault-header upload failure after successful manifest (A-13).
  - Pull with zero blobs to download (all already present).
  - Pull with mixed verification and transport failures in a single run.
  - Delete with `list_blobs("vault/")` transport failure.
  - Delete with some per-blob deletes failing (manifest and header still attempted).
  - Conflict probe: cloud-manifest `NotFound` → first-push behaviour.
  - Conflict probe: wrong `manifest_key` → `CloudManifestUnreadable`.
  - Fisher-Yates: deterministic under seeded RNG; entropy under `OsRng` (assert two consecutive shuffles of the same input differ — statistical tolerance acceptable for n >= 4).
- Proptest candidates: `prop_fisher_yates_preserves_multiset_for_any_ordered_input_and_rng_seed`.

## 7. Documentation impact

- `docs/architecture/designs/cloud-synchronisation/design.md`:
  - `#conflict-detection` — append a note mapping decryption/integrity failure to `SyncError::CloudManifestUnreadable` (C-3). **Deferred** (doc clarity only; does not change behaviour).
  - `#cloud-garbage-collection` — append a note that drain-on-push and drain-on-startup are tracked separately (C-5). **Deferred**.
- `.claude/rules/storage.md` "Cloud backup" section — add three bullets covering `rollback_snapshot_counter`, `list_sync_chunks`, and Fisher-Yates RNG policy. **Required this run** (governance sync gates the change).
- `docs/architecture/designs/cloud-synchronisation/diagrams/` — no new diagram required this run; push/pull flows are textual in the design. **Deferred** — if Phase 6 surfaces IPC diagrams, add a push-flow / pull-flow sequence diagram at that time.
- `docs/guides/glossary.md` — add entries for `SyncError`, `SyncConflict`, `snapshot_counter` rollback semantics, BLAKE3 pre-decrypt check. **Deferred** (glossary catches up post-implementation once Phase 6 surfaces these to end users).

## 8. Governance sync actions (pre-implementation)

| Action ID | Reason / linked concern | Target files | Required edit | Verification |
|---|---|---|---|---|
| **G-1** | C-1 | `.claude/rules/storage.md` ("Cloud backup" section) | Add bullet: `rollback_snapshot_counter` is a SQLCipher-specific helper on `SqlCipherMetadataStore` (not on `MetadataStore`); push-only, invoked after manifest-upload failure; enforces precondition that the stored counter equals `previous + 1`. | `grep -n 'rollback_snapshot_counter' .claude/rules/storage.md` finds the new bullet; `/copilot-sync` re-run produces no diff. |
| **G-2** | C-4 | `.claude/rules/storage.md` ("Cloud backup" section) | Add bullet: `list_sync_chunks` is a SQLCipher-specific helper on `SqlCipherMetadataStore` (not on `MetadataStore`); returns `(blob_name, blake3_checksum)` pairs in alphabetical order for push/pull flows. | `grep -n 'list_sync_chunks' .claude/rules/storage.md` finds the new bullet; `/copilot-sync` re-run produces no diff. |
| **G-3** | C-7 | `.claude/rules/storage.md` ("Cloud backup" section) | Add bullet: Fisher-Yates upload-order shuffle uses `rand::rngs::OsRng` (CSPRNG) in production; deterministic seeding is permitted only under `#[cfg(test)]`. | `grep -n 'Fisher-Yates' .claude/rules/storage.md` finds the new bullet; `/copilot-sync` re-run produces no diff. |

Run `/copilot-sync` after G-1, G-2, G-3 to propagate `.claude/rules/storage.md` changes to `.github/instructions/storage.instructions.md`.

## 9. Handoff Notes for Implementer

Working directory: `C:\Users\chris\source\repos\arx-runa`. The plan is self-contained — do **not** re-read the sub-phase; all relevant excerpts and signatures are inline. Order of operations: (1) execute Section 8 governance actions + `/copilot-sync`; (2) add SQLCipher helpers (`list_sync_chunks`, `rollback_snapshot_counter`) with tests in `sqlcipher.rs`; (3) create `storage/cloud/sync.rs` with types + internal drivers + unit tests; (4) wire `push_vault` / `pull_vault` / `delete_vault_from_cloud` with their full test suites; (5) add integration round-trip test; (6) run `cargo test --workspace --all-targets --all-features`; (7) run `cargo clippy -- -D warnings`; (8) spawn `security-reviewer` on the diff (Section 6b scope).

Traps:
- The conflict probe DB path (`staging_dir/manifest-backup-conflict-probe.db`) must be deleted on **every** exit path of `read_cloud_snapshot_state` — including the `NotFound` branch — or a stale file will cause the next invocation's `download_manifest_backup` to hit the "destination exists" short-circuit. Use a scopeguard or explicit cleanup in each arm.
- `download_manifest_backup` already refuses to overwrite — the probe helper must delete the stale file **before** calling `download_manifest_backup`, not after.
- `open_sqlcipher` (Phase 4.4 relocation target: `storage::sqlcipher::open_sqlcipher`) expects `&[u8; 32]`, not `SqlcipherKey`; dereference via `.expose()`.
- `rand::rngs::OsRng` is a ZST in `rand 0.8+`; do not confuse with `thread_rng` (not CSPRNG-guaranteed).
- `tokio::task::JoinSet` does not abort tasks on drop automatically — but we never drop it mid-drain; the driver loop completes before return.
- The new `SyncError::RollbackFailed` variant indicates local DB is in a wedged state (counter is one higher than cloud, last_synced_at moved). Phase 6 UI must treat this as critical.
- Fisher-Yates must shuffle in-place; pass `&mut [T]` not `Vec<T>` by value.
- The `set_meta("last_synced_at", ...)` call comes **after** `increment_snapshot_counter` and **before** `upload_manifest_backup` so the exported DB snapshot reflects the new timestamp.
- Single-invocation assumption (A-15): document at module level but do not implement locking.
- Do not drain `pending_deletions` in push — that is explicitly C-5 deferred.
- The integration test with a local Rclone remote (design `#testing-strategy`) requires the sidecar to be discoverable; if the harness cannot locate it, mark the test `#[ignore]` with a rationale and rely on `MockCloudTransport` for CI.
