//! Arx Runa backend library.

pub mod auth;
pub mod crypto;
pub mod memory;
pub(crate) mod platform;
pub mod sharing;
pub mod storage;
pub mod ui;

#[cfg(test)]
mod tests;

#[cfg(feature = "fuzzing")]
pub mod fuzz_api;

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

/// Spawns a best-effort background task that checks for an application update on
/// startup and, if one is available, offers to install it via a native dialog.
///
/// Any failure (no network, malformed manifest, signature mismatch, download
/// error) is logged at `warn!` and swallowed — a failed or absent update check
/// must never block application launch.
fn spawn_update_check(app_handle: tauri::AppHandle) {
    use tauri_plugin_dialog::{DialogExt as _, MessageDialogButtons};
    use tauri_plugin_updater::UpdaterExt as _;

    tauri::async_runtime::spawn(async move {
        let updater = match app_handle.updater() {
            Ok(updater) => updater,
            Err(error) => {
                tracing::warn!(error = %error, "updater unavailable");
                return;
            }
        };

        let update = match updater.check().await {
            Ok(Some(update)) => update,
            Ok(None) => return,
            Err(error) => {
                tracing::warn!(error = %error, "update check failed");
                return;
            }
        };

        let prompt = format!(
            "Arx Runa {} is available (you have {}). Install and restart now?",
            update.version, update.current_version
        );
        let accepted = app_handle
            .dialog()
            .message(prompt)
            .title("Update available")
            .buttons(MessageDialogButtons::OkCancelCustom(
                "Install".to_owned(),
                "Later".to_owned(),
            ))
            .blocking_show();
        if !accepted {
            return;
        }

        if let Err(error) = update
            .download_and_install(|_chunk, _total| {}, || {})
            .await
        {
            tracing::warn!(error = %error, "update download/install failed");
            return;
        }

        // Relaunch into the freshly installed version. `restart` diverges.
        app_handle.restart();
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
/// Starts the Arx Runa Tauri runtime with 62 commands via `generate_handler!` plus `video_stream` registered separately (63 total).
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();
    if let Err(error) = crate::ui::video_stream::register(tauri::Builder::default())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(crate::ui::AppState::construct_default())
        .invoke_handler(tauri::generate_handler![
            // Auth (18)
            ui::auth_commands::authenticate,
            ui::auth_commands::check_cloud_configured,
            ui::auth_commands::configure_cloud,
            ui::auth_commands::create_vault,
            ui::auth_commands::list_vaults,
            ui::auth_commands::change_password,
            ui::auth_commands::rotate_key_file,
            ui::auth_commands::setup_recovery,
            ui::auth_commands::recover_vault_with_phrase,
            ui::auth_commands::delete_vault,
            ui::auth_commands::lock_session,
            ui::auth_commands::get_session_status,
            ui::auth_commands::check_pending_vault_operations,
            ui::auth_commands::retry_pending_vault_operation,
            ui::auth_commands::recover_vault_from_cloud,
            ui::auth_commands::recover_vault_from_cloud_with_phrase,
            ui::auth_commands::scan_for_key_file,
            ui::auth_commands::is_path_on_removable_drive,
            // Files (10)
            ui::file_commands::list_directory,
            ui::file_commands::upload_file,
            ui::file_commands::download_file,
            ui::file_commands::delete_file,
            ui::file_commands::delete_directory,
            ui::file_commands::get_file_content,
            ui::file_commands::prefetch_video,
            ui::file_commands::list_remote,
            ui::file_commands::flush_epoch_buffer,
            ui::file_commands::stat_local_path,
            ui::file_commands::list_local_directory,
            ui::file_commands::create_vault_directory,
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
            ui::destination_commands::set_primary_destination,
            ui::destination_commands::begin_google_drive_setup,
            ui::destination_commands::begin_onedrive_setup,
            ui::destination_commands::poll_oauth_setup,
            ui::destination_commands::cancel_oauth_setup,
            // Sharing (11)
            ui::sharing_commands::export_public_key,
            ui::sharing_commands::get_own_public_key_b64,
            ui::sharing_commands::add_contact,
            ui::sharing_commands::list_contacts,
            ui::sharing_commands::share_file,
            ui::sharing_commands::import_share,
            ui::sharing_commands::check_share_receipts,
            ui::sharing_commands::download_received_share,
            ui::sharing_commands::get_received_share_content,
            ui::sharing_commands::revoke_share,
            ui::sharing_commands::list_shares,
            ui::sharing_commands::list_received_shares,
            ui::sharing_commands::set_gdrive_service_account,
            ui::sharing_commands::has_gdrive_service_account,
            // Shell (3)
            ui::shell_commands::reveal_in_explorer,
            ui::shell_commands::compose_email_with_attachment,
            ui::shell_commands::open_url,
            // Video streaming (1)
            ui::video_stream::video_scheme_base_url,
        ])
        .setup(|app| {
            use tauri::Emitter as _;

            let state = app.state::<AppState>();
            let app_handle = app.handle().clone();
            let active_vault_id = state.active_vault_id.clone();

            // Store the AppHandle for event emission.
            let _ = state.app_handle.set(app_handle.clone());

            // Best-effort: check for an application update on startup and offer to
            // install it. Never blocks launch — see `spawn_update_check`.
            spawn_update_check(app_handle.clone());

            // Sweep any orphaned rclone temp dirs left by previous crashes / forced
            // kills.  Directories are named `arx-runa-<16 hex>` in the OS temp dir.
            // This is best-effort — errors are silently ignored.
            tauri::async_runtime::spawn(async move {
                let temp = std::env::temp_dir();
                if let Ok(mut entries) = tokio::fs::read_dir(&temp).await {
                    while let Ok(Some(entry)) = entries.next_entry().await {
                        let name = entry.file_name();
                        let name_str = name.to_string_lossy();
                        if name_str.starts_with("arx-runa-")
                            && name_str.len() == "arx-runa-".len() + 16
                            && name_str["arx-runa-".len()..]
                                .chars()
                                .all(|c| c.is_ascii_hexdigit())
                        {
                            let _ = tokio::fs::remove_dir_all(entry.path()).await;
                        }
                    }
                }
            });

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

            // On window close: lock the session (zeroizes keys + deletes rclone.conf),
            // wipe staging/cache, then programmatically destroy the window.
            // `prevent_close()` + async spawn is required because `lock()` is async
            // and `on_window_event` is synchronous.
            if let Some(window) = app.get_webview_window("main") {
                let session_manager = state.session_manager.clone();
                window.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();

                        // Snapshot active vault for cache wipe (non-blocking try_read).
                        let cache_dir = active_vault_id
                            .try_read()
                            .ok()
                            .and_then(|g| g.clone())
                            .map(|vault_id| {
                                crate::ui::vault_paths::vault_staging_dir(&vault_id).join("cache")
                            });

                        let sm = session_manager.clone();
                        tauri::async_runtime::spawn(async move {
                            // lock() zeroizes all keys and calls destroy_rclone_conf().
                            let _ = sm.lock().await;

                            // Wipe preview cache blobs (fast, directory-entries only).
                            if let Some(dir) = cache_dir {
                                let _ = std::fs::remove_dir_all(&dir);
                            }

                            // Exit the process — window is already prevented from closing.
                            std::process::exit(0);
                        });
                    }
                });
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
