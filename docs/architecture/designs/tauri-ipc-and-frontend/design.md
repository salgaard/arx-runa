# Arx Runa — Tauri IPC Layer and Frontend Design

> Status: Reviewed. Implementation target: Phase 6.
> Last updated: 2026-04-12

### Review Log

| Date | Reviewer | Outcome |
|------|----------|---------|
| 2026-03-31 | Interactive design session | Critical fixes applied (password zeroization, polling cleanup), recommendations added |
| 2026-04-12 | Cross-phase consistency remediation | IPC/password/path/viewing/share terminology and auth-backoff alignment fixes applied |

---

## Goals

- Expose backend functionality (auth, storage, sync, sharing) to the frontend through Tauri commands with proper error sanitisation
- Build a minimal but functional Leptos UI for authentication, vault browsing, upload, download, and session management
- Enforce Zero-Trace principles: no localStorage, no persistent state, clear UI on lock
- Keep the IPC surface minimal and auditable — defense in depth via build.rs allowlist + capabilities
- Stream large file transfer progress in real-time without blocking the UI

---

## Scope Profiles

Phase 6 scope is documented with two profiles:

- **Full profile (canonical)**: complete IPC command surface and frontend scope. The single source of truth for command membership is [Canonical Command Surface (Normative)](#canonical-command-surface-normative).
- **MVP profile (optional)**: delivery slicing for UI breadth (for example, [Supported types (MVP)](#supported-types-mvp) in the file-viewing section). MVP framing may defer UI capabilities, but it must not add or remove commands from the canonical full profile.

---

## How to Read This Design

- **Normative** sections define required contract and scope.
- **Illustrative** code blocks keep detailed implementation cues and examples; if they diverge, follow the normative sections.
- Command inclusion/exclusion is governed only by [Canonical Command Surface (Normative)](#canonical-command-surface-normative).

---

## Contract Surface

### Interface contract

- Command surface is the registered async Tauri command set across `auth_commands`, `file_commands`, `sync_commands`, `destination_commands`, and `sharing_commands`, as defined in [Canonical Command Surface (Normative)](#canonical-command-surface-normative).
- Long-running command contracts stream progress through `tauri::ipc::Channel<T>` (`upload_file`, `download_file`, `sync_to_cloud`, `migrate_vault`, `sync_backup`).
- Build-time exposure contract is the `src-tauri/build.rs` `AppManifest::commands(...)` allowlist.

### Data contract

- Error payload contract is `IpcError` with sanitised, user-safe messages only.
- Response payload contract is `src-tauri/src/ui/types.rs` (`AuthResponse`, `SessionStatus`, `FileEntry`, `SyncResult`, `DestinationEntry`, `RemoteFileEntry`, and related progress types).
- Shared runtime state contract is `AppState { database, cloud_transport, device_monitor, session_manager, sync_status }` with key material excluded.

### Invariant contract

- IPC responses must never expose key material, passwords, stack traces, or unsanitised internal filesystem details.
- Tauri/UI state follows Zero-Trace: sensitive state is memory-only and frontend contexts are cleared on lock/timeout.
- Command handlers remain async and all domain errors are mapped through explicit sanitisation boundaries.
- Cross-phase invariant reference: [docs/architecture/design-invariants.md](../../design-invariants.md).

### Dependency contract

- UI command handlers depend on `auth`, `storage`, `sync`, and sharing-domain modules via typed `From` mappings into `IpcError`.
- Runtime wiring depends on `CloudTransport`, `DeviceMonitor`, `SessionManager`, and SQLCipher access through `AppState`.
- Security boundary depends on capability files, build.rs allowlisting, and CSP configuration.

---

## IPC Layer Architecture

### Command Organisation

Commands are organised into domain-grouped submodules under `src-tauri/src/ui/`, with all commands re-exported and registered in a single `invoke_handler` call.

```
src-tauri/src/ui/
├── mod.rs                 # Re-exports, invoke_handler registration
├── error.rs               # IpcError enum, From impls
├── auth_commands.rs       # Auth domain commands (see canonical command surface)
├── file_commands.rs       # File domain commands (see canonical command surface)
├── sync_commands.rs       # Sync domain commands (see canonical command surface)
├── destination_commands.rs # Destination domain commands (see canonical command surface)
├── sharing_commands.rs    # Sharing domain commands (see canonical command surface)
└── types.rs               # IPC-specific types (responses, progress updates)
```

### Canonical Command Surface (Normative)

This is the **only canonical command list** for Phase 6.

| Domain | Commands |
|--------|----------|
| Auth | `authenticate`, `create_vault`, `change_password`, `rotate_key_file`, `delete_vault`, `lock_session`, `get_session_status` |
| File | `list_directory`, `upload_file`, `download_file`, `delete_file`, `get_file_content`, `list_remote` |
| Sync | `sync_to_cloud`, `recover_from_cloud`, `get_sync_status`, `migrate_vault`, `sync_backup` |
| Destination | `add_destination`, `list_destinations`, `delete_destination` |
| Sharing | `export_public_key`, `add_contact`, `list_contacts`, `share_file`, `import_share`, `revoke_share`, `list_shares`, `list_received_shares` |

All other command enumerations in this document are detailed implementation mirrors and must stay aligned with this section.

### Command Signatures

> **Code block role: Illustrative**
>  
> The detailed signatures below show expected Rust command contracts. Command membership remains authoritative only in [Canonical Command Surface (Normative)](#canonical-command-surface-normative).

```rust
// --- auth_commands.rs ---

/// Authenticate with password (Tier 1) or password + USB key file (Tier 2).
/// Returns vault_id on success. Does NOT return any key material.
#[tauri::command]
async fn authenticate(
    password: String, // Immediately convert to Zeroizing<Vec<u8>>, scrub String bytes, then drop
    key_file_path: Option<PathBuf>,  // None for Tier 1 vaults
    state: tauri::State<'_, AppState>,
) -> Result<AuthResponse, IpcError>;

/// Zero all session keys and lock the vault.
#[tauri::command]
async fn lock_session(
    state: tauri::State<'_, AppState>,
) -> Result<(), IpcError>;

/// Check if vault is unlocked. Returns status only, no sensitive data.
#[tauri::command]
async fn get_session_status(
    state: tauri::State<'_, AppState>,
) -> Result<SessionStatus, IpcError>;

/// Create a new vault. For Tier 2, generates a key file at the destination path.
///
/// `chunk_size_bytes` must be in [131072, 67108864] (128 KiB–64 MiB) and is
/// immutable after creation — changing it requires re-encrypting every blob.
/// The UI layer should pre-fill 4194304 (4 MiB) as the default before invoking
/// this command.
///
/// `epoch_buffer_enabled` selects opt-in hybrid routing.
/// When true: files < chunk_size_bytes are staged and packed; files >=
/// chunk_size_bytes upload immediately via the standalone chunk path (including
/// padded trailing partial chunks). When false: all files upload immediately.
#[tauri::command]
async fn create_vault(
    vault_name: String,
    password: String, // Immediately convert to Zeroizing<Vec<u8>>, scrub String bytes, then drop
    tier: u8,
    key_file_destination: Option<PathBuf>,  // Required for Tier 2
    primary_destination: DestinationSessionConfig,  // Primary cloud/local destination
    chunk_size_bytes: u64,          // 131072–67108864; immutable after creation
    epoch_buffer_enabled: bool,     // pack small files before upload; off by default
    state: tauri::State<'_, AppState>,
) -> Result<AuthResponse, IpcError>;

/// Change the vault password. Requires an active session.
/// For Tier 2, the USB key file must be present.
#[tauri::command]
async fn change_password(
    current_password: String, // Immediately convert to Zeroizing<Vec<u8>>, scrub String bytes, then drop
    new_password: String,     // Immediately convert to Zeroizing<Vec<u8>>, scrub String bytes, then drop
    state: tauri::State<'_, AppState>,
) -> Result<(), IpcError>;

/// Rotate the USB key file. Tier 2 only. Generates a new key file.
#[tauri::command]
async fn rotate_key_file(
    new_key_file_destination: PathBuf,
    state: tauri::State<'_, AppState>,
) -> Result<(), IpcError>;

/// Delete the vault permanently. Requires typing the vault name as confirmation.
#[tauri::command]
async fn delete_vault(
    confirmation: String,
    state: tauri::State<'_, AppState>,
) -> Result<(), IpcError>;

// --- file_commands.rs ---

/// List contents of a directory in the vault.
/// Returns decrypted file/folder names and metadata (size, modified date).
#[tauri::command]
async fn list_directory(
    path: String,
    state: tauri::State<'_, AppState>,
) -> Result<Vec<FileEntry>, IpcError>;

/// Encrypt and upload a file. Progress streamed via channel.
#[tauri::command]
async fn upload_file(
    source_path: PathBuf,
    vault_path: String,
    progress: tauri::ipc::Channel<ProgressUpdate>,
    state: tauri::State<'_, AppState>,
) -> Result<FileEntry, IpcError>;

/// Download and decrypt a file. Progress streamed via channel.
#[tauri::command]
async fn download_file(
    file_id: String,
    destination_path: PathBuf,
    progress: tauri::ipc::Channel<ProgressUpdate>,
    state: tauri::State<'_, AppState>,
) -> Result<(), IpcError>;

/// Delete a file from the vault and cloud.
#[tauri::command]
async fn delete_file(
    file_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<(), IpcError>;

/// Decrypt and return file content for in-app viewing (Zero-Trace).
/// Returns base64-encoded content for files up to 50 MiB.
/// Files above 50 MiB return InvalidInput:
/// "File too large for in-app viewing; use download instead".
#[tauri::command]
async fn get_file_content(
    file_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<FileContent, IpcError>;

// --- sync_commands.rs ---

/// Push local changes to cloud.
#[tauri::command]
async fn sync_to_cloud(
    progress: tauri::ipc::Channel<SyncProgressUpdate>,
    state: tauri::State<'_, AppState>,
) -> Result<SyncResult, IpcError>;

/// Recover vault from cloud on a new device.
#[tauri::command]
async fn recover_from_cloud(
    vault_header_path: PathBuf,
    progress: tauri::ipc::Channel<SyncProgressUpdate>,
    state: tauri::State<'_, AppState>,
) -> Result<(), IpcError>;

/// Check current sync status.
#[tauri::command]
async fn get_sync_status(
    state: tauri::State<'_, AppState>,
) -> Result<SyncStatus, IpcError>;

/// Migrate vault blobs from current cloud remote to a new one.
/// No re-encryption required — blobs are opaque ciphertext.
#[tauri::command]
async fn migrate_vault(
    new_destination_id: String,
    progress: tauri::ipc::Channel<MigrationProgress>,
    state: tauri::State<'_, AppState>,
) -> Result<(), IpcError>;

/// Sync the primary destination to one or more backup destinations.
/// `destination_id` identifies which backup destination to sync to;
/// if None, syncs to all configured backup destinations.
#[tauri::command]
async fn sync_backup(
    destination_id: Option<String>,
    progress: tauri::ipc::Channel<SyncProgressUpdate>,
    state: tauri::State<'_, AppState>,
) -> Result<SyncResult, IpcError>;

// --- destination_commands.rs ---

/// Add a new destination session (primary or backup) to the vault.
/// Credentials are encrypted and stored in SQLCipher.
/// Validates the destination by issuing an `rclone lsd` probe before saving.
#[tauri::command]
async fn add_destination(
    config: DestinationSessionConfig,
    state: tauri::State<'_, AppState>,
) -> Result<DestinationEntry, IpcError>;

/// List all configured destination sessions for the current vault.
/// Returns metadata only — no credential material.
#[tauri::command]
async fn list_destinations(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<DestinationEntry>, IpcError>;

/// Delete a destination session from the vault.
/// The primary destination cannot be deleted while backup destinations remain.
#[tauri::command]
async fn delete_destination(
    destination_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<(), IpcError>;

// --- file_commands.rs (addition) ---

/// List files on the primary remote destination, resolved via the local manifest.
/// Vault must be unlocked. Returns a manifest-linked view (real filenames).
/// Blobs not present in the manifest are returned as orphaned entries.
#[tauri::command]
async fn list_remote(
    remote_prefix: String,  // e.g. "vault/" or a sub-path; empty = root
    state: tauri::State<'_, AppState>,
) -> Result<Vec<RemoteFileEntry>, IpcError>;

// --- sharing_commands.rs ---

/// Export the user's X25519 public key to a file for out-of-band exchange.
#[tauri::command]
async fn export_public_key(
    destination_path: PathBuf,
    state: tauri::State<'_, AppState>,
) -> Result<(), IpcError>;

/// Import a contact's public key from a file.
#[tauri::command]
async fn add_contact(
    display_name: String,
    public_key_path: PathBuf,
    email: Option<String>,
    state: tauri::State<'_, AppState>,
) -> Result<ContactEntry, IpcError>;

/// List all contacts.
#[tauri::command]
async fn list_contacts(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<ContactEntry>, IpcError>;

/// Share a file with a contact. Creates an HPKE (RFC 9180) share package.
#[tauri::command]
async fn share_file(
    file_id: String,
    contact_id: String,
    expiration_days: Option<u32>,
    state: tauri::State<'_, AppState>,
) -> Result<ShareResponse, IpcError>;

/// Import a received share package.
#[tauri::command]
async fn import_share(
    share_package_path: PathBuf,
    state: tauri::State<'_, AppState>,
) -> Result<ImportShareResponse, IpcError>;

/// Revoke a previously shared file.
#[tauri::command]
async fn revoke_share(
    share_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<(), IpcError>;

/// List outgoing shares.
#[tauri::command]
async fn list_shares(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<ShareEntry>, IpcError>;

/// List received shares.
#[tauri::command]
async fn list_received_shares(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<ReceivedShareEntry>, IpcError>;
```

### Command Registration

> **Code block role: Illustrative**
>  
> These wiring examples show how commands are registered and allowlisted. Use [Canonical Command Surface (Normative)](#canonical-command-surface-normative) as the source of truth for the command set.

All commands registered in `src-tauri/src/lib.rs`:

```rust
pub fn run() {
    tauri::Builder::default()
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            // Auth
            ui::authenticate,
            ui::create_vault,
            ui::change_password,
            ui::rotate_key_file,
            ui::delete_vault,
            ui::lock_session,
            ui::get_session_status,
            // Files
            ui::list_directory,
            ui::upload_file,
            ui::download_file,
            ui::delete_file,
            ui::get_file_content,
            ui::list_remote,
            // Sync
            ui::sync_to_cloud,
            ui::recover_from_cloud,
            ui::get_sync_status,
            ui::migrate_vault,
            ui::sync_backup,
            // Destinations
            ui::add_destination,
            ui::list_destinations,
            ui::delete_destination,
            // Sharing
            ui::export_public_key,
            ui::add_contact,
            ui::list_contacts,
            ui::share_file,
            ui::import_share,
            ui::revoke_share,
            ui::list_shares,
            ui::list_received_shares,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

Build-time allowlist in `src-tauri/build.rs`:

```rust
fn main() {
    tauri_build::try_build(
        tauri_build::Attributes::new()
            .app_manifest(
                tauri_build::AppManifest::new()
                    .commands(&[
                        "add_contact",
                        "add_destination",
                        "authenticate",
                        "change_password",
                        "create_vault",
                        "delete_destination",
                        "delete_file",
                        "delete_vault",
                        "download_file",
                        "export_public_key",
                        "get_file_content",
                        "get_session_status",
                        "get_sync_status",
                        "import_share",
                        "list_contacts",
                        "list_destinations",
                        "list_directory",
                        "list_received_shares",
                        "list_remote",
                        "list_shares",
                        "lock_session",
                        "migrate_vault",
                        "recover_from_cloud",
                        "revoke_share",
                        "rotate_key_file",
                        "share_file",
                        "sync_backup",
                        "sync_to_cloud",
                        "upload_file",
                    ])
            ),
    )
    .expect("failed to build tauri application");
}
```

---

## Out of Scope (MVP)

- **Multi-vault support**: Current design manages one vault per Arx Runa instance. Each device holds one vault in a single AppState. Multi-vault support (UI switcher, session-per-vault coordination, multi-device sync) is deferred to Phase 7+.

- **Advanced recovery flows**: Commands `recover_from_cloud`, `migrate_vault`, and `sync_backup` are registered in the IPC contract but do not have UI consumers in Phase 6. These are infrastructure-ready for Phase 7 implementation.

- **In-app file viewer**: The `get_file_content` command is available for retrieving file contents to display in-app (with 50 MiB cap). Media preview and document rendering are future enhancements.

---

## Category C: Architectural Decisions (Finalized)

These decisions are intentional MVP scope limitations that will persist through Phase 6. Phase 7+ planning may reconsider them with explicit research.

| Decision | Status | Rationale | Notes |
|----------|--------|-----------|-------|
| **c-multi-vault-support** — Single vault per device | ✅ Finalized | Current `AppState` design holds one vault. Multi-vault support requires per-vault session coordination, UI vault switcher, and `AppState` refactor. Deferred to Phase 7+ as architectural extension. | Documented in [Deferred Items Inventory](../../deferred-items-inventory.md) Category C and Category H |
| **c-inapp-file-viewer** — Backend ready, UI deferred | ✅ Finalized | `get_file_content` command is fully implemented with 50 MiB cap. Tauri sidecar infrastructure ready; in-app viewer UI (text, image, PDF rendering) is Phase 6.8+ feature. Backend can add viewers without command changes. | Documented in [Deferred Items Inventory](../../deferred-items-inventory.md) Category D |

**Phase 7+ Planning**: See [Deferred Items Inventory](../../deferred-items-inventory.md) §Category H for multi-vault dependency graph and priority roadmap.

---

## Error Sanitisation

### IpcError Enum

```rust
// src-tauri/src/ui/error.rs

use serde::Serialize;

/// Errors returned to the frontend. User-safe messages only.
#[derive(Debug, Serialize)]
#[serde(tag = "kind", content = "message")]
#[serde(rename_all = "camelCase")]
pub enum IpcError {
    /// Vault is locked, operation requires authentication.
    VaultLocked(String),
    
    /// Authentication failed (wrong password or key file).
    AuthenticationFailed(String),
    
    /// File or directory not found in vault.
    NotFound(String),
    
    /// File already exists at destination.
    AlreadyExists(String),
    
    /// Cloud operation failed (upload, download, sync).
    CloudError(String),
    
    /// Input validation failed.
    InvalidInput(String),
    
    /// Internal error. Details logged server-side only.
    InternalError(String),
}

impl std::fmt::Display for IpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::VaultLocked(msg) => write!(f, "Vault locked: {}", msg),
            Self::AuthenticationFailed(msg) => write!(f, "Authentication failed: {}", msg),
            Self::NotFound(msg) => write!(f, "Not found: {}", msg),
            Self::AlreadyExists(msg) => write!(f, "Already exists: {}", msg),
            Self::CloudError(msg) => write!(f, "Cloud error: {}", msg),
            Self::InvalidInput(msg) => write!(f, "Invalid input: {}", msg),
            Self::InternalError(msg) => write!(f, "Internal error: {}", msg),
        }
    }
}

impl std::error::Error for IpcError {}
```

### Error Mapping via From Traits

```rust
// Explicit mappings from domain errors — never expose internals

impl From<auth::AuthenticationError> for IpcError {
    fn from(err: auth::AuthenticationError) -> Self {
        // Log full error for debugging
        tracing::error!("Auth error: {:?}", err);
        
        match err {
            auth::AuthenticationError::InvalidCredentials => {
                IpcError::AuthenticationFailed("Invalid credentials".into())
            }
            auth::AuthenticationError::KeyFileNotFound => {
                IpcError::AuthenticationFailed("Key file not found".into())
            }
            auth::AuthenticationError::MemoryLockFailed(ref msg) => {
                tracing::error!("Memory lock failure: {}", msg);
                IpcError::InternalError("Cannot lock memory for session keys".into())
            }
            auth::AuthenticationError::VaultHeaderInvalid => {
                IpcError::InternalError("Vault configuration error".into())
            }
        }
    }
}

impl From<storage::StorageError> for IpcError {
    fn from(err: storage::StorageError) -> Self {
        tracing::error!("Storage error: {:?}", err);
        
        match err {
            storage::StorageError::FileNotFound { .. } => {
                IpcError::NotFound("File not found".into())
            }
            storage::StorageError::DirectoryNotFound { .. } => {
                IpcError::NotFound("Directory not found".into())
            }
            storage::StorageError::AlreadyExists { .. } => {
                IpcError::AlreadyExists("A file with this name already exists".into())
            }
            _ => IpcError::InternalError("An error occurred".into()),
        }
    }
}

impl From<sync::SyncError> for IpcError {
    fn from(err: sync::SyncError) -> Self {
        tracing::error!("Sync error: {:?}", err);
        
        match err {
            sync::SyncError::NetworkUnavailable => {
                IpcError::CloudError("Network unavailable".into())
            }
            sync::SyncError::CloudProviderError(_) => {
                IpcError::CloudError("Cloud provider error".into())
            }
            _ => IpcError::InternalError("An error occurred".into()),
        }
    }
}
```

### Sanitisation Rules

| Internal detail | Sanitised to |
|-----------------|--------------|
| Sensitive internal filesystem paths (`/Users/chris/...`, `C:\...`) in errors | "File not found" or generic |
| Key derivation parameters | Never exposed |
| Memory addresses | Never exposed |
| Stack traces | Logged server-side only |
| Specific crypto errors | "An error occurred" |

User-selected output paths may be returned only in explicit success payloads where the command contract requires them; sanitisation rules apply to error text and internal diagnostics.

---

## IPC Response Types

```rust
// src-tauri/src/ui/types.rs

use serde::Serialize;

/// Response from successful authentication.
#[derive(Serialize)]
pub struct AuthResponse {
    /// Opaque vault identifier (not a key).
    pub vault_id: String,
    /// Human-readable vault name.
    pub vault_name: String,
}

/// Current session status.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionStatus {
    /// Whether the vault is currently unlocked.
    pub is_unlocked: bool,
    /// Vault ID if unlocked, None otherwise.
    pub vault_id: Option<String>,
    /// Seconds until session timeout, None if locked.
    pub timeout_seconds: Option<u64>,
}

/// A file or directory entry in the vault.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileEntry {
    /// Unique file identifier.
    pub id: String,
    /// File or directory name (decrypted).
    pub name: String,
    /// "file" or "directory".
    pub entry_type: String,
    /// Size in bytes (0 for directories).
    pub size_bytes: u64,
    /// Last modified timestamp (ISO 8601).
    pub modified_at: String,
    /// Parent directory ID, None for root.
    pub parent_id: Option<String>,
}

/// Progress update for file operations.
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ProgressUpdate {
    /// 0-100 percentage complete.
    pub percent: u8,
    /// Bytes processed so far.
    pub bytes_processed: u64,
    /// Total bytes to process.
    pub bytes_total: u64,
    /// Current operation description.
    pub status: String,
}

/// Sync operation progress.
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SyncProgressUpdate {
    /// Overall progress 0-100.
    pub percent: u8,
    /// Current file being synced.
    pub current_file: Option<String>,
    /// Files processed / total.
    pub files_processed: u32,
    pub files_total: u32,
}

/// Result of a sync operation.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncResult {
    /// Number of files uploaded.
    pub files_uploaded: u32,
    /// Number of files downloaded.
    pub files_downloaded: u32,
    /// Number of files deleted from cloud.
    pub files_deleted: u32,
    /// Any conflicts requiring user attention.
    pub conflicts: Vec<SyncConflict>,
}

/// A sync conflict requiring resolution.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncConflict {
    pub file_name: String,
    pub local_modified: String,
    pub cloud_modified: String,
}

