<div class="arx-sheet">
  <svg viewBox="0 0 200 236" width="118" height="140" xmlns="http://www.w3.org/2000/svg">
    <path d="M 20 0 L 180 0 L 200 20 L 200 152 L 100 232 L 0 152 L 0 20 Z" fill="#0C0E14" stroke="#222736" stroke-width="1"/>
    <path d="M 30 9 L 170 9 L 188 27 L 188 148 L 100 220 L 12 148 L 12 27 Z" fill="none" stroke="#181D28" stroke-width="0.6"/>
    <line x1="100" y1="30" x2="100" y2="188" stroke="#5C7090" stroke-width="2.2" stroke-linecap="square"/>
    <line x1="100" y1="103" x2="59" y2="148" stroke="#5C7090" stroke-width="2.2" stroke-linecap="square"/>
    <line x1="100" y1="103" x2="141" y2="148" stroke="#5C7090" stroke-width="2.2" stroke-linecap="square"/>
    <line x1="0" y1="20" x2="14" y2="20" stroke="#1C2130" stroke-width="0.5"/>
    <line x1="186" y1="20" x2="200" y2="20" stroke="#1C2130" stroke-width="0.5"/>
  </svg>
  <div class="arx-wordmark">ARX RUNA</div>
  <div class="arx-tagline">Encrypted here &nbsp;·&nbsp; Stored anywhere</div>
  <div class="arx-divider"></div>
  <p class="arx-desc">
    Arx Runa encrypts files locally before cloud storage. Data is chunked and encrypted client-side using XChaCha20-Poly1305 AEAD; keys remain on the user's device and never leave the local system. Cloud providers receive only opaque ciphertext.
  </p>
  <div class="arx-rule-row">
    <div class="arx-rule"><em>Local-first.</em> Encryption happens entirely on your machine. The cloud never sees plaintext.</div>
    <div class="arx-rule"><em>Cloud-agnostic.</em> Rclone syncs sealed shards to any provider you choose — S3, Backblaze, Dropbox, your own server.</div>
    <div class="arx-rule"><em>Zero trust.</em> No accounts, no servers, no third-party key management. Your keys live with you.</div>
  </div>
</div>

**Arx Runa** is a personal file encryption tool built around one principle: your files should be unreadable to anyone but you — including the cloud service storing them.

When you upload a file, Arx Runa encrypts it on your device before it ever leaves. The cloud receives meaningless scrambled data. When you download, Arx Runa decrypts it locally. At no point does the cloud hold your encryption keys, your filenames, your folder structure, or any other metadata. This is called *zero-knowledge* storage.

## Research Problem

Mainstream cloud storage services (OneDrive, Google Drive, Dropbox) require users to trust the provider with their plaintext files, filenames, and metadata. A compromised or legally compelled provider can expose everything. Arx Runa explores whether it is possible to build a practical alternative where the provider is structurally incapable of reading your data.

**Main question:** How can a software solution for secure cloud storage be designed and implemented such that client-side encryption eliminates the need for trust in third-party providers, and how can the use of physical hardware factors (MFA) and Zero-Trace principles minimise the local attack surface on the user's machine?

This breaks down into five sub-questions:

1. **Encryption & key management** — Which modern encryption standards and key management principles best ensure confidentiality and integrity when data is stored outside the user's control?

2. **USB hardware factor** — How can a physical USB device be integrated into the authentication flow so that knowing the password alone is not enough to access data?

3. **Chunking & sync without metadata leakage** — How can files be split, encrypted, and synchronised without revealing filenames, directory structure, or file sizes to the cloud provider?

4. **Zero-Trace: RAM-based UI vs. virtual filesystem** — What are the trade-offs between presenting decrypted data inside an isolated application (in memory only) versus mounting a virtual filesystem, when the goal is to leave the fewest possible traces on the host machine?

5. **File sharing in a zero-trust system** — What cryptographic and protocol challenges arise when enabling per-file sharing between independent users in a system where the cloud is never trusted?

## Explore the Documentation

### What Arx Runa Does

- [**Use Cases**](use-cases/README.md) — Real-world scenarios Arx Runa is designed for
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

- [GitHub Repository(private at the moment)](https://github.com/Chorizzio/arx-runa)
