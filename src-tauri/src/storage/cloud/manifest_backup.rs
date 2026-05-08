//! Manifest cloud-backup primitives and orchestration.
//!
//! This module owns the canonical manifest-backup cloud path, AEAD wire-format
//! primitives, and upload/download orchestration for SQLCipher manifest backup
//! snapshots.

use std::io::{ErrorKind, Write};
use std::path::Path;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use chacha20poly1305::{
    AeadInPlace, KeyInit, XChaCha20Poly1305, aead::generic_array::GenericArray,
};
use thiserror::Error;
use zeroize::Zeroizing;

use super::{CloudTransport, CloudTransportError};
use crate::crypto::SqlcipherKey;
use crate::crypto::error::CryptoError;
use crate::crypto::nonce::generate_nonce;
use crate::storage::schema::{validate_manifest_meta, validate_schema_integrity};
use crate::storage::sqlcipher::open_sqlcipher;
use crate::storage::staging::{ensure_staging_directory, write_owner_only};

const NONCE_LEN: usize = 24;
const TAG_LEN: usize = 16;

/// Canonical cloud blob path for the encrypted manifest backup.
pub const MANIFEST_BACKUP_BLOB_NAME: &str = "manifest/manifest-backup.blob";
/// Upload-side local staging filename for the encrypted backup wire payload.
pub(crate) const MANIFEST_BACKUP_UPLOAD_STAGING_FILE_NAME: &str = "manifest-backup-staging.blob";
/// Download-side local staging filename for the encrypted backup wire payload.
pub(crate) const MANIFEST_BACKUP_DOWNLOAD_STAGING_FILE_NAME: &str = "manifest-backup-download.blob";
/// Local export filename used by `sqlcipher_export` before AEAD encryption.
pub(crate) const MANIFEST_EXPORT_FILE_NAME: &str = "manifest-export.db";

/// Errors produced while syncing `manifest/manifest-backup.blob`.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ManifestBackupSyncError {
    /// Staging-directory or staging-file I/O failed.
    #[error("manifest-backup staging I/O failed: {0}")]
    StagingIo(String),
    /// SQLCipher plaintext export failed.
    #[error("manifest export failed: {0}")]
    Vacuum(String),
    /// Reading the exported SQLCipher snapshot failed.
    #[error("manifest export read failed: {0}")]
    ExportRead(String),
    /// AEAD encryption/decryption failed.
    #[error("manifest-backup cryptographic operation failed")]
    CryptoFailed,
    /// Cloud transport operation failed.
    #[error("manifest-backup cloud transport failed: {0}")]
    Transport(#[from] CloudTransportError),
    /// SQLCipher integrity verification failed after decrypt/persist.
    #[error("manifest-backup integrity check failed")]
    IntegrityCheckFailed,
    /// Persisting decrypted SQLCipher bytes to the destination path failed.
    #[error("manifest destination DB persist I/O failed: {0}")]
    DbPersistIo(String),
}

/// Encrypts caller-owned `plaintext` under `manifest_key` with XChaCha20-Poly1305 and no AAD.
///
/// Returns the wire-format blob `[nonce || ciphertext || tag]`.
pub(crate) fn encrypt_manifest_backup(
    mut plaintext: Zeroizing<Vec<u8>>,
    manifest_key: &[u8; 32],
) -> Result<Vec<u8>, CryptoError> {
    let nonce_bytes = generate_nonce();
    let cipher = XChaCha20Poly1305::new(GenericArray::from_slice(manifest_key));
    let nonce = GenericArray::from_slice(&nonce_bytes);
    let tag = cipher
        .encrypt_in_place_detached(nonce, &[], plaintext.as_mut_slice())
        .map_err(|_| CryptoError::EncryptionFailed)?;

    let mut wire = Vec::with_capacity(NONCE_LEN + plaintext.len() + TAG_LEN);
    wire.extend_from_slice(&nonce_bytes);
    wire.extend_from_slice(&plaintext);
    wire.extend_from_slice(tag.as_slice());
    Ok(wire)
}