/// Current sync status.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncStatus {
    /// Whether a sync is in progress.
    pub syncing: bool,
    /// Last successful sync timestamp.
    pub last_synced_at: Option<String>,
    /// Pending changes count.
    pub pending_changes: u32,
}

/// Content returned for in-app file viewing.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileContent {
    /// MIME type (e.g., "image/jpeg", "text/plain").
    pub mime_type: String,
    /// Base64-encoded file content.
    pub data_base64: String,
    /// Original file size in bytes.
    pub size_bytes: u64,
}

/// Response from sharing a file.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShareResponse {
    /// Unique share identifier.
    pub share_id: String,
    /// Path to the exported share package file.
    pub package_path: String,
}

/// Response from importing a share.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportShareResponse {
    /// Share identifier from the package.
    pub share_id: String,
    /// File name from the share.
    pub file_name: String,
    /// Sender's display name (if contact is known).
    pub sender_name: Option<String>,
}

/// A contact entry.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContactEntry {
    pub contact_id: String,
    pub display_name: String,
    pub email: Option<String>,
    pub created_at: String,
}

/// An outgoing share entry.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShareEntry {
    pub share_id: String,
    pub file_name: String,
    pub contact_name: String,
    pub created_at: String,
    pub revoked: bool,
}

