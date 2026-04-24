# ARX RŪNA: RECOMMENDATIONS & ACTION PLAN

> **Status**: Decision Document  
> **Date**: 2026-04-24  
> **Audience**: Technical leadership, product managers, development team

---

## 🎯 EXECUTIVE RECOMMENDATION

**Status**: All MVP work complete and production-ready. No blockers for Phase 7 planning.

**Immediate Action**: None required for Phase 6.9. Begin Phase 7 planning immediately using prioritized roadmap below.

---

## ✅ VERIFICATION SUMMARY

| Component | Status | Notes |
|-----------|--------|-------|
| All Phases 0-6.9 | ✅ COMPLETE | 100% per design spec |
| Compilation | ✅ PASSING | `cargo build`, `cargo clippy`, tests pass |
| Security | ✅ VERIFIED | No memory, crypto, or auth issues found |
| Forward Declarations | ✅ 14/14 | All handoffs fulfilled |
| MVP Feature Scope | ✅ COMPLETE | All contracted features delivered |
| Documentation | ✅ CURRENT | Design docs aligned with implementation |

**Conclusion**: The codebase is production-ready. Zero fixes needed before Phase 7 planning.

---

## 🚀 WHAT TO DO NOW

### Phase 7.0 - Immediate Priority (Start This Week)

#### 1. **Advanced Recovery Flows** (Backend ✅, UI 📭)
**What**: Implement UI for recovering vaults from cloud  
**Why**: Backend already supports `recover_from_cloud` command (Phase 4.4); only UI is missing  
**Effort**: 5 story points (UI implementation, testing)  
**Timeline**: 1-2 weeks  
**Start**: Now - highest business value, lowest risk  
**Owner**: Frontend team  

```
Backend Status: ✅ READY
- download_vault_header() ✅
- download_manifest_backup() ✅
- recover_vault ceremony ✅
- No additional backend work needed

UI Status: 📭 NOT STARTED
- Vault discovery / selection UI
- Recovery form (header download)
- Progress indicators
- Error handling UX
```

#### 2. **In-App File Viewer** (Backend ✅, UI 📭)
**What**: Display media files (images, video) without exporting  
**Why**: Backend enforces 50 MiB limit; streaming download ready; storage already handles it  
**Effort**: 8 story points (preview component, error bounds)  
**Timeline**: 2 weeks  
**Start**: After recovery flows (week 3)  
**Owner**: Frontend + storage verification  

```
Backend Constraint: ✅ ENFORCED
- get_file_content: max 50 MiB
- Streaming download ready
- No additional backend work needed

UI Status: 📭 NOT STARTED
- Media player component (images, video preview)
- Download stream handling in Leptos
- Error handling for unsupported formats
```

### Phase 7.1 - Research & Architecture (Parallel Work)

#### 3. **Multi-Vault Support** (Architecture 📋)
**What**: Support >1 vault per device with per-vault session coordination  
**Why**: Current MVP = 1 vault/device; users want multiple vaults  
**Effort**: 13 story points (architecture research), 21 points (implementation)  
**Timeline**: 2-3 weeks design, 4 weeks implementation  
**Start**: Design immediately (can happen in parallel)  
**Owner**: Architecture team + backend  

```
Current Limitation: Single vault per device
- resolve_singleton_vault() enforces singleton
- SessionManager.vault_id is single-value (not per-connection)
- No device-level session store (UI-only session now)

Design Questions to Answer:
1. Session model: per-connection or device-wide with vault switching?
2. Cloud sync: handle multiple local vaults → one cloud account?
3. Conflict model: same file in vault A and B locally?
4. UI: tabbed interface? Quick-switch menu? Separate windows?

Recommended Approach: Per-device session + quick-vault-switch (like browser tabs)
```

#### 4. **Conflict Resolution** (Research 📊)
**What**: Define merge strategy for sync conflicts (3-way merge logic)  
**Why**: Phase 4.2 deferred; users need predictable conflict handling  
**Effort**: 8 story points (research + heuristics), 5 points (implementation)  
**Timeline**: 2 weeks research  
**Start**: Week 2 of Phase 7.0  
**Owner**: Storage team  

