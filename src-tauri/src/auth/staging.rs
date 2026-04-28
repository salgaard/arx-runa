//! Staging-file writer for `pending-vault-header.json`.
//!
//! Phase 2.4 needs to write the next vault-header value to a staging file
//! under `dirs::config_dir() / "arx-runa/"` before uploading it to the cloud,
//! so that a crash between the write and the upload leaves a record the
//! Phase 4.5 push flow will consume (header upload is idempotent per push).
//! This helper centralises the
//! owner-only permission logic so every ceremony writes consistently.
//!
//! Platform handling:
//! - Unix (Linux/macOS): file is created with mode `0o600`.
//! - Windows: both the staging directory and files are created with an
//!   explicit owner-only DACL.

#[cfg(not(windows))]
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

use zeroize::Zeroizing;

use crate::auth::error::AuthenticationError;

/// Returns the directory used for vault-header staging files.
///
/// The directory is `dirs::config_dir() / "arx-runa/"`. The directory is
/// created if missing. Returns `VaultHeaderInvalid` if `config_dir()` is
/// unavailable or the directory cannot be created.
pub(crate) async fn staging_directory() -> Result<PathBuf, AuthenticationError> {
    let base = dirs::config_dir().ok_or(AuthenticationError::VaultHeaderInvalid)?;
    let staging_dir = base.join("arx-runa");

    // The staging directory holds plaintext vault-header JSON — not secret material.
    // Owner-only ACL is applied to individual files that contain sensitive data;
    // using it on the directory itself causes FILE_ADD_FILE to be denied on Windows
    // because the Creator Owner SID (OW/S-1-3-0) is a template for child-object
    // inheritance and is never present in a real user token.
    tokio::fs::create_dir_all(&staging_dir)
        .await
        .map_err(|_| AuthenticationError::VaultHeaderInvalid)?;

    Ok(staging_dir)
}

/// Writes `bytes` to `path` with owner-only permissions.
///
/// On Unix the file is created with mode `0o600`. On Windows the file is
/// created with an explicit owner-only DACL. The file is fully written and
/// closed before this function returns.
pub(crate) async fn write_owner_only(path: &Path, bytes: &[u8]) -> Result<(), AuthenticationError> {
    write_owner_only_inner(path, bytes, false).await
}

/// Writes `bytes` to a newly created `path` with owner-only permissions.
///
/// Returns `VaultHeaderInvalid` when `path` already exists. This is used for
/// key-file outputs where silent overwrite is not allowed.
pub(crate) async fn write_owner_only_new(
    path: &Path,
    bytes: &[u8],
) -> Result<(), AuthenticationError> {
    write_owner_only_inner(path, bytes, true).await
}

/// Writes bytes with owner-only permissions, optionally requiring a new file.
async fn write_owner_only_inner(
    path: &Path,
    bytes: &[u8],
    require_new_file: bool,
) -> Result<(), AuthenticationError> {
    let path = path.to_path_buf();
    let bytes = Zeroizing::new(bytes.to_vec());
    tokio::task::spawn_blocking(move || -> Result<(), AuthenticationError> {
        #[cfg(not(windows))]
        let mut file = {
            let mut options = OpenOptions::new();
            options.write(true);
            if require_new_file {
                options.create_new(true);
            } else {
                options.create(true).truncate(true);
            }

            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }

            options
                .open(path)
                .map_err(|_| AuthenticationError::VaultHeaderInvalid)?
        };

        #[cfg(windows)]
        let mut file = create_owner_only_file_windows(&path, require_new_file)?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            file.set_permissions(std::fs::Permissions::from_mode(0o600))
                .map_err(|_| AuthenticationError::VaultHeaderInvalid)?;
        }

        file.write_all(bytes.as_slice())
            .map_err(|_| AuthenticationError::VaultHeaderInvalid)?;
        file.sync_all()
            .map_err(|_| AuthenticationError::VaultHeaderInvalid)?;
        Ok(())
    })
    .await
    .map_err(|_| AuthenticationError::VaultHeaderInvalid)?
}

/// Best-effort removal of a staging file. Ignores `NotFound`; surfaces
/// other errors as `VaultHeaderInvalid`.
pub(crate) async fn remove_if_exists(path: &Path) -> Result<(), AuthenticationError> {
    match tokio::fs::remove_file(path).await {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(AuthenticationError::VaultHeaderInvalid),
    }
}

