//! Windows `VirtualLock` / `VirtualUnlock` wrapper.

use std::ffi::c_void;

use windows::Win32::System::Memory::{VirtualLock, VirtualUnlock};

use crate::memory::error::MemoryLockError;

/// Locks `length` bytes starting at `pointer` into physical memory via
/// `VirtualLock`.
///
/// # Safety
/// `pointer` must be the start of a live allocation of at least `length`
/// bytes.
pub(crate) unsafe fn lock_memory(pointer: *mut u8, length: usize) -> Result<(), MemoryLockError> {
    #[cfg(test)]
    if super::fault_injection::is_force_lock_failure() {
        return Err(MemoryLockError::PlatformFailure {
            platform_message: platform_failure_message(),
        });
    }

    // SAFETY: caller guarantees `pointer` starts a live allocation of at
    // least `length` bytes. `VirtualLock` does not dereference it as data.
    let result = unsafe { VirtualLock(pointer as *const c_void, length) };
    if result.is_ok() {
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

    // SAFETY: caller guarantees this matches a prior successful `VirtualLock`.
    let _ = unsafe { VirtualUnlock(pointer as *const c_void, length) };
}

fn platform_failure_message() -> String {
    String::from(
        "Cannot lock session keys in memory (system working set quota exceeded). Try closing other applications or restarting Arx Runa.",
    )
}
