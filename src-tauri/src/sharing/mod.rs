//! Sharing module surface for vault identity and contacts.

mod error;
mod identity;
mod store;
mod types;

pub use error::SharingError;
pub use identity::{compute_fingerprint, export_public_key_bytes, public_key_qr_string};
pub use store::{Contact, SharingStore};
pub use types::{ContactId, DisplayName, Fingerprint, X25519PublicKey};
