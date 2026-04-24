# Deferred Items Audit — Quick Reference

**Generated**: 2026-04-23  
**Full Report**: `.claude/reviews/deferred-items-audit-20260423.md`

---

## One-Page Summary

### ✅ All Phases 0–6.9 Complete
- 14 forward declarations from earlier phases fully implemented
- All MVP features delivered
- No mandatory work left undone

### 📭 5 MVP Deferrals (Backend ✅, UI 📭)
All backend implementations working; UI consumers Phase 7+:
1. `recover_from_cloud` — Manifest re-import flow
2. `migrate_vault` — Cross-destination transfer
3. `sync_backup` — Backup/restore lifecycle
4. `get_file_content` — In-app viewer ready (50 MiB cap)
5. `get_sync_status` — Fully wired and working ✅

### 📋 8 Intentional MVP Scope Limitations
Not bugs; documented architectural decisions:
1. **Single-vault per device** — Multi-vault Phase 7+ research (13 pts)
2. **Files-only deletion** — Directory recursion Phase 7+ (6 pts)
3. **JPEG/PNG EXIF only** — Video MP4 moov EOF constraint (5 pts)
4. **Detect-and-block conflicts** — Three-way merge Phase 7+ (8 pts)
5. **Default revocation** — Crypto can't revoke fetched plaintext
6. **Fingerprint display-only** — Trust history Phase 7+ (5 pts)
7. **No TOTP apps** — USB key required for deterministic KDF
8. **OS trust assumption** — Permanent; crypto ≤ OS security

### 🔮 8 Phase 7+ Candidates (Prioritized)
**Must-Have** (blocks other features):
- Multi-vault support (13 pts)
- Advanced recovery flows (8 pts)
- Conflict resolution (8 pts)

**Nice-to-Have** (independent):
- Fingerprint trust model (5 pts)
- Directory operations (6 pts)
- Video EXIF stripping (5 pts)
- Strong revocation re-keying (4 pts)
- Performance optimization (7 pts)

### ⚠️ Non-Critical Items
- **28 mock trait stubs** in test utilities (`MockSharingStoreForFetch`, `FakeMetadataStore`)
- **UI TODOs**: Show delete error, export public key, list refresh, sync polling
- All non-blocking; quality-of-life enhancements Phase 7+

### 🔒 Out-of-Scope (Permanent)
- Compromised OS recovery
- Malicious cloud provider (BLAKE3 detects; can't prevent)
- Malicious Rclone sidecar
- TOTP multi-factor (must be deterministic)

---

## Component Status Scorecard

| Component | Critical | High | Medium | Low | Total | Status |
|-----------|----------|------|--------|-----|-------|--------|
| **Sharing** | 0 | 0 | 1 | 2 | 3 | ✅ |
| **Sync/Cloud** | 0 | 2 | 1 | 1 | 4 | ✅ |
| **Storage** | 0 | 1 | 1 | 1 | 3 | ✅ |
| **Auth** | 0 | 0 | 0 | 2 | 2 | ✅ |
| **UI/Frontend** | 0 | 1 | 3 | 1 | 5 | ✅ |
| **Crypto** | 0 | 0 | 0 | 0 | 0 | ✅ |
| **Total (prod)** | **0** | **4** | **6** | **14** | **24** | ✅ |

---

## Phase 7+ Roadmap (Recommended Order)

### Month 1: Research & Design
- [ ] Multi-vault dependency analysis
- [ ] Conflict resolution heuristics
- [ ] Video EXIF handling approaches
- [ ] Strong revocation cost-benefit

### Month 2: Core Implementation
1. **Advanced recovery flows** (3 pts) → highest user value
2. **Multi-vault support** (13 pts) → enables future work
3. **Conflict resolution** (8 pts) → reliability

### Month 3: Completion
4. **Directory operations** (6 pts)
5. **Performance optimization** (7 pts)
6. **Fingerprint trust model** (5 pts)

---

## Quick Query Guide

All findings tracked in SQL:

```sql
-- View all deferred items
SELECT feature_name, component, severity, status 
FROM deferred_items 
ORDER BY severity DESC, feature_name;

-- View code markers by component
SELECT component, COUNT(*) as count, GROUP_CONCAT(marker_type, ', ')
FROM code_markers 
GROUP BY component;

-- View production issues only
SELECT id, file_path, line_number, marker_type, content
FROM code_markers 
WHERE component NOT LIKE '%test%' AND severity IN ('high', 'medium')
ORDER BY file_path;
```

---

## Key Insights

**What was implemented correctly**:
- All cryptographic primitives per RFC standards (HKDF-SHA256, XChaCha20-Poly1305, BLAKE3)
- All session lifecycle management with memory-locking
- All cloud synchronization with conflict detection
- All file-sharing with HPKE/RFC 9180
- All IPC command surface with error sanitization
- All Zero-Trace compliance (no sensitive data in UI/logs)

**What was intentionally deferred**:
- Multi-vault UI coordination (requires design, not urgent for single-user MVP)
- Advanced recovery (backend complete; UI wizard Phase 7+)
- Directory deletion (complexity > value for Phase 6)
- Video EXIF (architectural constraint, not blocker)
- Fingerprint history tracking (display foundation done; history Phase 7+)

**What was designed but not yet implemented**:
- Orphan blob detection (deferrable for MVP; full manifest scan expensive)
- Epoch buffering upload (infrastructure ready; disabled for MVP caution)
- Strong revocation re-keying (cryptographic limitation requires feature design)

---

## Next Steps

1. **Immediate** (no action): Use this audit as Phase 7 requirements baseline
2. **Short-term** (1–2 weeks): Schedule Phase 7 planning session; prioritize roadmap
3. **Research** (ongoing): Deep-dive on multi-vault dependency graph; conflict resolution heuristics
4. **Design** (2–3 weeks): Multi-vault design document; recovery flows UI specification
5. **Implementation** (4+ weeks): Execute Phase 7 roadmap in recommended order

---

## Document References

- **Full Audit**: `.claude/reviews/deferred-items-audit-20260423.md`
- **Official Inventory**: `docs/architecture/deferred-items-inventory.md`
- **Design Invariants**: `docs/architecture/design-invariants.md`
- **Phase 7 Planning**: `docs/deferred_items_handling/phase-7-roadmap.md`

