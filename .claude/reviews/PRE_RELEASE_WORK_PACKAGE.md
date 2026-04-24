# Arx Runa Pre-Release Work Package

**Date**: 2026-04-24  
**Scope**: MVP pre-release fixes (3 MEDIUM-priority issues)  
**Status**: Ready for implementation  
**Reference**: See `DESIGN_IMPLEMENTATION_REVIEW_2026-04-23.md` for full context

---

## Overview

This document contains the **3 medium-priority issues that must be completed before MVP release**. All other issues are either critical (0 found) or deferred to Phase 7+ (4 low-priority items).

**Estimated Effort**: 2-3 days total  
**Complexity**: 2 straightforward, 1 difficult  
**Blocking**: Yes — MVP launch depends on these

---

## Issues Summary

| ID | Title | Complexity | Effort | Phase | Blocking |
|----|----|---------|--------|-------|----------|
| M1 | Cloud Sync Session Lifecycle Wiring | Difficult | 1-1.5 days | 4 | Yes |
| M2 | Startup Retry Orchestration | Straightforward | 1 day | 2 | Yes |
| M3 | Streaming Progress Channel Validation | Straightforward | 0.5-1 day | 6 | Yes |

---

## Issue M1: Cloud Sync Session Lifecycle Wiring

**Phase**: 4 (Cloud Synchronisation)  
**Design Location**: Phase 4.2 sub-phase implementation notes  
**Complexity**: Difficult (requires interaction between auth and cloud transport)  
**Estimated Effort**: 1-1.5 days

### Problem

The RcloneTransport subprocess lifecycle (rclone.conf file creation and cleanup) is not fully integrated with SessionManager. Currently:

- ✅ RcloneTransport creates encrypted rclone.conf on `authenticate()`
- ✅ Credentials are stored in SQLCipher
- ❌ **Gap**: rclone.conf is not automatically cleaned up on `lock()` or session timeout
- ❌ **Gap**: Concurrent session-scoped operations don't properly serialize rclone.conf access
- ❌ **Gap**: Session timeout doesn't trigger rclone.conf cleanup

### Impact

- **Before Fix**: Stale rclone.conf files accumulate in temp directory; credentials linger after session expires
- **After Fix**: rclone.conf lifecycle is bound to session lifecycle (created on auth, cleaned on lock/timeout)
- **Security Risk**: Medium (credentials could be read from temp directory after session expires if not cleaned)

### Files Affected

- `src-tauri/src/auth/session/manager.rs` — SessionManager lifecycle
- `src-tauri/src/storage/cloud/rclone.rs` — RcloneTransport subprocess management
- `src-tauri/src/storage/cloud/rclone_subprocess.rs` — Subprocess lifecycle

### Detailed Fix

**Step 1: Add rclone.conf cleanup to SessionManager::lock()**

File: `src-tauri/src/auth/session/manager.rs`

After the session keys are zeroized, call a cleanup function:

```rust
pub async fn lock(&self) -> Result<(), SessionError> {
    let mut state = self.state.write().await;
    if let SessionState::Active { vault_id, keys, created_at: _ } = &*state {
        // NEW: Cleanup rclone.conf for this session
        if let Some(cloud_transport) = &*self.cloud_transport.read().await {
            cloud_transport.cleanup_session_artifacts().await.ok();
        }
        
        // Existing zeroization code...
        keys.zeroize();
        *state = SessionState::Locked;
        Ok(())
    } else {
        Err(SessionError::NoSession)
    }
}
```

**Step 2: Add cleanup call to session timeout handler**

File: `src-tauri/src/auth/session/manager.rs` in the timeout task:

```rust
// In the timeout task's expiry loop:
if let Some(cloud_transport) = &*self.cloud_transport.read().await {
    cloud_transport.cleanup_session_artifacts().await.ok();
}
self.emit_event(SessionEvent::Locked).ok();
```

**Step 3: Implement cleanup_session_artifacts() in CloudTransport trait**

File: `src-tauri/src/storage/cloud/mod.rs`

Add to CloudTransport trait:

```rust
pub trait CloudTransport: Send + Sync {
    // ... existing methods ...
    
    /// Clean up session-scoped artifacts (e.g., rclone.conf temp files)
    async fn cleanup_session_artifacts(&self) -> Result<(), CloudTransportError>;
}
```

