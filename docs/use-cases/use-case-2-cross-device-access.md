# use-case-2: Cross-Device Synchronisation

**Category**: Individual Privacy

**Status**: Active

---

## Overview

An individual user wants to access and edit their encrypted files from multiple devices (home PC, work laptop, tablet) using the same vault. The cloud manifest acts as the synchronisation source of truth; conflicts are detected and resolved manually.

## Actors

- **Primary Actor**: Individual user with multiple devices
- **Secondary Actors**: Cloud storage provider (untrusted), Arx Runa system, USB key file (Tier 2 vaults only)

## Preconditions

- User has Arx Runa installed on multiple devices with the same Rclone backend configured
- User has previously created a vault and pushed an encrypted manifest to cloud (see [use-case-1](use-case-1-personal-file-backup.md))
- For Tier 2 vaults: same USB key file is available on the secondary device

## Main Flow

1. User launches Arx Runa on secondary device and selects "Pull Vault from Cloud"
2. User authenticates (password for Tier 1 vaults; password + USB key for Tier 2 vaults)
3. Arx Runa derives encryption keys and downloads the vault manifest from cloud
4. Arx Runa decrypts manifest and displays file browser
5. User selects a file to download
6. Arx Runa downloads and decrypts the file, verifying integrity
7. User views files in-app (Zero-Trace)
8. To update a file, user uploads the modified version via the drop zone
9. Arx Runa encrypts and uploads the updated file, replacing the previous version
10. Arx Runa increments the manifest version and pushes the updated manifest to cloud
11. User locks vault and removes USB key (if Tier 2)

## Alternate Flows

### Manifest Out of Sync

**Trigger**: Cloud manifest has a newer snapshot_counter than local copy

**Steps**:
1. Arx Runa detects local snapshot_counter < cloud snapshot_counter
2. Arx Runa prompts: "Cloud has a newer version — pull latest?"
3. If accepted: Arx Runa downloads the latest manifest from cloud, replacing the local copy
4. If declined: Arx Runa warns "Working with stale manifest — conflicts possible"

### Concurrent Edit Conflict

**Trigger**: Same file was edited on two devices before either pushed

**Steps**:
1. User pushes from Device A (snapshot_counter increments)
2. User attempts to push from Device B with stale manifest
3. Arx Runa detects conflict and prompts: "Keep local, keep cloud, or view both?"
4. User selects resolution; Arx Runa creates a conflict copy with a disambiguated name if needed

### USB Key Not Available (Tier 2 Vault)

**Trigger**: User at secondary device without their USB key

**Steps**:
1. User attempts to access a Tier 2 vault
2. Arx Runa displays: "Key file not found — insert USB drive"
3. User cannot access Tier 2 vault until USB key is available
4. Tier 1 vaults remain accessible with password only

### Download-Only Mode

**Trigger**: User wants read-only access on a shared or public device

**Steps**:
1. User follows Main Flow steps 1–6 (authenticate, pull, download, decrypt)
2. User views files but does not edit
3. User locks vault without pushing any changes

### Edit File Externally

**Trigger**: User wants to edit a file in an external application

**Steps**:
1. User exports a decrypted copy to disk (see [use-case-1](use-case-1-personal-file-backup.md) Export alternate flow)
2. User edits the file in an external application
3. User uploads the modified file back via the drop zone
4. Arx Runa encrypts the updated file and replaces the previous version
5. The exported copy remains on disk — the user is responsible for deleting it

## Success Criteria

- User can access vault from any device with the correct authentication factors
- Cloud manifest stays synchronised; snapshot_counter detects divergence
- Conflicts are detected and user is prompted for resolution
- Tier 1 vaults are accessible with password only; Tier 2 vaults require USB key on each device
- No device stores plaintext persistently unless the user explicitly exports a file

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

- All devices running Arx Runa are trusted (no malware capturing keys during session)
- User remembers to lock vault when leaving a device unattended
- Network between devices and cloud is not trusted (Arx Runa does not rely on transport security)

### Out of Scope

- Automatic conflict resolution (user must resolve manually)
- Real-time sync across devices (push/pull model, not live collaboration)
- Multi-user access control (single-user vault only in current design)

## Notes

Cross-device sync requires explicit pull/push operations — Arx Runa does not run a background sync daemon. For Tier 2 vaults, carrying the USB key between devices is a deliberate security trade-off.
