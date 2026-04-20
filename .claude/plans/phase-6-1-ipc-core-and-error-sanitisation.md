---
title: "Phase 6.1 — IPC Core, Error Sanitisation, and Types"
created: "2026-04-20T17:00:00Z"
status: approved
roadmap-phase: 6
sub-phase: "6.1"
design-document: docs/architecture/designs/tauri-ipc-and-frontend/design.md
sub-phase-roadmap: docs/architecture/designs/tauri-ipc-and-frontend/sub-phases/roadmap.md
governance-sync-required: true
tags: [ipc, tauri, error-sanitisation, validation, security-critical]
---

## 1. Goal

Establish the Tauri IPC foundation for Phase 6: the sanitised `IpcError` enum with `From` mappings, every IPC response/request DTO, `AppState`, input validators, and the 30-command registration surface so the Leptos frontend (Phase 6.2+) can compile-time bind to a stable contract.

## 2. Context

- Sub-phase scope (roadmap Phase 6.1): ~400 production LoC + ~150 test LoC; first of four sub-phases (6.1 → 6.2 → 6.3 → 6.4).
- Parent design: `docs/architecture/designs/tauri-ipc-and-frontend/design.md` (last updated 2026-04-12). Canonical command surface: 30 commands across Auth (7), File (6), Sync (5), Destination (3), Sharing (8).
- Predecessor phases available on `development`:
  - Phase 2 auth: `AuthenticationError`, `SessionManager`, `FileKeySource`, `create_vault`/`change_password`/`rotate_key_file` ceremonies (take typed `*Request` structs).
  - Phase 3 storage: `SqlCipherMetadataStore`, `StorageError` (`Database | NotFound | ChecksumMismatch | Io | WrongKey | ConstraintViolation`), `validate_remote_path`, `validate_blob_name_uuid_v4`, `parse_chunk_size_bytes`.
  - Phase 4 cloud: `CloudTransport` trait + `RcloneTransport`, `CloudTransportError`, `storage::SyncError` (re-export of `storage::cloud::sync::SyncError`), `DestinationSessionPublic`, destination CRUD in `storage::cloud::destination_session`.
  - Phase 5 sharing: `SharingError`, `SharingStore`, `sharing::cloud::{create_share,fetch_received_share_to_local}`, `sharing::revocation::{revoke_share,strong_revoke_share}`, `sharing::identity`.
- Current state of `src-tauri/src/ui/`: placeholder `mod.rs` + empty `UiError` + empty `types/mod.rs`. Single `greet` command registered in `src-tauri/src/lib.rs`.
- `src-tauri/build.rs` is a no-op `tauri_build::build()` — no `AppManifest::commands(...)` allowlist yet.
- `src-tauri/tauri.conf.json` already has `withGlobalTauri: true`; CSP is `null` (Phase 6.4 problem).
- Cross-phase invariants touched: #4 (chunk size range), #5 (vault path validation), #6 (IPC password handling), #7 (Zero-Trace).
- Rule anchors: `.claude/rules/tauri.md`, `.claude/rules/rust.md`, `.claude/rules/auth.md`, `.claude/rules/storage.md`, `.claude/rules/sharing.md`.
- Security-reviewer agent review is **mandatory** after implementation (sub-phase §Security Review).

## 3. Design Concerns / Open Questions

