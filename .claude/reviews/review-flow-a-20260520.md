---
flow: A
date: 2026-05-20
reviewer: claude-sonnet-4-6
invariants: 3, 16, 17
status: complete
---

# Flow A — Key Derivation & Session Memory Lifecycle

## Findings

### [FLOW-A-001] Redundant `Zeroizing<[u8; 32]>` stack copies of `sqlcipher_key`
**Severity**: high
**Invariant**: 16 (SQLCipher key handling)
**Location**:
- `src-tauri/src/auth/session/manager.rs:483` (`finalize_session_install`)
- `src-tauri/src/auth/ceremonies/create.rs:237` (`create_vault`)

**Observation**:
`finalize_session_install` copies the sqlcipher key into an intermediate `Zeroizing<[u8; 32]>` before passing it to `SqlCipherMetadataStore::open`:
```rust
let sqlcipher_key_bytes: Zeroizing<[u8; 32]> = {
    let mut key_bytes = Zeroizing::new([0u8; 32]);
    key_bytes.copy_from_slice(keys.sqlcipher_key.expose());
    key_bytes
};
// …
SqlCipherMetadataStore::open(&db_path, &sqlcipher_key_bytes).await
```

`create_vault` has an analogous pattern:
```rust
let dest_key = Zeroizing::new(*session_keys.sqlcipher_key.expose());
let dest_db = SqlCipherMetadataStore::open(&request.vault_db_path, &dest_key).await;
drop(dest_key);
```

`Zeroizing<[u8; 32]>` wraps a 32-byte array value. At the 32-byte size Rust typically keeps this on the call stack (or in register-spill slots), meaning the key bytes exist in non-mlocked memory for the duration of the `open` call. `SqlCipherMetadataStore::open` already accepts `&[u8; 32]`, so the intermediate binding is unnecessary — `keys.sqlcipher_key.expose()` / `session_keys.sqlcipher_key.expose()` could be passed directly.

The `SqlcipherKey::from_slice` docstring explicitly documents that `SecretBox` copies (heap, non-mlocked) are the accepted transient protection level for this type. A stack-allocated `Zeroizing<[u8; 32]>` is weaker than that because the OS can't page it out but a debugger, crash dump, or stack-scanning attack sees it without any indirection. The principal value of the `SecureBytes` model is that the bytes live only in mlocked heap pages; these intermediate copies bypass that.

**Violation**: The sqlcipher key bytes appear in a raw stack array (wrapped only in `Zeroizing`) for the duration of an `async` suspension point (`SqlCipherMetadataStore::open(...).await`). During that suspension the Rust runtime may move the future (including the stack frame) across threads; the bytes are not in any mlocked region at that point.

**Recommendation**: Pass `expose()` directly in both sites:
```rust
// finalize_session_install:
SqlCipherMetadataStore::open(&db_path, keys.sqlcipher_key.expose()).await

// create_vault:
SqlCipherMetadataStore::open(&request.vault_db_path, session_keys.sqlcipher_key.expose()).await
```
No intermediate binding is needed. The borrow on `keys.sqlcipher_key` ends before the mutable assignment to `keys.metadata_store`, so the borrow checker accepts it.

**Test coverage**: none

---

## Design Gaps

### [DESIGN-GAP] Intermediate `VaultKeys` in non-mlocked memory during HKDF expansion

**Location**: `src-tauri/src/crypto/hkdf.rs::expand_into_secret_box` → `src-tauri/src/auth/session/keys.rs::from_master_key_bytes`

**Observation**: `derive_vault_keys` expands each HKDF output into a `secrecy::SecretBox<[u8; 32]>` (heap-allocated, NOT mlocked), bundled in `VaultKeys`. `from_master_key_bytes` then copies from each field into the three `SecureBytes<32>` (mlocked). There is a window between HKDF expansion and `VaultKeys` being dropped where all three derived key bytes live in swappable heap pages.

This pattern is explicitly documented in the `SqlcipherKey::from_slice` docstring: "SecretBox does not mlock the allocation; the page is swappable until SqlcipherKey drops and zeroizes. True mlock requires a custom allocator — this is the accepted protection level for this type."

**Assessment**: Not a finding for the current design. The design acknowledges that transient HKDF output buffers are not mlocked; only the long-lived `SessionKeys` fields (via `SecureBytes`) are mlocked. The `VaultKeys` window is very short and ends well before the session is installed.

