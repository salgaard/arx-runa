# File Sharing

Arx Runa enables sharing individual encrypted files with specific contacts using HPKE (RFC 9180) with a committing AEAD. No central server is required — users exchange X25519 public keys out-of-band, and share packages are delivered through any channel of the sender's choice.

---

## Goals

- Share individual files with specific contacts without exposing vault-wide keys
- X25519 identity keypair generated locally — no central sign-up or server
- HPKE (RFC 9180) with committing AEAD (`CTX-ChaCha20-Poly1305`) for share packages
- Revocation via blob deletion — honest about the limits when content is already downloaded
- Shares are snapshots: the share captures the file at time of sharing; updates do not propagate

---

## Contract Surface

### Interface

Sharing surface: contact key exchange, share package export/import, revocation, receipt processing, expiration enforcement.

Package confidentiality/integrity: HPKE (RFC 9180) ciphersuite `DHKEM(X25519, HKDF-SHA256) + HKDF-SHA256 + CTX-ChaCha20-Poly1305`.

### Data

Canonical share package fields: `share_id`, `file_id`, `file_name`, `chunk_count`, `chunk_size`, `chunk_uuids`, `file_key`, `sender_public_key`, `cloud_endpoint`, optional `expires_at`.

Canonical cloud path: `shared/<file_share_id>/<uuid>.blob` plus `shared/<file_share_id>/receipts/<receipt_uuid>.blob`.

Canonical persistence tables: `contacts`, `shares`, `received_shares`.

### Invariants

- Per-file key isolation: sharing exposes only the selected file's `file_key`, never vault-wide keys.
- Share semantics are snapshot-based — `chunk_uuids` represent a fixed file version at share time.
- Revocation guarantees future-fetch blocking only; it cannot retract plaintext already decrypted.

### Dependencies

Depends on HPKE (RFC 9180) with the ciphersuite above and `info="arx-runa-share"` as the application context string. Depends on auth/session storage of the X25519 identity keypair and Phase 3 manifest contracts.

---

## Identity Model

Arx Runa generates an X25519 keypair on first run. This keypair is the user's identity. The private key is stored in SQLCipher, wrapped with `key_encryption_key`.

### Key exchange (out-of-band)

1. Alice exports her X25519 public key as a file or QR code
2. Alice sends it to Bob via email, messaging, USB, or any other channel
3. Bob imports Alice's public key into Arx Runa as a contact
4. Both sides repeat the process to enable bidirectional sharing

Arx Runa never touches email infrastructure. Delivery is delegated to the user's existing trusted channel — the same model used by WireGuard, age, and PGP.

### Key rotation

The X25519 identity keypair survives password changes and key file rotations. The keypair is re-wrapped under the new `key_encryption_key` but the keypair itself does not change — sharing relationships are preserved.

---

## Per-File Key Architecture

```
master_key  (Argon2id output, mlocked memory, never stored)
    │
    ├─ key_encryption_key  →  wraps per-file file_keys in SQLCipher
    ├─ sqlcipher_key        →  SQLCipher database
    └─ manifest_key         →  manifest cloud backup

Per file (generated at file creation):
    file_key  (random 256-bit CSPRNG)
        └─ stored encrypted with key_encryption_key in nodes table
        └─ used for all XChaCha20-Poly1305 chunk encryption
```

Per-file key isolation enables file-granularity sharing: the share package hands the recipient only the `file_key` for the shared file, encrypted under their public key. No vault-wide keys or other file keys are exposed.

---

## HPKE Construction

Share packages are encrypted using HPKE (RFC 9180) in one-shot Base mode:

```
Ciphersuite: DHKEM(X25519, HKDF-SHA256) + HKDF-SHA256 + CTX-ChaCha20-Poly1305
Application context: info = b"arx-runa-share"
```

`CTX-ChaCha20-Poly1305` replaces the standard 16-byte Poly1305 tag with a 32-byte BLAKE3 commitment tag, achieving CMT-4 (full key commitment) security. This defends against partition oracle attacks on `file_key`.

### Sender

```
(enc, ct) = HPKE.Seal(
    mode      = Base,
    recipient = recipient_public_key,   // X25519, 32 bytes
    info      = b"arx-runa-share",
    aad       = b"",
    plaintext = package_json_bytes,
)
```

HPKE generates the ephemeral X25519 keypair internally; the ephemeral private key is discarded after encapsulation.

### Recipient

```
plaintext = HPKE.Open(
    mode       = Base,
    recipient  = recipient_private_key, // X25519, 32 bytes
    enc        = enc,                   // 32-byte ephemeral public key from wire
    info       = b"arx-runa-share",
    aad        = b"",
    ciphertext = ct,
)
```

### Wire format

```
[32B enc | ciphertext | 32B CTX tag]
```

---

## Share Package Format

The plaintext inside the HPKE envelope:

```json
{
  "share_id": "<uuid-v4>",
  "file_id": "<uuid-v4>",
  "file_name": "report.pdf",
  "chunk_count": 12,
  "chunk_size": 4194304,
  "chunk_uuids": ["<uuid>", "<uuid>", "..."],
  "file_key": "<32-byte file_key, base64>",
  "sender_public_key": "<32-byte X25519 public key, base64>",
  "cloud_endpoint": {
    "provider": "s3",
    "bucket": "alice-arx-runa",
    "region": "eu-west-1",
    "endpoint": "https://s3.eu-west-1.amazonaws.com",
    "path_prefix": "shared/<file_share_id>/"
  },
  "expires_at": null
}
```

`file_key` is the raw 32-byte file encryption key inside the HPKE envelope. The outer HPKE layer provides all confidentiality and integrity — no inner wrapping is needed.

`sender_public_key` enables receipt encryption without requiring a pre-existing contact record.

### Delivery

The owner exports the package as a file and delivers it via their own channel. The recipient imports it via file picker or a `arx-runa://share-import?...` deep link.

---

## Cloud Storage Layout

```
<cloud_root>/
  shared/
    <file_share_id>/
      <uuid>.blob              # encrypted chunks (publicly readable)
      receipts/
        <receipt_uuid>.blob   # download receipts (owner-encrypted)
```

When the owner shares a file:
1. Arx Runa copies relevant encrypted blobs from `vault/` into `shared/<file_share_id>/`
2. No re-encryption required — blobs are already encrypted with `file_key`, which travels in the share package
3. `chunk_uuids` in the package point to blobs in `shared/<file_share_id>/`

Blobs in `shared/` are publicly readable. Without `file_key`, blobs are permanently inaccessible. UUID v4 blob names (122 bits of entropy) are not guessable.

---

## Revocation

### Recipient has not fetched the blobs

Delete `shared/<file_share_id>/` from the cloud. The share package the recipient holds now points to dead UUIDs — access is revoked without re-encryption.

### Recipient has already fetched and decrypted the blobs

Cryptographic revocation of already-fetched plaintext is not possible. For a stronger guarantee:

1. Re-encrypt the file under a new `file_key`; upload new blobs under a new `file_share_id`
2. Issue new share packages to all remaining recipients
3. Delete the old `shared/<old_file_share_id>/` folder

This requires coordination with remaining active recipients.

---

## Download Receipts

After a recipient successfully downloads and decrypts all chunks, Arx Runa writes a receipt to the owner's cloud folder:

```
shared/<file_share_id>/receipts/<receipt_uuid>.blob
```

The receipt is encrypted with the owner's X25519 public key via HPKE. Only the owner can decrypt it.

```json
{
  "share_id": "<uuid-v4>",
  "recipient_contact_id": "<uuid-v4>",
  "downloaded_at": 1714000000
}
```

Receipts are cooperative — a malicious recipient can choose not to write one, or write a false one. The owner should treat receipts as informational, not authoritative.

---

## Share Expiration

An optional `expires_at` field in the share package (Unix timestamp) signals an expiration date. Arx Runa checks this field at import time and before downloading blobs. The owner's sync task also cleans up `shared/<file_share_id>/` folders for expired shares.

---

## Database Schema

```sql
CREATE TABLE contacts (
    contact_id   TEXT PRIMARY KEY,
    display_name TEXT NOT NULL,
    email        TEXT,
    public_key   BLOB NOT NULL,
    created_at   INTEGER NOT NULL
);

CREATE TABLE shares (
    share_id      TEXT PRIMARY KEY,
    file_id       TEXT NOT NULL REFERENCES nodes(node_id),
    contact_id    TEXT NOT NULL REFERENCES contacts(contact_id),
    file_share_id TEXT NOT NULL,
    cloud_path    TEXT NOT NULL,
    created_at    INTEGER NOT NULL,
    expires_at    INTEGER,            -- NULL = no expiration
    revoked_at    INTEGER
);

CREATE TABLE received_shares (
    share_id           TEXT PRIMARY KEY,
    sender_contact_id  TEXT REFERENCES contacts(contact_id),
    file_name          TEXT NOT NULL,
    file_key_wrapped   BLOB NOT NULL,
    chunk_count        INTEGER NOT NULL,
    chunk_size         INTEGER NOT NULL,
    chunk_uuids        TEXT NOT NULL CHECK (json_valid(chunk_uuids)),
    cloud_endpoint     TEXT NOT NULL,
    imported_at        INTEGER NOT NULL
);
```

---

## Related Documents

- [Cryptographic Primitives](design-cryptographic-primitives.md) — per-file key model, XChaCha20-Poly1305 wrapper
- [Authentication and Session Management](design-authentication.md) — X25519 identity keypair storage and key rotation
- [Chunking and Manifest](design-chunking-and-manifest.md) — `file_key_wrapped`, chunk metadata, `received_shares` table
- [Cloud Synchronisation](design-cloud-synchronisation.md) — `shared/` cloud namespace, receipt polling
- [Tauri IPC and Frontend](design-tauri-ipc-and-frontend.md) — sharing command surface