| Concern | Source | Impact | Classification | Resolution | Documentation updates |
|---|---|---|---|---|---|
| 1. Design `From<storage::StorageError>` enumerates `FileNotFound { .. }`, `DirectoryNotFound { .. }`, `AlreadyExists { .. }` variants; real `StorageError` has only `Database`, `NotFound`, `ChecksumMismatch`, `Io`, `WrongKey`, `ConstraintViolation`. | design.md §Error Sanitisation lines 544-561 vs. `src-tauri/src/storage/error.rs`. | Implementer would write an impl that does not compile. | Non-blocking | Map the real variant set: `NotFound → IpcError::NotFound("File or directory not found")`, `ChecksumMismatch → IpcError::InternalError("Data integrity error")`, `WrongKey → IpcError::AuthenticationFailed("Vault database key mismatch")`, `ConstraintViolation(_) → IpcError::AlreadyExists("A record with this identifier already exists")` (most common cause — UNIQUE constraints on `node_id`, `blob_name`), `Database(_) → IpcError::InternalError("An error occurred")`, `Io(_) → IpcError::InternalError("An error occurred")`. Never interpolate inner messages. | GS-005: add a note under `.claude/rules/tauri.md` IPC section making the real `StorageError` variant set authoritative. |
| 2. Design `From<sync::SyncError>` targets `sync::SyncError`; the `sync` crate module is still an empty placeholder. The canonical type is `storage::SyncError` (re-export of `storage::cloud::sync::SyncError`) with variants `Conflict`, `CloudManifestUnreadable`, `PushUploadFailed`, `PushManifestBackupFailed`, `RollbackFailed`, `VaultHeaderUploadFailed`, `PullIncomplete`, `Transport`, `ManifestBackup`, `Storage`, `Io`. | design.md §Error Sanitisation lines 563-577 vs. `src-tauri/src/sync/error.rs` (empty) and `src-tauri/src/storage/cloud/sync.rs`. | Without correcting the target type, Phase 6.1 cannot compile; sync errors would silently fall back to `InternalError`. | Non-blocking | Implement `From<storage::SyncError> for IpcError` (re-exported via `storage::SyncError`). Map `Conflict(_) → IpcError::CloudError("Cloud snapshot conflict")`, `Transport { .. } / CloudManifestUnreadable { .. } / PushUploadFailed { .. } / PushManifestBackupFailed { .. } / VaultHeaderUploadFailed { .. } / PullIncomplete { .. } → IpcError::CloudError("Cloud operation failed")`, `Storage { .. } → delegate via nested From<StorageError>`, `Io(_) / ManifestBackup { .. } / RollbackFailed { .. } → IpcError::InternalError("An error occurred")`. Also implement `From<CloudTransportError> for IpcError` (maps `NotFound → IpcError::NotFound`, `AuthenticationFailed → IpcError::CloudError("Cloud authentication failed")`, other → `IpcError::CloudError("Cloud operation failed")`). | GS-006: update `.claude/rules/tauri.md` to reference `storage::SyncError` + `CloudTransportError` in the error-sanitisation surface, replacing the stale `sync::SyncError` reference. |
| 3. Design omits `From<SharingError> for IpcError`. Phase 5 sharing commands (`share_file`, `import_share`, `revoke_share`, `export_public_key`, `add_contact`, `list_contacts`) all surface `SharingError` variants the frontend must render as user-safe strings. | design.md §Error Sanitisation (gap) vs. canonical surface lists 8 sharing commands. | Without this impl, sharing commands degrade to `IpcError::InternalError` and leak no domain signal to the UI. | Non-blocking | Implement `From<SharingError> for IpcError` (see CS-004): `AuthenticationFailed → IpcError::AuthenticationFailed("Share authentication failed")`, `ContactNotFound / ShareNotFound / ReceivedShareNotFound → IpcError::NotFound(…)`, `ShareAlreadyRevoked / NoActiveSharesForRotation → IpcError::InvalidInput(…)`, `CloudOperation(_) / RevocationPartial { .. } → IpcError::CloudError(…)`, `MalformedSharePackage(_) / InvalidJsonPayload(_) / InvalidFileKeyLength(_) / InvalidSenderPublicKeyLength(_) / InvalidPublicKeyLength(_) / InvalidSharePackage → IpcError::InvalidInput("Share package is malformed")`, `EmptyDisplayName / InvalidContactId(_) → IpcError::InvalidInput(…)`, `ConstraintViolation(_) → IpcError::AlreadyExists(…)`, `IdentityMissing / Backend(_) → IpcError::InternalError("An error occurred")`. | GS-007: append `.claude/rules/sharing.md` IPC Error hygiene note declaring `SharingError::AuthenticationFailed` must map to `IpcError::AuthenticationFailed` without source context leak. |
| 4. `AppState.database` field type is documented as `Arc<RwLock<Option<DatabaseConnection>>>` but no `DatabaseConnection` type exists in the codebase; the concrete store is `SqlCipherMetadataStore`. | design.md §Application State line 851 vs. `src-tauri/src/storage/sqlcipher.rs`. | Implementer would either invent a stub type or fail to compile. | Non-blocking | Type the field as `Arc<RwLock<Option<SqlCipherMetadataStore>>>`. `SqlCipherMetadataStore` is already the concrete manifest store used by ceremonies, Phase 3-5 orchestration, and sync flows. | GS-008: correction note in `.claude/rules/tauri.md` clarifying the field type. |
| 5. Design `authenticate` command body delegates to `auth::authenticate_with_bytes(password_bytes, key_file_path, &state)` which does not exist. Real auth entry points are ceremony functions (`create_vault`/`change_password`/…) or `SessionManager::authenticate(password_utf8_bytes, key_source, salt, params)`. Authenticating against an existing vault requires first opening the SQLCipher DB (which itself requires a derived key from the password+salt), OR downloading the vault header from cloud to read the salt — neither is wired end-to-end. | design.md §Rust-side password zeroization at IPC boundary lines 938-951 vs. `src-tauri/src/auth/session/manager.rs`. | Writing full `authenticate` orchestration here would duplicate Phase 4.3 (vault header read) + Phase 2.4 work and exceed the sub-phase's ~400 LoC budget. | Non-blocking | Phase 6.1 stops at **command scaffolding**: each command handler validates inputs, converts password `String` → `Zeroizing<Vec<u8>>` at the boundary per invariant #6, and returns `IpcError::InternalError("command not yet wired")` where no existing backend entry point is a direct fit. Commands that map 1:1 to existing backend APIs (e.g. `lock_session → SessionManager::lock`, `get_session_status → SessionManager::state`, `list_contacts / list_destinations` via the respective stores) delegate directly. Full end-to-end wiring for `authenticate`, `create_vault`, `upload_file`, etc. is explicitly deferred to a Phase 6.5 / plan `phase-6-5-command-orchestration.md` follow-up **after** 6.2-6.4 make the frontend binding stable. This keeps 6.1 focused on the IPC contract boundary. | Handoff Note flags the deferred wiring. §7 documents the deferred orchestration plan. |
| 6. `validate_vault_path` design example allowlist regex `r"^[a-zA-Z0-9 ._/-]*$"` accepts the empty string, which is acceptable only when the command semantically allows root (`list_directory("")`). `upload_file`/`get_file_content` require non-empty paths. | design.md §Input Validation lines 886-915 vs. canonical command set. | Reject of empty path at validator level would break `list_directory`'s root case; accept would hide bugs elsewhere. | Non-blocking | Keep the validator permissive (empty allowed) and enforce non-empty at each command site that needs it (explicit `if vault_path.is_empty() { return Err(IpcError::InvalidInput("Path is required")) }`). Normalise leading "/" as equivalent to empty root in a helper `normalise_vault_path` so the frontend can send `"/"` without it being rejected by the validator. | None (internal refinement). |
| 7. `SessionManager` exponential backoff on `InvalidCredentials` is specified in design §Threat Model Additions lines 1639-1643 but `src-tauri/src/auth/session/manager.rs` line 218 still carries `TODO(phase-2.4/6.1): add per-vault exponential backoff`. | design.md line 1639 vs. session manager TODO. | Brute-force protection is listed as Phase 6.4's responsibility (§Security Review Checkpoints line 119). If not addressed in 6.1, it remains a gap through 6.2/6.3. | Non-blocking | Phase 6.1 does **not** implement backoff in `SessionManager`. Phase 6.4 owns the hardening check per roadmap. The 6.1 `authenticate` command scaffold carries a `// TODO(phase-6.4): backoff` comment as an anchor for the future edit. | Flagged in Handoff Notes; listed in §7 as deferred. |
| 8. Command bodies for long-running operations (`upload_file`, `download_file`, `sync_to_cloud`, `migrate_vault`, `sync_backup`, `recover_from_cloud`) take a `tauri::ipc::Channel<ProgressUpdate/SyncProgressUpdate/MigrationProgress>` per design contract (§Contract Surface line 47). Existing backend flows (`storage::vault_ops::upload_file`, `storage::cloud::push_vault`) do not yet accept a progress channel. | design.md §Command Signatures lines 188-264 vs. Phase 3/4 orchestration entry points. | Progress streaming wiring belongs with command orchestration (Concern 5). | Non-blocking | Phase 6.1 accepts the `progress` channel as a parameter to match the IPC contract but does not yet emit updates. Scaffold bodies return `IpcError::InternalError("command not yet wired")`; progress wiring lands alongside the full orchestration follow-up. | Noted in §7. |
| 9. Design registers `ui::authenticate` etc. as bare paths in `generate_handler!`. To preserve `.claude/rules/rust.md` "module layout: mod.rs re-exports only", handler functions live in `ui/auth_commands.rs` etc. and `ui/mod.rs` re-exports them. | design.md §Command Registration lines 375-410 vs. rust rule. | Risk of pushing bodies into `mod.rs`. | Non-blocking | `ui/mod.rs` only re-exports; every `#[tauri::command]` fn lives in its domain file. | None (rule-compliant structural choice). |
| 10. Sub-phase deliverable 9 says `validate_chunk_size` "rejects values below 131072, above 67108864, and zero". Zero is already `<131072`, so explicit zero-check is redundant. Sub-phase acceptance spec already lists zero as a rejection case, not a separate rule. | 6.1-ipc-core-and-error-sanitisation.md lines 38-42. | No impact; redundant phrasing. | Non-blocking | Implement as a single-range check returning `IpcError::InvalidInput("chunk_size_bytes must be between 131072 (128 KiB) and 67108864 (64 MiB)")`. Both `0` and `u64::MAX` fall through the same range check. Tests assert `0`, `1`, `131_071`, `67_108_865`, and `u64::MAX` rejection; `131_072` and `67_108_864` acceptance. | None. |
| 11. `DestinationSessionConfig` in design (`src/ui/types.rs`) is marked `Deserialize` only but design.md §IPC Response Types at line 780 uses `#[derive(Deserialize)]`. Canonical frontend wire type for `create_vault` / `add_destination` must deserialize from JS; the backend mapping from `DestinationSessionConfig` → `storage::cloud::destination_session::DestinationSession` is not spelled out. | design.md line 780 + §Command Signatures vs. real `DestinationSession` struct in `src-tauri/src/storage/cloud/destination_session.rs`. | Implementer could produce two mismatched structs. | Non-blocking | `DestinationSessionConfig` (IPC deserialize type) lives in `ui/types/destination_session_config.rs`. A `to_domain(&self, destination_id: String, rclone_remote_name: String) -> DestinationSession` helper builds the domain struct. `add_destination` assigns a new `Uuid::new_v4()` id and derives `rclone_remote_name` from `label` + destination id. Credential validation (`rclone lsd` probe) is deferred alongside the command orchestration follow-up (Concern 5). | §7: document the mapping helper. |
| 12. `delete_vault` takes a `confirmation: String` that must equal the vault name. Which vault name? The backend does not currently expose "current vault name" — the SQLCipher DB does not store it; vault header stores it (Phase 4.3). | design.md §Command Signatures lines 172-177. | Without a known-vault-name source, the check cannot run. | Non-blocking | Scaffold-level: accept `confirmation` and validate it is non-empty; return `InternalError("command not yet wired")` until the vault-header lookup path is sequenced. Record the dependency in the follow-up plan. | None. |
| 13. `From<auth::AuthenticationError>` design arms miss `SessionAlreadyActive`, `SessionNotActive`, `InvalidRecoveryPhrase`, `NoRecoverySlot`, `KeySource(_)` — all of which exist on the real enum. | design.md lines 521-542 vs. `src-tauri/src/auth/error.rs`. | Non-exhaustive `match` on `#[non_exhaustive]` enum — design snippet already uses `match err { … }` with no catch-all. Implementer must cover all variants. | Non-blocking | Implement all variants: `SessionAlreadyActive → IpcError::InvalidInput("A session is already active; lock it first")`, `SessionNotActive → IpcError::VaultLocked("No active session")`, `InvalidRecoveryPhrase → IpcError::InvalidInput("Recovery phrase is invalid")`, `NoRecoverySlot → IpcError::InvalidInput("No recovery slot configured for this vault")`, `KeySource(_) → IpcError::AuthenticationFailed("Key file is missing or invalid")`. Add a catch-all `_ => IpcError::InternalError("An error occurred")` for future `#[non_exhaustive]` growth. | None (confined to code). |