#[cfg(windows)]
fn create_owner_only_file_windows(
    path: &Path,
    require_new_file: bool,
) -> Result<std::fs::File, AuthenticationError> {
    use std::os::windows::io::FromRawHandle;

    use windows::Win32::Storage::FileSystem::{
        CREATE_ALWAYS, CREATE_NEW, CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_GENERIC_READ,
        FILE_GENERIC_WRITE, FILE_SHARE_MODE,
    };
    use windows::core::PCWSTR;

    let security_descriptor = WindowsSecurityDescriptor::from_sddl(owner_only_file_sddl())?;
    let security_attributes = security_descriptor.security_attributes();
    let path_wide = to_wide_null(path.as_os_str());
    let disposition = if require_new_file {
        CREATE_NEW
    } else {
        CREATE_ALWAYS
    };

    // SAFETY: `path_wide` is null-terminated and `security_attributes`
    // references a valid security descriptor for the duration of the call.
    let handle = unsafe {
        CreateFileW(
            PCWSTR(path_wide.as_ptr()),
            FILE_GENERIC_READ.0 | FILE_GENERIC_WRITE.0,
            FILE_SHARE_MODE(0),
            Some(&security_attributes),
            disposition,
            FILE_ATTRIBUTE_NORMAL,
            None,
        )
    }
    .map_err(|_| AuthenticationError::VaultHeaderInvalid)?;

    // SAFETY: `CreateFileW` returned a valid owned handle for this process.
    let file = unsafe { std::fs::File::from_raw_handle(handle.0) };

    if !require_new_file {
        apply_owner_only_acl_windows(path, false)?;
    }

    Ok(file)
}

#[cfg(windows)]
fn apply_owner_only_acl_windows(
    path: &Path,
    is_directory: bool,
) -> Result<(), AuthenticationError> {
    let sddl = if is_directory {
        owner_only_directory_sddl()
    } else {
        owner_only_file_sddl()
    };
    apply_sddl_to_path_windows(path, sddl)
}

#[cfg(windows)]
fn apply_sddl_to_path_windows(path: &Path, sddl: &str) -> Result<(), AuthenticationError> {
    use windows::Win32::Foundation::ERROR_SUCCESS;
    use windows::Win32::Security::Authorization::{SE_FILE_OBJECT, SetNamedSecurityInfoW};
    use windows::Win32::Security::{
        DACL_SECURITY_INFORMATION, PROTECTED_DACL_SECURITY_INFORMATION,
    };
    use windows::core::PCWSTR;

    let security_descriptor = WindowsSecurityDescriptor::from_sddl(sddl)?;
    let dacl = security_descriptor.dacl()?;
    let security_info = DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION;
    let path_wide = to_wide_null(path.as_os_str());

    // SAFETY: `path_wide` is null-terminated and `dacl` points into
    // `security_descriptor`, which stays alive for the call.
    let status = unsafe {
        SetNamedSecurityInfoW(
            PCWSTR(path_wide.as_ptr()),
            SE_FILE_OBJECT,
            security_info,
            None,
            None,
            Some(dacl),
            None,
        )
    };

    if status == ERROR_SUCCESS {
        Ok(())
    } else {
        Err(AuthenticationError::VaultHeaderInvalid)
    }
}

#[cfg(windows)]
fn owner_only_file_sddl() -> &'static str {
    "D:P(A;;FA;;;OW)(A;;FA;;;SY)(A;;FA;;;BA)"
}

#[cfg(windows)]
fn owner_only_directory_sddl() -> &'static str {
    "D:PAI(A;OICI;FA;;;OW)(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)"
}

#[cfg(windows)]
fn to_wide_null(value: &std::ffi::OsStr) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;

    let mut wide: Vec<u16> = value.encode_wide().collect();
    wide.push(0);
    wide
}

#[cfg(windows)]
fn to_wide_null_str(value: &str) -> Vec<u16> {
    let mut wide: Vec<u16> = value.encode_utf16().collect();
    wide.push(0);
    wide
}

#[cfg(windows)]
struct WindowsSecurityDescriptor {
    descriptor: windows::Win32::Security::PSECURITY_DESCRIPTOR,
}

