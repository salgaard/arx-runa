---
name: encrypt-chunk
description: Implement XChaCha20-Poly1305 chunk encryption for VoidGate. Use when writing or modifying encrypt_chunk or decrypt_chunk in src-tauri/src/crypto/.
---

Implement chunk encryption following this exact procedure. Do not deviate from the wire format or AAD construction — both sides of the encrypt/decrypt pair must match identically.

**1. Construct AAD as `file_id bytes (16) || chunk_index to_be_bytes u64 (8)`:**
```rust
fn build_aad(file_id: Uuid, chunk_index: u64) -> [u8; 24] {
    let mut aad = [0u8; 24];
    aad[..16].copy_from_slice(file_id.as_bytes());
    aad[16..].copy_from_slice(&chunk_index.to_be_bytes());
    aad
}
```
Use `to_be_bytes()` consistently on both encrypt and decrypt. An endianness mismatch causes silent authentication failure.

**2. Generate a 24-byte nonce via CSPRNG — never sequential, never derived:**
```rust
let mut nonce_bytes = [0u8; 24];
rand::thread_rng().fill_bytes(&mut nonce_bytes);
let nonce = XNonce::from(nonce_bytes);
```

**3. Encrypt using `XChaCha20Poly1305` (not `ChaCha20Poly1305`) with AAD:**
```rust
let cipher = XChaCha20Poly1305::new(file_key.expose_secret().into());
let ciphertext_and_tag = cipher.encrypt(&nonce, Payload { msg: plaintext, aad: &aad })
    .map_err(|_| EncryptionError::AeadFailed)?;
```
`encrypt()` returns `ciphertext || tag` as one blob. Do not append the tag again.
Note: `file_key` is the per-file random 256-bit key, fetched from SQLCipher and
unwrapped with `key_encryption_key` before use. Never pass `key_encryption_key`
directly to `XChaCha20Poly1305`.

**4. Assemble wire format `[24B nonce | ciphertext | 16B Poly1305 tag]`:**
```rust
let mut wire_blob = Vec::with_capacity(24 + ciphertext_and_tag.len());
wire_blob.extend_from_slice(&nonce_bytes);
wire_blob.extend_from_slice(&ciphertext_and_tag);
```

**5. Compute BLAKE3 over the encrypted blob — not over plaintext:**
```rust
let checksum = blake3::hash(&wire_blob);
```
Store `checksum.as_bytes()` in the manifest's `chunks` table. This is an integrity check for cloud corruption, not an auth primitive — the AEAD tag handles authenticity.

**6. On decrypt, verify BLAKE3 before touching the ciphertext:**
```rust
let computed = blake3::hash(wire_blob);
if computed.as_bytes() != expected_checksum {
    return Err(DecryptionError::ChecksumMismatch);
}
let (nonce_bytes, ciphertext_and_tag) = wire_blob.split_at(24);
let aad = build_aad(file_id, chunk_index);
let plaintext = cipher.decrypt(XNonce::from_slice(nonce_bytes), Payload { msg: ciphertext_and_tag, aad: &aad })
    .map_err(|_| DecryptionError::AeadFailed)?;
```

After implementing, invoke the security-reviewer agent on the new file.
