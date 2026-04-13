---
title: "Phase 2.1 — USB Key File Format and DeviceMonitor"
created: "2026-04-13T00:00:00Z"
status: implemented
roadmap-phase: 2
sub-phase: "2.1"
design-document: "docs/architecture/designs/authentication-and-session-management/design.md"
sub-phase-roadmap: "docs/architecture/designs/authentication-and-session-management/sub-phases/roadmap.md"
test-agent-required: false
governance-sync-required: true
tags: [auth, phase-2, usb-key-file, device-monitor, blake3, platform]
---

# Plan: Phase 2.1 — USB Key File Format and DeviceMonitor

## 1. Goal

Introduce the `KeySource` and `DeviceMonitor` trait boundaries, their production implementations (`FileKeySource`, `WindowsDeviceMonitor`, `LinuxDeviceMonitor`, `MacOsDeviceMonitor`) and their test doubles (`MockKeySource`, `MockDeviceMonitor`), plus BLAKE3 auto-detection and a local path-hint file — the hardware-factor plumbing that Phase 2.2 will layer Argon2id on top of.

## 2. Context

**Roadmap**: Phase 2 — Authentication and Session Management (`docs/roadmap.md` lines 55–61). Depends on Phase 1 (complete). Produces the Tier-2 key-file inputs that Phase 2.2 feeds to Argon2id.

**Sub-phase roadmap**: `docs/architecture/designs/authentication-and-session-management/sub-phases/roadmap.md`. Strict order 2.1 → 2.2 → 2.3 → 2.4. Sub-phase 2.1 is the first unit. The roadmap states security review is **not required** for 2.1 (preimage-resistant BLAKE3 fingerprint only, no crypto ops). Estimated scope: ~230 lines production + ~100 lines tests.

**Sub-phase doc**: `docs/architecture/designs/authentication-and-session-management/sub-phases/2.1-usb-key-file-and-device-monitor.md` (deliverables 1–13).

**Parent design sections used** (lines 50–103, 462–501 of `docs/architecture/designs/authentication-and-session-management/design.md`):

- **USB Key File** (format, generation, auto-detection, device monitoring): lines 50–103.
- **Contract Surface** (canonical per CLAUDE.md): the sub-phase must match the interfaces listed under "External interfaces are `KeySource::read_key` and `DeviceMonitor::watch` (production plus mock implementations)" (design.md line 28).
- **Tier-2 key file size**: fixed 32 bytes. Filenames not checked. Hash is `blake3::hash(content)` stored in vault header (not a secret — BLAKE3 preimage-resistant).

**Existing state** (commit `39572e4`):

- `src-tauri/src/auth/mod.rs` (8 lines) declares `pub mod error; pub mod types;` and nothing else.
- `src-tauri/src/auth/error.rs` defines an empty `AuthError` enum (`#[non_exhaustive]`, no variants yet).
- `src-tauri/src/auth/types/mod.rs` is just a module comment.
- `src-tauri/src/crypto/types/mod.rs` already defines `pub struct Blake3Hash(pub [u8; 32])` at line 166 — reusable as the reference-hash type for auto-detection.
- `src-tauri/src/memory/` exists but is empty (mlock belongs to Phase 2.2).
- `src-tauri/Cargo.toml` already pins `blake3 = "1"`, `zeroize = { version = "1", features = ["derive"] }`, `tokio` with `sync` feature, `thiserror = "2"`, `serde = { version = "1", features = ["derive"] }`, `serde_json = "1"`, `tempfile = "3"` (dev-dep), `assert_matches = "1"` (dev-dep). **No** `udev`, **no** `tokio-stream`, **no** `futures-core`, **no** `windows`, **no** `core-foundation` — this plan adds them.
- `.claude/rules/auth.md` and `.github/instructions/auth.instructions.md` already describe the `KeySource` and `DeviceMonitor` trait names and the `Pin<Box<dyn Stream<Item = DeviceEvent> + Send>>` return type. They are in sync with each other (no mirror drift).
- `CLAUDE.md` platform-compatibility rule requires all three targets (Windows, macOS, Linux) to be implemented for any new platform-specific feature, or the limitation documented in the canonical design. The sub-phase lists all four monitors; this plan implements all of them.

**No pending architectural decisions** in the roadmap touch Phase 2.1 directly.

## 3. Design Concerns / Open Questions

### DC-1 — `Stream` trait import not pinned to a crate

- **Concern**: The sub-phase trait signature uses `Stream<Item = DeviceEvent>` without naming the source crate. `Stream` is not in `core` yet; it lives in `futures_core::Stream`, `tokio_stream::Stream` (re-export), or `std::async_iter::AsyncIterator` (unstable).
- **Source**: 2.1 doc deliverable 5 (line 16).
- **Impact**: If left unspecified, Codex could pick either `futures` or `tokio-stream`, causing a dependency mismatch with Phase 2.2/2.3 when they consume the stream.
- **Classification**: Non-blocking.
- **Resolution**: Use `tokio_stream` (adds `tokio-stream = "0.1"`). `tokio_stream::Stream` is a re-export of `futures_core::Stream`, so the trait item type is `tokio_stream::Stream`. `ReceiverStream<T>` from the same crate gives the `mpsc::Receiver → Stream` bridge each OS monitor needs.
- **Documentation sync required on implementation**: None — crate choice is an implementation detail not mentioned in design.md.

### DC-2 — Device-event channel type and capacity unspecified

- **Concern**: The sub-phase says OS monitors "bridge mount events into a `tokio` channel" but does not specify bounded vs. unbounded or the capacity.
- **Source**: 2.1 doc Implementation Notes bullet 5 (line 73).
- **Impact**: An unbounded channel can OOM under a pathological mount storm. A 1-deep bounded channel can lose events if the consumer stalls.
- **Classification**: Non-blocking.
- **Resolution**: Use `tokio::sync::mpsc::channel::<DeviceEvent>(32)`. Rationale: mount/unmount events are human-paced (single-digit per minute in practice); 32 is enough headroom that a consumer stall does not drop events, but bounded enough to detect a run-away producer at test time.
- **Documentation sync required on implementation**: None.

### DC-3 — `MockKeySource` feature gating

- **Concern**: The sub-phase says "gate it behind `#[cfg(test)]` or a `test-utils` feature flag" — two mutually exclusive options.
- **Source**: 2.1 doc Implementation Notes bullet 2 (line 70).
- **Impact**: If gated behind `#[cfg(test)]`, downstream sub-phases (2.2, 2.3) cannot use `MockKeySource` from their own `#[cfg(test)]` modules because the mock only exists in `auth` unit-test scope. A feature flag is needed if the mock is to be reused across modules.
- **Classification**: Non-blocking.
- **Resolution**: Gate `MockKeySource` behind `#[cfg(any(test, feature = "test-utils"))]` and add a `test-utils` feature to `src-tauri/Cargo.toml` (no default members). Sub-phases 2.2+ enable the feature only in `[dev-dependencies]`-style activation via `cargo test --features test-utils`. Same pattern for `MockDeviceMonitor`.
- **Documentation sync required on implementation**: None.

### DC-4 — Recursive vs. root-only scan in auto-detection

