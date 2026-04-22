# Arx Runa Implementation Roadmap

Arx Runa is built in ten sequential phases, each delivering a distinct, self-contained piece of the system. The phases are ordered by dependency: cryptographic foundations come first, then authentication, then storage, cloud sync, sharing, and finally the user interface and testing.

| Phase | What gets built | Status |
|-------|----------------|--------|
| 0 — Scaffolding | Project structure, build pipeline, CI | Complete |
| 1 — Cryptographic Primitives | Encryption, key derivation, chunk encryption/decryption | Complete |
| 2 — Authentication & Session | Login flow, USB key file, session lifecycle and timeout | Planned |
| 3 — Storage & Chunking | File splitting, local encrypted database, blob staging | Planned |
| 4 — Cloud Synchronisation | Rclone integration, upload/download, new-device recovery | Planned |
| 5 — File Sharing | Per-file sharing via encrypted share packages, revocation | Planned |
| 6 — Tauri IPC & Frontend | User interface, backend commands, error handling | Planned |
| 7 — Integration Testing | End-to-end tests covering all modules and adversarial scenarios | Planned |
| 8 — Threat Model & Report | Formal threat model, architecture comparison, report consolidation | Planned |
| 9 — Hardening & Submission | Security review, dependency audit, final polish | Planned |

---

## Design Documents

Technical specifications (algorithms, schemas, wire formats) live in the canonical design documents. The roadmap does not duplicate them.

- **Project scaffolding**: [`docs/architecture/designs/project-scaffolding/design.md`](architecture/designs/project-scaffolding/design.md)
- **Cryptographic primitives**: [`docs/architecture/designs/cryptographic-primitives/design.md`](architecture/designs/cryptographic-primitives/design.md)
- **Authentication & session**: [`docs/architecture/designs/authentication-and-session-management/design.md`](architecture/designs/authentication-and-session-management/design.md)
- **Chunking & manifest**: [`docs/architecture/designs/chunking-and-manifest/design.md`](architecture/designs/chunking-and-manifest/design.md)
- **Cloud synchronization**: [`docs/architecture/designs/cloud-synchronisation/design.md`](architecture/designs/cloud-synchronisation/design.md)
- **File sharing**: [`docs/architecture/designs/file-sharing/design.md`](architecture/designs/file-sharing/design.md)
- **Tauri IPC & frontend**: [`docs/architecture/designs/tauri-ipc-and-frontend/design.md`](architecture/designs/tauri-ipc-and-frontend/design.md)
- **Cross-phase invariants**: [`docs/architecture/design-invariants.md`](architecture/design-invariants.md)

---

## Phase 0 — Project Scaffolding

**Depends on**: nothing
**Design document**: [`project-scaffolding/design.md`](architecture/designs/project-scaffolding/design.md)

Establish the compilable project skeleton — directory structure, Tauri workspace, dependency declarations, and CI pipeline — so all subsequent phases have a stable foundation.

---

## Phase 1 — Cryptographic Primitives

**Depends on**: Phase 0
**Design document**: [`cryptographic-primitives/design.md`](architecture/designs/cryptographic-primitives/design.md)

Implement the foundational cryptographic operations: HKDF key derivation, per-file key management, chunk encryption/decryption with AAD binding, BLAKE3 checksums, and memory-safe key handling.

Recovery-slot wrapping (`wrap_master_key_for_recovery` / `unwrap_master_key_from_recovery`) is Phase 2 work because it depends on the `MasterKey` type introduced by the authentication design.

---

## Phase 2 — Authentication and Session Management

**Depends on**: Phase 1
**Design document**: [`authentication-and-session-management/design.md`](architecture/designs/authentication-and-session-management/design.md)

Implement the full authentication flow: USB key file generation and auto-detection, Argon2id KDF producing `master_key`, session lifecycle with locked memory, session timeout with zeroization, and vault creation/password-change/key-rotation flows.

---

## Phase 3 — Storage: Chunking and Manifest

**Depends on**: Phase 1, Phase 2
**Design document**: [`chunking-and-manifest/design.md`](architecture/designs/chunking-and-manifest/design.md)