#[cfg(windows)]
impl WindowsSecurityDescriptor {
    fn from_sddl(sddl: &str) -> Result<Self, AuthenticationError> {
        use windows::Win32::Security::Authorization::{
            ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
        };
        use windows::Win32::Security::PSECURITY_DESCRIPTOR;
        use windows::core::PCWSTR;

        let mut descriptor = PSECURITY_DESCRIPTOR::default();
        let sddl_wide = to_wide_null_str(sddl);

        // SAFETY: `sddl_wide` is a valid null-terminated UTF-16 string and
        // `descriptor` outlives the call.
        unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                PCWSTR(sddl_wide.as_ptr()),
                SDDL_REVISION_1,
                &mut descriptor,
                None,
            )
        }
        .map_err(|_| AuthenticationError::VaultHeaderInvalid)?;

        if descriptor.is_invalid() {
            return Err(AuthenticationError::VaultHeaderInvalid);
        }

        Ok(Self { descriptor })
    }

    fn security_attributes(&self) -> windows::Win32::Security::SECURITY_ATTRIBUTES {
        windows::Win32::Security::SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<windows::Win32::Security::SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: self.descriptor.0,
            bInheritHandle: windows::Win32::Foundation::BOOL(0),
        }
    }

    fn dacl(&self) -> Result<*const windows::Win32::Security::ACL, AuthenticationError> {
        use windows::Win32::Foundation::BOOL;
        use windows::Win32::Security::{ACL, GetSecurityDescriptorDacl};

        let mut present = BOOL(0);
        let mut defaulted = BOOL(0);
        let mut dacl: *mut ACL = std::ptr::null_mut();

        // SAFETY: `self.descriptor` is valid for the lifetime of `self`; the
        // out-pointers are valid stack references.
        unsafe {
            GetSecurityDescriptorDacl(self.descriptor, &mut present, &mut dacl, &mut defaulted)
        }
        .map_err(|_| AuthenticationError::VaultHeaderInvalid)?;

        if !present.as_bool() || dacl.is_null() {
            return Err(AuthenticationError::VaultHeaderInvalid);
        }

        Ok(dacl.cast_const())
    }
}

