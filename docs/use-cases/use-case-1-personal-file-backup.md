# Use Case 1: Zero-Knowledge Personal Backup

## Overview

An individual user wants to back up sensitive personal files (documents, photos, videos) to cloud storage without exposing plaintext, filenames, or metadata to the cloud provider. Arx Runa uses a drop zone as the primary interface. When creating a vault the user chooses an authentication tier: Tier 1 (password-only) or Tier 2 (password + key file) — **Tier 2 is selected by default** for stronger out-of-box security. The tier applies to the entire vault — users who need different security levels create separate vaults.

## Actors

- **Primary Actor**: Individual user with sensitive personal files
- **Secondary Actors**: Cloud storage provider (untrusted), Arx Runa system

## Preconditions

- User has installed Arx Runa on their local machine
- User has configured an Rclone backend (e.g., Google Drive, Dropbox)

## Main Flow

1. User launches Arx Runa and selects "Create Vault"
2. Arx Runa prompts: "Choose authentication tier — Tier 1 (password only) or Tier 2 (password + key file)". Tier 2 is selected by default.
3. User selects a tier and completes setup (password for Tier 1; password + key file generation for Tier 2)
4. Arx Runa derives encryption keys from the provided credentials
5. Arx Runa unlocks vault and displays drop zone UI with vault file browser
6. User drags files or folders onto the drop zone
7. Arx Runa generates a unique encryption key for each file
8. Arx Runa splits and encrypts the file into fixed-size chunks
9. Arx Runa stores the encrypted chunks in the local vault database
10. User clicks sync
11. Arx Runa uploads vault-header, encrypted chunks and vault database saved as a manifest to cloud
12. User browses vault and views files in-app (Zero-Trace)
13. User locks vault (and removes key file if Tier 2)

## Alternate Flows

### Media Files (EXIF and In-Memory Viewing)

**Trigger**: User drops photos or videos onto the drop zone

**Steps**:
1. Arx Runa detects media file types
2. Arx Runa optionally strips EXIF metadata (GPS, camera model, timestamps) before encryption
3. Arx Runa encrypts and uploads as in Main Flow
4. When user opens a photo: Arx Runa decrypts chunks into RAM and renders in-app (no temp file written to disk)
5. For large videos: Arx Runa decrypts and streams progressively from cloud chunks

_Step 5 (video streaming) is not yet implemented._

### Export Decrypted File to Disk

**Trigger**: User wants to save a decrypted copy of a file outside Arx Runa (e.g., to edit in an external application)

**Steps**:
1. User selects a file in the vault browser and chooses "Download"
2. Arx Runa warns: "Exported file will be written to disk in plaintext, outside vault protection"
3. User confirms "Export Anyway" or cancels
4. Arx Runa prompts user to choose a save location
5. Arx Runa downloads encrypted chunks and decrypts in RAM
6. Arx Runa writes the plaintext file to the chosen location
7. User is responsible for the exported copy

### Cloud Provider Unavailable

**Trigger**: Rclone backend is unreachable

**Steps**:
1. Arx Runa completes local encryption and manifest update
2. When connectivity restores, user triggers sync and Arx Runa uploads pending chunks

### Cloud Provider Migration

**Trigger**: User wants to switch to a different cloud provider (e.g., from Google Drive to Backblaze B2)

**Steps**:
1. User adds the new provider as a destination with backup mode "Mirror" (Destinations page)
2. Arx Runa syncs all encrypted blobs to the new destination (UUID names and content unchanged)
3. User switches the new destination to "primary" on the Destinations page
4. User verifies sync and removes the old destination
5. No re-encryption required — data remains opaque to both providers

See [use-case-5](use-case-5-multi-destination-backup.md) for full multi-destination flows including mirror mode, accumulating mode, and backup failure recovery.

### File Already Exists

_This flow is not yet implemented. Arx Runa currently overwrites silently; a conflict prompt is planned._

## Success Criteria

- All files are encrypted in RAM before any data leaves the client
- Cloud provider receives only opaque blobs with random UUID names (no filenames, sizes, or metadata)
- Fixed-size chunks (default 4 MiB, configurable) hide exact file size from cloud provider
- EXIF metadata is stripped or encrypted before upload (media files)
- Decrypted content is displayed in-memory — no plaintext written to disk (Zero-Trace)
- Drop zone is the primary upload interface; a file picker button is also available as a supplementary upload method. Both files and folders can be dragged onto the drop zone.
- User selects authentication tier (Tier 1 or Tier 2) when creating the vault
- Tier 1 vault requires password only; Tier 2 vault additionally requires a key file
- Vault cannot be opened without the correct authentication factors for the chosen tier

See also [use-case-5](use-case-5-multi-destination-backup.md) for multi-destination and redundant backup scenarios.

## Security Considerations

### Threats Addressed

- **Untrusted cloud provider**: Cloud never receives plaintext or file metadata
- **Traffic analysis**: Fixed-size chunks prevent file size inference
- **EXIF metadata leakage**: GPS, camera model, timestamps stripped or encrypted
- **Temp file artifacts**: In-memory rendering prevents plaintext disk writes (Zero-Trace)
- **Chunk swap attacks**: AAD (file_id || chunk_index) binds each chunk to its file and position
- **AEAD tampering**: Authentication tag detects any modification to ciphertext

### Assumptions

- User's local machine is trusted and not compromised during a session
- Password has sufficient entropy (≥12 characters recommended)
- Rclone backend provides reliable storage (Arx Runa does not implement redundancy)

### Out of Scope

- Physical theft of device during an unlocked session
- Malware capturing keys or screen during session
- Cloud provider deleting or corrupting blobs
- Quantum computing attacks (symmetric XChaCha20-Poly1305 remains secure; see design doc)

## Notes

This is the canonical use case for Arx Runa. Tier 2 is the default for stronger security; users who prefer password-only may select Tier 1 during vault creation.

**Password loss warning**: For a Tier 1 vault, the password is the sole authentication factor. Forgetting it without a recovery phrase configured means permanent, unrecoverable data loss — there is no admin override or cloud-based reset. Tier 2 users who opted down from the default should also store their password securely. All users should either store their password in a password manager or configure the opt-in BIP-39 recovery phrase immediately after vault creation.

See [use-case-3](use-case-3-hardware-mfa-and-key-loss.md) for all credential-loss and recovery flows, including Tier 1 password loss (with and without a recovery phrase) and the full Tier 2 (key file) setup and key-loss scenarios.
