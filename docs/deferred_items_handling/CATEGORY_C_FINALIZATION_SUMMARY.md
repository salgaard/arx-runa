# Category C Design Decisions — Finalization Summary

**Date**: 2026-04-23  
**Status**: ✅ ALL 8 DECISIONS DOCUMENTED AND FINALIZED

---

## Executive Summary

All 8 Category C architectural decisions have been explicitly documented in their respective design documents with clear closure and Phase 7+ roadmap references. These are intentional MVP scope limitations that will persist through Phase 6 unless explicitly rethought in Phase 7+ planning.

**Result**: Zero "TBD" items remain; all decisions have explicit rationale and phase closures.

---

## Decision Summary Table

| # | Decision | Design File | Status | Phase 7+ Roadmap |
|---|----------|-------------|--------|------------------|
| 1 | **c-uuid-nodeid-migration** — NodeId at domain, Uuid at trait | chunking-and-manifest/design.md | ✅ Documented | Candidate for targeted refactor |
| 2 | **c-directory-deletion** — Files-only MVP | chunking-and-manifest/design.md | ✅ Documented | Phase 7 feature |
| 3 | **c-file-conflict-detection** — Detect-and-block MVP | cloud-synchronisation/design.md | ✅ Documented | Phase 7 research (timestamps) |
| 4 | **c-fingerprint-verification-ux** — Display-only, out-of-band | file-sharing/design.md | ✅ Implemented | Phase 7+ (history tracking) |
| 5 | **c-inapp-file-viewer** — Backend ready, UI deferred | tauri-ipc-and-frontend/design.md | ✅ Documented | Phase 6.8+ (viewers added) |
| 6 | **c-multi-vault-support** — Single vault per device | tauri-ipc-and-frontend/design.md | ✅ Documented | Phase 7+ (major refactor) |
| 7 | **c-optimistic-locking** — Out of scope | cloud-synchronisation/design.md | ✅ Documented | Phase 7 research (perf) |
| 8 | **c-compromised-os-threat** — Out of scope | authentication-and-session-management/design.md | ✅ Documented | Permanent limitation |

---

## Updates Made

### 1. chunking-and-manifest/design.md

**Section Added**: "Category C: Architectural Decisions (Finalized)"

```markdown
| Decision | Status | Rationale | Notes |
|----------|--------|-----------|-------|
| c-uuid-nodeid-migration | ✅ Finalized | Type safety provided at domain layer via NodeId wrapper; trait boundary uses Uuid for persistence contract abstraction. Avoids broad API churn across Phase 3–5 contracts. | Documented in Deferred Items Inventory Category C |
| c-directory-deletion | ✅ Finalized | Directory deletion requires recursive enumeration and cascade blob cleanup. MVP focuses on per-file operations. `delete_directory` is a Phase 7+ feature with separate IPC command + MetadataStore extension. | Documented in Deferred Items Inventory Category C |
| c-inapp-file-viewer | ✅ Finalized | `get_file_content` command is implemented with 50 MiB cap. Infrastructure is production-ready; in-app viewer UI is Phase 6.8+ feature. Future phases can add viewers without backend changes. | Command registered in canonical surface; UI consumer deferred per Deferred Items Inventory Category D |
```

**Verification**: Documents existing design decisions; establishes architectural layering as intentional.

---

### 2. tauri-ipc-and-frontend/design.md

**Section Added**: "Category C: Architectural Decisions (Finalized)"

```markdown
| Decision | Status | Rationale | Notes |
|----------|--------|-----------|-------|
| c-multi-vault-support | ✅ Finalized | Current AppState design holds one vault. Multi-vault support requires per-vault session coordination, UI vault switcher, and AppState refactor. Deferred to Phase 7+ as architectural extension. | Documented in Deferred Items Inventory Category C and Category H |
| c-inapp-file-viewer | ✅ Finalized | get_file_content command is fully implemented with 50 MiB cap. Tauri sidecar infrastructure ready; in-app viewer UI (text, image, PDF rendering) is Phase 6.8+ feature. Backend can add viewers without command changes. | Documented in Deferred Items Inventory Category D |
```

