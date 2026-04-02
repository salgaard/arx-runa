---
paths:
  - "src-tauri/src/crypto/**"
---

# Crypto module — rules

**Design specification**: `docs/architecture/designs/cryptographic-primitives/design.md`

## Cipher
- `XChaCha20Poly1305` only (not `ChaCha20Poly1305`) — 192-bit nonce
- AES-GCM rejected for this project

## Nonces
- 24 bytes via CSPRNG per chunk — never sequential/derived

## AAD
- Every encrypt/decrypt: AAD = `file_id || chunk_index` (big-endian)
- Mismatch = silent auth failure; missing = chunk swap attacks possible

## Wire format
- `[24B nonce | ciphertext | 16B tag]`
- BLAKE3 checksum over encrypted blob (not plaintext)

## Key derivation
- Never use `master_key` directly — derive via HKDF
- Per-file: random `file_key` wrapped with `key_encryption_key`
- See `docs/architecture/designs/authentication-and-session-management/design.md` for HKDF tree

## Memory
- All keys: `ZeroizeOnDrop` + `Secret<T>`
- Encrypt/decrypt in-place — no plaintext copies

## Argon2id minimums
- m ≥ 19456, t ≥ 2, p = 1
- See `docs/architecture/designs/authentication-and-session-management/design.md` for full parameters
