# UC-IND-001: Zero-Knowledge Personal Backup

**Category**: Individual Privacy

**Status**: Active

---

## Overview

An individual user wants to back up sensitive personal files (documents, photos, videos) to cloud storage without exposing plaintext, filenames, or metadata to the cloud provider. VoidGate uses a drop zone as the primary interface. When creating a vault the user chooses an authentication tier: Tier 1 (password-only) or Tier 2 (password + USB key file). The tier applies to the entire vault — users who need different security levels create separate vaults.

## Actors

- **Primary Actor**: Individual user with sensitive personal files
- **Secondary Actors**: Cloud storage provider (untrusted), VoidGate system

## Preconditions

- User has installed VoidGate on their local machine
- User has configured an Rclone backend (e.g., Google Drive, Dropbox, S3)

## Main Flow

1. User launches VoidGate and selects "Create Vault"
2. VoidGate prompts: "Choose authentication tier — Tier 1 (password only) or Tier 2 (password + USB key)"
3. User selects a tier and completes setup (password for Tier 1; password + USB key generation for Tier 2)
4. VoidGate derives encryption keys from the provided credentials
5. VoidGate unlocks vault and displays drop zone UI with vault file browser
6. User drags files or folders onto the drop zone
7. VoidGate generates a unique encryption key for each file
8. VoidGate splits and encrypts the file into fixed-size chunks
9. VoidGate records the file in the encrypted vault manifest
10. VoidGate uploads encrypted chunks to cloud
11. Drop zone shows sync progress and confirms completion
12. User browses vault and views files in-app (Zero-Trace)
13. User locks vault (and removes USB key if Tier 2)

## Alternate Flows

### Media Files (EXIF and In-Memory Viewing)

**Trigger**: User drops photos or videos onto the drop zone

**Steps**:
1. VoidGate detects media file types
2. VoidGate optionally strips EXIF metadata (GPS, camera model, timestamps) before encryption
3. VoidGate encrypts and uploads as in Main Flow
4. When user opens a photo: VoidGate decrypts chunks into RAM and renders in-app (no temp file written to disk)
5. For large videos: VoidGate decrypts and streams progressively from cloud chunks

### Export Decrypted File to Disk

**Trigger**: User wants to save a decrypted copy of a file outside VoidGate (e.g., to edit in an external application)

**Steps**:
1. User selects a file in the vault browser and chooses "Export"
2. VoidGate downloads encrypted chunks and decrypts in RAM
3. VoidGate prompts user to choose a save location
4. VoidGate writes the plaintext file to the chosen location
5. VoidGate warns: "Exported file is unencrypted and outside vault protection"
6. User is responsible for the exported copy

### Cloud Provider Unavailable

**Trigger**: Rclone backend is unreachable

**Steps**:
1. VoidGate completes local encryption and manifest update
2. VoidGate queues upload for retry and displays "Sync pending"
3. When connectivity restores, user triggers sync and VoidGate uploads pending chunks

### Cloud Provider Migration

**Trigger**: User wants to switch to a different cloud provider (e.g., from Google Drive to Backblaze B2)

**Steps**:
1. User configures a new Rclone backend in VoidGate settings
2. User initiates "Migrate Vault" — VoidGate downloads all encrypted blobs from the old backend
3. VoidGate re-uploads the same blobs to the new backend (UUID names and content unchanged)
4. VoidGate pushes vault header and manifest backup to new backend
5. User verifies migration and decommissions old backend
6. No re-encryption required — data remains opaque to both providers

### File Already Exists

**Trigger**: User drops a file that is already in the vault

**Steps**:
1. VoidGate checks manifest for existing file by name
2. VoidGate prompts: "File exists — overwrite or keep both?"
3. On overwrite: old chunks are deleted, new chunks encrypted and uploaded
4. On keep both: VoidGate saves the new file alongside the original with a disambiguated name

## Success Criteria

- All files are encrypted in RAM before any data leaves the client
- Cloud provider receives only opaque blobs with random UUID names (no filenames, sizes, or metadata)
- Fixed 4 MiB chunks hide exact file size from cloud provider
- EXIF metadata is stripped or encrypted before upload (media files)
- Decrypted content is displayed in-memory — no plaintext written to disk (Zero-Trace)
- Drop zone is the primary upload interface; user never selects files through a system file picker by default
- User selects authentication tier (Tier 1 or Tier 2) when creating the vault
- Tier 1 vault requires password only; Tier 2 vault additionally requires USB key file
- Vault cannot be opened without the correct authentication factors for the chosen tier

## Related Designs

- [Authentication & Session Management](../architecture/designs/authentication-and-session-management/design.md)
- [Cryptographic Primitives](../architecture/designs/cryptographic-primitives/design.md)
- [Chunking & Manifest](../architecture/designs/chunking-and-manifest/design.md)
- [Cloud Synchronisation](../architecture/designs/cloud-synchronisation/design.md)
- [Tauri IPC & Frontend](../architecture/designs/tauri-ipc-and-frontend/design.md)

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
- Rclone backend provides reliable storage (VoidGate does not implement redundancy)

### Out of Scope

- Physical theft of device during an unlocked session
- Malware capturing keys or screen during session
- Cloud provider deleting or corrupting blobs
- Quantum computing attacks (symmetric XChaCha20-Poly1305 remains secure; see design doc)

## Notes

This is the canonical use case for VoidGate. Tier 1 is the default for accessibility — see UC-IND-003 for the full Tier 2 (USB key) setup and key-loss scenarios.
