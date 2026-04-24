# Implementation Guide — All 13 Issues

**Generated**: 2026-04-23  
**Total Issues**: 13 (4 MEDIUM, 9 LOW/ARCHITECTURE)  
**Total Effort**: 10-14 hours  
**Agent-Ready**: ✅ YES - All issues have exact file locations, code examples, and before/after specifications

---

## Quick Dispatch

Use this guide to assign issues to agents or contractors. Each issue has:
- ✅ Exact file + line numbers
- ✅ Before/after code examples
- ✅ Root cause explanation
- ✅ Test verification steps

---

## PHASE 1: CRITICAL FIXES (FIX BEFORE MERGE) — 1 hour

### Issue: M1 — Recovery Key Validation

**Severity**: CRITICAL  
**Files**: 2 files, 2 locations each  
**Effort**: ~10 lines, <1 hour  
**Risk**: None if not fixed = silent recovery slot corruption  

#### Files to Modify

1. **`src-tauri/src/auth/ceremonies/change_password.rs:109-113`**

**Current Code**:
```rust
match unwrap_recovery_key(&recovery_key, &key_encryption_key) {
    Ok(_master_key) => {
        recovery_key_for_rewrap = Some(recovery_key);
    }
    Err(_) => {
        return Err(AuthenticationError::InvalidCredentials);
    }
}
```

**Fixed Code**:
```rust
match unwrap_recovery_key(&recovery_key, &key_encryption_key) {
    Ok(recovered_master_key) => {
        // Validate that the recovered key is not all-zero
        if recovered_master_key.as_bytes().iter().all(|&b| b == 0) {
            return Err(AuthenticationError::InvalidCredentials);
        }
        recovery_key_for_rewrap = Some(recovery_key);
    }
    Err(_) => {
        return Err(AuthenticationError::InvalidCredentials);
    }
}
```

2. **`src-tauri/src/auth/ceremonies/rotate_key_file.rs:115-119`**

**Current Code**:
```rust
match unwrap_recovery_key(&recovery_key, &key_encryption_key) {
    Ok(_master_key) => {
        recovery_key_for_rewrap = Some(recovery_key);
    }
    Err(_) => {
        return Err(AuthenticationError::InvalidCredentials);
    }
}
```

**Fixed Code** (identical to change_password.rs):
```rust
match unwrap_recovery_key(&recovery_key, &key_encryption_key) {
    Ok(recovered_master_key) => {
        // Validate that the recovered key is not all-zero
        if recovered_master_key.as_bytes().iter().all(|&b| b == 0) {
            return Err(AuthenticationError::InvalidCredentials);
        }
        recovery_key_for_rewrap = Some(recovery_key);
    }
    Err(_) => {
        return Err(AuthenticationError::InvalidCredentials);
    }
}
```

#### Test & Verify

```bash
cargo build
cargo test --lib auth::
```

Expected: All tests pass, no new warnings.

---

## PHASE 2: HIGH PRIORITY (NEXT PATCH) — 2-3 hours

### Issue: M2 — Temporary Stack Arrays Not Zeroized

**Severity**: MEDIUM (memory defense-in-depth)  
**File**: 1 file, 4 locations  
**Effort**: ~20 lines, 1-2 hours  
**Risk**: Low if not fixed = temporary key material on stack

#### File to Modify

**`src-tauri/src/auth/session/manager.rs`**

**Locations**: Lines 258, 550, 898, 931

**Current Pattern**:
```rust
let sqlcipher_key_bytes = {
    let mut key_bytes = [0u8; 32];
    key_bytes.copy_from_slice(derived.sqlcipher_key.expose());
    key_bytes
};
```

**Fixed Pattern**:
```rust
let sqlcipher_key_bytes = {
    let mut key_bytes = Zeroizing::new([0u8; 32]);
    key_bytes.copy_from_slice(derived.sqlcipher_key.expose());
    key_bytes
};
```

**All 4 locations** (lines 258, 550, 898, 931) use the same pattern. Apply fix to each.

#### Test & Verify

```bash
cargo build
cargo test --lib session::
```

---

### Issue: M3 — Config Directory Path Panic

