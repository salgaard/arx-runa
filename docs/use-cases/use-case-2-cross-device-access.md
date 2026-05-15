# Use Case 2: Cross-Device Synchronisation

## Overview

An individual user wants to access and edit their encrypted files from multiple devices (home PC, work laptop, tablet) using the same vault. The cloud manifest acts as the synchronisation source of truth; conflicts are detected and resolved manually.

## Actors

- **Primary Actor**: Individual user with multiple devices
- **Secondary Actors**: Cloud storage provider (untrusted), Arx Runa system, USB key file (Tier 2 vaults only)

## Preconditions

- User has Arx Runa installed on all devices
- The secondary device has `cloud-config.json` already present (either copied from the primary device or produced by the new-device bootstrap — see Alternate Flow below)
- User has previously created a vault and pushed an encrypted manifest to cloud (see [use-case-1](use-case-1-personal-file-backup.md))
- For Tier 2 vaults: the USB key file is available on the secondary device

## Main Flow

_This describes ongoing use on a device that already has a local vault state (manifest present). For first-time use on a new device, see the "First Time on This Device" alternate flow below._

1. User launches Arx Runa on secondary device
2. User authenticates (password for Tier 1 vaults; password + USB key for Tier 2 vaults)
3. Arx Runa derives encryption keys and opens the local manifest, displaying the file browser
4. User selects a file to download
5. Arx Runa downloads encrypted chunks from cloud and decrypts them, verifying integrity
6. User views files in-app (Zero-Trace)
7. To update a file, user uploads the modified version via the drop zone
8. Arx Runa encrypts and stages the updated file locally
9. User triggers sync; Arx Runa increments the snapshot counter, uploads the updated chunks and manifest backup to cloud
10. User locks vault and removes USB key (if Tier 2)

## Alternate Flows

### First Time on This Device

**Trigger**: Secondary device has Arx Runa installed but has never accessed this vault (no local manifest, no `cloud-config.json`)

**Steps**:
1. User clicks "Recover vault from cloud" on the vault picker screen
2. User enters the cloud endpoint details (Rclone remote name, bucket, region), vault password, and (Tier 2) path to the USB key file on the recovery page; Arx Runa writes `cloud-config.json` to the local app data directory
3. Arx Runa downloads `vault-header.json` (plaintext) from the cloud root
4. Arx Runa derives encryption keys and downloads `manifest/manifest-backup.blob` from cloud
5. Arx Runa decrypts the manifest backup and writes the local SQLCipher database
6. Device is now fully set up; continue from Main Flow step 3

### Recover with Recovery Phrase

**Trigger**: User has lost their vault password but retains their 24-word recovery phrase

**Steps**:
1. User clicks "Recover vault from cloud" on the vault picker screen, or selects "Forgot password?" on the login page
2. User selects the "Recovery phrase" mode and enters the cloud endpoint details, their 24-word recovery phrase, and (Tier 2) path to the USB key file
3. Arx Runa downloads `vault-header.json` from the cloud root
4. Arx Runa derives encryption keys from the recovery phrase and downloads `manifest/manifest-backup.blob` from cloud
5. Arx Runa decrypts the manifest backup and writes the local SQLCipher database
6. Device is now fully set up; continue from Main Flow step 3

### Manifest Out of Sync

**Trigger**: User syncs (pushes) and Arx Runa detects the cloud `snapshot_counter` is ahead of the local copy

**Steps**:
1. Arx Runa detects cloud snapshot_counter > local snapshot_counter during sync
2. Arx Runa shows dialog: "Another device has synced. Pull changes and continue?"
3. If accepted: Arx Runa runs `pull_and_reconcile`, downloads the latest manifest from cloud replacing the local copy, then retries sync
4. If declined: Arx Runa shows a persistent banner "Working with stale manifest — conflicts possible"; user can pull at any time via the banner

### Concurrent Edit Conflict

**Trigger**: Same file was edited on two devices before either pushed

**Steps**:
1. User pushes from Device A (snapshot_counter increments)
2. User attempts to push from Device B with stale manifest
3. Arx Runa detects conflict during sync (snapshot_counter mismatch) and prompts: "Another device has synced. Pull changes and continue?"
4. User accepts pull: Arx Runa downloads cloud manifest and replaces local copy
5. Locally-pending files whose names collide with cloud entries are automatically renamed with a `(conflicted copy)` suffix (e.g. `report.pdf` → `report (conflicted copy).pdf`)
6. Arx Runa retries sync; both the cloud version and the renamed local version are uploaded

### USB Key Not Available (Tier 2 Vault)

**Trigger**: User at secondary device without their USB key

**Steps**:
1. User attempts to access a Tier 2 vault
2. Arx Runa displays: "No key file selected"
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
- Conflicts are detected when syncing; pending local files are preserved as conflict copies when they collide with cloud state
- Tier 1 vaults are accessible with password only; Tier 2 vaults require USB key on each device
- No device stores plaintext persistently unless the user explicitly exports a file

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
