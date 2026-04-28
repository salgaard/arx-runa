---
paths:
  - "src-tauri/src/crypto/**"
---

# Crypto

> Design: `docs/architecture/designs/cryptographic-primitives/design.md`

- Cipher: `XChaCha20Poly1305` only (192-bit nonce); AES-GCM rejected
- Nonces: 24 bytes via CSPRNG per chunk — never sequential/derived
- Chunk AAD: `file_id || chunk_index` (big-endian); singletons: `file_key_wrapped` = empty AAD, manifest backup = no AAD
- Recovery slot AAD (mandatory): `b"arx-runa recovery v1" || vault_id_bytes` — use `wrap_master_key_for_recovery`/`unwrap_master_key_from_recovery`, never `wrap_file_key`
- Chunk AAD mismatch = silent auth failure; missing chunk AAD enables swap/reorder attacks
- Wire format: `[24B nonce | ciphertext | 16B tag]`; BLAKE3 checksum over encrypted blob (not plaintext)
- `verify_checksum` returns `VerifiedBlob`; `decrypt_chunk` accepts only `VerifiedBlob` — skipping is a compile error
- Key derivation: never use `master_key` directly — derive via HKDF; per-file: random `file_key` wrapped with `key_encryption_key`
- HKDF info strings: globally unique per derived key — reuse = silent key collision; format `b"arx-runa-<purpose>"`; verify against HKDF table in design doc before adding
- Argon2id: m=65536 KiB, t=3, p=4 (RFC 9106 §4)
- Memory/safety: see rust.md
