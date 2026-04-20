//! Sharing module surface for vault identity, contacts, and share packages.

#[allow(dead_code)] // TODO(phase-6): remove when Tauri commands consume ctx_aead
mod ctx_aead;
mod error;
#[allow(dead_code)] // TODO(phase-6): remove when Tauri commands consume hpke
mod hpke;
mod identity;
#[allow(dead_code)] // TODO(phase-6): remove when Tauri commands consume packages
mod packages;
mod store;
mod types;

pub use error::SharingError;
pub use identity::{compute_fingerprint, export_public_key_bytes, public_key_qr_string};
#[allow(unused_imports)] // TODO(phase-6): remove when Tauri commands wire these
pub(crate) use packages::{create_share_package, import_share_package};
pub use store::{Contact, ReceivedShare, SharingStore};
pub use types::{ContactId, DisplayName, Fingerprint, X25519PublicKey};
