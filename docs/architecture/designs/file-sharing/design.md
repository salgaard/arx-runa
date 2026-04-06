# Arx Runa — File Sharing Architecture Design

> Status: Design complete. Implementation target: Phase 5.
> Last updated: 2026-03-29

---

## Goals

- Users can share individual files with specific contacts
- Both personal use (family, friends) and team use must be supportable with the same architecture
- Revocation prevents future access; honest about the limits of cryptographic revocation
- No central sign-up required — Arx Runa generates a local X25519 identity on first run
- Folder sharing is a future extension of the same mechanism (multiple files in one share package)
- Recipients must run Arx Runa — there is no browser-based or app-less recipient flow
- Shares are snapshots: the share captures the file at the time of sharing; updates to the original file do not propagate automatically

---

## Identity Model

### No central server for identity

Arx Runa generates an X25519 keypair on first run. This keypair is the user's identity. The private key is stored in the local SQLCipher vault, wrapped with `key_encryption_key` and protected by the same auth flow (USB key file + password → Argon2id → `master_key` → `key_encryption_key`).

Email addresses are used as human-readable labels for contacts only. Arx Runa does not send email itself and does not require email credentials. The email address is a display name, not a delivery mechanism.

### Key exchange

Public keys are exchanged out-of-band:

1. Alice opens Arx Runa and exports her X25519 public key as a small file or QR code
2. Alice sends this to Bob via her own email client, a messaging app, a USB stick, or any other channel
3. Bob imports Alice's public key into Arx Runa — it is stored as a contact with Alice's display name and optional email label
4. The reverse happens so both sides can share with each other

Arx Runa never touches email infrastructure. Delegating delivery to the user's existing trusted channel avoids SMTP credential exposure and is the same model used by WireGuard, age, and PGP.

### Trust assumption

The security of key exchange depends on the out-of-band channel. This is an explicit design assumption stated in the threat model. If an attacker controls the channel (MITM), they can substitute their own public key and receive the share package instead of the intended recipient.

Mitigation: fingerprint verification. Arx Runa displays a short hash (first 16 hex characters of SHA-256 of the public key). The owner and contact can compare fingerprints out-of-band (phone call, in person). This is opt-in UX, not a system requirement.

---

## Key Architecture: Per-File Keys

The base Arx Runa design uses a vault-wide `key_encryption_key` (HKDF-derived from `master_key`) that wraps per-file random keys. Each file has its own `file_key` — a random 256-bit value generated at file creation time via CSPRNG.

```
master_key  (Argon2id output, mlocked memory, never stored)
    │
    ├─ key_encryption_key  (HKDF-SHA256, info: "voidgate-key-encryption")
    │       └─ wraps/unwraps per-file file_keys stored in SQLCipher
    ├─ sqlcipher_key       (HKDF-SHA256, info: "voidgate-sqlcipher")
    └─ manifest_key        (HKDF-SHA256, info: "voidgate-manifest-backup")

Per file (generated at file creation):
    file_key  (random 256-bit via CSPRNG)
        └─ stored encrypted with key_encryption_key in SQLCipher nodes table
        └─ used for all XChaCha20-Poly1305 chunk encryption of that file
```

This design enables:
- File-granularity sharing: hand a recipient only the `file_key` for the shared file
- Secure file deletion: destroy the `file_key` record from SQLCipher and the ciphertext is permanently inaccessible
- Key compromise isolation: compromising one file's `file_key` does not affect other files

---

## ECIES Construction

Share packages are encrypted using the recipient's X25519 public key via ECIES (Elliptic Curve Integrated Encryption Scheme). The construction is:

1. Generate an ephemeral X25519 keypair (ephemeral private key is discarded after use)
2. Perform ECDH between the ephemeral private key and the recipient's long-term public key → shared secret
3. Derive a symmetric key: `HKDF-SHA256(shared_secret, salt=ephemeral_public_key, info="voidgate-share")`
4. Encrypt the share package content with that symmetric key using XChaCha20-Poly1305
5. Transmit: `[ephemeral_public_key | nonce | ciphertext | Poly1305 tag]`

The recipient decrypts by:
1. Performing ECDH between their long-term private key and the received ephemeral public key
2. Deriving the same symmetric key via HKDF
3. Decrypting with XChaCha20-Poly1305

