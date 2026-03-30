# Code structure reference

On-demand reference. Load when creating new modules or scaffolding files.

Structure is applied incrementally as each roadmap phase is implemented.

## Module layout
Each module under `src-tauri/src/` follows this structure:
```
module_name/
├── mod.rs          # Public API re-exports only (minimal)
├── error.rs        # Module-specific thiserror enum
├── types/          # Newtype wrappers (one file per type)
│   ├── mod.rs
│   └── type_name.rs
└── concern.rs      # One file per concern (e.g., encrypt_chunk.rs)
```

## File granularity
One-concern-per-file — each `.rs` file focuses on a single type, trait, or
function. Examples:
- `encrypt_chunk.rs` — contains `encrypt_chunk` function and its helpers
- `key_source.rs` — contains `KeySource` trait and `FileKeySource` impl
- `types/file_key.rs` — contains `FileKey` newtype only

## Re-exports
`mod.rs` exposes only the public API. Internal helpers, intermediate types,
and implementation details stay private:
```rust
// mod.rs
mod encrypt_chunk;
mod decrypt_chunk;
mod error;
mod types;

pub use encrypt_chunk::encrypt_chunk;
pub use decrypt_chunk::decrypt_chunk;
pub use error::CryptoError;
pub use types::{FileKey, KeyEncryptionKey};
// Internal helpers NOT re-exported
```

## Newtype location
Domain-specific types live in `types/` subfolders:
- `crypto/types/` — `FileKey`, `KeyEncryptionKey`, `SqlcipherKey`, `ManifestKey`
- `storage/types/` — `ChunkIndex`, `FileId`, `NodeId`, `BlobName`, `VaultId`
- `auth/types/` — `SessionKeys`, `VaultHeader`
- `sharing/types/` — `ContactId`, `SharePackage`

## Target structure (end state)
```
src-tauri/src/
├── main.rs
├── lib.rs              # Crate root, re-exports public module APIs
├── crypto/
│   ├── mod.rs, error.rs
│   ├── types/ (file_key.rs, key_encryption_key.rs, ...)
│   ├── encrypt_chunk.rs, decrypt_chunk.rs
│   ├── hkdf.rs, nonce.rs, checksum.rs
├── auth/
│   ├── mod.rs, error.rs
│   ├── types/ (session_keys.rs, vault_header.rs)
│   ├── key_source.rs, device_monitor.rs, session.rs, timeout.rs, argon2.rs
├── storage/
│   ├── mod.rs, error.rs
│   ├── types/ (chunk_index.rs, file_id.rs, node_id.rs, blob_name.rs, vault_id.rs)
│   ├── manifest.rs, cloud.rs, chunking.rs, sync.rs
├── sharing/
│   ├── mod.rs, error.rs
│   ├── types/ (contact_id.rs, share_package.rs)
│   ├── identity.rs, contacts.rs, revocation.rs
└── ui/
    ├── mod.rs, error.rs
    ├── commands/ (auth.rs, files.rs, sharing.rs)
    └── state.rs
```
