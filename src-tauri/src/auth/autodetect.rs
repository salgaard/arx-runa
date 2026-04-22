//! BLAKE3-based auto-detection of USB key files on mounted volumes.

use std::io::Read;
use std::path::{Path, PathBuf};

use subtle::ConstantTimeEq;
use walkdir::WalkDir;
use zeroize::Zeroizing;

use crate::auth::error::KeySourceError;
use crate::crypto::Blake3Hash;

const KEY_FILE_SIZE: u64 = 32;
const MAX_SCAN_DEPTH: usize = 8;

/// Scans `mount_path` for a 32-byte file whose BLAKE3 hash matches `reference_hash`.
pub async fn find_key_file(
    mount_path: &Path,
    reference_hash: &Blake3Hash,
) -> Result<Option<PathBuf>, KeySourceError> {
    let mount_path = mount_path.to_path_buf();
    let reference_hash = *reference_hash;

    tokio::task::spawn_blocking(move || scan_blocking(&mount_path, &reference_hash))
        .await
        .map_err(|join_error| KeySourceError::IoFailed(std::io::Error::other(join_error)))?
}

/// Performs the blocking scan.
fn scan_blocking(
    mount_path: &Path,
    reference_hash: &Blake3Hash,
) -> Result<Option<PathBuf>, KeySourceError> {
    if !mount_path.exists() {
        return Err(KeySourceError::IoFailed(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("mount path does not exist: {}", mount_path.display()),
        )));
    }

    let walker = WalkDir::new(mount_path)
        .follow_links(false)
        .max_depth(MAX_SCAN_DEPTH)
        .into_iter()
        .filter_entry(|entry| !is_system_directory(entry.file_name().to_string_lossy().as_ref()));

    for entry in walker {
        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => continue,
        };
        if !entry.file_type().is_file() {
            continue;
        }

        let metadata = match entry.metadata() {
            Ok(metadata) => metadata,
            Err(_) => continue,
        };
        if metadata.len() != KEY_FILE_SIZE {
            continue;
        }

        let mut file = match std::fs::File::open(entry.path()) {
            Ok(file) => file,
            Err(_) => continue,
        };
        let mut buffer = Zeroizing::new([0u8; 32]);
        if file.read_exact(buffer.as_mut()).is_err() {
            continue;
        }

        let hash = blake3::hash(buffer.as_ref());
        if hash.as_bytes().ct_eq(&reference_hash.0).into() {
            return Ok(Some(entry.into_path()));
        }
    }

    Ok(None)
}

/// Returns whether the provided directory name should be skipped during scanning.
fn is_system_directory(name: &str) -> bool {
    matches!(
        name,
        "System Volume Information"
            | "$RECYCLE.BIN"
            | ".Trashes"
            | ".Spotlight-V100"
            | ".fseventsd"
    )
}

#[cfg(test)]
mod tests {
    use super::find_key_file;
    use crate::auth::error::KeySourceError;
    use crate::crypto::Blake3Hash;

    /// Creates a deterministic 32-byte key payload and its BLAKE3 hash.
    fn key_material(seed: u8) -> ([u8; 32], Blake3Hash) {
        let bytes = [seed; 32];
        let hash = Blake3Hash(*blake3::hash(&bytes).as_bytes());
        (bytes, hash)
    }

    #[tokio::test]
    async fn test_find_key_file_matches_single_32_byte_file_at_root() {
        let mount_directory = tempfile::tempdir().expect("mount tempdir should be created");
        let key_path = mount_directory.path().join("key.bin");
        let (bytes, reference_hash) = key_material(0x10);
        std::fs::write(&key_path, bytes).expect("key file should be written");

        let found = find_key_file(mount_directory.path(), &reference_hash)
            .await
            .expect("scan should succeed");

        assert_eq!(found, Some(key_path));
    }

