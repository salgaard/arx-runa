//! Shell integration commands (opener, file-manager reveal, email compose).

use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;

use crate::ui::error::IpcError;
use crate::ui::state::AppState;

/// Reveals a file or directory in the platform file manager.
#[tauri::command]
pub async fn reveal_in_explorer(
    path: String,
    _state: State<'_, AppState>,
    app: AppHandle,
) -> Result<(), IpcError> {
    app.opener()
        .reveal_item_in_dir(&path)
        .map_err(|e| IpcError::InternalError(format!("reveal failed: {e}")))?;
    Ok(())
}

/// Opens a URL in the default system browser.
///
/// Used by the OAuth destination setup flow to open the rclone auth URL.
#[tauri::command]
pub async fn open_url(url: String, app: AppHandle) -> Result<(), IpcError> {
    app.opener()
        .open_url(&url, None::<&str>)
        .map_err(|e| IpcError::InternalError(format!("open_url failed: {e}")))?;
    Ok(())
}

///
/// On Linux, delegates to `xdg-email --attach` so the `.arxshare` file is
/// attached automatically. On Windows and macOS the package is revealed in the
/// system file manager and the mail client is opened via `mailto:` — the user
/// must attach the file manually before sending.
#[tauri::command]
pub async fn compose_email_with_attachment(
    package_path: String,
    recipient_email: String,
    _state: State<'_, AppState>,
    app: AppHandle,
) -> Result<(), IpcError> {
    const SUBJECT: &str = "Shared%20file%20via%20Arx%20Runa";

    #[cfg(target_os = "linux")]
    {
        let body = "I%27ve%20shared%20a%20file%20with%20you%20using%20Arx%20Runa.\
                    %0A%0ATo%20access%20it%3A%0A1.%20Install%20Arx%20Runa\
                    %0A2.%20Go%20to%20Shares%20%E2%86%92%20Received%20%E2%86%92%20Import%20from%20file\
                    %0A3.%20Select%20the%20attached%20.arxshare%20file\
                    %0A%0AThe%20file%20is%20encrypted%20%E2%80%94%20only%20you%20can%20open%20it.";
        let mailto = format!("mailto:{recipient_email}?subject={SUBJECT}&body={body}");
        let spawned = std::process::Command::new("xdg-email")
            .arg("--attach")
            .arg(&package_path)
            .arg(&mailto)
            .spawn();
        if spawned.is_ok() {
            return Ok(());
        }
        // xdg-email unavailable — fall through to the generic opener path.
    }

    // Windows / macOS (or Linux fallback): reveal the package in the file manager
    // so the user can attach it, then open the mail client with pre-filled headers.
    let body = "I%27ve%20shared%20a%20file%20with%20you%20using%20Arx%20Runa.\
                %0A%0ATo%20access%20it%3A%0A1.%20Install%20Arx%20Runa\
                %0A2.%20Go%20to%20Shares%20%E2%86%92%20Received%20%E2%86%92%20Import%20from%20file\
                %0A3.%20Select%20the%20.arxshare%20file\
                %0A%0AThe%20file%20is%20encrypted%20%E2%80%94%20only%20you%20can%20open%20it.";
    let mailto = format!("mailto:{recipient_email}?subject={SUBJECT}&body={body}");

    let _ = app.opener().reveal_item_in_dir(&package_path);

    app.opener()
        .open_url(&mailto, None::<&str>)
        .map_err(|e| IpcError::InternalError(format!("open_url failed: {e}")))?;

    Ok(())
}
