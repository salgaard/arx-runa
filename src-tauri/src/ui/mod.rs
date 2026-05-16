//! Arx Runa Tauri IPC command surface.
//!
//! All `#[tauri::command]` functions live in domain-specific sub-modules.
//! This module re-exports only.

pub mod auth_commands;
pub(crate) mod commands_common;
pub mod destination_commands;
pub mod error;
pub mod file_commands;
pub mod sharing_commands;
pub mod shell_commands;
pub mod state;
pub mod sync_commands;
pub mod types;
pub mod validation;
pub(crate) mod vault_paths;
pub mod video_stream;

pub use error::IpcError;
pub use state::AppState;

// Auth commands (9)
pub use auth_commands::{
    authenticate, change_password, create_vault, delete_vault, get_session_status, lock_session,
    recover_vault_with_phrase, rotate_key_file, setup_recovery,
};

// File commands (6)
pub use file_commands::{
    delete_file, download_file, get_file_content, list_directory, list_remote, prefetch_video,
    upload_file,
};

// Sync commands (7)
pub use sync_commands::{
    get_backup_health, get_sync_status, migrate_vault, pull_and_reconcile, recover_from_cloud,
    sync_backup, sync_to_cloud,
};

// Destination commands (3)
pub use destination_commands::{add_destination, delete_destination, list_destinations};

// Sharing commands (11)
pub use sharing_commands::{
    add_contact, export_public_key, get_own_public_key_b64, has_gdrive_service_account,
    import_share, list_contacts, list_received_shares, list_shares, revoke_share,
    set_gdrive_service_account, share_file,
};

#[cfg(test)]
mod security_audit;
