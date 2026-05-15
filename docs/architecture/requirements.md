# Functional Requirements

This document captures the functional requirements for Arx Runa — the statements of what the
system **must** do. It sits between the use-cases (what users do end-to-end) and the design
documents (how we implement each capability).

## Traceability model

```
Use Case  →  Requirement  →  Design
(user flow)   (system must…)   (how we build it)
```

Each requirement carries:

- **Source** — the use-case(s) and/or design(s) from which it was extracted.
  - `UC-N` = extracted forward from use-case N.
  - `D-<area>` = extracted backward from a design's Contract Surface.
- **Design** — the primary design document that governs implementation of this requirement.

## Requirement ID scheme

| Prefix | Domain |
|--------|--------|
| `REQ-AUTH` | Authentication & Session Management |
| `REQ-CRYPTO` | Cryptographic Primitives |
| `REQ-VAULT` | Vault Storage & Manifest |
| `REQ-SYNC` | Cloud Synchronisation |
| `REQ-SHARE` | File Sharing |
| `REQ-UI` | Frontend & User Interface |

---

## REQ-AUTH — Authentication & Session Management

### REQ-AUTH-001: Vault creation tier selection

The system must offer Tier 1 (password-only) and Tier 2 (password + USB key) at vault creation,
with Tier 2 as the default.

**Source**: UC-1, UC-3  
**Design**: [Authentication & Session Management](designs/authentication-and-session-management/design.md)

---

### REQ-AUTH-002: Tier 1 unlock

The system must accept a correct password as the sole unlock factor for Tier 1 vaults.

**Source**: UC-1, UC-2, UC-3  
**Design**: [Authentication & Session Management](designs/authentication-and-session-management/design.md)

---

### REQ-AUTH-003: Tier 2 dual-factor requirement

The system must require both the correct password and the correct USB key file to unlock a Tier 2 vault.

**Source**: UC-1, UC-2, UC-3  
**Design**: [Authentication & Session Management](designs/authentication-and-session-management/design.md)

---

### REQ-AUTH-004: Tier 2 password-alone rejection

The system must reject an unlock attempt on a Tier 2 vault if the USB key file is absent, even
when the password is correct.

**Source**: UC-3  
**Design**: [Authentication & Session Management](designs/authentication-and-session-management/design.md)

---

### REQ-AUTH-005: Tier 2 USB-alone rejection

The system must reject an unlock attempt on a Tier 2 vault if the password is absent or incorrect,
even when the USB key file is present.

**Source**: UC-3  
**Design**: [Authentication & Session Management](designs/authentication-and-session-management/design.md)

---

### REQ-AUTH-006: Non-oracular authentication failure

The authentication error returned to the caller must not distinguish a wrong password from a wrong
key file — both cases return the same `InvalidCredentials` result.

**Source**: D-auth  
**Design**: [Authentication & Session Management](designs/authentication-and-session-management/design.md)

---

### REQ-AUTH-007: Tier 2 Argon2id input construction

The system must construct the Argon2id input for Tier 2 by concatenating password bytes and key
file bytes (`password_bytes || key_file_bytes`), not by hashing or salting them separately.

**Source**: D-auth  
**Design**: [Authentication & Session Management](designs/authentication-and-session-management/design.md)

---

### REQ-AUTH-008: USB key file byte format

A USB key file must consist of exactly 32 bytes of raw CSPRNG entropy with no imposed filename or
internal structure.

**Source**: D-auth  
**Design**: [Authentication & Session Management](designs/authentication-and-session-management/design.md)

---

### REQ-AUTH-009: Argon2id parameter immutability

Argon2id parameters (`m=65536`, `t=3`, `p=4`) must be fixed at vault creation, stored in the vault
header, and cached locally for downgrade-resistant validation. They must not change for the lifetime
of a vault.

**Source**: D-auth  
**Design**: [Authentication & Session Management](designs/authentication-and-session-management/design.md)

---

### REQ-AUTH-010: USB key auto-detection

The system must automatically detect a compatible USB key file by scanning removable drives for
32-byte files whose BLAKE3 fingerprint matches the value stored in the vault header.

