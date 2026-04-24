# Arx Runa — File Sharing Architecture Design

> Status: Design complete. Implementation target: Phase 5.
> Last updated: 2026-04-20

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

## Contract Surface

### Interface contract

- Sharing surface covers contact key exchange, share package export/import, revocation handling, receipt processing, and optional expiration enforcement.
- Package confidentiality/integrity contract is HPKE (RFC 9180): `DHKEM(X25519, HKDF-SHA256) + HKDF-SHA256 + CTX-ChaCha20-Poly1305` (committing AEAD).
- Delivery contract remains out-of-band; cloud stores encrypted shared blobs and encrypted receipt blobs.

### Data contract

- Canonical share package fields are `share_id`, `file_id`, `file_name`, `chunk_count`, `chunk_size`, `chunk_uuids`, `file_key`, `sender_public_key`, `cloud_endpoint`, and optional `expires_at`.
- Canonical shared-cloud path contract is `shared/<file_share_id>/<uuid>.blob` plus `shared/<file_share_id>/receipts/<receipt_uuid>.blob`.
- Canonical persistence tables are `contacts`, `shares`, and `received_shares` (including `revoked_at` / `expires_at` semantics, `received_shares.file_key_wrapped`, and `received_shares.sender_public_key`).

### Invariant contract

- Per-file key isolation is preserved: sharing exposes only the selected file's `file_key` context, never vault-wide keys.
- Share semantics are snapshot-based (`chunk_uuids` represent a fixed file version at share time).
- Revocation guarantees future-fetch blocking only; it cannot retract plaintext already downloaded/decrypted by a recipient.
- Cross-phase invariant reference: [docs/architecture/design-invariants.md](../../design-invariants.md).

### Dependency contract

- Depends on HPKE (RFC 9180) with ciphersuite `DHKEM(X25519, HKDF-SHA256) + HKDF-SHA256 + CTX-ChaCha20-Poly1305` for share package and receipt encryption; `info="arx-runa-share"` is the HPKE application context string.
- Depends on authentication/session storage of the local X25519 identity keypair and on phase-3 manifest contracts (`nodes.file_key_wrapped`, chunk metadata).
- Depends on phase-4 cloud layout (`shared/` namespace) and sync cycles for receipt polling and expired-share cleanup.

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
    ├─ key_encryption_key  (HKDF-SHA256, info: "arx-runa-key-encryption")
    │       └─ wraps/unwraps per-file file_keys stored in SQLCipher
    ├─ sqlcipher_key       (HKDF-SHA256, info: "arx-runa-sqlcipher")
    └─ manifest_key        (HKDF-SHA256, info: "arx-runa-manifest-backup")

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

## HPKE Construction

Share packages are encrypted using the recipient's X25519 public key via **HPKE (RFC 9180)** in one-shot Base mode. The ciphersuite is:

```
DHKEM(X25519, HKDF-SHA256) + HKDF-SHA256 + CTX-ChaCha20-Poly1305
```

`CTX-ChaCha20-Poly1305` is a committing AEAD: the standard 16-byte Poly1305 tag is replaced with a 32-byte BLAKE3 commitment tag `BLAKE3(b"arx-runa-ctx-v1" || key || nonce || ciphertext)`. This achieves CMT-4 (full key commitment) security, defending against partition oracle attacks on `file_key`. See `docs/research/file-sharing-cryptography.md` for the full rationale.

### Sender (share package creation)

```
(enc, ct) = HPKE.Seal(
    mode      = Base,
    recipient = recipient_public_key,   // X25519, 32 bytes
    info      = b"arx-runa-share",
    aad       = b"",
    plaintext = package_json_bytes,
)
```

HPKE generates the ephemeral X25519 keypair internally. Both the ephemeral and recipient public keys are automatically included in the KEM context — no manual HKDF salt construction is required. The ephemeral private key is discarded after encapsulation.

### Recipient (share package decryption)

```
plaintext = HPKE.Open(
    mode      = Base,
    recipient = recipient_private_key,  // X25519, 32 bytes
    enc       = enc,                    // 32-byte ephemeral public key from wire
    info      = b"arx-runa-share",
    aad       = b"",
    ciphertext = ct,
)
```

### Rust implementation

