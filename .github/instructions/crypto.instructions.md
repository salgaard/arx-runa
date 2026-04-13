---
applyTo: "src-tauri/src/crypto/**"
---


# Crypto module — rules

**Design specification**: `docs/architecture/designs/cryptographic-primitives/design.md` — last verified against design dated 2026-04-01

## Cipher
- `XChaCha20Poly1305` only (not `ChaCha20Poly1305`) — 192-bit nonce
- AES-GCM rejected for this project

## Nonces
- 24 bytes via CSPRNG per chunk — never sequential/derived

## AAD
- Every chunk encrypt/decrypt: AAD = `file_id || chunk_index` (big-endian)
- Singleton blobs follow design-specific AAD rules (`file_key_wrapped` uses empty AAD; manifest backup uses no AAD)
- Recovery slot wrapping uses **mandatory non-empty AAD**: `b"arx-runa recovery v1" || vault_id_bytes` — binds ciphertext to vault and purpose; use dedicated `wrap_master_key_for_recovery` / `unwrap_master_key_from_recovery` functions, not `wrap_file_key`
- Chunk AAD mismatch = silent auth failure; missing chunk AAD enables swap/reorder attacks

## Wire format
- `[24B nonce | ciphertext | 16B tag]`
- BLAKE3 checksum over encrypted blob (not plaintext)
- `verify_checksum` returns `VerifiedBlob`; `decrypt_chunk` accepts only `VerifiedBlob` — skipping the check is a compile error

## Key derivation
- Never use `master_key` directly — derive via HKDF
- Per-file: random `file_key` wrapped with `key_encryption_key`
- See `docs/architecture/designs/authentication-and-session-management/design.md` for HKDF tree

## Memory
- All keys: `ZeroizeOnDrop` + `SecretBox<[u8; 32]>`
- Encrypt/decrypt in-place — no plaintext copies

## HKDF info strings
- Every derived key needs a globally unique `info` string — reuse causes silent key collision (two purposes share the same key material)
- Verify against the HKDF expansion table in `docs/architecture/designs/cryptographic-primitives/design.md` before adding a new one
- Format: `b"arx-runa-<purpose>"`

## Argon2id parameters
- m = 65536 KiB, t = 3, p = 4 (RFC 9106 §4 recommended tier)
- See `docs/architecture/designs/authentication-and-session-management/design.md` for full derivation context