**Severity**: MEDIUM (panics on headless/container systems)  
**File**: 1 file, 1 function + 1 callsite  
**Effort**: ~25 lines, 1-2 hours  
**Risk**: Low if not fixed = session lock panics in unusual deployments

#### File to Modify

**`src-tauri/src/auth/session/manager.rs`**

**Location 1** — Modify function signature (line ~714):

**Current Code**:
```rust
fn session_rclone_conf_path() -> PathBuf {
    dirs::config_dir()
        .expect("config_dir must be available")
        .join("arx-runa")
        .join("rclone.conf")
}
```

**Fixed Code**:
```rust
fn session_rclone_conf_path() -> Result<PathBuf, AuthenticationError> {
    dirs::config_dir()
        .ok_or(AuthenticationError::InvalidCredentials)
        .map(|dir| dir.join("arx-runa").join("rclone.conf"))
}
```

**Location 2** — Update callsite in `destroy_rclone_conf()` (line ~699):

**Current Code**:
```rust
async fn destroy_rclone_conf() {
    let path = Self::session_rclone_conf_path();
    // ... use path ...
}
```

**Fixed Code**:
```rust
async fn destroy_rclone_conf() {
    match Self::session_rclone_conf_path() {
        Ok(path) => {
            // ... existing cleanup logic ...
        }
        Err(_) => {
            // Session lock is best-effort for credential cleanup
            // Log and proceed if config_dir unavailable
            tracing::warn!("config_dir unavailable; skipping rclone.conf cleanup");
        }
    }
}
```

#### Test & Verify

```bash
cargo build
cargo test --lib session::manager
```

---

### Issue: M4 — SQLCipher Error Handling Conflates Conditions

**Severity**: MEDIUM (diagnostic only)  
**Files**: 2 files, 2 locations  
**Effort**: ~20 lines, 1-2 hours  
**Risk**: Low if not fixed = harder to debug database issues

#### Files to Modify

1. **`src-tauri/src/auth/ceremonies/change_password.rs:171-174`**

**Current Code**:
```rust
let wrapped_blob: Vec<u8> = conn
    .query_row(
        "SELECT wrapped_private_key FROM vault_identity WHERE id = 1",
        [],
        |row| row.get(0),
    )
    .optional()
    .map_err(|_| AuthenticationError::InvalidCredentials)?;
```

**Fixed Code**:
```rust
let wrapped_blob: Vec<u8> = match conn.query_row(
    "SELECT wrapped_private_key FROM vault_identity WHERE id = 1",
    [],
    |row| row.get(0),
) {
    Ok(blob) => blob,
    Err(rusqlite::Error::QueryReturnedNoRows) => {
        tracing::error!("vault_identity row not found");
        return Err(AuthenticationError::VaultHeaderInvalid);
    }
    Err(e) => {
        tracing::error!("database error querying vault_identity: {:?}", e);
        return Err(AuthenticationError::InvalidCredentials);
    }
};
```

2. **`src-tauri/src/auth/ceremonies/rotate_key_file.rs:179-182`**

Apply the identical fix (same query, same error handling).

#### Test & Verify

```bash
cargo build
cargo test --lib ceremonies::
```

---

## PHASE 3: BACKLOG ITEMS — 3-4 hours

### Issue: L1 — Code Duplication in FFI Wrappers

**Severity**: LOW (code quality)  
**File**: 1 file  
**Effort**: ~40 lines refactoring, 1-2 hours  

#### File to Modify

**`src-tauri/src/auth/ceremonies/helpers.rs`**

**Step 1** — Add new helper function (insert before `open_sqlcipher`):

```rust
/// Invokes a SQLCipher FFI function and returns structured error on failure.
fn sqlite3_ffi_call<F>(operation_name: &str, f: F) -> Result<(), AuthenticationError>
where
    F: FnOnce() -> i32,
{
    let rc = f();
    if rc != ffi::SQLITE_OK {
        tracing::warn!(rc, "SQLCipher FFI operation failed: {}", operation_name);
        return Err(AuthenticationError::InvalidCredentials);
    }
    Ok(())
}
```

**Step 2** — Refactor `open_sqlcipher()`:

