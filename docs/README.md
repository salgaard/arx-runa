# VoidGate

**VoidGate** is a personal cloud storage application built around one principle: your files should be unreadable to anyone but you — including the cloud service storing them.

When you upload a file, VoidGate encrypts it on your device before it ever leaves. The cloud receives meaningless scrambled data. When you download, VoidGate decrypts it locally. At no point does the cloud hold your encryption keys, your filenames, your folder structure, or any other metadata. This is called *zero-knowledge* storage.

## Research Problem

Mainstream cloud storage services (OneDrive, Google Drive, Dropbox) require users to trust the provider with their plaintext files, filenames, and metadata. A compromised or legally compelled provider can expose everything. VoidGate explores whether it is possible to build a practical alternative where the provider is structurally incapable of reading your data.

**Main question:** How can a software solution for secure cloud storage be designed and implemented such that client-side encryption eliminates the need for trust in third-party providers, and how can the use of physical hardware factors (MFA) and Zero-Trace principles minimise the local attack surface on the user's machine?

This breaks down into five sub-questions:

1. **Encryption & key management** — Which modern encryption standards and key management principles best ensure confidentiality and integrity when data is stored outside the user's control?

2. **USB hardware factor** — How can a physical USB device be integrated into the authentication flow so that knowing the password alone is not enough to access data?

3. **Chunking & sync without metadata leakage** — How can files be split, encrypted, and synchronised without revealing filenames, directory structure, or file sizes to the cloud provider?

4. **Zero-Trace: RAM-based UI vs. virtual filesystem** — What are the trade-offs between presenting decrypted data inside an isolated application (in memory only) versus mounting a virtual filesystem, when the goal is to leave the fewest possible traces on the host machine?

5. **File sharing in a zero-trust system** — What cryptographic and protocol challenges arise when enabling per-file sharing between independent users in a system where the cloud is never trusted?

## Explore the Documentation

### What VoidGate Does

- [**Use Cases**](use-cases/README.md) — Real-world scenarios VoidGate is designed for
- [**Project Roadmap**](roadmap.md) — Development phases and what is planned

### How It Works

- [**Design Documents**](architecture/designs/README.md) — Detailed technical designs for each part of the system
- [**Architecture Diagrams**](architecture/diagrams/INDEX.md) — Visual overviews of data flows

## Core Features

| Feature | What it means |
|---------|---------------|
| **Client-side encryption** | Your files are encrypted on your device before upload — the cloud only ever sees scrambled data |
| **Hardware security key** | Optionally require a physical USB file as a second factor, so a stolen password is not enough |
| **Zero-Trace** | Sensitive data is erased from memory as soon as it is no longer needed; no temporary files are written |
| **Fixed-size chunks** | Files are split into equal-sized pieces so the cloud cannot guess file sizes from upload patterns |
| **Bring Your Own Cloud** | Works with any cloud provider (Dropbox, Google Drive, S3, etc.) — no lock-in |

## Technology Stack

| Component | Technology | Purpose |
|-----------|------------|---------|
| Language | Rust | Memory-safe systems programming |
| Application framework | Tauri | Native desktop app with a web-based UI |
| Encryption algorithm | XChaCha20-Poly1305 | Fast, secure authenticated encryption |
| Key derivation | Argon2id + HKDF-SHA256 | Turns passwords into strong cryptographic keys |
| Local database | SQLite + SQLCipher | Encrypted local storage for file metadata |
| Cloud transport | Rclone | Provider-agnostic file transfer |

## Source Code

- [GitHub Repository(private at the moment)](https://github.com/Chorizzio/void-gate)