Reference implementation: the `age` encryption tool uses the same construction and is audited. The `x25519-dalek` crate provides the X25519 primitives.

---

## Share Package Format

When the owner shares a file, Arx Runa produces a share package — a small file containing everything the recipient needs to fetch and decrypt the shared file.

### Plaintext fields (inside the ECIES envelope)

```json
{
  "share_id": "<uuid-v4>",
  "file_id": "<uuid-v4>",
  "file_name": "report.pdf",
  "chunk_count": 12,
  "chunk_size": 4194304,
  "chunk_uuids": ["<uuid>", "<uuid>", "..."],
  "file_key_wrapped": "<file_key encrypted with ECDH-derived symmetric key, base64>",
  "cloud_endpoint": {
    "provider": "s3",
    "bucket": "alice-voidgate",
    "region": "eu-west-1",
    "endpoint": "https://s3.eu-west-1.amazonaws.com",
    "share_path": "shared/<file_share_id>/"
  }
}
```

### Wire format

```
[32B ephemeral_public_key | 24B nonce | encrypted_package_json | 16B Poly1305 tag]
```

The `file_key_wrapped` field is the `file_key` encrypted with the ECDH-derived symmetric key. Once the outer envelope is decrypted, the recipient decrypts `file_key_wrapped` with the same symmetric key to obtain the `file_key` for chunk decryption.

### Delivery

The owner exports the share package as a file and delivers it via their own channel (email, messaging app, USB). The recipient imports it into Arx Runa via file picker or a `voidgate://share-import?...` deep link (Tauri custom URI scheme).

---

## Cloud Storage Layout

```
<cloud_root>/
  vault/
    <uuid>.blob          ← owner's private chunks (not accessible to recipients)
  shared/
    <file_share_id>/
      <uuid>.blob        ← copies of chunks for this file, publicly readable
      ...
```

`file_share_id` is a UUID v4 that identifies a shared copy of a file. All recipients of the same file share the same set of blobs. This is distinct from `share_id` (per recipient–file pair).

When the owner shares a file:
1. Arx Runa copies the relevant encrypted blobs from `vault/` into `shared/<file_share_id>/`
2. No re-encryption is required — blobs are already encrypted with `file_key`, which travels in the share package
3. The share package's `chunk_uuids` list points to blobs in `shared/<file_share_id>/`

### Cloud authentication for recipients

Blobs in `shared/<file_share_id>/` are publicly readable (Option A). The `cloud_endpoint` object in the share package provides the Rclone connection descriptor. No credentials travel in the share package.

Justification: blobs are opaque AEAD ciphertext. Without `file_key`, the blobs are permanently inaccessible. UUID v4 blob names (122 bits of entropy) are not guessable. The cloud provider already has read access to all blobs by design. Ciphertext exposure to a third party who discovers the folder is not a security failure — it is an accepted property of the architecture stated in the threat model.

---

## Revocation

### Case: recipient has not fetched the blobs

Delete `shared/<file_share_id>/` from the cloud. The share package the recipient holds now points to dead UUIDs. Access is revoked without re-encryption.

### Case: recipient has already fetched and decrypted the blobs

Cryptographic revocation of already-fetched plaintext is impossible. The owner can:
1. Accept the limitation (honest model)
2. For a stronger guarantee: re-encrypt the file under a new `file_key`, upload new blobs under a new `file_share_id`, issue new share packages to all remaining recipients, and delete the old `file_share_id` folder

**Explicit report statement**: revocation prevents future fetches via blob deletion. It cannot revoke access from a party who has already downloaded and decrypted the content. Re-encryption provides a stronger guarantee but requires coordination with remaining active recipients.

### Single recipient vs. multiple recipients

If a file is shared with multiple recipients and the owner revokes access for one:
- Deleting the shared folder would revoke all recipients
- The correct action for single-recipient revocation when others remain: mark the share as revoked in the local `shares` table; if a stronger guarantee is needed, re-encrypt the file, upload new blobs, re-share with remaining recipients, and delete the old folder

---

## Download Receipts

### Purpose

The owner of a shared file may want to know when a recipient has downloaded it. Download receipts provide a lightweight, cloud-mediated notification mechanism that does not require a central server.

### Mechanism

After a recipient successfully downloads and decrypts all chunks of a shared file, the recipient's Arx Runa writes a receipt to the owner's shared folder in the cloud:

```
shared/<file_share_id>/receipts/<receipt_uuid>.blob
```

The receipt is encrypted with the owner's X25519 public key via ECIES (the same construction used for share packages). Only the owner can decrypt it.

### Receipt format (plaintext inside ECIES envelope)

```json
{
  "share_id": "<uuid-v4>",
  "recipient_contact_id": "<uuid-v4>",
  "downloaded_at": 1714000000
}
```

### Owner reads receipts

On the next manifest pull or sync operation, the owner's Arx Runa:

1. Lists blobs under `shared/<file_share_id>/receipts/`
2. Downloads and decrypts each receipt using the owner's X25519 private key
3. Displays a notification: "Your shared file was downloaded by [contact name] on [date]"
4. Deletes the receipt blob from the cloud after reading (optional; keeps the shared folder tidy)

### Security properties

- Only the owner can read receipts (ECIES with owner's public key)
- The cloud provider sees that a receipt blob was written but cannot read its content
- A malicious recipient can choose not to write a receipt — receipts are cooperative, not enforceable
- A malicious recipient can write a false receipt — the owner should treat receipts as informational, not authoritative

### Scope

Implementation target: Phase 5 (optional enhancement alongside core file sharing).

---

## Share Expiration

### Purpose

The owner may want a share to expire automatically after a specified period, without requiring manual revocation.

### Share package field

The share package JSON (inside the ECIES envelope) gains an optional `expires_at` field:

```json
{
  "share_id": "<uuid-v4>",
  "file_id": "<uuid-v4>",
  "file_name": "report.pdf",
  "expires_at": 1717200000,
  ...
}
```

If `expires_at` is `null` or absent, the share does not expire.

### Database schema addition

The canonical `shares` table definition (see [Database Schema](#database-schema) below) includes `expires_at`. For existing databases, apply the following migration:

```sql
ALTER TABLE shares ADD COLUMN expires_at INTEGER;  -- Unix timestamp, NULL = no expiration
```

### Enforcement

Expiration is enforced at two levels:

1. **Cooperative (recipient-side)**: The recipient's Arx Runa checks `expires_at` before decrypting. If the current time exceeds `expires_at`, Arx Runa displays "Share expired — contact sender for renewed access" and refuses to decrypt. A malicious or modified recipient client can bypass this check.

2. **Server-side (owner-side)**: On each push or sync operation, the owner's Arx Runa checks all active shares for expired `expires_at` values. For each expired share:
   - Delete the blobs under `shared/<file_share_id>/` from the cloud
   - Set `shares.revoked_at` to the current timestamp
   - This provides enforcement independent of the recipient's cooperation

### UX

When sharing a file, the sender can optionally configure an expiration period (e.g., "7 days", "30 days", "90 days", or "No expiration"). Arx Runa computes `expires_at` as the current Unix timestamp plus the selected duration.

### Scope

Implementation target: Phase 5.

---

## Database Schema

### Owner side

```sql
-- Contacts: people the owner shares with
CREATE TABLE contacts (
    contact_id   TEXT PRIMARY KEY,  -- UUID v4
    display_name TEXT NOT NULL,
    email        TEXT,              -- display label only, not used for delivery
    public_key   BLOB NOT NULL,     -- X25519 public key, 32 bytes
    created_at   INTEGER NOT NULL
);

-- Active and revoked outgoing shares
CREATE TABLE shares (
    share_id        TEXT PRIMARY KEY,   -- UUID v4, per recipient-file pair
    file_id         TEXT NOT NULL REFERENCES nodes(node_id),
    contact_id      TEXT NOT NULL REFERENCES contacts(contact_id),
    file_share_id   TEXT NOT NULL,      -- UUID v4, groups all recipients of the same file
    cloud_path      TEXT NOT NULL,      -- path to shared/<file_share_id>/ in cloud
    created_at      INTEGER NOT NULL,
    revoked_at      INTEGER,            -- NULL = active
    expires_at      INTEGER             -- NULL = no expiration (Unix timestamp)
);
```

### Recipient side

```sql
-- Received shares (populated from imported share packages)
CREATE TABLE received_shares (
    share_id             TEXT PRIMARY KEY,   -- from the share package
    sender_contact_id    TEXT REFERENCES contacts(contact_id),
    file_name            TEXT NOT NULL,
    file_key_wrapped     BLOB NOT NULL,      -- encrypted file_key, decrypted on access
    chunk_count          INTEGER NOT NULL,
    chunk_size           INTEGER NOT NULL,
    chunk_uuids          TEXT NOT NULL,      -- JSON array
    cloud_endpoint       TEXT NOT NULL,      -- JSON object
    imported_at          INTEGER NOT NULL
);
```

### Nodes table addition

`file_key_wrapped` is stored in the `nodes` table (per file, not per chunk). This was established in the Phase 3 chunking design — one copy per file, cleaned up automatically by the existing CASCADE when a node is deleted. No schema addition is needed for sharing beyond the `shares`, `contacts`, and `received_shares` tables.

---

## Snapshot Semantics

A share is a snapshot of the file at the time of sharing. The `chunk_uuids` list is static. If the owner modifies the file after sharing:
- The existing share still points to the old blobs (the recipient sees the version at share time)
- The owner must create a new share to give the recipient the updated version

This is a deliberate design decision. Live sharing (recipient always sees the latest version) would require a directory-level share agreement rather than a snapshot package, and is deferred as a future extension.

---

## Threat Model Additions

### MITM on key exchange

If Alice and Bob exchange public keys over an untrusted channel (plain email without S/MIME or PGP), an attacker who controls the channel can substitute their own public key. The owner would encrypt the share package for the attacker, not the intended recipient.

**Mitigation**: fingerprint verification. Arx Runa displays a short fingerprint (first 16 hex characters of SHA-256 of the X25519 public key) alongside each contact. Alice and Bob compare fingerprints over a separate channel (phone call, in person). This is opt-in UX; the system does not enforce it.

**Threat model statement**: the security of the key exchange is as strong as the out-of-band delivery channel. This is the same trust assumption made by WireGuard, age, and PGP.

### Ciphertext exposure via public blobs

Blobs in `shared/<file_share_id>/` are publicly readable. A party who discovers the folder UUID can download the ciphertext. Without `file_key` they cannot decrypt, but:
- The existence of a share is exposed to the cloud provider and anyone who discovers the UUID
- AEAD ciphertext is not malleable, but offline analysis of the blob count and sizes may reveal approximate file size (mitigated by fixed-size uniform padding)

**Threat model statement**: ciphertext exposure to a party without `file_key` is an accepted property of the public-blobs architecture. The cloud provider already has this access. The alternative (per-recipient access tokens) requires per-provider token scoping support and introduces credential management complexity.

---

## Open Decisions

| Decision | Options | Status |
|----------|---------|--------|
| Enterprise key distribution | IT-distributed JSON file of employee public keys vs. optional internal key directory server | Extension point, not blocking for Phase 5 |
| Fingerprint verification UX | Display format, where in UI, whether to warn on unverified contacts | Not yet designed |
| Folder sharing UX | How to handle files added to a shared folder after the share was created | Deferred; snapshot model applies to files, folder extension is future |
| ~~Identity keypair on key rotation~~ | ~~Re-wrap vs. invalidate~~ | Resolved: X25519 keypair is re-wrapped under new key_encryption_key, not regenerated — sharing relationships survive key file rotation |

---

## Decisions Made

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Identity model | Local X25519 keypair, no central server | Fits zero-trust model, no sign-up |
| Key exchange | Out-of-band file or QR code export | No SMTP dependency, user controls delivery channel |
| Email as label | Display name only, not transport | Avoids credential exposure |
| Per-file keys | Yes, random `file_key` per file | Required for file-granularity sharing; enables secure deletion |
| ECIES construction | Ephemeral X25519 → ECDH → HKDF → XChaCha20-Poly1305 | Standard ECIES; same as age tool |
| Share package delivery | Owner-exported file, out-of-band | Cloud never holds sharing metadata |
| Cloud layout | Shared blobs in `shared/<file_share_id>/` (one copy per file) | Scalable; O(1) storage regardless of recipient count |
| Cloud auth | Public readable blobs | No credentials in share package; AEAD guarantees protect content |
| Revocation | Blob deletion + optional re-encryption | Honest model; stronger guarantee available at cost of re-encryption |
| Share semantics | Snapshot at time of sharing | Simple, correct, deliberate |