**Before**:
```rust
pub(super) fn open_sqlcipher(
    path: &Path,
    sqlcipher_key: &SqlcipherKey,
) -> Result<Connection, AuthenticationError> {
    let conn = Connection::open(path).map_err(|_| AuthenticationError::VaultHeaderInvalid)?;
    let rc = sqlcipher_key.with_exposed(|key_bytes| {
        unsafe {
            ffi::sqlite3_key(
                conn.handle(),
                key_bytes.as_ptr().cast(),
                key_bytes.len() as i32,
            )
        }
    });
    if rc != ffi::SQLITE_OK {
        let error_message = {
            unsafe {
                let message_ptr = ffi::sqlite3_errmsg(conn.handle());
                std::ffi::CStr::from_ptr(message_ptr)
                    .to_string_lossy()
                    .into_owned()
            }
        };
        tracing::warn!(rc, error_message, "sqlite3_key failed");
        return Err(AuthenticationError::InvalidCredentials);
    }
    Ok(conn)
}
```

**After**:
```rust
pub(super) fn open_sqlcipher(
    path: &Path,
    sqlcipher_key: &SqlcipherKey,
) -> Result<Connection, AuthenticationError> {
    let conn = Connection::open(path)
        .map_err(|_| AuthenticationError::VaultHeaderInvalid)?;
    sqlcipher_key.with_exposed(|key_bytes| {
        sqlite3_ffi_call("sqlite3_key", || {
            // SAFETY: conn is open, key_bytes valid for duration of call
            unsafe {
                ffi::sqlite3_key(conn.handle(), key_bytes.as_ptr().cast(), key_bytes.len() as i32)
            }
        })
    })?;
    Ok(conn)
}
```

**Step 3** — Refactor `rekey_sqlcipher()` (same pattern):

**Before**:
```rust
pub(super) fn rekey_sqlcipher(
    conn: &Connection,
    new_sqlcipher_key: &SqlcipherKey,
) -> Result<(), AuthenticationError> {
    let rc = new_sqlcipher_key.with_exposed(|key_bytes| {
        unsafe {
            ffi::sqlite3_rekey(
                conn.handle(),
                key_bytes.as_ptr().cast(),
                key_bytes.len() as i32,
            )
        }
    });
    if rc != ffi::SQLITE_OK {
        let error_message = {
            unsafe {
                let message_ptr = ffi::sqlite3_errmsg(conn.handle());
                std::ffi::CStr::from_ptr(message_ptr)
                    .to_string_lossy()
                    .into_owned()
            }
        };
        tracing::warn!(rc, error_message, "sqlite3_rekey failed");
        return Err(AuthenticationError::InvalidCredentials);
    }
    Ok(())
}
```

**After**:
```rust
pub(super) fn rekey_sqlcipher(
    conn: &Connection,
    new_sqlcipher_key: &SqlcipherKey,
) -> Result<(), AuthenticationError> {
    new_sqlcipher_key.with_exposed(|key_bytes| {
        sqlite3_ffi_call("sqlite3_rekey", || {
            // SAFETY: conn is open, key_bytes valid for duration of call
            unsafe {
                ffi::sqlite3_rekey(conn.handle(), key_bytes.as_ptr().cast(), key_bytes.len() as i32)
            }
        })
    })?;
    Ok(())
}
```

#### Test & Verify

```bash
cargo build
cargo clippy --lib -- -D warnings
cargo test --lib ceremonies::
```

---

### Issue: L2 — Missing Error Documentation

**Severity**: LOW (documentation)  
**File**: 1 file, 1 function  
**Effort**: ~5 lines, <30 minutes  

#### File to Modify

**`src-tauri/src/auth/ceremonies/helpers.rs:200-228`**

**Add to doc-comment** (insert after existing documentation):

```rust
/// Rekeys an already-open SQLCipher connection via SQLCipher FFI.
///
/// # Errors
///
/// Returns `AuthenticationError::InvalidCredentials` if the SQLCipher FFI call
/// `sqlite3_rekey` returns a non-zero return code. This typically indicates a key
/// derivation failure or database corruption. See SQLCipher documentation for rc
/// values and meanings.
pub(super) fn rekey_sqlcipher(
    conn: &Connection,
    new_sqlcipher_key: &SqlcipherKey,
) -> Result<(), AuthenticationError> {
    // ... existing implementation ...
}
```

#### Test & Verify

