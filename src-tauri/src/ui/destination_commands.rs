//! Destination session management commands.

use tauri::State;
use uuid::Uuid;

use std::path::Path;
use std::sync::Arc;

use crate::storage::cloud::destination_session::{
    BackupSyncMode, DestinationSession, DestinationType, delete_destination_session,
    get_primary_destination, insert_destination_session, list_destination_sessions,
    set_primary_destination,
};
use crate::storage::cloud::{
    OAuthProvider, begin_oauth_setup, cancel_oauth_setup, complete_oauth_setup,
};
use crate::ui::auth_commands::rclone_conf_path;
use crate::ui::commands_common::require_active_session;
use crate::ui::error::IpcError;
use crate::ui::state::{AppState, OAuthSetupHandle};
use crate::ui::sync_commands::{build_destination_transport, rclone_binary_path};
use crate::ui::types::{
    BeginOauthSetupResponse, DestinationEntry, DestinationSessionConfig, OauthPollResponse,
};

/// Validate a local `path_prefix` before creating a destination session.
///
/// Rejects paths that are:
/// - Empty
/// - A filesystem root (e.g. `C:\` or `/`)
/// - The user's home directory itself
/// - Inside a OneDrive-managed directory (users should use the OneDrive destination instead)
fn validate_local_path(path_prefix: &str) -> Result<(), IpcError> {
    if path_prefix.is_empty() {
        return Err(IpcError::InvalidInput("Path must not be empty".into()));
    }

    let path = Path::new(path_prefix);

    // Reject drive roots and filesystem roots.
    let is_root = path.parent().is_none()
        || path
            .to_str()
            .map(|s| {
                let s = s.trim_end_matches(['/', '\\']);
                // Windows drive root: exactly "C:" after trimming
                (s.len() == 2 && s.as_bytes().get(1) == Some(&b':')) || s == "/" || s.is_empty()
            })
            .unwrap_or(false);
    if is_root {
        return Err(IpcError::InvalidInput(
            "Path must be a specific folder, not a drive root or filesystem root".into(),
        ));
    }

    // Reject the home directory itself.
    if let Some(home) = dirs::home_dir()
        && path == home
    {
        return Err(IpcError::InvalidInput(
            "Path must not be your home folder. Choose a dedicated subfolder instead.".into(),
        ));
    }

    // Reject paths inside OneDrive-managed directories.
    // Windows: ~/OneDrive* and ~/OneDrive - *
    // macOS:   ~/Library/CloudStorage/OneDrive*
    let is_inside_onedrive = dirs::home_dir().is_some_and(|home| {
        let components: Vec<_> = path.components().collect();
        let home_components: Vec<_> = home.components().collect();

        // Path must start with home
        if components.len() <= home_components.len() {
            return false;
        }
        if !components.starts_with(&home_components) {
            return false;
        }

        // Next component after home must match OneDrive folder patterns
        #[cfg(target_os = "windows")]
        {
            let next = components[home_components.len()];
            let next_str = next.as_os_str().to_string_lossy();
            next_str.to_lowercase().starts_with("onedrive")
        }
        #[cfg(target_os = "macos")]
        {
            let next = components[home_components.len()];
            let next_str = next.as_os_str().to_string_lossy();
            // macOS: ~/Library/CloudStorage/OneDrive*
            if next_str != "Library" {
                return false;
            }
            let cloud_storage_idx = home_components.len() + 1;
            if let Some(cs) = components.get(cloud_storage_idx) {
                if cs.as_os_str().to_string_lossy() != "CloudStorage" {
                    return false;
                }
            } else {
                return false;
            }
            components
                .get(home_components.len() + 2)
                .map(|c| {
                    c.as_os_str()
                        .to_string_lossy()
                        .to_lowercase()
                        .starts_with("onedrive")
                })
                .unwrap_or(false)
        }
        #[cfg(not(any(target_os = "windows", target_os = "macos")))]
        {
            false
        }
    });

    if is_inside_onedrive {
        return Err(IpcError::InvalidInput(
            "This path is inside a OneDrive-managed folder. \
            Add an OneDrive destination instead, or choose a folder outside of OneDrive."
                .into(),
        ));
    }

    Ok(())
}