- **Concern**: Sub-phase deliverable 11 says "scan the mounted volume for files that are exactly 32 bytes". "Mounted volume" implies the full subtree, but recursive walks of large drives can be slow and surface platform issues (symlink loops, permission errors).
- **Source**: 2.1 doc deliverable 11 (line 22); design.md line 68 ("Arx Runa scans it for files that are exactly 32 bytes").
- **Impact**: Codex may default to `std::fs::read_dir` (non-recursive), missing key files in subdirectories the user chose. Or use unbounded recursion and hit symlink-loop issues.
- **Classification**: Non-blocking.
- **Resolution**: Recursive walk bounded to `max_depth = 8` and `follow_symlinks = false`. Skip entries where `metadata()` fails (permission-denied on macOS `.Trashes`/`.Spotlight-V100` and Windows `System Volume Information`). Use `walkdir = "2"` crate (well-maintained, handles the depth + symlink flags cleanly).
- **Documentation sync required on implementation**: None — the design says "scan the mounted volume" which is satisfied by a depth-bounded recursive walk.

### DC-5 — Local path-hint file schema and location

- **Concern**: Sub-phase mentions a JSON file at `%APPDATA%/arx-runa/key-hint.json` etc. but does not specify the JSON schema, whether it is per-vault or global, or how it behaves for multiple vaults.
- **Source**: 2.1 doc deliverable 12 + Implementation Notes bullet 6 (lines 23, 75).
- **Impact**: Codex may guess a single-vault schema and break when Phase 2.4 introduces the vault-id concept.
- **Classification**: Non-blocking.
- **Resolution**: Schema keyed by `vault_id`:
  ```json
  {
    "schema_version": 1,
    "hints": {
      "<vault_id_uuid>": { "last_key_file_path": "/absolute/path/to/file" }
    }
  }
  ```
  Write atomically (write to `key-hint.json.tmp`, `rename` over `key-hint.json`). Read tolerantly — if parse fails or the file is missing, treat as empty and fall back to full scan. Phase 2.1 will only populate the `hints` map with whatever `vault_id` the caller provides; Phase 2.4 will own the upsert path during vault creation. The tests for this phase exercise a canned `vault_id` string.
- **Documentation sync required on implementation**: Add the path-hint schema to `docs/architecture/designs/authentication-and-session-management/design.md` under the "Local path hint" subsection (design.md line 73) as a follow-up doc sync action. Flagged in section 8.

### DC-6 — Linux monitor scoping (partition vs. disk)

- **Concern**: Sub-phase deliverable 8 says "`udev` crate, monitoring for block device add events with `SUBSYSTEM=block`" — but the roadmap Risks table (roadmap.md line 129) says "scope monitoring to removable media (`DEVTYPE=partition` on Linux, `DBTF_NET` exclusion on Windows)". These are complementary, not contradictory, but the sub-phase under-specifies.
- **Source**: 2.1 doc deliverable 8 (line 19); roadmap.md risks row 2 (line 129).
- **Impact**: Without `DEVTYPE=partition` filtering, the monitor fires on every block device add including LVM and loop devices — noise, not usability.
- **Classification**: Non-blocking.
- **Resolution**: `LinuxDeviceMonitor` filters `SUBSYSTEM=block` **and** `DEVTYPE=partition` **and** reads the `ID_BUS=usb` or `ID_DRIVE_THUMB=1` udev property to further scope to removable media. Mount-path resolution uses `/proc/self/mountinfo` (reading the mount table when the partition appears; udev itself does not provide the mount point). If the partition is not auto-mounted within 2 s, the monitor skips it — the user will not get a `Mounted` event, which is correct because Arx Runa cannot read files from an unmounted partition.
- **Documentation sync required on implementation**: None — the design document already describes removable-media scoping in spirit; the refinement is an implementation detail.

### DC-7 — Windows monitor: WMI vs. `RegisterDeviceNotification`

- **Concern**: The sub-phase offers both "WMI `Win32_VolumeChangeEvent`" and "`RegisterDeviceNotification`" as options (deliverable 7). They have very different programming models. Windows also requires filtering out non-removable volumes.
- **Source**: 2.1 doc deliverable 7 (line 18); design.md lines 79–81; roadmap.md risks row 2.
- **Impact**: WMI requires COM initialization on a dedicated thread and is heavy. `RegisterDeviceNotification` requires a hidden window and a message pump. Both are bug-prone.
- **Classification**: Non-blocking.
- **Resolution**: Use WMI `Win32_VolumeChangeEvent` via the `wmi` crate (`wmi = "0.14"`) — simpler than a hidden-window message pump, does not require `unsafe`, and `wmi` crate handles the COM thread model. After receiving an arrival event (`EventType = 2`), call `GetDriveTypeW` via the `windows` crate to filter for `DRIVE_REMOVABLE` (= 2), rejecting `DRIVE_FIXED`, `DRIVE_REMOTE`, `DRIVE_CDROM`, `DRIVE_RAMDISK`. The `DBTF_NET` exclusion from the risks row is satisfied by the `DRIVE_REMOVABLE` filter.
- **Documentation sync required on implementation**: None — WMI is one of the two options already listed in design.md line 79.

### DC-8 — macOS `MacOsDeviceMonitor` FFI surface

- **Concern**: Sub-phase deliverable 9 describes using `DiskArbitration` via `core-foundation` + raw FFI but does not specify the exact bindings or crate layout. `disk-arbitration` on crates.io is unmaintained (last published 2015).
- **Source**: 2.1 doc deliverable 9 (line 20); design.md line 81; Implementation Notes bullet 4 (line 73).
- **Impact**: Codex may reach for the unmaintained crate or roll an inconsistent FFI shim that breaks soundness review.
- **Classification**: Non-blocking.
- **Resolution**: Add `core-foundation = "0.10"` and `core-foundation-sys = "0.8"`. Write a thin `#[link(name = "DiskArbitration", kind = "framework")] extern "C"` block declaring only `DASessionCreate`, `DASessionSetDispatchQueue`, `DARegisterDiskAppearedCallback`, `DARegisterDiskDisappearedCallback`, `DADiskCopyDescription`, and the opaque pointer types `DASessionRef`, `DADiskRef`. The monitor spawns a dedicated thread running `CFRunLoopRun()` with the session attached, and the callbacks (`extern "C" fn`) post `DeviceEvent`s through the bounded `mpsc` channel from DC-2. Each `unsafe` block carries a `// SAFETY:` comment explaining the invariant (framework lifetime, main-thread / CFRunLoop-thread guarantees, pointer provenance).
- **Documentation sync required on implementation**: None.

### DC-9 — Verification-date metadata drift in `.claude/rules/auth.md`

- **Concern**: `.claude/rules/auth.md` and `.github/instructions/auth.instructions.md` both say "last verified against design dated **2026-04-07**", but `design.md` now says "Last updated: **2026-04-12**". This is stale metadata, not a guidance contradiction.
- **Source**: `.claude/rules/auth.md` line 3; `.github/instructions/auth.instructions.md` line 7; `design.md` line 4.
- **Impact**: Low — the guidance content still matches design.md. But the stale date is a misleading signal to future planners.
- **Classification**: Non-blocking.
- **Resolution**: After implementation, bump both verification dates to `2026-04-12` (see Governance sync actions, Section 9).
- **Documentation sync required on implementation**: None beyond the rule-file bump.

## 4. Assumptions

These facts are not stated in the sub-phase but the plan takes them as given. If any is wrong, the implementation is wrong — correct them before handoff.

