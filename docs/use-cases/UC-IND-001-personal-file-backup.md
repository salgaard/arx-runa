# UC-IND-001: Zero-Knowledge Personal Backup

**Category**: Individual Privacy

**Status**: Active

---

## Overview

An individual user wants to back up sensitive personal files (documents, photos, videos) to cloud storage without exposing plaintext, filenames, or metadata to the cloud provider. VoidGate uses a drop zone as the primary interface and defaults to Tier 1 authentication (password-only); high-value folders can be upgraded to Tier 2 (password + USB key file).

## Actors

- **Primary Actor**: Individual user with sensitive personal files
- **Secondary Actors**: Cloud storage provider (untrusted), VoidGate system

## Preconditions

- User has installed VoidGate on their local machine
- User has configured an Rclone backend (e.g., Google Drive, Dropbox, S3)
- User has set a vault password

## Main Flow

1. User launches VoidGate and enters password (Tier 1)
2. VoidGate derives master_key via Argon2id(password, salt)
3. VoidGate unlocks vault and displays drop zone UI with vault file browser
4. User drags files or folders onto the drop zone
5. VoidGate reads each file into RAM, generates a random file_key per file
6. VoidGate splits the file into 4 MiB fixed-size chunks, zero-padding the final chunk
7. VoidGate encrypts each chunk with XChaCha20-Poly1305 (AAD: file_id || chunk_index)
8. VoidGate uploads encrypted chunks to cloud with random UUID blob names
9. VoidGate stores encrypted manifest (filenames, chunk map) in SQLCipher database
10. Drop zone shows sync progress and confirms completion
11. User browses vault and retrieves files via in-app viewer or file browser
12. User locks vault

## Alternate Flows

### Upgrade Folder to Tier 2

**Trigger**: User wants a folder to require USB key as a second factor

**Steps**:
1. User right-clicks folder in vault browser → "Require USB Key (Tier 2)"
2. User inserts USB drive; VoidGate reads 32-byte key_file_bytes
3. VoidGate re-derives folder keys combining password + key_file_bytes
4. VoidGate re-wraps affected file_keys under the new key material
5. Folder is now Tier 2 — future access requires both password and USB key

### Media Files (EXIF and In-Memory Viewing)

**Trigger**: User drops photos or videos onto the drop zone

**Steps**:
1. VoidGate detects media file types
2. VoidGate optionally strips EXIF metadata (GPS, camera model, timestamps) before encryption
3. VoidGate encrypts and uploads as in Main Flow
4. When user opens a photo: VoidGate decrypts chunks into RAM and renders in-app (no temp file written to disk)
5. For large videos: VoidGate decrypts and streams progressively from cloud chunks

### Cloud Provider Unavailable

**Trigger**: Rclone backend is unreachable

**Steps**:
1. VoidGate completes local encryption and manifest update
2. VoidGate queues upload for retry and displays "Sync pending"
3. When connectivity restores, VoidGate automatically retries upload

### File Already Exists

**Trigger**: User drops a file that is already in the vault

**Steps**:
1. VoidGate checks manifest for existing file by name
2. VoidGate prompts: "File exists — overwrite or keep both?"
3. On overwrite: old chunks are deleted, new chunks encrypted and uploaded
4. On keep both: new version added with timestamp suffix in manifest

## Success Criteria

- All files are encrypted in RAM before any data leaves the client
- Cloud provider receives only opaque blobs with random UUID names (no filenames, sizes, or metadata)
- Fixed 4 MiB chunks hide exact file size from cloud provider
- EXIF metadata is stripped or encrypted before upload (media files)
- Decrypted content is displayed in-memory — no plaintext written to disk (Zero-Trace)
- Drop zone is the primary upload interface; user never selects files through a system file picker by default
- Tier 1 folders require password only; Tier 2 folders additionally require USB key file
- Vault cannot be opened without the correct password

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
- Quantum computing attacks (symmetric AES/ChaCha20 remains secure; see design doc)

## Notes

This is the canonical use case for VoidGate. Tier 1 is the default for accessibility — see UC-IND-003 for the full Tier 2 (USB key) setup and key-loss scenarios.
