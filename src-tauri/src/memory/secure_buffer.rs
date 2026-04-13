//! Memory-locked, zero-on-drop byte buffer.
//!
//! `SecureBytes<N>` is the canonical container for Arx Runa session-key
//! bytes. Each instance allocates a heap buffer in zero state, locks it
//! before writes, and zeroes before unlock on drop.

use zeroize::Zeroize;

use crate::memory::error::MemoryLockError;
use crate::memory::platform;

/// Fixed-size byte buffer whose backing pages are locked into physical memory
/// and whose contents are zeroed on drop.
pub(crate) struct SecureBytes<const N: usize> {
    buffer: Box<[u8; N]>,
}

impl<const N: usize> SecureBytes<N> {
    /// Allocates and locks a zero-initialized buffer.
    pub(crate) fn new() -> Result<Self, MemoryLockError> {
        let mut buffer: Box<[u8; N]> = Box::new([0u8; N]);
        // SAFETY: `buffer` is a live allocation of exactly `N` bytes.
        unsafe { platform::lock_memory(buffer.as_mut_ptr(), N) }?;
        Ok(Self { buffer })
    }

    /// Returns a mutable view of the locked bytes.
    pub(crate) fn as_mut(&mut self) -> &mut [u8; N] {
        &mut self.buffer
    }

    /// Returns a read-only view of the locked bytes.
    pub(crate) fn expose(&self) -> &[u8; N] {
        &self.buffer
    }
}

impl<const N: usize> Zeroize for SecureBytes<N> {
    fn zeroize(&mut self) {
        self.buffer.as_mut().zeroize();
    }
}

impl<const N: usize> Drop for SecureBytes<N> {
    fn drop(&mut self) {
        self.buffer.as_mut().zeroize();
        // SAFETY: pointer/length match the prior successful `lock_memory` call.
        unsafe {
            platform::unlock_memory(self.buffer.as_mut_ptr(), N);
        }
    }
}

#[cfg(test)]
mod tests {
    use zeroize::Zeroize;

    use super::SecureBytes;
    use crate::memory::platform::{
        clear_last_unlock_snapshot, set_force_lock_failure, take_last_unlock_snapshot,
    };

    struct ForceLockFailureGuard;

    impl ForceLockFailureGuard {
        fn new() -> Self {
            set_force_lock_failure(true);
            Self
        }
    }

    impl Drop for ForceLockFailureGuard {
        fn drop(&mut self) {
            set_force_lock_failure(false);
        }
    }

    #[test]
    fn test_secure_bytes_new_zero_initializes_buffer() {
        let buffer = SecureBytes::<32>::new().expect("lock should succeed");
        assert_eq!(*buffer.expose(), [0u8; 32]);
    }

    #[test]
    fn test_secure_bytes_as_mut_writes_survive() {
        let mut buffer = SecureBytes::<32>::new().expect("lock should succeed");
        buffer.as_mut().copy_from_slice(&[0xABu8; 32]);
        assert_eq!(*buffer.expose(), [0xABu8; 32]);
    }

    #[test]
    fn test_secure_bytes_zeroize_trait_clears_buffer_in_place() {
        let mut buffer = SecureBytes::<32>::new().expect("lock should succeed");
        buffer.as_mut().copy_from_slice(&[0xEFu8; 32]);
        let pointer = buffer.expose().as_ptr();

        // SAFETY: `pointer` originates from a live allocation owned by `buffer`.
        let before = unsafe { std::slice::from_raw_parts(pointer, 32) };
        assert_eq!(before, &[0xEFu8; 32]);

        Zeroize::zeroize(&mut buffer);

        // SAFETY: same live allocation and length.
        let after = unsafe { std::slice::from_raw_parts(pointer, 32) };
        assert_eq!(after, &[0u8; 32]);
    }

    #[test]
    fn test_secure_bytes_drop_zeroizes_buffer_before_unlock() {
        clear_last_unlock_snapshot();

        let mut buffer = SecureBytes::<32>::new().expect("lock should succeed");
        buffer.as_mut().copy_from_slice(&[0xCDu8; 32]);
        drop(buffer);

        let snapshot =
            take_last_unlock_snapshot().expect("unlock snapshot should exist after drop");
        assert_eq!(snapshot, vec![0u8; 32]);
    }

    #[test]
    fn test_secure_bytes_new_returns_platform_failure_when_lock_is_forced_to_fail() {
        let _guard = ForceLockFailureGuard::new();
        let result = SecureBytes::<32>::new();
        let error = match result {
            Ok(_) => panic!("forced lock failure should propagate"),
            Err(error) => error,
        };
        let crate::memory::error::MemoryLockError::PlatformFailure { platform_message } = error;
        assert!(!platform_message.is_empty());
    }
}