/// A received share entry.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReceivedShareEntry {
    pub share_id: String,
    pub file_name: String,
    pub sender_name: Option<String>,
    pub imported_at: String,
}

/// Migration progress update.
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct MigrationProgress {
    pub percent: u8,
    pub blobs_transferred: u32,
    pub blobs_total: u32,
    pub current_phase: String,
}

/// Configuration for adding or updating a destination session.
/// Credentials in `rclone_config_blob` are encrypted into SQLCipher on save.
/// Legacy inline destination config is retired; use this type for vault creation and destination management.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DestinationSessionConfig {
    /// Human-readable label (e.g., "My Backblaze B2").
    pub label: String,
    /// Destination type: "cloud", "external_drive", or "local_path".
    pub destination_type: String,
    /// Non-sensitive endpoint metadata: remote name, bucket, region, endpoint URL.
    /// Stored in cloud-config.json for new-device bootstrap (no credentials here).
    pub provider: String,
    pub bucket: String,
    pub region: String,
    pub endpoint: String,
    pub path_prefix: String,
    /// Rclone config section for this remote (includes credentials).
    /// Encrypted into SQLCipher on save; never written to disk in plaintext.
    pub rclone_config_blob: String,
    /// True if this is the primary destination for uploads.
    pub is_primary: bool,
    /// Backup sync mode: "mirror" or "accumulating". None for primary destinations.
    pub backup_mode: Option<String>,
}