**Source**: UC-3, D-auth  
**Design**: [Authentication & Session Management](designs/authentication-and-session-management/design.md)

---

### REQ-AUTH-011: Key file path hint caching

The system must cache the last-used key file path in a local hint file to accelerate re-detection,
and fall back to a full removable-drive scan when the hint does not resolve.

**Source**: D-auth  
**Design**: [Authentication & Session Management](designs/authentication-and-session-management/design.md)

---

### REQ-AUTH-012: OS-native USB device monitoring

The system must monitor USB device arrival and removal using OS-native APIs: Windows
(`RegisterDeviceNotification`/WMI), Linux (udev), macOS (FSEvents).

**Source**: D-auth  
**Design**: [Authentication & Session Management](designs/authentication-and-session-management/design.md)

---

### REQ-AUTH-013: Offline authentication

Authentication must be fully offline — no internet connection may be required after the first
vault-header download.

**Source**: UC-3  
**Design**: [Authentication & Session Management](designs/authentication-and-session-management/design.md)

---

### REQ-AUTH-014: Memory-locked session keys

Session keys must be held in memory-locked (`mlock`d) RAM and zeroed immediately on vault lock or
session timeout.

**Source**: UC-1, UC-2, UC-3, D-auth  
**Design**: [Authentication & Session Management](designs/authentication-and-session-management/design.md)

---

### REQ-AUTH-015: Plaintext vault header in cloud

The vault header must be stored as plaintext JSON at the cloud root so it is readable before
authentication, enabling new-device bootstrap.

**Source**: D-auth, D-sync  
**Design**: [Authentication & Session Management](designs/authentication-and-session-management/design.md)

---

### REQ-AUTH-016: Pre-authentication bootstrap files

The bootstrap files (`cloud-config.json`, `local-vault-params.json`) must be readable before the
vault is authenticated.

**Source**: D-sync  
**Design**: [Cloud Synchronisation](designs/cloud-synchronisation/design.md)

---

### REQ-AUTH-017: Recovery phrase unlocks any tier

A BIP-39 recovery phrase (24 words, 256-bit entropy) must unlock the vault regardless of
authentication tier, when one has been configured.

**Source**: UC-3  
**Design**: [Authentication & Session Management](designs/authentication-and-session-management/design.md)

---

### REQ-AUTH-018: Recovery phrase display policy

The recovery phrase must be displayed to the user exactly once during setup and then zeroed from
memory. The system must never store the phrase.

**Source**: UC-3  
**Design**: [Authentication & Session Management](designs/authentication-and-session-management/design.md)

---

### REQ-AUTH-019: Credential reset after recovery

After recovery phrase unlock, the system must require the user to set new primary credentials
before the vault becomes fully operational.

**Source**: UC-3  
**Design**: [Authentication & Session Management](designs/authentication-and-session-management/design.md)

---

### REQ-AUTH-020: Recovery slot survives credential rotation

The recovery slot must remain valid across password changes and USB key rotations when the recovery
phrase is supplied during the ceremony.

**Source**: UC-3  
**Design**: [Authentication & Session Management](designs/authentication-and-session-management/design.md)

---

### REQ-AUTH-021: No external recovery mechanism

No cloud-based, third-party, or admin-override recovery mechanism may exist. Self-sovereignty is
absolute.

**Source**: UC-3  
**Design**: [Authentication & Session Management](designs/authentication-and-session-management/design.md)

---

### REQ-AUTH-022: Password change without blob re-encryption

Password change must re-wrap all internal keys under the new `master_key` without re-encrypting
any cloud blobs.

**Source**: UC-3  
**Design**: [Authentication & Session Management](designs/authentication-and-session-management/design.md)

---

### REQ-AUTH-023: USB key rotation without blob re-encryption

USB key rotation must re-wrap internal keys under the new `master_key` without re-encrypting any
cloud blobs.

**Source**: UC-3  
**Design**: [Authentication & Session Management](designs/authentication-and-session-management/design.md)

---

## REQ-CRYPTO — Cryptographic Primitives

### REQ-CRYPTO-001: Cipher selection

