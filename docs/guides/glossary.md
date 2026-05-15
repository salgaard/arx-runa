# Glossary

Terms used consistently across Arx Runa documentation, use cases, and source code.

---

## Vault

The entire encrypted storage namespace for a single user. A vault is not a folder - it is the top-level container that groups all of a user's encrypted files under one set of authentication credentials.

In cloud storage (Google Drive, Backblaze B2, S3, etc.) a vault appears as a configured root directory containing:

```
<cloud-root>/
  vault-header.json        <- plaintext JSON (public parameters)
  manifest/
    manifest-backup.blob   <- encrypted manifest backup
  vault/
    <uuid>.blob            <- encrypted file chunks (flat, no structure)
  shared/                  <- reserved for file sharing (Phase 5)
```

The cloud provider sees blob count, uniform blob sizes, and access timing - never filenames, folder structure, or file contents.

---

## Vault Header

A plaintext JSON file stored at the cloud root (`vault-header.json`). It contains only public parameters needed to bootstrap key derivation on a new device:

- `vault_id` - UUID v4 identifying the vault
- `tier` - authentication tier selected at vault creation: `1` (password only) or `2` (password + USB key file)
- `argon2_salt` - 32-byte Argon2id salt (CSPRNG-generated at vault creation)
- `argon2_params` - Argon2id cost parameters (memory, iterations, parallelism)
- `key_file_blake3` - BLAKE3 fingerprint of the USB key file (Tier 2 only; `null` for Tier 1; preimage-resistant, does not reveal key material)
- `recovery_slot` - optional `wrapped_master_key` for recovery-phrase access

The vault header is intentionally unencrypted: it must be downloadable before any keys exist, so a new device can derive the correct keys without prior authentication.

---

## local-vault-params.json

A trusted local cache of vault-header KDF parameters stored in app data. It contains `vault_id`, `argon2_salt`, and `argon2_params`.

On existing devices, downloaded `vault-header.json` values must match this cache exactly. This blocks parameter downgrade and salt-swap attacks. The file is written at vault creation and updated after successful password or key-file rotation.

---

## Manifest

The encrypted index of all files in a vault. The manifest is stored in a **SQLCipher database** locally (one database per device) and backed up to the cloud as `manifest/manifest-backup.blob`.

The manifest records, for each file:
- Filenames and directory structure
- Per-file `file_key_wrapped` stored in the `nodes` table
- Chunk map: ordered list of UUID blob names and their sizes
- `snapshot_counter` for sync conflict detection

The local SQLCipher database is encrypted with `sqlcipher_key` (derived from `master_key` via HKDF). The cloud backup (`manifest-backup.blob`) is separately encrypted with `manifest_key` (also derived from `master_key` via HKDF). The cloud never sees manifest contents in plaintext.

---

## SQLCipher

An open-source extension to SQLite that provides transparent 256-bit AES encryption of the entire database file. Arx Runa uses SQLCipher as its local manifest database, keyed with `sqlcipher_key`. The database stores file paths, directory structure, chunk records, BLAKE3 checksums, and wrapped file keys. Without the correct `sqlcipher_key`, the database file is indistinguishable from random bytes.

---

## Blob

A single encrypted file chunk stored in the cloud. Blob names are UUID v4 strings with a `.blob` extension (e.g., `3f7a1b2c-dead-beef-cafe-112233445566.blob`). All blobs are padded to exactly 4 MiB, making them indistinguishable by size.

---

## Chunk

A 4 MiB fixed-size block produced when a file is split for encryption. The final chunk of a file is zero-padded to 4 MiB. Each chunk is encrypted independently with XChaCha20-Poly1305 and becomes one blob in the cloud.

The chunk size is fixed (not content-defined) to prevent file size inference from blob sizes.

---

## XChaCha20-Poly1305

The authenticated encryption scheme used to encrypt every file chunk in Arx Runa. XChaCha20 is a stream cipher with a 192-bit nonce; Poly1305 is the authentication tag. Each encryption call generates a fresh 192-bit nonce from the CSPRNG, making nonce collisions negligible across any realistic file count.

The wire format per chunk is `[24-byte nonce | ciphertext | 16-byte Poly1305 tag]`. The tag detects any single-bit tampering before decryption returns data.

XChaCha20-Poly1305 is also used to wrap `master_key` in the `recovery_slot`.