/// Metadata returned when listing or adding destinations.
/// Never includes credential material.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DestinationEntry {
    pub destination_id: String,
    pub label: String,
    pub destination_type: String,
    pub provider: String,
    pub bucket: String,
    pub is_primary: bool,
    pub backup_mode: Option<String>,
}

/// A file entry returned by list_remote, linked to the local manifest.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteFileEntry {
    pub blob_id: String,
    /// Resolved filename from the manifest, if present.
    pub file_name: Option<String>,
    /// Resolved vault path from the manifest, if present.
    pub vault_path: Option<String>,
    /// Blob size in bytes as reported by rclone lsjson.
    pub size_bytes: u64,
    /// True if the blob UUID has no matching manifest entry.
    pub is_orphaned: bool,
}
```

---

## Application State

### What goes in `tauri::State<T>`

```rust
// src-tauri/src/ui/mod.rs

use std::sync::Arc;
use tokio::sync::RwLock;

/// Application state managed by Tauri.
/// 
/// This struct is accessible from all command handlers via State<AppState>.
/// NEVER store key material here — session keys live in mlocked memory
/// accessed via SessionManager.
pub struct AppState {
    /// Database connection pool (SQLCipher).
    pub database: Arc<RwLock<Option<DatabaseConnection>>>,
    
