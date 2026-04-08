# Use Case 4: Personal File Sharing

## Overview

A user wants to share specific files from their vault with a friend or family member — holiday photos, a shared document, a home video — without exposing the content to the cloud provider. Both parties use Arx Runa; the sender encrypts the file's key so only the intended recipient can access it.

## Actors

- **Primary Actor**: Individual user sharing files (sender)
- **Secondary Actors**: Friend or family member receiving files (recipient), cloud storage provider (untrusted), Arx Runa system

## Preconditions

- Sender has Arx Runa installed with a vault containing the files to share
- Recipient has Arx Runa installed and has shared their public identity key with the sender
- Both parties use the same or compatible Rclone backend (or sender publishes the share to a mutually accessible location)

## Main Flow

1. Sender unlocks vault and selects a file to share (e.g., holiday photos folder)
2. Sender selects "Share" and enters the recipient's identifier (name or public key)
3. Arx Runa retrieves the file's encryption key
4. Arx Runa encrypts the file key so only the recipient can decrypt it
5. Arx Runa creates an encrypted share package containing the file's encryption key and cloud location
6. Arx Runa copies the encrypted file chunks to a shared area in the cloud
7. Sender delivers the share package to the recipient out-of-band (email, messaging, USB)
8. Recipient opens Arx Runa and imports the share package
9. Arx Runa displays the shared file in "Shared with Me"
10. Recipient decrypts the file key using their private key
11. Recipient downloads and decrypts the shared file
12. Recipient views the file in-app or exports a decrypted copy to disk
13. Arx Runa writes a download receipt (file_id, timestamp) to the cloud under `shared/<file_share_id>/receipts/`

## Alternate Flows

### Share Expiration

**Trigger**: Sender wants the share to expire after a set time

**Steps**:
1. Sender configures expiration (e.g., "expire after 30 days") when sharing
2. After expiration, recipient attempts to access the file
3. Arx Runa checks timestamp; displays "Share expired — contact sender for renewed access"
4. Shared file chunks are deleted from cloud; recipient can no longer access the file

### Revoke Access

**Trigger**: Sender changes their mind and wants to remove access

**Steps**:
1. Sender selects the shared file → "Revoke share"
2. Arx Runa deletes the shared file chunks from the cloud and removes the share record locally
3. Recipient pulls updated manifest; file no longer appears in "Shared with Me"

### Owner Notified of Download

**Trigger**: Sender opens Arx Runa after recipient has downloaded a shared file

**Steps**:
1. Sender unlocks vault and pulls latest manifest from cloud
2. Arx Runa reads the download receipt written by the recipient (file_id, timestamp)
3. Arx Runa displays: "Your shared file was downloaded by [recipient] on [date]"
4. No server required — receipt is a small encrypted blob written under `shared/<file_share_id>/receipts/` in the cloud, picked up on next pull

### Recipient Does Not Have Arx Runa

**Trigger**: Recipient is a non-technical user without Arx Runa installed

**Steps**:
1. Current design: recipient must have Arx Runa to decrypt the share package and access the file
2. Workaround: sender downloads and decrypts the file locally, then shares the plaintext via another channel (email, messaging app)
3. Sender accepts that the plaintext copy is outside Arx Runa's protection once exported

## Success Criteria

- File content is never exposed to the cloud provider during sharing
- Only the intended recipient (holder of the matching private key) can decrypt the shared file
- Sender can revoke access or set expiration at any time
- Cloud provider can see that shared data exists but cannot read its content or identify the recipient
- Recipient does not need access to the sender's vault or authentication factors
- Sender is notified (on next pull) when recipient downloads a shared file
- Recipient can export a decrypted copy to disk; sender is warned this is outside vault protection

## Related Designs

- [File Sharing](../architecture/designs/file-sharing/design.md)
- [Cryptographic Primitives](../architecture/designs/cryptographic-primitives/design.md)
- [Chunking & Manifest](../architecture/designs/chunking-and-manifest/design.md)
- [Cloud Synchronisation](../architecture/designs/cloud-synchronisation/design.md)

## Security Considerations

### Threats Addressed

- **Untrusted cloud provider**: Share metadata encrypted; cloud cannot see who shared what
- **Wrong recipient**: The file key is encrypted for the intended recipient only — others cannot decrypt it
- **Persistent access after revocation**: Shared chunks deleted from cloud on revoke; recipient who has already downloaded the file retains their local copy

### Assumptions

- Recipient's public key is obtained through a trusted channel (not from the cloud provider)
- Recipient pulls the latest manifest before accessing the shared file
- Revocation is not instant — recipient who cached the manifest retains access until they pull

### Out of Scope

- Sharing with recipients who do not have Arx Runa installed
- Cryptographic enforcement of read-only access (recipient holds the file key and can re-encrypt)
- Group sharing with multiple recipients simultaneously (future enhancement)

## Notes

File sharing is specified in the design document but not yet implemented. This use case describes intended behaviour. See [use-case-3](use-case-3-hardware-mfa-and-key-loss.md) for authentication factor considerations when the sender uses a Tier 2 vault.
