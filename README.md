# Arx Runa

<div align="center">

<svg viewBox="0 0 200 236" width="118" height="140" xmlns="http://www.w3.org/2000/svg">
  <path d="M 20 0 L 180 0 L 200 20 L 200 152 L 100 232 L 0 152 L 0 20 Z" fill="#0C0E14" stroke="#222736" stroke-width="1"/>
  <path d="M 30 9 L 170 9 L 188 27 L 188 148 L 100 220 L 12 148 L 12 27 Z" fill="none" stroke="#181D28" stroke-width="0.6"/>
  <line x1="100" y1="30" x2="100" y2="188" stroke="#5C7090" stroke-width="2.2" stroke-linecap="square"/>
  <line x1="100" y1="103" x2="59" y2="148" stroke="#5C7090" stroke-width="2.2" stroke-linecap="square"/>
  <line x1="100" y1="103" x2="141" y2="148" stroke="#5C7090" stroke-width="2.2" stroke-linecap="square"/>
  <line x1="0" y1="20" x2="14" y2="20" stroke="#1C2130" stroke-width="0.5"/>
  <line x1="186" y1="20" x2="200" y2="20" stroke="#1C2130" stroke-width="0.5"/>
</svg>

### Encrypted here · Stored anywhere

</div>

[![CI](https://github.com/Chorizzio/arx-runa/actions/workflows/continuous-integration.yml/badge.svg)](https://github.com/Chorizzio/arx-runa/actions/workflows/continuous-integration.yml)
[![Security Audit](https://github.com/Chorizzio/arx-runa/actions/workflows/security-audit.yml/badge.svg)](https://github.com/Chorizzio/arx-runa/actions/workflows/security-audit.yml)
[![Docs](https://github.com/Chorizzio/arx-runa/actions/workflows/documentation-check.yml/badge.svg)](https://github.com/Chorizzio/arx-runa/actions/workflows/documentation-check.yml)
![Rust](https://img.shields.io/badge/rust-2024-orange?logo=rust)

**Zero-knowledge cloud storage** — files are encrypted on your device before upload. The cloud receives only opaque ciphertext. Keys never leave your machine.

---

**📖 [Read the full documentation](https://chorizzio.github.io/arx-runa/)** — architecture, design decisions, threat model, and development guides.

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

**📖 [Full Documentation on GitHub Pages](https://chorizzio.github.io/arx-runa/)**

Quick links:
- [Development Setup](https://chorizzio.github.io/arx-runa/guides/development.html) — toolchain, IDE, debugging
- [Architecture Decisions](https://chorizzio.github.io/arx-runa/architecture-decisions/) — all major design choices with rationale
- [System Architecture](https://chorizzio.github.io/arx-runa/architecture/) — key derivation, data flow, chunking
- [Threat Model](https://chorizzio.github.io/arx-runa/threat-model/) — security boundaries, assumptions, scope

## Academic Context

This is a bachelor's project in software development. All design decisions are
documented with rationale, trade-offs, and references to established standards
(NIST, OWASP, RFCs).

## License

TBD
