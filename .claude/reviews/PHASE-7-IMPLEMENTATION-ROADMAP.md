# Phase 7+ Implementation Roadmap — From Audit Findings

**Generated from**: Comprehensive deferred items audit (2026-04-23)  
**Scope**: Prioritized Phase 7+ work, research requirements, design decisions needed

---

## Overview

This document translates audit findings into a **actionable Phase 7+ roadmap**. All Phase 0–6.9 work is complete; these items represent planned extensions validated against the design corpus.

---

## IMMEDIATE ACTIONS (Ready to Implement)

### Tier 1: Highest User Value (Can start Week 1)

#### 1.1 Advanced Recovery Flows (UI Implementation)
**Effort**: 8 points (3-5 days)  
**Prerequisites**: None — backend already implemented  
**Scope**:
- [ ] Design recovery flow UI (wizard steps)
- [ ] Create `src/pages/RecoveryWizard.rs` component
- [ ] Wire `recover_from_cloud` IPC command
- [ ] Handle error states (corrupt manifest, no remote vault, etc.)
- [ ] Test across Windows/macOS/Linux

**Starting Points**:
- Backend: `src-tauri/src/ui/sync_commands.rs::pull_vault()`
- Design: `docs/architecture/designs/cloud-synchronisation/design.md` § Push/Pull Flows
- UI Template: Existing sync UI in `src/pages/Sync.rs`

**Definition of Done**:
- User can recover vault from new device
- Progress shown during manifest download
- Error handling clear and actionable

---

#### 1.2 In-App File Viewer (UI Implementation)
**Effort**: 5 points (2-3 days)  
**Prerequisites**: None — backend ready (50 MiB cap)  
**Scope**:
- [ ] Integrate media viewer library (e.g., `image-rs` for JPEG/PNG/TIFF)
- [ ] Create file preview component in Files page
- [ ] Handle unsupported file types gracefully
- [ ] Add download-then-view option for larger files

**Starting Points**:
- Backend: `src-tauri/src/ui/file_commands.rs::get_file_content()`
- Design: `docs/architecture/designs/tauri-ipc-and-frontend/design.md` § Out of Scope (MVP)
- UI: Add preview pane next to `FileListItem.rs`

**Definition of Done**:
- Click file → inline preview shown (if supported)
- Fallback download-and-open for unsupported
- Performance: <1s open for <5 MiB files

---

### Tier 2: Architecture Enablers (Research + Design Phase)

#### 2.1 Multi-Vault Support (Design)
**Effort**: 13 points (Architecture design 5 pts + implementation 8 pts)  
**Prerequisites**: Design dependency analysis  
**What This Unblocks**:
- Transparent vault switching UI
- Backup/restore across vaults
- Per-device session coordination

**Required Decisions**:
- [ ] Session model: One active vault at a time, or concurrent sessions?
- [ ] Session timeout: Reset on switch, or per-vault timeout?
- [ ] State context refactor scope
- [ ] Vault switcher UI UX

**Research Tasks**:
- [ ] Audit `AppState` for vault-related fields
- [ ] Map all `SessionManager` entry points (13 methods)
- [ ] List all state contexts (`SessionProvider`, `VaultProvider`, `SyncProvider`)
- [ ] Design new `VaultSelector` provider + context

**Starting Investigation**:
```bash
# Find all vault-related state
grep -r "vault_id\|active_vault" src-tauri/src/ui/ --include="*.rs" | head -20
grep -r "AppState" src-tauri/src/ui/ --include="*.rs" | grep "pub struct"
```

**Deliverable for Week 2**: Design document `docs/architecture/designs/multi-vault/design.md`

---

#### 2.2 Conflict Resolution Enhancement (Research)
**Effort**: 8 points (Research 4 pts + design 4 pts)  
**Current State**: Detect-and-block (manual resolution)  
**Phase 7+ Goal**: Three-way merge heuristics

