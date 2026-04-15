//! Session-key container and lifecycle manager.
//!
//! This module separates key derivation/expansion (`SessionKeys`) from session
//! lifecycle orchestration (`SessionManager`) while keeping the existing public
//! API stable for auth and ceremony flows.

mod keys;
mod manager;

pub(crate) use keys::SessionKeys;
pub use manager::{LifecycleState, OperationGuard, SessionEvent, SessionManager};