    /// Cloud transport for Rclone operations.
    pub cloud_transport: Arc<dyn CloudTransport>,
    
    /// Device monitor for USB key file detection.
    pub device_monitor: Arc<dyn DeviceMonitor>,
    
    /// Session manager — controls access to session keys.
    /// Keys themselves are in mlocked memory, not in this struct.
    pub session_manager: Arc<SessionManager>,
    
    /// Sync status tracker.
    pub sync_status: Arc<RwLock<SyncStatus>>,
}
```

### What does NOT go in `tauri::State<T>`

- `master_key`, `key_encryption_key`, `sqlcipher_key`, `manifest_key`
- `file_key` for any file
- Decrypted file contents
- Password or key file bytes

These live in `SessionManager` which controls mlocked memory regions.

---

## Input Validation

All Tauri command inputs are validated before processing:

```rust
/// Validates a vault path (no traversal, valid characters, forward slashes only).
fn validate_vault_path(path: &str) -> Result<(), IpcError> {
    // Reject backslashes. Vault-relative paths use '/' on all platforms.
    if path.contains('\\') {
        return Err(IpcError::InvalidInput("Use forward slashes in paths".into()));
    }

    // Allowlist: only expected filename/path characters.
    let allowed = regex::Regex::new(r"^[a-zA-Z0-9 ._/-]*$")
        .map_err(|_| IpcError::InternalError("Path validator misconfigured".into()))?;
    if !allowed.is_match(path) {
        return Err(IpcError::InvalidInput("Invalid characters in path".into()));
    }

    // Reject absolute paths.
    if path.starts_with('/') {
        return Err(IpcError::InvalidInput("Path must be relative".into()));
    }

    // Reject traversal segments.
    if path.split('/').any(|segment| segment == "..") {
        return Err(IpcError::InvalidInput("Invalid path".into()));
    }

    // Reject control characters.
    if path.chars().any(|c| c.is_control()) {
        return Err(IpcError::InvalidInput("Invalid characters in path".into()));
    }

    Ok(())
}

/// Validates a file ID format (UUID v4).
fn validate_file_id(id: &str) -> Result<(), IpcError> {
    uuid::Uuid::parse_str(id)
        .map(|_| ())
        .map_err(|_| IpcError::InvalidInput("Invalid file ID".into()))
}

/// Validates password meets minimum requirements.
fn validate_password(password: &str) -> Result<(), IpcError> {
    if password.is_empty() {
        return Err(IpcError::InvalidInput("Password required".into()));
    }
    // Note: We don't enforce password complexity — user choice
    Ok(())
}
```

### Rust-side password zeroization at IPC boundary

Handlers that accept `String` passwords must consume the Rust-side IPC copy into a zeroizing byte buffer at the boundary:

```rust
use zeroize::Zeroizing;

#[tauri::command]
async fn authenticate(
    password: String,
    key_file_path: Option<PathBuf>,
    state: tauri::State<'_, AppState>,
) -> Result<AuthResponse, IpcError> {
    let password_bytes = Zeroizing::new(password.into_bytes());

    auth::authenticate_with_bytes(password_bytes, key_file_path, &state).await
}
```

---

## Frontend Architecture

### Project Structure

```
src/                           # Leptos frontend
├── main.rs                    # App entry, context providers
├── app.rs                     # Root App component, routing
├── lib.rs                     # Re-exports
│
├── auth/                      # Authentication feature
│   ├── mod.rs
│   ├── login_page.rs          # Main login view
│   ├── password_input.rs      # Secure password field
│   └── key_file_indicator.rs  # USB key detection status
│
├── vault/                     # Vault browsing feature
│   ├── mod.rs
│   ├── vault_browser.rs       # Main vault view
│   ├── file_list.rs           # File/folder list
│   ├── file_item.rs           # Single file row
│   ├── breadcrumbs.rs         # Path navigation
│   └── upload_button.rs       # Upload trigger
│
├── remote/                    # Cloud file browser feature
│   ├── mod.rs
│   ├── remote_browser.rs      # Manifest-linked remote view (list_remote)
│   ├── remote_file_list.rs    # Remote file rows (pull / delete actions)
│   └── orphan_indicator.rs    # Display for blobs with no manifest entry
│
├── destinations/              # Destination session management
│   ├── mod.rs
│   ├── destination_list.rs    # List primary + backup destinations
│   ├── add_destination.rs     # Guided setup wizard (5 backends + paste)
│   └── destination_item.rs    # Single destination row (sync now, delete)
│
├── transfer/                  # File transfer feature
│   ├── mod.rs
│   ├── progress_modal.rs      # Upload/download progress
│   └── transfer_queue.rs      # Pending transfers list
│
├── layout/                    # Shared layout components
│   ├── mod.rs
│   ├── app_shell.rs           # Main layout wrapper
│   ├── header.rs              # Top bar with logo
│   └── session_status.rs      # Lock status, timeout countdown
│
├── components/                # Generic reusable components
│   ├── mod.rs
│   ├── button.rs
│   ├── input.rs
│   ├── modal.rs
│   └── spinner.rs
│
└── state/                     # Global state contexts
    ├── mod.rs
    ├── session_context.rs     # Auth state
    ├── vault_context.rs       # File list state
    └── sync_context.rs        # Sync status state
```

### State Contexts

#### Session Context

```rust
// src/state/session_context.rs

use leptos::*;

/// Authentication and session state.
#[derive(Clone, Debug, Default)]
pub struct SessionState {
    /// Whether the vault is unlocked.
    pub is_unlocked: bool,
    /// Vault ID if unlocked.
    pub vault_id: Option<String>,
    /// Seconds until session timeout.
    pub timeout_seconds: Option<u64>,
    /// Whether authentication is in progress.
    pub authenticating: bool,
    /// Last auth error message.
    pub error: Option<String>,
}

/// Session context provider.
#[component]
pub fn SessionProvider(children: Children) -> impl IntoView {
    let (state, set_state) = signal(SessionState::default());
    let (stop_polling, set_stop_polling) = signal(false);
    
    provide_context(state);
    provide_context(set_state);
    
    // Poll session status periodically while component is mounted.
    // Uses a stop signal for clean shutdown instead of an infinite loop.
    spawn_local({
        let stop_polling = stop_polling;
        async move {
            while !stop_polling.get_untracked() {
                if let Ok(status) = invoke::<_, SessionStatus>("get_session_status", &()).await {
                    // Guard: on_cleanup may fire while invoke is suspended.
                    // Check stop flag before writing to the signal to prevent
                    // a panic from writing to a disposed signal owner scope.
                    if stop_polling.get_untracked() {
                        break;
                    }
                    set_state.update(|s| {
                        s.is_unlocked = status.is_unlocked;
                        s.vault_id = status.vault_id.clone();
                        s.timeout_seconds = status.timeout_seconds;
                    });
                }
                // Guard: on_cleanup may fire while TimeoutFuture is suspended.
                if stop_polling.get_untracked() {
                    break;
                }
                gloo_timers::future::TimeoutFuture::new(5_000).await;
            }
        }
    });
    
    // Cleanup: stop polling when provider unmounts
    on_cleanup(move || set_stop_polling.set(true));
    // The stop flag must be checked at three points: the while condition,
    // after each awaited IPC call (before any signal write), and after each
    // awaited timer. Checking only at the loop top leaves a race window where
    // on_cleanup fires while a future is suspended, causing set_state.update
    // to execute against a disposed arena scope and panicking at runtime.
    
    children()
}