**Research Questions**:
- [ ] File timestamp comparison accuracy (system clock skew tolerance?)
- [ ] Heuristics for automatic merge decisions:
  - If remote newer: accept remote?
  - If local newer: accept local?
  - If modified timestamps same: size-based decision?
- [ ] Conflict presentation UX (show diffs? offer rollback?)
- [ ] Recovery from failed merge (rollback strategy)

**Literature Review**:
- Git three-way merge algorithm
- rsync conflict detection
- Dropbox/Google Drive conflict resolution UX

**Starting Point**:
```rust
// Current code location
src-tauri/src/storage/cloud/sync.rs::check_conflicts()
```

**Deliverable for Week 3**: Research document `docs/research/conflict-resolution-heuristics.md`

---

## PHASE 7+ DETAILED FEATURES

### Feature: Strong Revocation (Key Rotation)
**Status**: Cryptographic model complete; implementation deferred  
**Effort**: 4 points (1–2 days once design approved)  
**Complexity**: Medium

**What It Does**:
- Rotate `file_key` → new key
- Re-encrypt all chunks under new key
- Issue new share packages to remaining recipients
- Archive old share packages (optional)

**Why It Matters**:
- Default revocation prevents *future* access but can't claim plaintext recall
- Strong revocation prevents *present* access for recipients who haven't fetched yet
- Cost: O(file_size) re-encryption + recipient notification

**Design Questions**:
- [ ] Notify recipients automatically, or manual approval?
- [ ] Archive old key material, or destroy immediately?
- [ ] UI: single button "rotate key" or multi-step workflow?

**Implementation Path**:
1. Add `rotate_file_key` command to IPC contract
2. Implement `storage::vault_ops::rotate_file_key()`
3. Add UI button in share details
4. Test with multi-recipient shares

**Starting Point**: [File Sharing § Revocation](docs/architecture/designs/file-sharing/design.md) lines 165–230

---

### Feature: Directory Operations
**Status**: Files-only MVP; recursive deletion deferred  
**Effort**: 6 points (2–3 days)  
**Complexity**: Medium (recursive ops + cascade cleanup)

**What It Does**:
- Delete entire directory trees
- Move entire directories (maintain children)
- Copy directories (recursive)

**Why It Matters**:
- Users expect directory operations as bundle
- Current MVP forces individual file deletion
- Improves UX for large project uploads

**Implementation Path**:
1. Add `delete_directory` command to IPC contract
2. Implement recursive deletion in `storage::vault_ops`
3. Handle orphaned blobs on interruption
4. Add directory context menu in Files UI
5. Test with nested directories (10+ levels)

**Starting Point**: [Chunking & Manifest § Schema](docs/architecture/designs/chunking-and-manifest/design.md)

---

### Feature: Video EXIF Stripping
**Status**: JPEG/PNG complete; video deferred due to streaming constraint  
**Effort**: 5 points (research 2 pts + implementation 3 pts)  
**Complexity**: High (streaming invariant violation)

**Architectural Challenge**:
- MP4 `moov` atom (metadata) typically at **end of file**
- Current streaming pipeline: read → chunk → encrypt → upload
- Can't access `moov` without reading entire file first

**Possible Approaches**:
1. **Two-Pass Seek**: Read file twice (metadata pass, data pass)
   - Pros: Preserves streaming for uploads
   - Cons: Slower, requires random seek
   
2. **Temporary Spool**: Buffer file on disk, then strip
   - Pros: Works with existing tools
   - Cons: Violates zero-trace (temp files on disk)
   
3. **External Tool**: Delegate to ffmpeg/exiftool
   - Pros: Reliable, maintained
   - Cons: Binary dependency, security surface

**Recommendation for Week 2**: Research document evaluating approaches; recommend approach

---

## RESEARCH & DESIGN TASKS (Parallel Work)

### Design Review Checklist (Week 1–2)

Before committing to any Phase 7+ feature, validate:

- [ ] **Multi-vault**: Verify `SessionManager` can handle per-vault session keys
- [ ] **Conflict resolution**: Verify timestamp accuracy on target platforms
- [ ] **Strong revocation**: Measure re-encryption performance on 1GB + 10GB files
- [ ] **Directory operations**: Audit orphan cleanup for recursive deletes
- [ ] **Video EXIF**: Benchmark two-pass seek vs single-pass streaming

### Research Documents to Write

| Document | Owner | Deadline | Usage |
|----------|-------|----------|-------|
| Conflict Resolution Heuristics | Team | Week 2 | Design decision input |
| Multi-Vault Dependency Analysis | Team | Week 2 | Architecture planning |
| Video EXIF Approaches | Team | Week 2 | Implementation decision |
| Strong Revocation Cost-Benefit | Team | Week 3 | Priority decision |
| Performance Baseline (Phase 6.9) | QA | Week 1 | Optimization targets |

---

## PRIORITY MATRIX

```
┌─────────────────────────────────────────────────────────┐
│  URGENT & IMPORTANT              │  NOT URGENT & IMP    │
│  (Do First)                       │  (Schedule Soon)    │
├─────────────────────────────────────────────────────────┤
│ • Advanced recovery flows         │ • Multi-vault       │
│ • In-app file viewer             │ • Directory ops     │
│ • Conflict resolution (design)   │ • Performance opt   │
│                                   │                     │
├─────────────────────────────────────────────────────────┤
│  URGENT & NOT IMP                │  NOT URGENT & IMP   │
│  (Automate/Delegate)             │  (Do Later)         │
├─────────────────────────────────────────────────────────┤
│ • UI polish (delete error, etc)  │ • Video EXIF        │
│ • Documentation updates          │ • Strong revocation │
│ • Code cleanup                   │ • Fingerprint hist  │
│                                   │                     │
└─────────────────────────────────────────────────────────┘
```

---

## TIMELINE ESTIMATE

### Week 1: Research & Setup
- Performance baseline profiling (Phase 6.9)
- Multi-vault dependency analysis
- Conflict resolution research

### Week 2: Design Phase
- Multi-vault design document (architecture decision)
- Conflict resolution design (heuristics spec)
- Recovery flows UX specification

### Week 3: Implementation Sprint 1
- Advanced recovery flows UI (3 pts)
- In-app file viewer UI (2 pts)
- Directory deletion (1–2 pts)

### Week 4: Implementation Sprint 2
- Multi-vault state refactor (5–6 pts)
- Conflict resolution implementation (4 pts)
- Performance optimizations (3–4 pts)

### Week 5: Hardening & Testing
- Cross-platform validation (Win/Mac/Linux)
- Performance benchmarking
- Security audit on recovery flows

---

## SUCCESS CRITERIA

### For Phase 7 Completion
- [ ] All must-have features implemented and tested
- [ ] Nice-to-have features (at least 2 of 4) complete
- [ ] Cross-platform compatibility verified
- [ ] Performance degradation <5% vs Phase 6.9
- [ ] Design invariants maintained
- [ ] Zero-Trace compliance preserved

### Code Quality Gates
- [ ] `cargo clippy -- -D warnings` clean
- [ ] All unsafe blocks documented
- [ ] 80%+ test coverage on new code
- [ ] No deferred work introduced

---

## Open Questions for Stakeholder Approval

Before Week 1 starts, confirm:

1. **Multi-vault scope**: Concurrent sessions or single active vault?
2. **Recovery priority**: Is new-device recovery critical for Phase 7.1, or nice-to-have?
3. **Video EXIF approach**: Accept two-pass performance cost, or defer indefinitely?
4. **Performance targets**: 5% degradation acceptable, or aim for zero?
5. **Timeline flexibility**: 5-week estimate realistic, or need acceleration?

---

## Related Documents

- **Audit Report**: `.claude/reviews/deferred-items-audit-20260423.md`
- **Quick Reference**: `.claude/reviews/DEFERRED-ITEMS-QUICK-REFERENCE.md`
- **Official Inventory**: `docs/architecture/deferred-items-inventory.md`

