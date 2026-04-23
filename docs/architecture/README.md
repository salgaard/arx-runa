# Architecture Overview

This section explains how Arx Runa is structured internally — the components, data flows, and the reasoning behind key design choices.

## Sections

### [Design Documents](designs/README.md)

Detailed specifications for each part of the system: authentication, encryption, file storage, cloud sync, and the user interface. Each design document explains the goals, the approach taken, and the security properties it must uphold.

### [Global Design Invariants](design-invariants.md)

Cross-phase contracts that apply across all designs, including AAD binding, nonce generation policy, HKDF constants, chunk-size contract, cloud path contract, and Zero-Trace handling rules. Includes MVP scope note and deferred items reference.

### [Deferred Items Inventory](deferred-items-inventory.md)

Complete audit of all phases (0–6.9): resolved handoffs, forward declarations, architectural decisions, intentional MVP limitations, out-of-scope items, and Phase 7+ candidates. **Start here** if planning Phase 7+ features.

### [Phase 7 Roadmap](phase-7-roadmap.md)

Planning document for Phase 7 and beyond: prioritized feature candidates, effort estimates, design dependencies, and success criteria derived from the Deferred Items Inventory.

### [Diagrams](diagrams/INDEX.md)

Visual diagrams showing:
- How keys are derived from your password
- How a file flows from your device to the cloud (and back)
- How the chunk encryption pipeline works
- How file sharing is handled securely
- How the cloud sync sequence operates

## Key Concepts

### Trust Boundary

Arx Runa treats the client device as trusted and all cloud infrastructure as untrusted. The trust boundary is the point at which data leaves the user's machine. All encryption happens client-side; the cloud receives only ciphertext, never plaintext or encryption keys. Even if the cloud provider is compromised or legally compelled to hand over data, an attacker without the master key cannot decrypt anything.

### Key Hierarchy

Your password (and optionally a USB key file) is the single source of trust. From it, Arx Runa derives all the cryptographic keys it needs, each with a separate purpose so a compromise of one does not affect the others.

```
Password + USB Key File
        │
        ▼
    Argon2id (slow key derivation — makes brute-force expensive)
        │
        ▼
   master key (held in locked memory, never written to disk)
        │
        ├──► key encryption key  — protects the per-file encryption keys
        │
        ├──► database key        — encrypts the local file index
        │
        └──► manifest key        — encrypts the cloud-side backup manifest
```

### Chunk Model

Rather than uploading files as-is, Arx Runa splits every file into equal-sized pieces (chunks), encrypts each piece independently, and uploads them separately. This prevents an observer from guessing a file's size or structure by looking at the sizes of what was uploaded.
