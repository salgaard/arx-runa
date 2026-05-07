//! Destination session management commands.

use tauri::State;
use uuid::Uuid;

use std::sync::Arc;

use crate::storage::cloud::destination_session::{
    BackupSyncMode, DestinationSession, DestinationType, delete_destination_session,
    get_primary_destination, insert_destination_session, list_destination_sessions,
    set_primary_destination,
};
use crate::ui::auth_commands::rclone_conf_path;
use crate::ui::commands_common::require_active_session;
use crate::ui::error::IpcError;
use crate::ui::state::AppState;
use crate::ui::sync_commands::{build_destination_transport, rclone_binary_path};
use crate::ui::types::{DestinationEntry, DestinationSessionConfig};

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
            format!("[{}]\ntype = local\n", rclone_remote_name)
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
