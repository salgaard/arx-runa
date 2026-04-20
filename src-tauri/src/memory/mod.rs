//! Arx Runa memory module.
//!
//! Memory protection utilities: mlock/VirtualLock wrappers, zeroisation helpers.
//! Primary consumer: `auth` module (Phase 2) for session key memory locking.

pub mod error;
pub(crate) mod platform;
pub(crate) mod secure_buffer;
pub mod types;

pub use error::MemoryLockError;
pub(crate) use secure_buffer::SecureBytes;