**Step 4: Implement cleanup in RcloneTransport**

File: `src-tauri/src/storage/cloud/rclone.rs`

```rust
impl CloudTransport for RcloneTransport {
    async fn cleanup_session_artifacts(&self) -> Result<(), CloudTransportError> {
        if let Some(rclone_config_path) = &self.rclone_config_path {
            // Delete the temp rclone.conf
            tokio::fs::remove_file(rclone_config_path)
                .await
                .map_err(|e| CloudTransportError::IoError(e.to_string()))?;
        }
        Ok(())
    }
}
```

**Step 5: Implement cleanup in MockCloudTransport (for tests)**

File: `src-tauri/src/storage/cloud/mock.rs`

```rust
impl CloudTransport for MockCloudTransport {
    async fn cleanup_session_artifacts(&self) -> Result<(), CloudTransportError> {
        Ok(()) // No-op for mock
    }
}
```

### Test Verification

Add test to `src-tauri/src/auth/session/manager.rs`:

```rust
#[tokio::test]
async fn test_lock_cleans_up_rclone_artifacts() {
    let manager = create_test_session_manager().await;
    let cloud_transport = Arc::new(RcloneTransport::new(/* ... */));
    manager.set_cloud_transport(cloud_transport.clone());
    
    manager.authenticate(password, key_source, salt, params).await.unwrap();
    
    // Verify rclone.conf exists
    assert!(rclone_config_path.exists());
    
    manager.lock().await.unwrap();
    
    // Verify rclone.conf was deleted
    assert!(!rclone_config_path.exists());
}
```

### Effort Breakdown

- Modify SessionManager::lock() — 20 lines (15 min)
- Modify timeout handler — 10 lines (10 min)
- Add CloudTransport::cleanup_session_artifacts() — 5 lines (5 min)
- Implement in RcloneTransport — 15 lines (15 min)
- Implement in MockCloudTransport — 5 lines (5 min)
- Write test — 25 lines (20 min)
- **Total**: ~80 lines, 1-1.5 days (including testing)

---

## Issue M2: Startup Retry Orchestration

**Phase**: 2 (Authentication & Session Management)  
**Design Location**: Phase 2 deferred items (pending-vault-header retry)  
**Complexity**: Straightforward (add retry loop)  
**Estimated Effort**: 1 day

### Problem

When changing password or rotating key files, a `pending-vault-header.json` artifact is written to `dirs::config_dir() / "arx-runa/"`. If the process crashes or is interrupted:

- ✅ Artifact is stored with owner-only permissions (0o600)
- ✅ Header is plaintext (can be re-downloaded)
- ❌ **Gap**: Startup doesn't check for incomplete pending-vault-header
- ❌ **Gap**: No retry loop to complete the operation on next app launch
- ❌ **Gap**: Pending artifact cleanup on successful completion is missing

### Impact

- **Before Fix**: Crashed password-change operations leave incomplete state; user can't recover until manually deleting the artifact
- **After Fix**: On app startup, incomplete operations are detected and retried automatically
- **Data Loss Risk**: Low (artifact is plaintext; user can manually recover)

### Files Affected

- `src-tauri/src/main.rs` or equivalent startup code — Add startup retry check
- `src-tauri/src/auth/ceremonies/types.rs` — Pending artifact structure
- `src-tauri/src/auth/ceremonies/helpers.rs` — Artifact read/write/cleanup

### Detailed Fix

**Step 1: Add startup retry check in Tauri builder setup**

File: `src-tauri/src/main.rs` (or your main app startup):