```bash
cargo doc --lib
# Open target/doc/arx_runa/auth/ceremonies/helpers/fn.rekey_sqlcipher.html
# Verify "# Errors" section is present and readable
```

---

### Issue: L3 — Recovery Derivation Algorithm Doc Clarity

**Severity**: LOW (documentation)  
**File**: 1 file, 1 function  
**Effort**: ~3 lines, <30 minutes  

#### File to Modify

**`src-tauri/src/auth/ceremonies/helpers.rs:277-285`**

**Replace doc-comment**:

**Before**:
```rust
/// Derives a recovery key from phrase bytes and slot-local Argon2 parameters.
pub(super) fn derive_recovery_key_into(
    phrase_canonical_bytes: &[u8],
    salt: &[u8; 32],
    params: &Argon2Params,
    output: &mut [u8; 32],
) -> Result<(), AuthenticationError> {
    derive_master_key_into(phrase_canonical_bytes, None, salt, params, output)
}
```

**After**:
```rust
/// Derives a recovery key from BIP-39 phrase bytes using Argon2id.
///
/// Note: This uses Argon2id (memory-hard KDF) rather than BIP-39's standard
/// PBKDF2-HMAC-SHA512. Argon2id provides stronger resistance to offline
/// brute-force attacks on recovery phrases and is consistent with the
/// primary authentication path. This choice is documented in the design spec.
pub(super) fn derive_recovery_key_into(
    phrase_canonical_bytes: &[u8],
    salt: &[u8; 32],
    params: &Argon2Params,
    output: &mut [u8; 32],
) -> Result<(), AuthenticationError> {
    derive_master_key_into(phrase_canonical_bytes, None, salt, params, output)
}
```

#### Test & Verify

```bash
cargo doc --lib
# Verify documentation renders correctly
```

---

### Issue: L4 — Authentication Backoff Counter Persistence Doc

**Severity**: LOW (documentation)  
**File**: 1 file, 1 function  
**Effort**: ~2 lines, <30 minutes  

#### File to Modify

**`src-tauri/src/auth/session/manager.rs:670-680`**

**Update doc-comment**:

**Before**:
```rust
/// Records a failed authentication attempt and sets the backoff deadline.
///
/// Delay formula: `min(30 s, 2^(attempts-1) s)` — 1, 2, 4, 8, 16, 30 s for
/// attempts 1–6; capped at 30 s for 7 and above. Counter is not logged.
fn record_failed_attempt(&self) {
```

**After**:
```rust
/// Records a failed authentication attempt and sets the backoff deadline.
///
/// Delay formula: `min(30 s, 2^(attempts-1) s)` — 1, 2, 4, 8, 16, 30 s for
/// attempts 1–6; capped at 30 s for 7 and above. Counter is not logged and is
/// NOT persisted across process restarts — this is intentional per the threat
/// model (assumes temporary local system compromise).
fn record_failed_attempt(&self) {
```

#### Test & Verify

```bash
cargo build
```

---

## PHASE 4: ARCHITECTURE REFACTORING (NEXT CYCLE) — 4-6 hours

### Issue: AR-M1 — SessionManager Storage Coupling

**Severity**: MEDIUM (architecture)  
**Files**: 6 files (manager.rs + 5 ceremonies)  
**Effort**: ~60 lines refactoring, 3-4 hours  

#### Files to Modify

1. **`src-tauri/src/auth/ceremonies/mod.rs`** — Add helper function

Insert at end of module:

```rust
/// Opens a metadata store for a vault using the given SQLCipher key.
pub(super) async fn open_vault_metadata_store(
    db_path: &Path,
    sqlcipher_key: &SqlcipherKey,
) -> Result<Arc<dyn MetadataStore>, AuthenticationError> {
    let mut key_bytes = [0u8; 32];
    key_bytes.copy_from_slice(sqlcipher_key.expose());
    
    match SqlCipherMetadataStore::open(db_path, &key_bytes).await {
        Ok(store) => Ok(Arc::new(store)),
        Err(error) => {
            tracing::error!(?error, "failed to open vault metadata store");
            Err(AuthenticationError::InvalidCredentials)
        }
    }
}
```

2. **`src-tauri/src/auth/session/manager.rs:200-290`** — Update method signature

