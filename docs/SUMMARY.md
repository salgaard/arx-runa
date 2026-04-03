# Summary

[Introduction](README.md)

---

# Use Cases

- [Overview](use-cases/README.md)
  - [UC-IND-001: Personal File Backup](use-cases/UC-IND-001-personal-file-backup.md)
  - [UC-IND-002: Cross-Device Access](use-cases/UC-IND-002-cross-device-access.md)
  - [UC-IND-003: Privacy-Focused Photo Storage](use-cases/UC-IND-003-photo-storage.md)
  - [UC-IND-004: Hardware MFA Protection](use-cases/UC-IND-004-hardware-mfa-protection.md)
  - [UC-IND-005: Key Loss and Data Recovery](use-cases/UC-IND-005-key-loss-and-data-recovery.md)
  - [UC-BIZ-001: Confidential BYOC Storage](use-cases/UC-BIZ-001-confidential-byoc.md)
  - [UC-BIZ-002: Secure File Sharing](use-cases/UC-BIZ-002-secure-sharing.md)
  - [UC-BIZ-003: Regulated Industry Storage](use-cases/UC-BIZ-003-regulated-storage.md)
  - [UC-BIZ-004: Zero-Trust Compliance](use-cases/UC-BIZ-004-zero-trust.md)
  - [UC-DEV-001: Cryptographic Secret Storage](use-cases/UC-DEV-001-secret-storage.md)
  - [UC-DEV-002: Development Artifact Backup](use-cases/UC-DEV-002-dev-backup.md)
  - [UC-DEV-003: Custom Backend Integration](use-cases/UC-DEV-003-custom-backend.md)

---

# Architecture

- [Overview](architecture/README.md)
- [Designs](architecture/designs/README.md)
  - [Authentication & Session Management](architecture/designs/authentication-and-session-management/design.md)
    - [Authentication Flow](architecture/designs/authentication-and-session-management/diagrams/authentication-flow.md)
  - [Chunking & Manifest](architecture/designs/chunking-and-manifest/design.md)
    - [Chunk Pipeline](architecture/designs/chunking-and-manifest/diagrams/chunk-pipeline.md)
  - [Cloud Synchronisation](architecture/designs/cloud-synchronisation/design.md)
    - [Cloud Sync Sequence](architecture/designs/cloud-synchronisation/diagrams/cloud-sync-sequence.md)
    - [Sub-Phase Roadmap](architecture/designs/cloud-synchronisation/sub-phases/roadmap.md)
      - [4.1: Cloud Transport](architecture/designs/cloud-synchronisation/sub-phases/4.1-cloud-transport.md)
      - [4.2: Rclone Integration](architecture/designs/cloud-synchronisation/sub-phases/4.2-rclone-integration.md)
      - [4.3: Vault Header](architecture/designs/cloud-synchronisation/sub-phases/4.3-vault-header.md)
      - [4.4: Manifest Backup](architecture/designs/cloud-synchronisation/sub-phases/4.4-manifest-backup.md)
      - [4.5: Push/Pull Flows](architecture/designs/cloud-synchronisation/sub-phases/4.5-push-pull-flows.md)
  - [Cryptographic Primitives](architecture/designs/cryptographic-primitives/design.md)
    - [Key Derivation Tree](architecture/designs/cryptographic-primitives/diagrams/key-derivation-tree.md)
  - [File Sharing](architecture/designs/file-sharing/design.md)
    - [File Sharing Flow](architecture/designs/file-sharing/diagrams/file-sharing-flow.md)
  - [Tauri IPC & Frontend](architecture/designs/tauri-ipc-and-frontend/design.md)
- [Diagrams](architecture/diagrams/INDEX.md)
  - [End-to-End Encryption Flow](architecture/diagrams/end-to-end-encryption-flow.md)
  - [SSOT Information Flow](architecture/diagrams/ssot-information-flow.md)

---

# Decisions

- [Architecture Decisions](architecture-decisions/README.md)
  - [001: Code Structure and Patterns](architecture-decisions/001-code-structure-and-patterns.md)
  - [002: Frontend Stack Selection](architecture-decisions/002-frontend-stack-selection.md)
  - [003: Sub-Phase Roadmap Workflow](architecture-decisions/003-sub-phase-roadmap-workflow.md)

---

# Guides

- [Development Setup](guides/development.md)
- [Documentation SSOT](guides/documentation-ssot.md)

---

# Research

- [Market & Future Directions](research/market-and-future-directions.md)

---

# Reference

- [Project Roadmap](roadmap.md)
