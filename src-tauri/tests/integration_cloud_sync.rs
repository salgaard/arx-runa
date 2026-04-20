use arx_runa_tauri_lib::storage::SyncConfig;

#[tokio::test]
#[ignore = "Requires environment-coupled cloud harness (rclone sidecar + vault bootstrap secrets); covered by MockCloudTransport unit tests in CI."]
async fn test_integration_cloud_sync_round_trip_placeholder() {
    let _ = SyncConfig::default();
}
