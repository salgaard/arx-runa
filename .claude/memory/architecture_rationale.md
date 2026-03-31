---
name: Architecture rationale
description: Why each major design decision was made — rejected alternatives, trade-offs
type: project
---

## Cipher: XChaCha20-Poly1305
Rejected AES-GCM: catastrophic nonce-reuse failure, hardware dependency. XChaCha20: 192-bit random nonce safe per-chunk, less severe nonce-reuse, constant-time ARX. Trade-off: slower on AES-NI hardware. (RFC 8439, draft-irtf-cfrg-xchacha)

## Nonces: random 192-bit CSPRNG
Birthday bound 2⁹⁶ — negligible collision. Rejected counters (state loss = reuse), metadata-derived (re-encrypt = reuse).

## Wire: [24B nonce | ciphertext | 16B tag]
Self-contained chunks. Crate returns ct||tag; we prepend nonce.

## AAD: file_id || chunk_index
Prevents chunk reorder/swap by malicious cloud. Without AAD, any chunk decrypts with correct key regardless of position.

## USB: key file (32B entropy) not serial
Serial = identifier, spoofable. Key file + password → Argon2id. Rejected TOTP (non-deterministic). Recovery via Recovery Phrase.

## HKDF tree (RFC 5869)
master_key → {key_encryption_key, sqlcipher_key, manifest_key}. Key separation: one compromise ≠ total compromise. (LUKS, Signal pattern)

## Manifest: SQLCipher
Rejected JSON (no queries, not crash-safe), sled (more code). Filenames plaintext inside encrypted DB — no double-encryption.

## Manifest backup: encrypted blob + unencrypted header
Header: vault_id, salt, argon2_params (needed before keys exist). Snapshot model: full export, snapshot_counter for future conflict detection.

## Blob names: UUID v4
No correlation to files. Cloud cannot infer structure.

## BLAKE3: over encrypted blob
Pre-decrypt integrity check. Not security (AEAD handles auth) — operational UX.

## Deletion: immediate
Blobs removed, CASCADE cleans manifest. UI confirms before permanent delete.

## Chunks: fixed-size uniform padding
CDC leaks size. Storage overhead accepted — quantified in report.

## Memory: mlock + zeroize + secrecy
Does NOT protect cold boot or compromised kernel — explicit boundary.

## Rust
Deterministic memory (no GC) — critical for Zero-Trace. GC languages retain plaintext copies.