```rust
#[tauri::command]
async fn check_pending_vault_operations(
    app_handle: AppHandle,
) -> Result<bool, String> {
    let config_dir = dirs::config_dir()
        .ok_or("Could not determine config directory".to_string())?
        .join("arx-runa");
    
    let pending_path = config_dir.join("pending-vault-header.json");
    
    if !pending_path.exists() {
        return Ok(false); // No pending operations
    }
    
    // Read the pending header to determine operation type
    let pending_json = tokio::fs::read_to_string(&pending_path)
        .await
        .map_err(|e| e.to_string())?;
    
    let pending: PendingVaultHeader = serde_json::from_str(&pending_json)
        .map_err(|e| e.to_string())?;
    
    // Emit event to UI: "Recovery needed - incomplete password change detected"
    app_handle.emit_all("vault_operation_recovery_needed", pending).ok();
    
    Ok(true) // Pending operation detected
}

#[tauri::command]
async fn retry_pending_vault_operation(
    app_handle: AppHandle,
    password: String,
    key_source: Option<String>,
) -> Result<(), String> {
    let config_dir = dirs::config_dir()
        .ok_or("Could not determine config directory".to_string())?
        .join("arx-runa");
    
    let pending_path = config_dir.join("pending-vault-header.json");
    let pending_json = tokio::fs::read_to_string(&pending_path)
        .await
        .map_err(|e| e.to_string())?;
    
    let pending: PendingVaultHeader = serde_json::from_str(&pending_json)
        .map_err(|e| e.to_string())?;
    
    // Attempt to complete the original operation
    match pending.operation {
        PendingOperation::ChangePassword => {
            // Re-run change_password ceremony with recovered state
            ceremonies::change_password::complete_pending_change_password(
                password,
                key_source,
                pending,
            ).await.map_err(|e| e.to_string())?;
        }
        PendingOperation::RotateKeyFile => {
            ceremonies::rotate_key_file::complete_pending_rotation(
                password,
                key_source,
                pending,
            ).await.map_err(|e| e.to_string())?;
        }
    }
    
    // On success, delete the pending artifact
    tokio::fs::remove_file(&pending_path)
        .await
        .map_err(|e| e.to_string())?;
    
    Ok(())
}
```

**Step 2: Call startup check from Tauri setup**

In your Tauri builder setup hook:

```rust
.setup(|app| {
    let handle = app.handle().clone();
    
    tauri::async_runtime::spawn(async move {
        // Check for pending operations
        match check_pending_vault_operations(handle.clone()).await {
            Ok(true) => {
                handle.emit_all("startup_recovery_needed", ()).ok();
            }
            Ok(false) => {
                handle.emit_all("startup_ready", ()).ok();
            }
            Err(e) => {
                eprintln!("Error checking pending operations: {}", e);
            }
        }
    });
    
    Ok(())
})
```

**Step 3: Define PendingVaultHeader type**

File: `src-tauri/src/auth/ceremonies/types.rs`

```rust
#[derive(Serialize, Deserialize, Debug)]
pub struct PendingVaultHeader {
    pub vault_id: [u8; 16],
    pub operation: PendingOperation,
    pub partial_header: VaultHeader,
    pub created_at: SystemTime,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum PendingOperation {
    ChangePassword,
    RotateKeyFile,
}
```

**Step 4: Add cleanup to successful ceremonies**

File: `src-tauri/src/auth/ceremonies/change_password.rs`

After successful vault header upload:

```rust
// After ceremony completes successfully:
let config_dir = dirs::config_dir()
    .expect("config_dir")
    .join("arx-runa");

let pending_path = config_dir.join("pending-vault-header.json");
if pending_path.exists() {
    let _ = tokio::fs::remove_file(pending_path).await;
}
```

### Test Verification

Add test to `src-tauri/src/auth/ceremonies/helpers.rs`:

```rust
#[tokio::test]
async fn test_startup_detects_pending_vault_operation() {
    // Write a fake pending artifact
    let config_dir = dirs::config_dir().unwrap().join("arx-runa");
    tokio::fs::create_dir_all(&config_dir).await.unwrap();
    
    let pending = PendingVaultHeader { /* ... */ };
    let json = serde_json::to_string(&pending).unwrap();
    
    let pending_path = config_dir.join("pending-vault-header.json");
    tokio::fs::write(&pending_path, json).await.unwrap();
    
    // Verify startup detects it
    let has_pending = check_pending_vault_operations(app_handle).await.unwrap();
    assert!(has_pending);
    
    // Cleanup
    tokio::fs::remove_file(&pending_path).await.unwrap();
}
```

### Effort Breakdown

- Add check_pending_vault_operations() command — 30 lines (20 min)
- Add retry_pending_vault_operation() command — 40 lines (25 min)
- Add startup hook call — 15 lines (10 min)
- Define PendingVaultHeader type — 15 lines (10 min)
- Add cleanup to ceremonies — 10 lines (5 min)
- Write test — 20 lines (15 min)
- **Total**: ~130 lines, 1 day (including testing)

