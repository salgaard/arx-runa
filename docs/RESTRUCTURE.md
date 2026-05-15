# Docs Restructure Blueprint

**Goal**: Replace the Architecture section with a "How It Works" section — 6 concept pages
pitched at a technically curious user who wants to understand the security model, not implement it.
Nerdy but not code-level. Each page tells a story and owns its diagrams.

**Audience**: Potential users / security-conscious people evaluating trust. Not developers.

---

## What Changes in SUMMARY.md

Remove entirely from sidebar (keep files in source, just drop from docs/SUMMARY.md):
- All of `architecture/designs/*/` (sub-phases, sub-phase roadmaps, per-design diagrams)
- `architecture/design-invariants.md`
- `roadmap.md`
- `architecture/designs/README.md`
- `architecture-decisions/` section (all ADRs)
- `architecture/diagrams/` section (cross-cutting diagrams go into concept pages instead)
- `architecture/README.md`

Add new section: **How It Works** with 6 new pages under `how-it-works/`.

### New SUMMARY.md (complete, ready to apply)

```markdown
# Summary

[Introduction](README.md)

---

# Use Cases

- [Overview](use-cases/README.md)
  - [Use Case 1: Zero-Knowledge Personal Backup](use-cases/use-case-1-personal-file-backup.md)
  - [Use Case 2: Cross-Device Synchronisation](use-cases/use-case-2-cross-device-access.md)
  - [Use Case 3: Hardware MFA and Key Loss](use-cases/use-case-3-hardware-mfa-and-key-loss.md)
  - [Use Case 4: Personal File Sharing](use-cases/use-case-4-personal-file-sharing.md)
  - [Use Case 5: Multi Destination Backup](use-cases/use-case-5-multi-destination-backup.md)

---

# How It Works

- [Overview](how-it-works/README.md)
  - [The Vault](how-it-works/the-vault.md)
  - [Unlocking: Password and USB Key](how-it-works/unlocking.md)
  - [Recovery: If You Lose Your Key](how-it-works/recovery.md)
  - [How Files Are Encrypted](how-it-works/file-encryption.md)
  - [What the Cloud Sees](how-it-works/cloud-sync.md)
  - [Sharing Files Privately](how-it-works/file-sharing.md)

---

# Guides

- [Glossary](guides/glossary.md)
- [Security Model](guides/security-model.md)

---

# Research

- [Overview](research/README.md)
  - [Bin-Packing Small Files into Chunks](research/bin-packing.md)
  - [Compression and Cloud Storage Cost](research/compression-and-cloud-cost.md)
  - [Market & Future Directions](research/market-and-future-directions.md)
  - [Mobile: Encrypted Photo Backup](research/mobile-photo-backup.md)
  - [Multi-Cloud and Storage Destinations](research/multi-cloud-and-storage-destinations.md)
  - [Padding Overhead Reduction](research/padding-overhead-reduction.md)
  - [Password And Key Recovery](research/password-and-key-recovery.md)
  - [MFA for Vault Authentication](research/mfa-for-vault-authentication.md)
  - [Authentication and Session Management](research/authentication-and-session-management.md)
  - [Cryptographic Primitive Rationale](research/cryptographic-primitive-rationale.md)
  - [File Sharing Cryptography](research/file-sharing-cryptography.md)

---

# Alternative Research

- [Overview](research-alternatives/README.md)
  - [Rust Programming Language (Long-form Draft)](research-alternatives/rust-programming-language-long-form.md)
```

---

## Files to Create

All go in `docs/how-it-works/`. The directory does not exist yet — create it.

---

### `how-it-works/README.md` — Overview

**One paragraph intro to the section.** Sets expectations: these pages explain *what* the system
does and *why it's trustworthy*, not how to build it. Mentions the key guarantee up front:
everything is encrypted before it leaves your device. Links to the 6 sub-pages.

Source material: none needed — write from scratch, 1 short page.

