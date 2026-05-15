# Arx Runa — Architecture

Zero-knowledge bring-your-own-cloud file encryption. The client encrypts before upload; the cloud receives only opaque blobs — no keys, file names, or metadata.

---

## Design Documents

| Document | Description |
|----------|-------------|
| [Project Scaffolding](design-project-scaffolding.md) | Workspace layout, tech stack (Tauri v2, Leptos 0.8, Trunk, Tailwind v4), dependencies |
| [Cryptographic Primitives](design-cryptographic-primitives.md) | XChaCha20-Poly1305, HKDF-SHA256, per-file keys, BLAKE3 integrity, wire formats |
| [Authentication and Session Management](design-authentication.md) | Argon2id, USB key file, session lifecycle, vault creation, password change, recovery |
| [Chunking and Manifest](design-chunking-and-manifest.md) | Fixed-size chunks, zero-padding, SQLCipher manifest, EXIF stripping, encrypt/decrypt pipeline |
| [Cloud Synchronisation](design-cloud-synchronisation.md) | Rclone sidecar, `CloudTransport` trait, push/pull flows, multi-destination model, vault header |
| [File Sharing](design-file-sharing.md) | HPKE (RFC 9180), X25519 identity, share packages, revocation, download receipts |
| [Tauri IPC and Frontend](design-tauri-ipc-and-frontend.md) | Canonical command surface, `IpcError`, `AppState`, Zero-Trace, Leptos frontend |
| [Future Work](future-work.md) | Deferred features with known design paths |

---

## Key Properties

- **Zero-knowledge**: the cloud provider sees only uniform-size encrypted blobs with random UUID names — no file names, sizes, or folder structure
- **Bring your own cloud**: works with any Rclone-supported backend (S3, Backblaze B2, Google Drive, SFTP, local, and more)
- **Per-file encryption**: each file has its own randomly generated key, enabling fine-grained sharing and secure deletion
- **Two authentication tiers**: password only (Tier 1) or password + USB key file (Tier 2)
- **Session memory safety**: keys held in mlocked RAM, zeroized on timeout or lock
- **No central server**: identity and sharing use local X25519 keypairs with out-of-band key exchange