---

## Issue M3: Streaming Progress Channel Validation

**Phase**: 6 (Tauri IPC & Frontend)  
**Design Location**: Phase 6.3 sub-phase (streaming channels for long operations)  
**Complexity**: Straightforward (add validation)  
**Estimated Effort**: 0.5-1 day

### Problem

Long-running IPC commands (upload_file, download_file, sync_to_cloud) use Tauri channels for streaming progress updates. Currently:

- ✅ Channel is created and passed to backend operations
- ✅ Progress callback updates are emitted
- ❌ **Gap**: No validation that channel is still open before emitting (can panic if frontend closes)
- ❌ **Gap**: No timeout if frontend disconnects and channel blocks indefinitely
- ❌ **Gap**: Error case (operation fails) doesn't gracefully close the channel

### Impact

- **Before Fix**: If frontend closes connection during upload, backend can panic or deadlock
- **After Fix**: Channel status is checked; operation gracefully terminates if channel is closed
- **Runtime Risk**: Medium (could crash backend during long uploads)

### Files Affected

- `src-tauri/src/ui/commands/upload_file.rs` — Progress channel handling
- `src-tauri/src/ui/commands/download_file.rs` — Progress channel handling
- `src-tauri/src/ui/commands/sync_to_cloud.rs` — Progress channel handling
- `src-tauri/src/storage/vault_ops/upload_file.rs` — Underlying progress callback

### Detailed Fix

**Step 1: Add channel validation wrapper**

File: `src-tauri/src/ui/commands/mod.rs`

```rust
/// Wraps a Tauri channel to gracefully handle closed connections
pub struct ProgressChannel {
    tx: tauri::ipc::Channel<ProgressUpdate>,
    closed: Arc<AtomicBool>,
}

impl ProgressChannel {
    pub fn new(tx: tauri::ipc::Channel<ProgressUpdate>) -> Self {
        Self {
            tx,
            closed: Arc::new(AtomicBool::new(false)),
        }
    }
    
    pub async fn send(&self, update: ProgressUpdate) -> Result<(), String> {
        if self.closed.load(std::sync::atomic::Ordering::Relaxed) {
            return Err("Channel closed by frontend".to_string());
        }
        
        self.tx
            .send(update)
            .await
            .map_err(|e| format!("Channel send failed: {}", e))?;
        
        Ok(())
    }
    
    pub fn is_open(&self) -> bool {
        !self.closed.load(std::sync::atomic::Ordering::Relaxed)
    }
}
```

**Step 2: Update upload_file command**

File: `src-tauri/src/ui/commands/upload_file.rs`

```rust
#[tauri::command]
pub async fn upload_file(
    session_manager: tauri::State<'_, Arc<SessionManager>>,
    database: tauri::State<'_, Arc<RwLock<SqlCipherMetadataStore>>>,
    cloud_transport: tauri::State<'_, Arc<RwLock<Arc<dyn CloudTransport>>>>,
    vault_id: String,
    node_id: String,
    file_path: String,
    channel: tauri::ipc::Channel<ProgressUpdate>,
) -> Result<(), String> {
    let progress_ch = ProgressChannel::new(channel);
    let progress_cb = move |bytes_processed: u64, bytes_total: u64| {
        if progress_ch.is_open() {
            let update = ProgressUpdate {
                bytes_processed,
                bytes_total,
            };
            // Spawn to avoid blocking
            tauri::async_runtime::spawn({
                let ch = Arc::new(progress_ch.clone());
                async move {
                    let _ = ch.send(update).await;
                }
            });
        }
    };
    
    // Validate inputs
    let vault_id_parsed = VaultId::from_str(&vault_id)
        .map_err(|_| IpcError::InvalidInput("Invalid vault_id".to_string()))?;
    
    // Call underlying upload with progress callback
    vault_ops::upload_file(
        &database.read().await,
        &cloud_transport.read().await,
        vault_id_parsed,
        node_id,
        file_path,
        Some(&progress_cb),
    )
    .await
    .map_err(|e| format!("Upload failed: {}", e))?;
    
    // Signal completion
    let _ = progress_ch.send(ProgressUpdate {
        bytes_processed: bytes_total,
        bytes_total,
    }).await;
    
    Ok(())
}
```

