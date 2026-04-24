# Phase 7 Roadmap (Planning Document)

> **Status**: Stub — Populated during Phase 7 kickoff  
> **Last updated**: 2026-04-23

## Overview

This document outlines the planned features, enhancements, and research items for Phase 7 (End-to-End Integration Testing and Beyond). It is populated from the [Deferred Items Inventory](deferred-items-inventory.md) and serves as the starting point for Phase 7 sprint planning.

---

## Priority Tier 1: Must-Have (Unblocks other features)

### 1. Multi-Vault Support

**Blocked items**: Transparent vault switching, backup/restore across vaults  
**Estimated effort**: 13 story points  
**Design dependencies**: [Authentication & Session Management](designs/authentication-and-session-management/design.md) §Session Model

**Work breakdown**:
- [ ] Design per-device session coordination (SessionManager → SessionRegistry)
- [ ] Refactor `AppState` to hold `Map<VaultId, VaultHandle>`
- [ ] Implement UI vault switcher / quick-select
- [ ] Update all IPC commands to accept `vault_id` parameter (or route via active session)
- [ ] Test switching between vaults with active sync operations

**Design questions**:
- Should only one vault be active per session, or allow concurrent operations?
- How does sync status change when switching vaults?
- Do partial-completion sync operations survive vault switches?

---

### 2. Advanced Recovery UI

**Blocked items**: Multi-device vault restore workflows  
**Current state**: Backend fully implemented; Phase 6.7 explicitly deferred UI  
**Estimated effort**: 8 story points  
**Design dependencies**: [Cloud Synchronisation](designs/cloud-synchronisation/design.md) §Push/Pull Flows

**Work breakdown**:
- [ ] Implement UI for `recover_from_cloud` command (device bootstrap flow)
- [ ] Implement UI for `migrate_vault` command (cross-device data transfer)
- [ ] Add recovery progress streaming to sync monitor
- [ ] Test recovery on fresh device without prior vault data

**Backend readiness**: ✅ All commands wired and tested

---

### 3. File-Level Conflict Resolution

**Blocked items**: Multi-device sync with conflicting local changes  
**Current state**: Detect-and-block only (intentional MVP)  
**Estimated effort**: 8 story points  
**Design dependencies**: [Cloud Synchronisation](designs/cloud-synchronisation/design.md) §Conflict Detection

**Work breakdown**:
- [ ] Research three-way merge strategies for encrypted blobs
- [ ] Design file-level timestamp comparison (pre-pull check)
- [ ] Implement conflict detection and reporting UI
- [ ] Define UX for manual conflict resolution (keep local, keep remote, keep both)
- [ ] Test concurrent edits on same file across devices

**Research questions**:
- Can we safely compare encrypted modification times?
- Should users see just "conflict detected" or full diff of encrypted metadata?
- How long should conflict history be retained?

---

## Priority Tier 2: High-Value Enhancements (Independent)

### 4. Fingerprint Trust Model

**Depends on**: Nothing (Phase 5.1 provides foundation)  
**Estimated effort**: 5 story points  
**Design dependencies**: [File Sharing](designs/file-sharing/design.md) §Fingerprint

**Work breakdown**:
- [ ] Store contact verification timestamp in `contacts` table
- [ ] Add "verified" badge to contact list when fingerprint was out-of-band verified
- [ ] Warn when sharing with new/unverified contacts (optional auto-dismiss)
- [ ] Maintain verification history (did you verify, when, any notes)

**Current state**: Phase 5.1 displays fingerprint; Phase 6.8 shows 16-hex format in UI

---

### 5. Directory Operations (Recursive Delete & Moves)

**Depends on**: Nothing (UI addition + MetadataStore extension)  
**Estimated effort**: 6 story points  
**Design dependencies**: [Chunking & Manifest](designs/chunking-and-manifest/design.md) §Schema

**Work breakdown**:
- [ ] Add `delete_directory` IPC command with recursive flag
- [ ] Implement `MetadataStore::delete_directory` (transactional cascade)
- [ ] Add UI "Delete Folder" button with confirmation
- [ ] Enqueue orphaned chunks into `pending_deletions` for sync cleanup
- [ ] Test deletion of deeply nested directories with many files

**Complexity**: Transactional safety — must not leave orphaned blobs if delete is interrupted

---

### 6. Video EXIF Stripping

**Depends on**: Nothing (preprocessing pipeline extension)  
**Estimated effort**: 5 story points  
**Design dependencies**: [Chunking & Manifest](designs/chunking-and-manifest/design.md) §EXIF Stripping