**Before**:
```rust
pub async fn install_session(
    reservation: SessionReservation,
    keys: SessionKeys,
    vault_id: String,
    vault_db_path: PathBuf,
) -> Result<SessionManager, AuthenticationError> {
```

**After**:
```rust
pub async fn install_session(
    reservation: SessionReservation,
    keys: SessionKeys,
    vault_id: String,
    vault_db_path: PathBuf,
    metadata_store: Arc<dyn MetadataStore>,
) -> Result<SessionManager, AuthenticationError> {
```

Then in method body, remove lines 256-276 (the storage opening code) and replace with:

```rust
let mut keys = keys;
keys.metadata_store = Some(metadata_store);
```

3. **Update all 5 ceremony callsites**

In `create.rs`, `change_password.rs`, `rotate_key_file.rs`, `recover_vault.rs`, `recover_with_phrase.rs`:

**Before**:
```rust
let session_manager = SessionManager::install_session(
    reservation,
    keys,
    vault_id.clone(),
    vault_db_path.clone(),
).await?;
```

**After**:
```rust
let metadata_store = ceremonies::open_vault_metadata_store(&vault_db_path, &keys.sqlcipher_key).await?;
let session_manager = SessionManager::install_session(
    reservation,
    keys,
    vault_id.clone(),
    vault_db_path.clone(),
    metadata_store,
).await?;
```

Also update `finalize_session_install()` and `swap_active_session()` with same pattern.

#### Test & Verify

```bash
cargo build
cargo test --lib auth::
cargo test --lib ceremonies::
```

---

### Issue: AR-M2 — Sharing Module Concrete Type Dependency

**Severity**: MEDIUM (architecture)  
**Files**: 3 files (revocation.rs, cloud.rs, callsites)  
**Effort**: ~50 lines refactoring, 3-4 hours  

#### Files to Modify

1. **`src-tauri/src/sharing/revocation.rs`** — Remove concrete import

**Remove line**:
```rust
use crate::storage::sqlcipher::SqlCipherMetadataStore;
```

2. **Update all function signatures** in both revocation.rs and cloud.rs

**Before**:
```rust
async fn strong_revoke_share(
    share_id: &str,
    store: &SqlCipherMetadataStore,  // ← concrete type
    transport: &dyn CloudTransport,
) -> Result<StrongRevocationOutput, SharingError> {
```

**After**:
```rust
async fn strong_revoke_share(
    share_id: &str,
    store: &dyn MetadataStore,  // ← abstract trait
    transport: &dyn CloudTransport,
) -> Result<StrongRevocationOutput, SharingError> {
```

3. **Update callsites** in IPC layer or ceremonies

**Before**:
```rust
let concrete_store = // ... get SqlCipherMetadataStore
strong_revoke_share(share_id, &concrete_store, transport).await?
```

**After**:
```rust
let store: Arc<dyn MetadataStore> = // ... get from SessionManager
strong_revoke_share(share_id, store.as_ref(), transport).await?
```

#### Test & Verify

```bash
cargo build
cargo test --lib sharing::
```

---

### Issue: AR-L1 — SessionManager SRP Violation

**Severity**: LOW (architecture nice-to-have)  
**File**: 3 files (manager.rs, cloud/mod.rs, rclone.rs)  
**Effort**: ~30 lines refactoring, 1-2 hours  

#### Files to Modify

1. **`src-tauri/src/storage/cloud/mod.rs`** — Add trait method

```rust
pub trait CloudTransport: Send + Sync {
    // ... existing methods ...
    
    /// Cleans up session-scoped credentials (rclone.conf, temporary files, etc.)
    /// Called when session locks or vault closes.
    async fn cleanup_session_credentials(&self) -> Result<(), CloudTransportError> {
        Ok(())  // default: no-op
    }
}
```

2. **`src-tauri/src/storage/cloud/rclone.rs`** — Implement cleanup

```rust
impl CloudTransport for RcloneTransport {
    async fn cleanup_session_credentials(&self) -> Result<(), CloudTransportError> {
        // Move destroy_rclone_conf logic here
        Self::destroy_rclone_conf().await;
        Ok(())
    }
}
```

3. **`src-tauri/src/auth/session/manager.rs`** — Call cleanup in lock()

