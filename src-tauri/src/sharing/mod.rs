//! Sharing module surface for vault identity, contacts, and share packages.

pub(crate) mod b2_api;
pub(crate) mod cloud;
mod ctx_aead;
mod error;
pub(crate) mod gdrive_api;
pub(crate) mod hpke;
mod identity;
mod packages;
mod revocation;
mod store;
mod types;

pub use error::SharingError;
pub use identity::{compute_fingerprint, export_public_key_bytes, public_key_qr_string};
#[cfg(test)]
pub(crate) use packages::create_share_package;
pub(crate) use packages::import_share_package;
pub(crate) use revocation::revoke_share;
pub use store::{Contact, FileShareSnapshot, ReceivedShare, ShareRecord, SharingStore};
pub use types::{ContactId, DisplayName, Fingerprint, X25519PublicKey};