## 4. Assumptions

1. `src-tauri/src/ui/` is fleshed out with the full module split: `mod.rs`, `error.rs`, `state.rs`, `validation.rs`, `auth_commands.rs`, `file_commands.rs`, `sync_commands.rs`, `destination_commands.rs`, `sharing_commands.rs`, `types/mod.rs`, plus one file per DTO under `types/`. `mod.rs` is re-exports only.
2. The legacy `greet` command in `src-tauri/src/lib.rs` is deleted; `invoke_handler` binds only the 30 canonical commands.
3. `IpcError` replaces (not supplements) the empty `UiError` placeholder in `src-tauri/src/ui/error.rs`.
4. `AppState` is constructed in `lib.rs::run` with production wiring: `session_manager = Arc::new(SessionManager::from_config())`, `database = Arc::new(RwLock::new(None))` (populated by `authenticate`/`create_vault` when wired), `cloud_transport = Arc::new(storage::cloud::RcloneTransport::from_env_for_testing_or_default())` or a placeholder `Arc<dyn CloudTransport>` (Concern 5 defers real wiring), `device_monitor = Arc::new(platform-specific monitor)` via existing `cfg!(target_os = ...)` selection, `sync_status = Arc::new(RwLock::new(SyncStatus::default()))`.
5. `SyncStatus` lives in `ui/types/sync_status.rs` matching the design struct (`syncing: bool`, `last_synced_at: Option<String>`, `pending_changes: u32`) — this is the **IPC response type**, distinct from the lower-level snapshot state inside `storage::cloud::sync`.
6. Password `String` zeroization at IPC boundary: each password-receiving command runs `let password_bytes = Zeroizing::new(password.into_bytes());` as its first statement. `String::into_bytes()` consumes the `String` and its backing allocation becomes the `Vec<u8>` inside `Zeroizing`, which zeroes on Drop. Per invariant #6 interpretation, this satisfies "scrub the Rust String backing bytes, drop the original String" because the buffer is the same allocation, now tracked by `Zeroizing`.
7. `tracing::error!` calls log with `{:?}` (Debug format) server-side; catch-all arms interpolate no source text into the returned `IpcError`.
8. `validate_vault_path` uses a `OnceLock<Regex>` (not re-compiling per call) under `ui/validation.rs` with `r"^[a-zA-Z0-9 ._/-]*$"` per design, plus the four explicit checks (backslash, allowlist, absolute, traversal, control chars) and treats the empty string as root.
9. `validate_chunk_size` reuses `storage::validation::parse_chunk_size_bytes` semantics but returns an `IpcError::InvalidInput` with user-safe phrasing. We do NOT call `parse_chunk_size_bytes` directly (it takes `&str` and returns `StorageError`); instead, we duplicate the range check with `const MIN_CHUNK: u64 = 131_072; const MAX_CHUNK: u64 = 67_108_864;` constants co-located in `ui/validation.rs`.
10. `validate_file_id` uses `uuid::Uuid::parse_str` and asserts `parsed.get_version_num() == 4` for parity with `storage::validation::validate_blob_name_uuid_v4`.
11. Each `From` impl lives in `ui/error.rs`, in alphabetical order (`AuthenticationError → CloudTransportError → SharingError → StorageError → SyncError`), with tests asserting each variant maps to a sanitised `IpcError` (no path, key, or hex-like substrings in the emitted message).
12. `ui/mod.rs` additionally re-exports all command functions (bare names: `pub use auth_commands::{authenticate, create_vault, …};`) so `tauri::generate_handler![ui::authenticate, …]` resolves without the `_commands` suffix (per design §Command Registration).
13. `build.rs` carries the 30-entry `AppManifest::commands` allowlist exactly — names sorted alphabetically for stable diffs.
14. Tests: unit tests live in-file under `#[cfg(test)] mod tests` per project convention. Test names follow `test_<unit>_<scenario>_<expected_outcome>` per `.claude/rules/rust.md`.
15. Frontend-facing JSON shape is `{"kind": "notFound", "message": "…"}` with `kind` camelCased via `#[serde(rename_all = "camelCase")]`. Tests assert `serde_json::to_value` produces exactly this shape for each variant.
16. Commands that take `tauri::ipc::Channel<T>` keep the channel parameter in their signatures but do not emit on it in Phase 6.1.
17. No third-party Tauri plugins are added in 6.1 beyond the already-present `tauri-plugin-shell` (bundled rclone sidecar) and `tauri-plugin-opener` (OAuth browser launches).
18. The `regex` crate is already a dependency (`Cargo.toml` line 35).

