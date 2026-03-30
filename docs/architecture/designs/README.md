# Design Documents

Detailed technical designs for each VoidGate subsystem. Each document follows
a consistent format:

1. **Overview** — Problem statement and goals
2. **Architecture** — Component structure and relationships
3. **Data Model** — Rust types, SQL schemas, wire formats
4. **Security Analysis** — Threat considerations and mitigations
5. **Implementation Notes** — Guidance for developers

## Documents

| Design | Status | Description |
|--------|--------|-------------|
| [Authentication & Session](authentication-and-session-management.md) | Draft | USB key file, Argon2id KDF, session lifecycle |
| [Chunking & Manifest](chunking-and-manifest.md) | Draft | Fixed-size chunks, SQLCipher manifest, integrity checks |
| [Cloud Synchronisation](cloud-synchronisation.md) | Draft | Rclone transport, conflict resolution, vault header |
| [File Sharing](file-sharing.md) | Draft | X25519 key exchange, share packages, revocation |

## Creating New Designs

Use the `/design` skill to create new design documents following the
established format.
