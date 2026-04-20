//! Platform-specific memory locking primitives.
//!
//! All unsafe code touching `mlock` / `VirtualLock` lives in inner
//! submodules. This module only re-exports the two functions consumed by
//! `SecureBytes`.

#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

#[cfg(test)]
pub(crate) use fault_injection::set_force_lock_failure;
#[cfg(test)]
pub(crate) use fault_injection::{clear_last_unlock_snapshot, take_last_unlock_snapshot};
#[cfg(unix)]
pub(crate) use unix::{lock_memory, unlock_memory};
#[cfg(windows)]
pub(crate) use windows::{lock_memory, unlock_memory};

#[cfg(test)]
mod fault_injection {
    use std::cell::{Cell, RefCell};

    thread_local! {
        static FORCE_LOCK_FAILURE: Cell<bool> = const { Cell::new(false) };
        static LAST_UNLOCK_SNAPSHOT: RefCell<Option<Vec<u8>>> = const { RefCell::new(None) };
    }

    pub(crate) fn set_force_lock_failure(value: bool) {
        FORCE_LOCK_FAILURE.with(|cell| cell.set(value));
    }

    pub(crate) fn is_force_lock_failure() -> bool {
        FORCE_LOCK_FAILURE.with(|cell| cell.get())
    }

    pub(crate) fn clear_last_unlock_snapshot() {
        LAST_UNLOCK_SNAPSHOT.with(|snapshot| snapshot.replace(None));
    }

    pub(crate) fn take_last_unlock_snapshot() -> Option<Vec<u8>> {
        LAST_UNLOCK_SNAPSHOT.with(|snapshot| snapshot.borrow_mut().take())
    }

    pub(crate) unsafe fn record_unlock_snapshot(pointer: *const u8, length: usize) {
        // SAFETY: caller guarantees pointer points to a live allocation with
        // at least `length` bytes while this snapshot is taken.
        let bytes = unsafe { std::slice::from_raw_parts(pointer, length).to_vec() };
        LAST_UNLOCK_SNAPSHOT.with(|snapshot| snapshot.replace(Some(bytes)));
    }
}
