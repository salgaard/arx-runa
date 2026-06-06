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

/// Fixed filename for the single delivery-receipt blob inside a share's
/// `receipts/` and `import-receipts/` folders.
///
/// Google Drive service accounts have no storage quota and cannot *create*
/// files in the owner's personal My Drive, but they *can* update the content of
/// an existing owner-owned file (billed to the owner). So the owner pre-creates
/// this blob once and the recipient overwrites it in place. B2 does not need
/// this and continues to use per-event UUID blob names.
pub(crate) const RECEIPT_BLOB_NAME: &str = "receipt.blob";

/// Content the owner writes when pre-creating a receipt blob. The receipt
/// reader skips any blob whose plaintext equals this sentinel, so the empty
/// placeholder is ignored until the recipient overwrites it with a real,
/// HPKE-sealed receipt.
pub(crate) const RECEIPT_PLACEHOLDER: &[u8] = b"ARX_RECEIPT_PLACEHOLDER_V1";
