# Arx Runa

[![CI](https://github.com/Chorizzio/arx-runa/actions/workflows/continuous-integration.yml/badge.svg)](https://github.com/Chorizzio/arx-runa/actions/workflows/continuous-integration.yml)
[![Security Audit](https://github.com/Chorizzio/arx-runa/actions/workflows/security-audit.yml/badge.svg)](https://github.com/Chorizzio/arx-runa/actions/workflows/security-audit.yml)
[![Docs](https://github.com/Chorizzio/arx-runa/actions/workflows/documentation-check.yml/badge.svg)](https://github.com/Chorizzio/arx-runa/actions/workflows/documentation-check.yml)
![Rust](https://img.shields.io/badge/rust-2024-orange?logo=rust)

**Zero-knowledge cloud storage** — data leaves encrypted, arrives as opaque
blobs, comes back readable only on the client. The gate is the trust boundary.

## Core Pillars

- **Client-side encryption** — XChaCha20-Poly1305 (AEAD), never plaintext
  in the cloud
- **Hardware MFA** — USB key file as a mandatory cryptographic factor;
  password alone cannot compromise data
- **Zero-Trace** — sensitive data zeroed from RAM immediately after use;
  no temp files, no plaintext on disk
- **Fixed-size padded chunks** — cloud provider cannot infer file sizes,
  structure, or metadata
- **Bring Your Own Cloud** — encrypted blobs via Rclone; no provider lock-in

## Technology Stack

| Component | Technology |
|---|---|
| Language | Rust (edition 2024) |
| Framework | Tauri (web frontend + Rust backend) |
| Encryption | XChaCha20-Poly1305 via `chacha20poly1305` crate |
| KDF | Argon2id → HKDF-SHA256 key separation |
| MFA | USB key file (32 bytes random entropy) |
| Local DB | SQLite + SQLCipher |
| Cloud transport | Rclone |
| Memory safety | `zeroize`, `secrecy`, `mlock`/`VirtualLock` |

## Documentation

- [Development Setup](docs/guides/development.md) — toolchain, IDE, debugging
- [Architecture Decisions](docs/architecture-decisions/) — all major design choices with rationale
- [System Architecture](docs/architecture/) — key derivation, data flow, chunking
- [Threat Model](docs/threat-model/) — security boundaries, assumptions, scope

## Academic Context

This is a bachelor's project in software development. All design decisions are
documented with rationale, trade-offs, and references to established standards
(NIST, OWASP, RFCs).

## License

TBD