**Step 3: Update download_file command similarly**

File: `src-tauri/src/ui/commands/download_file.rs`

(Same pattern as upload_file)

**Step 4: Update sync_to_cloud command**

File: `src-tauri/src/ui/commands/sync_to_cloud.rs`

```rust
#[tauri::command]
pub async fn sync_to_cloud(
    session_manager: tauri::State<'_, Arc<SessionManager>>,
    database: tauri::State<'_, Arc<RwLock<SqlCipherMetadataStore>>>,
    cloud_transport: tauri::State<'_, Arc<RwLock<Arc<dyn CloudTransport>>>>,
    vault_id: String,
    channel: tauri::ipc::Channel<SyncProgressUpdate>,
) -> Result<(), String> {
    let progress_ch = ProgressChannel::new(channel);
    let progress_cb = move |files_processed: u32, files_total: u32, current_file: Option<&str>| {
        if progress_ch.is_open() {
            let update = SyncProgressUpdate {
                files_processed,
                files_total,
                current_file: current_file.map(|s| s.to_string()),
            };
            tauri::async_runtime::spawn({
                let ch = Arc::new(progress_ch.clone());
                async move {
                    let _ = ch.send(update).await;
                }
            });
        }
    };
    
    // Call sync with validated progress callback
    vault_ops::push_vault(
        &database.read().await,
        &cloud_transport.read().await,
        vault_id_parsed,
        Some(&progress_cb),
    )
    .await
    .map_err(|e| IpcError::SyncFailed(e.to_string()))?;
    
    Ok(())
}
```

### Test Verification

Add test to `src-tauri/src/ui/commands/mod.rs`:

```rust
#[tokio::test]
async fn test_progress_channel_handles_closed_connection() {
    let (tx, mut rx) = tauri::ipc::channel(|_msg| {});
    let progress_ch = ProgressChannel::new(tx);
    
    // Simulate frontend closing the channel
    drop(rx);
    
    // Verify send gracefully fails instead of panicking
    let result = progress_ch.send(ProgressUpdate {
        bytes_processed: 100,
        bytes_total: 1000,
    }).await;
    
    assert!(result.is_err());
}
```

### Effort Breakdown

- Create ProgressChannel wrapper — 30 lines (20 min)
- Update upload_file command — 25 lines (15 min)
- Update download_file command — 25 lines (15 min)
- Update sync_to_cloud command — 25 lines (15 min)
- Write test — 15 lines (10 min)
- **Total**: ~120 lines, 0.5-1 day (including testing)

---

## Implementation Order

**Recommended sequence** (dependencies matter):

1. **M2 first** (1 day) — Auth startup recovery doesn't depend on other fixes
2. **M1 second** (1-1.5 days) — Cloud sync session lifecycle depends on completion of M2
3. **M3 third** (0.5-1 day) — IPC validation is independent

**Total Effort**: 2.5-3.5 days

---

## Testing & Validation

### Pre-Merge Checklist

- [ ] M1: SessionManager cleanup tested, RcloneTransport artifacts verified deleted
- [ ] M2: Startup recovery tested, pending artifact properly written/deleted
- [ ] M3: Channel validation tested with closed/open connections
- [ ] Build succeeds: `cargo build --release`
- [ ] All tests pass: `cargo test --lib`
- [ ] Clippy passes: `cargo clippy -- -D warnings`
- [ ] Format correct: `cargo fmt --check`

### Integration Testing

- [ ] Create vault → lock session → verify rclone.conf deleted (M1)
- [ ] Change password → kill app mid-operation → restart app → verify recovery prompt (M2)
- [ ] Start large file upload → close frontend → verify backend handles gracefully (M3)

---

## Reference

**Full Design Review**: See `DESIGN_IMPLEMENTATION_REVIEW_2026-04-23.md`

**Sections**:
- Section: "Issues Classified by Priority" — MEDIUM issues
- Section: "Phase 4 — Cloud Synchronisation" — M1 context
- Section: "Phase 2 — Auth & Session Management" — M2 context
- Section: "Phase 6 — Tauri IPC & Frontend" — M3 context

---

**Prepared For**: MVP Release  
**Status**: Ready for agent handoff  
**Go/No-Go**: Ready to implement (blocks MVP launch)