---

## BLAKE3

A cryptographic hash function used in Arx Runa for two purposes:

1. **Blob integrity** — a BLAKE3 checksum is computed over each encrypted blob after encryption and stored in the manifest. On download the checksum is verified before decryption begins, catching bit rot or storage corruption immediately.
2. **USB key file fingerprint** — the vault header stores the BLAKE3 hash of the 32-byte key file. The hash identifies which file to use; it is preimage-resistant and reveals nothing about the key file bytes.

---

## master_key

The root key material for a vault session, derived by Argon2id from the user's password (and USB key file bytes for Tier 2). All other vault keys are derived from `master_key` via HKDF-SHA256.

- **Tier 1**: `master_key = Argon2id(password, salt)`
- **Tier 2**: `master_key = Argon2id(password || key_file_bytes, salt)`

`master_key` exists only in RAM during an active session and is zeroized on vault lock.

---

## Argon2id

A memory-hard key derivation function and winner of the Password Hashing Competition. Arx Runa uses Argon2id to derive `master_key` from the user's password (and USB key file bytes for Tier 2).

Its memory requirement — 64 MiB per invocation at default parameters — ensures that offline brute-force attacks remain expensive regardless of the attacker's hardware: where conventional hash functions can be parallelised onto thousands of GPU cores, Argon2id's memory bandwidth demand caps the number of guesses that can run simultaneously. Parameters (memory, iterations, parallelism) are stored in the vault header so they can be tuned in future versions without invalidating existing vaults.

---

## HKDF

HMAC-based Extract-and-Expand Key Derivation Function (RFC 5869). Arx Runa uses HKDF-SHA256 to expand `master_key` into three purpose-specific vault-level keys, each derived with a distinct `info` label:

- `b"arx-runa-key-encryption"` → `key_encryption_key`
- `b"arx-runa-sqlcipher"` → `sqlcipher_key`
- `b"arx-runa-manifest-backup"` → `manifest_key`

Separate labels ensure the three derived keys are cryptographically independent: compromise of one reveals nothing about the others. `master_key` is zeroized immediately after all three derivations complete.

---

## CSPRNG

Cryptographically Secure Pseudo-Random Number Generator. The source of all random material in Arx Runa: encryption nonces, `file_key` values, Argon2id salts, and USB key file bytes. Arx Runa delegates CSPRNG calls to the operating system (`getrandom` on Linux/macOS, `BCryptGenRandom` on Windows).

---

## file_key

A 32-byte random key generated per file at encryption time. It is the XChaCha20-Poly1305 encryption key for that file's chunks. At rest, it is stored only as `file_key_wrapped` in the `nodes` table.

---

## key_encryption_key

A vault-level key derived from `master_key` via HKDF with info `b"arx-runa-key-encryption"`. It wraps and unwraps at-rest file keys (`nodes.file_key_wrapped` and `received_shares.file_key_wrapped`). It is not used for chunk encryption.

---

## manifest_key

A vault-level key derived from `master_key` via HKDF with info `b"arx-runa-manifest-backup"`. It encrypts and decrypts the singleton cloud manifest backup `manifest/manifest-backup.blob`.

---

## sqlcipher_key

A vault-level key derived from `master_key` via HKDF, used to encrypt the local SQLCipher manifest database.

---

## mlock

A POSIX system call (and its Windows equivalent `VirtualLock`) that pins memory pages to RAM and instructs the OS not to swap them to disk. Arx Runa calls `mlock` on all session key buffers (`key_encryption_key`, `sqlcipher_key`, `manifest_key`) immediately after derivation. If `mlock` fails, Arx Runa refuses to open the session rather than silently operating with keys that could be written to a swap file or hibernation image.

---

## Staging

A temporary local directory used during sync. Encrypted blobs are written to staging immediately after encryption, before cloud upload. Once a blob is confirmed uploaded it is deleted from staging. On startup, any orphaned blobs left by a previously interrupted session are cleaned up. The cloud never receives data from staging directly; it receives only finished encrypted blobs.

---

## USB Key File

A 32-byte file of cryptographically random entropy stored on a physical USB drive. Used as the hardware second factor in Tier 2 authentication. The key file is identified by its BLAKE3 fingerprint stored in the vault header - the filename is irrelevant.