XChaCha20-Poly1305 is the only permitted symmetric cipher. AES-GCM and ChaCha20-Poly1305 with
96-bit nonces are explicitly excluded.

**Source**: D-crypto  
**Design**: [Cryptographic Primitives](designs/cryptographic-primitives/design.md)

---

### REQ-CRYPTO-002: Nonce size

All nonces must be 24-byte values generated by a CSPRNG at encryption time.

**Source**: D-crypto  
**Design**: [Cryptographic Primitives](designs/cryptographic-primitives/design.md)

---

### REQ-CRYPTO-003: master_key zeroization

`master_key` must be zeroed immediately after HKDF key derivation is complete. It must never be
stored in persistent storage or emitted to any log.

**Source**: D-crypto, D-auth  
**Design**: [Cryptographic Primitives](designs/cryptographic-primitives/design.md)

---

### REQ-CRYPTO-004: HKDF-derived vault keys

Three vault-level keys must be derived from `master_key` via HKDF-SHA256 with distinct info
strings: `key_encryption_key` (`"arx-runa-key-encryption"`), `sqlcipher_key`
(`"arx-runa-sqlcipher"`), and `manifest_key` (`"arx-runa-manifest-backup"`).

**Source**: D-crypto  
**Design**: [Cryptographic Primitives](designs/cryptographic-primitives/design.md)

---

### REQ-CRYPTO-005: Per-file key uniqueness

Each file must be encrypted with a unique per-file key generated by a CSPRNG at file creation
time.

**Source**: UC-1, UC-2, D-crypto  
**Design**: [Cryptographic Primitives](designs/cryptographic-primitives/design.md)

---

### REQ-CRYPTO-006: Per-file key isolation

Sharing a file must expose only that file's `file_key`. Vault-level keys (`key_encryption_key`,
`sqlcipher_key`, `manifest_key`) must never be exposed to any share recipient.

**Source**: UC-4, D-share  
**Design**: [Cryptographic Primitives](designs/cryptographic-primitives/design.md)

---

### REQ-CRYPTO-007: Chunk wire format

The on-wire format for every encrypted chunk must be: `[24-byte nonce | ciphertext | 16-byte AEAD
tag]`.

**Source**: D-crypto  
**Design**: [Cryptographic Primitives](designs/cryptographic-primitives/design.md)

---

### REQ-CRYPTO-008: Wrapped key wire format

The on-wire format for every wrapped key must be: `[24-byte nonce | 32-byte ciphertext | 16-byte
AEAD tag]`.

**Source**: D-crypto  
**Design**: [Cryptographic Primitives](designs/cryptographic-primitives/design.md)

---

### REQ-CRYPTO-009: Chunk AAD binding

Every chunk encrypt and decrypt operation must include an AAD of `file_id || chunk_index` (big-endian
`u32`) so each chunk is cryptographically bound to its file identity and position.

**Source**: UC-1, D-crypto  
**Design**: [Cryptographic Primitives](designs/cryptographic-primitives/design.md)

---

### REQ-CRYPTO-010: Verify before decrypt

BLAKE3 checksum verification must occur before decryption on every read path (VerifiedBlob
pattern). A checksum mismatch must abort the operation.

**Source**: D-crypto  
**Design**: [Cryptographic Primitives](designs/cryptographic-primitives/design.md)

---

### REQ-CRYPTO-011: AEAD tamper detection

The AEAD authentication tag must cause decryption to fail if any byte of the ciphertext or AAD has
been modified.

**Source**: UC-1  
**Design**: [Cryptographic Primitives](designs/cryptographic-primitives/design.md)

---

### REQ-CRYPTO-012: EXIF stripping

For media files, EXIF metadata (GPS coordinates, camera model, timestamps) must be stripped or
encrypted before the file data is encrypted.

**Source**: UC-1  
**Design**: [Cryptographic Primitives](designs/cryptographic-primitives/design.md)

---

### REQ-CRYPTO-013: Recovery key derivation

The BIP-39 recovery key must be derived from the 24-word entropy via Argon2id.

**Source**: UC-3  
**Design**: [Authentication & Session Management](designs/authentication-and-session-management/design.md)