Implement the fixed-size chunking pipeline, the SQLCipher manifest database, and the local file-to-chunk-to-blob workflow without cloud sync.

---

## Phase 4 — Cloud Synchronisation

**Depends on**: Phase 3
**Design document**: [`cloud-synchronisation/design.md`](architecture/designs/cloud-synchronisation/design.md)

Implement the `CloudTransport` trait backed by Rclone, vault header upload/download, manifest cloud backup, and the full upload/download cycle including new-device recovery.

---

## Phase 5 — Identity and File Sharing

**Depends on**: Phase 1, Phase 3, Phase 4
**Design document**: [`file-sharing/design.md`](architecture/designs/file-sharing/design.md)

Implement the file sharing layer: local X25519 identity, contact management, encrypted share package creation and import, shared blob cloud layout, revocation, and optional receipts and expiration.

---

## Phase 6 — Tauri IPC Layer and Frontend

**Depends on**: Phase 2, Phase 3, Phase 4, Phase 5
**Design document**: [`tauri-ipc-and-frontend/design.md`](architecture/designs/tauri-ipc-and-frontend/design.md)

Expose backend functionality through Tauri commands with error sanitisation, and build the user-facing UI for authentication, vault browsing, file transfer, sync, and sharing workflows.

| Sub-phase | What gets built | Commands covered |
|-----------|----------------|-----------------|
| 6.1 — IPC Core & Types | Command registration, error sanitisation, IPC types | all 29 (stubbed) |
| 6.2 — Frontend State & Invoke | `SessionProvider`, `VaultProvider`, `SyncProvider`, type-safe `invoke_command` | `get_session_status`, `list_directory` |
| 6.3 — Frontend Pages | `LoginPage`, `VaultCreationPage`, `VaultBrowser`, `DropZone`, `ProgressModal`, `AppShell` | `authenticate`, `create_vault`, `upload_file`, `lock_session` |
| 6.4 — Zero-Trace Hardening | CSP, password zeroization, state clearing on lock, auth backoff | — |
| 6.5 — Backend Command Wiring | Connect all stubs to Phase 2–5 implementations, progress channels | all 29 (wired) |
| 6.6 — File Operations UI | Download, delete, and inline file preview | `download_file`, `delete_file`, `get_file_content` |
| 6.7 — Sync & Destination UI | Sync button, destination manager, sync status polling | `sync_to_cloud`, `get_sync_status`, `add_destination`, `list_destinations`, `delete_destination` |
| 6.8 — Vault Settings UI | Change password, rotate key file, delete vault | `change_password`, `rotate_key_file`, `delete_vault` |
| 6.9 — Sharing UI | Contacts, share file, received/sent share panels | `export_public_key`, `add_contact`, `list_contacts`, `share_file`, `import_share`, `revoke_share`, `list_shares`, `list_received_shares` |

---

## Phase 7 — End-to-End Integration Testing

**Depends on**: Phases 1–6

Validate the complete pipeline from authentication through file upload, cloud sync, sharing, and new-device recovery. Cover both happy-path and adversarial scenarios across all module boundaries.

---

## Phase 8 — Threat Model, Architecture Comparison, and Report

**Depends on**: Phase 7

Produce the formal threat model, architecture comparison (Arx Runa vs. OneDrive vs. Cryptomator sharing), and consolidate report-log entries into the bachelor report structure.

---

## Phase 9 — Hardening and Submission

**Depends on**: Phase 8

Final quality pass: security review of all crypto-adjacent modules, dependency audit, CI clean, and submission preparation.

---

## Dependency Graph

```
Phase 0  (scaffolding)
    │
    v
Phase 1  (crypto primitives)
    │
    v
Phase 2  (auth + session)
    │
    v
Phase 3  (chunking + manifest)
    │
    v
Phase 4  (cloud sync)
    │
    v
Phase 5  (identity + file sharing)
    │
    v
Phase 6  (Tauri IPC + frontend)
    │
    v
Phase 7  (integration testing)
    │
    v
Phase 8  (threat model + report)
    │
    v
Phase 9  (hardening + submission)
```
