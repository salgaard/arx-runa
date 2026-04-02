# UC-BIZ-001: Confidential Document Storage with BYOC

**Category**: Business & Enterprise

**Status**: Active

---

## Overview

A small/medium business needs to store confidential documents (contracts, financial records, trade secrets) in cloud storage but requires zero-knowledge encryption and the flexibility to choose their own cloud provider for compliance or cost reasons.

## Actors

- **Primary Actor**: Business administrator or IT manager
- **Secondary Actors**: Employees with vault access, Cloud storage provider (untrusted), VoidGate system, USB key file (company-controlled)

## Preconditions

- Organization has VoidGate deployed on workstations
- Organization has generated company USB key file stored in secure location (e.g., office safe)
- Organization has configured Rclone backend (e.g., company S3 bucket, Azure Blob, on-prem MinIO)
- Organization has defined access policy (who gets password, who can access USB key)

## Main Flow

1. IT manager creates company vault with password + USB key file
2. IT manager configures Rclone to point to company-controlled cloud storage
3. IT manager documents password and USB key location in secure company procedures
4. Employee needs to store confidential contract:
5. Employee launches VoidGate on work computer
6. Employee retrieves USB key from secure location (e.g., office safe)
7. Employee unlocks vault with company password + USB key
8. Employee uploads contract PDF to vault
9. VoidGate encrypts contract with XChaCha20-Poly1305
10. VoidGate uploads encrypted chunks to company S3 bucket (or configured backend)
11. VoidGate stores encrypted manifest in local SQLCipher database
12. VoidGate pushes encrypted manifest to cloud
13. Employee locks vault and returns USB key to secure location
14. Later, different employee needs to access contract:
15. Employee retrieves USB key and unlocks vault with password
16. Employee browses manifest for contract
17. Employee downloads and decrypts contract
18. Employee reviews contract (read-only or edits locally)
19. Employee re-encrypts and pushes updates if modified
20. Employee locks vault and returns USB key

## Alternate Flows

### Cloud Provider Migration

**Trigger**: Organization switches from AWS S3 to Azure Blob or on-prem storage

**Steps**:
1. IT manager configures new Rclone backend in VoidGate
2. IT manager initiates "Migrate Vault" operation
3. VoidGate downloads all encrypted chunks from old backend
4. VoidGate uploads encrypted chunks to new backend (UUIDs unchanged)
5. VoidGate pushes encrypted manifest to new backend
6. IT manager verifies integrity (checksum all blobs)
7. IT manager decommissions old backend
8. No re-encryption required (data remains opaque to both providers)

### Compliance Audit

**Trigger**: Auditor requires proof of encryption and access control

**Steps**:
1. Auditor requests evidence that documents are encrypted at rest
2. IT manager shows auditor encrypted blobs in cloud (random UUIDs, no plaintext)
3. IT manager demonstrates vault unlock requires USB key + password (dual-factor)
4. IT manager provides audit log from manifest (if enabled)
5. Auditor verifies cloud provider cannot access plaintext
6. Compliance requirement satisfied (zero-knowledge architecture)

### Employee Termination

**Trigger**: Employee leaves company and must be revoked from vault access

**Steps**:
1. IT manager changes company password
2. IT manager optionally rotates USB key file (creates new vault, re-encrypts data)
3. Former employee's password no longer works
4. If USB key was not rotated: former employee cannot access vault (no key access)
5. For maximum security: IT manager creates new vault and migrates data

**Note**: Current design does not support granular per-user access control. Future enhancement: multi-user vaults with per-user keys.

### Disaster Recovery

**Trigger**: Office fire destroys USB key and local workstations

**Steps**:
1. IT manager retrieves backup USB key from off-site location (safety deposit box)
2. IT manager reinstalls VoidGate on new workstations
3. IT manager configures same Rclone backend
4. IT manager pulls encrypted vault from cloud
5. IT manager unlocks vault with backup USB key + documented password
6. All encrypted documents recovered (cloud sync preserved data)

## Success Criteria

- Confidential documents are encrypted before leaving company network
- Cloud provider (AWS, Azure, etc.) cannot access plaintext documents
- Organization controls cloud backend choice (BYOC flexibility)
- Vault access requires dual-factor authentication (USB key + password)
- Organization can migrate cloud providers without re-encryption
- Compliance audits can verify zero-knowledge architecture
- Disaster recovery is possible with backup USB key

## Related Designs

- [Authentication & Session Management](../architecture/designs/authentication-and-session-management/design.md) — USB key file access control, company password policy, session timeout
- [Cryptographic Primitives](../architecture/designs/cryptographic-primitives/design.md) — XChaCha20-Poly1305 encryption, BLAKE3 integrity verification
- [Chunking & Manifest](../architecture/designs/chunking-and-manifest/design.md) — Encrypted manifest for document metadata, fixed-size chunks
- [Cloud Synchronisation](../architecture/designs/cloud-synchronisation/design.md) — Rclone BYOC integration, cloud provider flexibility, push/pull flows

## Security Considerations

### Threats Addressed

- **Untrusted cloud provider**: Cloud cannot access plaintext or document metadata
- **Cloud provider breach**: Even if provider is compromised, data remains encrypted
- **Insider threats at cloud provider**: Provider employees cannot access plaintext
- **Compliance requirements**: GDPR, CCPA, HIPAA, SOC 2 (data never in plaintext at rest in cloud)
- **Vendor lock-in**: Organization can switch providers without exposing data

### Assumptions

- Organization secures USB key in physical safe or access-controlled location
- Employees follow password policy (do not share company password insecurely)
- Company workstations are trusted (no malware capturing keys during session)
- Organization implements backup strategy for USB key (off-site copy)
- Rclone backend is configured with proper access controls (IAM policies, etc.)

### Out of Scope

- Per-user access control (current design: single vault password shared by authorized users)
- Audit logging of individual user actions (manifest tracks changes but not user identity)
- Automated key rotation (IT manager must manually rotate USB key + re-encrypt)
- Cross-organization file sharing (single-organization vault only)

## Notes

This use case highlights VoidGate's BYOC (Bring Your Own Cloud) design — critical for organizations with:
- **Compliance requirements**: Industry regulations mandate data residency or specific providers
- **Cost optimization**: Switch to cheaper provider without re-engineering
- **Hybrid/on-prem**: Use MinIO, Ceph, or on-prem S3-compatible storage

**Enterprise Gap**: Current single-user vault design is a limitation for organizations. Future enhancement: multi-user vaults with per-user encryption keys and role-based access control.

**Comparison to Traditional Enterprise Storage**:
- **SharePoint, Google Drive, Dropbox**: Provider has plaintext access
- **Box with Customer Managed Keys (CMK)**: Provider still sees metadata
- **VoidGate**: Zero-knowledge — provider sees only opaque blobs

---

**References**:
- GDPR Article 32: Security of Processing
- CCPA: California Consumer Privacy Act
- SOC 2 Type II: Trust Services Criteria
- NIST SP 800-53: Security Controls for Cloud
