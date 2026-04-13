//! POSIX `mlock` / `munlock` wrapper.
//!
//! Covers both Linux and macOS. `libc::mlock` / `libc::munlock` are POSIX
//! calls with identical signatures on both targets.

use std::ffi::c_void;

use crate::memory::error::MemoryLockError;

/// Locks `length` bytes starting at `pointer` into physical memory.
///
/// # Safety
/// `pointer` must be the start of a live allocation of at least `length`
/// bytes. The caller retains ownership of the allocation for the lifetime
/// of the lock.
pub(crate) unsafe fn lock_memory(pointer: *mut u8, length: usize) -> Result<(), MemoryLockError> {
    #[cfg(test)]
    if super::fault_injection::is_force_lock_failure() {
        return Err(MemoryLockError::PlatformFailure {
            platform_message: platform_failure_message(),
        });
    }

    // SAFETY: caller guarantees `pointer` starts a live allocation of at least
    // `length` bytes; `mlock` does not dereference the pointer as data.
    let result = unsafe { libc::mlock(pointer as *const c_void, length) };
    if result == 0 {
        Ok(())
    } else {
        Err(MemoryLockError::PlatformFailure {
            platform_message: platform_failure_message(),
        })
    }
}

/// Unlocks `length` bytes starting at `pointer`.
///
/// # Safety
/// `pointer` and `length` must match a prior successful `lock_memory` call.
pub(crate) unsafe fn unlock_memory(pointer: *mut u8, length: usize) {
    #[cfg(test)]
    // SAFETY: caller guarantees `pointer..pointer+length` is still live here.
    unsafe {
        super::fault_injection::record_unlock_snapshot(pointer as *const u8, length);
    }

    // SAFETY: caller guarantees this matches a prior successful `mlock` call.
    let _ = unsafe { libc::munlock(pointer as *const c_void, length) };
}

#[cfg(target_os = "linux")]
fn platform_failure_message() -> String {
    String::from(
        "Cannot lock memory. Increase the memory lock limit: `ulimit -l unlimited` or edit `/etc/security/limits.conf`.",
    )
}

#[cfg(target_os = "macos")]
fn platform_failure_message() -> String {
    String::from("Cannot lock memory. Ensure sufficient physical RAM is available and try again.")
}
