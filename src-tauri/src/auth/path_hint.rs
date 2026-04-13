//! Local JSON storage for per-vault key-file path hints.

use std::collections::BTreeMap;
use std::fs;
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::auth::error::KeySourceError;

const SCHEMA_VERSION: u32 = 1;

/// Per-vault last-used key-file path.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VaultHint {
    /// Absolute path to the last-used key file on this machine.
    pub last_key_file_path: PathBuf,
}

/// Top-level key-hint file schema.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct HintFile {
    #[serde(default)]
    schema_version: u32,
    #[serde(default)]
    hints: BTreeMap<String, VaultHint>,
}

/// Reads and writes key-file path hints.
#[derive(Debug, Clone)]
pub struct KeyHintStore {
    file_path: PathBuf,
}

impl KeyHintStore {
    /// Builds a store at the platform default location.
    pub fn default_location() -> Option<Self> {
        let base_directory = dirs::data_local_dir()?;
        Some(Self {
            file_path: base_directory.join("arx-runa").join("key-hint.json"),
        })
    }

    /// Builds a store at an explicit file path.
    pub fn at_path(file_path: PathBuf) -> Self {
        Self { file_path }
    }

    /// Returns the configured hint file path.
    pub fn path(&self) -> &Path {
        &self.file_path
    }

    /// Returns the stored hint for `vault_id`.
    pub fn get(&self, vault_id: &str) -> Result<Option<VaultHint>, KeySourceError> {
        let contents = match fs::read_to_string(&self.file_path) {
            Ok(contents) => contents,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(KeySourceError::IoFailed(error)),
        };

        let parsed: HintFile = match serde_json::from_str(&contents) {
            Ok(parsed) => parsed,
            Err(_) => return Ok(None),
        };

        Ok(parsed.hints.get(vault_id).cloned())
    }

    /// Upserts a hint for `vault_id` while preserving hints for other vaults.
    pub fn set(&self, vault_id: &str, hint: VaultHint) -> Result<(), KeySourceError> {
        let mut parsed = match fs::read_to_string(&self.file_path) {
            Ok(contents) => serde_json::from_str::<HintFile>(&contents).unwrap_or_default(),
            Err(error) if error.kind() == ErrorKind::NotFound => HintFile::default(),
            Err(error) => return Err(KeySourceError::IoFailed(error)),
        };

        parsed.schema_version = SCHEMA_VERSION;
        parsed.hints.insert(vault_id.to_owned(), hint);

        if let Some(parent_directory) = self.file_path.parent() {
            fs::create_dir_all(parent_directory).map_err(KeySourceError::IoFailed)?;
        }

        let temporary_path = self.file_path.with_extension("json.tmp");
        let encoded = serde_json::to_vec_pretty(&parsed)
            .map_err(|error| KeySourceError::IoFailed(std::io::Error::other(error)))?;
        {
            let mut file = fs::File::create(&temporary_path).map_err(KeySourceError::IoFailed)?;
            file.write_all(&encoded).map_err(KeySourceError::IoFailed)?;
            file.sync_all().map_err(KeySourceError::IoFailed)?;
        }

        fs::rename(&temporary_path, &self.file_path).map_err(KeySourceError::IoFailed)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{KeyHintStore, VaultHint};

    #[test]
    fn test_key_hint_store_returns_none_when_file_missing() {
        let directory = tempfile::tempdir().expect("temp directory should be created");
        let store = KeyHintStore::at_path(directory.path().join("key-hint.json"));

        let hint = store
            .get("vault-a")
            .expect("missing file lookup should succeed");

        assert_eq!(hint, None);
    }

    #[test]
    fn test_key_hint_store_roundtrips_single_vault_hint() {
        let directory = tempfile::tempdir().expect("temp directory should be created");
        let store = KeyHintStore::at_path(directory.path().join("key-hint.json"));
        let expected = VaultHint {
            last_key_file_path: directory.path().join("usb").join("key.bin"),
        };

        store
            .set("vault-a", expected.clone())
            .expect("hint should be persisted");
        let actual = store
            .get("vault-a")
            .expect("stored hint should be readable")
            .expect("stored hint should exist");

        assert_eq!(actual, expected);
    }

    #[test]
    fn test_key_hint_store_preserves_other_vaults_on_write() {
        let directory = tempfile::tempdir().expect("temp directory should be created");
        let store = KeyHintStore::at_path(directory.path().join("key-hint.json"));
        let hint_a = VaultHint {
            last_key_file_path: directory.path().join("vault-a-key.bin"),
        };
        let hint_b = VaultHint {
            last_key_file_path: directory.path().join("vault-b-key.bin"),
        };

        store
            .set("vault-a", hint_a.clone())
            .expect("vault-a hint should be set");
        store
            .set("vault-b", hint_b.clone())
            .expect("vault-b hint should be set");

        assert_eq!(
            store.get("vault-a").expect("vault-a read should succeed"),
            Some(hint_a)
        );
        assert_eq!(
            store.get("vault-b").expect("vault-b read should succeed"),
            Some(hint_b)
        );
    }

    #[test]
    fn test_key_hint_store_returns_none_on_corrupt_file() {
        let directory = tempfile::tempdir().expect("temp directory should be created");
        let path = directory.path().join("key-hint.json");
        std::fs::write(&path, "not json").expect("corrupt file should be written");
        let store = KeyHintStore::at_path(path);

        let hint = store
            .get("vault-a")
            .expect("corrupt file read should not fail");

        assert_eq!(hint, None);
    }

    #[test]
    fn test_key_hint_store_atomic_write_uses_tempfile() {
        let directory = tempfile::tempdir().expect("temp directory should be created");
        let path = directory.path().join("key-hint.json");
        let store = KeyHintStore::at_path(path.clone());
        let hint = VaultHint {
            last_key_file_path: directory.path().join("usb").join("key.bin"),
        };

        store.set("vault-a", hint).expect("hint should be written");

        assert!(!path.with_extension("json.tmp").exists());
    }
}
