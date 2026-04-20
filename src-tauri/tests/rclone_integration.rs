use std::path::PathBuf;

use arx_runa_tauri_lib::storage::{
    BackupSyncMode, CloudEndpoint, CloudTransport, DestinationSessionPublic, DestinationType,
    RcloneTransport, SyncConfig,
};

fn resolve_rclone_binary() -> PathBuf {
    PathBuf::from("rclone")
}

#[tokio::test]
#[ignore]
async fn test_rclone_transport_round_trip_with_local_remote() {
    if std::env::var("ARX_RCLONE_INTEGRATION").ok().as_deref() != Some("1") {
        return;
    }

    let temp = tempfile::tempdir().expect("tempdir should be created");
    let remote_name = format!("arx-runa-test-{}", uuid::Uuid::new_v4());
    let config_path = temp.path().join("rclone.conf");
    let remote_root_dir = temp.path().join("remote-root");
    let bucket_dir = remote_root_dir.join("bucket");
    tokio::fs::create_dir_all(&bucket_dir)
        .await
        .expect("bucket directory should be created");

    let status = tokio::process::Command::new(resolve_rclone_binary())
        .arg("config")
        .arg("create")
        .arg(&remote_name)
        .arg("local")
        .arg(format!(
            "nounc={}",
            if cfg!(windows) { "true" } else { "false" }
        ))
        .arg(format!("root={}", remote_root_dir.display()))
        .arg("--non-interactive")
        .arg("--config")
        .arg(&config_path)
        .status()
        .await
        .expect("rclone config create should execute");
    assert!(status.success(), "rclone config create failed");

    let endpoint = CloudEndpoint {
        provider: "local".to_owned(),
        bucket: "bucket".to_owned(),
        region: String::new(),
        endpoint: String::new(),
        path_prefix: "vault".to_owned(),
    };
    let destination = DestinationSessionPublic {
        destination_id: uuid::Uuid::new_v4().hyphenated().to_string(),
        label: "integration".to_owned(),
        destination_type: DestinationType::Cloud,
        rclone_remote_name: remote_name,
        bucket: "bucket".to_owned(),
        path_prefix: "vault".to_owned(),
        is_primary: true,
        backup_mode: Some(BackupSyncMode::Mirror),
    };
    let transport = RcloneTransport::new(
        resolve_rclone_binary(),
        config_path.clone(),
        &endpoint,
        &destination,
        SyncConfig {
            max_concurrent: 1,
            operation_timeout_seconds: 60,
        },
    )
    .expect("transport should be constructed");

    let upload_source = temp.path().join("upload.bin");
    let download_target = temp.path().join("download.bin");
    tokio::fs::write(&upload_source, b"integration-bytes")
        .await
        .expect("upload source should be written");

    transport
        .upload_blob(&upload_source, "round-trip/test.blob")
        .await
        .expect("upload should succeed");
    let listed = transport
        .list_blobs("round-trip")
        .await
        .expect("list should succeed");
    assert!(!listed.is_empty(), "list should include uploaded object");

    transport
        .download_blob("round-trip/test.blob", &download_target)
        .await
        .expect("download should succeed");
    assert_eq!(
        tokio::fs::read(&download_target).await.unwrap(),
        b"integration-bytes"
    );

    transport
        .delete_blob("round-trip/test.blob")
        .await
        .expect("delete should succeed");
}
