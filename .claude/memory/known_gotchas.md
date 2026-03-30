---
name: Known gotchas
description: Implementation pitfalls discovered during development
type: project
---

- The `chacha20poly1305` crate returns ciphertext || tag as one blob from
  `encrypt()` — do not manually append the tag or you will double it
- AAD mismatch between encrypt and decrypt will cause silent auth failure —
  ensure file_id and chunk_index are serialised identically on both paths
- Vault header must be uploaded BEFORE the manifest blob — a new device
  needs the salt first to derive keys
- BLAKE3 checksum is over the encrypted blob (nonce + ciphertext + tag),
  not over plaintext — verify checksum before attempting decryption