---

### REQ-CRYPTO-014: Wrapped master_key for recovery

The wrapped `master_key` stored for recovery must be decryptable using only the `recovery_key`
derived from the phrase — no other vault credential may be required.

**Source**: UC-3  
**Design**: [Authentication & Session Management](designs/authentication-and-session-management/design.md)

---

### REQ-CRYPTO-015: USB key file BLAKE3 fingerprint

The BLAKE3 fingerprint of the USB key file must be stored in the vault header to enable
deterministic device-scan matching.

**Source**: UC-3, D-auth  
**Design**: [Authentication & Session Management](designs/authentication-and-session-management/design.md)

---

### REQ-CRYPTO-016: HPKE ciphersuite for sharing

Share packages must use HPKE RFC 9180 with ciphersuite `DHKEM(X25519, HKDF-SHA256) + HKDF-SHA256
+ CTX-ChaCha20-Poly1305`. The CTX variant replaces the 16-byte Poly1305 tag with a 32-byte BLAKE3
commitment tag for CMT-4 binding security.

**Source**: UC-4, D-share  
**Design**: [File Sharing](designs/file-sharing/design.md)

---

### REQ-CRYPTO-017: Receipt encryption

Download receipts must be encrypted with the owner's X25519 public key via HPKE (same
construction as share packages), so only the owner can decrypt them.

**Source**: D-share  
**Design**: [File Sharing](designs/file-sharing/design.md)

---

## REQ-VAULT — Vault Storage & Manifest

### REQ-VAULT-001: Opaque cloud blobs

The cloud must receive only opaque ciphertext blobs identified by random UUID v4 filenames. No
original filenames, file sizes, directory structure, or metadata may be inferrable from cloud
content.

**Source**: UC-1, UC-2, UC-4, UC-5  
**Design**: [Chunking & Manifest](designs/chunking-and-manifest/design.md)

---

### REQ-VAULT-002: Immutable chunk size

File data must be split into fixed-size chunks. The chunk size (range 128 KiB–64 MiB, default
4 MiB) is set at vault creation and must not change for the lifetime of the vault.

**Source**: UC-1, D-manifest  
**Design**: [Chunking & Manifest](designs/chunking-and-manifest/design.md)

---

### REQ-VAULT-003: Chunk zero-padding

Every chunk must be zero-padded to exactly `chunk_size_bytes` before encryption. On decryption,
content is truncated to `size_bytes` from the manifest entry.

**Source**: D-manifest  
**Design**: [Chunking & Manifest](designs/chunking-and-manifest/design.md)

---

### REQ-VAULT-004: Streaming chunk processing

File I/O must be streaming — at most one chunk plaintext buffer may be held in memory at any
time.

**Source**: D-manifest  
**Design**: [Chunking & Manifest](designs/chunking-and-manifest/design.md)

---

### REQ-VAULT-005: Epoch buffer (opt-in)

`epoch_buffer_enabled` is an opt-in vault setting (default off). When enabled, files smaller than
`chunk_size_bytes` are staged locally and packed before upload; files at or above `chunk_size_bytes`
upload immediately.

**Source**: D-manifest  
**Design**: [Chunking & Manifest](designs/chunking-and-manifest/design.md)

---

### REQ-VAULT-006: Monotonic snapshot counter

The vault manifest must include a monotonic `snapshot_counter` that advances exclusively via
`increment_snapshot_counter`. No other code path may write to it.

**Source**: UC-2, D-manifest  
**Design**: [Chunking & Manifest](designs/chunking-and-manifest/design.md)

---

### REQ-VAULT-007: Divergence detection before push

The system must check that the local `snapshot_counter` equals the cloud value before any push
operation. A stale local counter must abort the push.

**Source**: UC-2, D-sync  
**Design**: [Cloud Synchronisation](designs/cloud-synchronisation/design.md)

---

### REQ-VAULT-008: Conflict renaming

When a push conflict is detected, pending local files whose names collide with cloud entries must
be automatically renamed with a `(conflicted copy)` suffix before merging.