/// Decrypts a wire-format manifest backup blob under `manifest_key`.
///
/// Returns plaintext bytes wrapped in [`Zeroizing`] for controlled lifetime.
pub(crate) fn decrypt_manifest_backup(
    wire: &[u8],
    manifest_key: &[u8; 32],
) -> Result<Zeroizing<Vec<u8>>, CryptoError> {
    if wire.len() < NONCE_LEN + TAG_LEN {
        return Err(CryptoError::DecryptionFailed);
    }
    let nonce_slice = &wire[..NONCE_LEN];
    let ciphertext_slice = &wire[NONCE_LEN..wire.len() - TAG_LEN];
    let tag_slice = &wire[wire.len() - TAG_LEN..];

    let cipher = XChaCha20Poly1305::new(GenericArray::from_slice(manifest_key));
    let nonce = GenericArray::from_slice(nonce_slice);
    let tag = GenericArray::from_slice(tag_slice);

    let mut plaintext: Zeroizing<Vec<u8>> = Zeroizing::new(ciphertext_slice.to_vec());
    cipher
        .decrypt_in_place_detached(nonce, &[], plaintext.as_mut_slice(), tag)
        .map_err(|_| CryptoError::DecryptionFailed)?;
    Ok(plaintext)
}

/// Exports, encrypts, and uploads a manifest backup to the canonical cloud path.
pub async fn upload_manifest_backup(
    vault_db_path: &Path,
    sqlcipher_key: &SqlcipherKey,
    manifest_key: &[u8; 32],
    cloud_transport: &dyn CloudTransport,
    staging_dir: &Path,
) -> Result<(), ManifestBackupSyncError> {
    ensure_staging_directory(staging_dir)
        .await
        .map_err(|error| ManifestBackupSyncError::StagingIo(error.to_string()))?;

    let export_path = staging_dir.join(MANIFEST_EXPORT_FILE_NAME);
    remove_file_if_present(&export_path)
        .await
        .map_err(|error| ManifestBackupSyncError::StagingIo(error.to_string()))?;

    let vault_db_path = vault_db_path.to_path_buf();
    let export_path_for_task = export_path.clone();
    let sqlcipher_key = sqlcipher_key_from_array(sqlcipher_key.with_exposed(|bytes| *bytes));
    tokio::task::spawn_blocking(move || {
        run_vacuum_into_export(&vault_db_path, &sqlcipher_key, &export_path_for_task)
    })
    .await
    .map_err(|error| ManifestBackupSyncError::Vacuum(error.to_string()))??;

    let plaintext = match tokio::fs::read(&export_path).await {
        Ok(bytes) => Zeroizing::new(bytes),
        Err(error) => {
            let _ = remove_file_if_present(&export_path).await;
            return Err(ManifestBackupSyncError::ExportRead(error.to_string()));
        }
    };
    remove_file_if_present(&export_path)
        .await
        .map_err(|error| ManifestBackupSyncError::StagingIo(error.to_string()))?;

    let wire = encrypt_manifest_backup(plaintext, manifest_key)
        .map_err(|_| ManifestBackupSyncError::CryptoFailed)?;

    let upload_staging_path = staging_dir.join(MANIFEST_BACKUP_UPLOAD_STAGING_FILE_NAME);
    write_owner_only(&upload_staging_path, &wire)
        .await
        .map_err(|error| ManifestBackupSyncError::StagingIo(error.to_string()))?;

    if let Err(error) = cloud_transport
        .upload_blob(&upload_staging_path, MANIFEST_BACKUP_BLOB_NAME)
        .await
    {
        let _ = remove_file_if_present(&upload_staging_path).await;
        return Err(ManifestBackupSyncError::Transport(error));
    }

    if let Err(cleanup_error) = remove_file_if_present(&upload_staging_path).await {
        tracing::warn!(
            ?cleanup_error,
            "manifest-backup upload staging cleanup failed after successful upload"
        );
    }

    Ok(())
}