**Crate clarification**: The code uses the `secrecy` crate (`use secrecy::SecretBox`), which provides heap allocation + zeroize-on-drop but **no** `mlock`. This is distinct from the `secrets` crate (`docs.rs/secrets`), whose `SecretBox` calls `mlock(2)`, adds `PROT_NONE` guard pages, and dynamically changes memory protection to `PROT_READ`/`PROT_WRITE` only on explicit borrow. Using `secrets::SecretBox` for `VaultKeys` fields would eliminate this gap — the derived bytes would be mlocked for their entire lifetime, not just after the copy to `SecureBytes`. Feasibility depends on whether `secrets` supports Windows (`VirtualLock` equivalent); this should be evaluated if tightening the transient key window is a priority.

---

## Confirmed invariants (no findings)

| Check | Result | Notes |
|---|---|---|
| HKDF salt is `b"arx-runa-v1"` at every call site | **PASS** | Defined as `pub(crate) const HKDF_SALT` in `crypto/hkdf.rs:10`; used by every `expand_vault_key_into` call — no out-of-band HKDF call sites exist |
| HKDF info strings are exactly `b"arx-runa-key-encryption"`, `b"arx-runa-sqlcipher"`, `b"arx-runa-manifest-backup"` | **PASS** | All three defined as constants in `crypto/hkdf.rs:12-18`; no other info strings exist in the file or referenced elsewhere |
| No new info strings share a value with existing ones | **PASS** | Three distinct ASCII prefixed strings; no overlap |
| `sqlcipher_key` never by-value on the stack through `apply_sqlcipher_key` | **PASS** | `apply_sqlcipher_key` takes `&SqlcipherKey`, uses `with_exposed()` closure and passes the pointer directly to `sqlite3_key` FFI — no copy |
| `SessionKeys` has `mlock`/`VirtualLock` called immediately on derivation | **PASS** | `SecureBytes::new()` calls `platform::lock_memory()` before returning; construction of `SessionKeys` via `from_master_key_bytes` calls `SecureBytes::<32>::new()` for each key |
| `SessionKeys` fields zeroize on drop | **PASS** | Each `SecureBytes<32>` field has an explicit `Drop` impl: calls `zeroize()` then `unlock_memory()`; test `test_secure_bytes_drop_zeroizes_buffer_before_unlock` verifies ordering |
| `master_key` zeroized immediately after session keys are installed | **PASS** | `create_vault` calls `drop(master_key)` explicitly after `SessionKeys::from_master_key_bytes` returns (line ~168); `SessionKeys::derive` scopes `master_key: Zeroizing<[u8; 32]>` to the function body |
| Unix `mlock` failure non-silent | **PASS** | `memory/platform/unix.rs::lock_memory` returns `Err(MemoryLockError::PlatformFailure { platform_message })` on non-zero `libc::mlock` return |
| Windows `VirtualLock` failure non-silent | **PASS** | `memory/platform/windows.rs::lock_memory` returns `Err(MemoryLockError::PlatformFailure { platform_message })` when `VirtualLock` returns non-OK |
| Windows DACL `D:P(A;;FA;;;OW)(A;;FA;;;SY)(A;;FA;;;BA)` granting SYSTEM and BA is documented as accepted limitation | **PASS** | `set_file_private_permissions` docstring (`platform/permissions.rs:11-29`) explicitly names the SDDL string, explains SYSTEM+BA inclusion (Defender/VSS/recovery), and cites `docs/architecture/design-invariants.md §"Out-of-Scope Architectural Limitations"` |

---

## Summary

| Severity | Count |
|---|---|
| Critical | 0 |
| High | 1 |
| Medium | 0 |
| Low | 0 |

**Invariant 3** (HKDF constants): fully confirmed, no findings.
**Invariant 16** (SQLCipher key handling): one high finding — two redundant `Zeroizing<[u8; 32]>` stack copies where `expose()` suffices.
**Invariant 17** (mlock + zeroize): fully confirmed, no findings. Both platform implementations handle lock failure non-silently; zeroize-before-unlock ordering is tested.

**Recommendation**: Fix [FLOW-A-001] before the next release — it is a one-line change in each location and closes the only gap against Invariant 16. No other follow-up session required for this flow.
