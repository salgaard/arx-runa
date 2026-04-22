//! Sharing module surface for vault identity, contacts, and share packages.

#[allow(dead_code)] // TODO(phase-6): remove when Tauri commands consume cloud
pub(crate) mod cloud;
#[allow(dead_code)] // TODO(phase-6): remove when Tauri commands consume ctx_aead
mod ctx_aead;
mod error;
#[allow(dead_code)] // TODO(phase-6): remove when Tauri commands consume hpke
mod hpke;
mod identity;
mod packages;
mod revocation;
mod store;
mod types;

pub use error::SharingError;
pub use identity::{compute_fingerprint, export_public_key_bytes, public_key_qr_string};
pub(crate) use packages::{create_share_package, import_share_package};
#[allow(unused_imports)] // Phase 7: strong revocation re-keying
pub(crate) use revocation::{
    ReissuedPackage, StrongRevocationOutput, revoke_share, strong_revoke_share,
};
pub use store::{Contact, ReceivedShare, ShareRecord, SharingStore};
pub use types::{ContactId, DisplayName, Fingerprint, X25519PublicKey};
