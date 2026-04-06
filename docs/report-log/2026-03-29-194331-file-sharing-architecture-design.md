---
timestamp: "2026-03-29T19:43:31+0200"
type: decision
report-sections:
  - method
  - discussion
tags: [sharing, per-file-keys, ecies, x25519, key-management, revocation]
source: agent
commit: "5d71df7"
---

## File Sharing Architecture — Key Design Decisions

## Context

Arx Runa requires a file sharing mechanism that preserves the zero-trust model: the cloud provider must never hold key material, sharing must not require a central authority, and access control must operate at file granularity. The base architecture used a single vault-wide `chunk_key` derived from `master_key` via HKDF, which is incompatible with per-file access delegation. Sub-question 5 of the problem formulation was revised to make file sharing the primary implementation target rather than a speculative extensibility discussion.

## Substance

### Per-file random keys

The single vault-wide `chunk_key` was replaced with a two-level key structure:

1. `key_encryption_key` — HKDF-SHA256 derived from `master_key` (info: `"voidgate-key-encryption"`); replaces the `chunk_key` HKDF branch
2. `file_key` — random 256-bit key generated per file via CSPRNG; stored wrapped (encrypted) with `key_encryption_key` in the SQLCipher `nodes` table (per file, not per chunk — see [chunking design report](2026-03-29-211003-chunking-manifest-design.md) for schema evolution)

All chunk encryption uses `file_key`. The `key_encryption_key` is never used directly for chunk encryption — it only wraps and unwraps `file_key` values. This is the standard key-wrapping pattern used by LUKS key slots and age.

This change was adopted proactively in Phase 1 (cryptographic primitives), not deferred to the sharing phase, because retrofitting later would require touching the encrypt/decrypt pipeline and manifest schema twice.
<!-- CITE: LUKS key slot design — dm-crypt/LUKS documentation or original LUKS paper -->

An additional benefit: secure single-file deletion is now achievable by destroying the `file_key` record from SQLCipher — without rekeying the vault or affecting other files.

### ECIES for share package encryption

Share packages are encrypted using the recipient's X25519 public key via ECIES:

1. Owner generates an ephemeral X25519 keypair
2. ECDH between ephemeral private key and recipient's long-term public key → shared secret
3. `HKDF-SHA256(shared_secret, salt=ephemeral_public_key, info="voidgate-share")` → symmetric key
4. XChaCha20-Poly1305 encrypts the share package content
5. Wire format: `[32B ephemeral_public_key | 24B nonce | ciphertext | 16B Poly1305 tag]`

"Encrypted with recipient's X25519 public key" is a common shorthand but imprecise: X25519 is key agreement, not encryption. Specifying the full ECIES construction is necessary for a correct implementation. The `age` encryption tool uses the same construction and is widely regarded as a reference implementation of this pattern.
<!-- SOURCE: age encryption format — https://github.com/FiloSottile/age — X25519 recipient type uses ephemeral keypair + HKDF + ChaCha20-Poly1305, identical construction -->
<!-- CITE: RFC 7748 — Elliptic Curves for Security (X25519 definition) -->

### Shared blob storage

The initial design proposed duplicating blobs per recipient (one copy per share). This was replaced with shared blob storage: one copy of the file's encrypted blobs in `shared/<file_share_id>/`, with multiple recipients each holding a share package whose `file_key_wrapped` field is encrypted for their specific public key.

The duplication model's apparent advantage — instant revocation by folder deletion — does not hold if recipients have already fetched the blobs. Revocation is always re-encryption in the general case. Given this, shared blob storage is strictly preferable: storage cost is O(1) per shared file regardless of recipient count, vs O(n) for duplication.

### Revocation model

Revocation is honest about what is achievable:

- **Before recipient fetch**: delete `shared/<file_share_id>/` — future access is prevented
- **After recipient fetch**: plaintext already held by recipient cannot be recalled; re-encryption under a new `file_key` with new blob upload and re-sharing to remaining recipients provides a stronger guarantee

This is the correct model for any key-based sharing system. The alternative — claiming that link deletion constitutes access revocation — is a common source of false assurance in practice.