**Source**: UC-2  
**Design**: [Chunking & Manifest](designs/chunking-and-manifest/design.md)

---

### REQ-VAULT-009: Zero-Trace decryption

Decrypted file content must be held entirely in memory for in-app display. No plaintext may be
written to disk unless the user explicitly initiates an export.

**Source**: UC-1, UC-2  
**Design**: [Tauri IPC & Frontend](designs/tauri-ipc-and-frontend/design.md)

---

### REQ-VAULT-010: Export warning

When a user initiates an export, the system must warn that the exported copy will be plaintext
outside vault protection before completing the export.

**Source**: UC-1, UC-2, UC-4  
**Design**: [Tauri IPC & Frontend](designs/tauri-ipc-and-frontend/design.md)

---

### REQ-VAULT-011: Directory hierarchy invariant

For any insert or move operation: the target parent must be a directory, a node may not be its own
parent, and moves must not create a directory cycle.

**Source**: D-manifest  
**Design**: [Chunking & Manifest](designs/chunking-and-manifest/design.md)

---

### REQ-VAULT-012: SQLCipher key stack policy

The SQLCipher key must not be copied as a raw value on the stack during vault open, create, or
re-keying operations.

**Source**: D-manifest  
**Design**: [Chunking & Manifest](designs/chunking-and-manifest/design.md)

---

### REQ-VAULT-013: Zero-byte file handling

A zero-byte file must produce a `nodes` row with `size_bytes = 0` and a freshly generated
`file_key_wrapped`. No chunks are written, but the file key is generated for use in future updates
or shares.

**Source**: D-manifest  
**Design**: [Chunking & Manifest](designs/chunking-and-manifest/design.md)

---

### REQ-VAULT-014: Per-vault destination scope

Destination records (cloud credentials and configuration) must be scoped to a single vault and
stored in its SQLCipher database. No global credential store may be shared across vaults.

**Source**: D-sync  
**Design**: [Cloud Synchronisation](designs/cloud-synchronisation/design.md)

---

### REQ-VAULT-015: Immutable manifest meta keys

The manifest meta keys (`schema_version`, `vault_id`, `chunk_size_bytes`, `epoch_buffer_enabled`)
must be read-only after vault creation and must not be modifiable via `set_meta`.

**Source**: D-manifest  
**Design**: [Chunking & Manifest](designs/chunking-and-manifest/design.md)

---

## REQ-SYNC — Cloud Synchronisation

### REQ-SYNC-001: Rclone sidecar transport

Rclone must be the sole concrete cloud transport implementation, bundled as a Tauri sidecar binary.
No cloud provider SDK may be linked as a crate dependency.

**Source**: D-sync  
**Design**: [Cloud Synchronisation](designs/cloud-synchronisation/design.md)

---

### REQ-SYNC-002: Remote path sanitisation

All remote paths constructed for blob operations must be sanitised — no path traversal sequences
and no absolute-path escapes are permitted.

**Source**: D-sync  
**Design**: [Cloud Synchronisation](designs/cloud-synchronisation/design.md)

---

### REQ-SYNC-003: Idempotent blob operations

`upload_blob` and `delete_blob` must be idempotent — repeated calls with the same arguments must
be safe to retry without side effects.

**Source**: D-sync  
**Design**: [Cloud Synchronisation](designs/cloud-synchronisation/design.md)

---

### REQ-SYNC-004: Rclone stderr sanitisation

Rclone stderr output must be sanitised before appearing in any user-facing error message, with
credential-related lines stripped.

**Source**: D-sync  
**Design**: [Cloud Synchronisation](designs/cloud-synchronisation/design.md)

---

### REQ-SYNC-005: HTTPS-only cloud endpoints

Cloud endpoint URLs must use HTTPS by default. HTTP is rejected unless explicitly overridden for
local development on loopback addresses.

**Source**: D-sync  
**Design**: [Cloud Synchronisation](designs/cloud-synchronisation/design.md)

---

### REQ-SYNC-006: Destination probe before save

`add_destination` must validate connectivity with an rclone probe (e.g., `lsd`) before persisting
credentials. An unreachable or misconfigured destination must be rejected.