---

### `how-it-works/the-vault.md` — The Vault

**Narrative angle**: What is a vault? Where does it live? What does it contain? Why does the
master key never leave the device?

**Pitch level**: Explain the concept of a master key, key derivation into sub-keys (one for
files, one for the manifest database, etc.), and the vault header. Use an analogy if helpful.
No Rust signatures. No contract tables.

**Concepts to cover**:
- A vault is a local encrypted database + a set of encrypted blobs in the cloud
- The master key is derived from your password (+ optional USB key) and never stored anywhere
- All other keys descend from the master key via HKDF — explain this as a key tree
- The vault header (stored in cloud) contains encrypted copies of keys + parameters needed to
  re-derive, but nothing an attacker without the password can use
- The manifest (SQLCipher database) tracks file→chunk mappings, encrypted with its own derived key

**Diagram to embed**: `architecture/designs/cryptographic-primitives/diagrams/key-derivation-tree.md`

**Source sections** (for the writer to read, not quote verbatim):
- `architecture/designs/cryptographic-primitives/design.md` §Key Derivation, §Per-File Key Management
- `architecture/designs/authentication-and-session-management/design.md` §Vault Creation Flow,
  §Input Construction for Argon2id > Key derivation from master_key
- `architecture/designs/chunking-and-manifest/design.md` §Manifest Database Schema (just the concept,
  not the SQL)

---

### `how-it-works/unlocking.md` — Unlocking: Password and USB Key

**Narrative angle**: What actually happens when you unlock your vault? Why does it feel fast even
though it's doing expensive cryptography? What does the USB key add?

**Pitch level**: Walk through the unlock flow as a sequence of events. Explain Argon2id in one
sentence (deliberately slow to resist brute force). Explain that the USB key file is a second
secret combined with the password — losing one without the other is useless. Explain session
keys and auto-lock on USB removal.

**Concepts to cover**:
- Password alone is not enough if USB MFA is enabled — both are required
- Argon2id is a memory-hard KDF: deliberately slow so an attacker can't brute-force offline
- The USB key file is 64 bytes of random data on a physical drive — it's a hardware second factor
  without any special hardware
- USB auto-detection: Arx Runa finds the key file by BLAKE3 fingerprint — you don't pick a path
- Once unlocked, session keys live in locked memory (mlock) and are zeroed on lock/timeout
- Auto-lock on USB removal: removing the drive ends the session

**Diagram to embed**: `architecture/designs/authentication-and-session-management/diagrams/authentication-flow.md`

**Source sections**:
- `architecture/designs/authentication-and-session-management/design.md` §USB Key File (all
  subsections), §Input Construction for Argon2id (Password/key combination, Argon2id parameters),
  §Session Management (session lifecycle, timeout mechanism, memory locking)

---

### `how-it-works/recovery.md` — Recovery: If You Lose Your Key

**Narrative angle**: What are your options when things go wrong? This is a page that builds trust
by showing the system has thought about failure modes.

**Pitch level**: Two distinct failure scenarios — lost password/USB key, and new device. Each has
a specific recovery path. Explain the recovery phrase concept (like a seed phrase), and the
new-device bootstrap flow. Be honest about what is NOT recoverable (if you lose both your
password and your recovery phrase, your data is gone — this is a feature).

**Concepts to cover**:
- Recovery slot: a separate key slot derived from a BIP39-style recovery phrase (shown once at
  vault creation, must be written down)
- Recovery flow: enter phrase → Argon2id derives recovery key → unwraps master key → normal session
- New-device recovery: download the vault header from cloud, enter recovery phrase, bootstrap
  on the new device
- USB key rotation: how you replace a lost USB key (requires knowing the password)
- The hard limit: if you lose both your password and your recovery phrase, the vault cannot be
  opened — this is intentional, it means Arx Runa itself cannot open it either

**Diagram to embed**: `architecture/designs/cryptographic-primitives/diagrams/key-derivation-recovery-slot.md`