/// Downloads, decrypts, persists, and integrity-checks a manifest backup.
pub async fn download_manifest_backup(
    cloud_transport: &dyn CloudTransport,
    staging_dir: &Path,
    manifest_key: &[u8; 32],
    destination_db_path: &Path,
    sqlcipher_key: &SqlcipherKey,
) -> Result<(), ManifestBackupSyncError> {
    ensure_staging_directory(staging_dir)
        .await
        .map_err(|error| ManifestBackupSyncError::StagingIo(error.to_string()))?;

    if tokio::fs::try_exists(destination_db_path)
        .await
        .map_err(|error| ManifestBackupSyncError::DbPersistIo(error.to_string()))?
    {
        return Err(ManifestBackupSyncError::DbPersistIo(
            "destination exists".to_owned(),
        ));
    }

    let download_staging_path = staging_dir.join(MANIFEST_BACKUP_DOWNLOAD_STAGING_FILE_NAME);
    remove_file_if_present(&download_staging_path)
        .await
        .map_err(|error| ManifestBackupSyncError::StagingIo(error.to_string()))?;

    if let Err(error) = cloud_transport
        .download_blob(MANIFEST_BACKUP_BLOB_NAME, &download_staging_path)
        .await
    {
        let _ = remove_file_if_present(&download_staging_path).await;
        return Err(ManifestBackupSyncError::Transport(error));
    }

    // Some rclone backends exit 0 without creating the local file when the
    // remote blob does not yet exist. Detect this and surface it as NotFound
    // so the caller handles an absent backup the same as a transport NotFound.
    match tokio::fs::try_exists(&download_staging_path).await {
        Ok(true) => {}
        Ok(false) => {
            return Err(ManifestBackupSyncError::Transport(
                CloudTransportError::NotFound,
            ));
        }
        Err(error) => {
            return Err(ManifestBackupSyncError::StagingIo(error.to_string()));
        }
    }

    let wire = match tokio::fs::read(&download_staging_path).await {
        Ok(bytes) => {
            if let Err(cleanup_error) = remove_file_if_present(&download_staging_path).await {
                tracing::warn!(
                    ?cleanup_error,
                    "manifest-backup download staging cleanup failed after read"
                );
            }
            bytes
        }
        Err(error) => {
            let _ = remove_file_if_present(&download_staging_path).await;
            return Err(ManifestBackupSyncError::StagingIo(error.to_string()));
        }
    };

    let plaintext = decrypt_manifest_backup(&wire, manifest_key)
        .map_err(|_| ManifestBackupSyncError::CryptoFailed)?;

    if let Some(parent) = destination_db_path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|error| ManifestBackupSyncError::DbPersistIo(error.to_string()))?;
    } else {
        return Err(ManifestBackupSyncError::DbPersistIo(
            "destination path has no parent".to_owned(),
        ));
    }

    let destination_db_path = destination_db_path.to_path_buf();
    let destination_for_persist = destination_db_path.clone();
    tokio::task::spawn_blocking(move || {
        persist_manifest_database_atomically(&destination_for_persist, plaintext)
    })
    .await
    .map_err(|error| ManifestBackupSyncError::DbPersistIo(error.to_string()))??;

    let destination_for_integrity = destination_db_path.clone();
    let key_bytes = sqlcipher_key.with_exposed(|bytes| *bytes);
    let integrity_join = tokio::task::spawn_blocking(move || {
        let verify_key = sqlcipher_key_from_array(key_bytes);
        verify_manifest_database_integrity(&destination_for_integrity, &verify_key)
    })
    .await;
    let integrity_result = match integrity_join {
        Ok(result) => result,
        Err(_) => {
            let _ = remove_file_if_present(&destination_db_path).await;
            return Err(ManifestBackupSyncError::IntegrityCheckFailed);
        }
    };
    if integrity_result.is_err() {
        let _ = remove_file_if_present(&destination_db_path).await;
        return Err(ManifestBackupSyncError::IntegrityCheckFailed);
    }

    Ok(())
}

/// Exports the vault manifest as a SQLCipher snapshot using `VACUUM INTO`.
///
/// The output is a SQLCipher-encrypted database in passphrase mode using the
/// same key as the source.  The caller AEAD-encrypts the resulting bytes
/// before upload so that the cloud receives only opaque ciphertext.
fn run_vacuum_into_export(
    vault_db_path: &Path,
    sqlcipher_key: &SqlcipherKey,
    export_path: &Path,
) -> Result<(), ManifestBackupSyncError> {
    let conn = open_sqlcipher(vault_db_path, sqlcipher_key)
        .map_err(|error| ManifestBackupSyncError::Vacuum(error.to_string()))?;
    let export_path_str = export_path
        .to_str()
        .ok_or_else(|| {
            ManifestBackupSyncError::Vacuum("export path is not valid UTF-8".to_owned())
        })?
        .replace('\'', "''");
    conn.execute_batch(&format!("VACUUM INTO '{export_path_str}'"))
        .map_err(|error| ManifestBackupSyncError::Vacuum(error.to_string()))?;
    drop(conn);

    #[cfg(unix)]
    {
        std::fs::set_permissions(export_path, std::fs::Permissions::from_mode(0o600))
            .map_err(|error| ManifestBackupSyncError::StagingIo(error.to_string()))?;
    }

    Ok(())
}