## 5. Approach

### `CONTRACT_SNIPPETS`

**CS-001 — `IpcError` enum** (`src-tauri/src/ui/error.rs`)

```rust
use serde::Serialize;
use thiserror::Error;

/// Errors returned to the frontend. User-safe messages only.
#[derive(Debug, Serialize, Error)]
#[serde(tag = "kind", content = "message")]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub enum IpcError {
    #[error("Vault is locked: {0}")]
    VaultLocked(String),
    #[error("Authentication failed: {0}")]
    AuthenticationFailed(String),
    #[error("Not found: {0}")]
    NotFound(String),
    #[error("Already exists: {0}")]
    AlreadyExists(String),
    #[error("Cloud error: {0}")]
    CloudError(String),
    #[error("Invalid input: {0}")]
    InvalidInput(String),
    #[error("Internal error: {0}")]
    InternalError(String),
}
```

**CS-002 — `From<auth::AuthenticationError> for IpcError`** (`src-tauri/src/ui/error.rs`)

```rust
impl From<crate::auth::AuthenticationError> for IpcError {
    fn from(error: crate::auth::AuthenticationError) -> Self {
        tracing::error!("auth error: {:?}", error);
        use crate::auth::AuthenticationError as A;
        match error {
            A::InvalidCredentials => IpcError::AuthenticationFailed("Invalid credentials".into()),
            A::KeyFileNotFound => IpcError::AuthenticationFailed("Key file not found".into()),
            A::MemoryLockFailed(_) => IpcError::InternalError("Cannot lock memory for session keys".into()),
            A::VaultHeaderInvalid => IpcError::InternalError("Vault configuration error".into()),
            A::SessionAlreadyActive => IpcError::InvalidInput("A session is already active; lock it first".into()),
            A::SessionNotActive => IpcError::VaultLocked("No active session".into()),
            A::InvalidRecoveryPhrase => IpcError::InvalidInput("Recovery phrase is invalid".into()),
            A::NoRecoverySlot => IpcError::InvalidInput("No recovery slot configured for this vault".into()),
            A::KeySource(_) => IpcError::AuthenticationFailed("Key file is missing or invalid".into()),
            _ => IpcError::InternalError("An error occurred".into()),
        }
    }
}
```

**CS-003 — `From<storage::StorageError> for IpcError`** (real variants per Concern 1)

```rust
impl From<crate::storage::StorageError> for IpcError {
    fn from(error: crate::storage::StorageError) -> Self {
        tracing::error!("storage error: {:?}", error);
        use crate::storage::StorageError as S;
        match error {
            S::NotFound => IpcError::NotFound("File or directory not found".into()),
            S::ChecksumMismatch => IpcError::InternalError("Data integrity error".into()),
            S::WrongKey => IpcError::AuthenticationFailed("Vault database key mismatch".into()),
            S::ConstraintViolation(_) => IpcError::AlreadyExists("A record with this identifier already exists".into()),
            S::Database(_) | S::Io(_) => IpcError::InternalError("An error occurred".into()),
            _ => IpcError::InternalError("An error occurred".into()),
        }
    }
}
```

**CS-004 — `From<sharing::SharingError> for IpcError`** (new impl per Concern 3)

