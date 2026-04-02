# UC-BIZ-002: Secure File Sharing Within Organization

**Category**: Business & Enterprise

**Status**: Active

---

## Overview

An organization needs to share sensitive files between employees or departments without exposing plaintext to external cloud providers, while maintaining access control and audit trails.

## Actors

- **Primary Actor**: Employee sharing files (sender)
- **Secondary Actors**: Employee receiving files (recipient), Cloud storage provider (untrusted), VoidGate system

## Preconditions

- Organization has deployed VoidGate with company vault
- Both sender and recipient have vault access (company password + USB key file access)
- Organization has configured shared Rclone backend
- VoidGate file sharing feature is enabled (per design document)

## Main Flow

1. Sender unlocks vault with company password + USB key
2. Sender encrypts confidential file (e.g., financial report) and uploads to vault
3. Sender selects "Share File" in VoidGate UI
4. Sender chooses recipient(s) from organization directory (if integrated) or enters email
5. VoidGate generates share_key for this specific file
6. VoidGate wraps share_key with recipient's public key (if PKI integrated) OR company-wide vault key
7. VoidGate adds share metadata to encrypted manifest:
   - Shared file_id
   - Share_key (wrapped)
   - Recipient identifier
   - Expiration timestamp (optional)
8. VoidGate pushes updated manifest to cloud
9. Recipient receives notification (email, in-app) that file is shared
10. Recipient unlocks vault with company password + USB key
11. Recipient pulls latest manifest from cloud
12. VoidGate displays shared file in "Shared with Me" section
13. Recipient unwraps share_key using company vault key
14. Recipient downloads encrypted chunks from cloud
15. Recipient decrypts chunks with share_key
16. Recipient views file (read-only or editable, depending on share permissions)
17. If editable: recipient makes changes and re-encrypts with same share_key
18. Recipient uploads updated chunks and pushes manifest
19. Sender sees updated version in manifest

## Alternate Flows

### Share Expiration

**Trigger**: Sender sets expiration time on shared file

**Steps**:
1. Sender configures "Expire after 7 days" when sharing
2. VoidGate stores expiration timestamp in manifest
3. After 7 days, recipient attempts to access file
4. VoidGate checks expiration timestamp
5. VoidGate displays: "Share expired — contact owner for renewed access"
6. Recipient cannot decrypt file (share_key entry removed from manifest)

### Revoke Access

**Trigger**: Sender wants to revoke recipient's access before expiration

**Steps**:
1. Sender selects shared file in VoidGate
2. Sender chooses "Revoke Access" for specific recipient
3. VoidGate removes share_key entry from manifest for that recipient
4. VoidGate pushes updated manifest
5. Recipient pulls manifest and sees share no longer listed
6. Recipient cannot decrypt file (share_key no longer accessible)

### Cross-Department Sharing

**Trigger**: Finance department shares file with Legal department

**Steps**:
1. Finance employee (sender) shares file with Legal employee (recipient)
2. Both use same company vault (shared backend)
3. Share_key is wrapped with company vault key (all authorized employees can unwrap)
4. Legal employee pulls manifest and decrypts file
5. No external sharing outside organization (all recipients must have vault access)

### Audit Trail for Shared Files

**Trigger**: Organization needs to track who accessed shared file for compliance

**Steps**:
1. Admin enables audit logging in VoidGate
2. Each share operation logs: sender, recipient, file_id, timestamp
3. Each access (download/decrypt) logs: recipient, file_id, timestamp
4. Audit log is encrypted and stored in manifest or separate secure database
5. Admin can export audit log for compliance review

### External Sharing (Out of Scope - Workaround)

**Trigger**: Organization needs to share file with external partner (no vault access)

**Steps**:
1. Current design: external sharing not supported (all recipients need vault access)
2. Workaround: Sender exports decrypted file and shares via traditional means (email, encrypted ZIP)
3. Future enhancement: Generate one-time share link with ephemeral key (not implemented)

## Success Criteria

- Files are shared without exposing plaintext to cloud provider
- Only authorized recipients (with vault access) can decrypt shared files
- Sender can revoke access or set expiration for shared files
- Cloud provider cannot identify who shared what with whom (metadata encrypted)
- Audit trail captures share operations for compliance
- Recipients can collaborate (view, edit) on shared files within vault

## Related Designs

- [File Sharing](../architecture/designs/file-sharing/design.md) — Share key wrapping, recipient management, expiration, revocation
- [Cryptographic Primitives](../architecture/designs/cryptographic-primitives/design.md) — Key wrapping (share_key), public-key encryption (future PKI integration)
- [Chunking & Manifest](../architecture/designs/chunking-and-manifest/design.md) — Share metadata in encrypted manifest
- [Cloud Synchronisation](../architecture/designs/cloud-synchronisation/design.md) — Manifest push/pull for share updates
- [Tauri IPC & Frontend](../architecture/designs/tauri-ipc-and-frontend/design.md) — Share UI, recipient selection, permission management

## Security Considerations

### Threats Addressed

- **Untrusted cloud provider**: Cloud cannot see who shared files or with whom
- **External eavesdropping**: Share metadata is encrypted in manifest
- **Unauthorized access**: Only users with vault access can decrypt shared files
- **Access persistence**: Revoked shares cannot be decrypted (share_key removed)

### Assumptions

- All recipients have legitimate vault access (company password + USB key)
- Organization trusts all vault users (no per-user isolation in current design)
- Recipients do not export and redistribute decrypted files insecurely
- Share_key revocation requires recipient to pull latest manifest (not real-time)

### Out of Scope

- **External sharing**: Recipients without vault access cannot receive shares
- **Real-time revocation**: Recipient who already downloaded manifest must pull updates to see revocation
- **Granular permissions**: No read-only vs. edit enforcement (recipient can always decrypt)
- **End-to-end per-user encryption**: Current design uses company-wide vault key (all authorized users can decrypt)

## Notes

This use case extends VoidGate from single-user to organizational use. Key challenges:
- **Access control granularity**: Current design relies on company-wide vault access (all-or-nothing). Future enhancement: per-user encryption keys with share_key wrapped per recipient.
- **External sharing gap**: Cannot share with partners outside organization. Future enhancement: one-time share links with ephemeral keys.

**Comparison to Enterprise File Sharing**:
- **Dropbox, Google Drive**: Provider sees who shared what with whom
- **SharePoint**: Metadata visible to Microsoft
- **VoidGate**: Share metadata encrypted — provider sees only opaque manifest updates

**Implementation Note**: File sharing design is specified but not yet implemented in VoidGate. This use case describes intended functionality based on the file-sharing design document.

---

**References**:
- NIST SP 800-57: Key Management Recommendations (Key Wrapping)
- Zero-Knowledge File Sharing: Tresorit, SpiderOak (comparison)
