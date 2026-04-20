//! Domain types for the auth module.
//!
//! Re-exports domain types defined alongside their owners so external callers
//! can import from a single stable path.

pub use crate::auth::device_monitor::DeviceEvent;
pub use crate::auth::error::KeySourceError;
pub use crate::auth::path_hint::VaultHint;