```rust
impl From<crate::sharing::SharingError> for IpcError {
    fn from(error: crate::sharing::SharingError) -> Self {
        tracing::error!("sharing error: {:?}", error);
        use crate::sharing::SharingError as Sh;
        match error {
            Sh::AuthenticationFailed => IpcError::AuthenticationFailed("Share authentication failed".into()),
            Sh::ContactNotFound | Sh::ShareNotFound | Sh::ReceivedShareNotFound
                => IpcError::NotFound("Share record not found".into()),
            Sh::ShareAlreadyRevoked | Sh::NoActiveSharesForRotation
                => IpcError::InvalidInput("Share cannot be revoked in its current state".into()),
            Sh::CloudOperation(_) | Sh::RevocationPartial { .. }
                => IpcError::CloudError("Cloud share operation failed".into()),
            Sh::MalformedSharePackage(_) | Sh::InvalidJsonPayload(_)
            | Sh::InvalidFileKeyLength(_) | Sh::InvalidSenderPublicKeyLength(_)
            | Sh::InvalidPublicKeyLength(_) | Sh::InvalidSharePackage
                => IpcError::InvalidInput("Share package is malformed".into()),
            Sh::EmptyDisplayName => IpcError::InvalidInput("Display name is required".into()),
            Sh::InvalidContactId(_) => IpcError::InvalidInput("Contact identifier is invalid".into()),
            Sh::ConstraintViolation(_) => IpcError::AlreadyExists("A sharing record already exists".into()),
            Sh::IdentityMissing | Sh::Backend(_) => IpcError::InternalError("An error occurred".into()),
            _ => IpcError::InternalError("An error occurred".into()),
        }
    }
}
```

**CS-005 — `From<storage::SyncError> for IpcError` + `From<CloudTransportError> for IpcError`** (per Concern 2)

```rust
impl From<crate::storage::SyncError> for IpcError {
    fn from(error: crate::storage::SyncError) -> Self {
        tracing::error!("sync error: {:?}", error);
        use crate::storage::SyncError as Sy;
        match error {
            Sy::Conflict(_) => IpcError::CloudError("Cloud snapshot conflict".into()),
            Sy::Transport { .. } | Sy::CloudManifestUnreadable { .. }
            | Sy::PushUploadFailed { .. } | Sy::PushManifestBackupFailed { .. }
            | Sy::VaultHeaderUploadFailed { .. } | Sy::PullIncomplete { .. }
                => IpcError::CloudError("Cloud operation failed".into()),
            Sy::Storage { source } => IpcError::from(source),
            Sy::Io(_) | Sy::ManifestBackup { .. } | Sy::RollbackFailed { .. }
                => IpcError::InternalError("An error occurred".into()),
            _ => IpcError::InternalError("An error occurred".into()),
        }
    }
}

impl From<crate::storage::CloudTransportError> for IpcError {
    fn from(error: crate::storage::CloudTransportError) -> Self {
        tracing::error!("cloud transport error: {:?}", error);
        use crate::storage::CloudTransportError as C;
        match error {
            C::NotFound => IpcError::NotFound("Cloud blob not found".into()),
            C::AuthenticationFailed => IpcError::CloudError("Cloud authentication failed".into()),
            C::Timeout | C::IoError(_) | C::RcloneProcessFailed { .. } | C::Other(_)
                => IpcError::CloudError("Cloud operation failed".into()),
        }
    }
}
```

**CS-006 — `AppState` struct** (`src-tauri/src/ui/state.rs`, per Concern 4)

```rust
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::auth::{DeviceMonitor, SessionManager};
use crate::storage::cloud::CloudTransport;
use crate::storage::SqlCipherMetadataStore;
use crate::ui::types::SyncStatus;

/// Shared application state injected into every Tauri command.
pub struct AppState {
    pub database: Arc<RwLock<Option<SqlCipherMetadataStore>>>,
    pub cloud_transport: Arc<dyn CloudTransport>,
    pub device_monitor: Arc<dyn DeviceMonitor>,
    pub session_manager: Arc<SessionManager>,
    pub sync_status: Arc<RwLock<SyncStatus>>,
}
```

**CS-007 — Input validators** (`src-tauri/src/ui/validation.rs`)

```rust
use std::sync::OnceLock;

use regex::Regex;
use uuid::Uuid;

use crate::ui::error::IpcError;

const VAULT_PATH_ALLOWLIST: &str = r"^[a-zA-Z0-9 ._/-]*$";
const MIN_CHUNK_SIZE_BYTES: u64 = 131_072;
const MAX_CHUNK_SIZE_BYTES: u64 = 67_108_864;

fn vault_path_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(VAULT_PATH_ALLOWLIST)
        .expect("vault path regex is a compile-time literal"))
}

pub(crate) fn validate_vault_path(path: &str) -> Result<(), IpcError> { /* 5 checks per design §Input Validation */ }
pub(crate) fn validate_file_id(id: &str) -> Result<(), IpcError> { /* UUID v4 parse + version check */ }
pub(crate) fn validate_password(password: &str) -> Result<(), IpcError> { /* non-empty */ }
pub(crate) fn validate_chunk_size(chunk_size_bytes: u64) -> Result<(), IpcError> { /* range check */ }
pub(crate) fn normalise_vault_path(path: &str) -> &str { /* treat "/" as empty root */ }
```

**CS-008 — Auth command signatures** (`src-tauri/src/ui/auth_commands.rs`, matches design §Command Signatures lines 109-177)