**Verification**: Establishes backend readiness vs. UI deferral distinction; links to Phase 7 planning.

---

### 3. file-sharing/design.md

**Section Added**: "Category C: Architectural Decisions (Finalized)"

```markdown
| Decision | Status | Rationale | Notes |
|----------|--------|-----------|-------|
| c-fingerprint-verification-ux | ✅ Implemented | Fingerprint verification is shown in UI (16-character lowercase hex from SHA-256(public_key)). Out-of-band verification (phone call, in person, QR code) is user responsibility. No UX forcing or automated trust tracking in Phase 6. | Implemented in Phase 6.8 UI; documented in Deferred Items Inventory Category C |
```

**Verification**: Documents Phase 6.8 implementation; clarifies scope boundary for Phase 7+ extensions.

---

### 4. cloud-synchronisation/design.md

**Section Added**: "Category C: Architectural Decisions (Finalized)"

```markdown
| Decision | Status | Rationale | Notes |
|----------|--------|-----------|-------|
| c-file-conflict-detection | ✅ Finalized | Manifest-level conflicts detected via snapshot_counter. File-level conflicts (same file modified on multiple devices) require manual resolution. Three-way merge out of scope. | Documented in Deferred Items Inventory Category C and Category H |
| c-optimistic-locking | ✅ Finalized | Optimistic locking with conditional writes (AWS S3 ETags, etc.) is provider-specific and out of scope. Current detect-and-block with snapshot_counter is sufficient for MVP. | Documented in Deferred Items Inventory Category C as "Out of scope" |
| c-compromised-os-threat | ✅ Finalized | Arx Runa assumes the OS is trusted. Cryptography cannot be stronger than the OS. Threat of malicious OS/binaries is out of scope and accepted as a permanent limitation. | Documented in threat model section above; Deferred Items Inventory Category F |
```

**Verification**: Establishes conflict detection as MVP limitation; documents provider-specific constraints; references threat model.

---

### 5. authentication-and-session-management/design.md

**Section Added**: "Category C: Architectural Decisions (Finalized)"

```markdown
| Decision | Status | Rationale | Notes |
|----------|--------|-----------|-------|
| c-compromised-os-threat | ✅ Finalized | Arx Runa assumes the OS is trusted. Cryptography cannot be stronger than the OS itself. Threat of malicious OS/binaries is out of scope and documented as an accepted permanent limitation in the threat model. | Documented in Deferred Items Inventory Category F |
```

Plus comprehensive threat model statement establishing:
- **Threats Mitigated**: untrusted cloud, network eavesdropping, disk theft, lost credentials
- **Threats Out of Scope**: malicious OS, cold-boot attacks, supply-chain compromise
- **Consistency**: aligned with Tahoe-LAFS and Cryptomator threat models

**Verification**: Establishes accepted threat model boundaries; documents permanent limitations.

---

### 6. FINGERPRINT_UI_GUIDE.md

**Header Updated**: Added implementation status and Category C reference

```markdown
> **Status**: ✅ IMPLEMENTED in Phase 6.8  
> **Last updated**: 2026-04-23

## Implementation Status

This document describes the fingerprint verification UI which has been fully implemented in Phase 6.8. 

**Category C Decision**: Fingerprint verification is display-only and out-of-band. Automated verification history tracking and trust warnings are Phase 7+ enhancements (see Deferred Items Inventory Category H).
```

**Verification**: Marks implementation as complete; cross-references deferred enhancements.

---

## Phase 7+ Roadmap Integration

All 8 decisions reference the Phase 7+ roadmap via [Deferred Items Inventory](docs/architecture/deferred-items-inventory.md):

- **Category H items** (Phase 7+ candidates): 
  - Multi-vault support (13 pts) — blocks other features
  - Conflict resolution enhancement (8 pts) — three-way merge research
  - Fingerprint trust model (5 pts) — verification history
  - Directory operations (6 pts) — recursive deletion
  