/// Persists decrypted SQLCipher bytes atomically to the destination path.
fn persist_manifest_database_atomically(
    destination_db_path: &Path,
    plaintext: Zeroizing<Vec<u8>>,
) -> Result<(), ManifestBackupSyncError> {
    let parent = destination_db_path.parent().ok_or_else(|| {
        ManifestBackupSyncError::DbPersistIo("destination path has no parent".to_owned())
    })?;
    let mut temporary_file = tempfile::NamedTempFile::new_in(parent)
        .map_err(|error| ManifestBackupSyncError::DbPersistIo(error.to_string()))?;
    temporary_file
        .write_all(plaintext.as_slice())
        .map_err(|error| ManifestBackupSyncError::DbPersistIo(error.to_string()))?;
    temporary_file
        .as_file()
        .sync_all()
        .map_err(|error| ManifestBackupSyncError::DbPersistIo(error.to_string()))?;
    #[cfg(unix)]
    {
        std::fs::set_permissions(
            temporary_file.path(),
            std::fs::Permissions::from_mode(0o600),
        )
        .map_err(|error| ManifestBackupSyncError::DbPersistIo(error.to_string()))?;
    }
    temporary_file
        .persist_noclobber(destination_db_path)
        .map_err(|error| ManifestBackupSyncError::DbPersistIo(error.error.to_string()))?;
    Ok(())
}

/// Verifies that the persisted manifest database satisfies canonical schema invariants.
///
/// The database is a SQLCipher file produced by `VACUUM INTO` (passphrase mode),
/// so it must be opened with the same key used during upload.
fn verify_manifest_database_integrity(
    destination_db_path: &Path,
    sqlcipher_key: &SqlcipherKey,
) -> Result<(), ManifestBackupSyncError> {
    let conn = open_sqlcipher(destination_db_path, sqlcipher_key)
        .map_err(|_| ManifestBackupSyncError::IntegrityCheckFailed)?;
    validate_schema_integrity(&conn).map_err(|_| ManifestBackupSyncError::IntegrityCheckFailed)?;
    validate_manifest_meta(&conn).map_err(|_| ManifestBackupSyncError::IntegrityCheckFailed)?;
    Ok(())
}

fn sqlcipher_key_from_array(bytes: [u8; 32]) -> SqlcipherKey {
    let mut boxed = Box::new([0u8; 32]);
    boxed.copy_from_slice(&bytes);
    SqlcipherKey::from_secret_box(secrecy::SecretBox::new(boxed))
}