```rust
#[tauri::command]
pub async fn authenticate(
    password: String,
    key_file_path: Option<std::path::PathBuf>,
    state: tauri::State<'_, crate::ui::AppState>,
) -> Result<crate::ui::types::AuthResponse, crate::ui::IpcError>;

#[tauri::command]
pub async fn create_vault(
    vault_name: String,
    password: String,
    tier: u8,
    key_file_destination: Option<std::path::PathBuf>,
    primary_destination: crate::ui::types::DestinationSessionConfig,
    chunk_size_bytes: u64,
    epoch_buffer_enabled: bool,
    state: tauri::State<'_, crate::ui::AppState>,
) -> Result<crate::ui::types::AuthResponse, crate::ui::IpcError>;

#[tauri::command] pub async fn change_password(current_password: String, new_password: String, state: tauri::State<'_, crate::ui::AppState>) -> Result<(), crate::ui::IpcError>;
#[tauri::command] pub async fn rotate_key_file(new_key_file_destination: std::path::PathBuf, state: tauri::State<'_, crate::ui::AppState>) -> Result<(), crate::ui::IpcError>;
#[tauri::command] pub async fn delete_vault(confirmation: String, state: tauri::State<'_, crate::ui::AppState>) -> Result<(), crate::ui::IpcError>;
#[tauri::command] pub async fn lock_session(state: tauri::State<'_, crate::ui::AppState>) -> Result<(), crate::ui::IpcError>;
#[tauri::command] pub async fn get_session_status(state: tauri::State<'_, crate::ui::AppState>) -> Result<crate::ui::types::SessionStatus, crate::ui::IpcError>;
```

**CS-009 — File/Sync/Destination/Sharing command signatures**

All 23 remaining commands follow the signatures in design.md §Command Signatures lines 180-361 verbatim (types re-homed under `crate::ui::types::*` and `crate::ui::IpcError`). No duplication here — the design's §Canonical Command Surface (Normative) is authoritative for membership; signatures are reproduced in-file.

**CS-010 — Command registration** (`src-tauri/src/lib.rs`)

```rust
pub fn run() {
    if let Err(error) = tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(crate::ui::AppState::construct_default())
        .invoke_handler(tauri::generate_handler![
            // Auth (7)
            ui::authenticate, ui::create_vault, ui::change_password,
            ui::rotate_key_file, ui::delete_vault, ui::lock_session, ui::get_session_status,
            // Files (6)
            ui::list_directory, ui::upload_file, ui::download_file,
            ui::delete_file, ui::get_file_content, ui::list_remote,
            // Sync (5)
            ui::sync_to_cloud, ui::recover_from_cloud, ui::get_sync_status,
            ui::migrate_vault, ui::sync_backup,
            // Destinations (3)
            ui::add_destination, ui::list_destinations, ui::delete_destination,
            // Sharing (8)
            ui::export_public_key, ui::add_contact, ui::list_contacts,
            ui::share_file, ui::import_share, ui::revoke_share,
            ui::list_shares, ui::list_received_shares,
        ])
        .run(tauri::generate_context!())
    {
        eprintln!("error while running tauri application: {error}");
    }
}
```

**CS-011 — `build.rs` allowlist** (`src-tauri/build.rs`)

```rust
fn main() {
    tauri_build::try_build(
        tauri_build::Attributes::new().app_manifest(
            tauri_build::AppManifest::new().commands(&[
                "add_contact", "add_destination", "authenticate", "change_password",
                "create_vault", "delete_destination", "delete_file", "delete_vault",
                "download_file", "export_public_key", "get_file_content",
                "get_session_status", "get_sync_status", "import_share", "list_contacts",
                "list_destinations", "list_directory", "list_received_shares",
                "list_remote", "list_shares", "lock_session", "migrate_vault",
                "recover_from_cloud", "revoke_share", "rotate_key_file", "share_file",
                "sync_backup", "sync_to_cloud", "upload_file",
            ]),
        ),
    )
    .expect("failed to build tauri application");
}
```

**CS-012 — IPC response DTOs** (`src-tauri/src/ui/types/`)

One file per DTO — `auth_response.rs`, `session_status.rs`, `file_entry.rs`, `progress_update.rs`, `sync_progress_update.rs`, `sync_result.rs`, `sync_conflict.rs`, `sync_status.rs`, `file_content.rs`, `share_response.rs`, `import_share_response.rs`, `contact_entry.rs`, `share_entry.rs`, `received_share_entry.rs`, `migration_progress.rs`, `destination_session_config.rs`, `destination_entry.rs`, `remote_file_entry.rs` — each field-for-field matching design.md §IPC Response Types lines 603-831. `mod.rs` re-exports only.

### Step-by-step implementation order

