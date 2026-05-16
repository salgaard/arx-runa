# Distribution

## How releases work

Releases are built automatically by GitHub Actions when a version tag is pushed:

```powershell
git tag v0.1.0
git push origin v0.1.0
```

The workflow (`.github/workflows/release.yml`) builds on all three platforms in parallel and creates a **draft** GitHub Release. Review the artifacts on GitHub → Releases, then publish manually.

## Artifacts per platform

| Platform | Format | Notes |
|---|---|---|
| Windows | NSIS installer (`.exe`) | SmartScreen warning — click More info → Run anyway |
| macOS | Universal disk image (`.dmg`) | Gatekeeper blocks first launch — right-click → Open |
| Linux | `.AppImage` + `.deb` | AppImage needs `chmod +x` before running |

Releases are **unsigned** (no code signing certificates). This is normal for open-source software.

## rclone sidecar

rclone is bundled as a Tauri sidecar binary. The release workflow downloads the correct rclone build for each platform at build time and renames it to Tauri's triple-suffix convention (`rclone-{target_triple}`). The pinned version is set in the workflow — update `RCLONE_VERSION` when upgrading rclone.

## CI

`.github/workflows/continuous-integration.yml` runs on every push across all three platforms (Ubuntu, Windows, macOS). The e2e tests run Linux-only (they require `xvfb` + `webkit2gtk-driver`).
