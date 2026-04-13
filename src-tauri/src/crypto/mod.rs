//! Arx Runa crypto module.
//!
//! Cryptographic primitives: key derivation, chunk encryption, file key
//! management, BLAKE3 checksums.

pub mod checksum;
pub mod decrypt_chunk;
pub mod encrypt_chunk;
pub mod error;
pub mod generate_file_key;
pub mod hkdf;
pub mod nonce;
pub mod types;
pub mod wrap_key;

pub use checksum::{VerifiedBlob, compute_checksum, verify_checksum};
pub use decrypt_chunk::decrypt_chunk;
pub use encrypt_chunk::encrypt_chunk;
pub use error::CryptoError;
pub use generate_file_key::generate_file_key;
pub use hkdf::{VaultKeys, derive_vault_keys};
pub use nonce::generate_nonce;
pub use types::{
    Blake3Hash, ChunkIndex, FileId, FileKey, KeyEncryptionKey, ManifestKey, SqlcipherKey,
    WrappedFileKey,
};
pub use wrap_key::{unwrap_file_key, wrap_file_key};