1. **Crate choices**: `tokio-stream = "0.1"` (stream trait + `ReceiverStream`), `walkdir = "2"` (bounded recursive scan), `wmi = "0.14"` + `windows = "0.58"` (Windows monitor), `udev = "0.9"` (Linux monitor), `core-foundation = "0.10"` + `core-foundation-sys = "0.8"` (macOS monitor), `dirs = "5"` (cross-platform config-dir lookup for the path-hint file).
2. **Feature flag**: `test-utils` added to `[features]` in `src-tauri/Cargo.toml`, empty default, gates `MockKeySource` and `MockDeviceMonitor` behind `#[cfg(any(test, feature = "test-utils"))]`.
3. **Channel type**: `tokio::sync::mpsc::channel::<DeviceEvent>(32)`; OS monitors hold the `Sender` and return a `ReceiverStream` from `watch()`.
4. **Path hint file layout**: `dirs::data_local_dir()?.join("arx-runa/key-hint.json")`. On Windows this resolves to `%LOCALAPPDATA%\arx-runa\key-hint.json` (close to the sub-phase's `%APPDATA%/arx-runa/…` — `data_local_dir` is the `dirs` idiom and is the closer match to "not roamed"). On Linux: `~/.local/share/arx-runa/key-hint.json`. On macOS: `~/Library/Application Support/arx-runa/key-hint.json`. The directory is created with `fs::create_dir_all` on first write.
5. **Auto-detection signature**: `pub async fn find_key_file(mount_path: &Path, reference_hash: &Blake3Hash) -> Result<Option<PathBuf>, KeySourceError>` — returns `Ok(None)` for no-match, `Err(KeySourceError::ReadFailed)` only for hard I/O errors at scan time, not for individual per-file permission denials (which are skipped).
6. **`FileKeySource` constructor**: `FileKeySource::new(path: PathBuf) -> Self` stores the path; `read_key` performs the actual I/O and size check. Rationale: construction must be infallible so callers can hold a `FileKeySource` before the USB drive is inserted.
7. **`InvalidSize` variant carries `actual: usize`** exactly as stated. The `NotFound` variant carries no payload (the path is already known to the caller).
8. **`Zeroizing<[u8; 32]>`**: the 32 bytes are read into a stack-allocated `[u8; 32]` buffer via `Read::read_exact`, then wrapped in `Zeroizing::new`. No `Vec<u8>` intermediate, per sub-phase Implementation Notes bullet 1.
9. **Event ordering for `MockDeviceMonitor`**: events are delivered in the exact order pushed by test code (FIFO semantics of `mpsc::channel`). Test emits `Mounted(/media/usb)` then `Unmounted(/media/usb)` and asserts they arrive in that order.
10. **Tests are written inline in each file** under `#[cfg(test)]` modules, not in a separate `tests/` crate, consistent with the project convention visible in `src-tauri/src/crypto/types/mod.rs`.
11. **No Tauri command is wired up in this sub-phase** — `KeySource`/`DeviceMonitor` are backend traits only. The IPC surface lands in Phase 6.

## 5. Approach

All file paths are absolute. Every step lists the exact types and signatures Codex must produce.

### 5.1 Add dependencies

**File**: `C:\Users\chris\source\repos\arx-runa\src-tauri\Cargo.toml`

Under `[dependencies]` add:

```toml
# --- Auth / Phase 2.1 ---
tokio-stream = "0.1"
walkdir = "2"
dirs = "5"

[target.'cfg(target_os = "linux")'.dependencies]
udev = "0.9"

[target.'cfg(target_os = "windows")'.dependencies]
wmi = "0.14"
windows = { version = "0.58", features = ["Win32_Storage_FileSystem", "Win32_Foundation"] }

[target.'cfg(target_os = "macos")'.dependencies]
core-foundation = "0.10"
core-foundation-sys = "0.8"
```

Under `[features]` (create the table if it does not yet exist) add:

```toml
[features]
default = []
test-utils = []
```

Verify with `cargo check` on each platform; use `cargo check --target x86_64-pc-windows-msvc` / `--target x86_64-unknown-linux-gnu` / `--target x86_64-apple-darwin` if available locally. At minimum, `cargo check` must pass on the host platform.

### 5.2 Declare auth submodules

**File**: `C:\Users\chris\source\repos\arx-runa\src-tauri\src\auth\mod.rs`

Replace the current 8-line body with:

```rust
//! Arx Runa auth module.
//!
//! Authentication and session management: Argon2id KDF, USB key file, session
//! lifecycle, memory locking.

pub mod autodetect;
pub mod device_monitor;
pub mod error;
pub mod key_source;
pub mod path_hint;
pub mod types;

pub use autodetect::find_key_file;
pub use device_monitor::{DeviceEvent, DeviceMonitor};
pub use error::KeySourceError;
pub use key_source::{FileKeySource, KeySource};
pub use path_hint::{KeyHintStore, VaultHint};

#[cfg(any(test, feature = "test-utils"))]
pub use device_monitor::MockDeviceMonitor;
#[cfg(any(test, feature = "test-utils"))]
pub use key_source::MockKeySource;
```

### 5.3 Extend `auth/error.rs` with `KeySourceError`

**File**: `C:\Users\chris\source\repos\arx-runa\src-tauri\src\auth\error.rs`

Replace the current body with:

```rust
//! Error types for the auth module.

use std::io;

use thiserror::Error;

/// Errors produced by the auth module.
#[non_exhaustive]
#[derive(Debug, Error)]
pub enum AuthError {
    /// A key-source operation failed.
    #[error(transparent)]
    KeySource(#[from] KeySourceError),
}

/// Errors produced by a [`crate::auth::KeySource`] implementation.
#[non_exhaustive]
#[derive(Debug, Error)]
pub enum KeySourceError {
    /// The configured key-file path does not exist.
    #[error("key file not found")]
    NotFound,

    /// The file exists but is not exactly 32 bytes.
    #[error("key file has invalid size: {actual} bytes (expected 32)")]
    InvalidSize { actual: usize },

    /// An unrecoverable I/O error occurred while reading the key file.
    #[error("failed to read key file")]
    ReadFailed(#[source] io::Error),
}
```

Rationale for the `NotFound` / `ReadFailed` split: on Linux `ErrorKind::NotFound` is trivially distinguishable; plan maps it explicitly and folds every other `io::Error` into `ReadFailed`.

### 5.4 Implement `key_source.rs`

**File**: `C:\Users\chris\source\repos\arx-runa\src-tauri\src\auth\key_source.rs` (new)

```rust
//! `KeySource` trait and its production and test implementations.

use std::fs::File;
use std::io::{ErrorKind, Read};
use std::path::PathBuf;

use zeroize::Zeroizing;

use crate::auth::error::KeySourceError;

/// Reads a 32-byte USB key file.
///
/// Implementations must return the exact 32 raw bytes without ever
/// materialising them in a heap-allocated intermediate buffer that
/// bypasses zeroization.
pub trait KeySource: Send + Sync {
    /// Reads the underlying key file and returns its 32-byte content.
    fn read_key(&self) -> Result<Zeroizing<[u8; 32]>, KeySourceError>;
}

/// Reads a key file from a filesystem path.
#[derive(Debug, Clone)]
pub struct FileKeySource {
    path: PathBuf,
}

impl FileKeySource {
    /// Builds a `FileKeySource` that will read `path` on every `read_key` call.
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// Returns the configured path (for logging and diagnostics — never the content).
    pub fn path(&self) -> &std::path::Path {
        &self.path
    }
}

impl KeySource for FileKeySource {
    fn read_key(&self) -> Result<Zeroizing<[u8; 32]>, KeySourceError> {
        let mut file = match File::open(&self.path) {
            Ok(file) => file,
            Err(error) if error.kind() == ErrorKind::NotFound => {
                return Err(KeySourceError::NotFound);
            }
            Err(error) => return Err(KeySourceError::ReadFailed(error)),
        };

        let metadata = file.metadata().map_err(KeySourceError::ReadFailed)?;
        let length = metadata.len();
        if length != 32 {
            return Err(KeySourceError::InvalidSize {
                actual: length as usize,
            });
        }

        let mut buffer: Zeroizing<[u8; 32]> = Zeroizing::new([0u8; 32]);
        file.read_exact(buffer.as_mut())
            .map_err(KeySourceError::ReadFailed)?;
        Ok(buffer)
    }
}

/// A `KeySource` that returns caller-controlled bytes — test only.
#[cfg(any(test, feature = "test-utils"))]
#[derive(Debug, Clone)]
pub struct MockKeySource {
    bytes: [u8; 32],
}

#[cfg(any(test, feature = "test-utils"))]
impl MockKeySource {
    /// Creates a mock with the given 32 bytes.
    pub fn new(bytes: [u8; 32]) -> Self {
        Self { bytes }
    }
}

#[cfg(any(test, feature = "test-utils"))]
impl KeySource for MockKeySource {
    fn read_key(&self) -> Result<Zeroizing<[u8; 32]>, KeySourceError> {
        Ok(Zeroizing::new(self.bytes))
    }
}
```

Add `#[cfg(test)] mod tests { … }` at the bottom covering:

- `test_file_key_source_reads_valid_32_byte_file` — write a 32-byte file via `tempfile::NamedTempFile`, read back, assert contents.
- `test_file_key_source_returns_invalid_size_for_31_bytes` — `InvalidSize { actual: 31 }`.
- `test_file_key_source_returns_invalid_size_for_33_bytes` — `InvalidSize { actual: 33 }`.
- `test_file_key_source_returns_invalid_size_for_empty_file` — `InvalidSize { actual: 0 }`.
- `test_file_key_source_returns_not_found_for_missing_path` — build with a path that does not exist, assert `KeySourceError::NotFound`.
- `test_mock_key_source_returns_controlled_bytes`.

### 5.5 Implement `device_monitor.rs`

**File**: `C:\Users\chris\source\repos\arx-runa\src-tauri\src\auth\device_monitor.rs` (new)

Structure:

```rust
//! `DeviceMonitor` trait, `DeviceEvent` enum, and platform-specific implementations.

use std::path::PathBuf;
use std::pin::Pin;

use tokio_stream::Stream;

/// A removable-storage mount or unmount event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeviceEvent {
    /// A removable device was mounted at `mount_path`.
    Mounted { mount_path: PathBuf },
    /// A removable device was unmounted from `mount_path`.
    Unmounted { mount_path: PathBuf },
}

/// Monitors for removable storage device mount and unmount events.
///
/// The return type is `Pin<Box<dyn Stream<…> + Send>>` rather than an
/// RPITIT (`-> impl Stream<…>`) because an `impl Trait` return would
/// make the trait non-dyn-safe, and `Box<dyn DeviceMonitor>` is the
/// dispatch mechanism runtime uses to pick between the OS-specific
/// implementations.
pub trait DeviceMonitor: Send + Sync {
    /// Returns a stream of device mount events.
    fn watch(&self) -> Pin<Box<dyn Stream<Item = DeviceEvent> + Send>>;
}

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::LinuxDeviceMonitor;

#[cfg(target_os = "windows")]
mod windows_impl;
#[cfg(target_os = "windows")]
pub use windows_impl::WindowsDeviceMonitor;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::MacOsDeviceMonitor;

#[cfg(any(test, feature = "test-utils"))]
mod mock;
#[cfg(any(test, feature = "test-utils"))]
pub use mock::MockDeviceMonitor;
```

#### 5.5a `device_monitor/mock.rs` (always-available test double)

**File**: `C:\Users\chris\source\repos\arx-runa\src-tauri\src\auth\device_monitor\mock.rs` (new)

```rust
//! In-memory `DeviceMonitor` used by tests — no hardware required.

use std::pin::Pin;
use std::sync::Mutex;

use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::Stream;

use crate::auth::device_monitor::{DeviceEvent, DeviceMonitor};

/// A `DeviceMonitor` that emits events pushed via [`MockDeviceMonitor::push`].
pub struct MockDeviceMonitor {
    sender: mpsc::Sender<DeviceEvent>,
    receiver: Mutex<Option<mpsc::Receiver<DeviceEvent>>>,
}

impl MockDeviceMonitor {
    /// Creates a mock monitor with a bounded 32-deep channel.
    pub fn new() -> Self {
        let (sender, receiver) = mpsc::channel(32);
        Self {
            sender,
            receiver: Mutex::new(Some(receiver)),
        }
    }

    /// Pushes a synthetic event. Panics if the channel is full or closed —
    /// both indicate a test error.
    pub fn push(&self, event: DeviceEvent) {
        self.sender
            .try_send(event)
            .expect("mock device monitor channel is full or closed");
    }
}

impl Default for MockDeviceMonitor {
    fn default() -> Self {
        Self::new()
    }
}

impl DeviceMonitor for MockDeviceMonitor {
    fn watch(&self) -> Pin<Box<dyn Stream<Item = DeviceEvent> + Send>> {
        let receiver = self
            .receiver
            .lock()
            .expect("mock device monitor mutex poisoned")
            .take()
            .expect("MockDeviceMonitor::watch called more than once");
        Box::pin(ReceiverStream::new(receiver))
    }
}
```

Move the `mod mock;` declaration into the `#[cfg(any(test, feature = "test-utils"))]` block in `device_monitor.rs` (already shown above). Because this file lives at `auth/device_monitor/mock.rs`, `device_monitor.rs` must become a `device_monitor/mod.rs` — update step 5.5 accordingly.

Codex note — the module layout for this module is:

```
src-tauri/src/auth/
├── device_monitor/
│   ├── mod.rs          (trait + DeviceEvent + cfg-gated re-exports)
│   ├── mock.rs
│   ├── linux.rs        (target_os = "linux" only)
│   ├── windows_impl.rs (target_os = "windows" only)
│   └── macos.rs        (target_os = "macos" only)
```

#### 5.5b `device_monitor/linux.rs`

```rust
//! Linux `DeviceMonitor` using the `udev` crate.

use std::path::PathBuf;
use std::pin::Pin;

use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::Stream;

use crate::auth::device_monitor::{DeviceEvent, DeviceMonitor};

/// A `DeviceMonitor` backed by udev block-device events.
pub struct LinuxDeviceMonitor;

impl LinuxDeviceMonitor {
    /// Creates a new Linux monitor.
    pub fn new() -> Self {
        Self
    }
}

impl Default for LinuxDeviceMonitor {
    fn default() -> Self {
        Self::new()
    }
}

impl DeviceMonitor for LinuxDeviceMonitor {
    fn watch(&self) -> Pin<Box<dyn Stream<Item = DeviceEvent> + Send>> {
        let (sender, receiver) = mpsc::channel::<DeviceEvent>(32);

        // Spawn a dedicated blocking task: udev's MonitorSocket is a blocking
        // iterator that we bridge into the async channel via try_send. The
        // spawn uses tokio::task::spawn_blocking so the runtime does not
        // block a worker.
        tokio::task::spawn_blocking(move || {
            if let Err(error) = run_udev_loop(sender) {
                tracing::warn!(%error, "linux device monitor loop exited");
            }
        });

        Box::pin(ReceiverStream::new(receiver))
    }
}

fn run_udev_loop(sender: mpsc::Sender<DeviceEvent>) -> std::io::Result<()> {
    let socket = udev::MonitorBuilder::new()?
        .match_subsystem_devtype("block", "partition")?
        .listen()?;

    for event in socket.iter() {
        let Some(event_type) = event.event_type() else { continue; };
        let is_removable = event
            .property_value("ID_BUS")
            .and_then(|value| value.to_str())
            .map(|value| value == "usb")
            .unwrap_or(false);
        if !is_removable {
            continue;
        }

        let Some(devnode) = event.devnode() else { continue; };
        let Some(mount_path) = resolve_mount_path(devnode) else { continue; };

        let message = match event_type {
            udev::EventType::Add => DeviceEvent::Mounted { mount_path },
            udev::EventType::Remove => DeviceEvent::Unmounted { mount_path },
            _ => continue,
        };

        if sender.blocking_send(message).is_err() {
            break;
        }
    }
    Ok(())
}

fn resolve_mount_path(devnode: &std::path::Path) -> Option<PathBuf> {
    // Read /proc/self/mountinfo and find the line whose source matches devnode.
    // Implementation detail: parse the fifth whitespace-separated field (mount point)
    // on lines where the tenth field (source) equals the devnode path.
    let mountinfo = std::fs::read_to_string("/proc/self/mountinfo").ok()?;
    for line in mountinfo.lines() {
        let mut fields = line.split_whitespace();
        let mount_point = fields.nth(4)?;
        let source = fields.nth(4)?; // skip root, options, optional, separator
        if std::path::Path::new(source) == devnode {
            return Some(PathBuf::from(mount_point));
        }
    }
    None
}
```

Note the `/proc/self/mountinfo` parser is a narrow implementation detail — see `man proc` §`/proc/[pid]/mountinfo` for field order. The mount point is always field 5 (1-indexed); the separator `-` appears before the source field, and field indices shift. Codex must verify field indices by running the unit test below against a real `mountinfo` sample. Include a unit test `test_resolve_mount_path_parses_sample_mountinfo` that feeds a canned string into a helper function extracted from `resolve_mount_path`.

#### 5.5c `device_monitor/windows_impl.rs`

```rust
//! Windows `DeviceMonitor` using WMI `Win32_VolumeChangeEvent`.

use std::path::PathBuf;
use std::pin::Pin;

use serde::Deserialize;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::Stream;
use wmi::{COMLibrary, FilterValue, WMIConnection};

use crate::auth::device_monitor::{DeviceEvent, DeviceMonitor};

/// A `DeviceMonitor` backed by WMI `Win32_VolumeChangeEvent`.
pub struct WindowsDeviceMonitor;

impl WindowsDeviceMonitor {
    /// Creates a new Windows monitor.
    pub fn new() -> Self {
        Self
    }
}

impl Default for WindowsDeviceMonitor {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename = "Win32_VolumeChangeEvent")]
#[serde(rename_all = "PascalCase")]
struct VolumeChangeEvent {
    event_type: u32,
    drive_name: String,
}

impl DeviceMonitor for WindowsDeviceMonitor {
    fn watch(&self) -> Pin<Box<dyn Stream<Item = DeviceEvent> + Send>> {
        let (sender, receiver) = mpsc::channel::<DeviceEvent>(32);

        tokio::task::spawn_blocking(move || {
            if let Err(error) = run_wmi_loop(sender) {
                tracing::warn!(%error, "windows device monitor loop exited");
            }
        });

        Box::pin(ReceiverStream::new(receiver))
    }
}

fn run_wmi_loop(sender: mpsc::Sender<DeviceEvent>) -> wmi::WMIResult<()> {
    let com_lib = COMLibrary::new()?;
    let wmi_con = WMIConnection::new(com_lib)?;

    let iterator = wmi_con.notification_native_wrapper::<VolumeChangeEvent>(None)?;
    for event in iterator {
        let event = match event { Ok(event) => event, Err(_) => continue };
        if !is_removable_drive(&event.drive_name) {
            continue;
        }
        let mount_path = PathBuf::from(&event.drive_name);
        let message = match event.event_type {
            2 => DeviceEvent::Mounted { mount_path },   // Arrival
            3 => DeviceEvent::Unmounted { mount_path }, // Removal
            _ => continue,                               // 1 = Config change, 4 = Docking
        };
        if sender.blocking_send(message).is_err() {
            break;
        }
    }
    Ok(())
}

fn is_removable_drive(drive: &str) -> bool {
    use windows::core::PCWSTR;
    use windows::Win32::Storage::FileSystem::{GetDriveTypeW, DRIVE_REMOVABLE};

    let mut wide: Vec<u16> = drive.encode_utf16().collect();
    wide.push(0);
    // SAFETY: `wide` is a null-terminated UTF-16 buffer kept alive for the
    // duration of the GetDriveTypeW call.
    let drive_type = unsafe { GetDriveTypeW(PCWSTR(wide.as_ptr())) };
    drive_type == DRIVE_REMOVABLE
}
```

Verify `wmi` crate's exact API — if `notification_native_wrapper` has a different name in `wmi = "0.14"`, use the equivalent iterator constructor (`wmi_con.notification::<VolumeChangeEvent>()`) and update the `for event in iterator` loop accordingly. Codex must `cargo doc --open -p wmi` (or read docs.rs) to confirm before committing.

#### 5.5d `device_monitor/macos.rs`

Skeleton (implementation notes inlined):

```rust
//! macOS `DeviceMonitor` using DiskArbitration framework.

use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use std::thread;

use core_foundation::base::TCFType;
use core_foundation::runloop::{CFRunLoop, CFRunLoopRun};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::Stream;

use crate::auth::device_monitor::{DeviceEvent, DeviceMonitor};

pub struct MacOsDeviceMonitor;

impl MacOsDeviceMonitor {
    pub fn new() -> Self {
        Self
    }
}

impl Default for MacOsDeviceMonitor {
    fn default() -> Self {
        Self::new()
    }
}

impl DeviceMonitor for MacOsDeviceMonitor {
    fn watch(&self) -> Pin<Box<dyn Stream<Item = DeviceEvent> + Send>> {
        let (sender, receiver) = mpsc::channel::<DeviceEvent>(32);
        let sender = Arc::new(sender);

        thread::spawn(move || {
            // SAFETY: The DiskArbitration session is created, attached to the
            // current thread's CFRunLoop, and only used from this thread.
            // The raw pointers handed to DARegisterDisk*Callback are stable
            // for the duration of CFRunLoopRun(), which blocks until the
            // thread exits (i.e., for program lifetime).
            unsafe {
                run_disk_arbitration_loop(sender);
            }
        });

        Box::pin(ReceiverStream::new(receiver))
    }
}

mod ffi {
    use core_foundation::base::CFAllocatorRef;
    use core_foundation::dictionary::CFDictionaryRef;
    use core_foundation::runloop::CFRunLoopRef;
    use core_foundation::string::CFStringRef;
    use std::os::raw::c_void;

    pub type DASessionRef = *mut c_void;
    pub type DADiskRef = *mut c_void;
    pub type DADiskAppearedCallback =
        extern "C" fn(disk: DADiskRef, context: *mut c_void);
    pub type DADiskDisappearedCallback =
        extern "C" fn(disk: DADiskRef, context: *mut c_void);

    #[link(name = "DiskArbitration", kind = "framework")]
    unsafe extern "C" {
        pub fn DASessionCreate(allocator: CFAllocatorRef) -> DASessionRef;
        pub fn DASessionScheduleWithRunLoop(
            session: DASessionRef,
            run_loop: CFRunLoopRef,
            run_loop_mode: CFStringRef,
        );
        pub fn DARegisterDiskAppearedCallback(
            session: DASessionRef,
            match_: CFDictionaryRef,
            callback: DADiskAppearedCallback,
            context: *mut c_void,
        );
        pub fn DARegisterDiskDisappearedCallback(
            session: DASessionRef,
            match_: CFDictionaryRef,
            callback: DADiskDisappearedCallback,
            context: *mut c_void,
        );
        pub fn DADiskCopyDescription(disk: DADiskRef) -> CFDictionaryRef;
    }
}

unsafe fn run_disk_arbitration_loop(_sender: Arc<mpsc::Sender<DeviceEvent>>) {
    // TODO(implementer): wire DASessionCreate → schedule on current CFRunLoop
    // → DARegisterDiskAppearedCallback → translate DADiskCopyDescription's
    // DAVolumeMountable + DAVolumePath keys into DeviceEvent::Mounted,
    // post into `_sender` via its blocking_send.
    //
    // Full implementation: parse the CFDictionary returned by
    // DADiskCopyDescription, extract kDADiskDescriptionVolumeMountableKey
    // (filter for true), extract kDADiskDescriptionVolumePathKey (CFURLRef),
    // convert to PathBuf, push Mounted. Same for disappeared → Unmounted.
    // Then call CFRunLoopRun() to pump the loop.
    let _ = CFRunLoopRun;
    let _ = CFRunLoop::get_current();
}
```

The `TODO(implementer)` is explicitly kept in the plan because filling it in requires reading Apple's DiskArbitration headers and is the place where Codex will spend the most time. The plan asks for a **compiling stub** that registers the callbacks and exits the run loop cleanly when dropped — full event translation can land incrementally as long as the trait is implemented and `MockDeviceMonitor` gives tests full coverage.

**Acceptance for 5.5d**: file compiles on macOS (`cargo check --target x86_64-apple-darwin`), `MacOsDeviceMonitor::new` and `MacOsDeviceMonitor::watch()` return without panic, and the stream yields no events yet on a stub-only implementation. Mark the incomplete event translation with a `#[cfg(not(doctest))]` runtime panic if `PANIC_ON_UNIMPLEMENTED_MACOS_MONITOR=1` is set in the environment; tests must not set that variable. A follow-up ticket tracks filling the stub — noted in section 8.

### 5.6 Implement `autodetect.rs`

**File**: `C:\Users\chris\source\repos\arx-runa\src-tauri\src\auth\autodetect.rs` (new)

```rust
//! BLAKE3-based auto-detection of a key file on a mounted volume.

use std::io::Read;
use std::path::{Path, PathBuf};

use walkdir::WalkDir;

use crate::auth::error::KeySourceError;
use crate::crypto::Blake3Hash;

const KEY_FILE_SIZE: u64 = 32;
const MAX_SCAN_DEPTH: usize = 8;

/// Scans `mount_path` for a 32-byte file whose BLAKE3 hash matches `reference_hash`.
///
/// Returns the path of the matching file or `Ok(None)` if no match is found.
/// Per-entry errors (permission denied, unreadable junk files) are skipped.
/// A hard failure to read the top-level mount point is reported as
/// `KeySourceError::ReadFailed`.
pub async fn find_key_file(
    mount_path: &Path,
    reference_hash: &Blake3Hash,
) -> Result<Option<PathBuf>, KeySourceError> {
    let mount_path = mount_path.to_path_buf();
    let reference_hash = *reference_hash;
    tokio::task::spawn_blocking(move || scan_blocking(&mount_path, &reference_hash))
        .await
        .map_err(|join_error| {
            KeySourceError::ReadFailed(std::io::Error::other(join_error))
        })?
}

fn scan_blocking(
    mount_path: &Path,
    reference_hash: &Blake3Hash,
) -> Result<Option<PathBuf>, KeySourceError> {
    if !mount_path.exists() {
        return Err(KeySourceError::ReadFailed(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("mount path does not exist: {}", mount_path.display()),
        )));
    }

    let walker = WalkDir::new(mount_path)
        .follow_links(false)
        .max_depth(MAX_SCAN_DEPTH)
        .into_iter()
        .filter_entry(|entry| !is_system_dir(entry.file_name().to_string_lossy().as_ref()));

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
        let mut buffer = [0u8; 32];
        if file.read_exact(&mut buffer).is_err() {
            continue;
        }

        let hash = blake3::hash(&buffer);
        if hash.as_bytes() == &reference_hash.0 {
            return Ok(Some(entry.into_path()));
        }
    }
    Ok(None)
}

fn is_system_dir(name: &str) -> bool {
    matches!(
        name,
        "System Volume Information"
            | "$RECYCLE.BIN"
            | ".Trashes"
            | ".Spotlight-V100"
            | ".fseventsd"
    )
}
```

Reuses `crate::crypto::Blake3Hash` — already re-exported from `src-tauri/src/crypto/mod.rs`. No new imports needed in `crypto`.

Add `#[cfg(test)] mod tests { … }` covering:

- `test_find_key_file_matches_single_32_byte_file_at_root`.
- `test_find_key_file_matches_file_in_subdirectory` (depth 2).
- `test_find_key_file_ignores_non_32_byte_files` — mix 31, 32 (non-matching), 33 byte files.
- `test_find_key_file_returns_none_when_no_match`.
- `test_find_key_file_returns_none_when_mount_is_empty`.
- `test_find_key_file_returns_read_failed_when_mount_path_does_not_exist`.
- `test_find_key_file_finds_correct_file_among_many_32_byte_files` — three 32-byte files with different content, only one matches.
- `test_find_key_file_skips_system_directories` — plant a key file inside `System Volume Information` and confirm it is not returned (regression guard for the filter).

All tests use `tempfile::TempDir` to build a fake mount tree.

### 5.7 Implement `path_hint.rs`

**File**: `C:\Users\chris\source\repos\arx-runa\src-tauri\src\auth\path_hint.rs` (new)

```rust
//! Local JSON file that remembers the last-used key file path per vault.

use std::collections::BTreeMap;
use std::fs;
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::auth::error::KeySourceError;

const SCHEMA_VERSION: u32 = 1;

/// Per-vault last-used key file path, persisted outside SQLCipher so it is
/// readable before authentication.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultHint {
    /// Absolute path to the last-used key file on this machine.
    pub last_key_file_path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct HintFile {
    schema_version: u32,
    #[serde(default)]
    hints: BTreeMap<String, VaultHint>,
}

/// Reads and writes the key-hint JSON file at the platform-standard location.
#[derive(Debug, Clone)]
pub struct KeyHintStore {
    file_path: PathBuf,
}

impl KeyHintStore {
    /// Builds a store at the platform default location (~/.local/share/arx-runa/
    /// key-hint.json on Linux, %LOCALAPPDATA%\arx-runa\key-hint.json on Windows,
    /// ~/Library/Application Support/arx-runa/key-hint.json on macOS).
    pub fn default_location() -> Option<Self> {
        let base = dirs::data_local_dir()?;
        Some(Self {
            file_path: base.join("arx-runa").join("key-hint.json"),
        })
    }

    /// Builds a store at an explicit path — test only and power users.
    pub fn at_path(file_path: PathBuf) -> Self {
        Self { file_path }
    }

    /// Returns the configured hint-file path.
    pub fn path(&self) -> &Path {
        &self.file_path
    }

    /// Returns the hint for `vault_id` or `None` if the file is missing or the
    /// vault has never been seen.
    pub fn get(&self, vault_id: &str) -> Result<Option<VaultHint>, KeySourceError> {
        let contents = match fs::read_to_string(&self.file_path) {
            Ok(contents) => contents,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(KeySourceError::ReadFailed(error)),
        };
        let parsed: HintFile = match serde_json::from_str(&contents) {
            Ok(parsed) => parsed,
            Err(_) => return Ok(None),
        };
        Ok(parsed.hints.get(vault_id).cloned())
    }

    /// Writes a hint for `vault_id`, preserving entries for other vaults.
    pub fn set(&self, vault_id: &str, hint: VaultHint) -> Result<(), KeySourceError> {
        let mut file = match fs::read_to_string(&self.file_path) {
            Ok(contents) => serde_json::from_str::<HintFile>(&contents).unwrap_or_default(),
            Err(error) if error.kind() == ErrorKind::NotFound => HintFile::default(),
            Err(error) => return Err(KeySourceError::ReadFailed(error)),
        };
        file.schema_version = SCHEMA_VERSION;
        file.hints.insert(vault_id.to_owned(), hint);

        if let Some(parent) = self.file_path.parent() {
            fs::create_dir_all(parent).map_err(KeySourceError::ReadFailed)?;
        }

        let temporary = self.file_path.with_extension("json.tmp");
        let bytes =
            serde_json::to_vec_pretty(&file).map_err(|error| {
                KeySourceError::ReadFailed(std::io::Error::other(error))
            })?;
        {
            let mut handle = fs::File::create(&temporary).map_err(KeySourceError::ReadFailed)?;
            handle.write_all(&bytes).map_err(KeySourceError::ReadFailed)?;
            handle.sync_all().map_err(KeySourceError::ReadFailed)?;
        }
        fs::rename(&temporary, &self.file_path).map_err(KeySourceError::ReadFailed)?;
        Ok(())
    }
}
```

Tests under `#[cfg(test)]`:

- `test_key_hint_store_returns_none_when_file_missing` — fresh `TempDir`, no file written.
- `test_key_hint_store_roundtrips_single_vault_hint` — set then get.
- `test_key_hint_store_preserves_other_vaults_on_write` — set hint for `vault-a`, set hint for `vault-b`, confirm both readable.
- `test_key_hint_store_returns_none_on_corrupt_file` — write `"not json"` to the file, assert `get` returns `Ok(None)` (tolerant path).
- `test_key_hint_store_atomic_write_uses_tempfile` — after a successful write, assert `.json.tmp` no longer exists (renamed, not left behind).

### 5.8 Wire `auth::types` re-exports

**File**: `C:\Users\chris\source\repos\arx-runa\src-tauri\src\auth\types\mod.rs`

Replace the current body with:

```rust
//! Domain types for the auth module.
//!
//! Re-exports domain types defined alongside their owners so external callers
//! can import from a single stable path.

pub use crate::auth::device_monitor::DeviceEvent;
pub use crate::auth::error::KeySourceError;
pub use crate::auth::path_hint::VaultHint;
```

### 5.9 Pre-flight checks

Before starting coding:

1. `cargo check -p arx-runa-tauri` — baseline green.
2. `cargo clippy -- -D warnings` — baseline green.
3. `cargo test auth` — currently passes with zero tests; must stay green after each subsection.

After each file is added, re-run `cargo check` before touching the next file to localise failures.

## 6. Security Implications

### 6a. Expected sensitive path set

This plan anticipates touching the following files under `src-tauri/src/auth/` (sensitive per Section 6 policy):

- `src-tauri/src/auth/mod.rs` (extend)
- `src-tauri/src/auth/error.rs` (extend)
- `src-tauri/src/auth/key_source.rs` (new)
- `src-tauri/src/auth/device_monitor/mod.rs` (new, replaces `device_monitor.rs` from step 5.5)
- `src-tauri/src/auth/device_monitor/linux.rs` (new, target_os = "linux")
- `src-tauri/src/auth/device_monitor/windows_impl.rs` (new, target_os = "windows")
- `src-tauri/src/auth/device_monitor/macos.rs` (new, target_os = "macos")
- `src-tauri/src/auth/device_monitor/mock.rs` (new)
- `src-tauri/src/auth/autodetect.rs` (new)
- `src-tauri/src/auth/path_hint.rs` (new)
- `src-tauri/src/auth/types/mod.rs` (rewrite)

No files under `src-tauri/src/crypto/` or `src-tauri/src/storage/` are expected to change. `crypto::Blake3Hash` is consumed read-only.

### 6b. Invoke security-reviewer agent? **NO**

Rationale (independent review, not a mirror of the sub-phase self-assessment):

- No AEAD operations, no key derivation, no nonces, no Zeroization-on-error paths beyond the stack-allocated `Zeroizing<[u8; 32]>` read buffer (which is a `Drop` guarantee).
- BLAKE3 is used exclusively as a preimage-resistant fingerprint comparator. The reference hash in `key_file_blake3` is already designed to be public (vault header is plaintext JSON in the cloud per `design.md` line 62).
- The only `unsafe` code is in `device_monitor/windows_impl.rs` (`GetDriveTypeW` FFI call) and `device_monitor/macos.rs` (DiskArbitration framework calls). Both have narrow soundness arguments already captured in the `// SAFETY:` comments above. Neither touches key material.
- `/implement-plan` must still perform the drift check: if any file outside the list above lands under `src-tauri/src/auth/`, `src-tauri/src/crypto/`, or `src-tauri/src/storage/`, flag it as a Plan Deviation and escalate before merging.

### 6c. What an escalation would cover (if the drift check fires)

If implementation surfaces new sensitive-path changes the plan did not anticipate, the `security-reviewer` agent should check:

- Zeroization of the 32-byte read buffer on error paths (`read_exact` failure after partial read).
- Confirmation that `FileKeySource::path` never logs file contents, only the path string.
- Absence of any code that stores the returned `Zeroizing<[u8; 32]>` in a longer-lived field than its caller's stack scope.
- `unsafe` blocks justified by `// SAFETY:` comments with cited invariants.

## 7. Execution and testing strategy

### Test scope

- [x] Basic unit tests (written during implementation — listed per-file above).
- [ ] Adversarial tests (none — no crypto primitives in this sub-phase).
- [ ] Property-based tests (none — I/O and BLAKE3 fingerprint comparison are deterministic and small-input).
- [x] Integration tests — only `MockDeviceMonitor` + `MockKeySource` composition test at the end of the sub-phase (see below).
- [x] Boundary cases — file size 0, 31, 32 (match), 32 (non-match), 33; mount path empty, mount path missing, symlink loops (walkdir handles by skipping with `follow_links = false`), system directories.

**Coverage target**: >80 % on new files (auth is sensitive-path policy). Measure with `cargo llvm-cov -p arx-runa-tauri --summary-only` after implementation.

### Boundary cases to cover (consolidated)

- `FileKeySource`: 0-byte file, 31-byte file, 32-byte file, 33-byte file, non-existent path, unreadable path (Linux: `chmod 000`), symlink to valid 32-byte file (must work — `File::open` follows symlinks by default; document it).
- `find_key_file`: single match, multi-file tie (only one matches by hash), no match, depth-boundary (file at depth 8 is found, file at depth 9 is not), mount path missing, mount path empty, system-dir planted key is skipped.
- `KeyHintStore`: missing file → `Ok(None)`, corrupt file → `Ok(None)`, multi-vault preservation on write, atomic-rename leaves no `.tmp`.
- `MockDeviceMonitor`: FIFO ordering of pushed events, `watch()` called twice should panic (single-consumer contract).

### End-of-phase integration test

**File**: `C:\Users\chris\source\repos\arx-runa\src-tauri\src\auth\mod.rs` (under `#[cfg(test)] mod integration_tests { … }`)

```rust
#[tokio::test]
async fn test_autodetect_with_mock_device_monitor_finds_planted_key_file() {
    // 1. MockDeviceMonitor emits Mounted(TempDir::path).
    // 2. Consumer takes the mount_path from the event.
    // 3. Calls find_key_file with a known reference_hash.
    // 4. Asserts the returned path matches the planted key file.
}
```

### Validation checkpoint from sub-phase

- `cargo test auth::key_source` — all green.
- `cargo test auth::device_monitor` — all green (mock tests plus Linux unit tests if host is Linux).
- `cargo test auth::autodetect` — all green.
- `cargo test auth::path_hint` — all green.
- `cargo clippy -- -D warnings` — no new warnings on any target.
- `cargo check --target x86_64-pc-windows-msvc` (if available) and `--target x86_64-unknown-linux-gnu` — both green.

### Manual verification (delegated to reviewer, not CI)

Per sub-phase §Validation Checkpoint lines 44–48:

- Insert a USB drive on the host platform, observe the OS-specific monitor emits a `Mounted` event with the correct `mount_path`.
- Plant a 32-byte file on the drive, run an ad-hoc binary (or `cargo run --example find_key_file`) that calls `find_key_file` with the known hash, observe it returns the correct path.
- Plant a 33-byte file alongside it, observe it is ignored.

If manual verification is not feasible on a given platform during implementation, state so in the Handoff Notes rather than claiming success.

### Invoke test-writer agent? **NO**

Rationale: the tests listed above are direct unit tests over deterministic I/O and trait boundaries — they do not need adversarial crypto scenarios or proptest generators. `test-writer` is reserved for retroactive coverage of crypto modules (per `.claude/agents/test-writer.md`), not for plumbing.

### Test acceptance criteria

- Every `KeySourceError` variant has at least one test that triggers it (per `.claude/rules/rust.md` "Every `thiserror` variant must have a test that triggers it").
- All tests must use `#[cfg(test)]`-gated `unwrap`/`expect` only — no production `unwrap`/`expect` added.
- `cargo clippy -- -D warnings` across all three target triples (minimum: host) passes green.

## 8. Documentation Impact

1. **`docs/architecture/designs/authentication-and-session-management/design.md`** — add path-hint schema under the "Local path hint" paragraph (currently design.md line 73). Append a short JSON block matching DC-5's resolution. This is a genuine deviation from the pre-existing doc because the schema was previously only described in prose.
2. **`docs/architecture/designs/authentication-and-session-management/sub-phases/2.1-usb-key-file-and-device-monitor.md`** — add an "Implementation Decisions" note covering: `tokio-stream` choice (DC-1), bounded-32 channel (DC-2), `test-utils` feature flag (DC-3), `walkdir` depth 8 (DC-4), per-vault hint schema (DC-5), `DEVTYPE=partition` + `ID_BUS=usb` filter (DC-6), WMI `Win32_VolumeChangeEvent` + `DRIVE_REMOVABLE` (DC-7), DiskArbitration FFI layout (DC-8). These are deviations the plan commits to — record them so future planners do not rediscover the questions.
3. **`.claude/rules/auth.md`** and **`.github/instructions/auth.instructions.md`** — bump "last verified against design dated 2026-04-07" to `2026-04-12`. Metadata only; no guidance change. (See also Governance Sync Action G-1.)
4. **`docs/report-log/`** — append a short entry (≤ 10 lines) documenting that Phase 2.1 landed, the `test-utils` feature flag was added, and the `MacOsDeviceMonitor` stub status (if event translation is deferred).

No new architecture decision record is required — `003-sub-phase-roadmap-workflow.md` already governs sub-phase execution.

## 9. Governance sync actions (pre-implementation)

### G-1 — Bump auth rule verification date

- **Reason / linked concern**: DC-9 (metadata drift: rule and instruction files claim last-verified-against 2026-04-07, design.md dated 2026-04-12; no guidance contradiction).
- **Target files**:
  - `C:\Users\chris\source\repos\arx-runa\.claude\rules\auth.md`
  - `C:\Users\chris\source\repos\arx-runa\.github\instructions\auth.instructions.md`
- **Required edit**: replace the string `last verified against design dated 2026-04-07` with `last verified against design dated 2026-04-12` in both files. Do **not** otherwise modify the content of either file.
- **Verification**: `rg "last verified against" .claude/rules/auth.md .github/instructions/auth.instructions.md` must return both files with the new date and nothing else.
- **Note for `/implement-plan`**: after editing `.claude/rules/auth.md`, run `/copilot-sync` to reconfirm the instructions mirror is in sync. The mirror edit above is included so `/copilot-sync` is idempotent.

No other governance sync actions are required — `.claude/rules/auth.md`, `.claude/rules/rust.md`, `.claude/rules/memory-protection.md`, `.claude/reference/rust-patterns.md`, and the `.claude/agents/` set were all reviewed and contain no contradictions, stale guardrails, or outdated execution guidance for Phase 2.1.

`governance-sync-required: false` in the frontmatter because the only action is a metadata refresh, not a guidance change.

## 10. Handoff Notes for Implementer

Working directory: `C:\Users\chris\source\repos\arx-runa`. This plan is self-contained — you do **not** need to re-read the sub-phase to implement, but you must cross-check against `docs/architecture/designs/authentication-and-session-management/design.md` lines 50–103 and the Contract Surface at line 21 for any ambiguity. Run sections in order: 5.1 (Cargo.toml) → 5.2 (mod.rs) → 5.3 (error.rs) → 5.4 (key_source.rs) → 5.5 (device_monitor/ subtree; start with mock.rs + mod.rs, then add your host-OS monitor last) → 5.6 (autodetect.rs) → 5.7 (path_hint.rs) → 5.8 (types/mod.rs). Run `cargo check` after each file. Traps: (a) `MockKeySource` and `MockDeviceMonitor` must be gated behind `#[cfg(any(test, feature = "test-utils"))]` — forgetting the feature gate breaks Phase 2.2's tests; (b) the `device_monitor.rs` single file from step 5.5 becomes `device_monitor/mod.rs` because the monitors live in submodules — make the rename before adding per-OS files; (c) the `wmi` crate API name for the notification iterator may differ in 0.14 — verify against `cargo doc -p wmi` before committing; (d) the macOS monitor is allowed to ship as a compiling stub that registers callbacks but does not yet translate `DADiskCopyDescription` — document the gap in the follow-up report-log entry rather than blocking on full DiskArbitration bindings; (e) `windows_impl.rs` is named with the `_impl` suffix to avoid clashing with the `windows` crate name. Do not rename it to `windows.rs`. Platform-specific code paths are gated by `#[cfg(target_os = "…")]`; the mock is the only monitor that compiles on all targets, so all tests run via the mock to avoid platform skew. If the drift check at verify time flags any file outside Section 6a's expected set under a sensitive path, escalate to the user before merging — do not silently accept.
