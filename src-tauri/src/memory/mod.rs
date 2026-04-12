//! Arx Runa memory module.
//!
//! Memory protection utilities: mlock/VirtualLock wrappers, zeroisation helpers.
//! Primary consumer: `auth` module (Phase 2) for session key memory locking.

pub mod error;
pub mod types;
