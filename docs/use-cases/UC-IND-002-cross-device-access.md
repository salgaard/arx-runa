# UC-IND-002: Cross-Device Synchronisation

**Category**: Individual Privacy

**Status**: Active

---

## Overview

An individual user wants to access and edit their encrypted files from multiple devices (home PC, work laptop, tablet) using the same vault. The cloud manifest acts as the synchronisation source of truth; conflicts are detected and resolved manually.

## Actors

- **Primary Actor**: Individual user with multiple devices
- **Secondary Actors**: Cloud storage provider (untrusted), VoidGate system, USB key file (Tier 2 folders only)

## Preconditions

- User has VoidGate installed on multiple devices with the same Rclone backend configured
- User has previously created a vault and pushed an encrypted manifest to cloud (see UC-IND-001)
- For Tier 2 folders: same USB key file is available on the secondary device

## Main Flow

1. User launches VoidGate on secondary device and selects "Pull Vault from Cloud"
2. User authenticates (password for Tier 1 folders; password + USB key for Tier 2 folders)
3. VoidGate derives keys and downloads encrypted manifest from cloud (see UC-IND-001 for derivation detail)
4. VoidGate decrypts manifest and displays file browser
5. User selects a file to download
6. VoidGate downloads encrypted chunks, verifies BLAKE3 checksums, decrypts with file_key
7. User opens and edits file locally
8. VoidGate re-encrypts modified file with new nonces and uploads new chunks
9. VoidGate increments manifest snapshot_counter and pushes updated manifest to cloud
10. User locks vault and removes USB key (if Tier 2)

## Alternate Flows

### Manifest Out of Sync

**Trigger**: Cloud manifest has a newer snapshot_counter than local copy

**Steps**:
1. VoidGate detects local snapshot_counter < cloud snapshot_counter
2. VoidGate prompts: "Cloud has newer version — pull and merge?"
3. If accepted: VoidGate downloads latest manifest and merges changes
4. If declined: VoidGate warns "Working with stale manifest — conflicts possible"

### Concurrent Edit Conflict

**Trigger**: Same file was edited on two devices before either pushed

**Steps**:
1. User pushes from Device A (snapshot_counter increments)
2. User attempts to push from Device B with stale manifest
3. VoidGate detects conflict and prompts: "Keep local, keep cloud, or view both?"
4. User selects resolution; VoidGate creates conflict copy with timestamp suffix if needed

### USB Key Not Available (Tier 2 Folder)

**Trigger**: User at secondary device without their USB key

**Steps**:
1. User attempts to access a Tier 2 folder
2. VoidGate displays: "Key file not found — insert USB drive"
3. User cannot access Tier 2 folder until USB key is available
4. Tier 1 folders remain accessible with password only

### Download-Only Mode

**Trigger**: User wants read-only access on a shared or public device

**Steps**:
1. User follows Main Flow steps 1–6 (authenticate, pull, download, decrypt)
2. User views files but does not edit
3. User locks vault without pushing any changes

## Success Criteria

- User can access vault from any device with the correct authentication factors
- Cloud manifest stays synchronised; snapshot_counter detects divergence
- Conflicts are detected and user is prompted for resolution
- Tier 1 folders are accessible with password only; Tier 2 folders require USB key on each device
- No device stores plaintext persistently unless the user explicitly downloads a file

## Related Designs

- [Authentication & Session Management](../architecture/designs/authentication-and-session-management/design.md)
- [Cryptographic Primitives](../architecture/designs/cryptographic-primitives/design.md)
- [Chunking & Manifest](../architecture/designs/chunking-and-manifest/design.md)
- [Cloud Synchronisation](../architecture/designs/cloud-synchronisation/design.md)

## Security Considerations

### Threats Addressed

- **Cloud provider correlation**: Cloud sees only random UUID uploads from different devices
- **Device compromise**: Compromise of one device does not affect other devices (no plaintext at rest)
- **Shared computer risk**: User can access vault temporarily without leaving plaintext artifacts

### Assumptions

- All devices running VoidGate are trusted (no malware capturing keys during session)
- User remembers to lock vault when leaving a device unattended
- Network between devices and cloud is not trusted (VoidGate does not rely on transport security)

### Out of Scope

- Automatic conflict resolution (user must resolve manually)
- Real-time sync across devices (push/pull model, not live collaboration)
- Multi-user access control (single-user vault only in current design)

## Notes

Cross-device sync requires explicit pull/push operations — VoidGate does not run a background sync daemon. For Tier 2 folders, carrying the USB key between devices is a deliberate security trade-off.
