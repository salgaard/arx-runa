---
applyTo: "src-tauri/src/sharing/**,src-tauri/src/storage/sharing.rs"
---


# Sharing module — rules

**Design specification**: `docs/architecture/designs/file-sharing/design.md`

## Trait boundaries
- `SharingStore` is the sharing storage boundary in `sharing::store`.
- Never extend `MetadataStore` with sharing-specific methods.

## Identity ownership
- `sharing::` treats `vault_identity` as read-only.
- Identity generation stays in `auth::ceremonies::create::create_vault`.

## Fingerprint contract
- Fingerprint is the first 8 bytes of `SHA-256(public_key)`.
- Render fingerprint as 16 lowercase hex characters.

## Logging and debug hygiene
- Never log X25519 public-key bytes.
- `Debug` output must not print raw public-key bytes.

## Storage placement
- Contacts CRUD lives in `storage::sharing`.
- Do not colocate sharing CRUD in `storage::sqlcipher`; mirror the destination-session split pattern.

