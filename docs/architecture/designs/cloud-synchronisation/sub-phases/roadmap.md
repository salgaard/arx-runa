# Cloud Synchronisation — Sub-Phase Roadmap

**Parent design**: [`design.md`](../design.md)  
**Contract anchor**: [`design.md#contract-surface`](../design.md#contract-surface) is canonical for interface/data/invariant/dependency contracts; roadmap and sub-phases should reference it instead of duplicating full contract payloads.  
**Created**: 2026-04-02  
**Status**: Draft  
**Implementation order**: 4.1 → 4.2 → 4.3 → 4.4 → 4.5 (strict dependencies)

---

## Overview

This sub-phase roadmap decomposes the cloud synchronisation design (722 lines) into 5 independently testable implementation units, enabling incremental validation of the cloud storage layer before moving to complex flows.

**Total sub-phases**: 5

**Rationale for decomposition**:
-  **Size**: Exceeds ~100-150 lines (722 lines total)
-  **Trait boundaries**: `CloudTransport` trait → `MockTransport` → `RcloneTransport` → vault header/manifest operations
-  **Integration breadth**: Touches storage module, introduces Rclone subprocess, cloud config management, conflict detection
-  **Error surface**: Defines 7 distinct `CloudTransportError` variants requiring separate test coverage
-  **Multi-step flows**: Upload/download flows, push/pull flows, conflict detection, garbage collection

**Implementation strategy**: Build foundational trait with mock → integrate Rclone → implement bootstrap structures (vault header) → implement backup mechanisms (manifest) → add conflict detection and cloud flows

---

## Dependency Graph

```
4.1 (CloudTransport trait + MockTransport)
 ↓
4.2 (Rclone integration + provider setup)
 ↓
4.3 (Vault header upload/download)
 ↓
4.4 (Manifest cloud backup)
 ↓
4.5 (Push/pull flows + conflict detection)
```

**Legend**:
- `→` strict dependency (must complete predecessor before starting)

---

## Sub-Phases

1. **[Phase 4.1: CloudTransport Trait and Mock Implementation](4.1-cloud-transport.md)**
   - CloudTransport trait definition
   - MockTransport implementation for testing
   - Error handling and endpoint configuration
   - **Estimated**: ~150 lines production code, ~100 lines tests

2. **[Phase 4.2: Rclone Integration and Provider Setup](4.2-rclone-integration.md)**
   - RcloneTransport implementation
   - Rclone sidecar bundling
   - Guided setup wizard (S3, Google Drive)
   - Path and stderr sanitisation
   - **Estimated**: ~350 lines production code, ~150 lines tests

3. **[Phase 4.3: Vault Header Upload and Download](4.3-vault-header.md)**
   - VaultHeader struct and serialization
   - Upload/download flows
   - Validation logic
   - **Estimated**: ~120 lines production code, ~80 lines tests

4. **[Phase 4.4: Manifest Cloud Backup](4.4-manifest-backup.md)**
   - Manifest encryption and upload
   - Download and decrypt for recovery
   - SQLCipher integration
   - **Estimated**: ~150 lines production code, ~100 lines tests

5. **[Phase 4.5: Push/Pull Flows and Conflict Detection](4.5-push-pull-flows.md)**
   - Full push/pull implementation
   - Conflict detection with snapshot_counter
   - Upload order randomization
   - Cloud garbage collection
   - **Estimated**: ~400 lines production code, ~250 lines tests

---

## Testing Strategy

### Per-Sub-Phase Testing
Each sub-phase includes its own test suite. Tests must pass before proceeding to the next sub-phase.

**Test types**:
- **Unit tests**: Core functionality in isolation (trait methods, error mapping, validation)
- **Mock-based tests**: Use `MockTransport` for dependencies not yet implemented (Phases 4.1, 4.3, 4.4)
- **Integration tests**: Use local Rclone filesystem remote (Phase 4.2), real cloud provider (Phases 4.3-4.5)
- **Property-based tests**: Upload order randomization (Phase 4.5)

### Regression Testing
After completing each sub-phase, run:
```bash
cargo test storage::cloud  # All cloud module tests must pass
cargo clippy -- -D warnings  # No new warnings
```

### Manual Testing Checklist
- Phase 4.2: Setup wizard creates valid `rclone.conf` and `cloud-config.json`
- Phase 4.3: Vault header is plaintext JSON in cloud
- Phase 4.4: Manifest backup is encrypted (not plaintext SQLite)
- Phase 4.5: Push → pull round-trip restores full vault on new device

---

## Security Review Checkpoints

- **Phase 4.2**: Requires `security-reviewer` agent review (subprocess security, credential handling, path sanitisation)
- **Phase 4.4**: Requires `security-reviewer` agent review (manifest encryption, zeroization)
- **Phase 4.5**: Requires `security-reviewer` agent review (conflict detection correctness, BLAKE3 verification)

---

## Implementation Workflow

```bash
# Phase 4.1
/plan 4.1
/implement-plan phase-004-1-cloud-transport.md
cargo test storage::cloud::mock_transport
# [Manual verification checkpoint]

# Phase 4.2
/plan 4.2
/implement-plan phase-004-2-rclone-integration.md
cargo test storage::cloud::rclone
# [Manual verification checkpoint - test with real cloud provider]

# ... continue for phases 4.3, 4.4, 4.5
```

---

## Notes

- **Manifest size exception**: Manifest backup loads entire DB into memory (exception to streaming rule). Typical size <10 MiB, acceptable for in-memory encryption.
- **No auto-merge**: Conflict resolution is manual. Arx Runa detects conflicts but does not attempt automatic merge.
- **Future work (Phase 5)**: `shared/` directory structure is defined but not implemented until Phase 5 (file sharing)
