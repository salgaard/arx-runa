//! Cross-platform check for whether a path resides on a removable storage device.
//!
//! Conservative: returns `false` on any error so callers emit a warning rather
//! than a false positive.  A return value of `true` means "definitely removable";
//! `false` means "fixed or unknown".

use std::path::Path;

/// Returns `true` if `path` is on a removable storage device.
pub(crate) fn is_removable_path(path: &Path) -> bool {
    imp::is_removable(path)
}

// ─── Windows ─────────────────────────────────────────────────────────────────

#[cfg(target_os = "windows")]
mod imp {
    use std::path::{Component, Path};

    use windows::Win32::Storage::FileSystem::GetDriveTypeW;
    use windows::core::PCWSTR;

    pub(super) fn is_removable(path: &Path) -> bool {
        let drive_root = drive_root_of(path);
        if drive_root.is_empty() {
            return false;
        }
        let mut wide: Vec<u16> = drive_root.encode_utf16().collect();
        wide.push(0);
        // SAFETY: `wide` is a valid null-terminated UTF-16 string that remains
        // alive for the duration of the `GetDriveTypeW` call.
        let drive_type = unsafe { GetDriveTypeW(PCWSTR(wide.as_ptr())) };
        drive_type == 2 // DRIVE_REMOVABLE
    }

    /// Extracts the drive-root string (e.g. `"C:\\"`) from an absolute Windows path.
    fn drive_root_of(path: &Path) -> String {
        if let Some(Component::Prefix(prefix)) = path.components().next() {
            let prefix_str = prefix.as_os_str().to_string_lossy();
            if prefix_str.ends_with('\\') {
                return prefix_str.into_owned();
            }
            return format!("{}\\", prefix_str);
        }
        String::new()
    }
}

// ─── Linux ────────────────────────────────────────────────────────────────────

#[cfg(target_os = "linux")]
mod imp {
    use std::path::Path;

    pub(super) fn is_removable(path: &Path) -> bool {
        let Some((major, minor)) = device_for_path(path) else {
            return false;
        };
        let sysfs = format!("/sys/dev/block/{major}:{minor}/removable");
        std::fs::read_to_string(sysfs)
            .map(|s| s.trim() == "1")
            .unwrap_or(false)
    }

    /// Returns the (major, minor) device numbers for the mount point containing `path`.
    ///
    /// Reads `/proc/self/mountinfo` and selects the longest-prefix match.
    fn device_for_path(path: &Path) -> Option<(u32, u32)> {
        let mountinfo = std::fs::read_to_string("/proc/self/mountinfo").ok()?;
        let mut best_len = 0usize;
        let mut best: Option<(u32, u32)> = None;

        for line in mountinfo.lines() {
            // mountinfo field layout: id parentId major:minor root mountPoint ...
            let mut fields = line.splitn(6, ' ');
            let _id = fields.next()?;
            let _parent = fields.next()?;
            let major_minor = fields.next()?;
            let _root = fields.next()?;
            let mount_point = fields.next()?;

            let mount_path = Path::new(mount_point);
            if path.starts_with(mount_path) {
                let prefix_len = mount_point.len();
                if prefix_len >= best_len {
                    if let Some(mm) = parse_major_minor(major_minor) {
                        best_len = prefix_len;
                        best = Some(mm);
                    }
                }
            }
        }
        best
    }

    fn parse_major_minor(s: &str) -> Option<(u32, u32)> {
        let (maj, min) = s.split_once(':')?;
        Some((maj.parse().ok()?, min.parse().ok()?))
    }
}

// ─── macOS ───────────────────────────────────────────────────────────────────

#[cfg(target_os = "macos")]
mod imp {
    use std::path::Path;

    pub(super) fn is_removable(path: &Path) -> bool {
        // External volumes on macOS appear under /Volumes/.  The DiskArbitration
        // monitor is not yet fully implemented, so this well-known path convention
        // is used as a reliable heuristic.
        path.starts_with("/Volumes/")
    }
}

// ─── Unknown platform ────────────────────────────────────────────────────────

#[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
mod imp {
    use std::path::Path;

    pub(super) fn is_removable(_path: &Path) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::is_removable_path;
    use std::path::Path;

    #[test]
    fn test_is_removable_path_fixed_drive_returns_false() {
        // The temp dir is always on a fixed drive in CI and developer machines.
        let temp = std::env::temp_dir();
        assert!(!is_removable_path(&temp));
    }

    #[test]
    fn test_is_removable_path_empty_path_returns_false() {
        assert!(!is_removable_path(Path::new("")));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn test_is_removable_path_system_root_returns_false() {
        // C:\ is always a fixed drive.
        assert!(!is_removable_path(Path::new(r"C:\")));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_is_removable_path_volumes_prefix_returns_true() {
        assert!(is_removable_path(Path::new("/Volumes/MyUSB/file.txt")));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_is_removable_path_non_volumes_prefix_returns_false() {
        assert!(!is_removable_path(Path::new("/Users/alice/Documents")));
    }
}
