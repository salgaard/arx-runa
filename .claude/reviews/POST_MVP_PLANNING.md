# Post-MVP Planning & Phase 7+ Roadmap

**Date**: 2026-04-24  
**Scope**: Pre-release fixes (Week 1) + Phase 7–10 features (Weeks 2–10)  
**Status**: MVP pre-release work identified; Phase 7+ features prioritized

---

## Timeline Overview

```
Week 1         Week 2-3         Week 4-5         Week 6-7         Week 8
[Pre-Release]  [Phase 7]        [Phase 8]        [Phase 9]        [Phase 10]
2-3 days       2-4 weeks        1-2 weeks        1-2 weeks        1 week
└─ 3 fixes     └─ Features      └─ Testing       └─ Threat Model  └─ Hardening
   → MVP         → Production      → Security       → Report
```

---

## Pre-MVP Work (Week 1 — 2–3 days)

**Critical path**: These 3 medium-priority items block MVP launch.

### Issue M1: Cloud Sync Session Lifecycle Wiring

**Complexity**: Difficult  
**Effort**: 1–1.5 days  
**Blocking**: Yes

**Problem**: rclone.conf temp files are not cleaned up on session lock/timeout, leaving stale credentials.

**Fix Location**: 
- `src-tauri/src/auth/session/manager.rs` — Add cleanup calls to `lock()` and timeout handler
- `src-tauri/src/storage/cloud/mod.rs` — Add `cleanup_session_artifacts()` trait method
- `src-tauri/src/storage/cloud/rclone.rs` — Implement cleanup for RcloneTransport

**See**: `.claude/reviews/PRE_RELEASE_WORK_PACKAGE.md` § Issue M1 for complete implementation code.

---

### Issue M2: Startup Retry Orchestration

**Complexity**: Straightforward  
**Effort**: 1 day  
**Blocking**: Yes

**Problem**: Interrupted password-change operations (app crash during ceremony) leave a `pending-vault-header.json` artifact. On restart, users are stuck until manually deleting the file.

**Fix Location**:
- `src-tauri/src/main.rs` — Add startup check + recovery commands
- `src-tauri/src/auth/ceremonies/types.rs` — Define `PendingVaultHeader` type
- Existing ceremony files — Add cleanup after successful completion

**See**: `.claude/reviews/PRE_RELEASE_WORK_PACKAGE.md` § Issue M2 for complete implementation code.

---

### Issue M3: Streaming Progress Channel Validation

**Complexity**: Straightforward  
**Effort**: 0.5–1 day  
**Blocking**: Yes

**Problem**: If frontend closes connection during long uploads, backend can panic or deadlock on channel send.

**Fix Location**:
- `src-tauri/src/ui/commands/mod.rs` — Create `ProgressChannel` validation wrapper
- `src-tauri/src/ui/commands/upload_file.rs` — Use wrapper to check channel status
- `src-tauri/src/ui/commands/download_file.rs` — Same pattern
- `src-tauri/src/ui/commands/sync_to_cloud.rs` — Same pattern

**See**: `.claude/reviews/PRE_RELEASE_WORK_PACKAGE.md` § Issue M3 for complete implementation code.

---

## Phase 7: Advanced Features (Weeks 2–4 — 2–4 weeks)

Post-MVP features that improve user experience and expand platform capabilities.

### Tier 1: Highest User Value (Start Week 2)

#### 1.1 Advanced Recovery Flows (UI Implementation)

**Effort**: 3–5 days  
**Complexity**: Straightforward (backend ready)  
**User Value**: ⭐⭐⭐⭐⭐ (critical for multi-device)

**What**: User can recover vault from a new device via recovery phrases or cloud recovery.

**Backend Status**: ✅ `recover_from_cloud` IPC command already wired  
**Frontend Status**: ❌ No RecoveryWizard UI component

**Implementation**:
1. Design recovery UI flow (wizard steps, error handling)
2. Create `src/pages/RecoveryWizard.rs` Leptos component
3. Wire UI to backend `recover_from_cloud` command
4. Test error states (corrupt manifest, no remote vault, network timeout)
5. Validate across Windows/macOS/Linux

**Acceptance Criteria**:
- [ ] User sees recovery option on login page
- [ ] Can enter recovery phrase or select cloud recovery
- [ ] Progress shown during manifest download/validation
- [ ] Clear error messages (e.g., "Manifest corrupted")
- [ ] Recovered vault opens in main app without re-authentication

