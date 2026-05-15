# Arx Runa

<div align="center">

<img src="docs/arx-runa-logo.svg" width="118" height="140" alt="Arx Runa Logo">

<h3>Encrypted here · Stored anywhere</h3>

</div>

[![CI](https://github.com/salgaard/arx-runa/actions/workflows/continuous-integration.yml/badge.svg)](https://github.com/salgaard/arx-runa/actions/workflows/continuous-integration.yml)
[![Security Audit](https://github.com/salgaard/arx-runa/actions/workflows/security-audit.yml/badge.svg)](https://github.com/salgaard/arx-runa/actions/workflows/security-audit.yml)
[![Docs](https://github.com/salgaard/arx-runa/actions/workflows/documentation-check.yml/badge.svg)](https://github.com/salgaard/arx-runa/actions/workflows/documentation-check.yml)
![Rust](https://img.shields.io/badge/rust-2024-orange?logo=rust)

**Zero-knowledge file encryption tool** — files are encrypted on your device before upload. The cloud receives only opaque ciphertext. Keys never leave your machine.

---

**📖 [Read the full documentation](https://salgaard.github.io/arx-runa/)** — architecture, threat model, roadmap, and development guides.

## Core Pillars

- **Client-side encryption** — files are encrypted before upload; the cloud
  stores opaque ciphertext blobs only
- **Tiered authentication** — Tier 1 (password only) or Tier 2 (password +
  32-byte USB key file); both combined before key derivation
- **Zero-Trace policy** — plaintext is transient in active memory only and is
  never persisted to disk, logs, or telemetry; session keys are mlock'd and
  zeroed on lock or timeout
- **Metadata-minimizing storage** — fixed-size padded chunks, random UUID blob
  names, and EXIF/XMP/IPTC stripping in memory before encryption reduce
  leakage of file structure, sizes, and embedded personal data
- **Tamper-evident chunks** — each encrypted blob is BLAKE3-hashed; the
  checksum is verified before decryption so bit rot or tampering is caught
  immediately
- **Secure sharing** — share packages use HPKE (RFC 9180) with X25519/HKDF-SHA256/ChaCha20-Poly1305; only the recipient's private key can open the envelope
- **Bring Your Own Cloud** — encrypted blobs via Rclone; no provider lock-in

## Technology Stack

| Component | Technology |
|---|---|
| Language | Rust (edition 2024) |
| Framework | Tauri (native shell + Rust backend) |
| UI | Leptos (Rust/WASM frontend, CSR mode) |
| Encryption | XChaCha20-Poly1305 via `chacha20poly1305` crate |
| KDF | Argon2id → HKDF-SHA256 key separation |
| Authentication | Tier 1 password-only or Tier 2 password + USB key file |
| File sharing | HPKE (RFC 9180) with X25519/HKDF-SHA256/ChaCha20-Poly1305 |
| Integrity | BLAKE3 per-chunk checksums; verified before decryption |
| Local DB | SQLite + SQLCipher |
| Cloud transport | Rclone |
| Memory safety | `zeroize`, `secrecy`, `mlock`/`VirtualLock` |

## Documentation

Quick links:
- [Use Cases](https://salgaard.github.io/arx-runa/use-cases/index.html) — Real-world scenarios Arx Runa is designed for
- [Roadmap](https://salgaard.github.io/arx-runa/roadmap.html) — Implementation roadmap
- [Architecture](https://salgaard.github.io/arx-runa/architecture/index.html) — Detailed technical designs for each part of the system
- [Glossary](https://salgaard.github.io/arx-runa/guides/glossary.html) — Terms used consistently across Arx Runa
- [Research](https://salgaard.github.io/arx-runa/research/index.html) — Research to critique and explore ideas and options