Losing the USB key file without a backup means permanent loss of access to Tier 2 vaults. See [Use Case 3](../use-cases/use-case-3-hardware-mfa-and-key-loss.md).

---

## Tier 1 / Tier 2

Authentication tiers selected when creating a vault:

| Tier | Factors | Key derivation |
|------|---------|---------------|
| **Tier 1** | Password only | `Argon2id(password, salt)` |
| **Tier 2** | Password + USB key file | `Argon2id(password || key_file_bytes, salt)` |

Both tiers are zero-knowledge - the cloud provider never holds key material. Tier 2 additionally requires physical possession of the USB key file on every access.

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

The binding value included in every XChaCha20-Poly1305 encryption call: `file_id || chunk_index`. AAD prevents chunk-swap attacks - a chunk from one file cannot be spliced into another file's chunk sequence without causing an authentication failure.

---

## EXIF

Exchangeable Image File format metadata embedded in photos and other media files. A photo typically carries GPS coordinates, camera model, lens settings, capture timestamp, and device serial number alongside the image itself. Arx Runa strips EXIF (and the related XMP and IPTC formats) from media files in memory before encryption begins. The original file on disk is never modified; the stripped copy is what enters the encryption pipeline.

---

## BIP-39

A standard wordlist encoding scheme for binary entropy, originally designed for cryptocurrency hardware wallet seed phrases. Arx Runa uses it to encode the 256-bit recovery phrase as 24 human-readable words. The final word embeds a checksum that catches single-word transcription errors immediately, before Argon2id even runs.

---

## recovery_key

A 256-bit key derived by running the 24-word BIP-39 recovery phrase through Argon2id (using the same parameters and a dedicated recovery salt from the vault header). It exists only in RAM during a recovery ceremony and is used to unwrap `wrapped_master_key` from the vault header's `recovery_slot`. It is never stored anywhere.

---

## recovery_slot

An optional field in the vault header that stores `wrapped_master_key`. When present, it allows the vault to be opened with the BIP-39 recovery phrase instead of the primary password. The slot is populated at vault creation (opt-in) and re-wrapped under updated credentials after every successful password rotation or recovery ceremony.

---

## wrapped_master_key

The `master_key` encrypted with `recovery_key` under XChaCha20-Poly1305, stored in the vault header `recovery_slot`. 72 bytes total: `[24-byte nonce | 32-byte ciphertext | 16-byte Poly1305 tag]`. The AAD binding is `vault_id_bytes`, preventing the wrapped key from being transplanted to a different vault.

---

## HPKE

Hybrid Public Key Encryption (RFC 9180). The asymmetric encryption scheme used in Arx Runa's file sharing. The ciphersuite is `DHKEM(X25519, HKDF-SHA256) + HKDF-SHA256 + ChaCha20-Poly1305`. HPKE seals the `file_key` and file metadata for a recipient's X25519 public key so that only the holder of the corresponding private key can open the envelope. Neither the cloud provider nor Arx Runa's infrastructure can read the contents.

---

## X25519

The Diffie-Hellman key exchange function over Curve25519. In Arx Runa, each user generates an X25519 key pair as their sharing identity. The private key is stored in the encrypted vault; the public key is exported as a small file or QR code for out-of-band exchange with contacts. X25519 public keys are also the recipient key in HPKE share packages.

---

## file_share_id

A UUID v4 that identifies the shared blob set at `shared/<file_share_id>/`. All recipients of the same shared file snapshot reference the same `file_share_id`. This is distinct from `share_id`, which is per recipient-file pair.

---

## share_id

A UUID v4 that identifies one recipient-file share relationship. It appears in each share package and is the primary key in `shares` and `received_shares`. Multiple `share_id` values can reference one `file_share_id`.

---

## sender_public_key

The owner's 32-byte X25519 public key in the share package, stored as `received_shares.sender_public_key`. Recipients use it to encrypt download receipts even when no contact row exists.

---

## received_shares.file_key_wrapped

The recipient-side at-rest file key in SQLCipher. During import, raw `file_key` from the HPKE package is wrapped with local `key_encryption_key`, and only the wrapped value is persisted. Raw key bytes are zeroized after wrapping.

---

## File Sharing Key

There is no separate `share_key` in Arx Runa. Share packages carry the existing per-file `file_key` inside the HPKE envelope. See [Use Case 4](../use-cases/use-case-4-personal-file-sharing.md).
