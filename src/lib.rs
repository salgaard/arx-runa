//! Arx Runa Leptos frontend — library crate.
//!
//! Re-exports the IPC wrapper, error type, mirrored DTOs, state providers,
//! page components, and extern shims used by the binary at `src/main.rs`.

pub mod app;
pub mod auth;
pub mod components;
pub mod contacts;
pub mod destinations;
pub mod dialog;
pub mod drag_drop;
pub mod error;
pub mod invoke;
pub mod ipc_channel;
pub mod ipc_types;
pub mod layout;
pub mod settings;
pub mod shares;
pub mod state;
pub mod transfer;
pub mod utils;
pub mod vault;

pub use app::App;
pub use error::IpcError;
pub use invoke::invoke_command;
