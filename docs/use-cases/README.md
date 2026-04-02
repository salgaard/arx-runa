# Use Cases

This directory documents the real-world scenarios that VoidGate is designed to address. Use cases bridge the gap between user needs and technical implementation, providing requirements traceability from problem statements to design decisions.

## Purpose

Use cases serve multiple functions:

1. **Requirements Validation**: Ensure technical designs actually solve real-world problems
2. **Design Traceability**: Link user needs to specific design documents and architectural decisions
3. **Coverage Verification**: Identify gaps where use cases lack design support
4. **Academic Documentation**: Demonstrate requirements engineering for the bachelor's project

## Structure

Each use case follows a standardized format (see `_template.md`):

- **UC-ID**: Unique identifier with category prefix
- **Actors**: Who is involved (users, systems, external entities)
- **Preconditions**: What must be true before the use case executes
- **Main Flow**: Step-by-step primary success scenario
- **Alternate Flows**: Variations and exception paths
- **Success Criteria**: Measurable outcomes that define success
- **Related Designs**: Links to design documents that address this use case
- **Security Considerations**: Threats addressed, assumptions, and out-of-scope items

## Categories

### Individual Privacy (UC-IND)

Use cases for individuals who don't trust cloud providers with their sensitive data.

| ID | Title | Key Features |
| ---- | ------- | -------------- |
| [UC-IND-001](UC-IND-001-personal-file-backup.md) | Personal File Backup with Zero-Knowledge Encryption | XChaCha20-Poly1305 encryption, fixed-size chunks, hardware MFA |
| [UC-IND-002](UC-IND-002-cross-device-access.md) | Cross-Device Secure File Access | Multi-device sync, conflict detection, portable USB key |
| [UC-IND-003](UC-IND-003-photo-storage.md) | Privacy-Focused Photo Storage | EXIF metadata protection, size padding, in-memory viewing |
| [UC-IND-004](UC-IND-004-hardware-mfa-protection.md) | Hardware MFA for Personal Data Protection | USB key file, no password-only fallback, offline authentication |

### Business & Enterprise (UC-BIZ)

Use cases for organizations requiring compliance, multi-user access, or BYOC flexibility.

| ID | Title | Key Features |
| ---- | ------- | -------------- |
| [UC-BIZ-001](UC-BIZ-001-confidential-byoc.md) | Confidential Document Storage with BYOC | Bring Your Own Cloud, compliance-ready, cloud migration |
| [UC-BIZ-002](UC-BIZ-002-secure-sharing.md) | Secure File Sharing Within Organization | Share key wrapping, revocation, expiration, audit trails |
| [UC-BIZ-003](UC-BIZ-003-regulated-storage.md) | Regulated Industry Cloud Storage | HIPAA/GDPR/FINRA compliance, breach notification exemption |
| [UC-BIZ-004](UC-BIZ-004-zero-trust.md) | Zero-Trust Architecture Compliance | Cryptographic trust boundaries, continuous verification |

### Developer & Technical (UC-DEV)

Use cases for technical users who want full control over cryptographic implementation.

| ID | Title | Key Features |
| ---- | ------- | -------------- |
| [UC-DEV-001](UC-DEV-001-secret-storage.md) | Cryptographic Secret Storage | API keys/certificates backup, in-memory decryption, rotation |
| [UC-DEV-002](UC-DEV-002-dev-backup.md) | Development Artifact Backup | Docker images, binaries, source archives, integrity checksums |
| [UC-DEV-003](UC-DEV-003-custom-backend.md) | Custom Cloud Backend Integration | Rclone 70+ backends, self-hosted, decentralized, cost optimization |

## Verification

Use cases are validated against design documents using the `use-case-coverage` skill:

```bash
# Check which use cases have design coverage
/use-case-coverage
```

The skill parses the "Related Designs" section of each use case and reports:
- ✅ Use cases with complete design coverage
- ⚠️ Use cases with partial coverage (some designs missing)
- ❌ Use cases with no design references (gaps requiring attention)

**Latest Coverage Report** (as of documentation creation):
- **11/11 use cases** have complete design coverage (100%)
- **42 total design references**, all valid
- Most referenced designs: `cryptographic-primitives` and `cloud-synchronisation` (used in all 11 use cases)

Run the skill after any use case or design changes to validate traceability.

## How to Read Use Cases

1. **Start with the Overview**: Understand the problem being solved
2. **Check Actors and Preconditions**: Identify who's involved and what's required
3. **Follow the Main Flow**: Walk through the primary success scenario step-by-step
4. **Review Alternate Flows**: Consider variations and edge cases
5. **Verify Success Criteria**: Understand what defines a successful outcome
6. **Trace to Designs**: Follow links to see how VoidGate's architecture addresses this use case
7. **Assess Security**: Review threats addressed and security assumptions

## Adding New Use Cases

1. Copy `_template.md` to a new file: `UC-[CATEGORY]-NNN-kebab-case-title.md`
2. Choose appropriate category prefix: `IND`, `BIZ`, or `DEV`
3. Fill in all template sections with specific details
4. Link to relevant design documents in "Related Designs"
5. Add entry to the appropriate category list in this README
6. Run `use-case-coverage` skill to verify design traceability
7. Update `docs/SUMMARY.md` to include the new use case in the navigation

## Design Document References

Use cases reference the following design documents:

- [Authentication & Session Management](../architecture/designs/authentication-and-session-management/design.md)
- [Cryptographic Primitives](../architecture/designs/cryptographic-primitives/design.md)
- [Chunking & Manifest](../architecture/designs/chunking-and-manifest/design.md)
- [Cloud Synchronisation](../architecture/designs/cloud-synchronisation/design.md)
- [File Sharing](../architecture/designs/file-sharing/design.md)
- [Tauri IPC & Frontend](../architecture/designs/tauri-ipc-and-frontend/design.md)

---

**Maintenance**: Review use cases when designs change significantly or when new features are added. The `use-case-coverage` skill should be run after any updates to ensure traceability remains intact.