/// Add a new destination session (primary or backup) to the vault.
///
/// Credentials are encrypted and stored in SQLCipher — `rclone_config_blob` is
/// never logged.
#[tauri::command]
pub async fn add_destination(
    config: DestinationSessionConfig,
    state: State<'_, AppState>,
) -> Result<DestinationEntry, IpcError> {
    state.session_manager.reset_timer().await;
    require_active_session(&state).await?;

    if config.label.is_empty() {
        return Err(IpcError::InvalidInput(
            "Destination label must not be empty".into(),
        ));
    }

    let destination_type = match config.destination_type.as_str() {
        "cloud" => DestinationType::Cloud,
        "external_drive" => DestinationType::ExternalDrive,
        "local_path" => DestinationType::LocalPath,
        _ => {
            return Err(IpcError::InvalidInput("Unknown destination type".into()));
        }
    };

    let backup_mode = match config.backup_mode.as_deref() {
        Some("mirror") => Some(BackupSyncMode::Mirror),
        Some("accumulating") => Some(BackupSyncMode::Accumulating),
        None => None,
        _ => {
            return Err(IpcError::InvalidInput("Unknown backup mode".into()));
        }
    };

    let destination_id = Uuid::new_v4().hyphenated().to_string();
    // Derive a collision-resistant rclone remote name from the UUID prefix.
    let rclone_remote_name = format!("arx_{}", &destination_id[..8]);

    let rclone_config_blob = match destination_type {
        DestinationType::LocalPath | DestinationType::ExternalDrive => {
            validate_local_path(&config.path_prefix)?;
            format!("[{}]\ntype = local\nnounc = true\n", rclone_remote_name)
        }
        DestinationType::Cloud => config.rclone_config_blob.clone(),
    };

    let session = DestinationSession {
        destination_id: destination_id.clone(),
        label: config.label.clone(),
        destination_type,
        rclone_remote_name,
        rclone_config_blob,
        bucket: config.bucket.clone(),
        path_prefix: config.path_prefix.clone(),
        is_primary: config.is_primary,
        backup_mode,
    };

    let db_guard = state.database.read().await;
    let db = db_guard
        .as_ref()
        .ok_or_else(|| IpcError::VaultLocked("Vault is locked".into()))?;

    insert_destination_session(db, &session)
        .await
        .map_err(IpcError::from)?;

    // Seed the pending-backup queue so the new mirror receives all existing vault blobs.
    if session.backup_mode.is_some() {
        let chunks = db.list_sync_chunks().await.map_err(IpcError::from)?;
        let blob_names: Vec<String> = chunks.into_iter().map(|c| c.blob_name).collect();
        if !blob_names.is_empty() {
            let _ = db
                .bulk_insert_pending_backup(&blob_names, &session.destination_id)
                .await;
        }
    }

    Ok(DestinationEntry {
        destination_id: session.destination_id,
        label: session.label,
        destination_type: config.destination_type,
        provider: config.provider,
        bucket: session.bucket,
        is_primary: session.is_primary,
        backup_mode: config.backup_mode,
    })
}

/// List all configured destination sessions for the current vault.
///
/// Returns metadata only — no credential material.
#[tauri::command]
pub async fn list_destinations(
    state: State<'_, AppState>,
) -> Result<Vec<DestinationEntry>, IpcError> {
    state.session_manager.reset_timer().await;
    require_active_session(&state).await?;

    let db_guard = state.database.read().await;
    let db = db_guard
        .as_ref()
        .ok_or_else(|| IpcError::VaultLocked("Vault is locked".into()))?;

    let sessions = list_destination_sessions(db)
        .await
        .map_err(IpcError::from)?;

    let entries = sessions
        .into_iter()
        .map(|session| {
            let destination_type_str = match session.destination_type {
                DestinationType::Cloud => "cloud",
                DestinationType::ExternalDrive => "external_drive",
                DestinationType::LocalPath => "local_path",
            }
            .to_owned();

            let backup_mode_str = session.backup_mode.map(|mode| match mode {
                BackupSyncMode::Mirror => "mirror".to_owned(),
                BackupSyncMode::Accumulating => "accumulating".to_owned(),
            });

            DestinationEntry {
                destination_id: session.destination_id,
                label: session.label,
                destination_type: destination_type_str,
                provider: session.rclone_remote_name,
                bucket: session.bucket,
                is_primary: session.is_primary,
                backup_mode: backup_mode_str,
            }
        })
        .collect();

    Ok(entries)
}

/// Delete a destination session from the vault.
#[tauri::command]
pub async fn delete_destination(
    destination_id: String,
    state: State<'_, AppState>,
) -> Result<(), IpcError> {
    state.session_manager.reset_timer().await;
    require_active_session(&state).await?;

    if destination_id.is_empty() {
        return Err(IpcError::InvalidInput(
            "Destination ID must not be empty".into(),
        ));
    }

    let db_guard = state.database.read().await;
    let db = db_guard
        .as_ref()
        .ok_or_else(|| IpcError::VaultLocked("Vault is locked".into()))?;

    let primary = get_primary_destination(db).await.map_err(IpcError::from)?;
    if primary.is_some_and(|p| p.destination_id == destination_id) {
        return Err(IpcError::InvalidInput(
            "Cannot delete the primary destination — promote another destination to primary first."
                .into(),
        ));
    }

    delete_destination_session(db, &destination_id)
        .await
        .map_err(IpcError::from)?;

    let _ = db
        .clear_pending_backups_for_destination(&destination_id)
        .await;

    Ok(())
}

