//! Arx Runa backend library.

pub mod auth;
pub mod crypto;
pub mod memory;
pub mod sharing;
pub mod storage;
pub mod sync;
pub mod ui;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
/// Starts the Arx Runa Tauri runtime with all 29 registered commands.
pub fn run() {
    if let Err(error) = tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(crate::ui::AppState::construct_default())
        .invoke_handler(tauri::generate_handler![
            // Auth (7)
            ui::auth_commands::authenticate,
            ui::auth_commands::create_vault,
            ui::auth_commands::change_password,
            ui::auth_commands::rotate_key_file,
            ui::auth_commands::delete_vault,
            ui::auth_commands::lock_session,
            ui::auth_commands::get_session_status,
            // Files (6)
            ui::file_commands::list_directory,
            ui::file_commands::upload_file,
            ui::file_commands::download_file,
            ui::file_commands::delete_file,
            ui::file_commands::get_file_content,
            ui::file_commands::list_remote,
            // Sync (5)
            ui::sync_commands::sync_to_cloud,
            ui::sync_commands::recover_from_cloud,
            ui::sync_commands::get_sync_status,
            ui::sync_commands::migrate_vault,
            ui::sync_commands::sync_backup,
            // Destinations (3)
            ui::destination_commands::add_destination,
            ui::destination_commands::list_destinations,
            ui::destination_commands::delete_destination,
            // Sharing (8)
            ui::sharing_commands::export_public_key,
            ui::sharing_commands::add_contact,
            ui::sharing_commands::list_contacts,
            ui::sharing_commands::share_file,
            ui::sharing_commands::import_share,
            ui::sharing_commands::revoke_share,
            ui::sharing_commands::list_shares,
            ui::sharing_commands::list_received_shares,
        ])
        .run(tauri::generate_context!())
    {
        eprintln!("error while running tauri application: {error}");
    }
}
