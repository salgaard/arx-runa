# Use Cases

This section describes the real-world scenarios Arx Runa is built to handle — what a user is trying to do, how Arx Runa helps, and what security guarantees apply.

## Security Tiers

Arx Runa lets you choose how strongly each vault is protected when you create it:

| Tier | What you need to unlock | Best for |
|------|------------------------|----------|
| **Tier 1** | Password only | Everyday use — accessible from any device |
| **Tier 2** | Password + a specific USB file | High-value data — two factors required (password + physical USB key file); opt-in recovery phrase available |

Regardless of tier, the cloud never holds your encryption keys or unencrypted files.

## Use Cases

| | Scenario |
|-|----------|
| [Personal File Backup](use-case-1-personal-file-backup.md) | Back up sensitive files to any cloud provider. Only you can read them, even if the provider is breached. |
| [Cross-Device Access](use-case-2-cross-device-access.md) | Access the same vault from multiple devices. Changes sync automatically without leaking filenames or structure. |
| [Hardware Key & Recovery](use-case-3-hardware-mfa-and-key-loss.md) | Use a physical USB file as a second factor. Covers what happens if the USB or password is lost, and opt-in BIP-39 recovery phrase setup and use. |
| [File Sharing](use-case-4-personal-file-sharing.md) | Share individual files with another person securely — without sharing your password or compromising the vault. |
| [Multi-Destination Backup](use-case-5-multi-destination-backup.md) | Back up to multiple cloud providers simultaneously. Covers mirror and accumulating modes, cloud provider migration, and backup failure recovery. |

## What Each Use Case Covers

Each use case document describes:
- Who is involved and what they are trying to do
- The step-by-step flow of a successful scenario
- What can go wrong and how Arx Runa handles it
- The security properties that must hold throughout

---

<details>
<summary>Design traceability (for developers and reviewers)</summary>

### Sub-Question Traceability

| Sub-question | Description | Covered by |
|---|---|---|
| SQ1 | Encryption standards and key management | Use case 1 |
| SQ2 | USB hardware factor in authentication | Use case 3 |
| SQ3 | Chunking and sync without metadata leakage | Use cases 1, 2 |
| SQ4 | RAM-based UI / Zero-Trace | Use case 1 |
| SQ5 | File sharing in a zero-trust system | Use case 4 |

### Design Document Coverage

- [Authentication & Session Management](../architecture/designs/authentication-and-session-management/design.md)
- [Cryptographic Primitives](../architecture/designs/cryptographic-primitives/design.md)
- [Chunking & Manifest](../architecture/designs/chunking-and-manifest/design.md)
- [Cloud Synchronisation](../architecture/designs/cloud-synchronisation/design.md)
- [File Sharing](../architecture/designs/file-sharing/design.md)
- [Tauri IPC & Frontend](../architecture/designs/tauri-ipc-and-frontend/design.md)


</details>