**Source**: D-ipc  
**Design**: [Tauri IPC & Frontend](designs/tauri-ipc-and-frontend/design.md)

---

### REQ-SYNC-007: Explicit sync only

Sync (push and pull) must be triggered explicitly by the user. No background sync daemon or
scheduled sync may run without user action.

**Source**: UC-2  
**Design**: [Cloud Synchronisation](designs/cloud-synchronisation/design.md)

---

### REQ-SYNC-008: Pull-before-push enforcement

When a push is attempted and the cloud manifest counter is ahead of the local counter, the system
must prompt the user to pull first and must not proceed with the push.

**Source**: UC-2  
**Design**: [Cloud Synchronisation](designs/cloud-synchronisation/design.md)

---

### REQ-SYNC-009: Offline upload queuing

When the cloud is unavailable, completed local encryption must be preserved and the upload queued
for retry on the next sync attempt.

**Source**: UC-1, UC-5  
**Design**: [Cloud Synchronisation](designs/cloud-synchronisation/design.md)

---

### REQ-SYNC-010: Multi-destination push

A single push operation must upload identical encrypted blobs to every active destination.

**Source**: UC-5  
**Design**: [Cloud Synchronisation](designs/cloud-synchronisation/design.md)

---

### REQ-SYNC-011: Mirror destination deletion semantics

On sync, a mirror destination must reflect the current vault state — blobs for locally deleted
files must be removed from mirror destinations.

**Source**: UC-5  
**Design**: [Cloud Synchronisation](designs/cloud-synchronisation/design.md)

---

### REQ-SYNC-012: Accumulating destination retention

An accumulating destination must retain blobs even when the corresponding file is deleted from the
vault locally.

**Source**: UC-5  
**Design**: [Cloud Synchronisation](designs/cloud-synchronisation/design.md)

---

### REQ-SYNC-013: Single primary destination

Exactly one destination must be marked primary at all times. The primary destination cannot be
deleted while any other destinations remain without first promoting one as the new primary.

**Source**: UC-5  
**Design**: [Cloud Synchronisation](designs/cloud-synchronisation/design.md)

---

### REQ-SYNC-014: Per-destination failure reporting

Backup sync failures must be reported per-destination and cleared automatically after the next
successful sync to that destination.

**Source**: UC-5  
**Design**: [Cloud Synchronisation](designs/cloud-synchronisation/design.md)

---

### REQ-SYNC-015: Provider-agnostic migration

Cloud provider migration must not require re-encryption. The same opaque ciphertext blobs must be
usable on any supported provider without modification.

**Source**: UC-1, UC-5  
**Design**: [Cloud Synchronisation](designs/cloud-synchronisation/design.md)

---

## REQ-SHARE — File Sharing

### REQ-SHARE-001: X25519 identity keypair

The system must generate an X25519 identity keypair on first run. The private key must be stored in
SQLCipher wrapped with `key_encryption_key`.

**Source**: D-share  
**Design**: [File Sharing](designs/file-sharing/design.md)

---

### REQ-SHARE-002: Out-of-band public key exchange

Public key exchange between sender and recipient is out-of-band. Arx Runa must not provide a key
server, email delivery, or any centralised exchange mechanism.

**Source**: D-share  
**Design**: [File Sharing](designs/file-sharing/design.md)

---

### REQ-SHARE-003: Cloud provider cannot read shared content

The cloud provider must not receive or be able to reconstruct file content or file keys during a
share operation.

**Source**: UC-4  
**Design**: [File Sharing](designs/file-sharing/design.md)

---

### REQ-SHARE-004: Recipient-key exclusivity

Only the holder of the recipient's X25519 private key must be able to decrypt the shared file.

**Source**: UC-4  
**Design**: [File Sharing](designs/file-sharing/design.md)

---

### REQ-SHARE-005: Snapshot share semantics

A share is a snapshot of the file at share time. Updates to the original file must not propagate
automatically to existing share recipients.

**Source**: D-share  
**Design**: [File Sharing](designs/file-sharing/design.md)

---

### REQ-SHARE-006: Publicly readable shared blobs