**See**: `PHASE-7-IMPLEMENTATION-ROADMAP.md` § 1.1 Advanced Recovery Flows

---

#### 1.2 In-App File Viewer (UI Implementation)

**Effort**: 2–3 days  
**Complexity**: Straightforward (backend ready)  
**User Value**: ⭐⭐⭐⭐ (improves UX)

**What**: Click file in vault → inline preview shown (for images, documents). Fallback to download-and-open for large files.

**Backend Status**: ✅ `get_file_content` returns plaintext (50 MiB cap)  
**Frontend Status**: ❌ No file viewer component

**Implementation**:
1. Add media viewer library (e.g., `image-rs` crate for JPEG/PNG/TIFF)
2. Create file preview pane in Files page
3. Wire to `get_file_content` with file type detection
4. Handle unsupported file types (show "download to open")
5. Performance optimization: <1s open for <5 MiB files

**Acceptance Criteria**:
- [ ] Click image file → preview shown inline
- [ ] Unsupported file types show "download to open"
- [ ] Large files (>10 MiB) warn "download-and-open"
- [ ] File picker shows preview pane
- [ ] No crashes on corrupted files

**See**: `PHASE-7-IMPLEMENTATION-ROADMAP.md` § 1.2 In-App File Viewer

---

### Tier 2: Architecture Enablers (Start Week 3, design phase)

#### 2.1 Multi-Vault Support (Design + Implementation)

**Effort**: 5 days (design) + 8 days (implementation) = 13 days  
**Complexity**: Difficult (requires state context refactoring)  
**User Value**: ⭐⭐⭐⭐ (enables advanced workflows)

**What**: User can have multiple vaults open simultaneously with transparent switching.

**Current State**: SessionManager supports one active vault per session. UI state contexts bind to single vault.

**Design Phase** (Week 3):
- [ ] Audit `AppState` for vault-specific fields
- [ ] Map SessionManager entry points (13 methods)
- [ ] Design new multi-vault state model
- [ ] Define vault switching semantics (concurrent sessions or sequential?)
- [ ] Deliverable: Design document `docs/architecture/designs/multi-vault/design.md`

**Implementation Phase** (Week 4):
- [ ] Refactor SessionManager to track multiple vault IDs
- [ ] Add VaultSelector provider to state context hierarchy
- [ ] Update VaultProvider to support switching
- [ ] Create vault switcher UI component
- [ ] Test: Switch vaults, verify isolation

**Acceptance Criteria**:
- [ ] App remembers last 3 vaults
- [ ] Can switch between vaults without re-auth
- [ ] Each vault has independent sync status
- [ ] Closing vault clears only that vault's UI state
- [ ] No cross-vault credential leakage

**Blocking Decision**: Concurrent sessions or sequential? (Session timeout applies globally vs. per-vault?)

**See**: `PHASE-7-IMPLEMENTATION-ROADMAP.md` § 2.1 Multi-Vault Support

---

#### 2.2 Conflict Resolution Enhancement (Research + Design)

**Effort**: 4 days (research) + 4 days (design) = 8 days  
**Complexity**: Difficult (requires algorithm design)  
**User Value**: ⭐⭐⭐ (improves robustness)

**What**: Auto-resolve sync conflicts using heuristics instead of manual resolution.

**Current State**: Detect-and-block (user must manually resolve or delete one version).

**Phase 7+ Goal**: Three-way merge heuristics (Git-style conflict resolution).

**Research Phase** (Week 3):
- [ ] File timestamp comparison accuracy analysis
- [ ] Heuristics evaluation:
  - If remote newer: accept remote? (risk: overwrite local edits)
  - If local newer: accept local? (risk: data loss)
  - If timestamps equal: size-based decision? (unreliable)
- [ ] Study Git 3-way merge + rsync conflict handling
- [ ] Deliverable: Research document `docs/research/conflict-resolution-heuristics.md`

**Design Phase** (Week 4):
- [ ] Select best heuristic based on research
- [ ] Design user notification flow (show diffs, offer rollback?)
- [ ] Design recovery strategy (how to undo bad merge?)
- [ ] Deliverable: Design update `docs/architecture/designs/cloud-synchronisation/design.md` § Conflict Resolution

**Implementation**: Defer to Phase 8 (requires user testing of heuristics)

**See**: `PHASE-7-IMPLEMENTATION-ROADMAP.md` § 2.2 Conflict Resolution Enhancement

---

### Tier 3: Advanced Cryptography (Start Week 4)

