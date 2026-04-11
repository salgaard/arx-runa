# Summary

[Introduction](README.md)

---

# Use Cases

- [Overview](use-cases/README.md)
  - [Use Case 1: Zero-Knowledge Personal Backup](use-cases/use-case-1-personal-file-backup.md)
  - [Use Case 2: Cross-Device Synchronisation](use-cases/use-case-2-cross-device-access.md)
  - [Use Case 3: Hardware MFA and Key Loss](use-cases/use-case-3-hardware-mfa-and-key-loss.md)
  - [Use Case 4: Personal File Sharing](use-cases/use-case-4-personal-file-sharing.md)

---

# Architecture

- [Overview](architecture/README.md)
- [Global Design Invariants](architecture/design-invariants.md)
- [Project Roadmap](roadmap.md)
- [Designs](architecture/designs/README.md)
  - [Project Scaffolding](architecture/designs/project-scaffolding/design.md)
    - [Diagram: Build Pipeline](architecture/designs/project-scaffolding/diagrams/build-pipeline.md)
    - [Sub-Phase Roadmap](architecture/designs/project-scaffolding/sub-phases/roadmap.md)
      - [0.1: Tauri Workspace Init](architecture/designs/project-scaffolding/sub-phases/0.1-tauri-workspace-init.md)
      - [0.2: Dependencies and Modules](architecture/designs/project-scaffolding/sub-phases/0.2-dependencies-and-modules.md)
      - [0.3: Frontend Build Pipeline](architecture/designs/project-scaffolding/sub-phases/0.3-frontend-build-pipeline.md)
  - [Cryptographic Primitives](architecture/designs/cryptographic-primitives/design.md)
    - [Diagram: Key Derivation Tree](architecture/designs/cryptographic-primitives/diagrams/key-derivation-tree.md)
    - [Diagram: Key Derivation — Recovery Slot](architecture/designs/cryptographic-primitives/diagrams/key-derivation-recovery-slot.md)
    - [Diagram: Key Derivation Flow](architecture/designs/cryptographic-primitives/diagrams/key-derivation-flow.md)
    - [Diagram: Chunk Encryption Flow](architecture/designs/cryptographic-primitives/diagrams/chunk-encryption-flow.md)
    - [Sub-Phase Roadmap](architecture/designs/cryptographic-primitives/sub-phases/roadmap.md)
      - [1.1: Key Types and Derivation](architecture/designs/cryptographic-primitives/sub-phases/1.1-key-types-and-derivation.md)
      - [1.2: AEAD Encrypt/Decrypt](architecture/designs/cryptographic-primitives/sub-phases/1.2-aead-encrypt-decrypt.md)
      - [1.3: Key Wrapping and Checksums](architecture/designs/cryptographic-primitives/sub-phases/1.3-key-wrapping-and-checksums.md)
  - [Authentication & Session Management](architecture/designs/authentication-and-session-management/design.md)
    - [Diagram: Authentication Flow](architecture/designs/authentication-and-session-management/diagrams/authentication-flow.md)
    - [Sub-Phase Roadmap](architecture/designs/authentication-and-session-management/sub-phases/roadmap.md)
      - [2.1: USB Key File and Device Monitor](architecture/designs/authentication-and-session-management/sub-phases/2.1-usb-key-file-and-device-monitor.md)
      - [2.2: Argon2id and Session Keys](architecture/designs/authentication-and-session-management/sub-phases/2.2-argon2id-and-session-keys.md)
      - [2.3: Session Lifecycle and Timeout](architecture/designs/authentication-and-session-management/sub-phases/2.3-session-lifecycle-and-timeout.md)
      - [2.4: Vault Ceremonies](architecture/designs/authentication-and-session-management/sub-phases/2.4-vault-ceremonies.md)
  - [Chunking & Manifest](architecture/designs/chunking-and-manifest/design.md)
    - [Diagram: Chunk Pipeline](architecture/designs/chunking-and-manifest/diagrams/chunk-pipeline.md)
    - [Diagram: Manifest Schema](architecture/designs/chunking-and-manifest/diagrams/manifest-schema.md)
    - [Sub-Phase Roadmap](architecture/designs/chunking-and-manifest/sub-phases/roadmap.md)
      - [3.1: Schema and Metadata Store](architecture/designs/chunking-and-manifest/sub-phases/3.1-schema-and-metadata-store.md)
      - [3.2: Encrypt/Decrypt Pipelines](architecture/designs/chunking-and-manifest/sub-phases/3.2-encrypt-decrypt-pipelines.md)
      - [3.3: Staging and Error Recovery](architecture/designs/chunking-and-manifest/sub-phases/3.3-staging-and-error-recovery.md)
  - [Cloud Synchronisation](architecture/designs/cloud-synchronisation/design.md)
    - [Diagram: Cloud Sync Sequence](architecture/designs/cloud-synchronisation/diagrams/cloud-sync-sequence.md)
    - [Sub-Phase Roadmap](architecture/designs/cloud-synchronisation/sub-phases/roadmap.md)
      - [4.1: Cloud Transport](architecture/designs/cloud-synchronisation/sub-phases/4.1-cloud-transport.md)
      - [4.2: Rclone Integration](architecture/designs/cloud-synchronisation/sub-phases/4.2-rclone-integration.md)
      - [4.3: Vault Header](architecture/designs/cloud-synchronisation/sub-phases/4.3-vault-header.md)
      - [4.4: Manifest Backup](architecture/designs/cloud-synchronisation/sub-phases/4.4-manifest-backup.md)
      - [4.5: Push/Pull Flows](architecture/designs/cloud-synchronisation/sub-phases/4.5-push-pull-flows.md)
  - [File Sharing](architecture/designs/file-sharing/design.md)
    - [Diagram: File Sharing Flow](architecture/designs/file-sharing/diagrams/file-sharing-flow.md)
    - [Sub-Phase Roadmap](architecture/designs/file-sharing/sub-phases/roadmap.md)
      - [5.1: Identity and Contacts](architecture/designs/file-sharing/sub-phases/5.1-identity-and-contacts.md)
      - [5.2: ECIES and Share Packages](architecture/designs/file-sharing/sub-phases/5.2-ecies-and-share-packages.md)
      - [5.3: Cloud Layout and Revocation](architecture/designs/file-sharing/sub-phases/5.3-cloud-layout-and-revocation.md)
  - [Tauri IPC & Frontend](architecture/designs/tauri-ipc-and-frontend/design.md)
    - [Diagram: Session State Machine](architecture/designs/tauri-ipc-and-frontend/diagrams/session-state-machine.md)
    - [Sub-Phase Roadmap](architecture/designs/tauri-ipc-and-frontend/sub-phases/roadmap.md)
      - [6.1: IPC Core and Error Sanitisation](architecture/designs/tauri-ipc-and-frontend/sub-phases/6.1-ipc-core-and-error-sanitisation.md)
      - [6.2: Frontend State and Invoke Wrapper](architecture/designs/tauri-ipc-and-frontend/sub-phases/6.2-frontend-state-and-invoke-wrapper.md)
      - [6.3: Frontend Pages](architecture/designs/tauri-ipc-and-frontend/sub-phases/6.3-frontend-pages.md)
      - [6.4: Zero-Trace and Security Hardening](architecture/designs/tauri-ipc-and-frontend/sub-phases/6.4-zero-trace-and-security-hardening.md)
- [Diagrams](architecture/diagrams/INDEX.md)
  - [End-to-End Encryption Flow](architecture/diagrams/end-to-end-encryption-flow.md)
  - [SSOT Information Flow](architecture/diagrams/ssot-information-flow.md)
  - [Module Dependency Graph](architecture/diagrams/module-dependency-graph.md)
---

# Guides

- [Glossary](guides/glossary.md)
- [Security Model](guides/security-model.md)

---

# Research

- [Overview](research/README.md)
  - [Bin-Packing Small Files into Chunks](research/bin-packing.md)
  - [Compression and Cloud Storage Cost](research/compression-and-cloud-cost.md)
  - [Market & Future Directions](research/market-and-future-directions.md)
  - [Mobile: Encrypted Photo Backup](research/mobile-photo-backup.md)
  - [Multi-Cloud and Storage Destinations](research/multi-cloud-and-storage-destinations.md)
  - [Padding Overhead Reduction](research/padding-overhead-reduction.md)
  - [Password And Key Recovery](research/password-and-key-recovery.md)
  - [MFA for Vault Authentication](research/mfa-for-vault-authentication.md)

