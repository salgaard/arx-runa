//! Staging-file writer for `pending-vault-header.json`.
//!
//! Phase 2.4 needs to write the next vault-header value to a staging file
//! under `dirs::config_dir() / "arx-runa/"` before uploading it to the cloud,
//! so that a crash between the write and the upload leaves a record the
//! Phase 4.3 startup retry path can consume. This helper centralises the
//! owner-only permission logic so every ceremony writes consistently.
//!
//! Platform handling:
//! - Unix (Linux/macOS): file is created with mode `0o600`.
//! - Windows: file is created with `OpenOptions`; restrictive DACLs are a
//!   documented limitation — Phase 4.3 will add explicit DACL restriction
//!   via the `windows` crate. The staging file only ever holds the public
//!   vault-header JSON (no secrets); the permission gap does not leak key
//!   material but does allow local non-admin users to read the ciphertext
//!   of recovery-slot blobs.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::auth::error::AuthenticationError;

/// Returns the directory used for vault-header staging files.
///
/// The directory is `dirs::config_dir() / "arx-runa/"`. The directory is
/// created if missing. Returns `VaultHeaderInvalid` if `config_dir()` is
/// unavailable or the directory cannot be created.
pub(crate) fn staging_directory() -> Result<PathBuf, AuthenticationError> {
    let base = dirs::config_dir().ok_or(AuthenticationError::VaultHeaderInvalid)?;
    let staging_dir = base.join("arx-runa");
    std::fs::create_dir_all(&staging_dir).map_err(|_| AuthenticationError::VaultHeaderInvalid)?;
    Ok(staging_dir)
}

/// Writes `bytes` to `path` with owner-only permissions.
///
/// On Unix the file is created with mode `0o600`. On Windows the file is
/// created with a default DACL (see module-level note). The file is fully
/// written and closed before this function returns.
pub(crate) fn write_owner_only(path: &Path, bytes: &[u8]) -> Result<(), AuthenticationError> {
    let mut options = OpenOptions::new();
    options.write(true).create(true).truncate(true);

    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }

    let mut file = options
        .open(path)
        .map_err(|_| AuthenticationError::VaultHeaderInvalid)?;
    file.write_all(bytes)
        .map_err(|_| AuthenticationError::VaultHeaderInvalid)?;
    file.sync_all()
        .map_err(|_| AuthenticationError::VaultHeaderInvalid)?;
    Ok(())
}

/// Best-effort removal of a staging file. Ignores `NotFound`; surfaces
/// other errors as `VaultHeaderInvalid`.
pub(crate) fn remove_if_exists(path: &Path) -> Result<(), AuthenticationError> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(AuthenticationError::VaultHeaderInvalid),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_write_owner_only_writes_exact_bytes() {
        let directory = tempdir().expect("tempdir must succeed");
        let path = directory.path().join("header.json");
        let payload = b"{\"schema_version\":1}";

        write_owner_only(&path, payload).expect("write must succeed");

        let recovered = std::fs::read(&path).expect("read must succeed");
        assert_eq!(recovered, payload);
    }

    #[cfg(unix)]
    #[test]
    fn test_write_owner_only_sets_mode_0o600_on_unix() {
        use std::os::unix::fs::PermissionsExt;
        let directory = tempdir().expect("tempdir must succeed");
        let path = directory.path().join("header.json");

        write_owner_only(&path, b"payload").expect("write must succeed");

        let metadata = std::fs::metadata(&path).expect("metadata must succeed");
        assert_eq!(metadata.permissions().mode() & 0o777, 0o600);
    }

    #[test]
    fn test_write_owner_only_truncates_existing_content() {
        let directory = tempdir().expect("tempdir must succeed");
        let path = directory.path().join("header.json");

        write_owner_only(&path, b"longer initial content").expect("first write must succeed");
        write_owner_only(&path, b"short").expect("second write must succeed");

        let recovered = std::fs::read(&path).expect("read must succeed");
        assert_eq!(recovered, b"short");
    }

    #[test]
    fn test_remove_if_exists_returns_ok_on_missing_file() {
        let directory = tempdir().expect("tempdir must succeed");
        let path = directory.path().join("missing.json");

        remove_if_exists(&path).expect("missing file must not error");
    }

    #[test]
    fn test_remove_if_exists_deletes_existing_file() {
        let directory = tempdir().expect("tempdir must succeed");
        let path = directory.path().join("delete-me.json");
        write_owner_only(&path, b"content").expect("write must succeed");

        remove_if_exists(&path).expect("remove must succeed");

        assert!(!path.exists());
    }
}
