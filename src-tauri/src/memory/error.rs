//! Error types for the memory module.

use thiserror::Error;

/// Errors produced by platform-specific memory locking operations.
#[non_exhaustive]
#[derive(Debug, Error)]
pub enum MemoryLockError {
    /// The OS refused to lock the buffer into physical memory.
    ///
    /// `platform_message` is the user-facing string defined by the
    /// authentication design (see
    /// `docs/architecture/designs/authentication-and-session-management/design.md`
    /// "Memory locking" subsection).
    #[error("{platform_message}")]
    PlatformFailure { platform_message: String },
}

#[cfg(test)]
mod tests {
    use super::MemoryLockError;

    #[test]
    fn test_memory_lock_error_display_forwards_platform_message() {
        let error = MemoryLockError::PlatformFailure {
            platform_message: String::from(
                "Cannot lock memory. Ensure sufficient physical RAM is available and try again.",
            ),
        };

        assert_eq!(
            error.to_string(),
            "Cannot lock memory. Ensure sufficient physical RAM is available and try again."
        );
    }
}