/// Hook to access session state.
pub fn use_session() -> ReadSignal<SessionState> {
    use_context::<ReadSignal<SessionState>>()
        .expect("SessionProvider must wrap the app")
}

/// Hook to access session actions.
pub fn use_session_actions() -> WriteSignal<SessionState> {
    use_context::<WriteSignal<SessionState>>()
        .expect("SessionProvider must wrap the app")
}
```

#### Vault Context

```rust
// src/state/vault_context.rs

use leptos::*;

/// Current vault browser state.
#[derive(Clone, Debug, Default)]
pub struct VaultState {
    /// Current directory path.
    pub current_path: String,
    /// Files in current directory.
    pub files: Vec<FileEntry>,
    /// Loading state.
    pub loading: bool,
    /// Error message if any.
    pub error: Option<String>,
    /// Selected file IDs.
    pub selected: Vec<String>,
}

/// Actions for vault state mutations.
pub struct VaultActions {
    set_state: WriteSignal<VaultState>,
}

impl VaultActions {
    /// Navigate to a directory.
    pub fn navigate(&self, path: String) {
        let set_state = self.set_state;
        spawn_local(async move {
            set_state.update(|s| s.loading = true);
            
            match invoke::<_, Vec<FileEntry>>("list_directory", &path).await {
                Ok(files) => {
                    set_state.update(|s| {
                        s.current_path = path;
                        s.files = files;
                        s.loading = false;
                        s.error = None;
                    });
                }
                Err(err) => {
                    set_state.update(|s| {
                        s.loading = false;
                        s.error = Some(err.to_string());
                    });
                }
            }
        });
    }
    
    /// Clear all state on vault lock.
    pub fn clear(&self) {
        self.set_state.update(|s| {
            s.files.clear();
            s.current_path = String::new();
            s.selected.clear();
        });
    }
}
```

### Page Components

#### Login Page

```rust
// src/auth/login_page.rs

use leptos::*;
use zeroize::Zeroize;
use crate::components::{Button, Input};
use crate::state::use_session_actions;

/// Login page with password and optional USB key file inputs.
/// Key file field is shown only for Tier 2 vaults.
#[component]
pub fn LoginPage() -> impl IntoView {
    let (password, set_password) = signal(String::new());
    let (key_file_path, set_key_file_path) = signal::<Option<String>>(None);
    let (error, set_error) = signal::<Option<String>>(None);
    let (loading, set_loading) = signal(false);
    let (vault_tier, set_vault_tier) = signal::<u8>(2); // Read from vault header
    
    let session_actions = use_session_actions();
    
    // Handle USB key file detection (Tier 2 only)
    Effect::new(move |_| {
        spawn_local(async move {
            // Read vault header to determine tier
            // If Tier 2: listen for device events, auto-populate key_file_path
        });
    });
    
    let on_submit = move |_| {
        let mut password_value = password.get();
        let key_file = key_file_path.get();
        let tier = vault_tier.get();
        
        if password_value.is_empty() {
            set_error.set(Some("Password is required".into()));
            return;
        }
        
        // Key file required only for Tier 2 vaults
        if tier == 2 && key_file.is_none() {
            set_error.set(Some("Please insert your USB key".into()));
            return;
        }
        
        set_loading.set(true);
        set_error.set(None);
        
        spawn_local(async move {
            let result = invoke::<_, AuthResponse>("authenticate", &AuthRequest {
                password: password_value.clone(),
                key_file_path: key_file, // None for Tier 1, Some(...) for Tier 2
            }).await;
            
            // CRITICAL: Zero password immediately after use (Zero-Trace compliance)
            password_value.zeroize();
            set_password.update(|s| s.zeroize());
            
            match result {
                Ok(response) => {
                    session_actions.update(|s| {
                        s.is_unlocked = true;
                        s.vault_id = Some(response.vault_id);
                        s.authenticating = false;
                    });
                }
                Err(err) => {
                    set_error.set(Some(err.message));
                    set_loading.set(false);
                }
            }
        });
    };
    
    view! {
        <div class="min-h-screen bg-iron flex items-center justify-center p-4">
            <div class="w-full max-w-md">
                <div class="bg-stone border border-steel rounded-xl p-6 shadow-xl">
                    <h1 class="text-2xl font-semibold text-bone text-center mb-6">
                        "Unlock Vault"
                    </h1>
                    
                    <div class="space-y-4">
                        <Input
                            input_type="password"
                            label="Password"
                            placeholder="Enter your password"
                            value=password
                            on_input=move |v| set_password.set(v)
                        />
                        
                        <KeyFileIndicator
                            detected_path=key_file_path
                            on_manual_select=move |path| set_key_file_path.set(Some(path))
                        />
                        
                        {move || error.get().map(|e| view! {
                            <div class="text-danger text-sm">{e}</div>
                        })}
                        
                        <Button
                            variant="primary"
                            loading=loading
                            on_click=on_submit
                        >
                            "Unlock"
                        </Button>
                    </div>
                </div>
            </div>
        </div>
    }
}
```

#### Vault Browser

```rust
// src/vault/vault_browser.rs

use leptos::*;
use crate::state::{use_vault, use_vault_actions};
use crate::layout::AppShell;

/// Main vault browser view.
#[component]
pub fn VaultBrowser() -> impl IntoView {
    let vault = use_vault();
    let actions = use_vault_actions();
    
    // Load root on mount
    Effect::new(move |_| {
        actions.navigate("/".into());
    });
    
    view! {
        <AppShell>
            <div class="space-y-4">
                <Breadcrumbs path=move || vault.get().current_path.clone() />
                
                <div class="flex gap-2">
                    <UploadButton />
                    <SyncButton />
                </div>
                
                <Show
                    when=move || !vault.get().loading
                    fallback=|| view! { <Spinner /> }
                >
                    <FileList files=move || vault.get().files.clone() />
                </Show>
                
                {move || vault.get().error.map(|e| view! {
                    <div class="text-danger text-sm">{e}</div>
                })}
            </div>
        </AppShell>
    }
}
```

#### Cloud File Browser

```rust
// src/remote/remote_browser.rs

use leptos::*;
use crate::layout::AppShell;

