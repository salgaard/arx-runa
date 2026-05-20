use arx_runa_tauri_lib::crypto::{KeyEncryptionKey, derive_vault_keys};
use arx_runa_tauri_lib::storage::{
    SqlCipherMetadataStore, StorageError, delete_file, download_file_to_memory, upload_file,
};
use tempfile::TempDir;
use uuid::Uuid;

async fn create_test_store(db_path: &std::path::Path) -> SqlCipherMetadataStore {
    SqlCipherMetadataStore::create(db_path, &[1u8; 32], Uuid::new_v4(), 4_194_304, false)
        .await
        .expect("test SqlCipherMetadataStore must be created")
}

fn test_kek() -> KeyEncryptionKey {
    derive_vault_keys(&[7u8; 32])
        .expect("test key derivation must succeed")
        .key_encryption_key
}

/// Uploads a file with a generated node ID, returns `(node_id, TempDir)`.
/// `TempDir` is returned to keep the staging directory alive.
async fn upload_test_file(
    content: &[u8],
    store: &SqlCipherMetadataStore,
    staging_dir: &std::path::Path,
) -> (Uuid, std::path::PathBuf) {
    let source_file = staging_dir.join("source.bin");
    tokio::fs::write(&source_file, content)
        .await
        .expect("test source file must be written");

    let node_id = Uuid::new_v4();
    upload_file(
        &source_file,
        node_id,
        None,
        "test.bin",
        0,
        0,
        store,
        &test_kek(),
        staging_dir,
        None,
    )
    .await
    .expect("upload_file must succeed");

    (node_id, source_file)
}

/// Returns the path of the first `.blob` file in `dir`, panicking if none found.
fn find_blob_file(dir: &std::path::Path) -> std::path::PathBuf {
    std::fs::read_dir(dir)
        .expect("staging dir must be readable")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .find(|p| p.extension().map_or(false, |ext| ext == "blob"))
        .expect("at least one .blob file must exist after upload")
}

#[tokio::test]
async fn test_upload_then_download_recovers_exact_bytes() {
    let tmp = TempDir::new().unwrap();
    let store = create_test_store(&tmp.path().join("manifest.db")).await;
    let staging = tmp.path().join("staging");
    tokio::fs::create_dir_all(&staging).await.unwrap();
    let content = b"integration round-trip content";

    let (node_id, _) = upload_test_file(content, &store, &staging).await;

    let recovered = download_file_to_memory(node_id, &store, &test_kek(), &staging, None)
        .await
        .expect("download_file_to_memory must succeed");

    assert_eq!(recovered.as_slice(), content);
}

#[tokio::test]
async fn test_upload_empty_file_succeeds() {
    let tmp = TempDir::new().unwrap();
    let store = create_test_store(&tmp.path().join("manifest.db")).await;
    let staging = tmp.path().join("staging");
    tokio::fs::create_dir_all(&staging).await.unwrap();

    let (node_id, _) = upload_test_file(b"", &store, &staging).await;

    let recovered = download_file_to_memory(node_id, &store, &test_kek(), &staging, None)
        .await
        .expect("download of empty file must succeed");

    assert!(recovered.is_empty());
}

#[tokio::test]
async fn test_delete_file_node_no_longer_retrievable() {
    let tmp = TempDir::new().unwrap();
    let store = create_test_store(&tmp.path().join("manifest.db")).await;
    let staging = tmp.path().join("staging");
    tokio::fs::create_dir_all(&staging).await.unwrap();
    let (node_id, _) = upload_test_file(b"to be deleted", &store, &staging).await;

    delete_file(node_id, &store, &staging)
        .await
        .expect("delete_file must succeed");

    let result = download_file_to_memory(node_id, &store, &test_kek(), &staging, None).await;
    assert!(
        matches!(result, Err(StorageError::NotFound)),
        "expected NotFound after delete, got {result:?}"
    );
}

#[tokio::test]
async fn test_concurrent_uploads_two_files_both_persisted() {
    let tmp = TempDir::new().unwrap();
    let store = create_test_store(&tmp.path().join("manifest.db")).await;
    let staging = tmp.path().join("staging");
    tokio::fs::create_dir_all(&staging).await.unwrap();

    let src_a = staging.join("a.bin");
    let src_b = staging.join("b.bin");
    tokio::fs::write(&src_a, b"file-a-content").await.unwrap();
    tokio::fs::write(&src_b, b"file-b-content").await.unwrap();

    let node_a = Uuid::new_v4();
    let node_b = Uuid::new_v4();
    let kek = test_kek();

    let (result_a, result_b) = tokio::join!(
        upload_file(
            &src_a, node_a, None, "a.bin", 0, 0, &store, &kek, &staging, None
        ),
        upload_file(
            &src_b, node_b, None, "b.bin", 0, 0, &store, &kek, &staging, None
        ),
    );
    result_a.expect("concurrent upload A must succeed");
    result_b.expect("concurrent upload B must succeed");

    let data_a = download_file_to_memory(node_a, &store, &kek, &staging, None)
        .await
        .expect("download A must succeed");
    let data_b = download_file_to_memory(node_b, &store, &kek, &staging, None)
        .await
        .expect("download B must succeed");

    assert_eq!(data_a.as_slice(), b"file-a-content");
    assert_eq!(data_b.as_slice(), b"file-b-content");
}

#[tokio::test]
async fn test_download_corrupted_blob_returns_storage_error() {
    let tmp = TempDir::new().unwrap();
    let store = create_test_store(&tmp.path().join("manifest.db")).await;
    let staging = tmp.path().join("staging");
    tokio::fs::create_dir_all(&staging).await.unwrap();
    let (node_id, _) = upload_test_file(b"original data", &store, &staging).await;

    let blob_path = find_blob_file(&staging);
    tokio::fs::write(&blob_path, b"garbage bytes that are not valid ciphertext")
        .await
        .expect("blob overwrite must succeed");

    let result = download_file_to_memory(node_id, &store, &test_kek(), &staging, None).await;
    assert!(
        result.is_err(),
        "download of corrupted blob must return an error"
    );
}

#[tokio::test]
async fn test_upload_multi_chunk_file_round_trip() {
    let tmp = TempDir::new().unwrap();
    let store = create_test_store(&tmp.path().join("manifest.db")).await;
    let staging = tmp.path().join("staging");
    tokio::fs::create_dir_all(&staging).await.unwrap();
    // 9 MiB — forces at least two 4 MiB chunks.
    let content: Vec<u8> = (0u8..=255).cycle().take(9 * 1024 * 1024).collect();

    let (node_id, _) = upload_test_file(&content, &store, &staging).await;

    let recovered = download_file_to_memory(node_id, &store, &test_kek(), &staging, None)
        .await
        .expect("multi-chunk download must succeed");

    assert_eq!(recovered.len(), content.len());
    assert_eq!(recovered.as_slice(), content.as_slice());
}
