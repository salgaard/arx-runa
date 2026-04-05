# Design Documents

Each major part of VoidGate has a dedicated design document covering how it works, what security properties it must uphold, and how it fits into the overall system.

## Design Documents

| Design | Status | What it covers |
|--------|--------|----------------|
| [Authentication & Session](authentication-and-session-management/design.md) | Complete | How you log in, how the optional USB key file works, and how sessions are managed securely (Argon2id KDF, session lifecycle) |
| [Chunking & Manifest](chunking-and-manifest/design.md) | Complete | How files are split into equal-sized pieces, encrypted, and tracked in a local index (XChaCha20-Poly1305, SQLCipher manifest, integrity checks) |
| [Cloud Synchronisation](cloud-synchronisation/design.md) | Complete | How encrypted chunks are uploaded, downloaded, and kept in sync across devices (Rclone transport, conflict resolution, vault header) |
| [Cryptographic Primitives](cryptographic-primitives/design.md) | Complete | The encryption algorithm and key derivation scheme used throughout VoidGate (XChaCha20-Poly1305 AEAD, HKDF-SHA256, key derivation tree) |
| [File Sharing](file-sharing/design.md) | Complete | How you can share files with others without giving them your password or keys (X25519 key exchange, share packages, revocation) |
| [Tauri IPC & Frontend](tauri-ipc-and-frontend/design.md) | Reviewed | How the user interface communicates with the Rust backend, and how errors are handled safely (Tauri IPC, error sanitisation, command surface) |

## Document Structure

Each design folder contains:
- **`design.md`** — The full specification: goals, data models, security analysis, and implementation notes
- **`diagrams/`** — Diagrams illustrating the flows described in the design
- **`sub-phases/`** (where present) — The design broken into smaller implementation steps, each with its own roadmap