/// Cloud file browser: lists files on the primary remote destination,
/// resolved via the local manifest. Vault must be unlocked.
#[component]
pub fn RemoteBrowser() -> impl IntoView {
    let (entries, set_entries) = signal::<Vec<RemoteFileEntry>>(vec![]);
    let (loading, set_loading) = signal(true);
    let (error, set_error) = signal::<Option<String>>(None);

    // Load remote listing on mount
    Effect::new(move |_| {
        spawn_local(async move {
            set_loading.set(true);
            match invoke::<_, Vec<RemoteFileEntry>>("list_remote", &ListRemoteRequest {
                remote_prefix: "vault/".into(),
            }).await {
                Ok(result) => {
                    set_entries.set(result);
                    set_loading.set(false);
                }
                Err(err) => {
                    set_error.set(Some(err.message));
                    set_loading.set(false);
                }
            }
        });
    });

    view! {
        <AppShell>
            <div class="space-y-4">
                <h2 class="text-xl font-semibold text-bone">"Remote Files"</h2>

                <Show
                    when=move || !loading.get()
                    fallback=|| view! { <Spinner /> }
                >
                    <RemoteFileList
                        entries=move || entries.get()
                        on_pull=move |blob_id| { /* invoke download_file */ }
                        on_delete=move |blob_id| { /* invoke delete_file, refresh list */ }
                    />
                </Show>

                {move || error.get().map(|e| view! {
                    <div class="text-danger text-sm">{e}</div>
                })}
            </div>
        </AppShell>
    }
}
```

#### Destination Manager

```rust
// src/destinations/destination_list.rs

use leptos::*;
use crate::layout::AppShell;

/// Lists all configured destination sessions and provides add/delete/sync actions.
#[component]
pub fn DestinationList() -> impl IntoView {
    let (destinations, set_destinations) = signal::<Vec<DestinationEntry>>(vec![]);
    let (loading, set_loading) = signal(true);

    Effect::new(move |_| {
        spawn_local(async move {
            if let Ok(result) = invoke::<_, Vec<DestinationEntry>>("list_destinations", &()).await {
                set_destinations.set(result);
                set_loading.set(false);
            }
        });
    });

    view! {
        <AppShell>
            <div class="space-y-4">
                <div class="flex justify-between items-center">
                    <h2 class="text-xl font-semibold text-bone">"Storage Destinations"</h2>
                    <AddDestinationButton on_added=move |entry| {
                        set_destinations.update(|d| d.push(entry));
                    } />
                </div>

                <Show
                    when=move || !loading.get()
                    fallback=|| view! { <Spinner /> }
                >
                    <For
                        each=move || destinations.get()
                        key=|d| d.destination_id.clone()
                        children=move |dest| view! {
                            <DestinationItem
                                destination=dest.clone()
                                on_sync=move |id| { /* invoke sync_backup(Some(id)) */ }
                                on_delete=move |id| { /* invoke delete_destination(id), refresh */ }
                            />
                        }
                    />
                </Show>
            </div>
        </AppShell>
    }
}
```

### Tauri IPC Integration

```rust
// src/lib.rs — Tauri invoke wrapper

use serde::{de::DeserializeOwned, Serialize};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "core"])]
    async fn invoke(cmd: &str, args: JsValue) -> JsValue;
}

/// Type-safe Tauri command invocation.
pub async fn invoke_command<A, R>(cmd: &str, args: &A) -> Result<R, IpcError>
where
    A: Serialize,
    R: DeserializeOwned,
{
    let args_js = serde_wasm_bindgen::to_value(args)
        .map_err(|_| IpcError::internal("Serialization error"))?;
    
    let result = invoke(cmd, args_js).await;
    
    serde_wasm_bindgen::from_value(result)
        .map_err(|_| IpcError::internal("Deserialization error"))
}

/// Frontend representation of IPC errors.
#[derive(Debug, Clone)]
pub struct IpcError {
    pub kind: String,
    pub message: String,
}

impl IpcError {
    fn internal(msg: &str) -> Self {
        Self {
            kind: "internalError".into(),
            message: msg.into(),
        }
    }
}
```

---

## Drop Zone Component

The drop zone is the primary upload interface for Arx Runa (Use Case 1). Users drag files or folders onto the drop zone area rather than using a system file picker by default.

### Implementation

Tauri provides native drag-and-drop events via the WebView. When files are dropped onto the drop zone area:

1. The WebView captures the `drop` event and extracts file paths from the `DataTransfer` object
2. For each dropped path, the frontend invokes the `upload_file` command
3. Folders are recursively traversed — each file within the folder is uploaded individually
4. The transfer queue displays progress for all pending uploads

### Component structure

Add `drop_zone.rs` to the vault browser module:

```
src/vault/
├── ...
├── drop_zone.rs           # Drag-and-drop upload area
└── upload_button.rs       # Fallback file picker button
```

### Visual feedback

- **Idle**: Drop zone displays "Drag files here to upload" with a dashed border
- **Dragover**: Border highlights, background tints to indicate a valid drop target
- **Processing**: Drop zone shows the count of files queued and a spinner

The upload button remains as a fallback for accessibility and for users who prefer a file picker dialog.

### Scope

Implementation target: Phase 6.

---

## In-App File Viewing (Zero-Trace)

For supported file types, Arx Runa decrypts file content into WASM memory and renders it in the WebView without writing a temporary file to disk. This preserves Zero-Trace compliance.

### Supported types (MVP)

| Type | Rendering approach |
|------|-------------------|
| Images (JPEG, PNG, GIF, WebP) | Decrypt → base64 → `blob:` URL in `<img>` tag |
| Text (plain text, Markdown, JSON, CSV) | Decrypt → UTF-8 string → `<pre>` or rendered Markdown |
| PDF | Deferred — requires embedded PDF viewer |
| Video | Deferred — requires streaming decryption and progressive playback |

### Flow

1. User selects a file in the vault browser and chooses "View"
2. Frontend invokes `get_file_content(file_id)`
3. Backend validates file size:
   - If `size_bytes > 50 MiB`: return `IpcError::InvalidInput("File too large for in-app viewing; use download instead")`
   - Else: decrypt all chunks into a RAM buffer, assemble the file, and return it as base64 in a `FileContent` response
4. Frontend creates a `blob:` URL from the decoded bytes and renders it in the appropriate viewer component
5. On close or vault lock, the `blob:` URL is revoked and the buffer is released

### Size limits

For files exceeding 50 MiB, in-app viewing is rejected with:
`IpcError::InvalidInput("File too large for in-app viewing; use download instead")`.
The user must use `download_file` to export a decrypted copy. This limit prevents excessive WASM memory usage.

### Security property

No decrypted content touches the filesystem. The `blob:` URL exists only in WebView memory and is revoked when the viewer closes. The CSP prevents `blob:` URLs from being accessed by external scripts.

### Scope

Implementation target: Phase 6.

---

## Zero-Trace Compliance

### Frontend Requirements

1. **No localStorage or sessionStorage** — all state in Leptos signals (RAM only)
2. **No IndexedDB** — disabled in CSP
3. **No service workers** — disabled in CSP
4. **Clear state on lock** — `VaultActions::clear()` called when session locks
5. **No console logging of sensitive data** — never log file contents, keys, or passwords
6. **Zeroize sensitive strings** — use `zeroize` crate for password fields before clearing

### CSP Configuration

```json
// tauri.conf.json
{
  "app": {
    "withGlobalTauri": true,  // Required for Leptos WASM IPC via window.__TAURI__
    "security": {
      "csp": {
        "default-src": "'self'",
        "connect-src": "ipc: http://ipc.localhost",
        "script-src": "'self' 'wasm-unsafe-eval'",
        "style-src": "'self' 'unsafe-inline'",
        "img-src": "'self' asset: http://asset.localhost blob: data:"
      }
    }
  }
}
```

**Note on `withGlobalTauri`**: This exposes `window.__TAURI__` globally, which is required for Leptos WASM to invoke Tauri commands. The CSP ensures only local scripts (from `'self'`) can access it — external scripts cannot be loaded or executed.

### State Clearing on Lock

```rust
// When session locks (timeout or explicit lock):

