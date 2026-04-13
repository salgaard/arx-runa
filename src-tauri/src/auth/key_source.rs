//! `KeySource` trait and its production and test implementations.

use std::fs::File;
use std::io::{ErrorKind, Read};
use std::path::{Path, PathBuf};

use zeroize::Zeroizing;

use crate::auth::error::KeySourceError;

/// Reads a 32-byte USB key file.
pub trait KeySource: Send + Sync {
    /// Reads the underlying key file and returns its 32-byte content.
    fn read_key(&self) -> Result<Zeroizing<[u8; 32]>, KeySourceError>;
}

/// Reads a key file from a filesystem path.
#[derive(Debug, Clone)]
pub struct FileKeySource {
    path: PathBuf,
}

impl FileKeySource {
    /// Builds a key source that reads `path` on every `read_key` call.
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// Returns the configured path.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl KeySource for FileKeySource {
    fn read_key(&self) -> Result<Zeroizing<[u8; 32]>, KeySourceError> {
        let mut file = match File::open(&self.path) {
            Ok(file) => file,
            Err(error) if error.kind() == ErrorKind::NotFound => {
                return Err(KeySourceError::NotFound);
            }
            Err(error) => return Err(KeySourceError::ReadFailed(error)),
        };

        let metadata = file.metadata().map_err(KeySourceError::ReadFailed)?;
        if metadata.len() != 32 {
            return Err(KeySourceError::InvalidSize {
                actual: metadata.len() as usize,
            });
        }

        let mut buffer = Zeroizing::new([0u8; 32]);
        file.read_exact(buffer.as_mut())
            .map_err(KeySourceError::ReadFailed)?;
        Ok(buffer)
    }
}

/// A `KeySource` that returns caller-controlled bytes.
#[cfg(any(test, feature = "test-utils"))]
#[derive(Debug, Clone)]
pub struct MockKeySource {
    bytes: [u8; 32],
}

#[cfg(any(test, feature = "test-utils"))]
impl MockKeySource {
    /// Creates a mock source with the provided bytes.
    pub fn new(bytes: [u8; 32]) -> Self {
        Self { bytes }
    }
}

#[cfg(any(test, feature = "test-utils"))]
impl KeySource for MockKeySource {
    fn read_key(&self) -> Result<Zeroizing<[u8; 32]>, KeySourceError> {
        Ok(Zeroizing::new(self.bytes))
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{FileKeySource, KeySource, MockKeySource};
    use crate::auth::error::KeySourceError;

    /// Writes a file with the provided content and returns the containing temp directory and path.
    fn write_temp_file(contents: &[u8]) -> (tempfile::TempDir, PathBuf) {
        let directory = tempfile::tempdir().expect("temp directory should be created");
        let path = directory.path().join("key.bin");
        std::fs::write(&path, contents).expect("temp file should be written");
        (directory, path)
    }

    #[test]
    fn test_file_key_source_reads_valid_32_byte_file() {
        let expected = [0xABu8; 32];
        let (_directory, path) = write_temp_file(&expected);
        let key_source = FileKeySource::new(path);

        let actual = key_source.read_key().expect("32-byte key should be read");

        assert_eq!(*actual, expected);
    }

    #[test]
    fn test_file_key_source_returns_invalid_size_for_31_bytes() {
        let (_directory, path) = write_temp_file(&[0x11u8; 31]);
        let key_source = FileKeySource::new(path);

        let error = key_source.read_key().expect_err("31-byte key must fail");

        let KeySourceError::InvalidSize { actual } = error else {
            panic!("expected invalid-size error");
        };
        assert_eq!(actual, 31);
    }

    #[test]
    fn test_file_key_source_returns_invalid_size_for_33_bytes() {
        let (_directory, path) = write_temp_file(&[0x22u8; 33]);
        let key_source = FileKeySource::new(path);

        let error = key_source.read_key().expect_err("33-byte key must fail");

        let KeySourceError::InvalidSize { actual } = error else {
            panic!("expected invalid-size error");
        };
        assert_eq!(actual, 33);
    }

    #[test]
    fn test_file_key_source_returns_invalid_size_for_empty_file() {
        let (_directory, path) = write_temp_file(&[]);
        let key_source = FileKeySource::new(path);

        let error = key_source.read_key().expect_err("empty key must fail");

        let KeySourceError::InvalidSize { actual } = error else {
            panic!("expected invalid-size error");
        };
        assert_eq!(actual, 0);
    }

    #[test]
    fn test_file_key_source_returns_not_found_for_missing_path() {
        let missing_path =
            std::env::temp_dir().join(format!("missing-key-{}.bin", uuid::Uuid::new_v4()));
        let key_source = FileKeySource::new(missing_path);

        let error = key_source
            .read_key()
            .expect_err("missing path must produce not-found");

        let KeySourceError::NotFound = error else {
            panic!("expected not-found error");
        };
    }

    #[test]
    fn test_file_key_source_returns_read_failed_for_directory_path() {
        let directory = tempfile::tempdir().expect("temp directory should be created");
        let key_source = FileKeySource::new(directory.path().to_path_buf());

        let error = key_source
            .read_key()
            .expect_err("directory path must produce read-failed");

        let KeySourceError::ReadFailed(_) = error else {
            panic!("expected read-failed error");
        };
    }

    #[test]
    fn test_mock_key_source_returns_controlled_bytes() {
        let expected = [0x5Au8; 32];
        let key_source = MockKeySource::new(expected);

        let actual = key_source.read_key().expect("mock should return bytes");

        assert_eq!(*actual, expected);
    }
}