#### 3.1 Strong Revocation (Key Rotation)

**Effort**: 1–2 days  
**Complexity**: Medium (crypto model complete)  
**User Value**: ⭐⭐⭐ (security feature)

**What**: Rotate file encryption key to prevent future access to shared files.

**Current State**: Default revocation (delete share packages) prevents *future* downloads but can't recall plaintext already downloaded.

**Strong Revocation**: Rotate `file_key` → re-encrypt all chunks → issue new share packages. Recipients who haven't downloaded yet get new key; old key is destroyed.

**Design Questions**:
- [ ] Automatic recipient notification or manual approval?
- [ ] Archive old key material or destroy immediately?
- [ ] UI: Single button or multi-step workflow?

**Implementation**:
1. Add `rotate_file_key` IPC command
2. Implement `storage::vault_ops::rotate_file_key()` (re-encrypt chunks)
3. Update sharing store to rotate share packages
4. Notify recipients of new key (async queue)
5. Add UI button in "Share Details" view

**Acceptance Criteria**:
- [ ] Click "Rotate Key" → backend re-encrypts all chunks
- [ ] Share packages updated with new key
- [ ] Old key is destroyed (verified by test)
- [ ] Recipients can re-download with new key
- [ ] Progress shown during re-encryption

**See**: `PHASE-7-IMPLEMENTATION-ROADMAP.md` § Strong Revocation (Key Rotation)

---

## Phase 8: Integration Testing & Validation (Weeks 5–6 — 1–2 weeks)

Comprehensive end-to-end testing covering all modules and adversarial scenarios.

### Scope

- [ ] **Happy-path workflows**: Create vault → upload file → share → recover on new device
- [ ] **Adversarial scenarios**: Corrupted manifest, network timeout mid-sync, concurrent uploads, key file deleted
- [ ] **Cross-platform validation**: Windows, macOS, Linux (UI rendering, file paths, permissions)
- [ ] **Performance baselines**: Encryption throughput (MB/s), sync latency, memory usage
- [ ] **Security edge cases**: Session timeout during operation, password change race conditions, revocation edge cases

### Key Test Areas

**From Design Review Deferred Items**:
- Recovery from incomplete password-change artifacts (after M2 fixed)
- Rclone session cleanup on timeout (after M1 fixed)
- Channel disconnection handling (after M3 fixed)

**From Deferred Items Inventory**:
- `docs/architecture/deferred-items-inventory.md` § Testing Coverage

### Deliverables

- [ ] Test plan document (`docs/testing/e2e-test-plan.md`)
- [ ] Test suite in `src-tauri/tests/` (integration tests)
- [ ] CI configuration for cross-platform testing
- [ ] Performance baseline report

**Effort**: 1–2 weeks (5–10 days)

---

## Phase 9: Threat Model & Report (Weeks 6–7 — 1–2 weeks)

Produce formal threat model, architecture comparison, and consolidate report-log entries.

### Scope

- [ ] **Formal Threat Model**: STRIDE analysis for all attack surfaces
- [ ] **Architecture Comparison**: Arx Runa vs. OneDrive vs. Cryptomator vs. Tresorit
- [ ] **Report Consolidation**: Integrate all report-log entries (`docs/report-log/`) into bachelor report structure
- [ ] **Known Limitations**: Document explicitly (e.g., "MP4 EXIF stripped deferred", "Directory deletion not implemented")

### Deliverables

- [ ] Threat model document (`docs/security/threat-model.md`)
- [ ] Architecture comparison document (`docs/research/architecture-comparison.md`)
- [ ] Consolidated bachelor report (`docs/AREX_RUNA_BACHELOR_REPORT.md`)

**Effort**: 1–2 weeks (5–10 days)

---

## Phase 10: Hardening & Submission (Week 8 — 1 week)

Final security review, dependency audit, CI cleanup, and submission preparation.

### Scope

- [ ] **Security Review**: Audit all crypto-adjacent modules (crypto, auth, storage, sharing)
- [ ] **Dependency Audit**: Check for known vulnerabilities (`cargo audit`)
- [ ] **CI Pipeline**: Ensure all checks pass (build, test, clippy, fmt, security-audit)
- [ ] **Code Cleanup**: Remove TODOs, debug logging, test-only helpers
- [ ] **Documentation**: Verify all doc links, code examples, README accuracy
- [ ] **Submission Prep**: Verify source tree structure, license headers, changelog

### Deliverables

