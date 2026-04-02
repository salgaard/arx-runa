# VoidGate Documentation

Welcome to the VoidGate technical documentation.

**VoidGate** is a zero-knowledge cloud storage system. Data leaves the client
encrypted, arrives at the cloud as opaque blobs, and comes back readable only
on the client. The gate is the trust boundary — the cloud never holds keys,
filenames, folder structure, or metadata.

## Quick Links

### Architecture

- [**Design Documents**](architecture/designs/README.md) — Detailed technical designs for each subsystem
- [**Diagrams**](architecture/diagrams/INDEX.md) — Visual representations of data flows and structures
- [**Architecture Decisions**](architecture-decisions/README.md) — Rationale for major design choices

### Getting Started

- [**Development Setup**](guides/development.md) — Set up your local development environment

### Reference

- [**Project Roadmap**](roadmap.md) — Development phases and milestones

## Core Pillars

| Pillar | Description |
|--------|-------------|
| **Client-side encryption** | XChaCha20-Poly1305 (AEAD) — plaintext never reaches the cloud |
| **Hardware MFA** | USB key file as mandatory cryptographic factor |
| **Zero-Trace** | Sensitive data zeroed from RAM immediately; no temp files |
| **Fixed-size chunks** | Cloud cannot infer file sizes or structure |
| **Bring Your Own Cloud** | Encrypted blobs via Rclone — no provider lock-in |

## Technology Stack

| Component | Technology |
|-----------|------------|
| Language | Rust (edition 2024) |
| Framework | Tauri (web frontend + Rust backend) |
| Encryption | XChaCha20-Poly1305 via `chacha20poly1305` |
| KDF | Argon2id → HKDF-SHA256 key separation |
| Local DB | SQLite + SQLCipher |
| Cloud transport | Rclone |

## Source Code

- [GitHub Repository](https://github.com/Chorizzio/void-gate)