### Snapshot share semantics

A share is a snapshot at the time of sharing. The `chunk_uuids` list in the share package is static. File updates by the owner do not propagate to existing shares. This is an explicit design decision, not an oversight. Live sharing would require a directory-level share agreement rather than a package exchange, and is deferred.

### Cloud authentication for recipients

Blobs in `shared/<file_share_id>/` are publicly readable. The share package contains an Rclone connection descriptor (provider, bucket, region, endpoint, share path) but no credentials. Justification: blobs are AEAD ciphertext — without `file_key`, the ciphertext is permanently inaccessible. UUID v4 blob names (122 bits of entropy) are not enumerable. The cloud provider already has read access to all blobs by design.
<!-- CITE: Birthday bound for UUID v4 collision — probability analysis for 2^122 keyspace -->

### Identity model

Arx Runa generates an X25519 keypair on first run. Public keys are exchanged out-of-band (file export, QR code). No email infrastructure, no central directory, no sign-up. The trust assumption is explicit: security of key exchange is as strong as the out-of-band channel. MITM is mitigated by optional fingerprint verification (short hash of public key compared over a separate channel).

This is the same trust model as WireGuard, age, and PGP.
<!-- CITE: WireGuard whitepaper — key exchange model and out-of-band trust assumption -->

## Alternatives considered

### Vault-wide chunk_key with separate share key

Retaining `chunk_key` and adding a parallel key for sharing was considered. Rejected: this would require re-encrypting chunks at share time and introduces two code paths for chunk encryption. Per-file keys are a simpler and more capable base.

### share_key intermediate layer

An intermediate `share_key` (file_key → wrapped with share_key → share_key wrapped with recipient public key) was present in the initial design. Removed: the intermediate layer provides no security property over direct ECDH-derived wrapping of `file_key`. The multi-recipient efficiency argument (wrap file_key once) was evaluated and found negligible (wrapping a 256-bit key takes microseconds).

### Blob duplication per recipient

Each share creates a separate `shared/<share_id>/` folder. Revocation is blob deletion. Rejected: storage cost grows linearly with recipient count; the revocation advantage disappears once a recipient has fetched the blobs; re-encryption is required in both models for the stronger revocation guarantee.

### Central sharing server

A relay or directory server for key exchange and blob mediation. Rejected: violates the "bring your own cloud, no server" design pillar and introduces an infrastructure dependency.

## Implications

- Phase 1 (cryptographic primitives) now includes per-file key generation and wrapping, not just HKDF derivation of vault-level keys
- Phase 5 is a new roadmap phase dedicated to identity and file sharing implementation
- Sub-question 5 of the problem formulation is revised to: "What cryptographic and protocol-level challenges arise when enabling file-granularity sharing between independent users in a zero-trust client-side encrypted system, and how does the proposed sharing architecture compare to existing approaches such as OneDrive sharing links and Cryptomator shared vaults?"
- The `nodes` table gains a `file_key_wrapped` column (originally proposed for `chunks` table, moved to `nodes` in [chunking design](2026-03-29-211003-chunking-manifest-design.md))
- A new `src-tauri/src/sharing/` module is introduced
- Open decisions: enterprise key distribution, fingerprint verification UX, folder sharing UX, identity keypair rotation on USB key file replacement

## References

<!-- SOURCE: age encryption format — https://github.com/FiloSottile/age — X25519 recipient type: ephemeral keypair + ECDH + HKDF + ChaCha20-Poly1305; reference implementation of the ECIES construction used by Arx Runa sharing -->
<!-- CITE: RFC 7748 — Elliptic Curves for Security — defines X25519 Diffie-Hellman function -->
<!-- CITE: HMAC-based Extract-and-Expand Key Derivation Function (HKDF) — RFC 5869 — used in ECIES key derivation step -->
<!-- CITE: LUKS (Linux Unified Key Setup) key slot design — LUKS1 specification or cryptsetup documentation — reference for key-wrapping pattern -->
<!-- CITE: WireGuard whitepaper — Jason Donenfeld — out-of-band public key exchange model -->