- **Permanent limitations** (Category F):
  - Compromised OS recovery
  - Malicious cloud provider (mitigation-only)
  - Malicious Rclone sidecar
  - TOTP/authenticator apps (not deterministic)
  - Transparent multi-vault switching

---

## Verification Checklist

✅ **Decision 1 (c-uuid-nodeid-migration)**: NodeId wrapper at domain, Uuid at trait. Documented as intentional layering with Phase 7 refactor candidate.

✅ **Decision 2 (c-directory-deletion)**: File-only MVP. Documented with Phase 7 feature deferral. MetadataStore extension planned.

✅ **Decision 3 (c-file-conflict-detection)**: Detect-and-block with `snapshot_counter`. Three-way merge out of scope. Manual resolution required.

✅ **Decision 4 (c-fingerprint-verification-ux)**: Display-only, 16-char hex fingerprint. Out-of-band verification user responsibility. Phase 6.8 implemented.

✅ **Decision 5 (c-inapp-file-viewer)**: Backend `get_file_content` ready. UI viewers deferred to Phase 6.8+. Command infrastructure production-ready.

✅ **Decision 6 (c-multi-vault-support)**: Single `AppState` per device. Phase 7+ major refactor required for multi-vault coordination.

✅ **Decision 7 (c-optimistic-locking)**: Provider-specific, out of scope. `snapshot_counter` detect-and-block sufficient for MVP.

✅ **Decision 8 (c-compromised-os-threat)**: Out of scope, documented as accepted permanent limitation. Aligns with Tahoe-LAFS threat model.

---

## No TBD Items Remain

- ✅ All 8 decisions have explicit closure
- ✅ All decisions reference Phase 7+ roadmap where applicable
- ✅ All limitations explicitly documented as accepted
- ✅ All deferred items captured in Deferred Items Inventory
- ✅ No "future enhancement" placeholders without explicit deferral

---

## Consistency Across Design Documents

All Category C sections follow uniform structure:

```markdown
## Category C: Architectural Decisions (Finalized)

These decisions are intentional MVP scope limitations that will persist through Phase 6. 
Phase 7+ planning may reconsider them with explicit research.

| Decision | Status | Rationale | Notes |
|----------|--------|-----------|-------|
| **c-IDENTIFIER** — Description | ✅ Status | Rationale paragraph | Cross-references |

**Phase 7+ Planning**: Reference to Deferred Items Inventory Category.
```

---

## Related Documentation

- **Deferred Items Inventory**: Complete audit of all phases (0–6.9) with Category C/D/E/F/G/H classifications
- **Design Invariants**: Cross-phase contract references for all decisions
- **Phase 7 Roadmap**: Ready for Phase 7+ planning kickoff
- **Threat Model**: Explicit scope statement in auth/session design

---

## Commit Message

```
docs: Finalize Category C design decisions with explicit phase closures

All 8 Category C architectural decisions are now explicitly documented in their 
respective design documents:

1. c-uuid-nodeid-migration: Hybrid state intentional (chunking-and-manifest)
2. c-directory-deletion: Files-only MVP (chunking-and-manifest)
3. c-file-conflict-detection: Detect-and-block MVP (cloud-synchronisation)
4. c-fingerprint-verification-ux: Display-only, out-of-band (file-sharing) 
5. c-inapp-file-viewer: Backend ready, UI deferred (tauri-ipc-and-frontend)
6. c-multi-vault-support: Single vault MVP (tauri-ipc-and-frontend)
7. c-optimistic-locking: Provider-specific, out of scope (cloud-synchronisation)
8. c-compromised-os-threat: Out of scope permanent limitation (auth)

All decisions reference Phase 7+ roadmap and Deferred Items Inventory.
FINGERPRINT_UI_GUIDE.md marked as Phase 6.8 implementation complete.

No TBD or unresolved items remain; all MVP limitations explicitly documented.

Co-authored-by: Copilot <223556219+Copilot@users.noreply.github.com>
```

---

**Status**: ✅ FINALIZED  
**All 8 Decisions**: Documented and ready for Phase 7+ planning
