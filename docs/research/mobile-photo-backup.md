# Arx Runa Mobile: Encrypted Photo Backup

> **Document type**: Exploration / feasibility research
> **Status**: Living document
> **Last updated**: 2026-04-06

This document evaluates the feasibility of an Arx Runa mobile application for iOS and Android that automatically encrypts photos from the device camera roll and uploads them to a user-configured Rclone cloud backend.

---

## Table of Contents

1. [Motivation](#motivation)
2. [Platform Feasibility Overview](#platform-feasibility-overview)
3. [Tauri 2.0 Mobile Support](#tauri-20-mobile-support)
4. [Rclone on Mobile](#rclone-on-mobile)
5. [iCloud as a Backend](#icloud-as-a-backend)
6. [Photo Library Access](#photo-library-access)
7. [Background Upload Constraints](#background-upload-constraints)
8. [Key Management on Mobile](#key-management-on-mobile)
9. [Encryption Core Portability](#encryption-core-portability)
10. [Recommended Architecture Per Platform](#recommended-architecture-per-platform)
11. [Recommendation](#recommendation)
12. [Decisions](#decisions)
13. [Open Questions](#open-questions)
14. [Sources](#sources)

---

## Motivation

Cloud photo services (Google Photos, iCloud, Amazon Photos) have introduced AI-based content scanning and indexing of user images. Privacy-conscious users have no mainstream encrypted alternative that:

- Works automatically in the background (like Google Photos)
- Keeps photos encrypted before they leave the device
- Is cloud-agnostic (no vendor lock-in)

Arx Runa's existing crypto core and BYOC model map directly onto this use case. The open question is how much new platform work is required to make it viable on iOS and Android.

---

## Platform Feasibility Overview

| Capability | Android | iOS |
|---|---|---|
| Tauri 2.0 support | Yes | Yes |
| Rclone as native library | Yes (Go, NDK) | Hard (App Store restrictions) |
| Background auto-upload | Yes (WorkManager / foreground service) | Restricted (URLSession background transfers) |
| Photo library access | MediaStore API | PhotoKit API |
| Secure key storage | Android Keystore | Secure Enclave |
| USB key file (Tier 2) | Possible via OTG | Not practical |
| iCloud as upload target | No | No (no public API) |

**Summary**: Android is a straightforward target. iOS is possible but requires a different upload strategy and faces meaningful App Store restrictions.

---

## Tauri 2.0 Mobile Support

Tauri 2.0 (released 2024) added first-class iOS and Android targets. The Rust backend compiles to the same native code on mobile, meaning:

- The existing `crypto` module (XChaCha20-Poly1305, Argon2id, HKDF) requires no changes
- Tauri commands (`#[tauri::command]`) work identically
- The Leptos frontend compiles to a WebView running inside the native app shell

**Key implication**: The encryption core is already cross-platform. Mobile work is primarily platform integration (photo access, background tasks, key storage), not crypto.

**Plugins available**:

| Plugin | Purpose |
|---|---|
| `tauri-plugin-fs` | File system access |
| `tauri-plugin-biometric` | Biometric unlock (Face ID, fingerprint) |
| `tauri-plugin-notification` | Background sync status |
| `tauri-plugin-background-service` | Long-running background tasks (Android) |

---

## Rclone on Mobile

Rclone is written in Go and can be cross-compiled for mobile targets.

### Android

- Go compiles to Android via the NDK
- Rclone can be embedded as a `.so` native library using `gomobile bind`
- Runs in-process — no subprocess spawning required
- Tested by third-party apps (e.g., the Rclone Android project)

**Verdict**: Viable. Adds ~30–50 MB to the APK (Go runtime + rclone backends).

### iOS

- Go compiles to iOS via `gomobile bind` — produces an `.xcframework`
- **App Store constraint**: Apple prohibits downloading and executing code at runtime. Statically-linked Go code compiled into the app bundle is permitted.
- The `.xcframework` must be included at build time, not fetched dynamically
- App Store review may flag the use of `os/exec` or subprocess spawning within the Go code — rclone's pure-library mode avoids this

**Verdict**: Technically possible, but requires careful audit of rclone's iOS build to ensure no dynamic execution patterns. App Store approval carries risk.

### Alternative for iOS: Native Cloud Protocol Implementation

If rclone proves problematic on iOS, Arx Runa can speak cloud protocols directly for the most common backends:

| Backend | Protocol | Rust crate |
|---|---|---|
| S3-compatible | S3 REST API | `aws-sdk-s3` or `rusoto` |
| WebDAV | WebDAV | `reqwest` + manual |
| Google Drive | REST API | `google-drive3` |
| OneDrive | Microsoft Graph API | `reqwest` + manual |

This covers the majority of user-configured backends without requiring rclone at all. Less flexible than rclone's 70+ backends, but practical for an initial iOS release.

---

## iCloud as a Backend

iCloud Drive does **not** expose a public API. Apple does not provide:

- A WebDAV endpoint for iCloud Drive
- An S3-compatible interface
- An official REST API for third-party apps

Apple's iCloud protocols are proprietary and undocumented. The `icloud-drive-docker` open-source project reverse-engineered partial access, but this is unsupported, fragile, and violates Apple's Terms of Service.

**Verdict**: iCloud cannot be an Arx Runa backend via Rclone or any other reliable method. This is a fundamental Apple platform restriction, not an Arx Runa limitation.

Users on Apple devices who want photo backup should configure one of: S3-compatible storage, Google Drive, OneDrive, WebDAV, or SFTP.

---

## Photo Library Access

### Android

- **API**: `MediaStore` content provider
- Access via `READ_MEDIA_IMAGES` permission (Android 13+) or `READ_EXTERNAL_STORAGE` (Android 12 and below)
- Can observe new photos via `ContentObserver` for automatic detection
- Tauri exposes this via `tauri-plugin-fs` or direct Rust JNI calls

### iOS

- **API**: PhotoKit (`PHPhotoLibrary`)
- Requires `NSPhotoLibraryUsageDescription` in `Info.plist`
- Change observation via `PHPhotoLibraryChangeObserver` — app must be in foreground or have background app refresh enabled
- `PHAsset` provides access to original image data without writing to temp files (important for zero-trace compliance)

**Zero-trace consideration**: On both platforms, photo data must be read directly from the OS photo library API into memory, encrypted in-place, and uploaded — without writing the plaintext image to any intermediate file. This aligns with Arx Runa's existing zero-trace constraint.

---

## Background Upload Constraints

This is the most significant platform difference.

### Android

- **WorkManager**: Suitable for deferrable background work with constraints (Wi-Fi only, charging). Survives app restarts.
- **Foreground Service**: For active uploads requiring persistent notification. More reliable, less battery-friendly.
- **Battery optimization**: Users may need to exempt Arx Runa from Doze mode for reliable automatic backup
- **Verdict**: True automatic background upload is achievable

### iOS

- **Background App Refresh**: Limited CPU time, non-deterministic scheduling, disabled by system under battery pressure
- **URLSession Background Transfers**: The correct API for background uploads. Uploads continue even if app is suspended or terminated. However:
  - Transfers are managed by iOS, not the app
  - Cannot run arbitrary Rust/Go code during the transfer — iOS handles the HTTP request
  - This means rclone cannot manage the transfer; Arx Runa must upload directly to the cloud provider's HTTP API
  - Completion handler called when upload finishes, even if app was not running
- **Significant Location / BGTaskScheduler**: Can wake the app periodically (e.g., to detect new photos and enqueue uploads), but wakeups are infrequent and unguaranteed

**Implication for iOS architecture**: Rclone cannot be used for the actual upload leg on iOS if true background behavior is needed. The upload must go through `URLSession` background transfers, meaning Arx Runa must speak cloud APIs directly (S3, WebDAV, etc.) rather than delegating to rclone.

**Practical iOS flow**:
1. App foreground / BGTaskScheduler wakeup → detect new photos via PhotoKit
2. Encrypt photos in memory → write encrypted chunks to app container (temporary, not cloud plaintext)
3. Hand encrypted chunks to `URLSession` background transfer for upload
4. `URLSession` handles upload while app is suspended
5. On completion callback → delete temporary encrypted chunks from local container

Note: Step 2 writes **encrypted** data to the local container, not plaintext. This is not a zero-trace violation since the data is already encrypted and only stored temporarily pending upload.

---

## Key Management on Mobile

### Tier 1 (Password only)

No changes required. Argon2id runs on mobile without modification. Memory-hardness parameters may need tuning for mobile hardware (lower `m_cost` or `t_cost` to keep unlock time under ~2 seconds on older devices).

### Tier 2 (Password + USB key file)

USB key file support is impractical on mobile:
- iOS has no standard USB file access
- Android supports USB OTG but this is an uncommon workflow for a photo backup app

**Recommended replacement factors for mobile**:

| Factor | iOS | Android | Notes |
|---|---|---|---|
| Biometric (Face ID / fingerprint) | Secure Enclave | Android Keystore | Device-bound key, unlocked by biometric |
| Device-bound key (no biometric prompt) | Keychain with `kSecAttrAccessibleWhenUnlocked` | Keystore | Weaker — unlocks with device unlock |

**Biometric Tier 2 flow**:
```
Setup:
  device_key = CSPRNG(32 bytes)
  store device_key in Secure Enclave / Android Keystore (biometric-protected)
  master_key = Argon2id(password || device_key, vault_salt)

Unlock:
  device_key = BiometricPrompt → Secure Enclave / Keystore release
  master_key = Argon2id(password || device_key, vault_salt)
```

This is consistent with the Progressive Security Model described in `market-and-future-directions.md`.

### Recovery

Loss of the mobile device without recovery means vault access is lost (by design — that is the security property). Users should be informed at vault creation to maintain a recovery method:
- Desktop vault (same cloud backend, password-only tier)
- Shamir secret sharing recovery (future feature, see market-and-future-directions.md)

---

## Encryption Core Portability

The existing Rust crypto core compiles to mobile without modification:

| Component | Mobile status |
|---|---|
| `XChaCha20Poly1305` (`chacha20poly1305` crate) | Compiles to ARM64 / x86_64 Android and iOS |
| `Argon2id` (`argon2` crate) | Compiles; tune memory parameters for mobile |
| `HKDF-SHA256` (`hkdf` crate) | Compiles unchanged |
| `zeroize` / `secrecy` | Compiles unchanged |
| `mlock` / `VirtualLock` | Not available on mobile — use platform secure memory APIs |

**`mlock` replacement on mobile**:
- iOS: `mlock(2)` is available in the Darwin kernel but restricted for apps. Use `SecureZeroMemory` equivalent via `zeroize` crate.
- Android: `mlock(2)` available, but memory-locked pages are limited. Use `zeroize` as primary mitigation.

---

## Recommended Architecture Per Platform

### Android

```
Arx Runa Android (Tauri 2.0)
  ├── Rust crypto core (unchanged from desktop)
  ├── Rclone as embedded Go library (gomobile bind)
  ├── MediaStore observer → new photo detection
  ├── WorkManager → background encrypt + upload task
  ├── Android Keystore → biometric-protected device_key
  └── Leptos WebView UI (unchanged from desktop)
```

### iOS

```
Arx Runa iOS (Tauri 2.0)
  ├── Rust crypto core (unchanged from desktop)
  ├── Native cloud clients (S3, WebDAV, Google Drive via reqwest)
  │   — rclone used only if App Store review permits
  ├── PhotoKit change observer → new photo detection (foreground / BGTask wakeup)
  ├── URLSession background transfers → upload encrypted chunks
  ├── Secure Enclave → biometric-protected device_key
  └── Leptos WebView UI (unchanged from desktop)
```

---

## Recommendation

| Dimension | Android | iOS |
|---|---|---|
| Technical feasibility | High | Medium |
| Rclone integration | Straightforward | Risky (App Store) |
| Background auto-upload | Full support | Limited (URLSession workaround) |
| Crypto core reuse | 100% | 100% |
| iCloud support | No | No |
| Main risk | Battery optimization edge cases | App Store review, URLSession limitations |
| Recommended path | Full Rclone + WorkManager | Native cloud clients + URLSession |

Android is the recommended first target. The crypto core, Tauri shell, and Leptos UI are all directly reusable. The primary integration work is MediaStore observation, WorkManager scheduling, and embedding rclone as a Go library.

iOS is viable but requires a divergent upload strategy (URLSession background transfers + native cloud APIs instead of rclone) and carries App Store review uncertainty.

---

## Decisions

> Choices made during this research session. Updated as the session progresses.

| Decision | Alternatives considered | Rationale |
|---|---|---|
| **Android as recommended first target** | iOS first, simultaneous Android and iOS | Fewer technical constraints; WorkManager provides reliable background upload; rclone integration via gomobile is straightforward and has real-world precedent (Round-Sync) |
| **Rclone embedded as Go library (gomobile bind) for Android** | Subprocess sidecar, native cloud APIs per-provider | Runs in-process; avoids subprocess spawning; adds ~30–50 MB to APK but provides 70+ backends immediately |
| **Native cloud clients (S3, WebDAV, Google Drive) for iOS instead of rclone** | rclone via gomobile bind | URLSession background transfers require direct HTTP calls — rclone cannot manage transfers when app is suspended; App Store restrictions on dynamic execution increase review risk |
| **Biometric (Secure Enclave / Android Keystore device_key) as Tier 2 replacement on mobile** | USB key file, NFC tag, password-only | USB key impractical on iOS; biometric unlock is native UX on both platforms and device-binds the second factor to the hardware |

---

## Open Questions

1. **Rclone App Store approval**: Has any iOS app successfully shipped with an embedded rclone/gomobile binary? If not, the native client approach is the safer path.

2. **Argon2id parameters on mobile**: What `m_cost`/`t_cost` gives acceptable unlock time (<2s) on a budget Android device (e.g., Snapdragon 4xx)?

3. **Temporary encrypted chunk storage on iOS**: How large can the app container grow before iOS evicts it? Needs a maximum queue depth and retry strategy.

4. **Photo deduplication**: If the user backs up to the same cloud vault from both desktop and mobile, how does the manifest handle duplicate file_ids?

5. **Offline queue**: Should the app queue encrypted chunks locally and upload when connectivity is restored, or re-encrypt on upload? Local queue is faster; re-encrypt avoids persistent encrypted local copies.

~~6. **Android vs iOS priority**~~ — Resolved: Android is the recommended first target. See Decisions.

7. **Monetization**: Following the Cryptomator model (desktop free, mobile paid), what is the right price point for the mobile app? App Store average for privacy tools is $3–10 one-time or $1–3/month.

---

## Sources

| Source | Topic | URL |
|---|---|---|
| **Tauri** | Tauri 2.0 stable release — confirms iOS and Android targets | [v2.tauri.app/blog/tauri-20](https://v2.tauri.app/blog/tauri-20/) |
| **Tauri** | Mobile plugin development guide | [v2.tauri.app/develop/plugins/develop-mobile](https://v2.tauri.app/develop/plugins/develop-mobile/) |
| **rclone / Go Packages** | `librclone/gomobile` package — official gomobile shim for Android/iOS | [pkg.go.dev/github.com/rclone/rclone/librclone/gomobile](https://pkg.go.dev/github.com/rclone/rclone/librclone/gomobile) |
| **GitHub — newhinton/Round-Sync** | Android cloud file manager built on rclone (real-world gomobile usage) | [github.com/newhinton/Round-Sync](https://github.com/newhinton/Round-Sync) |
| **GitHub — rclone/rclone #6784** | `librclone` integration with iOS — documents build failures and open challenges | [github.com/rclone/rclone/issues/6784](https://github.com/rclone/rclone/issues/6784) |
| **Apple Developer** | `URLSessionConfiguration.background(withIdentifier:)` — official background transfer API | [developer.apple.com/documentation/foundation/urlsessionconfiguration/background(withidentifier:)](https://developer.apple.com/documentation/foundation/urlsessionconfiguration/background%28withidentifier:%29) |
| **avanderlee.com** | URLSession background upload pitfalls — tasks require file-based body, suspended behavior | [avanderlee.com/swift/urlsession-common-pitfalls-with-background-download-upload-tasks](https://www.avanderlee.com/swift/urlsession-common-pitfalls-with-background-download-upload-tasks/) |
| **Apple Developer** | WWDC23 — Build robust and resumable file transfers | [developer.apple.com/videos/play/wwdc2023/10006](https://developer.apple.com/videos/play/wwdc2023/10006/) |
| **Apple Developer** | iCloud — Allowing users to manage data (CloudKit is the only official third-party path) | [developer.apple.com/icloud/allowing-users-to-manage-data](https://developer.apple.com/icloud/allowing-users-to-manage-data/) |
| **Android Developers** | Background tasks overview — WorkManager, foreground services | [developer.android.com/develop/background-work/background-tasks](https://developer.android.com/develop/background-work/background-tasks) |
| **Android Developers** | WorkManager getting started — persistent background work that survives app restarts | [developer.android.com/develop/background-work/background-tasks/persistent/getting-started](https://developer.android.com/develop/background-work/background-tasks/persistent/getting-started) |
