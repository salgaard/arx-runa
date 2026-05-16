<div class="arx-sheet">
  <svg viewBox="0 0 200 236" width="118" height="140" xmlns="http://www.w3.org/2000/svg">
    <path d="M 20 0 L 180 0 L 200 20 L 200 152 L 100 232 L 0 152 L 0 20 Z" style="fill: #0C0E14; stroke: #222736; stroke-width: 1"/>
    <path d="M 30 9 L 170 9 L 188 27 L 188 148 L 100 220 L 12 148 L 12 27 Z" style="fill: none; stroke: #222736; stroke-width: 0.6; opacity: 0.5"/>
    <line x1="100" y1="30" x2="100" y2="188" style="stroke: #5C7090; stroke-width: 2.2; stroke-linecap: square"/>
    <line x1="100" y1="103" x2="59" y2="148" style="stroke: #5C7090; stroke-width: 2.2; stroke-linecap: square"/>
    <line x1="100" y1="103" x2="141" y2="148" style="stroke: #5C7090; stroke-width: 2.2; stroke-linecap: square"/>
    <line x1="0" y1="20" x2="14" y2="20" style="stroke: #222736; stroke-width: 0.5"/>
    <line x1="186" y1="20" x2="200" y2="20" style="stroke: #222736; stroke-width: 0.5"/>
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

**Main question:** How can a software solution for secure cloud storage be designed and implemented such that client-side encryption eliminates the need for trust in third-party providers, and how can the use of physical hardware factors (MFA) and "Zero-Trace" principles minimise the local attack surface on the user's machine?

This breaks down into five sub-questions:

1. **Encryption standards and key management:** Which modern encryption standards and key management principles are best suited for ensuring data confidentiality and integrity when data must be stored in an environment outside the user's control?

2. **USB key file factor and offline recovery:** How can a physical USB key file be integrated into the authentication flow as a mandatory second factor — ensuring that password knowledge alone is insufficient to access vault data — and how can an offline BIP-39 recovery mechanism enable user-controlled credential recovery without delegating trust to a third party or introducing a server-side backdoor?

3. **Chunking, synchronisation, and provider-agnostic storage:** How can effective chunking and synchronisation logic be implemented to upload changes to the cloud without revealing filenames, directory structures, or metadata to the cloud provider — and how can the synchronisation protocol maintain consistency across multiple devices while remaining provider-agnostic, enabling redundant backup to multiple destinations without re-encryption?

4. **Zero-Trace operation through a RAM-based UI:** How can a RAM-based in-application UI achieve Zero-Trace operation — ensuring that decrypted file content is never written to disk during a session — and what forensic residue, if any, persists on the host machine after the vault is locked?

5. **File sharing in a zero-trust system:** What cryptographic and protocol-level challenges arise when enabling file-granularity sharing between independent users in a zero-trust client-side encrypted system, and how does the proposed sharing architecture compare to existing approaches such as OneDrive sharing links and Cryptomator shared vaults?

## Explore some stuff

### What Arx Runa Does

- [**Use Cases**](use-cases/index.html) — Real-world scenarios Arx Runa is designed for

### How It Works

- [**How It Works**](how-it-works/index.html) — Plain-language walkthroughs: the vault, unlocking, encryption, cloud sync, sharing, and recovery

### Going Deeper

- [**Deep Dives**](research/index.html) — Research-level detail on cryptographic choices, file sharing, key recovery, and padding

### Reference

- [**Glossary**](guides/glossary.md) — Term definitions
- [**Security Model**](guides/security-model.md) — Trust boundaries and threat model

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