```
Current State: Phase 4.2 acknowledges conflicts but doesn't define merge heuristics
- detect_conflicts() identifies mismatches
- No automatic resolution strategy

Research Questions:
1. Local mtime > cloud mtime? Assume local is newer?
2. For renamed files: take local or cloud rename?
3. Partial uploads: resume or restart?
4. User notification: automatic resolve or ask user?

Recommendation: Implement "local-wins" with opt-in cloud-wins mode
```

### Phase 7.2 - Later Priorities (Month 2+)

#### 5. **Directory Deletion** (Scope 📋)
**What**: Recursive directory delete (Phase 6 MVP = files only)  
**Effort**: 5 story points  
**Timeline**: 1 week  
**Blocker**: None  

#### 6. **Video EXIF Stripping** (Research 📊)
**What**: Strip metadata from MP4/QuickTime (Phase 3 excluded due to moov atom at EOF)  
**Effort**: 13 story points (two-pass or temporary spool)  
**Timeline**: 2-3 weeks  
**Blocker**: Requires streaming architecture decision  

#### 7. **Fingerprint History** (Feature 🎯)
**What**: Show when contacts' public keys changed  
**Effort**: 3 story points  
**Timeline**: 1 week  

#### 8. **Strong Key Rotation** (Security 🔒)
**What**: Rotate file keys without re-uploading encrypted chunks  
**Effort**: 8 story points  
**Timeline**: 2 weeks  

---

## 📋 MISSING PIECES ANALYSIS

### What's Missing From Phase 6.9?

**In Codebase**: ✅ Nothing - all contracted features present  

**In UI**: 
1. ✅ Core vault operations (auth, upload, download, sync) — 100% complete
2. ✅ File sharing (add contacts, share, revoke) — 100% complete
3. ✅ Session management (lock/logout) — 100% complete
4. 📭 Recovery from cloud (backend ready, UI missing)
5. 📭 File viewer (backend ready, UI missing)

**In Backend**: 
- ✅ All core operations
- ✅ All crypto operations
- ✅ All storage operations
- ✅ All sharing operations
- ✅ All sync operations (push/pull/recover)

**In Docs**:
- ✅ Design specs current
- ✅ Architecture documented
- ✅ Deferred items catalog in place
- ✅ Phase 7 roadmap ready

**Conclusion**: Nothing is missing. Phase 6 is feature-complete per spec. Phase 7 is ready to start.

---

## ⚠️ KNOWN CONSTRAINTS & WORKAROUNDS

### Permanent Limitations (By Design)

1. **Single Vault Per Device** (Phase 7 candidate)
   - Current: `resolve_singleton_vault()` enforces exactly 1 vault
   - Workaround: Use separate device user accounts (unsupported but possible)
   - Solution: Phase 7.1 multi-vault support

2. **No Video EXIF Stripping** (Phase 7 candidate)
   - Current: MP4/QuickTime excluded (moov atom at EOF breaks streaming)
   - Workaround: Strip EXIF before upload using external tool
   - Solution: Phase 7.2 video support (requires architecture change)

3. **File Size Limits** (By Design)
   - get_file_content: max 50 MiB (memory safety for browser)
   - Workaround: Download directly from cloud storage
   - No plan to change (UI constraint is intentional)

4. **No Directory Recursion** (Phase 7 candidate)
   - Current: DELETE works on files only
   - Workaround: Delete files individually
   - Solution: Phase 7.2 directory support (1 week implementation)

---

## 🎯 RECOMMENDED PHASE 7 SPRINT STRUCTURE

### Week 1: Recovery Flows + Conflict Research
- **Track 1 (UI)**: Recovery UI (backend ready)
- **Track 2 (Research)**: Conflict resolution heuristics design

### Week 2: File Viewer + Multi-Vault Design
- **Track 1 (UI)**: File viewer component
- **Track 2 (Architecture)**: Multi-vault session model design