The `hpke` crate (v0.13.0) is used only for DHKEM(X25519, HKDF-SHA256) encapsulation/decapsulation via `hpke::kem::X25519HkdfSha256`. The HPKE key schedule (RFC 9180 §5.1) is implemented manually in `sharing::hpke` so that `CTX-ChaCha20-Poly1305` can be plugged in as the AEAD without using the crate's sealed `Aead` trait, which does not support custom tag lengths. The `suite_id` used for labeled extraction/expansion is `b"HPKE" || 0x0020 || 0x0001 || 0x0003`, where AEAD ID `0x0003` is the IANA-registered identifier for ChaCha20-Poly1305; CTX is a wire-equivalent committing wrapper that does not alter the key schedule.

`CTX-ChaCha20-Poly1305` is a thin wrapper type in the sharing crypto module that replaces the standard 16-byte Poly1305 tag with a 32-byte BLAKE3 commitment.
Implementation must include adversarial tests that flip one bit in `enc`, ciphertext, and the 32-byte CTX tag; all variants must fail decryption with authentication error.

---

## Share Package Format

When the owner shares a file, Arx Runa produces a share package — a small file containing everything the recipient needs to fetch and decrypt the shared file.

### Plaintext fields (inside the HPKE envelope)

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
  }
}
```

### Wire format

```
[32B enc | ciphertext | 32B CTX tag]
```

`enc` is the ephemeral X25519 public key output by HPKE's KEM encapsulation. The HPKE nonce is managed internally and does not appear in the wire format. The CTX tag is 32 bytes (vs 16 bytes for a plain Poly1305 tag).

`file_key` is the raw 32-byte file encryption key in plaintext inside the HPKE-encrypted envelope. The outer HPKE layer (CTX-ChaCha20-Poly1305) provides all confidentiality and integrity — no inner wrapping is needed.

`sender_public_key` is the owner's X25519 public key. Recipients use this field for receipt encryption even when no local contact entry exists for the sender.

### Delivery

The owner exports the share package as a file and delivers it via their own channel (email, messaging app, USB). The recipient imports it into Arx Runa via file picker or a `arx-runa://share-import?...` deep link (Tauri custom URI scheme).

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
- The correct action for single-recipient revocation when others remain is a two-path flow:

1. **Default (cooperative)**: set `shares.revoked_at` for the revoked recipient and stop issuing new share packages to that contact. This does not prevent access for a recipient that already has package + blobs.
2. **Strong revocation (enforced)**:
   - Generate a new random `file_key`
   - Re-encrypt the file and upload chunks under a new `file_share_id`
   - Create fresh share packages for remaining recipients only
   - Delete the old `shared/<old_file_share_id>/` folder
   - Mark all old rows tied to the old `file_share_id` as revoked (`revoked_at`)

---

## Download Receipts

### Purpose

The owner of a shared file may want to know when a recipient has downloaded it. Download receipts provide a lightweight, cloud-mediated notification mechanism that does not require a central server.

### Mechanism

After a recipient successfully downloads and decrypts all chunks of a shared file, the recipient's Arx Runa writes a receipt to the owner's shared folder in the cloud:

```
shared/<file_share_id>/receipts/<receipt_uuid>.blob
```

The receipt is encrypted with the owner's X25519 public key via HPKE (the same construction used for share packages). The recipient reads this public key from `sender_public_key` in the imported share package, so receipt encryption does not depend on a pre-existing contact record. Only the owner can decrypt it.

### Receipt format (plaintext inside HPKE envelope)

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

