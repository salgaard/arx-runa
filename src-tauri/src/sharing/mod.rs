//! Sharing module surface for vault identity, contacts, and share packages.

pub(crate) mod b2_api;
#[allow(dead_code)] // TODO(phase-6): remove when Tauri commands consume cloud
pub(crate) mod cloud;
#[allow(dead_code)] // TODO(phase-6): remove when Tauri commands consume ctx_aead
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
#[allow(unused_imports)]
pub(crate) use packages::create_share_package;
pub(crate) use packages::import_share_package;
#[allow(unused_imports)] // Phase 7: strong revocation re-keying
pub(crate) use revocation::{
    ReissuedPackage, StrongRevocationOutput, revoke_share, strong_revoke_share,
};
pub use store::{Contact, ReceivedShare, ShareRecord, SharingStore};
pub use types::{ContactId, DisplayName, Fingerprint, X25519PublicKey};
