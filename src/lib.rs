//! Arx Runa Leptos frontend — library crate.
//!
//! Re-exports the IPC wrapper, error type, mirrored DTOs, and state providers
//! used by the binary at `src/main.rs` and by Phase 6.3 page components.

pub mod app;
pub mod error;
pub mod invoke;
pub mod ipc_types;
pub mod state;

pub use app::App;
pub use error::IpcError;
pub use invoke::invoke_command;
