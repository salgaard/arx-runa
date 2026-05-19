/// Applies the most restrictive file permissions compatible with the platform.
///
/// - **Unix**: mode `0o600` (owner read/write only).
/// - **Windows**: DACL `D:P(A;;FA;;;OW)(A;;FA;;;SY)(A;;FA;;;BA)` — Full Access
///   for the file owner (`OW`), SYSTEM (`SY`), and Built-in Administrators (`BA`).
///   SYSTEM and BA are included because excluding them breaks Windows Defender,
///   VSS, and system recovery. This is an accepted platform limitation; see
///   `docs/architecture/design-invariants.md` § "Out-of-Scope Architectural Limitations".
///
/// The file must already exist.
pub(crate) fn set_file_private_permissions(path: &std::path::Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::fs::Permissions;
        use std::os::unix::fs::PermissionsExt;
        return std::fs::set_permissions(path, Permissions::from_mode(0o600));
    }

    #[cfg(windows)]
    {
        return apply_owner_only_acl_windows(path);
    }

    #[allow(unreachable_code)]
    {
        let _ = path;
        Ok(())
    }
}

#[cfg(windows)]
pub(crate) fn apply_owner_only_acl_windows(path: &std::path::Path) -> std::io::Result<()> {
    use windows::Win32::Foundation::ERROR_SUCCESS;
    use windows::Win32::Security::Authorization::{SE_FILE_OBJECT, SetNamedSecurityInfoW};
    use windows::Win32::Security::{
        DACL_SECURITY_INFORMATION, PROTECTED_DACL_SECURITY_INFORMATION,
    };
    use windows::core::PCWSTR;

    const FILE_SDDL: &str = "D:P(A;;FA;;;OW)(A;;FA;;;SY)(A;;FA;;;BA)";

    let security_descriptor = WindowsSecurityDescriptor::from_sddl(FILE_SDDL)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::PermissionDenied, e.message()))?;
    let dacl = security_descriptor
        .dacl()
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::PermissionDenied, e.message()))?;

    let security_info = DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION;
    let path_wide = to_wide_null(path.as_os_str());

    // SAFETY: `path_wide` is null-terminated and `dacl` points into
    // `security_descriptor`, which stays alive for the duration of this call.
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
        Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            format!("SetNamedSecurityInfoW failed: {status:?}"),
        ))
    }
}

#[cfg(windows)]
struct WindowsSecurityDescriptor {
    descriptor: windows::Win32::Security::PSECURITY_DESCRIPTOR,
}

#[cfg(windows)]
impl WindowsSecurityDescriptor {
    fn from_sddl(sddl: &str) -> windows::core::Result<Self> {
        use windows::Win32::Security::Authorization::{
            ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
        };
        use windows::Win32::Security::PSECURITY_DESCRIPTOR;
        use windows::core::PCWSTR;

        let mut descriptor = PSECURITY_DESCRIPTOR::default();
        let sddl_wide = to_wide_null_str(sddl);

        // SAFETY: `sddl_wide` is null-terminated; output pointer is writable.
        unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                PCWSTR(sddl_wide.as_ptr()),
                SDDL_REVISION_1,
                &mut descriptor,
                None,
            )
        }?;
        Ok(Self { descriptor })
    }

    fn dacl(&self) -> windows::core::Result<*const windows::Win32::Security::ACL> {
        use windows::Win32::Foundation::BOOL;
        use windows::Win32::Security::{ACL, GetSecurityDescriptorDacl};

        let mut present = BOOL(0);
        let mut defaulted = BOOL(0);
        let mut dacl: *mut ACL = std::ptr::null_mut();

        // SAFETY: `self.descriptor` is valid for the lifetime of `self`;
        // out-pointers are valid stack references.
        unsafe {
            GetSecurityDescriptorDacl(self.descriptor, &mut present, &mut dacl, &mut defaulted)
        }?;
        Ok(dacl.cast_const())
    }
}

#[cfg(windows)]
impl Drop for WindowsSecurityDescriptor {
    fn drop(&mut self) {
        use windows::Win32::Foundation::{HLOCAL, LocalFree};

        // SAFETY: `descriptor` was allocated by
        // ConvertStringSecurityDescriptorToSecurityDescriptorW and must be
        // released with `LocalFree`.
        unsafe {
            let _ = LocalFree(Some(HLOCAL(self.descriptor.0)));
        }
    }
}

#[cfg(windows)]
fn to_wide_null(s: &std::ffi::OsStr) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    let mut wide: Vec<u16> = s.encode_wide().collect();
    wide.push(0);
    wide
}

#[cfg(windows)]
fn to_wide_null_str(s: &str) -> Vec<u16> {
    let mut wide: Vec<u16> = s.encode_utf16().collect();
    wide.push(0);
    wide
}
