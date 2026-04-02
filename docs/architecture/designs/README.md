# Design Documents

Detailed technical designs for each VoidGate subsystem. Each design is organized in its own folder containing the main design document, related diagrams, and sub-phase roadmaps (for large designs).

## Folder Structure

Each design folder contains:
- **`design.md`** — Main design document with architecture, data models, security analysis
- **`diagrams/`** — Mermaid diagrams specific to this design
- **`sub-phases/`** (optional) — For large designs (>100 lines or logically separable):
  - `roadmap.md` — Overview and dependency graph
  - Individual sub-phase files (`4.1-cloud-transport.md`, etc.)

## Design Documents

| Design | Status | Sub-Phases | Description |
|--------|--------|------------|-------------|
| [Authentication & Session](authentication-and-session-management/design.md) | Draft | No | USB key file, Argon2id KDF, session lifecycle |
| [Chunking & Manifest](chunking-and-manifest/design.md) | Draft | No | Fixed-size chunks, SQLCipher manifest, integrity checks |
| [Cloud Synchronisation](cloud-synchronisation/design.md) | Draft | **Yes** (5 phases) | Rclone transport, conflict resolution, vault header |
| [Cryptographic Primitives](cryptographic-primitives/design.md) | Draft | No | XChaCha20-Poly1305, HKDF, key derivation tree |
| [File Sharing](file-sharing/design.md) | Draft | No | X25519 key exchange, share packages, revocation |
| [Tauri IPC & Frontend](tauri-ipc-and-frontend/design.md) | Draft | TBD | Error sanitisation, command surface, frontend UI |

## Creating New Designs

Use the `/design` command to create new design documents. The command will:
1. Create a design folder in `docs/architecture/designs/`
2. Generate `design.md` from the design template
3. Create a `diagrams/` subdirectory
4. Generate related diagrams automatically

## When to Create Sub-Phases

Create a sub-phase roadmap (`sub-phases/roadmap.md`) when a design exhibits:
- **Size**: Exceeds ~100-150 lines
- **Trait boundaries**: Multiple trait definitions implementable independently
- **Platform splits**: OS-specific implementations (Windows/Linux)
- **Integration breadth**: Touches 3+ existing modules
- **Multiple flows**: Contains 3+ distinct operational flows

See **[Templates](_templates/)** for sub-phase roadmap and individual sub-phase templates.

## Diagram Co-Location

Design-specific diagrams live in each design's `diagrams/` subdirectory. Cross-cutting diagrams (SSOT flow, etc.) remain in the central `docs/architecture/diagrams/` directory.