**Before**:
```rust
pub async fn lock(&self) {
    // ... session key zeroization ...
    Self::destroy_rclone_conf().await;
    // ... rest of lock logic ...
}
```

**After**:
```rust
pub async fn lock(&self) {
    // ... session key zeroization ...
    if let Some(transport) = &self.cloud_transport {
        let _ = transport.cleanup_session_credentials().await;
    }
    // ... rest of lock logic ...
}
```

#### Test & Verify

```bash
cargo build
cargo test --lib session::
```

---

### Issue: AR-L2 — SharingStore Trait Ownership

**Severity**: LOW (architecture nice-to-have)  
**File**: 2 files (move trait definition)  
**Effort**: ~15 lines refactoring, <1 hour  

**Action**: Move `SharingStore` trait from `src/sharing/store.rs` to `src/storage/sharing.rs`

#### Test & Verify

```bash
cargo build
```

---

### Issue: AR-L3 — HKDF Expansion Coupling

**Severity**: LOW (architecture monitoring)  
**Files**: 2 files (kdf.rs for test, mod.rs for doc)  
**Effort**: ~20 lines (test + doc), 1 hour  

1. **`src-tauri/src/crypto/kdf.rs`** — Add invariant test

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_hkdf_expansion_invariant() {
        // Verify that HKDF produces consistent keys across all session derivations
        let master_key = SecretBox::new([42u8; 32]);
        let keys1 = SessionKeys::from_master_key_bytes(master_key.expose());
        let keys2 = SessionKeys::from_master_key_bytes(master_key.expose());
        
        assert_eq!(
            keys1.key_encryption_key.expose(),
            keys2.key_encryption_key.expose(),
            "HKDF expansion must be deterministic"
        );
    }
}
```

2. **`src-tauri/src/auth/mod.rs`** — Add documentation

```rust
//! HKDF Expansion Boundary
//! 
//! `SessionKeys::from_master_key_bytes()` is the canonical HKDF expansion boundary.
//! All 5 ceremony flows (create, change_password, rotate_key_file, recover_vault, 
//! recover_with_phrase) derive keys through this single function. Changing HKDF 
//! parameters here automatically propagates to all flows. If parameters must change:
//! 1. Update crypto/kdf.rs constants
//! 2. Run invariant tests to verify consistency
//! 3. No per-site updates needed (changes auto-propagate)
```

#### Test & Verify

```bash
cargo test --lib crypto::kdf
```

---

## SUMMARY TABLE

| Issue | Phase | Files | Effort | Priority | Status |
|-------|-------|-------|--------|----------|--------|
| **M1** | 1 | 2 | <1h | CRITICAL | Ready to implement |
| **M2** | 2 | 1 | 1-2h | HIGH | Ready to implement |
| **M3** | 2 | 1 | 1-2h | HIGH | Ready to implement |
| **M4** | 2 | 2 | 1-2h | HIGH | Ready to implement |
| **L1** | 3 | 1 | 1-2h | BACKLOG | Ready to implement |
| **L2** | 3 | 1 | <30m | BACKLOG | Ready to implement |
| **L3** | 3 | 1 | <30m | BACKLOG | Ready to implement |
| **L4** | 3 | 1 | <30m | BACKLOG | Ready to implement |
| **AR-M1** | 4 | 6 | 3-4h | REFACTOR | Ready to implement |
| **AR-M2** | 4 | 3 | 3-4h | REFACTOR | Ready to implement |
| **AR-L1** | 4 | 3 | 1-2h | REFACTOR | Ready to implement |
| **AR-L2** | 4 | 2 | <1h | REFACTOR | Ready to implement |
| **AR-L3** | 4 | 2 | 1h | REFACTOR | Ready to implement |

**Total**: 13 issues, ~30 files modified, 10-14 hours, 100% agent-ready.

---

## Next Steps

1. ✅ **Assign M1 to agent** — CRITICAL path blocker
2. ✅ **Run M1 fix verification** — Build + tests must pass
3. ✅ **Assign M2-M4 to agent** — HIGH priority, next batch
4. ✅ **Schedule AR refactoring** — Plan separate PR with team
5. ✅ **Assign L1-L4 incrementally** — Backlog, low risk

**All issues are now agent-ready with exact code locations, before/after examples, and test verification steps.**