#[cfg(windows)]
impl Drop for WindowsSecurityDescriptor {
    fn drop(&mut self) {
        use windows::Win32::Foundation::{HLOCAL, LocalFree};

        // SAFETY: `descriptor` memory is owned by this instance and must be
        // released with `LocalFree`.
        unsafe {
            let _ = LocalFree(Some(HLOCAL(self.descriptor.0)));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(windows)]
    use std::path::Path;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_write_owner_only_writes_exact_bytes() {
        let directory = tempdir().expect("tempdir must succeed");
        let path = directory.path().join("header.json");
        let payload = b"{\"schema_version\":1}";

        write_owner_only(&path, payload)
            .await
            .expect("write must succeed");

        let recovered = std::fs::read(&path).expect("read must succeed");
        assert_eq!(recovered, payload);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn test_write_owner_only_sets_mode_0o600_on_unix() {
        use std::os::unix::fs::PermissionsExt;
        let directory = tempdir().expect("tempdir must succeed");
        let path = directory.path().join("header.json");

        write_owner_only(&path, b"payload")
            .await
            .expect("write must succeed");

        let metadata = std::fs::metadata(&path).expect("metadata must succeed");
        assert_eq!(metadata.permissions().mode() & 0o777, 0o600);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn test_write_owner_only_tightens_existing_permissions_on_unix() {
        use std::os::unix::fs::PermissionsExt;
        let directory = tempdir().expect("tempdir must succeed");
        let path = directory.path().join("header.json");

        std::fs::write(&path, b"existing payload").expect("seed write must succeed");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644))
            .expect("seed mode set must succeed");

        let initial = std::fs::metadata(&path).expect("metadata must succeed");
        assert_eq!(initial.permissions().mode() & 0o777, 0o644);

        write_owner_only(&path, b"updated payload")
            .await
            .expect("write must succeed");

        let metadata = std::fs::metadata(&path).expect("metadata must succeed");
        assert_eq!(metadata.permissions().mode() & 0o777, 0o600);
    }

    #[tokio::test]
    async fn test_write_owner_only_truncates_existing_content() {
        let directory = tempdir().expect("tempdir must succeed");
        let path = directory.path().join("header.json");

        write_owner_only(&path, b"longer initial content")
            .await
            .expect("first write must succeed");
        write_owner_only(&path, b"short")
            .await
            .expect("second write must succeed");

        let recovered = std::fs::read(&path).expect("read must succeed");
        assert_eq!(recovered, b"short");
    }

    #[tokio::test]
    async fn test_write_owner_only_new_rejects_existing_file_and_keeps_original_content() {
        let directory = tempdir().expect("tempdir must succeed");
        let path = directory.path().join("header.json");
        std::fs::write(&path, b"existing").expect("seed write must succeed");

        let result = write_owner_only_new(&path, b"replacement").await;
        assert!(matches!(
            result,
            Err(AuthenticationError::VaultHeaderInvalid)
        ));

        let recovered = std::fs::read(&path).expect("read must succeed");
        assert_eq!(recovered, b"existing");
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn test_write_owner_only_new_applies_owner_only_acl_on_windows() {
        let directory = tempdir().expect("tempdir must succeed");
        let path = directory.path().join("header.json");

        write_owner_only_new(&path, b"payload")
            .await
            .expect("write must succeed");

        let sddl = read_path_dacl_sddl_windows(&path);
        assert_owner_only_sddl_windows(&sddl);
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn test_write_owner_only_overwrite_reapplies_owner_only_acl_on_windows() {
        let directory = tempdir().expect("tempdir must succeed");
        let path = directory.path().join("header.json");
        std::fs::write(&path, b"seed").expect("seed write must succeed");

        apply_sddl_to_path_windows(&path, "D:P(A;;FA;;;WD)")
            .expect("widening acl must succeed in test");
        let widened = read_path_dacl_sddl_windows(&path);
        assert!(widened.contains(";;;WD"));

        write_owner_only(&path, b"payload")
            .await
            .expect("write must succeed");

        let sddl = read_path_dacl_sddl_windows(&path);
        assert_owner_only_sddl_windows(&sddl);
        assert!(!sddl.contains(";;;WD"));
    }

    #[tokio::test]
    async fn test_remove_if_exists_returns_ok_on_missing_file() {
        let directory = tempdir().expect("tempdir must succeed");
        let path = directory.path().join("missing.json");

        remove_if_exists(&path)
            .await
            .expect("missing file must not error");
    }

    #[tokio::test]
    async fn test_remove_if_exists_deletes_existing_file() {
        let directory = tempdir().expect("tempdir must succeed");
        let path = directory.path().join("delete-me.json");
        write_owner_only(&path, b"content")
            .await
            .expect("write must succeed");

        remove_if_exists(&path).await.expect("remove must succeed");

        assert!(!path.exists());
    }

    #[cfg(windows)]
    fn assert_owner_only_sddl_windows(sddl: &str) {
        assert!(sddl.contains("D:P"));
        assert!(sddl.contains(";;;OW"));
        assert!(sddl.contains(";;;SY"));
        assert!(sddl.contains(";;;BA"));
        assert!(!sddl.contains(";;;WD"));
        assert!(!sddl.contains(";;;BU"));
        assert!(!sddl.contains(";;;AU"));
    }

    #[cfg(windows)]
    fn read_path_dacl_sddl_windows(path: &Path) -> String {
        use windows::Win32::Foundation::{ERROR_SUCCESS, HLOCAL, LocalFree};
        use windows::Win32::Security::Authorization::{
            ConvertSecurityDescriptorToStringSecurityDescriptorW, GetNamedSecurityInfoW,
            SDDL_REVISION_1, SE_FILE_OBJECT,
        };
        use windows::Win32::Security::{DACL_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR};
        use windows::core::{PCWSTR, PWSTR};

        let path_wide = to_wide_null(path.as_os_str());
        let mut descriptor = PSECURITY_DESCRIPTOR::default();

        // SAFETY: `path_wide` is a valid null-terminated UTF-16 string and
        // `descriptor` points to writable stack memory.
        let status = unsafe {
            GetNamedSecurityInfoW(
                PCWSTR(path_wide.as_ptr()),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION,
                None,
                None,
                None,
                None,
                &mut descriptor,
            )
        };
        assert_eq!(status, ERROR_SUCCESS, "GetNamedSecurityInfoW must succeed");
        assert!(
            !descriptor.is_invalid(),
            "security descriptor must be valid"
        );

        let mut string_descriptor = PWSTR::null();
        // SAFETY: `descriptor` is valid and output pointer is writable.
        unsafe {
            ConvertSecurityDescriptorToStringSecurityDescriptorW(
                descriptor,
                SDDL_REVISION_1,
                DACL_SECURITY_INFORMATION,
                &mut string_descriptor,
                None,
            )
        }
        .expect("ConvertSecurityDescriptorToStringSecurityDescriptorW must succeed");

        let mut length = 0usize;
        // SAFETY: `string_descriptor` points to a valid null-terminated
        // UTF-16 string allocated by Windows.
        unsafe {
            while *string_descriptor.0.add(length) != 0 {
                length += 1;
            }
        }

        // SAFETY: `string_descriptor` points to at least `length` UTF-16 code
        // units before the terminator.
        let sddl = unsafe {
            String::from_utf16_lossy(std::slice::from_raw_parts(string_descriptor.0, length))
        };

        // SAFETY: both buffers were allocated by Windows and must be released
        // with `LocalFree`.
        unsafe {
            let _ = LocalFree(Some(HLOCAL(descriptor.0)));
            let _ = LocalFree(Some(HLOCAL(string_descriptor.0.cast())));
        }

        sddl
    }
}
