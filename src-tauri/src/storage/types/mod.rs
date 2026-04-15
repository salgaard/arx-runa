//! Domain types for the storage module.
//!
//! Newtypes added in implementation phases.

/// Opaque cloud-side blob name.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BlobName(String);

impl BlobName {
    /// Creates a blob-name newtype from an owned string.
    pub fn new(name: String) -> Self {
        Self(name)
    }

    /// Returns the underlying blob-name string slice.
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl From<String> for BlobName {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

impl From<&str> for BlobName {
    fn from(value: &str) -> Self {
        Self::new(value.to_owned())
    }
}