- [ ] Security audit report
- [ ] Dependency audit report
- [ ] Final CI run (all green)
- [ ] Submission package ready for evaluation

**Effort**: 1 week (5 days)

---

## Low-Priority Post-MVP Items (Phase 7+, no deadline)

These items are non-blocking and can be deferred further if needed.

### Deferred From Design Review

**4 LOW-priority items** identified in `DESIGN_IMPLEMENTATION_REVIEW_2026-04-23.md`:

1. **Windows DACL Hardening** — Add Windows ACL policy to protect vault files
   - Complexity: Straightforward
   - Effort: 1–2 days
   - Can combine with Phase 10 hardening

2. **Pending Deletions Durable Drain** — Retry orphaned blob deletion on sync failure
   - Complexity: Difficult (requires durable queue pattern)
   - Effort: 2–3 days
   - Can defer to Phase 8+ performance optimization

3. **Epoch Buffer Staging** — Batch small files into larger blobs before upload (storage optimization)
   - Complexity: Difficult (architecture change to chunking pipeline)
   - Effort: 3–5 days
   - Performance feature, not user-facing; defer to Phase 8+

4. **Advanced Sharing Workflows** — Multi-recipient grouping, delegation, expiration
   - Complexity: Hard (requires protocol design)
   - Effort: 5–8 days
   - Feature expansion; defer to Phase 9+

---

## Reference Documents

- **MVP Review**: `DESIGN_IMPLEMENTATION_REVIEW_2026-04-23.md` — Technical compliance audit (all 7 phases)
- **Pre-Release Work**: `PRE_RELEASE_WORK_PACKAGE.md` — 3 blocking issues with implementation code
- **Phase 7 Details**: `PHASE-7-IMPLEMENTATION-ROADMAP.md` — Detailed feature backlog from prior analysis
- **Deferred Items**: `docs/architecture/deferred-items-inventory.md` — Comprehensive deferred items list

---

## Success Criteria

### MVP (Week 1)
- ✅ 3 pre-release fixes completed
- ✅ All tests pass (`cargo test --lib`)
- ✅ Build succeeds (`cargo build --release`)
- ✅ Launch decision: GO

### Phase 7 (Weeks 2–4)
- ✅ Recovery UI complete and tested
- ✅ File viewer integrated and performant
- ✅ Multi-vault design finalized
- ✅ Conflict resolution research completed

### Phase 8 (Weeks 5–6)
- ✅ End-to-end test suite passes on all platforms
- ✅ Performance baselines established
- ✅ Adversarial scenarios verified

### Phase 9 (Weeks 6–7)
- ✅ Threat model published
- ✅ Architecture comparison completed
- ✅ Report consolidated and ready for evaluation

### Phase 10 (Week 8)
- ✅ Security audit passed
- ✅ Dependency audit clean (`cargo audit` zero vulnerabilities)
- ✅ CI all green
- ✅ Submission package ready

---

## Dependencies & Blockers

**MVP Pre-Release** → All other phases (Phase 7 cannot start until MVP fixed)

**Phase 7.1 (Recovery UI)** → Independent (backend ready)  
**Phase 7.2 (File Viewer)** → Independent (backend ready)  
**Phase 7.3 (Multi-Vault Design)** → Independent (design-only)  
**Phase 7.4 (Conflict Resolution)** → Independent (research-only)  

**Phase 8** → Depends on Phase 7 (features must be implemented before testing)  
**Phase 9** → Depends on Phase 8 (threat model informed by test results)  
**Phase 10** → Depends on Phase 9 (final cleanup before submission)

---

## Team Assignments

### Pre-MVP (Week 1)
- **M1 (Cloud Session Lifecycle)**: Rust implementer (1.5 days)
- **M2 (Startup Recovery)**: Rust implementer (1 day)
- **M3 (Progress Validation)**: Rust implementer (0.5 days)

### Phase 7 (Weeks 2–4)
- **Recovery UI**: Frontend specialist (3–5 days)
- **File Viewer**: Frontend specialist (2–3 days)
- **Multi-Vault Design**: Architect (5 days)
- **Conflict Resolution Research**: Researcher (4 days)

### Phase 8–10
- Assign based on domain expertise (testing, security, reporting)

---

**Next Steps**: 
1. Complete MVP pre-release work (Week 1)
2. Launch MVP
3. Begin Phase 7 Feature work (Week 2)
4. Plan Phases 8–10 in parallel (assign researchers/testers early)

**Last Updated**: 2026-04-24  
**Status**: Ready for execution