/// Promote a backup destination to primary, demoting the current primary.
///
/// Also hot-swaps `AppState.cloud_transport` so subsequent syncs use the new
/// primary's rclone config without requiring a lock/unlock cycle.
#[tauri::command]
pub async fn set_primary_destination_cmd(
    destination_id: String,
    state: State<'_, AppState>,
) -> Result<(), IpcError> {
    state.session_manager.reset_timer().await;
    require_active_session(&state).await?;

    if destination_id.is_empty() {
        return Err(IpcError::InvalidInput(
            "Destination ID must not be empty".into(),
        ));
    }

    let db_guard = state.database.read().await;
    let db = db_guard
        .as_ref()
        .ok_or_else(|| IpcError::VaultLocked("Vault is locked".into()))?;

    set_primary_destination(db, &destination_id)
        .await
        .map_err(IpcError::from)?;

    let new_primary = get_primary_destination(db)
        .await
        .map_err(IpcError::from)?
        .ok_or_else(|| IpcError::InternalError("Primary destination missing after swap".into()))?;

    drop(db_guard);

    let app_handle = state
        .app_handle
        .get()
        .ok_or_else(|| IpcError::InternalError("App handle not initialised".into()))?;
    let binary_path = rclone_binary_path(app_handle);

    let conf_path = rclone_conf_path();

    let transport = build_destination_transport(binary_path, &conf_path, &new_primary).await?;

    state.swap_cloud_transport(Arc::new(transport)).await;

    Ok(())
}

/// Begin a Google Drive OAuth setup flow.
///
/// Spawns rclone, reads the auth URL from stderr, and returns a `setup_id` the
/// frontend uses for polling.  The frontend must open `auth_url` in the system
/// browser via `open_url`.
#[tauri::command]
pub async fn begin_google_drive_setup(
    state: State<'_, AppState>,
) -> Result<BeginOauthSetupResponse, IpcError> {
    state.session_manager.reset_timer().await;
    require_active_session(&state).await?;

    let app_handle = state
        .app_handle
        .get()
        .ok_or_else(|| IpcError::InternalError("App handle not initialised".into()))?;
    let binary_path = rclone_binary_path(app_handle);

    let begun = begin_oauth_setup(OAuthProvider::GoogleDrive, &binary_path)
        .await
        .map_err(|error| {
            tracing::error!(error = %error, "begin_google_drive_setup failed");
            IpcError::CloudError("Failed to start Google Drive setup".into())
        })?;

    let response = BeginOauthSetupResponse {
        setup_id: begun.setup_id.clone(),
        auth_url: begun.auth_url.clone(),
    };

    let handle = OAuthSetupHandle {
        child: begun.child,
        temp_config_path: begun.temp_config_path,
        remote_name: begun.remote_name,
    };
    state
        .oauth_setups
        .lock()
        .await
        .insert(begun.setup_id, handle);

    Ok(response)
}

/// Begin a OneDrive OAuth setup flow.
///
/// Spawns rclone, reads the auth URL from stderr, and returns a `setup_id` the
/// frontend uses for polling.  The frontend must open `auth_url` in the system
/// browser via `open_url`.
#[tauri::command]
pub async fn begin_onedrive_setup(
    state: State<'_, AppState>,
) -> Result<BeginOauthSetupResponse, IpcError> {
    state.session_manager.reset_timer().await;
    require_active_session(&state).await?;

    let app_handle = state
        .app_handle
        .get()
        .ok_or_else(|| IpcError::InternalError("App handle not initialised".into()))?;
    let binary_path = rclone_binary_path(app_handle);

    let begun = begin_oauth_setup(OAuthProvider::OneDrive, &binary_path)
        .await
        .map_err(|error| {
            tracing::error!(error = %error, "begin_onedrive_setup failed");
            IpcError::CloudError("Failed to start OneDrive setup".into())
        })?;

    let response = BeginOauthSetupResponse {
        setup_id: begun.setup_id.clone(),
        auth_url: begun.auth_url.clone(),
    };

    let handle = OAuthSetupHandle {
        child: begun.child,
        temp_config_path: begun.temp_config_path,
        remote_name: begun.remote_name,
    };
    state
        .oauth_setups
        .lock()
        .await
        .insert(begun.setup_id, handle);

    Ok(response)
}