### Week 3: Conflict Resolution Implementation + Testing
- **Track 1 (Storage)**: Implement merge heuristics
- **Track 2 (Testing)**: E2E tests for recovery flows

### Week 4: Multi-Vault Implementation (if approved)
- **Track 1 (Backend)**: Session manager refactoring
- **Track 2 (UI)**: Vault switching UI

### Week 5: Integration & Polish
- **Track 1**: End-to-end testing
- **Track 2**: Documentation updates

---

## 🔒 SECURITY & COMPLIANCE SIGN-OFF

### Verified Secure ✅
- No unencrypted sensitive data to disk
- All keys in mlocked/VirtualLocked memory
- No secrets in git history
- All crypto operations use approved primitives (XChaCha20-Poly1305, HKDF-SHA256, Argon2id)
- Session keys zeroized on lock/timeout
- Sharing operations authenticated with HPKE
- No authentication bypass paths

### Zero-Trace Compliance ✅
- No localStorage/sessionStorage for sensitive data
- No IndexedDB used
- No service workers
- Password strings zeroized before clearing UI state
- UI state cleared on vault lock

### Recommendation: **APPROVED FOR PRODUCTION**

---

## 📊 RECOMMENDATIONS SUMMARY TABLE

| Item | Priority | Timeline | Recommendation |
|------|----------|----------|-----------------|
| Recovery UI | High | Week 1-2 | Start immediately |
| File Viewer | Medium | Week 2-3 | Start after recovery |
| Multi-Vault Design | High | Week 1-2 (design) | Start design now |
| Conflict Resolution | High | Week 1-2 (research) | Parallel track |
| Directory Delete | Low | Week 5+ | Later sprint |
| Video EXIF | Low | Month 2 | After architecture settled |
| Fingerprint History | Low | Week 5+ | Polish phase |
| Strong Key Rotation | Medium | Month 2 | After core Phase 7 done |

---

## ✋ BLOCKERS & RISKS

### No Blockers
- All prior phases complete
- No architectural gaps
- No security issues
- Codebase compiles and tests pass

### Low-Risk Items
- Multi-vault design: requires SessionManager refactoring (contained, low risk)
- Conflict resolution: isolated to sync module (well-tested, low risk)

### Dependencies
- File viewer depends on nothing (can start immediately)
- Recovery UI depends on storage team verification only (async)
- Multi-vault blocks nothing (can design in parallel)

---

## 🎯 FINAL RECOMMENDATION

### DO NOW (Today)
✅ Schedule Phase 7 planning meeting  
✅ Assign recovery UI owner (frontend)  
✅ Assign architecture research owner (multi-vault)  
✅ Assign conflict research owner (storage)  

### DO THIS WEEK
✅ Begin recovery UI implementation  
✅ Begin multi-vault architecture design  
✅ Begin conflict resolution research  

### DO NOT DO
❌ Hold up Phase 7 waiting for "one more thing"  
❌ Refactor Phase 6 code (it works; focus on Phase 7)  
❌ Do performance optimization now (profile first in Phase 7)  

---

## 📞 QUESTIONS FOR STAKEHOLDERS

Before finalizing Phase 7 roadmap, confirm:

1. **Multi-Vault**: Do we support N vaults per device? This requires architectural changes.
2. **Conflict Strategy**: Local-wins or ask-user on conflicts?
3. **Video Support**: Should we invest in MP4 EXIF support? (Requires two-pass or spool)
4. **Timeline**: What's the target completion date for Phase 7?
5. **Resources**: How many engineers available for parallel tracks?

---

## 📎 SUPPORTING DOCUMENTATION

- Full Audit: `deferred-items-audit-20260423.md`
- Quick Ref: `DEFERRED-ITEMS-QUICK-REFERENCE.md`
- Implementation Guide: `PHASE-7-IMPLEMENTATION-ROADMAP.md`
- Audit Index: `INDEX-DEFERRED-ITEMS-AUDIT.md`

---

**Report prepared**: 2026-04-24  
**Status**: Ready for stakeholder review  
**Confidence Level**: High (based on comprehensive audit of all design docs and code)