/// Removes a file if present, tolerating missing-file races.
async fn remove_file_if_present(path: &Path) -> Result<(), std::io::Error> {
    match tokio::fs::remove_file(path).await {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use secrecy::SecretBox;
    use tempfile::tempdir;
    use uuid::Uuid;

    use super::*;
    use crate::storage::SqlCipherMetadataStore;
    use crate::storage::cloud::mock::{CloudTransportErrorKind, MockCloudTransport};
    use crate::storage::sqlcipher::open_sqlcipher;

    /// Creates a `SqlcipherKey` from fixed test bytes.
    fn sqlcipher_key_from_bytes(bytes: [u8; 32]) -> SqlcipherKey {
        let mut boxed = Box::new([0u8; 32]);
        boxed.copy_from_slice(&bytes);
        SqlcipherKey::from_secret_box(SecretBox::new(boxed))
    }

    /// Seeds a SQLCipher manifest database on disk for upload tests.
    async fn create_manifest_database(path: &Path, sqlcipher_key: &[u8; 32]) {
        let _store =
            SqlCipherMetadataStore::create(path, sqlcipher_key, Uuid::new_v4(), 4_194_304, false)
                .await
                .expect("manifest database must be created");
    }

    /// Uploads caller-provided bytes to a remote path using `MockCloudTransport`.
    async fn upload_raw_remote_blob(
        cloud_transport: &MockCloudTransport,
        temp_root: &Path,
        remote_path: &str,
        bytes: &[u8],
    ) {
        let source_path = temp_root.join("raw-remote-seed.blob");
        tokio::fs::write(&source_path, bytes)
            .await
            .expect("raw seed write must succeed");
        cloud_transport
            .upload_blob(&source_path, remote_path)
            .await
            .expect("raw seed upload must succeed");
    }

    #[tokio::test]
    async fn test_manifest_backup_round_trip_returns_plaintext() {
        let manifest_key = [0x11u8; 32];
        let plaintext = b"CREATE TABLE foo (id INTEGER);";

        let wire = encrypt_manifest_backup(Zeroizing::new(plaintext.to_vec()), &manifest_key)
            .expect("encrypt must succeed");
        let recovered =
            decrypt_manifest_backup(&wire, &manifest_key).expect("decrypt must succeed");

        assert_eq!(recovered.as_slice(), plaintext);
    }

    #[tokio::test]
    async fn test_manifest_backup_wrong_key_returns_decryption_failed() {
        let wire = encrypt_manifest_backup(Zeroizing::new(b"payload".to_vec()), &[0x11u8; 32])
            .expect("encrypt must succeed");

        let result = decrypt_manifest_backup(&wire, &[0x22u8; 32]);

        assert!(matches!(result, Err(CryptoError::DecryptionFailed)));
    }

    #[tokio::test]
    async fn test_manifest_backup_truncated_wire_returns_decryption_failed() {
        let result = decrypt_manifest_backup(&[0u8; 10], &[0x11u8; 32]);

        assert!(matches!(result, Err(CryptoError::DecryptionFailed)));
    }

    #[tokio::test]
    async fn test_manifest_backup_corrupted_tag_returns_decryption_failed() {
        let manifest_key = [0x11u8; 32];
        let mut wire = encrypt_manifest_backup(Zeroizing::new(b"payload".to_vec()), &manifest_key)
            .expect("encrypt must succeed");
        let tag_index = wire.len() - 1;
        wire[tag_index] ^= 0x01;

        let result = decrypt_manifest_backup(&wire, &manifest_key);

        assert!(matches!(result, Err(CryptoError::DecryptionFailed)));
    }

    #[tokio::test]
    async fn test_upload_manifest_backup_writes_canonical_remote_path_and_cleans_staging() {
        let temp = tempdir().expect("tempdir must succeed");
        let source_db_path = temp.path().join("source.db");
        let staging_dir = temp.path().join("staging");
        let sqlcipher_key_bytes = [0x41u8; 32];
        let sqlcipher_key = sqlcipher_key_from_bytes(sqlcipher_key_bytes);
        let manifest_key = [0x51u8; 32];
        let cloud_transport = MockCloudTransport::new();

        create_manifest_database(&source_db_path, &sqlcipher_key_bytes).await;
        upload_manifest_backup(
            &source_db_path,
            &sqlcipher_key,
            &manifest_key,
            &cloud_transport,
            &staging_dir,
        )
        .await
        .expect("upload must succeed");

        let downloaded_path = temp.path().join("downloaded-wire.blob");
        cloud_transport
            .download_blob(MANIFEST_BACKUP_BLOB_NAME, &downloaded_path)
            .await
            .expect("canonical remote blob must exist");
        let downloaded_wire = tokio::fs::read(&downloaded_path)
            .await
            .expect("downloaded wire must be readable");
        assert!(downloaded_wire.len() > NONCE_LEN + TAG_LEN);
        assert!(!staging_dir.join(MANIFEST_EXPORT_FILE_NAME).exists());
        assert!(
            !staging_dir
                .join(MANIFEST_BACKUP_UPLOAD_STAGING_FILE_NAME)
                .exists()
        );
    }

    #[tokio::test]
    async fn test_upload_manifest_backup_transport_failure_removes_staging_file() {
        let temp = tempdir().expect("tempdir must succeed");
        let source_db_path = temp.path().join("source.db");
        let staging_dir = temp.path().join("staging");
        let sqlcipher_key_bytes = [0x42u8; 32];
        let sqlcipher_key = sqlcipher_key_from_bytes(sqlcipher_key_bytes);
        let manifest_key = [0x52u8; 32];
        let cloud_transport = MockCloudTransport::new();

        create_manifest_database(&source_db_path, &sqlcipher_key_bytes).await;
        cloud_transport
            .inject_failure(MANIFEST_BACKUP_BLOB_NAME, CloudTransportErrorKind::Timeout)
            .await;

        let result = upload_manifest_backup(
            &source_db_path,
            &sqlcipher_key,
            &manifest_key,
            &cloud_transport,
            &staging_dir,
        )
        .await;

        assert!(matches!(
            result,
            Err(ManifestBackupSyncError::Transport(
                CloudTransportError::Timeout
            ))
        ));
        assert!(
            !staging_dir
                .join(MANIFEST_BACKUP_UPLOAD_STAGING_FILE_NAME)
                .exists()
        );
    }

    #[tokio::test]
    async fn test_upload_manifest_backup_encrypt_upload_download_decrypt_round_trip() {
        let temp = tempdir().expect("tempdir must succeed");
        let source_db_path = temp.path().join("source.db");
        let recovered_db_path = temp.path().join("recovered.db");
        let upload_staging_dir = temp.path().join("upload-staging");
        let download_staging_dir = temp.path().join("download-staging");
        let sqlcipher_key_bytes = [0x43u8; 32];
        let sqlcipher_key = sqlcipher_key_from_bytes(sqlcipher_key_bytes);
        let manifest_key = [0x53u8; 32];
        let cloud_transport = MockCloudTransport::new();

        create_manifest_database(&source_db_path, &sqlcipher_key_bytes).await;
        upload_manifest_backup(
            &source_db_path,
            &sqlcipher_key,
            &manifest_key,
            &cloud_transport,
            &upload_staging_dir,
        )
        .await
        .expect("upload must succeed");

        download_manifest_backup(
            &cloud_transport,
            &download_staging_dir,
            &manifest_key,
            &recovered_db_path,
            &sqlcipher_key,
        )
        .await
        .expect("download must succeed");

        let connection = open_sqlcipher(&recovered_db_path, &sqlcipher_key)
            .expect("recovered database must open");
        let table_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE name = 'manifest_meta'",
                [],
                |row| row.get(0),
            )
            .expect("metadata table query must succeed");
        assert_eq!(table_count, 1);
    }

    #[tokio::test]
    async fn test_download_manifest_backup_missing_manifest_meta_returns_integrity_check_failed() {
        let temp = tempdir().expect("tempdir must succeed");
        let source_db_path = temp.path().join("source.db");
        let destination_db_path = temp.path().join("recovered.db");
        let upload_staging_dir = temp.path().join("upload-staging");
        let download_staging_dir = temp.path().join("download-staging");
        let sqlcipher_key_bytes = [0x58u8; 32];
        let sqlcipher_key = sqlcipher_key_from_bytes(sqlcipher_key_bytes);
        let manifest_key = [0x68u8; 32];
        let cloud_transport = MockCloudTransport::new();

        let store = SqlCipherMetadataStore::create(
            &source_db_path,
            &sqlcipher_key_bytes,
            Uuid::new_v4(),
            4_194_304,
            false,
        )
        .await
        .expect("manifest database must be created");
        store
            .drop_manifest_meta_table_for_tests()
            .await
            .expect("manifest_meta drop must succeed");
        drop(store);

        upload_manifest_backup(
            &source_db_path,
            &sqlcipher_key,
            &manifest_key,
            &cloud_transport,
            &upload_staging_dir,
        )
        .await
        .expect("upload must succeed");

        let result = download_manifest_backup(
            &cloud_transport,
            &download_staging_dir,
            &manifest_key,
            &destination_db_path,
            &sqlcipher_key,
        )
        .await;

        assert!(matches!(
            result,
            Err(ManifestBackupSyncError::IntegrityCheckFailed)
        ));
        assert!(!destination_db_path.exists());
    }

    #[tokio::test]
    async fn test_download_manifest_backup_wrong_key_returns_integrity_check_failed_and_removes_destination()
     {
        let temp = tempdir().expect("tempdir must succeed");
        let source_db_path = temp.path().join("source.db");
        let destination_db_path = temp.path().join("recovered.db");
        let upload_staging_dir = temp.path().join("upload-staging");
        let download_staging_dir = temp.path().join("download-staging");
        let upload_key_bytes = [0xAAu8; 32];
        let upload_sqlcipher_key = sqlcipher_key_from_bytes(upload_key_bytes);
        let wrong_sqlcipher_key = sqlcipher_key_from_bytes([0xBBu8; 32]);
        let manifest_key = [0xCCu8; 32];
        let cloud_transport = MockCloudTransport::new();

        create_manifest_database(&source_db_path, &upload_key_bytes).await;
        upload_manifest_backup(
            &source_db_path,
            &upload_sqlcipher_key,
            &manifest_key,
            &cloud_transport,
            &upload_staging_dir,
        )
        .await
        .expect("upload must succeed");

        let result = download_manifest_backup(
            &cloud_transport,
            &download_staging_dir,
            &manifest_key,
            &destination_db_path,
            &wrong_sqlcipher_key,
        )
        .await;

        assert!(matches!(
            result,
            Err(ManifestBackupSyncError::IntegrityCheckFailed)
        ));
        assert!(!destination_db_path.exists());
    }

    #[tokio::test]
    async fn test_download_manifest_backup_truncated_wire_returns_crypto_failed() {
        let temp = tempdir().expect("tempdir must succeed");
        let destination_db_path = temp.path().join("recovered.db");
        let download_staging_dir = temp.path().join("download-staging");
        let manifest_key = [0x55u8; 32];
        let cloud_transport = MockCloudTransport::new();
        let sqlcipher_key = sqlcipher_key_from_bytes([0u8; 32]);

        upload_raw_remote_blob(
            &cloud_transport,
            temp.path(),
            MANIFEST_BACKUP_BLOB_NAME,
            &[0u8; 10],
        )
        .await;

        let result = download_manifest_backup(
            &cloud_transport,
            &download_staging_dir,
            &manifest_key,
            &destination_db_path,
            &sqlcipher_key,
        )
        .await;

        assert!(matches!(result, Err(ManifestBackupSyncError::CryptoFailed)));
        assert!(!destination_db_path.exists());
    }

    #[tokio::test]
    async fn test_download_manifest_backup_corrupted_ciphertext_returns_crypto_failed_and_cleans_outputs()
     {
        let temp = tempdir().expect("tempdir must succeed");
        let source_db_path = temp.path().join("source.db");
        let destination_db_path = temp.path().join("recovered.db");
        let upload_staging_dir = temp.path().join("upload-staging");
        let tamper_dir = temp.path().join("tamper");
        let download_staging_dir = temp.path().join("download-staging");
        let sqlcipher_key_bytes = [0x5Au8; 32];
        let sqlcipher_key = sqlcipher_key_from_bytes(sqlcipher_key_bytes);
        let manifest_key = [0x6Au8; 32];
        let cloud_transport = MockCloudTransport::new();

        create_manifest_database(&source_db_path, &sqlcipher_key_bytes).await;
        upload_manifest_backup(
            &source_db_path,
            &sqlcipher_key,
            &manifest_key,
            &cloud_transport,
            &upload_staging_dir,
        )
        .await
        .expect("upload must succeed");
        ensure_staging_directory(&tamper_dir)
            .await
            .expect("tamper directory must be created");
        let tamper_path = tamper_dir.join("wire.blob");
        cloud_transport
            .download_blob(MANIFEST_BACKUP_BLOB_NAME, &tamper_path)
            .await
            .expect("uploaded wire must be downloadable");
        let mut wire = tokio::fs::read(&tamper_path)
            .await
            .expect("uploaded wire must be readable");
        let tag_index = wire.len() - 1;
        wire[tag_index] ^= 0x01;
        tokio::fs::write(&tamper_path, &wire)
            .await
            .expect("tampered wire must be writable");
        cloud_transport
            .upload_blob(&tamper_path, MANIFEST_BACKUP_BLOB_NAME)
            .await
            .expect("tampered wire upload must succeed");

        let result = download_manifest_backup(
            &cloud_transport,
            &download_staging_dir,
            &manifest_key,
            &destination_db_path,
            &sqlcipher_key,
        )
        .await;

        assert!(matches!(result, Err(ManifestBackupSyncError::CryptoFailed)));
        assert!(!destination_db_path.exists());
        assert!(
            !download_staging_dir
                .join(MANIFEST_BACKUP_DOWNLOAD_STAGING_FILE_NAME)
                .exists()
        );
    }

    #[tokio::test]
    async fn test_download_manifest_backup_destination_exists_returns_db_persist_io_without_touching_cloud()
     {
        let temp = tempdir().expect("tempdir must succeed");
        let destination_db_path = temp.path().join("already-present.db");
        let download_staging_dir = temp.path().join("download-staging");
        let manifest_key = [0x56u8; 32];
        let cloud_transport = MockCloudTransport::new();
        let sqlcipher_key = sqlcipher_key_from_bytes([0u8; 32]);
        tokio::fs::write(&destination_db_path, b"existing")
            .await
            .expect("destination seed file must be written");

        let result = download_manifest_backup(
            &cloud_transport,
            &download_staging_dir,
            &manifest_key,
            &destination_db_path,
            &sqlcipher_key,
        )
        .await;

        assert!(matches!(
            result,
            Err(ManifestBackupSyncError::DbPersistIo(message)) if message == "destination exists"
        ));
        let destination_contents = tokio::fs::read(&destination_db_path)
            .await
            .expect("destination file must remain present");
        assert_eq!(destination_contents, b"existing");
    }

    #[tokio::test]
    async fn test_download_manifest_backup_transport_not_found_returns_transport_error_and_no_partial_files()
     {
        let temp = tempdir().expect("tempdir must succeed");
        let destination_db_path = temp.path().join("recovered.db");
        let download_staging_dir = temp.path().join("download-staging");
        let manifest_key = [0x57u8; 32];
        let cloud_transport = MockCloudTransport::new();
        let sqlcipher_key = sqlcipher_key_from_bytes([0u8; 32]);

        let result = download_manifest_backup(
            &cloud_transport,
            &download_staging_dir,
            &manifest_key,
            &destination_db_path,
            &sqlcipher_key,
        )
        .await;

        assert!(matches!(
            result,
            Err(ManifestBackupSyncError::Transport(
                CloudTransportError::NotFound
            ))
        ));
        assert!(!destination_db_path.exists());
        assert!(
            !download_staging_dir
                .join(MANIFEST_BACKUP_DOWNLOAD_STAGING_FILE_NAME)
                .exists()
        );
    }

    #[tokio::test]
    async fn test_upload_manifest_backup_missing_source_db_returns_vacuum() {
        let temp = tempdir().expect("tempdir must succeed");
        let missing_db_path = temp.path().join("missing-dir").join("missing.db");
        let staging_dir = temp.path().join("staging");
        let sqlcipher_key = sqlcipher_key_from_bytes([0x48u8; 32]);
        let manifest_key = [0x58u8; 32];
        let cloud_transport = MockCloudTransport::new();

        let result = upload_manifest_backup(
            &missing_db_path,
            &sqlcipher_key,
            &manifest_key,
            &cloud_transport,
            &staging_dir,
        )
        .await;

        assert!(matches!(result, Err(ManifestBackupSyncError::Vacuum(_))));
    }

    #[tokio::test]
    async fn test_upload_manifest_backup_invalid_staging_directory_returns_staging_io() {
        let temp = tempdir().expect("tempdir must succeed");
        let source_db_path = temp.path().join("source.db");
        let staging_path_that_is_file = temp.path().join("not-a-directory");
        let sqlcipher_key_bytes = [0x49u8; 32];
        let sqlcipher_key = sqlcipher_key_from_bytes(sqlcipher_key_bytes);
        let manifest_key = [0x59u8; 32];
        let cloud_transport = MockCloudTransport::new();

        create_manifest_database(&source_db_path, &sqlcipher_key_bytes).await;
        tokio::fs::write(&staging_path_that_is_file, b"seed")
            .await
            .expect("seed file must be created");

        let result = upload_manifest_backup(
            &source_db_path,
            &sqlcipher_key,
            &manifest_key,
            &cloud_transport,
            &staging_path_that_is_file,
        )
        .await;

        assert!(matches!(result, Err(ManifestBackupSyncError::StagingIo(_))));
    }

    #[test]
    fn test_manifest_backup_sync_error_export_read_variant_formats_expected_message() {
        let error = ManifestBackupSyncError::ExportRead("io error".to_owned());
        assert_eq!(error.to_string(), "manifest export read failed: io error");
    }

    /// A [`CloudTransport`] that returns `Ok(())` for every call without
    /// touching the filesystem. Simulates rclone backends that exit 0 without
    /// creating the local destination file when the remote blob is absent.
    struct SilentOkCloudTransport;

    #[async_trait::async_trait]
    impl CloudTransport for SilentOkCloudTransport {
        async fn upload_blob(
            &self,
            _local_path: &std::path::Path,
            _remote_path: &str,
        ) -> Result<(), CloudTransportError> {
            Ok(())
        }

        async fn download_blob(
            &self,
            _remote_path: &str,
            _local_path: &std::path::Path,
        ) -> Result<(), CloudTransportError> {
            Ok(())
        }

        async fn delete_blob(&self, _remote_path: &str) -> Result<(), CloudTransportError> {
            Ok(())
        }

        async fn list_blobs(
            &self,
            _remote_prefix: &str,
        ) -> Result<Vec<String>, CloudTransportError> {
            Ok(Vec::new())
        }
    }

    #[tokio::test]
    async fn test_download_manifest_backup_download_ok_but_no_file_returns_not_found() {
        let temp = tempdir().expect("tempdir must succeed");
        let destination_db_path = temp.path().join("recovered.db");
        let download_staging_dir = temp.path().join("download-staging");
        let manifest_key = [0x57u8; 32];
        let transport = SilentOkCloudTransport;
        let sqlcipher_key = sqlcipher_key_from_bytes([0u8; 32]);

        let result = download_manifest_backup(
            &transport,
            &download_staging_dir,
            &manifest_key,
            &destination_db_path,
            &sqlcipher_key,
        )
        .await;

        assert!(matches!(
            result,
            Err(ManifestBackupSyncError::Transport(
                CloudTransportError::NotFound
            ))
        ));
        assert!(!destination_db_path.exists());
        assert!(
            !download_staging_dir
                .join(MANIFEST_BACKUP_DOWNLOAD_STAGING_FILE_NAME)
                .exists()
        );
    }
}
