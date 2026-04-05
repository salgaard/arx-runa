# Architecture Overview

This section explains how VoidGate is structured internally — the components, data flows, and the reasoning behind key design choices.

## Sections

### [Design Documents](designs/README.md)

Detailed specifications for each part of the system: authentication, encryption, file storage, cloud sync, and the user interface. Each design document explains the goals, the approach taken, and the security properties it must uphold.

### [Diagrams](diagrams/INDEX.md)

Visual diagrams showing:
- How keys are derived from your password
- How a file flows from your device to the cloud (and back)
- How the chunk encryption pipeline works
- How file sharing is handled securely
- How the cloud sync sequence operates

## Key Concepts

### Trust Boundary

The "gate" in VoidGate is the trust boundary — the line between what you control and what you do not. Your device is trusted; the cloud is not. VoidGate ensures that nothing crosses that boundary in an unencrypted form. Even if the cloud provider is breached, the attacker only finds scrambled data.

### Key Hierarchy

Your password (and optionally a USB key file) is the single source of trust. From it, VoidGate derives all the cryptographic keys it needs, each with a separate purpose so a compromise of one does not affect the others.

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

Rather than uploading files as-is, VoidGate splits every file into equal-sized pieces (chunks), encrypts each piece independently, and uploads them separately. This prevents an observer from guessing a file's size or structure by looking at the sizes of what was uploaded.
