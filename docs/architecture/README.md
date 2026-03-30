# Architecture Overview

This section contains the technical architecture documentation for VoidGate.

## Sections

### [Design Documents](designs/README.md)

Detailed technical designs for each subsystem, including:
- Rust trait signatures
- SQL DDL schemas
- Wire formats
- Security analysis
- Implementation guidance

### [Diagrams](diagrams/INDEX.md)

Visual representations of:
- Key derivation tree
- Authentication flow
- Chunk encryption pipeline
- Cloud synchronisation sequence
- File sharing protocol

## Key Concepts

### Trust Boundary

The "gate" in VoidGate is the trust boundary. Everything outside the client
(cloud storage, network, servers) is untrusted. Data crosses the gate only
in encrypted form.

### Key Hierarchy

```
Password + USB Key File
        │
        ▼
    Argon2id
        │
        ▼
   master_key (mlocked)
        │
        ├─── HKDF ──► key_encryption_key (wraps file keys)
        │
        ├─── HKDF ──► sqlcipher_key (local database)
        │
        └─── HKDF ──► manifest_key (cloud backup)
```

### Chunk Model

Files are split into fixed-size chunks, each encrypted independently with
a per-file key. Chunk boundaries are uniform (not content-defined) to prevent
size-based inference attacks.