Blobs under `shared/<file_share_id>/` must be publicly readable on the cloud so the recipient does
not need the sender's credentials. Recipient identity must not appear in the share package or blob
paths.

**Source**: D-share  
**Design**: [File Sharing](designs/file-sharing/design.md)

---

### REQ-SHARE-007: Recipient vault independence

The recipient must not require access to the sender's vault or authentication factors at any stage
of downloading or decrypting a share.

**Source**: UC-4  
**Design**: [File Sharing](designs/file-sharing/design.md)

---

### REQ-SHARE-008: Sender-initiated revocation

The sender must be able to revoke a share at any time. Revocation must delete the shared blobs
from the cloud, making the encrypted content inaccessible to recipients who have not yet downloaded
it.

**Source**: UC-4  
**Design**: [File Sharing](designs/file-sharing/design.md)

---

### REQ-SHARE-009: Honest revocation limitation

The system must inform the sender that revocation cannot retract plaintext that a recipient has
already downloaded and decrypted.

**Source**: D-share  
**Design**: [File Sharing](designs/file-sharing/design.md)

---

### REQ-SHARE-010: Share expiration — owner-side enforcement

When a share has an `expires_at` timestamp, the owner's system must delete the shared blobs from
the cloud on the next push or sync after expiry, independently of any recipient-side behaviour.

**Source**: UC-4, D-share  
**Design**: [File Sharing](designs/file-sharing/design.md)

---

### REQ-SHARE-011: Share expiration — recipient-side enforcement

When a share has an `expires_at` timestamp, the recipient's system must refuse to decrypt the
share after expiry and display an appropriate message.

**Source**: D-share  
**Design**: [File Sharing](designs/file-sharing/design.md)

---

### REQ-SHARE-012: Download receipts

After successfully downloading all chunks of a share, the recipient's system must write a receipt
blob encrypted to the sender's public key under `shared/<file_share_id>/receipts/`. Receipts are
cooperative and informational — they are not a security control.

**Source**: UC-4, D-share  
**Design**: [File Sharing](designs/file-sharing/design.md)

---

### REQ-SHARE-013: Download notification

The sender must be notified of a recipient's download on the next pull or sync operation.

**Source**: UC-4  
**Design**: [File Sharing](designs/file-sharing/design.md)

---

### REQ-SHARE-014: Cloud provider cannot identify recipient

The cloud provider must not be able to identify the intended recipient from share metadata, blob
paths, or any other observable cloud artifact.

**Source**: UC-4  
**Design**: [File Sharing](designs/file-sharing/design.md)

---

## REQ-UI — Frontend & User Interface

### REQ-UI-001: IPC response sanitisation

IPC command responses must never expose key material, passwords, stack traces, or unsanitised
internal filesystem paths to the frontend or any log.

**Source**: D-ipc  
**Design**: [Tauri IPC & Frontend](designs/tauri-ipc-and-frontend/design.md)

---

### REQ-UI-002: Zero-Trace frontend state

The frontend must use no `localStorage` or persistent client-side storage. All sensitive UI state
must be cleared when the vault is locked or the session times out.

**Source**: D-ipc  
**Design**: [Tauri IPC & Frontend](designs/tauri-ipc-and-frontend/design.md)

---

### REQ-UI-003: IPC password zeroization

Password strings received over IPC must be immediately converted to `Zeroizing<Vec<u8>>`, the
original `String` bytes scrubbed, and the `String` dropped — before any other processing.

**Source**: D-ipc  
**Design**: [Tauri IPC & Frontend](designs/tauri-ipc-and-frontend/design.md)

---

### REQ-UI-004: Streaming progress for long operations

Long-running operations (file upload, download, sync, vault migration, backup sync) must stream
real-time progress updates to the UI via `tauri::ipc::Channel`.

**Source**: D-ipc  
**Design**: [Tauri IPC & Frontend](designs/tauri-ipc-and-frontend/design.md)

---

### REQ-UI-005: Build-time IPC allowlist

The set of exposed IPC commands must be enforced at build time via the `AppManifest::commands`
allowlist in `build.rs` and Tauri capabilities files. Commands not in the allowlist must not be
callable from the frontend.