1. **Scaffold** `src-tauri/src/ui/` module tree: create `error.rs`, `state.rs`, `validation.rs`, `auth_commands.rs`, `file_commands.rs`, `sync_commands.rs`, `destination_commands.rs`, `sharing_commands.rs`, and `types/` directory with one file per DTO (CS-012). Update `src-tauri/src/ui/mod.rs` to re-export only.
2. **Define DTOs** per CS-012. Each struct derives `Serialize` (or `Deserialize` for `DestinationSessionConfig`) with `#[serde(rename_all = "camelCase")]` where design specifies. Add one doc-comment per field.
3. **Define `IpcError` (CS-001)** in `error.rs`. Replace the empty `UiError`. Add serde shape test asserting `{"kind":"notFound","message":"…"}`.
4. **Implement `From` impls** in this order: `AuthenticationError` (CS-002), `StorageError` (CS-003), `SharingError` (CS-004), `SyncError` + `CloudTransportError` (CS-005). Each impl starts with `tracing::error!("… error: {:?}", error)` then matches exhaustively.
5. **Write sanitisation tests**: for each `From` impl, assert (a) no panic on any representative variant, (b) emitted `message` contains no substring from a fixture list (`C:\`, `/Users/`, `key`, hex-like `[0-9a-f]{32}` regex, the Debug string of a source error). Use `proptest` for the hex-key adversarial fixture.
6. **Implement validators (CS-007)** in `validation.rs`. Unit tests per design's Validation Checkpoint list (`../escape`, `/absolute/path`, `path\with\backslash`, `\x00`–`\x1F`, malformed UUIDs, empty password, chunk sizes `0 / 131_071 / 131_072 / 67_108_864 / 67_108_865 / u64::MAX`).
7. **Implement `AppState` (CS-006)** in `state.rs`. Add `AppState::construct_default()` that builds each field with the platform-specific device monitor (`#[cfg(target_os)]` branch), the default `RcloneTransport` (or a `MockCloudTransport` under `#[cfg(feature = "test-utils")]`), and `SessionManager::from_config()`.
8. **Scaffold each command** (CS-008, CS-009). Per command:
   - Destructure `String` password inputs into `let password_bytes = Zeroizing::new(password.into_bytes());`.
   - Validate inputs via `crate::ui::validation::*`, returning early on `InvalidInput`.
   - Where a direct backend call exists (`lock_session`, `get_session_status`, `list_contacts`, `list_destinations`, `export_public_key`, `list_shares`, `list_received_shares`), delegate and return via `?`.
   - Otherwise return `Err(IpcError::InternalError("command not yet wired".into()))` and add a `// TODO(phase-6.5): wire orchestration` anchor.
   - Commands with `tauri::ipc::Channel<T>`: accept the channel but do not emit.
9. **Register commands and allowlist**: update `src-tauri/src/lib.rs` (CS-010) and `src-tauri/build.rs` (CS-011). Delete the legacy `greet` command.
10. **Validate `tauri.conf.json`**: `withGlobalTauri: true` is already set; no change needed here (6.4 handles CSP).
11. **Run governance sync actions** (§8) — rule updates must be committed in the same changeset as the code.
12. **Run `/copilot-sync`** after rule edits.
13. **Invoke `security-reviewer` agent** per sub-phase Security Review: audit every `From` impl arm, catch-all hygiene, validator traversal coverage.
14. **Run the full validation checkpoint**: `cargo test --workspace --all-targets --all-features`, `cargo clippy -- -D warnings`, `cargo build` (compile-time command allowlist check).

## 6. Review focus areas

### 6a. Rust change surface

- `src-tauri/src/ui/mod.rs`
- `src-tauri/src/ui/error.rs`
- `src-tauri/src/ui/state.rs` (new)
- `src-tauri/src/ui/validation.rs` (new)
- `src-tauri/src/ui/auth_commands.rs` (new)
- `src-tauri/src/ui/file_commands.rs` (new)
- `src-tauri/src/ui/sync_commands.rs` (new)
- `src-tauri/src/ui/destination_commands.rs` (new)
- `src-tauri/src/ui/sharing_commands.rs` (new)
- `src-tauri/src/ui/types/mod.rs`
- `src-tauri/src/ui/types/*.rs` (18 new DTO files per CS-012)
- `src-tauri/src/lib.rs` (registration + AppState wiring; `greet` removed)
- `src-tauri/build.rs` (30-command allowlist)

### 6b. Security-sensitive paths

- `src-tauri/src/ui/error.rs` — **all** `From` impls are the sanitisation boundary. Verify: (i) every arm emits a fixed English string with no formatted source, (ii) `tracing::error!` uses `{:?}` Debug, never `{}` Display, on the wrapped error, (iii) catch-all arms return `IpcError::InternalError("An error occurred")` and never interpolate the original.
- `src-tauri/src/ui/validation.rs` — `validate_vault_path` must reject URL-encoded traversal (`%2E%2E`, `%2F..`) by virtue of the allowlist regex (the `%` character is not in `[a-zA-Z0-9 ._/-]`); verify no decoding happens before the regex check. Null bytes are rejected by `is_control()`.
- `src-tauri/src/ui/auth_commands.rs::authenticate`, `create_vault`, `change_password` — verify the `Zeroizing::new(password.into_bytes())` line is the **first** statement, before any `.await`. No `password.clone()` anywhere.
- `src-tauri/src/ui/state.rs` — verify `AppState` contains **no** key material, no password, no file-content buffer. The only owned fields are IPC/runtime orchestration handles.
- `src-tauri/build.rs` — verify the allowlist is the 30-command canonical set exactly; extras or omissions are Phase 6 contract violations.

### 6c. Architecture risk areas

- `src-tauri/src/ui/mod.rs` — must re-export only (rust rule). No `pub fn`, no types defined here.
- Module split discipline — no command body in `mod.rs` or `state.rs`. Each `#[tauri::command]` function lives in its domain file.
- `AppState` must depend on traits (`dyn CloudTransport`, `dyn DeviceMonitor`) not concrete impls where the rule is set. `SqlCipherMetadataStore` is concrete per Concern 4.
- DTO types live in `ui/types/*` and do not leak backend domain types across the IPC boundary. `DestinationSessionConfig` → `DestinationSession` conversion lives in `destination_commands.rs`, not in the DTO file.
- Dependency direction: `ui` depends on `auth`, `storage`, `sharing`; none of those depend on `ui`. Grep `use crate::ui::` from `auth/`, `storage/`, `sharing/`, `crypto/`, `memory/`, `sync/` to confirm zero hits.
- `From` impls in `ui::error` may only wrap; they must not contain crypto operations, file I/O, or network calls.

### 6d. Testing requirements

**From sub-phase Validation Checkpoint:**
- `cargo test ui::error` — covers auth/storage/sync mappings from design §Error Sanitisation with adversarial-fixture checks (no paths, key material, hex-like strings, or stack traces in any emitted `IpcError` message).
- `cargo test ui::validation` — per sub-phase:
  - `validate_vault_path` rejects `../escape`, `/absolute/path`, `path\with\backslash`, strings with `\x00`-`\x1F`, and URL-encoded traversal (`%2E%2E/foo`).
  - `validate_file_id` rejects non-UUID strings, non-v4 UUIDs (e.g. `f81d4fae-7dec-11d0-a765-00a0c91e6bf6` is v1), and empty string.
  - `validate_password` rejects empty string.
  - `validate_chunk_size` rejects `0`, `131_071`, `67_108_865`, `u64::MAX`; accepts `131_072`, `4_194_304`, `67_108_864`.
- `cargo build` — compile-time check that every command in `generate_handler!` matches the `build.rs` allowlist.

**Additional edge cases (from Step 2 review):**
- Serde shape test: each `IpcError` variant serialises to `{"kind": "<camelCase>", "message": "<string>"}`.
- `validate_vault_path("")` succeeds (root case) — tested.
- `validate_vault_path("/")` treated as root via `normalise_vault_path` — tested.
- `From<AuthenticationError::KeySource(KeySourceError::InvalidSize { actual: 31 })>` does not emit the number `31` in the IPC message.
- `From<StorageError::ConstraintViolation("UNIQUE constraint failed: nodes.node_id")>` does not emit the SQL text.
- `From<SharingError::InvalidContactId("00000000-0000-0000-0000-000000000000")>` does not emit the UUID.
- `AppState::construct_default()` compiles on all three target OS (Windows, Linux, macOS) with the correct `#[cfg]`-gated `DeviceMonitor` impl.
- `tauri::generate_handler![]` with all 30 commands compiles (asserts command signatures are Tauri-compatible: all `async`, `state` last, `Result<T, IpcError>`).

**Proptest fixtures:**
- 10,000 random source-error Debug strings fed through each `From` impl — verify no 16+ hex-character substring appears in the emitted `IpcError.message`.

## 7. Documentation impact

- **Required this run:**
  - `.claude/rules/tauri.md` — update per GS-005 (real `StorageError` variant set), GS-006 (`storage::SyncError` + `CloudTransportError` replace stale `sync::SyncError`), GS-008 (`AppState.database: Arc<RwLock<Option<SqlCipherMetadataStore>>>`).
  - `.claude/rules/sharing.md` — append GS-007 IPC error-hygiene note.
  - `.claude/rules/auth.md` — no change (session backoff deferred to 6.4).
  - `.github/instructions/*.md` — regenerated via `/copilot-sync` after rule edits.
- **Deferred/optional:**
  - `docs/architecture/designs/tauri-ipc-and-frontend/design.md` §Error Sanitisation code block (lines 521-577) — does not match real `StorageError`/`sync` types. **Rationale for deferral:** the code block is marked "Illustrative" and the canonical contract is in §Contract Surface / §Error Sanitisation rules (not the code snippet). The snippet is a drift debt worth fixing but not load-bearing for Phase 6.1 correctness. Tracked for a `/review-design` follow-up sweep after 6.4.
  - `docs/architecture-decisions/011-ipc-error-sanitisation.md` — referenced in the sub-phase roadmap line 174. **Rationale for deferral:** ADR drafting is a dedicated task; 6.1 implementation ships without it and logs a note for a later `docs-sync` pass. An ADR placeholder can be added if the security-reviewer requests it.
  - `docs/architecture/designs/tauri-ipc-and-frontend/design.md` §Application State `DatabaseConnection` → `SqlCipherMetadataStore` correction. **Rationale for deferral:** same as above; tracked for the design sweep.

## 8. Governance sync actions (pre-implementation)

| Action ID | Reason / linked concern | Target files | Required edit | Verification |
|---|---|---|---|---|
| GS-005 | Concern 1 — `StorageError` variant set drift | `.claude/rules/tauri.md` | Append to the "IPC / UI layer" bullet list: "`From<StorageError>` maps the real variants (`Database`, `NotFound`, `ChecksumMismatch`, `Io`, `WrongKey`, `ConstraintViolation`); design §Error Sanitisation code block is illustrative and out of date." | `cargo test ui::error` after implementation; visual diff against the rule. |
| GS-006 | Concern 2 — `sync::SyncError` location | `.claude/rules/tauri.md` | Replace the stale `sync::SyncError` reference (if present) with "`storage::SyncError` (re-export of `storage::cloud::sync::SyncError`) and `storage::CloudTransportError` are the canonical cloud-error inputs for IPC sanitisation." | Grep for `sync::SyncError` returns zero hits in `.claude/rules/`. |
| GS-007 | Concern 3 — sharing IPC mapping | `.claude/rules/sharing.md` | Append under "HPKE error hygiene": "`SharingError::AuthenticationFailed` maps to `IpcError::AuthenticationFailed` with a fixed user-safe string; `ipc` adapters must never include KEM/CTX context bytes in the user-facing message." | Grep confirms the line is added; `cargo test ui::error` passes. |
| GS-008 | Concern 4 — `DatabaseConnection` correction | `.claude/rules/tauri.md` | Under "IPC / UI layer": add "`AppState.database` is `Arc<RwLock<Option<SqlCipherMetadataStore>>>`; there is no separate `DatabaseConnection` type." | `cargo build` succeeds with the declared field type. |
| GS-009 | Rule refresh after edits | `.github/instructions/*.md` | Run `/copilot-sync` so Copilot instruction mirrors match the rule edits above. | `git diff .github/instructions/` matches the corresponding `.claude/rules/` changes. |

Run order: GS-005 → GS-006 → GS-007 → GS-008 → implement code → GS-009 (`/copilot-sync`).

## 9. Handoff Notes for Implementer

Working directory: `C:\Users\chris\source\repos\arx-runa\`. Start by running the four governance-sync actions (GS-005..GS-008) so rule edits land in the same changeset as the code. Then scaffold `src-tauri/src/ui/*` per §5 Steps 1-10, drop the legacy `greet` command in `src-tauri/src/lib.rs`, and register the 30 canonical commands in both `generate_handler!` and `build.rs`. The plan is largely self-contained, but before writing the `create_vault` / `add_destination` command bodies, re-read design.md §Canonical Command Surface (Normative) to confirm no command has been added since 2026-04-12. Traps: (i) `String::into_bytes()` must be the **first** line of every password-bearing command body — no `password.clone()`, no logging of the password slot; (ii) `generate_handler!` requires `pub use auth_commands::authenticate;` etc. in `ui/mod.rs` so the `ui::authenticate` path resolves — otherwise `cargo build` will fail at the macro with cryptic errors; (iii) cross-platform `DeviceMonitor` selection in `AppState::construct_default()` must be `cfg!`-gated for Windows/Linux/macOS and fall through to `MockDeviceMonitor` only under `#[cfg(any(test, feature = "test-utils"))]` (never in production runs); (iv) all 30 commands must be in `build.rs` allowlist — missing any causes a runtime capability denial on first invocation, not a compile error. After implementation, run `cargo test --workspace --all-targets --all-features`, `cargo clippy -- -D warnings`, `cargo build`, then invoke the `security-reviewer` agent per sub-phase §Security Review. The full command-orchestration wiring (real `authenticate`/`upload_file`/`sync_to_cloud` bodies, progress-channel emission, brute-force backoff) is explicitly deferred to a follow-up plan after Phase 6.2-6.4 stabilise the frontend binding; leave the `TODO(phase-6.5)` anchors in place.
