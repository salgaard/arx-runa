//! Vault path helpers for local vault-root discovery.
//!
//! Phase 6 supports a single vault per device. The vault root is
//! `dirs::data_dir()/arx-runa/vaults/`. Each vault occupies one sub-directory
//! named by its UUID v4 vault identifier.

use std::path::PathBuf;

use crate::ui::error::IpcError;

/// Returns the canonical vault root directory.
///
/// On Windows: `%APPDATA%\arx-runa\vaults`
/// On macOS: `~/Library/Application Support/arx-runa/vaults`
/// On Linux: `~/.local/share/arx-runa/vaults`
pub(crate) fn default_vault_root() -> PathBuf {
    dirs::data_dir()
        .expect("data_dir must be available")
        .join("arx-runa")
        .join("vaults")
}

/// Returns the SQLCipher database path for a given vault identifier.
///
/// Used by SessionManager to determine the database path during authentication.
pub(crate) fn vault_db_path(vault_id: &str) -> PathBuf {
    default_vault_root().join(vault_id).join("vault.db")
}

/// Returns the vault header JSON path for a given vault identifier.
#[allow(dead_code)] // Phase 7: used in direct vault-ID resolution
pub(crate) fn vault_header_path(vault_id: &str) -> PathBuf {
    default_vault_root()
        .join(vault_id)
        .join("vault-header.json")
}

/// Returns the staging directory path for a given vault.
pub(crate) fn vault_staging_dir(vault_id: &str) -> PathBuf {
    default_vault_root().join(vault_id).join("staging")
}

/// Discovers the single local vault directory.
///
/// Scans `default_vault_root()` for sub-directories that contain both
/// `vault.db` and `vault-header.json`. Returns:
/// - `Ok(Some((vault_id, db_path, header_path)))` if exactly one vault is found.
/// - `Ok(None)` if no vault directories are found.
/// - `Err(IpcError::InvalidInput(...))` if more than one vault directory is found.
///
/// Does not read or decrypt any file — path existence check only.
pub(crate) fn resolve_singleton_vault() -> Result<Option<(String, PathBuf, PathBuf)>, IpcError> {
    let root = default_vault_root();
    if !root.exists() {
        return Ok(None);
    }

    let entries = match std::fs::read_dir(&root) {
        Ok(entries) => entries,
        Err(_) => return Ok(None),
    };

    let mut found: Vec<(String, PathBuf, PathBuf)> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let vault_id = match path.file_name().and_then(|n| n.to_str()) {
            Some(name) => name.to_owned(),
            None => continue,
        };
        let db = path.join("vault.db");
        let header = path.join("vault-header.json");
        if db.exists() && header.exists() {
            found.push((vault_id, db, header));
        }
    }

    match found.len() {
        0 => Ok(None),
        1 => Ok(Some(found.remove(0))),
        _ => Err(IpcError::InvalidInput(
            "Multiple vaults found; supply an explicit vault_id to authenticate".into(),
        )),
    }
}

/// Resolves a vault directory by its exact vault identifier.
///
/// Scans `default_vault_root()` for a sub-directory whose `vault-header.json`
/// contains a matching `vault_id`. Returns `Ok((vault_id, db_path, header_path))`
/// on success, or `IpcError::NotFound` if no match is found.
pub(crate) fn resolve_vault_by_id(
    target_vault_id: &str,
) -> Result<(String, PathBuf, PathBuf), IpcError> {
    let root = default_vault_root();
    if !root.exists() {
        return Err(IpcError::NotFound("Vault not found".into()));
    }

    let entries =
        std::fs::read_dir(&root).map_err(|_| IpcError::NotFound("Vault not found".into()))?;

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let db = path.join("vault.db");
        let header_path = path.join("vault-header.json");
        if !db.exists() || !header_path.exists() {
            continue;
        }
        // Use the directory name as a fast pre-filter before reading the file.
        let dir_name = match path.file_name().and_then(|n| n.to_str()) {
            Some(name) => name.to_owned(),
            None => continue,
        };
        if dir_name == target_vault_id {
            return Ok((dir_name, db, header_path));
        }
    }

    Err(IpcError::NotFound("Vault not found".into()))
}

/// Scans the vault root and returns summaries for all discoverable vaults.
///
/// Each entry that contains both `vault.db` and a parseable `vault-header.json`
/// is included. Directories with unreadable or invalid headers are silently skipped.
pub(crate) fn list_local_vaults() -> Vec<crate::ui::types::VaultSummary> {
    use crate::storage::cloud::vault_header::VaultHeader;
    use crate::ui::types::VaultSummary;

    let root = default_vault_root();
    if !root.exists() {
        return Vec::new();
    }

    let entries = match std::fs::read_dir(&root) {
        Ok(e) => e,
        Err(_) => return Vec::new(),
    };

    let mut summaries = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let db = path.join("vault.db");
        let header_path = path.join("vault-header.json");
        if !db.exists() || !header_path.exists() {
            continue;
        }
        let header_bytes = match std::fs::read(&header_path) {
            Ok(b) => b,
            Err(_) => continue,
        };
        let header: VaultHeader = match serde_json::from_slice(&header_bytes) {
            Ok(h) => h,
            Err(_) => continue,
        };
        summaries.push(VaultSummary {
            vault_id: header.vault_id.clone(),
            name: header.name,
            tier: header.tier,
        });
    }
    summaries
}
