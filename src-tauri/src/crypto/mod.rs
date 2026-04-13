//! Arx Runa crypto module.
//!
//! Cryptographic primitives: key derivation, chunk encryption, file key
//! management, BLAKE3 checksums.

pub mod decrypt_chunk;
pub mod encrypt_chunk;
pub mod error;
pub mod hkdf;
pub mod nonce;
pub mod types;

pub use decrypt_chunk::decrypt_chunk;
pub use encrypt_chunk::encrypt_chunk;
pub use error::CryptoError;
pub use hkdf::{VaultKeys, derive_vault_keys};
pub use nonce::generate_nonce;
pub use types::{
    Blake3Hash, ChunkIndex, FileId, FileKey, KeyEncryptionKey, ManifestKey, SqlcipherKey,
    WrappedFileKey,
};
