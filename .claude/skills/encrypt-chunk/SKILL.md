---
name: encrypt-chunk
description: Implement XChaCha20-Poly1305 chunk encryption for VoidGate. Use when writing or modifying encrypt_chunk or decrypt_chunk in src-tauri/src/crypto/.
---

Implement chunk encryption following the canonical specifications in `docs/architecture/designs/cryptographic-primitives/design.md`. Do not deviate from the wire format or AAD construction — both sides of the encrypt/decrypt pair must match identically.

**Implementation checklist:**

1. **AAD construction**: `file_id (16 bytes) || chunk_index (4 bytes, big-endian)` — see design doc section 3.2
2. **Nonce generation**: 24 bytes via CSPRNG (never sequential, never derived) — see design doc section 3.1
3. **Cipher**: `XChaCha20Poly1305` (not `ChaCha20Poly1305`) — see design doc section 2.1
4. **Wire format**: `[24B nonce | ciphertext | 16B tag]` — see design doc section 3
5. **BLAKE3**: Hash encrypted blob (not plaintext) — see design doc section 4.3
6. **Key handling**: Use `file_key` (per-file random key), never `key_encryption_key` directly

**Reference implementation pattern:**

```rust
fn build_aad(file_id: Uuid, chunk_index: u32) -> [u8; 20] {
    let mut aad = [0u8; 20];
    aad[..16].copy_from_slice(file_id.as_bytes());
    aad[16..].copy_from_slice(&chunk_index.to_be_bytes()); // big-endian, both sides
    aad
}

fn encrypt_chunk(...) -> Result<EncryptedChunk, EncryptionError> {
    // 1. Generate nonce via CSPRNG
    let mut nonce_bytes = [0u8; 24];
    rand::thread_rng().fill_bytes(&mut nonce_bytes);
    let nonce = XNonce::from(nonce_bytes);
    
    // 2. Build AAD
    let aad = build_aad(file_id, chunk_index);
    
    // 3. Encrypt with AAD
    let cipher = XChaCha20Poly1305::new(file_key.expose_secret().into());
    let ciphertext_and_tag = cipher.encrypt(&nonce, Payload { msg: plaintext, aad: &aad })?;
    
    // 4. Assemble wire format: [nonce | ciphertext | tag]
    let mut wire_blob = Vec::with_capacity(24 + ciphertext_and_tag.len());
    wire_blob.extend_from_slice(&nonce_bytes);
    wire_blob.extend_from_slice(&ciphertext_and_tag); // already includes tag
    
    // 5. Compute BLAKE3 checksum
    let checksum = blake3::hash(&wire_blob);
    
    Ok(EncryptedChunk { wire_blob, checksum })
}

fn decrypt_chunk(...) -> Result<Vec<u8>, DecryptionError> {
    // 1. Verify BLAKE3 before decryption
    let computed = blake3::hash(wire_blob);
    if computed.as_bytes() != expected_checksum {
        return Err(DecryptionError::ChecksumMismatch);
    }
    
    // 2. Parse wire format
    let (nonce_bytes, ciphertext_and_tag) = wire_blob.split_at(24);
    
    // 3. Rebuild AAD (must match encryption side exactly)
    let aad = build_aad(file_id, chunk_index);
    
    // 4. Decrypt with AAD verification
    let cipher = XChaCha20Poly1305::new(file_key.expose_secret().into());
    let plaintext = cipher.decrypt(XNonce::from_slice(nonce_bytes), Payload { msg: ciphertext_and_tag, aad: &aad })?;
    
    Ok(plaintext)
}
```

**Common pitfalls:**
- AAD endianness mismatch causes silent auth failure
- Appending tag twice (encrypt() already returns ciphertext || tag)
- BLAKE3 over plaintext instead of encrypted blob
- Using `key_encryption_key` instead of `file_key`

**After implementing:** Invoke the security-reviewer agent on the new file.

**See also:**
- `docs/architecture/designs/cryptographic-primitives/design.md` — full wire format, AAD, and nonce specifications
