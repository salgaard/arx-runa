# UC-DEV-001: Cryptographic Secret Storage

**Category**: Developer & Technical

**Status**: Active

---

## Overview

A developer or DevOps engineer needs to store cryptographic secrets (API keys, private keys, database credentials, signing certificates) in cloud backup without exposing them to the cloud provider or risking compromise through password-only attacks.

## Actors

- **Primary Actor**: Developer, DevOps engineer, or security engineer
- **Secondary Actors**: Cloud storage provider (untrusted), VoidGate system, USB key file (hardware factor)

## Preconditions

- Developer has VoidGate installed on development machine
- Developer has created USB key file on dedicated USB drive
- Developer has configured Rclone backend (personal S3, corporate storage, etc.)
- Developer has secrets to protect (SSH keys, API tokens, certificates)

## Main Flow

1. Developer generates sensitive cryptographic material:
   - SSH private key for production servers
   - API token for cloud services (AWS Access Key, GitHub token)
   - TLS certificate private key
   - Database password for production environment
2. Developer launches VoidGate and unlocks vault with password + USB key
3. Developer selects "Upload Secrets" and chooses secret files
4. VoidGate encrypts each secret:
   - Uses XChaCha20-Poly1305 AEAD
   - Generates random file_key per secret
   - Wraps file_key with key_encryption_key (derived from master_key)
5. VoidGate uploads encrypted secrets to cloud (random UUID blob names)
6. VoidGate stores encrypted manifest with secret metadata (filename, tags, not content)
7. Developer locks vault and stores USB key in secure location (home safe, safety deposit box)
8. Later, developer needs to deploy application requiring API token:
9. Developer unlocks vault on deployment machine
10. Developer searches manifest for "AWS API Token"
11. VoidGate downloads encrypted chunks from cloud
12. VoidGate decrypts secret in memory (no plaintext written to disk)
13. Developer copies secret to clipboard or exports to environment variable
14. Developer uses secret for deployment (e.g., `export AWS_ACCESS_KEY_ID=...`)
15. Developer completes deployment
16. Developer locks vault (secret evicted from memory)

## Alternate Flows

### Secret Rotation

**Trigger**: Developer rotates API token (old token expires, new token generated)

**Steps**:
1. Developer unlocks vault
2. Developer selects old API token entry
3. Developer chooses "Update Secret"
4. Developer pastes new API token
5. VoidGate re-encrypts with new nonces
6. VoidGate uploads updated encrypted chunks
7. VoidGate marks old version as archived (optional versioning)
8. Developer locks vault
9. Old token no longer accessible (or accessible in version history)

### Emergency Access (USB Key Not Available)

**Trigger**: Developer needs secret urgently but USB key is at home

**Steps**:
1. Developer attempts to unlock vault without USB key
2. VoidGate displays: "Key file required — vault cannot be unlocked"
3. Developer cannot access secrets (by design — no password-only fallback)
4. Developer must retrieve USB key or use backup USB key (if available)
5. Flow terminates (hardware MFA enforced)

### Backup USB Key for Disaster Recovery

**Trigger**: Developer creates backup USB key in case primary is lost

**Steps**:
1. Developer unlocks vault with primary USB key
2. Developer inserts second USB drive
3. Developer selects "Export Key File"
4. VoidGate copies key file to second USB drive
5. Developer stores backup USB key in safety deposit box or with trusted family member
6. If primary USB key is lost: developer retrieves backup and continues using vault

### Exporting Secret to CI/CD Pipeline

**Trigger**: Developer needs to inject secret into CI/CD environment (GitHub Actions, GitLab CI)

**Steps**:
1. Developer unlocks vault
2. Developer downloads and decrypts secret
3. Developer adds secret to CI/CD secrets store (GitHub Secrets, GitLab Variables)
4. Developer does NOT commit secret to git (VoidGate is backup, not source of truth for CI/CD)
5. CI/CD pipeline uses secret from secrets store (not from VoidGate directly)

## Success Criteria

- Secrets are encrypted before upload (cloud never sees plaintext API keys, passwords)
- Secrets cannot be accessed with password alone (USB key required)
- Secrets are decrypted in memory only (no plaintext files on disk)
- Developer can rotate secrets and archive old versions
- Developer can search secrets by tags or filenames (metadata in encrypted manifest)
- Backup USB key enables disaster recovery

## Related Designs

- [Authentication & Session Management](../architecture/designs/authentication-and-session-management/design.md) — USB key file enforces hardware MFA (password alone insufficient for secrets access)
- [Cryptographic Primitives](../architecture/designs/cryptographic-primitives/design.md) — XChaCha20-Poly1305 encryption, per-file random file_keys, key wrapping
- [Chunking & Manifest](../architecture/designs/chunking-and-manifest/design.md) — Secrets stored as encrypted chunks (4 MiB max, most secrets <1 KiB)
- [Cloud Synchronisation](../architecture/designs/cloud-synchronisation/design.md) — Rclone BYOC (developer can use personal S3, Backblaze, etc.)

## Security Considerations

### Threats Addressed

- **Cloud provider compromise**: Cloud cannot access plaintext secrets even if breached
- **Password-only attacks**: USB key prevents password brute-force (requires physical access to USB)
- **Accidental git commit**: Secrets never in plaintext files (VoidGate manages encrypted copies only)
- **Laptop theft**: If laptop is stolen without USB key, secrets remain inaccessible
- **Secrets sprawl**: Centralized encrypted storage for all secrets (not scattered across .env files)

### Assumptions

- Developer physically secures USB key (home safe, worn on keychain, etc.)
- Developer does not export secrets to insecure locations (unencrypted USB, plaintext files)
- Developer's workstation is trusted during session (no malware capturing clipboard)
- Developer creates backup USB key and stores separately (disaster recovery)
- Secrets have sufficient entropy (long API tokens, strong passwords)

### Out of Scope

- **Secrets management in production**: VoidGate is for backup/personal use, not production secrets rotation (use HashiCorp Vault, AWS Secrets Manager, etc.)
- **Automated secrets injection**: VoidGate does not integrate with CI/CD directly (manual export required)
- **Secrets sharing**: Single-user vault (cannot share secrets with team in current design)
- **Key escrow**: No recovery if both USB key and backup are lost (intentional design choice)

## Notes

This use case addresses a common developer problem: **where to securely back up sensitive secrets**. Developers often resort to:
- Plaintext .env files (insecure)
- Password managers (convenient but password-only, cloud provider has keys)
- Encrypted ZIP files (manual, no versioning)
- Git-crypt (tied to git repo, not for arbitrary secrets)

VoidGate provides:
- Hardware MFA (USB key prevents password-only compromise)
- Zero-knowledge cloud backup (secrets never in plaintext at provider)
- Version history (optional rotation tracking)
- BYOC flexibility (use any Rclone backend)

**Comparison to Secrets Management Tools**:
- **1Password, LastPass, Bitwarden**: Password-only, provider has keys (convenient but less secure)
- **HashiCorp Vault**: Production secrets management (overkill for personal backup)
- **VoidGate**: Hardware MFA + zero-knowledge backup (balance of security and usability)

**Limitation**: VoidGate is not designed for team secrets sharing or automated CI/CD integration. It is a personal backup solution for developers who want hardware-enforced encryption.

---

**References**:
- OWASP Secrets Management Cheat Sheet
- NIST SP 800-57: Key Management Recommendations
- 12 Factor App: Store config in environment (not code)
