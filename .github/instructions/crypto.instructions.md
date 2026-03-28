---
applyTo: "src-tauri/src/crypto/**"
---

# Crypto module — scoped instructions

These rules apply to all files under `src-tauri/src/crypto/`.

## Cipher
- Use `XChaCha20Poly1305` from the `chacha20poly1305` crate — not `ChaCha20Poly1305`
  (the non-extended variant has a 96-bit nonce, insufficient for random generation)
- Never use any other cipher — AES-GCM is explicitly rejected for this project

## Nonces
- Every nonce must be 24 bytes (192-bit), generated fresh per chunk via CSPRNG:
  ```rust
  let mut nonce_bytes = [0u8; 24];
  rand::thread_rng().fill_bytes(&mut nonce_bytes);
  let nonce = XNonce::from(nonce_bytes);
  ```
- Never use sequential counters, timestamps, or metadata-derived nonces

## AAD (Authenticated Associated Data)
- Every `encrypt` and `decrypt` call MUST pass AAD = `file_id || chunk_index`
- Serialise file_id and chunk_index identically on both paths — a mismatch
  causes silent authentication failure
- Missing AAD enables chunk reordering/swapping attacks by a cloud provider

## Wire format
- Encrypted chunk = `[24-byte nonce | ciphertext | 16-byte Poly1305 tag]`
- The `encrypt()` call returns `ciphertext || tag` as one blob — prepend the
  nonce separately. Do NOT append the tag again or it will be doubled.
- BLAKE3 checksum is computed over the full encrypted blob (nonce + ciphertext
  + tag), not over plaintext

## Key derivation
- master_key must NEVER be used directly for encryption
- Use HKDF-SHA256 to derive: chunk_key, sqlcipher_key, manifest_key
- Each derived key uses a distinct `info` parameter (see CLAUDE.md for values)
- Compromise of one derived key must not compromise others

## Memory
- All key types must implement `ZeroizeOnDrop`
- Wrap key material in `secrecy::Secret<T>` to prevent accidental logging
- Encrypt/decrypt in-place on mutable buffers — do not create a second
  plaintext copy

## Argon2id parameters (minimum)
- memory: m >= 19456
- iterations: t >= 2
- parallelism: p = 1

## Required tests (every function in this module)
- Round-trip encrypt -> decrypt returns original plaintext
- AAD mismatch on decrypt returns error
- Wrong key returns error
- Corrupted ciphertext returns error
- Tag tampering returns error
- Two encryptions of the same plaintext produce different ciphertexts
- Key material is zeroed after drop
