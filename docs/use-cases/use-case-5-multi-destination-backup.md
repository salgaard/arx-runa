# Use Case 5: Multi-Destination & Redundant Backup

## Overview

A user wants their encrypted vault backed up to more than one cloud provider simultaneously — for redundancy, cost diversification, or to migrate from one provider to another without downtime. Arx Runa lets the user add multiple destinations, each with its own backup mode (mirror or accumulating), and designate one as the primary. Sync pushes to all active destinations; the primary is used as the read source when pulling the vault.

## Actors

- **Primary Actor**: Individual user with an existing vault
- **Secondary Actors**: Two or more cloud storage providers (both untrusted), Arx Runa system

## Preconditions

- User has Arx Runa installed and has at least one vault configured with a primary destination
- User has credentials for a second cloud provider (e.g., Backblaze B2 account, Google Drive Service Account)

## Main Flow — Add a Mirror Destination

1. User opens the Destinations page
2. User clicks "Add Destination", enters a label, and selects backup mode "Mirror"
3. User selects the provider type (Backblaze B2, Google Drive, local path, S3-compatible, etc.) and fills in the provider-specific fields
4. User clicks "Add Destination" — Arx Runa registers the new destination
5. User triggers sync
6. Arx Runa encrypts and uploads all pending chunks to every active destination
7. Both destinations now hold an identical set of encrypted blobs
8. If either destination is unreachable, Arx Runa records the backup failure and completes the upload to the reachable destination

## Alternate Flows

### Accumulating Mode (Retain Deleted Files)

**Trigger**: User wants a secondary destination that keeps a historical copy of deleted files

**Steps**:
1. User adds a new destination and selects backup mode "Accumulating"
2. When the user deletes a file from the vault and syncs, Arx Runa removes the chunks from mirror destinations but **retains** them on accumulating destinations
3. The primary destination always reflects the current vault state; the accumulating destination acts as a long-term archive
4. To recover a deleted file the user must restore from the accumulating destination manually (not yet supported in-app)

### Cloud Provider Migration

**Trigger**: User wants to switch from one cloud provider to another with no data loss and minimal downtime

**Steps**:
1. User adds the new provider as a destination with backup mode "Mirror"
2. User syncs — Arx Runa uploads all encrypted blobs to the new destination (UUID names and content unchanged; no re-encryption needed)
3. User verifies the new destination is healthy (no backup failures shown)
4. User clicks "Set as Primary" on the new destination — Arx Runa promotes it and demotes the old primary
5. User deletes the old destination once confident the migration is complete
6. No re-encryption required at any step — the same opaque blobs work across providers

### Backup Failure Recovery

**Trigger**: A destination becomes unreachable during or after a sync

**Steps**:
1. After a failed sync to a destination, Arx Runa displays a "N backup failures" badge on that destination in the list
2. User resolves the connectivity or credential issue (e.g., re-configures the Rclone backend)
3. User triggers sync — Arx Runa retries the failed destination alongside the others
4. On success, the failure badge clears

### Delete a Destination

**Trigger**: User wants to remove a destination (e.g., after migrating to a new provider)

**Steps**:
1. User clicks "Delete" on a non-primary destination
2. Arx Runa asks for confirmation
3. User confirms — Arx Runa removes the destination from the list; existing blobs on the remote are not deleted by Arx Runa
4. Primary destinations cannot be deleted; user must first promote another destination to primary

## Success Criteria

- Sync pushes identical encrypted blobs to every active destination in a single operation
- Mirror destinations always reflect the current vault state (deleted files are removed)
- Accumulating destinations retain blobs even when the corresponding file is deleted from the vault
- Exactly one destination is marked "primary" at all times; it cannot be deleted without first promoting another
- Backup failures are surfaced per-destination and cleared automatically on the next successful sync
- No re-encryption is required when migrating between providers

## Security Considerations

### Threats Addressed

- **Single provider failure**: Data survives the loss of any one provider when a mirror destination is configured
- **Provider lock-in**: Migration between providers never requires decrypting and re-encrypting; blobs are provider-agnostic
- **Accidental deletion**: Accumulating mode retains deleted chunks on at least one destination

### Assumptions

- Both providers are untrusted; neither receives encryption keys or plaintext at any point
- The user is responsible for keeping provider credentials up to date
- Arx Runa does not verify that a deleted destination's remote storage has been cleaned up — that is the user's responsibility

### Out of Scope

- Automatic conflict resolution between destinations that have diverged
- In-app restoration of files from an accumulating destination
- Bandwidth or cost management across destinations
