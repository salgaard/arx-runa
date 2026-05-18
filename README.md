# Arx Runa

<div align="center">

<img src="docs/arx-runa-logo.svg" width="118" height="140" alt="Arx Runa Logo">

<h3>Encrypted here · Stored anywhere</h3>

</div>

[![CI](https://github.com/salgaard/arx-runa/actions/workflows/continuous-integration.yml/badge.svg)](https://github.com/salgaard/arx-runa/actions/workflows/continuous-integration.yml)
[![Release](https://github.com/salgaard/arx-runa/actions/workflows/release.yml/badge.svg)](https://github.com/salgaard/arx-runa/actions/workflows/release.yml)
[![Security Audit](https://github.com/salgaard/arx-runa/actions/workflows/security-audit.yml/badge.svg)](https://github.com/salgaard/arx-runa/actions/workflows/security-audit.yml)
[![Deploy Docs](https://github.com/salgaard/arx-runa/actions/workflows/docs.yml/badge.svg)](https://github.com/salgaard/arx-runa/actions/workflows/docs.yml)
[![Secret Scan](https://github.com/salgaard/arx-runa/actions/workflows/secret-scan.yml/badge.svg)](https://github.com/salgaard/arx-runa/actions/workflows/secret-scan.yml)
[![Gitlab Mirror](https://github.com/salgaard/arx-runa/actions/workflows/mirror.yml/badge.svg)](https://github.com/salgaard/arx-runa/actions/workflows/mirror.yml)
![Rust](https://img.shields.io/badge/rust-2024-orange?logo=rust)

> ⚠️ **Early demo — expect bugs and data loss.** This is pre-release software. Encrypted vaults, keys, and file metadata may be lost or corrupted between versions. Do **not** rely on this as your only copy of important files.

**Zero-knowledge file encryption tool** — files are encrypted on your device before upload. The cloud receives only opaque ciphertext. Keys never leave your machine.

---

**📖 [Read more stuff!](https://salgaard.github.io/arx-runa/)**

## Core Pillars

| Feature | What it means |
|---------|---------------|
| **Client-side encryption** | Your files are encrypted on your device before upload — the cloud only ever sees opaque ciphertext blobs |
| **Tiered authentication** | Tier 1 (password only) or Tier 2 (password + 32-byte USB key file); both are combined before key derivation, so neither factor alone is sufficient |
| **Zero-Trace** | Sensitive data is zeroed from memory as soon as it is no longer needed; session keys are mlock'd so the OS cannot page them to disk; no temporary plaintext files are written |
| **Fixed-size chunks with BLAKE3 integrity** | Files are split into equal-sized, padded chunks so the cloud cannot guess sizes; every chunk is BLAKE3-hashed and verified before decryption to catch bit rot or tampering |
| **Secure file sharing** | Share individual files using HPKE (RFC 9180) with X25519 identities — only the recipient's private key opens the share; the cloud sees only encrypted blobs |
| **Bring Your Own Cloud** | Works with any provider Rclone supports (S3, Backblaze B2, Dropbox, Google Drive, and 70+ more) — no lock-in, multiple destinations supported |

## Technology Stack

| Component | Technology | Purpose |
|-----------|------------|---------|
| Language | Rust (edition 2024) | Memory-safe systems programming |
| Application framework | Tauri | Native desktop shell and Rust backend |
| UI framework | Leptos (Rust/WASM, CSR) | Reactive frontend compiled to WebAssembly |
| Encryption | XChaCha20-Poly1305 | Authenticated encryption for every chunk; 192-bit random nonce per chunk |
| Key derivation | Argon2id → HKDF-SHA256 | Memory-hard password hardening; then key expansion into independent vault keys |
| File sharing | HPKE (RFC 9180) with X25519 | End-to-end encrypted share packages; only the recipient's private key can open them |
| Integrity | BLAKE3 | Per-chunk checksums recorded in the manifest; verified on download before decryption |
| Local database | SQLite + SQLCipher | Encrypted manifest: file paths, chunk records, wrapped file keys |
| Cloud transport | Rclone | Provider-agnostic transfer to 70+ storage backends |
| Memory safety | `zeroize`, `secrecy`, `mlock`/`VirtualLock` | Keys zeroed after use; locked memory never paged to disk |

## Quick links

- [Intro](https://salgaard.github.io/arx-runa/)
- [Use Cases](https://salgaard.github.io/arx-runa/use-cases/index.html)
- [How it works](https://salgaard.github.io/arx-runa/how-it-works/index.html)
