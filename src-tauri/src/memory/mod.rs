//! Arx Runa memory module.
//!
//! Memory protection utilities: mlock/VirtualLock wrappers, zeroisation helpers.
//! Primary consumer: `auth` module (Phase 2) for session key memory locking.

pub mod error;
#[allow(dead_code)]
pub(crate) mod platform;
#[allow(dead_code)]
pub(crate) mod secure_buffer;
pub mod types;

pub use error::MemoryLockError;
#[allow(unused_imports)]
pub(crate) use secure_buffer::SecureBytes;
