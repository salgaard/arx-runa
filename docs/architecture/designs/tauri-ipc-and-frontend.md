# VoidGate — Tauri IPC Layer and Frontend Design

> Status: Reviewed. Implementation target: Phase 6.
> Last updated: 2026-03-31

### Review Log

| Date | Reviewer | Outcome |
|------|----------|---------|
| 2026-03-31 | Interactive design session | Critical fixes applied (password zeroization, polling cleanup), recommendations added |

---

## Goals

- Expose backend functionality (auth, storage, sync, sharing) to the frontend through Tauri commands with proper error sanitisation
- Build a minimal but functional Leptos UI for authentication, vault browsing, upload, download, and session management
- Enforce Zero-Trace principles: no localStorage, no persistent state, clear UI on lock
- Keep the IPC surface minimal and auditable — defense in depth via build.rs allowlist + capabilities
- Stream large file transfer progress in real-time without blocking the UI

---

## IPC Layer Architecture

### Command Organisation

Commands are organised into domain-grouped submodules under `src-tauri/src/ui/`, with all commands re-exported and registered in a single `invoke_handler` call.

```
src-tauri/src/ui/
├── mod.rs                 # Re-exports, invoke_handler registration
├── error.rs               # IpcError enum, From impls
├── auth_commands.rs       # authenticate, lock_session, get_session_status
├── file_commands.rs       # list_directory, upload_file, download_file, delete_file
├── sync_commands.rs       # sync_to_cloud, recover_from_cloud, get_sync_status
└── types.rs               # IPC-specific types (responses, progress updates)
```

### Command Signatures

```rust
// --- auth_commands.rs ---

/// Authenticate with password and USB key file, derive session keys.
/// Returns vault_id on success. Does NOT return any key material.
#[tauri::command]
async fn authenticate(
    password: String,
    key_file_path: PathBuf,
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
```

### Command Registration

All commands registered in `src-tauri/src/lib.rs`:

```rust
pub fn run() {
    tauri::Builder::default()
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            // Auth
            ui::authenticate,
            ui::lock_session,
            ui::get_session_status,
            // Files
            ui::list_directory,
            ui::upload_file,
            ui::download_file,
            ui::delete_file,
            // Sync
            ui::sync_to_cloud,
            ui::recover_from_cloud,
            ui::get_sync_status,
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
                        "authenticate",
                        "delete_file",
                        "download_file",
                        "get_session_status",
                        "get_sync_status",
                        "list_directory",
                        "lock_session",
                        "recover_from_cloud",
                        "sync_to_cloud",
                        "upload_file",
                    ])
            ),
    )
    .expect("failed to build tauri application");
}
```

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

impl From<auth::AuthError> for IpcError {
    fn from(err: auth::AuthError) -> Self {
        // Log full error for debugging
        tracing::error!("Auth error: {:?}", err);
        
        match err {
            auth::AuthError::InvalidPassword => {
                IpcError::AuthenticationFailed("Invalid credentials".into())
            }
            auth::AuthError::KeyFileNotFound => {
                IpcError::AuthenticationFailed("Key file not found".into())
            }
            auth::AuthError::KeyFileMismatch => {
                IpcError::AuthenticationFailed("Invalid credentials".into())
            }
            auth::AuthError::SessionLocked => {
                IpcError::VaultLocked("Please unlock your vault".into())
            }
            _ => IpcError::InternalError("An error occurred".into()),
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
| File paths (`/Users/chris/...`) | "File not found" or generic |
| Key derivation parameters | Never exposed |
| Memory addresses | Never exposed |
| Stack traces | Logged server-side only |
| Specific crypto errors | "An error occurred" |

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
/// Validates a vault path (no traversal, valid characters).
fn validate_vault_path(path: &str) -> Result<(), IpcError> {
    // Reject path traversal
    if path.contains("..") {
        return Err(IpcError::InvalidInput("Invalid path".into()));
    }
    
    // Reject absolute paths
    if path.starts_with('/') || path.starts_with('\\') {
        return Err(IpcError::InvalidInput("Path must be relative".into()));
    }
    
    // Reject control characters
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
                    set_state.update(|s| {
                        s.is_unlocked = status.is_unlocked;
                        s.vault_id = status.vault_id.clone();
                        s.timeout_seconds = status.timeout_seconds;
                    });
                }
                gloo_timers::future::TimeoutFuture::new(5_000).await;
            }
        }
    });
    
    // Cleanup: stop polling when provider unmounts
    on_cleanup(move || set_stop_polling.set(true));
    
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