**Work breakdown**:
- [ ] Research MP4 moov-atom handling (two-pass read vs temporary spool)
- [ ] Implement video EXIF detection (magic bytes for MP4/WebM/MOV)
- [ ] Update `detect_and_strip_exif` to handle video containers
- [ ] Test with sample videos; measure memory overhead of spool strategy
- [ ] Document in design why certain video formats are unsupported

**Technical challenge**: MP4 moov atom is at EOF; streaming read cannot process it without seeking or buffering

---

### 7. Strong Revocation (Key Rotation)

**Depends on**: Nothing (option flag on share revoke)  
**Estimated effort**: 4 story points  
**Design dependencies**: [File Sharing](designs/file-sharing/design.md) §Revocation Semantics

**Work breakdown**:
- [ ] Add "strong revocation" checkbox to revoke modal
- [ ] Implement `rotate_file_key` and re-encrypt all chunks (transactional)
- [ ] Generate new `file_share_id`, republish share package
- [ ] Enqueue old `blob_name`s into `pending_deletions`
- [ ] Test strong revocation under active downloads (in-flight ops should fail)

**Current state**: Default revocation blocks future access but doesn't claim plaintext recall

---

### 8. Performance Optimization (Baseline + Incremental)

**Depends on**: Phase 7 profiling results  
**Estimated effort**: 7 story points (to be refined after profiling)  

**Work breakdown**:
- [ ] Profile Phase 6.9 build with realistic vault (10k files, 1 GB total)
  - Identify hot paths: sync queries, chunk upload/download, metadata loads
  - Measure memory usage under concurrent operations
- [ ] Implement partial indexes on frequently queried columns:
  - `shares(sender_vault_id, revoked_at)` for sent-shares queries
  - `received_shares(recipient_vault_id, imported_at)` for import tracking
- [ ] Evaluate chunk download caching (in-memory LRU, size limits)
- [ ] Optimize metadata queries (lazy loading, pagination)

**Metrics to collect**:
- Sync time for 1GB vault (baseline)
- Memory peak during large file upload
- Share list query latency (10k shares)

---

## Priority Tier 3: Technical Debt & Polish

### 9. Frontend Structure Refactoring

**Depends on**: Nothing (low priority)  
**Estimated effort**: 3 story points  
**Current state**: Phase 6.3 deferred nested directory structure due to tooling constraints

**Work breakdown**:
- [ ] Refactor `src/*.rs` flat structure to `src/{auth,vault,transfer,layout,components}/*.rs`
- [ ] Update imports and module paths
- [ ] Verify `trunk build` still succeeds

---

### 10. Documentation & Design Sweeps

**Estimated effort**: 7 story points total (distributed across sprints)

- [ ] ADR 011: IPC error sanitisation patterns (Phase 6.1)
- [ ] Windows DACL hardening design (Phase 4.5 follow-up)
- [ ] Startup retry orchestration diagram (Phase 4.5)
- [ ] Post-Phase-6.9 design sweep: remove phase-sequencing comments
- [ ] Update [design-invariants.md](design-invariants.md) with any Phase 7 discoveries

---

## Phase 7 Design Review Checklist

**Before sprint planning, validate**:

- [ ] **Multi-vault dependency graph**: Does multi-vault break any Phase 1–6 invariants?
- [ ] **Conflict merge strategy**: Decide on three-way merge vs conflict history
- [ ] **Fingerprint UX**: Define "verified" badge semantics and warning thresholds
- [ ] **Video EXIF research**: Validate MP4 moov-atom strategy
- [ ] **Performance baseline**: Profile Phase 6.9 before optimization work
- [ ] **Strong revocation cost-benefit**: Measure key rotation vs plaintext-retention risk

---

## Success Criteria for Phase 7

1. ✅ Multi-vault switching works transparently with active sync operations
2. ✅ Recovery workflows (`recover_from_cloud`, `migrate_vault`) are tested end-to-end
3. ✅ File-level conflict detection prevents silent data loss on multi-device edits
4. ✅ Performance baseline established; sync time for 1GB vault is < 30 seconds
5. ✅ All Phase 7 features maintain Zero-Trace compliance

---

## Related Documents

- [Deferred Items Inventory](deferred-items-inventory.md) — Source of Phase 7 candidates
- [Global Design Invariants](design-invariants.md) — Must not be violated in Phase 7
- [PHASE_6_9_VALIDATION.md](../../PHASE_6_9_VALIDATION.md) — Phase 6.9 manual testing requirements

---

**Document Status**: Planning stub (to be completed during Phase 7 kickoff)  
**Next Update**: When Phase 7 sprint planning begins
