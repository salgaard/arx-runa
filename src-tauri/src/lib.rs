//! Arx Runa backend library.

pub mod auth;
pub mod crypto;
pub mod memory;
pub mod sharing;
pub mod storage;
pub mod sync;
pub mod ui;

use tauri::Manager as _;
use tokio_stream::StreamExt as _;

use crate::auth::DeviceEvent;
use crate::ui::AppState;

/// Payload emitted to the frontend on USB key-file device changes.
#[derive(serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct DeviceEventPayload {
    /// `"mounted"` or `"unmounted"`.
    kind: &'static str,
    /// Absolute path to the device mount point.
    mount_path: String,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
/// Starts the Arx Runa Tauri runtime with all 45 registered commands.
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();
    if let Err(error) = tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(crate::ui::AppState::construct_default())
        .invoke_handler(tauri::generate_handler![
            // Auth (13)
            ui::auth_commands::authenticate,
            ui::auth_commands::check_cloud_configured,
            ui::auth_commands::configure_cloud,
            ui::auth_commands::create_vault,
            ui::auth_commands::list_vaults,
            ui::auth_commands::change_password,
            ui::auth_commands::rotate_key_file,
            ui::auth_commands::delete_vault,
            ui::auth_commands::lock_session,
            ui::auth_commands::get_session_status,
            ui::auth_commands::check_pending_vault_operations,
            ui::auth_commands::retry_pending_vault_operation,
            ui::auth_commands::recover_vault_from_cloud,
            // Files (7)
            ui::file_commands::list_directory,
            ui::file_commands::upload_file,
            ui::file_commands::download_file,
            ui::file_commands::delete_file,
            ui::file_commands::get_file_content,
            ui::file_commands::list_remote,
            ui::file_commands::flush_epoch_buffer,
            // Sync (7)
            ui::sync_commands::sync_to_cloud,
            ui::sync_commands::recover_from_cloud,
            ui::sync_commands::pull_and_reconcile,
            ui::sync_commands::get_sync_status,
            ui::sync_commands::migrate_vault,
            ui::sync_commands::sync_backup,
            ui::sync_commands::get_backup_health,
            // Destinations (8)
            ui::destination_commands::add_destination,
            ui::destination_commands::list_destinations,
            ui::destination_commands::delete_destination,
            ui::destination_commands::set_primary_destination_cmd,
            ui::destination_commands::begin_google_drive_setup,
            ui::destination_commands::begin_onedrive_setup,
            ui::destination_commands::poll_oauth_setup,
            ui::destination_commands::cancel_oauth_setup_cmd,
            // Sharing (11)
            ui::sharing_commands::export_public_key,
            ui::sharing_commands::get_own_public_key_b64,
            ui::sharing_commands::add_contact,
            ui::sharing_commands::list_contacts,
            ui::sharing_commands::share_file,
            ui::sharing_commands::import_share,
            ui::sharing_commands::check_share_receipts,
            ui::sharing_commands::download_received_share,
            ui::sharing_commands::revoke_share,
            ui::sharing_commands::list_shares,
            ui::sharing_commands::list_received_shares,
            ui::sharing_commands::set_gdrive_service_account,
            ui::sharing_commands::has_gdrive_service_account,
            // Shell (3)
            ui::shell_commands::reveal_in_explorer,
            ui::shell_commands::compose_email_with_attachment,
            ui::shell_commands::open_url,
        ])
        .setup(|app| {
            use tauri::Emitter as _;

            let state = app.state::<AppState>();
            let app_handle = app.handle().clone();

            // Store the AppHandle for event emission.
            let _ = state.app_handle.set(app_handle.clone());

            // Spawn the device-event subscriber task on the Tauri async runtime.
            // `Arc<dyn DeviceMonitor>` is cloned here so the task owns it
            // independently of the setup closure lifetime.
            let device_monitor = state.device_monitor.clone();
            app.manage(tauri::async_runtime::spawn(async move {
                // `watch()` returns `Pin<Box<dyn Stream<...>>>`, which is Unpin
                // (Box<T>: Unpin always), so `.next()` works without pin_mut!.
                let mut stream = device_monitor.watch();
                while let Some(event) = stream.next().await {
                    let payload = match event {
                        DeviceEvent::Mounted { mount_path } => DeviceEventPayload {
                            kind: "mounted",
                            mount_path: mount_path.to_string_lossy().into_owned(),
                        },
                        DeviceEvent::Unmounted { mount_path } => DeviceEventPayload {
                            kind: "unmounted",
                            mount_path: mount_path.to_string_lossy().into_owned(),
                        },
                    };
                    if let Err(emit_error) = app_handle.emit("device-event", &payload) {
                        tracing::warn!("device-event emit failed: {emit_error}");
                    }
                }
            }));

            if let (Some(window), Some(icon)) =
                (app.get_webview_window("main"), app.default_window_icon())
            {
                let _ = window.set_icon(icon.clone());
            }

            #[cfg(debug_assertions)]
            if let Some(window) = app.get_webview_window("main") {
                window.open_devtools();
            }

            Ok(())
        })
        .run(tauri::generate_context!())
    {
        eprintln!("error while running tauri application: {error}");
    }
}
