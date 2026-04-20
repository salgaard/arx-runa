# Rclone Sidecar Binaries

Place platform-specific `rclone` sidecar binaries in this directory for Tauri bundling.

Expected filenames:

- `rclone-x86_64-pc-windows-msvc(.exe when built)`
- `rclone-x86_64-unknown-linux-gnu`
- `rclone-aarch64-unknown-linux-gnu`
- `rclone-x86_64-apple-darwin`
- `rclone-aarch64-apple-darwin`

Download from: <https://rclone.org/downloads/>

Verify checksums before use:

- SHA-256 (Windows x86_64): `<fill-me>`
- SHA-256 (Linux x86_64): `<fill-me>`
- SHA-256 (Linux aarch64): `<fill-me>`
- SHA-256 (macOS x86_64): `<fill-me>`
- SHA-256 (macOS aarch64): `<fill-me>`