    #[tokio::test]
    async fn test_find_key_file_matches_file_in_subdirectory() {
        let mount_directory = tempfile::tempdir().expect("mount tempdir should be created");
        let nested_directory = mount_directory.path().join("a").join("b");
        std::fs::create_dir_all(&nested_directory).expect("nested directory should be created");
        let key_path = nested_directory.join("key.bin");
        let (bytes, reference_hash) = key_material(0x11);
        std::fs::write(&key_path, bytes).expect("key file should be written");

        let found = find_key_file(mount_directory.path(), &reference_hash)
            .await
            .expect("scan should succeed");

        assert_eq!(found, Some(key_path));
    }

    #[tokio::test]
    async fn test_find_key_file_ignores_non_32_byte_files() {
        let mount_directory = tempfile::tempdir().expect("mount tempdir should be created");
        std::fs::write(mount_directory.path().join("short.bin"), [0x22u8; 31])
            .expect("31-byte file should be written");
        std::fs::write(mount_directory.path().join("long.bin"), [0x33u8; 33])
            .expect("33-byte file should be written");
        let (_, reference_hash) = key_material(0x44);
        std::fs::write(mount_directory.path().join("wrong.bin"), [0x99u8; 32])
            .expect("non-matching 32-byte file should be written");

        let found = find_key_file(mount_directory.path(), &reference_hash)
            .await
            .expect("scan should succeed");

        assert_eq!(found, None);
    }

    #[tokio::test]
    async fn test_find_key_file_returns_none_when_no_match() {
        let mount_directory = tempfile::tempdir().expect("mount tempdir should be created");
        std::fs::write(mount_directory.path().join("candidate.bin"), [0x01u8; 32])
            .expect("candidate file should be written");
        let (_, reference_hash) = key_material(0x02);

        let found = find_key_file(mount_directory.path(), &reference_hash)
            .await
            .expect("scan should succeed");

        assert_eq!(found, None);
    }

    #[tokio::test]
    async fn test_find_key_file_returns_none_when_mount_is_empty() {
        let mount_directory = tempfile::tempdir().expect("mount tempdir should be created");
        let (_, reference_hash) = key_material(0x55);

        let found = find_key_file(mount_directory.path(), &reference_hash)
            .await
            .expect("scan should succeed");

        assert_eq!(found, None);
    }

    #[tokio::test]
    async fn test_find_key_file_returns_io_failed_when_mount_path_does_not_exist() {
        let missing_mount_path =
            std::env::temp_dir().join(format!("missing-mount-{}", uuid::Uuid::new_v4()));
        let (_, reference_hash) = key_material(0x66);

        let error = find_key_file(&missing_mount_path, &reference_hash)
            .await
            .expect_err("missing mount path should fail");

        let KeySourceError::IoFailed(source) = error else {
            panic!("expected io-failed error");
        };
        assert_eq!(source.kind(), std::io::ErrorKind::NotFound);
    }

    #[tokio::test]
    async fn test_find_key_file_finds_correct_file_among_many_32_byte_files() {
        let mount_directory = tempfile::tempdir().expect("mount tempdir should be created");
        let key_path = mount_directory.path().join("target.bin");
        std::fs::write(mount_directory.path().join("candidate-a.bin"), [0x0Au8; 32])
            .expect("candidate a should be written");
        std::fs::write(mount_directory.path().join("candidate-b.bin"), [0x0Bu8; 32])
            .expect("candidate b should be written");
        let (bytes, reference_hash) = key_material(0x0C);
        std::fs::write(&key_path, bytes).expect("target should be written");

        let found = find_key_file(mount_directory.path(), &reference_hash)
            .await
            .expect("scan should succeed");

        assert_eq!(found, Some(key_path));
    }

    #[tokio::test]
    async fn test_find_key_file_skips_system_directories() {
        let mount_directory = tempfile::tempdir().expect("mount tempdir should be created");
        let system_directory = mount_directory.path().join("System Volume Information");
        std::fs::create_dir_all(&system_directory).expect("system directory should be created");
        let (bytes, reference_hash) = key_material(0x90);
        std::fs::write(system_directory.join("key.bin"), bytes)
            .expect("system key should be written");

        let found = find_key_file(mount_directory.path(), &reference_hash)
            .await
            .expect("scan should succeed");

        assert_eq!(found, None);
    }
}