**Source**: D-ipc  
**Design**: [Tauri IPC & Frontend](designs/tauri-ipc-and-frontend/design.md)

---

### REQ-UI-006: Upload via drop zone and file picker

The drop zone must be the primary file upload interface. A file picker button must also be
available as an equivalent entry point.

**Source**: UC-1  
**Design**: [Tauri IPC & Frontend](designs/tauri-ipc-and-frontend/design.md)

---

### REQ-UI-007: Tier selection at vault creation

The authentication tier must be selectable at vault creation time. Tier 2 (password + USB key)
must be pre-selected as the default.

**Source**: UC-1, UC-3  
**Design**: [Tauri IPC & Frontend](designs/tauri-ipc-and-frontend/design.md)

---

### REQ-UI-008: Vault deletion confirmation

Vault deletion must require the user to type the vault name as explicit confirmation before the
operation proceeds.

**Source**: D-ipc  
**Design**: [Tauri IPC & Frontend](designs/tauri-ipc-and-frontend/design.md)

---

### REQ-UI-009: Chunk size selector at vault creation

The chunk size must be configurable in the vault creation UI within the range 128 KiB–64 MiB. The
UI must pre-fill 4 MiB as the default.

**Source**: D-ipc  
**Design**: [Tauri IPC & Frontend](designs/tauri-ipc-and-frontend/design.md)

---

### REQ-UI-010: In-app viewing size limit

In-app file viewing (Zero-Trace) must be available for files up to 50 MiB. Files above 50 MiB
must prompt the user to use the export/download path instead.

**Source**: D-ipc  
**Design**: [Tauri IPC & Frontend](designs/tauri-ipc-and-frontend/design.md)

---

### REQ-UI-011: Sync pending indicator

When the cloud is unreachable or a sync is queued, the system must display a visible "Sync
pending" status to the user.

> **Not yet implemented.** The epoch buffer (`DEFAULT_EPOCH_BUFFER_ENABLED`) is disabled by
> default. All uploads take the `Immediate` route so `pending_changes` is always 0 and the
> sync badge never appears. This requirement is deferred until the epoch buffer is enabled.

**Source**: UC-1  
**Design**: [Tauri IPC & Frontend](designs/tauri-ipc-and-frontend/design.md)

---

### REQ-UI-012: Stale manifest banner

When the local manifest is known to be stale (cloud counter ahead), the system must display a
persistent "Working with stale manifest" banner until the manifest is refreshed.

**Source**: UC-2  
**Design**: [Tauri IPC & Frontend](designs/tauri-ipc-and-frontend/design.md)

---

### REQ-UI-013: USB key not found message

When a Tier 2 vault unlock is attempted and the USB key file is not detected, the system must
display a clear "Key file not found — insert USB drive" message.

**Source**: UC-2, UC-3  
**Design**: [Tauri IPC & Frontend](designs/tauri-ipc-and-frontend/design.md)

---

### REQ-UI-014: Shared with Me view

Files shared with the user by others must appear in a distinct "Shared with Me" view, separate
from the user's own vault files.

**Source**: UC-4  
**Design**: [Tauri IPC & Frontend](designs/tauri-ipc-and-frontend/design.md)

---

### REQ-UI-015: Share package import

Share packages must be importable via a file picker and via the `arx-runa://share-import` deep
link (custom URI scheme).

**Source**: D-share, D-ipc  
**Design**: [File Sharing](designs/file-sharing/design.md)

---

### REQ-UI-016: Per-destination failure badge

Backup sync failures must be surfaced as a per-destination badge in the destinations list and
cleared automatically after the next successful sync.

**Source**: UC-5  
**Design**: [Tauri IPC & Frontend](designs/tauri-ipc-and-frontend/design.md)

---

### REQ-UI-017: Primary destination visual distinction

The primary destination must be visually distinguished in the destinations list. The delete action
on a primary destination must be blocked until another destination is promoted to primary.

**Source**: UC-5  
**Design**: [Tauri IPC & Frontend](designs/tauri-ipc-and-frontend/design.md)