**Source sections**:
- `architecture/designs/authentication-and-session-management/design.md` §Recovery Slot (all
  subsections), §New-Device Recovery Flow, §USB Key File Rotation

---

### `how-it-works/file-encryption.md` — How Files Are Encrypted

**Narrative angle**: What actually happens to a file from the moment you drop it into Arx Runa?
This is the core technical page — the one a security-minded person will read most carefully.

**Pitch level**: Walk through the pipeline step by step, but explain the *why* at each step.
Why chunks? Why padding to fixed size? Why EXIF stripping? Why a unique key per file? You can
name the algorithms (ChaCha20-Poly1305, BLAKE3) without explaining them — link to the Glossary
for definitions.

**Concepts to cover**:
- Pre-processing: EXIF metadata is stripped before encryption (GPS coordinates, camera model, etc.
  are removed so they can't leak even if somehow the ciphertext was cracked)
- Chunking: files split into fixed-size chunks (default 4 MiB) — enables streaming, partial
  download, and bounded memory use
- Padding: every chunk is padded to the exact same size before encryption — an observer watching
  the cloud cannot tell if a chunk contains 1 byte or 4 MiB
- Per-file key: each file gets a unique random encryption key, wrapped (encrypted) with the
  master key and stored in the manifest — rekeying one file doesn't touch others
- Chunk encryption: ChaCha20-Poly1305 AEAD (or AES-256-GCM-SIV) — the ciphertext includes an
  authentication tag so tampering is detected
- Nonce: each chunk gets a unique nonce derived from randomness + chunk position — reuse is
  prevented by construction
- BLAKE3 checksum: the encrypted blob is checksummed so corruption is detected on download
- Manifest: a SQLCipher database stores file paths, chunk IDs, wrapped keys — it's also
  encrypted and backed up to cloud

**Diagrams to embed**:
- `architecture/designs/cryptographic-primitives/diagrams/chunk-encryption-flow.md`
- `architecture/designs/chunking-and-manifest/diagrams/chunk-pipeline.md`

**Source sections**:
- `architecture/designs/cryptographic-primitives/design.md` §Cipher Selection, §Key Derivation,
  §Per-File Key Management, §Chunk Encryption and Decryption, §Nonce Generation, §BLAKE3 Checksum,
  §Security Considerations
- `architecture/designs/chunking-and-manifest/design.md` §Chunk Size, §Padding Scheme,
  §Pre-Encryption Processing: EXIF Stripping, §Encrypt Pipeline, §File Key Lifecycle

---

### `how-it-works/cloud-sync.md` — What the Cloud Sees

**Narrative angle**: The cloud is the most obvious threat model. This page is about what an
attacker who controls your cloud storage (or just has read access) can and cannot learn.

**Pitch level**: Explain the cloud layout — what files are actually stored there. Emphasize what
is NOT there (file names, directory structure, metadata, unencrypted content). Cover
multi-destination (backups to multiple clouds simultaneously). Explain rclone as the transport
layer — Arx Runa doesn't implement cloud auth from scratch.

**Concepts to cover**:
- Cloud layout: flat directory of UUID-named blobs + a vault header file — no folder structure,
  no file names, no timestamps that mean anything
- What the cloud provider sees: opaque fixed-size blobs, a vault header with encrypted parameters,
  nothing else
- Multi-destination: the same encrypted blobs can be pushed to multiple cloud providers
  simultaneously for redundancy
- Rclone: Arx Runa uses rclone as the transport layer — supports S3, B2, Google Drive, Dropbox,
  WebDAV, and ~40 others — credentials are never stored permanently (session-only)
- Vault header backup: the header (needed to re-derive keys on a new device) is also stored in
  cloud, encrypted — losing local data doesn't mean losing access

**Diagram to embed**: `architecture/designs/cloud-synchronisation/diagrams/cloud-sync-sequence.md`

**Source sections**:
- `architecture/designs/cloud-synchronisation/design.md` §Cloud Storage Layout, §Multi-Destination
  Model, §Vault Backup, §Destination Session Storage
- `architecture/designs/chunking-and-manifest/design.md` §Staging Directory (brief — explain
  that staging is local and temporary, never the cloud)

---

### `how-it-works/file-sharing.md` — Sharing Files Privately

**Narrative angle**: Sharing usually means giving someone a password or relying on a trusted
server. Arx Runa does neither. This page explains how you share a file with someone when
neither party trusts the cloud or each other's infrastructure.

**Pitch level**: Explain public-key cryptography at a conceptual level (you have a public key
your contact can encrypt to; only you can decrypt). Introduce HPKE as the construction used.
Explain the share package — what it contains and why the cloud hosting it can't read it. Cover
revocation limits honestly.

**Concepts to cover**:
- Identity: each user has an HPKE key pair (public + private). You share your public key with
  contacts out-of-band (e.g., paste it in a message) — no central identity server
- Share package: the sender wraps the file's encryption key inside an HPKE envelope addressed
  to the recipient's public key. Only the recipient's private key can open it.
- What the cloud hosts: the encrypted file blobs (which the recipient can download) + the HPKE
  envelope — the cloud sees neither the file content nor the file key
- Snapshot semantics: shares are a point-in-time snapshot — if the file changes after sharing,
  the recipient does not automatically get the update
- Revocation: if the recipient has not yet fetched the blobs, you can delete them from the
  cloud. If they have already fetched and decrypted — the data is on their machine and cannot
  be recalled (this is honest and correct, not a flaw)
- Expiration: shares can have an expiry date — after which the blobs are removed from cloud
- Fingerprint verification (future): out-of-band verification of public keys to prevent MITM
  on the key exchange

**Diagram to embed**: `architecture/designs/file-sharing/diagrams/file-sharing-flow.md`

**Source sections**:
- `architecture/designs/file-sharing/design.md` §Identity Model, §Key Architecture: Per-File Keys,
  §HPKE Construction, §Share Package Format, §Revocation, §Share Expiration, §Snapshot Semantics,
  §Threat Model Additions, §Fingerprint Verification

---

## Sequencing: Recommended Session Order

Each session = one page. The pages are relatively independent once the blueprint is loaded,
but conceptual dependencies exist:

1. `the-vault.md` — foundation, defines master key and key tree (write first)
2. `unlocking.md` — depends on vault concept
3. `recovery.md` — depends on vault + unlock concepts
4. `file-encryption.md` — depends on vault (per-file keys wrap to master key)
5. `cloud-sync.md` — depends on file encryption (blobs = encrypted chunks)
6. `file-sharing.md` — most independent, can be written any time after vault

Then one final session: update `SUMMARY.md` + create `how-it-works/README.md` + verify mdbook
builds cleanly.

---

## Tone and Style Guide for Writers

- Address the reader as "you" — "when you drop a file…", "your master key…"
- State security properties positively: "only you can decrypt" not "the attacker cannot decrypt"
- Name algorithms without apology (Argon2id, ChaCha20-Poly1305, HKDF, HPKE) — link to Glossary
- Keep each page under ~600 words of prose (diagrams carry a lot of weight)
- No bullet-point walls — prose with occasional lists for enumerable items
- No references to implementation phases, sub-phases, contract surfaces, or Rust signatures
- If a security property has a limit or caveat, state it honestly — this builds more trust than
  omitting it

---

## What to Tell a Future Session

> "Load docs/RESTRUCTURE.md. We are restructuring the Arx Runa book.
> Your task is to write [page name] as specified in the blueprint.
> Read the source sections listed for that page using jdocmunch (repo: local/arx-runa-docs),
> then write the new file at docs/how-it-works/[filename].md.
> Follow the tone guide. Do not touch SUMMARY.md — that comes last."
