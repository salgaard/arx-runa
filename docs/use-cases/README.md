# Use Cases

This directory documents the real-world scenarios that VoidGate is designed to address. Use cases bridge the gap between user needs and technical implementation, providing requirements traceability from problem statements to design decisions.

## Purpose

Use cases serve multiple functions:

1. **Requirements Validation**: Ensure technical designs actually solve real-world problems
2. **Design Traceability**: Link user needs to specific design documents and architectural decisions
3. **Coverage Verification**: Identify gaps where use cases lack design support
4. **Academic Documentation**: Demonstrate requirements engineering for the bachelor's project

## Structure

Each use case follows a standardised format (see `_template.md`):

- **Actors**: Who is involved (users, systems, external entities)
- **Preconditions**: What must be true before the use case executes
- **Main Flow**: Step-by-step primary success scenario
- **Alternate Flows**: Variations and exception paths
- **Success Criteria**: Measurable outcomes that define success
- **Related Designs**: Links to design documents that address this use case
- **Security Considerations**: Threats addressed, assumptions, and out-of-scope items
- **Notes**: Brief context (≤3 sentences)

## Progressive Security Model

VoidGate supports two authentication tiers, selectable per vault:

| Tier | Auth Factors | Use Case |
|------|-------------|----------|
| **Tier 1** | Password only | Chosen at vault creation; accessible from any device with the password |
| **Tier 2** | Password + USB key file | Chosen at vault creation; hardware factor required on every access |

All tiers are zero-knowledge — the cloud provider never holds keys or plaintext regardless of tier. The tier applies to the entire vault and is chosen at creation time. Users who need different security levels create separate vaults.

## Use Cases

| ID | Title | Sub-question |
|----|-------|-------------|
| [UC-IND-001](UC-IND-001-personal-file-backup.md) | Zero-Knowledge Personal Backup | SQ1 (crypto), SQ3 (chunking), SQ4 (Zero-Trace) |
| [UC-IND-002](UC-IND-002-cross-device-access.md) | Cross-Device Synchronisation | SQ3 (sync) |
| [UC-IND-003](UC-IND-003-hardware-mfa-and-key-loss.md) | Hardware MFA and Key Loss | SQ2 (USB hardware factor) |
| [UC-IND-004](UC-IND-004-personal-file-sharing.md) | Personal File Sharing | SQ5 (file sharing) |

## Sub-Question Traceability

Mapping to the five problem-formulation sub-questions:

| Sub-question | Description | Use case coverage |
|---|---|---|
| SQ1 | Encryption standards and key management | UC-IND-001 |
| SQ2 | USB hardware factor in authentication | UC-IND-003 |
| SQ3 | Chunking and sync without metadata leakage | UC-IND-001, UC-IND-002 |
| SQ4 | RAM-based UI / Zero-Trace | UC-IND-001 |
| SQ5 | File sharing in a zero-trust system | UC-IND-004 |

## Design Document Coverage

All use cases reference at least one canonical design document:

- [Authentication & Session Management](../architecture/designs/authentication-and-session-management/design.md)
- [Cryptographic Primitives](../architecture/designs/cryptographic-primitives/design.md)
- [Chunking & Manifest](../architecture/designs/chunking-and-manifest/design.md)
- [Cloud Synchronisation](../architecture/designs/cloud-synchronisation/design.md)
- [File Sharing](../architecture/designs/file-sharing/design.md)
- [Tauri IPC & Frontend](../architecture/designs/tauri-ipc-and-frontend/design.md)

Run `/use-case-coverage` after any use case or design changes to verify traceability.

## Adding New Use Cases

1. Copy `_template.md` to `UC-[CATEGORY]-NNN-kebab-case-title.md`
2. Choose category prefix: `IND` or `BIZ`
3. Fill in all sections; keep the main flow to ≤15 steps and alternate flows to ≤4
4. Add entry to the appropriate table above
5. Run `/use-case-coverage` to verify design references
6. Update `docs/SUMMARY.md`