/// Login page with password and USB key file inputs.
#[component]
pub fn LoginPage() -> impl IntoView {
    let (password, set_password) = signal(String::new());
    let (key_file_path, set_key_file_path) = signal::<Option<String>>(None);
    let (error, set_error) = signal::<Option<String>>(None);
    let (loading, set_loading) = signal(false);
    
    let session_actions = use_session_actions();
    
    // Handle USB key file detection
    Effect::new(move |_| {
        spawn_local(async move {
            // Listen for device events from backend
            // Auto-populate key_file_path when detected
        });
    });
    
    let on_submit = move |_| {
        let mut password_value = password.get();
        let key_file = key_file_path.get();
        
        if password_value.is_empty() {
            set_error.set(Some("Password is required".into()));
            return;
        }
        
        let Some(key_file) = key_file else {
            set_error.set(Some("Please insert your USB key".into()));
            return;
        };
        
        set_loading.set(true);
        set_error.set(None);
        
        spawn_local(async move {
            let result = invoke::<_, AuthResponse>("authenticate", &AuthRequest {
                password: password_value.clone(),
                key_file_path: key_file,
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
        <div class="min-h-screen bg-void-950 flex items-center justify-center p-4">
            <div class="w-full max-w-md">
                <div class="bg-void-900 border border-void-700 rounded-xl p-6 shadow-xl">
                    <h1 class="text-2xl font-semibold text-void-50 text-center mb-6">
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
| Error messages reach frontend | Sanitised via IpcError — no paths, keys, or internals | Critical |
| Session status polling | Status contains only boolean + timeout — no keys | Acceptable |
| Progress updates stream to frontend | Only percentages and byte counts — no file content | Acceptable |
| Frontend state persists until lock | Vault lock clears all contexts via `VaultActions::clear()` | Critical for Zero-Trace |
| WebView may cache resources | CSP + no localStorage + asset scope restrictions | See capabilities |

---

## Threat Model Additions

### Frontend as untrusted

The frontend runs in a WebView which is inherently less trusted than the Rust backend. Design principle: the frontend is a display layer only — it never holds keys or decrypted content.

- **Compromised WebView scenario**: If an attacker injects JS into the WebView (via XSS or browser exploit), they can call any exposed Tauri command. Mitigation: commands never return key material, and all destructive operations (delete, overwrite) require explicit user action in the frontend.

- **Clipboard attack**: A malicious script could read/write clipboard. Mitigation: clipboard capability denied in Tauri config.

- **Brute-force authentication**: A compromised WebView could attempt rapid auth attempts. Mitigation: backend implements exponential backoff (1s base, 30s max) on failed authentication attempts.

### Out of scope

- **Memory scraping in WebView**: WASM memory could theoretically be dumped. This is equivalent to the cold-boot threat already documented in the threat model.
- **Malicious Tauri plugins**: VoidGate uses no third-party Tauri plugins.

---

## Open Decisions

| Decision | Options | Recommendation | Status |
|----------|---------|----------------|--------|
| File preview in UI | Show preview for images/text vs. download-only | Download-only for MVP (security, scope) | Deferred |
| Conflict resolution UI | Auto-resolve vs. manual merge | Last-write-wins with conflict notification | Deferred |
| Keyboard shortcuts | Implement standard shortcuts (Ctrl+L lock, etc.) | Ctrl+L lock, Escape cancel only for MVP | Deferred |
| Vault creation flow | Add create_vault command | Separate design task | Open |

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
