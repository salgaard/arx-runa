# Glossary

Terms used consistently across Arx Runa documentation, use cases, and source code.

---

## Vault

The entire encrypted storage namespace for a single user. A vault is not a folder — it is the top-level container that groups all of a user's encrypted files under one set of authentication credentials.

In cloud storage (Google Drive, Backblaze B2, S3, etc.) a vault appears as a configured root directory containing:

```
<cloud-root>/
  vault-header.json        ← plaintext JSON (public parameters)
  manifest/
    manifest-backup.blob   ← encrypted manifest backup
  vault/
    <uuid>.blob            ← encrypted file chunks (flat, no structure)
  shared/                  ← reserved for file sharing (Phase 5)
```

The cloud provider sees blob count, uniform blob sizes, and access timing — never filenames, folder structure, or file contents.

---

## Vault Header

A plaintext JSON file stored at the cloud root (`vault-header.json`). It contains only public parameters needed to bootstrap key derivation on a new device:

- `vault_id` — UUID v4 identifying the vault
- `tier` — authentication tier selected at vault creation: `1` (password only) or `2` (password + USB key file)
- `argon2_salt` — 32-byte Argon2id salt (CSPRNG-generated at vault creation)
- `argon2_params` — Argon2id cost parameters (memory, iterations, parallelism)
- `key_file_blake3` — BLAKE3 fingerprint of the USB key file (Tier 2 only; `null` for Tier 1; preimage-resistant, does not reveal key material)

The vault header is intentionally unencrypted: it must be downloadable before any keys exist, so a new device can derive the correct keys without prior authentication.

---

## Manifest

The encrypted index of all files in a vault. The manifest is stored in a **SQLCipher database** locally (one database per device) and backed up to the cloud as `manifest/manifest-backup.blob`.

The manifest records, for each file:
- Filenames and directory structure
- Per-file `file_key_wrapped`: the file encryption key, wrapped with `key_encryption_key` (HKDF-derived from `master_key`); stored in the `nodes` table
- Chunk map: ordered list of UUID blob names and their sizes
- `snapshot_counter` for sync conflict detection

The local SQLCipher database is encrypted with `sqlcipher_key` (derived from `master_key` via HKDF). The cloud backup (`manifest-backup.blob`) is separately encrypted with `manifest_key` (also derived from `master_key` via HKDF). The cloud never sees manifest contents in plaintext.

---

## Blob

A single encrypted file chunk stored in the cloud. Blob names are UUID v4 strings with a `.blob` extension (e.g., `3f7a1b2c-dead-beef-cafe-112233445566.blob`). All blobs are padded to exactly 4 MiB, making them indistinguishable by size.

---

## Chunk

A 4 MiB fixed-size block produced when a file is split for encryption. The final chunk of a file is zero-padded to 4 MiB. Each chunk is encrypted independently with XChaCha20-Poly1305 and becomes one blob in the cloud.

The chunk size is fixed (not content-defined) to prevent file size inference from blob sizes.

---

## master_key

The root key material for a vault session, derived by Argon2id from the user's password (and USB key file bytes for Tier 2). All other vault keys are derived from `master_key` via HKDF-SHA256.

- **Tier 1**: `master_key = Argon2id(password, salt)`
- **Tier 2**: `master_key = Argon2id(password ‖ key_file_bytes, salt)`

`master_key` exists only in RAM during an active session and is zeroized on vault lock.

---

## file_key

A 32-byte random key generated per file at encryption time. Used as the actual XChaCha20-Poly1305 encryption key for that file's chunks. Stored encrypted inside the SQLCipher manifest (wrapped under `key_encryption_key`, HKDF-derived from `master_key`) as `file_key_wrapped` in the `nodes` table.

---

## sqlcipher_key

A vault-level key derived from `master_key` via HKDF, used to encrypt the local SQLCipher manifest database.

---

## USB Key File

A 32-byte file of cryptographically random entropy stored on a physical USB drive. Used as the hardware second factor in Tier 2 authentication. The key file is identified by its BLAKE3 fingerprint stored in the vault header — the filename is irrelevant.

Losing the USB key file without a backup means permanent loss of access to Tier 2 vaults. See [UC-IND-003](../use-cases/UC-IND-003-hardware-mfa-and-key-loss.md).

---

## Tier 1 / Tier 2

Authentication tiers selected when creating a vault:

| Tier | Factors | Key derivation |
|------|---------|---------------|
| **Tier 1** | Password only | `Argon2id(password, salt)` |
| **Tier 2** | Password + USB key file | `Argon2id(password ‖ key_file_bytes, salt)` |

Both tiers are zero-knowledge — the cloud provider never holds key material. Tier 2 additionally requires physical possession of the USB key file on every access.

---

## BYOC (Bring Your Own Cloud)

Arx Runa's cloud-agnostic storage model. Users configure any Rclone-supported backend (Google Drive, Backblaze B2, Amazon S3, Azure Blob, self-hosted MinIO, etc.) as their vault's cloud storage. Arx Runa does not operate its own storage infrastructure.

---

## Rclone

An open-source command-line tool that manages file synchronization and transfer to cloud storage backends. Arx Runa uses Rclone as its cloud transport layer to achieve Bring Your Own Cloud (BYOC) compatibility. Rclone supports 70+ storage providers including Google Drive, S3, Backblaze B2, Dropbox, Azure Blob, and self-hosted solutions like MinIO.

Arx Runa invokes Rclone programmatically to upload and download encrypted blobs, treating it as a storage-agnostic abstraction. Users configure their chosen backend via Rclone's standard configuration file (`rclone.conf`), and Arx Runa never handles cloud provider credentials directly.

See [Rclone official documentation](https://rclone.org) for configuration guides and supported backends.

---

## Zero-Trace

The principle that Arx Runa leaves no plaintext artifacts on the host machine during a session. Decrypted file content is held in RAM only, never written to disk as temp files, thumbnails, or OS caches. When the vault is locked, no recoverable plaintext remains on the device.

---

## snapshot_counter

An integer stored in the manifest that increments on every push. Used to detect whether the local manifest is out of date relative to the cloud backup. If the local `snapshot_counter` is less than the cloud's, a pull is required before pushing changes.

---

## AAD (Additional Authenticated Data)

The binding value included in every XChaCha20-Poly1305 encryption call: `file_id ‖ chunk_index`. AAD prevents chunk-swap attacks — a chunk from one file cannot be spliced into another file's chunk sequence without causing an authentication failure.

---

## File Sharing Key

There is no separate `share_key` in Arx Runa. When a file is shared, the existing `file_key` is re-encrypted inside an ECIES envelope (using an ECDH-derived symmetric key) addressed to the recipient's X25519 public key. The resulting `file_key_wrapped` is included in the share package. Share metadata is stored in the `shares` table (sender side) and `received_shares` table (recipient side) of the SQLCipher database. See [UC-IND-004](../use-cases/use-case-4-personal-file-sharing.md).