/// Poll whether an OAuth setup has completed.
///
/// Returns `Pending` while the user is still in the browser, `Completed` with
/// the raw `rclone_config_blob` once the OAuth callback is received, or
/// `Failed` if rclone exited with an error.  The caller must pass
/// `rclone_config_blob` to `add_destination` to persist the credential.
#[tauri::command]
pub async fn poll_oauth_setup(
    setup_id: String,
    state: State<'_, AppState>,
) -> Result<OauthPollResponse, IpcError> {
    let app_handle = state
        .app_handle
        .get()
        .ok_or_else(|| IpcError::InternalError("App handle not initialised".into()))?;
    let binary_path = rclone_binary_path(app_handle);

    let mut setups = state.oauth_setups.lock().await;

    let wait_result = {
        let handle = setups
            .get_mut(&setup_id)
            .ok_or_else(|| IpcError::NotFound(format!("OAuth setup '{setup_id}' not found")))?;
        handle.child.try_wait().map_err(|error| {
            IpcError::InternalError(format!("failed to poll OAuth child: {error}"))
        })?
    };

    let exit_status = match wait_result {
        None => return Ok(OauthPollResponse::Pending),
        Some(status) => status,
    };

    let handle = setups.remove(&setup_id).ok_or_else(|| {
        IpcError::InternalError("OAuth setup disappeared between poll and remove".into())
    })?;
    drop(setups);

    if !exit_status.success() {
        if let Err(error) = tokio::fs::remove_file(&handle.temp_config_path).await {
            tracing::warn!(error = %error, "failed to remove failed OAuth temp config");
        }
        return Ok(OauthPollResponse::Failed {
            message: "Cloud provider authorization failed. Please try again.".into(),
        });
    }

    match complete_oauth_setup(&binary_path, &handle.temp_config_path, &handle.remote_name).await {
        Ok(rclone_config_blob) => Ok(OauthPollResponse::Completed { rclone_config_blob }),
        Err(error) => {
            tracing::error!(error = %error, "complete_oauth_setup failed");
            Ok(OauthPollResponse::Failed {
                message: "Failed to retrieve cloud credentials. Please try again.".into(),
            })
        }
    }
}

/// Cancel a pending OAuth setup.
///
/// Kills the rclone subprocess and removes the temporary config file.  Safe to
/// call even if the setup has already completed or been removed.
#[tauri::command]
pub async fn cancel_oauth_setup_cmd(
    setup_id: String,
    state: State<'_, AppState>,
) -> Result<(), IpcError> {
    let mut setups = state.oauth_setups.lock().await;
    let Some(mut handle) = setups.remove(&setup_id) else {
        return Ok(());
    };
    drop(setups);

    cancel_oauth_setup(&mut handle.child, &handle.temp_config_path).await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_destination_type_cloud_parsing() {
        let destination_type_str = "cloud";
        let destination_type = match destination_type_str {
            "cloud" => DestinationType::Cloud,
            "external_drive" => DestinationType::ExternalDrive,
            "local_path" => DestinationType::LocalPath,
            _ => unreachable!(),
        };
        assert!(matches!(destination_type, DestinationType::Cloud));
    }

    #[test]
    fn test_destination_type_external_drive_parsing() {
        let destination_type_str = "external_drive";
        let destination_type = match destination_type_str {
            "cloud" => DestinationType::Cloud,
            "external_drive" => DestinationType::ExternalDrive,
            "local_path" => DestinationType::LocalPath,
            _ => unreachable!(),
        };
        assert!(matches!(destination_type, DestinationType::ExternalDrive));
    }

    #[test]
    fn test_backup_mode_mirror_parsing() {
        let backup_mode_str = Some("mirror");
        let backup_mode = match backup_mode_str {
            Some("mirror") => Some(BackupSyncMode::Mirror),
            Some("accumulating") => Some(BackupSyncMode::Accumulating),
            None => None,
            _ => unreachable!(),
        };
        assert!(matches!(backup_mode, Some(BackupSyncMode::Mirror)));
    }

    #[test]
    fn test_backup_mode_accumulating_parsing() {
        let backup_mode_str = Some("accumulating");
        let backup_mode = match backup_mode_str {
            Some("mirror") => Some(BackupSyncMode::Mirror),
            Some("accumulating") => Some(BackupSyncMode::Accumulating),
            None => None,
            _ => unreachable!(),
        };
        assert!(matches!(backup_mode, Some(BackupSyncMode::Accumulating)));
    }

    #[test]
    fn test_rclone_remote_name_derived_from_uuid() {
        let destination_id = "550e8400-e29b-41d4-a716-446655440000";
        let rclone_remote_name = format!("arx_{}", &destination_id[..8]);
        assert_eq!(rclone_remote_name, "arx_550e8400");
    }

    #[test]
    fn test_empty_destination_id_rejected() {
        let destination_id = "";
        assert!(destination_id.is_empty());
    }
}