- Only the owner can read receipts (HPKE with owner's public key)
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

The share package JSON (inside the HPKE envelope) gains an optional `expires_at` field:

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
CREATE INDEX IF NOT EXISTS idx_shares_active_file ON shares(file_id) WHERE revoked_at IS NULL;
CREATE INDEX IF NOT EXISTS idx_shares_active_expiry ON shares(expires_at) WHERE revoked_at IS NULL AND expires_at IS NOT NULL;
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

CREATE INDEX idx_shares_active_file
    ON shares(file_id)
    WHERE revoked_at IS NULL;

CREATE INDEX idx_shares_active_expiry
    ON shares(expires_at)
    WHERE revoked_at IS NULL AND expires_at IS NOT NULL;
```

### Recipient side

```sql
-- Received shares (populated from imported share packages)
CREATE TABLE received_shares (
    share_id             TEXT PRIMARY KEY,   -- from the share package
    sender_contact_id    TEXT REFERENCES contacts(contact_id),
    sender_public_key    BLOB NOT NULL,      -- X25519 public key from share package, 32 bytes
    file_name            TEXT NOT NULL,
    file_key_wrapped     BLOB NOT NULL,      -- KEK-wrapped file_key for at-rest protection
    chunk_count          INTEGER NOT NULL,
    chunk_size           INTEGER NOT NULL,
    chunk_uuids          TEXT NOT NULL,      -- JSON array
    cloud_endpoint       TEXT NOT NULL,      -- JSON object
    expires_at           INTEGER,            -- NULL = no expiration (Unix timestamp)
    imported_at          INTEGER NOT NULL
);
```

### Nodes table addition

`file_key_wrapped` is stored in the `nodes` table (per file, not per chunk) — this is the vault-internal KEK-wrapped key used for local decryption, and is unchanged. On share import, Arx Runa decrypts the package to recover raw `file_key`, immediately wraps it with local `key_encryption_key`, and persists only `received_shares.file_key_wrapped`. Raw `file_key` bytes are zeroized after wrapping. `received_shares.sender_public_key` is stored so receipts remain possible even when no sender contact exists.

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

## Fingerprint Verification (Phase 6.5+)

**Current Implementation (Phase 6.5+)**:
- Display fingerprint as first 16 lowercase hex characters of SHA-256(public_key)
- Show fingerprint in contact list and share initiation flow
- Format: `0a1b2c3d4e5f6789` (16 chars, monospace font)

**User Guidance**:
> "Verify this fingerprint matches what the recipient sees before sharing sensitive files. Contact them out-of-band (phone, video call, etc.) to confirm fingerprints match."

**Out of Scope (Phase 7+)**:
- Fingerprint history tracking ("verified since date X")
- Automatic pin/trust warnings
- QR code fingerprint verification

---

## Open Decisions

| Decision | Options | Status |
|----------|---------|--------|
| Enterprise key distribution | IT-distributed JSON file of employee public keys vs. optional internal key directory server | Extension point, not blocking for Phase 5 |
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
| HPKE construction | HPKE RFC 9180: `DHKEM(X25519, HKDF-SHA256) + HKDF-SHA256 + CTX-ChaCha20-Poly1305` | Formal IND-CCA2 proof; both public keys in key schedule by construction; committing AEAD (CMT-4) via CTX layer; see `docs/research/file-sharing-cryptography.md` |
| Share package delivery | Owner-exported file, out-of-band | Cloud never holds sharing metadata |
| Cloud layout | Shared blobs in `shared/<file_share_id>/` (one copy per file) | Scalable; O(1) storage regardless of recipient count |
| Cloud auth | Public readable blobs | No credentials in share package; AEAD guarantees protect content |
| Revocation | Blob deletion + optional re-encryption | Honest model; stronger guarantee available at cost of re-encryption |
| Share semantics | Snapshot at time of sharing | Simple, correct, deliberate |
| Share package sender key | Include `sender_public_key` in every package | Enables receipt encryption without coupling to local contact DB state |
| Received-share key storage | Persist `received_shares.file_key_wrapped` (not raw `file_key`) | Keeps at-rest treatment consistent with node file keys and zeroization model |
| Single-recipient revocation | Cooperative `revoked_at` default; strong path rotates `file_key` and `file_share_id` | Makes revocation behavior explicit and implementable for both low-cost and enforced cases |
| Share expiration query performance | Partial indexes on active rows (`file_id`, `expires_at`) | Avoids full scans during sync/push enforcement loops as share volume grows |

---

## Category C: Architectural Decisions (Finalized)

These decisions are intentional MVP scope limitations that will persist through Phase 6. Phase 7+ planning may reconsider them with explicit research.

| Decision | Status | Rationale | Notes |
|----------|--------|-----------|-------|
| **c-fingerprint-verification-ux** — Display-only, out-of-band verification | ✅ Implemented | Fingerprint verification is shown in UI (16-character lowercase hex from SHA-256(public_key)). Out-of-band verification (phone call, in person, QR code) is user responsibility. No UX forcing or automated trust tracking in Phase 6. | Implemented in Phase 6.8 UI; documented in [Deferred Items Inventory](../../deferred-items-inventory.md) Category C |

**Phase 7+ Enhancement**: Fingerprint history tracking ("verified since date X") and automatic unverified-contact warnings are Phase 7+ features, documented in [Deferred Items Inventory](../../deferred-items-inventory.md) Category H.

