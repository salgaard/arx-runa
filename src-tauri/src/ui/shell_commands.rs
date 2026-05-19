//! Shell integration commands (opener, file-manager reveal, email compose).

use std::path::PathBuf;

use tauri::{AppHandle, Manager as _, State};
use tauri_plugin_opener::OpenerExt;

use crate::ui::error::IpcError;
use crate::ui::state::AppState;

/// Validates that `path` is within an app-owned or session-registered root.
///
/// Allowed: any path under `app_data_dir`, or any path registered by
/// `download_received_share` in the current session. Paths are canonicalized
/// before comparison so symlinks and `..` components cannot escape the root.
async fn validate_reveal_path(
    path: &str,
    app: &AppHandle,
    state: &AppState,
) -> Result<PathBuf, IpcError> {
    let app_data = app
        .path()
        .app_data_dir()
        .map_err(|_| IpcError::InternalError("Shell operation failed".into()))?;

    let canonical = std::path::Path::new(path)
        .canonicalize()
        .map_err(|_| IpcError::InvalidInput("Path does not exist".into()))?;

    let allowed = state.allowed_reveal_paths.lock().await;
    let permitted = canonical.starts_with(&app_data) || allowed.contains(&canonical);
    if !permitted {
        tracing::warn!(path = %path, "reveal path outside allowed set");
        return Err(IpcError::InvalidInput(
            "Path is outside the allowed directory".into(),
        ));
    }

    Ok(canonical)
}

/// Validates that `url` uses an allowed scheme.
///
/// Allows `https://` unconditionally. Allows `http://` only for `127.0.0.1`
/// (rclone OAuth local callback). All other schemes are rejected.
fn validate_url_scheme(url: &str) -> Result<(), IpcError> {
    if url.starts_with("https://") {
        return Ok(());
    }
    if url.starts_with("http://127.0.0.1") {
        let rest = &url["http://127.0.0.1".len()..];
        if rest.is_empty() || rest.starts_with(':') || rest.starts_with('/') {
            return Ok(());
        }
    }
    Err(IpcError::InvalidInput(
        "Only https:// and http://127.0.0.1 URLs are permitted".into(),
    ))
}

/// Validates that `email` contains only characters safe to embed in a `mailto:` URL.
fn validate_email_address(email: &str) -> Result<(), IpcError> {
    let valid = !email.is_empty()
        && email.len() <= 254
        && email.contains('@')
        && email.chars().all(
            |c| matches!(c, 'a'..='z' | 'A'..='Z' | '0'..='9' | '.' | '_' | '%' | '+' | '-' | '@'),
        );
    if valid {
        Ok(())
    } else {
        Err(IpcError::InvalidInput("Invalid email address".into()))
    }
}

/// Reveals a file or directory in the platform file manager.
#[tauri::command]
pub async fn reveal_in_explorer(
    path: String,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<(), IpcError> {
    validate_reveal_path(&path, &app, &state).await?;
    app.opener().reveal_item_in_dir(&path).map_err(|e| {
        tracing::warn!(error = %e, "reveal_in_explorer failed");
        IpcError::InternalError("Shell operation failed".into())
    })?;
    Ok(())
}

/// Opens a URL in the default system browser.
///
/// Allows `https://` URLs and `http://127.0.0.1` for the rclone OAuth
/// local callback. All other schemes are rejected.
#[tauri::command]
pub async fn open_url(url: String, app: AppHandle) -> Result<(), IpcError> {
    validate_url_scheme(&url)?;
    app.opener().open_url(&url, None::<&str>).map_err(|e| {
        tracing::warn!(error = %e, "open_url failed");
        IpcError::InternalError("Shell operation failed".into())
    })?;
    Ok(())
}

/// Composes an email with a share package attached.
///
/// On Linux, delegates to `xdg-email --attach` so the `.arxshare` file is
/// attached automatically. On Windows and macOS the package is revealed in the
/// system file manager and the mail client is opened via `mailto:` — the user
/// must attach the file manually before sending.
#[tauri::command]
pub async fn compose_email_with_attachment(
    package_path: String,
    recipient_email: String,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<(), IpcError> {
    validate_email_address(&recipient_email)?;
    validate_reveal_path(&package_path, &app, &state).await?;

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

    app.opener().open_url(&mailto, None::<&str>).map_err(|e| {
        tracing::warn!(error = %e, "compose_email open_url failed");
        IpcError::InternalError("Shell operation failed".into())
    })?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- validate_url_scheme ---

    #[test]
    fn test_validate_url_scheme_https_allowed() {
        assert!(validate_url_scheme("https://accounts.google.com/o/oauth2/auth").is_ok());
        assert!(validate_url_scheme("https://login.microsoftonline.com/").is_ok());
    }

    #[test]
    fn test_validate_url_scheme_http_localhost_ip_allowed() {
        assert!(validate_url_scheme("http://127.0.0.1:53682/auth").is_ok());
        assert!(validate_url_scheme("http://127.0.0.1/").is_ok());
        assert!(validate_url_scheme("http://127.0.0.1").is_ok());
    }

    #[test]
    fn test_validate_url_scheme_http_non_loopback_rejected() {
        assert!(validate_url_scheme("http://localhost/").is_err());
        assert!(validate_url_scheme("http://127.0.0.2/").is_err());
        assert!(validate_url_scheme("http://example.com/").is_err());
    }

    #[test]
    fn test_validate_url_scheme_dangerous_schemes_rejected() {
        assert!(validate_url_scheme("file:///etc/passwd").is_err());
        assert!(validate_url_scheme("javascript:alert(1)").is_err());
        assert!(validate_url_scheme("data:text/html,<h1>x</h1>").is_err());
        assert!(validate_url_scheme("ftp://example.com/").is_err());
    }

    #[test]
    fn test_validate_url_scheme_http_loopback_bypass_rejected() {
        // Must not match e.g. http://127.0.0.1_evil
        assert!(validate_url_scheme("http://127.0.0.1_evil.com/").is_err());
        assert!(validate_url_scheme("http://127.0.0.10/").is_err());
    }

    // --- validate_email_address ---

    #[test]
    fn test_validate_email_address_valid() {
        assert!(validate_email_address("user@example.com").is_ok());
        assert!(validate_email_address("user.name+tag@sub.domain.org").is_ok());
    }

    #[test]
    fn test_validate_email_address_injection_chars_rejected() {
        assert!(validate_email_address("victim@x.com&attach=/etc/passwd").is_err());
        assert!(validate_email_address("user@example.com?bcc=evil").is_err());
        assert!(validate_email_address("user @example.com").is_err());
        assert!(validate_email_address("user\x00@example.com").is_err());
    }

    #[test]
    fn test_validate_email_address_empty_and_no_at_rejected() {
        assert!(validate_email_address("").is_err());
        assert!(validate_email_address("notanemail").is_err());
    }

    #[test]
    fn test_validate_email_address_path_chars_rejected() {
        assert!(validate_email_address("user@x.com/../../etc/passwd").is_err());
        assert!(validate_email_address("user@x.com\\path").is_err());
    }
}
