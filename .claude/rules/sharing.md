---
paths:
  - "src-tauri/src/sharing/**"
  - "src-tauri/src/storage/sharing.rs"
---

# Sharing

> Design: `docs/architecture/designs/file-sharing/design.md`

- `SharingStore` is the sharing storage boundary in `sharing::store`; never extend `MetadataStore` with sharing-specific methods
- `sharing::` treats `vault_identity` as read-only; identity generation stays in `auth::ceremonies::create::create_vault`
- Fingerprint: first 8 bytes of `SHA-256(public_key)`, rendered as 16 lowercase hex chars
- Never log X25519 public-key bytes; `Debug` must not print raw public-key bytes
- All HPKE open failures (KEM decap, CTX commitment mismatch, stream decrypt) emit `SharingError::AuthenticationFailed` with no source context; error text must not include `enc`, ciphertext, or CTX tag bytes
- `SharingError::AuthenticationFailed` → `IpcError::AuthenticationFailed` with fixed user-safe string; `ipc` adapters must never include KEM/CTX context bytes
- Contacts CRUD in `storage::sharing`; do not colocate in `storage::sqlcipher` — mirror destination-session split
- Revocation blob deletion: sequential loop; on failure return `SharingError::RevocationPartial { failed_index }` with `shares.revoked_at` unchanged (retryable)
- Strong revocation: rotate `file_key` and `file_share_id` atomically at manifest layer (`replace_file_key_and_chunks`) before shared-folder cleanup
