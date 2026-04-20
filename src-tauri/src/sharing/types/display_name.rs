//! Display-name newtype for contacts.

use crate::sharing::error::SharingError;

/// User-facing contact display name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisplayName(String);

impl DisplayName {
    /// Validates and stores a display name.
    pub fn new(value: &str) -> Result<Self, SharingError> {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Err(SharingError::EmptyDisplayName);
        }
        Ok(Self(trimmed.to_owned()))
    }

    /// Returns the validated display-name string.
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

#[cfg(test)]
mod tests {
    use crate::sharing::SharingError;
    use crate::sharing::types::DisplayName;

    /// Verifies empty and whitespace-only input is rejected.
    #[test]
    fn test_display_name_new_for_empty_or_whitespace_returns_empty_display_name_error() {
        assert!(matches!(
            DisplayName::new(""),
            Err(SharingError::EmptyDisplayName)
        ));
        assert!(matches!(
            DisplayName::new("   "),
            Err(SharingError::EmptyDisplayName)
        ));
    }

    /// Verifies surrounding whitespace is trimmed and preserved correctly.
    #[test]
    fn test_display_name_new_with_trimmed_input_returns_trimmed_value() {
        let display_name =
            DisplayName::new("  Alice Example  ").expect("display name validation should succeed");

        assert_eq!(display_name.as_str(), "Alice Example");
    }
}
