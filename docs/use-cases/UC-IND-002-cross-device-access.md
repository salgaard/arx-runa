# UC-IND-002: Cross-Device Secure File Access

**Category**: Individual Privacy

**Status**: Active

---

## Overview

An individual user wants to access their encrypted files from multiple devices (home PC, work laptop, tablet) without exposing plaintext to the cloud provider or maintaining separate copies.

## Actors

- **Primary Actor**: Individual user with multiple devices
- **Secondary Actors**: Cloud storage provider (untrusted), VoidGate system, USB key file (portable between devices)

## Preconditions

- User has VoidGate installed on multiple devices
- User has a portable USB key file (same key used across all devices)
- User has configured the same Rclone backend on all devices
- User has previously created a vault and pushed encrypted manifest to cloud

## Main Flow

1. User connects to new/secondary device
2. User launches VoidGate on secondary device
3. User inserts USB drive with key file
4. User selects "Pull Vault from Cloud"
5. User provides password
6. VoidGate downloads vault header from cloud (unencrypted salt + metadata)
7. VoidGate derives encryption keys using Argon2id(password || key_file_bytes, salt)
8. VoidGate downloads encrypted manifest from cloud
9. VoidGate decrypts manifest with manifest_key (derived via HKDF from master_key)
10. VoidGate displays file tree (filenames and metadata now visible locally)
11. User selects a file to download
12. VoidGate downloads encrypted chunks from cloud (by UUID blob names)
13. VoidGate verifies BLAKE3 checksums on encrypted chunks
14. VoidGate decrypts chunks with file_key (unwrapped from manifest)
15. VoidGate reassembles plaintext file in memory
16. User opens/edits file locally
17. User saves changes
18. VoidGate re-encrypts modified file with new nonces
19. VoidGate uploads new encrypted chunks to cloud
20. VoidGate updates and pushes encrypted manifest
21. User locks vault and removes USB key

## Alternate Flows

### Manifest Out of Sync

**Trigger**: User made changes on Device A but hasn't pulled latest manifest to Device B

**Steps**:
1. VoidGate detects local manifest snapshot_counter < cloud snapshot_counter
2. VoidGate prompts: "Cloud has newer version. Pull and merge?"
3. If user accepts: VoidGate downloads latest manifest and merges changes
4. If user declines: VoidGate warns "Working with stale manifest — conflicts possible"
5. Flow continues from Main Flow step 10

### Concurrent Edits (Conflict)

**Trigger**: User edits same file on Device A and Device B before syncing

**Steps**:
1. User pushes from Device A (snapshot_counter increments)
2. User attempts to push from Device B with stale manifest
3. VoidGate detects conflict (file modified on both devices)
4. VoidGate prompts: "File conflict detected. Keep local, keep cloud, or manual merge?"
5. User selects resolution strategy
6. VoidGate creates conflict copy if needed (with timestamp suffix)
7. Flow continues with conflict resolved

### USB Key Forgotten at Home

**Trigger**: User at work without USB key file

**Steps**:
1. User attempts to unlock vault
2. VoidGate displays: "Key file not found — vault cannot be unlocked"
3. User cannot access files until USB key is retrieved
4. Flow terminates (by design — no password-only fallback)

### Download-Only Mode

**Trigger**: User wants read-only access on shared/public device

**Steps**:
1. User follows Main Flow steps 1-15 (download and decrypt)
2. User views files but does not edit
3. User locks vault without pushing changes
4. No manifest updates or uploads occur
5. Flow completes with read-only access

## Success Criteria

- User can access vault from any device with password + USB key
- Cloud manifest stays synchronized across devices
- Conflicts are detected and user is prompted for resolution
- No device stores plaintext files persistently (files decrypted in memory only)
- USB key file is the authoritative hardware factor (portable, required on all devices)
- Each device can independently pull latest vault state from cloud

## Related Designs

- [Authentication & Session Management](../architecture/designs/authentication-and-session-management/design.md) — Portable USB key file, consistent key derivation across devices, session timeout
- [Cryptographic Primitives](../architecture/designs/cryptographic-primitives/design.md) — Deterministic key derivation (same password + key file → same keys on all devices)
- [Chunking & Manifest](../architecture/designs/chunking-and-manifest/design.md) — Manifest snapshot counter for sync detection, SQLCipher local database per device
- [Cloud Synchronisation](../architecture/designs/cloud-synchronisation/design.md) — Push/pull flows, manifest versioning, atomic updates, conflict detection

## Security Considerations

### Threats Addressed

- **Cloud provider correlation**: Cloud cannot link devices to same user (only sees random UUID uploads)
- **Device compromise**: If one device is compromised, other devices remain secure (no plaintext on disk)
- **Shared computer risk**: User can access vault temporarily without leaving plaintext artifacts
- **Physical key theft**: Attacker needs USB key + password + access to configured Rclone backend

### Assumptions

- USB key file is physically secured during transport between devices
- All devices running VoidGate are trusted (no malware capturing keys during session)
- User remembers to lock vault when leaving device unattended
- Network between devices and cloud is not trusted (TLS assumed but not required by VoidGate)

### Out of Scope

- Automatic conflict resolution (user must manually resolve)
- Real-time sync across devices (push/pull model, not live collaboration)
- Revocation of access for lost USB key (user must rotate vault if USB key compromised)
- Multi-user access control (single-user vault only in current design)

## Notes

This use case extends UC-IND-001 to multi-device scenarios. It highlights the importance of:
- Deterministic key derivation (same inputs → same keys)
- Portable hardware factor (USB key works on any device)
- Conflict detection via manifest versioning

**Current Limitation**: VoidGate does not support real-time sync or automatic merging. Cross-device usage requires explicit pull/push operations and manual conflict resolution.

**Future Enhancements**:
- Automated conflict resolution strategies (last-write-wins, versioning)
- Live sync with operational transformation (CRDT)
- Per-device audit log in manifest

---

**References**:
- Dropbox Sync Architecture (for comparison of cloud sync patterns)
- CRDTs (Conflict-free Replicated Data Types) for future live collaboration
