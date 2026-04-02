# UC-IND-001: Personal File Backup with Zero-Knowledge Encryption

**Category**: Individual Privacy

**Status**: Active

---

## Overview

An individual user wants to back up sensitive personal files (documents, tax records, medical information) to cloud storage without exposing plaintext data to the cloud provider.

## Actors

- **Primary Actor**: Individual user with sensitive personal files
- **Secondary Actors**: Cloud storage provider (untrusted), VoidGate system, USB key file (hardware factor)

## Preconditions

- User has installed VoidGate on their local machine
- User has created a USB key file (32 bytes random entropy) stored on a USB drive
- User has configured an Rclone backend (e.g., Google Drive, Dropbox, S3)
- User has set a password for vault encryption

## Main Flow

1. User launches VoidGate application
2. User inserts USB drive containing key file
3. User selects "Unlock Vault" and provides password
4. VoidGate derives encryption keys using Argon2id(password || key_file_bytes, salt)
5. User selects files to back up (e.g., tax documents folder)
6. VoidGate encrypts each file:
   - Generates random file_key for each file
   - Chunks file into 4 MiB fixed-size blocks with zero-padding
   - Encrypts each chunk with XChaCha20-Poly1305 (nonce + ciphertext + tag)
   - Computes BLAKE3 checksum over encrypted blob
7. VoidGate uploads encrypted chunks to cloud with random UUID blob names
8. VoidGate stores encrypted manifest (file metadata) in SQLCipher database
9. VoidGate pushes encrypted manifest and vault header to cloud
10. User receives confirmation that backup is complete
11. User locks vault and removes USB key

## Alternate Flows

### Missing USB Key File

**Trigger**: User attempts to unlock vault without USB drive inserted

**Steps**:
1. VoidGate displays error: "Key file not found"
2. User inserts USB drive and retries
3. Flow returns to Main Flow step 3

### Incorrect Password

**Trigger**: User provides wrong password

**Steps**:
1. VoidGate attempts key derivation with wrong password
2. Derived key fails to decrypt vault header
3. VoidGate displays generic "Authentication failed" (does not reveal which factor failed)
4. User retries with correct password
5. Flow returns to Main Flow step 3

### Cloud Provider Unavailable

**Trigger**: Rclone backend is unreachable (network issue, API down)

**Steps**:
1. VoidGate completes local encryption and manifest update
2. VoidGate queues upload for retry
3. VoidGate displays warning: "Cloud sync pending — files encrypted locally"
4. When connectivity restores, VoidGate automatically retries upload
5. Flow continues after successful upload

### File Already Exists

**Trigger**: User attempts to upload a file that already exists in vault

**Steps**:
1. VoidGate checks manifest for existing file
2. VoidGate prompts: "File exists. Overwrite or create new version?"
3. If overwrite: VoidGate deletes old chunks and proceeds with encryption
4. If new version: VoidGate uploads with version suffix in manifest
5. Flow continues from Main Flow step 6

## Success Criteria

- All files are encrypted locally before any data leaves the client
- Cloud provider receives only opaque blobs with random UUIDs (no filenames, sizes, or metadata)
- User can retrieve and decrypt files using password + USB key file
- Encrypted chunks are verified with BLAKE3 checksums before decryption
- Vault cannot be unlocked with password alone (USB key required)
- No plaintext data is written to disk during encryption process

## Related Designs

- [Authentication & Session Management](../architecture/designs/authentication-and-session-management/design.md) — USB key file + password dual-factor authentication, Argon2id key derivation, session key management
- [Cryptographic Primitives](../architecture/designs/cryptographic-primitives/design.md) — XChaCha20-Poly1305 AEAD encryption, BLAKE3 checksums, nonce generation
- [Chunking & Manifest](../architecture/designs/chunking-and-manifest/design.md) — 4 MiB fixed-size chunks with zero-padding, SQLCipher manifest storage, chunk indexing
- [Cloud Synchronisation](../architecture/designs/cloud-synchronisation/design.md) — Rclone integration, vault header format, encrypted manifest backup, push/pull flows

## Security Considerations

### Threats Addressed

- **Untrusted cloud provider**: Cloud never receives plaintext or file metadata
- **Password-only compromise**: USB key file prevents password-only attacks
- **Traffic analysis**: Fixed-size chunks prevent file size inference
- **Man-in-the-middle**: AEAD authentication tags detect tampering
- **Chunk swap attacks**: AAD (file_id || chunk_index) binds chunks to specific files and positions

### Assumptions

- User's local machine is trusted and not compromised
- USB key file is physically secured (not copied or exposed)
- Password has sufficient entropy (recommended ≥12 characters, mixed case, symbols)
- Rclone backend provides reliable storage (VoidGate does not implement redundancy)

### Out of Scope

- Physical theft of USB key + password extraction from user
- Malware on user's local machine capturing keys during session
- Quantum computing attacks on XChaCha20-Poly1305 (considered future work)
- Cloud provider deleting or corrupting blobs (user must verify backup integrity separately)

## Notes

This is the canonical use case for VoidGate — individual privacy against untrusted cloud providers. It demonstrates all core features: zero-knowledge encryption, hardware MFA, fixed-size chunking, and BYOC flexibility.

**Future Enhancements**:
- Multi-device sync (current design is single-device)
- Incremental backup with deduplication (currently full-file re-upload)
- Automated backup scheduling (currently manual)

---

**References**:
- NIST SP 800-63B: Digital Identity Guidelines (Authentication and Lifecycle Management)
- OWASP: Cryptographic Storage Cheat Sheet
