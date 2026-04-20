# Authentication and Session Management — Sub-Phase Roadmap

**Parent design**: [`design.md`](../design.md)  
**Contract anchor**: [`design.md#contract-surface`](../design.md#contract-surface) is canonical for interface/data/invariant/dependency contracts; roadmap and sub-phases should reference it instead of duplicating full contract payloads.  
**Created**: 2026-04-04  
**Status**: Draft  
**Implementation order**: 2.1 → 2.2 → 2.3 → 2.4 (strict dependencies)

---

## Overview

This sub-phase roadmap decomposes the authentication and session management design (432 lines) into 4 independently testable implementation units, enabling incremental validation of the two-factor authentication layer before implementing vault ceremonies.

**Total sub-phases**: 4

**Rationale for decomposition**:
-  **Size**: Exceeds ~100-150 lines (382 lines total)
-  **Trait boundaries**: `KeySource` and `DeviceMonitor` traits implementable independently of the KDF layer
-  **Platform splits**: OS-specific implementations for device monitoring (`WindowsDeviceMonitor` / `LinuxDeviceMonitor`)
-  **Integration breadth**: Touches auth module, crypto module (BLAKE3, Argon2id, HKDF), storage module (SQLCipher), cloud module (vault header upload)
-  **Error surface**: Defines `AuthenticationError` enum plus `KeySourceError` requiring separate test coverage
-  **Multi-step flows**: Vault creation (21 steps), password change, USB key file rotation, new-device recovery

**Implementation strategy**: Build key file trait and device monitoring → add Argon2id KDF and memory-locked session keys → implement session lifecycle and timeout → implement vault ceremonies

---

## Dependency Graph

```
2.1 (USB key file + DeviceMonitor)
 ↓
2.2 (Argon2id + SessionKeys)
 ↓
2.3 (Session lifecycle + timeout)
 ↓
2.4 (Vault ceremonies)
```

**Legend**:
- `→` strict dependency (must complete predecessor before starting)

Note: Phase 2.4 also carries a cross-phase dependency on Phase 3.1 (SQLCipher schema). The vault creation ceremony requires the schema to be defined before the SQLCipher DB can be created with its table structure.

---

## Sub-Phases

1. **[Phase 2.1: USB Key File Format and DeviceMonitor](2.1-usb-key-file-and-device-monitor.md)**
   - `KeySource` and `DeviceMonitor` trait definitions
   - `FileKeySource`, `MockKeySource`, platform monitors, `MockDeviceMonitor`
   - Auto-detection logic via BLAKE3 fingerprint matching
   - Local path hint (last-used key file path in local config)
   - **Estimated**: ~180 lines production code, ~100 lines tests

2. **[Phase 2.2: Argon2id KDF and SessionKeys](2.2-argon2id-and-session-keys.md)**
   - Argon2id wrapper with `password_utf8 || key_file_bytes` input
   - HKDF expansion to `key_encryption_key`, `sqlcipher_key`, `manifest_key`
   - `SessionKeys` struct with `ZeroizeOnDrop` and memory locking
   - `AuthenticationError` enum
   - **Estimated**: ~150 lines production code, ~120 lines tests

3. **[Phase 2.3: Session Lifecycle and Timeout](2.3-session-lifecycle-and-timeout.md)**
   - `SessionManager` state machine (No session → Active → Expired)
   - Activity-based timeout with configurable duration
   - `tokio` background timer with operation-in-progress gate
   - 60-second pre-warning signal to frontend
   - **Estimated**: ~150 lines production code, ~120 lines tests

4. **[Phase 2.4: Vault Ceremonies](2.4-vault-ceremonies.md)**
   - Vault creation (21-step ceremony)
   - Password change and USB key file rotation (re-wrap, no chunk re-encryption)
   - New-device recovery from cloud vault header and manifest backup
   - **Estimated**: ~300 lines production code, ~200 lines tests

---

## Testing Strategy

### Per-Sub-Phase Testing
Each sub-phase includes its own test suite. Tests must pass before proceeding to the next sub-phase.

**Test types**:
- **Unit tests**: Core functionality in isolation (trait methods, error mapping, KDF correctness)
- **Mock-based tests**: Use `MockKeySource` and `MockDeviceMonitor` for phases that depend on hardware (Phases 2.1, 2.2, 2.3)
- **Property-based tests**: Use `proptest` for KDF determinism verification (same inputs → same outputs) and invalid-input rejection
- **Integration tests**: Once all sub-phases complete, end-to-end vault creation → authentication → timeout round-trip

### Regression Testing
After completing each sub-phase, run:
```bash
cargo test auth           # All auth module tests must pass
cargo clippy -- -D warnings  # No new warnings
```

### Manual Testing Checklist
- Phase 2.1: Insert a USB drive; verify `DeviceMonitor` fires `Mounted` event and auto-detection finds a planted 32-byte file
- Phase 2.2: Confirm `mlock`/`VirtualLock` is applied (inspect `/proc/<pid>/status` on Linux: `VmLck` increases)
- Phase 2.3: Trigger timeout via inactivity; confirm keys are zeroed and SQLCipher connection is closed
- Phase 2.4: Full vault creation → new-device recovery round-trip using a cloud provider

---

## Security Review Checkpoints

- **Phase 2.1**: Not required — no crypto operations beyond BLAKE3 hashing (preimage-resistant, non-secret)
- **Phase 2.2**: Required — Invoke `security-reviewer` agent after implementation (Argon2id params correctness, mlock enforcement, `master_key` lifetime)
- **Phase 2.3**: Required — Invoke `security-reviewer` agent after implementation (timeout correctness, zeroization on expiry, operation-in-progress gate)
- **Phase 2.4**: Required — Invoke `security-reviewer` agent after implementation (ceremony correctness, re-wrapping logic, `master_key` lifetime in all flows)

---

## Notes

### Design Clarifications
- **Input concatenation**: `password_utf8 || key_file_bytes` is unambiguous because the key file is always exactly 32 bytes. The split point is deterministic at `total_length - 32`. No length-prefix is needed.
- **Salt reuse**: a new 32-byte CSPRNG salt is mandatory whenever any KDF input changes (password change or key file rotation). Reusing a salt with a different password is explicitly prohibited.
- **Cross-phase dependency**: Phase 2.4 depends on Phase 3.1 (SQLCipher schema) for the vault creation step that creates DB tables. Phase 2.4 can be started but cannot be fully tested until Phase 3.1 is complete.

### Future Work
- Argon2id parameter upgrade policy for existing vaults (deferred — design stores params in vault header for future use)
- Session timeout behaviour during active upload verified during Phase 6 integration testing

### Risks and Mitigations

| Risk | Mitigation |
|------|------------|
| `mlock` fails on resource-constrained systems | Hard fail with platform-specific error message; required memory is < 1 KiB, well within default limits |
| USB auto-detection fires on non-USB removable volumes | Scope monitoring to removable media (`DEVTYPE=partition` on Linux, `DBTF_NET` exclusion on Windows) |
| Cross-phase dependency (2.4 → 3.1) blocks completion | Implement Phase 2.4 ceremony logic with a stub schema; integrate fully once Phase 3.1 is done |
| `master_key` escape via compiler optimisation preventing zeroize | Use `zeroize` crate's `Zeroizing<T>` wrapper, which calls `volatile_set` to prevent optimisation |

---

## References

- **Parent design**: `docs/architecture/designs/authentication-and-session-management/design.md`
- **Roadmap entry**: `docs/roadmap.md` Phase 2
- **Related phases**: Phase 1.1 (HKDF derivation reused in 2.2), Phase 1.3 (BLAKE3 used in 2.1), Phase 3.1 (SQLCipher schema required by 2.4), Phase 4 (cloud upload required by 2.4)