// 1. Backend: SessionManager zeros all keys in mlocked memory
session_manager.lock().await;

// 2. Frontend: Clear all contexts
session_state.update(|s| {
    s.is_unlocked = false;
    s.vault_id = None;
    s.timeout_seconds = None;
});

vault_state.update(|s| {
    s.files.clear();
    s.current_path.clear();
    s.selected.clear();
});

sync_state.update(|s| {
    s.syncing = false;
    s.pending_changes = 0;
});
```

---

## Security Analysis

| Observable | Mitigation | Notes |
|------------|------------|-------|
| IPC command names visible in JS | Commands are generic ("upload_file") — no sensitive info in names | Acceptable |
| Error messages reach frontend | Sanitised via IpcError — no sensitive internal paths, keys, or internals | Critical |
| Session status polling | Status contains only boolean + timeout — no keys | Acceptable |
| Progress updates stream to frontend | Only percentages and byte counts — no file content | Acceptable |
| Frontend state persists until lock | Vault lock clears all contexts via `VaultActions::clear()` | Critical for Zero-Trace |
| WebView may cache resources | CSP + no localStorage + asset scope restrictions | See capabilities |

---

## Threat Model Additions

### Frontend as untrusted

The frontend runs in a WebView which is inherently less trusted than the Rust backend. Design principle: the frontend is a display layer only — it never holds key material and never persistently stores decrypted content. Decrypted bytes may exist transiently in WebView/WASM memory during active rendering and are cleared on close or lock.

- **Compromised WebView scenario**: If an attacker injects JS into the WebView (via XSS or browser exploit), they can call any exposed Tauri command. Mitigation: commands never return key material, and all destructive operations (delete, overwrite) require explicit user action in the frontend.

- **Clipboard attack**: A malicious script could read/write clipboard. Mitigation: clipboard capability denied in Tauri config.

- **Brute-force authentication**: A compromised WebView could attempt rapid auth attempts. Mitigation: backend applies per-vault in-memory exponential backoff in `SessionManager` (`delay = min(30s, 2^(attempt-1)s)`), resets counter on successful authentication, and does not persist counters across app restarts. Delay is applied before returning `InvalidCredentials`.

### Out of scope

- **Memory scraping in WebView**: WASM memory could theoretically be dumped. This is equivalent to the cold-boot threat already documented in the threat model.
- **Malicious Tauri plugins**: Arx Runa uses no third-party Tauri plugins.

---

## Open Decisions

| Decision | Options | Recommendation | Status |
|----------|---------|----------------|--------|
| Conflict resolution UI | Auto-resolve vs. manual merge | Last-write-wins with conflict notification | Deferred |
| Keyboard shortcuts | Implement standard shortcuts (Ctrl+L lock, etc.) | Ctrl+L lock, Escape cancel only for MVP | Deferred |

---

## Decisions Made

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Frontend framework | Leptos (Rust + WASM) | Single language, Zero-Trace compliance, type safety (ADR-002) |
| CSS framework | Tailwind CSS | Utility-first, purges unused CSS, dark mode support |
| Command organisation | Domain-grouped submodules | Better maintainability while keeping single invoke_handler |
| Error sanitisation | Trait-based From impls | Clean code, consistent sanitisation, explicit mappings |
| Frontend state | Multiple focused contexts | Maps to security boundaries (session ≠ vault ≠ sync) |
| Progress updates | Tauri IPC Channel | Real-time streaming, no polling overhead |
| Session status | 5-second polling | Simple, low overhead, immediate UI update on lock |
| Vault creation | `create_vault` command with tier selection | Completes the Phase 2 auth workflow |
| File viewing | In-app for images/text, download-only for other types | Zero-Trace compliance; video deferred |
| Sharing commands | `sharing_commands.rs` command group (see canonical command surface) | Covers Phase 5 file sharing operations |
| Vault chunk size | User-configurable at creation via `chunk_size_bytes` (128 KiB–64 MiB, default 4 MiB) | Chunk size is a privacy vs. efficiency dial; immutable after creation to preserve uniform blob size within the vault |
| Epoch buffer | User-toggleable at creation via `epoch_buffer_enabled` (off by default) | Opt-in hybrid routing: small files are packed via epoch buffering; large files remain immediate standalone uploads |
| Legacy inline destination config retired | Replaced by `DestinationSessionConfig` for destination configuration commands | Multi-destination model requires a richer type that includes credential blob, backup mode, and is_primary flag; old type had no concept of primary vs. backup |
| Destination commands | New `destination_commands.rs` module (`add_destination`, `list_destinations`, `delete_destination`) | Destination session management is a distinct domain from sync operations; separating into its own module keeps `sync_commands.rs` focused |
| `migrate_vault` parameter | `new_destination_id: String` (references an existing `DestinationSession`) instead of inline legacy destination config | Migration target must already be configured and validated as a destination session; passing raw config inline would bypass credential storage and validation |
| `sync_backup` command | Optional `destination_id`; None = sync to all backup destinations | Gives users control (sync one specific backup vs. all at once) without requiring multiple invocations |
| `list_remote` command | In `file_commands.rs`; returns `Vec<RemoteFileEntry>` with manifest-linked filenames | Remote listing is a file operation from the user's perspective; separating from sync commands keeps the IPC surface aligned with user intent |
| Cloud file browser frontend | `src/remote/` module with `RemoteBrowser`, `RemoteFileList`, `OrphanIndicator` | Logically separate from local vault browser; orphan handling is remote-specific |
| Destination manager frontend | `src/destinations/` module with `DestinationList`, `AddDestination`, `DestinationItem` | Destination management is a settings-like workflow, distinct from file browsing and sync status |
| IPC password handling | Convert incoming `String` to `Zeroizing<Vec<u8>>`, scrub backing bytes, drop original String immediately | Removes Rust-side heap residue for passwords after IPC deserialization |
| `share_file` terminology | HPKE (RFC 9180), not ECIES | Aligns command docs with Phase 5 canonical sharing construction |
| `get_file_content` behavior | Full response only; reject files above 50 MiB with explicit `InvalidInput` error | Matches command signature and avoids implicit streaming ambiguity |
| Vault path validation | Allowlist character regex + explicit traversal/absolute-path checks | Stronger boundary than denylist-only validation |
| Auth brute-force mitigation | Per-vault in-memory exponential backoff (1s base, 30s max), reset on success, non-persistent | Deterministic anti-automation behavior aligned with auth/session lifecycle |
