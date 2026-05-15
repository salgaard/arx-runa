//! Scenario tests: backup pipeline (Use Case 1 — Zero-Knowledge Personal Backup).
//!
//! Tests the full encrypt → stage → upload → download → decrypt round trip,
//! and that EXIF metadata is stripped before chunks leave the client.

use uuid::Uuid;

use crate::auth::ceremonies::test_support::*;
use crate::crypto::{KeyEncryptionKey, WrappedFileKey, unwrap_file_key};
use crate::storage::pipeline::exif::strip_exif;
use crate::storage::{MetadataStore, SqlCipherMetadataStore, decrypt_file, upload_file};

// ---------------------------------------------------------------------------
// UC1: encrypt → stage → decrypt round trip
// ---------------------------------------------------------------------------

/// Full round trip: upload a small file, decrypt from the staged blobs, bytes match exactly.
#[tokio::test(flavor = "multi_thread")]
async fn test_backup_encrypt_decrypt_round_trip_bytes_identical() {
    let _lock = ceremony_lock().await;
    let vault = create_tier_one_vault().await;
    let derived = derive_vault_keys_tier_one(&vault);
    let store = SqlCipherMetadataStore::open(&vault.vault_db_path, &derived.sqlcipher_key)
        .await
        .expect("store must open with derived sqlcipher key");
    let kek = KeyEncryptionKey::from_bytes(derived.key_encryption_key);
    let staging_dir = temp_dir();

    let source_bytes: &[u8] = b"hello arx runa round trip test";
    let source_temp = temp_dir();
    let source_path = source_temp.path().join("input.bin");
    tokio::fs::write(&source_path, source_bytes)
        .await
        .expect("source file must be writable");

    let node_id = Uuid::new_v4();
    let node = upload_file(
        &source_path,
        node_id,
        None,
        "input.bin",
        1_700_000_000,
        1_700_000_000,
        &store,
        &kek,
        staging_dir.path(),
        None,
    )
    .await
    .expect("upload_file must succeed");

    let wrapped_bytes = node
        .file_key_wrapped
        .expect("uploaded file node must carry a wrapped file key");
    let file_key = unwrap_file_key(&WrappedFileKey::new(wrapped_bytes), &kek)
        .expect("file key must unwrap with the same KEK used at upload");

    let chunks = store
        .get_chunks(node_id)
        .await
        .expect("chunks must be queryable after upload");

    let dest_temp = temp_dir();
    let dest_path = dest_temp.path().join("output.bin");
    decrypt_file(
        &dest_path,
        node_id,
        &file_key,
        source_bytes.len() as u64,
        &chunks,
        staging_dir.path(),
        &store,
        None,
    )
    .await
    .expect("decrypt_file must succeed");

    let result = tokio::fs::read(&dest_path)
        .await
        .expect("decrypted output must be readable");
    assert_eq!(
        result, source_bytes,
        "decrypted bytes must be identical to original"
    );
}

// ---------------------------------------------------------------------------
// UC1: EXIF stripping before staging
// ---------------------------------------------------------------------------

/// JPEG with an APP1 segment has that segment stripped by strip_exif before any chunk leaves
/// the client.
#[test]
fn test_exif_stripped_from_jpeg_before_staging() {
    // Minimal JPEG: SOI + APP1 (marker FF E1, length 14, "Exif\0\0" + 6 bytes) + EOI.
    // The length field (0x000E = 14) includes its own 2 bytes + 12 bytes of payload.
    let jpeg_with_exif: Vec<u8> = vec![
        0xFF, 0xD8, // SOI
        0xFF, 0xE1, // APP1 marker
        0x00, 0x0E, // length = 14
        0x45, 0x78, 0x69, 0x66, 0x00, 0x00, // "Exif\0\0"
        0xDE, 0xAD, 0xBE, 0xEF, 0xCA, 0xFE, // fake EXIF payload
        0xFF, 0xD9, // EOI
    ];

    let stripped = strip_exif(jpeg_with_exif);

    assert!(
        !stripped.windows(2).any(|w| w == [0xFF, 0xE1]),
        "APP1 marker must not appear in stripped output"
    );
}
