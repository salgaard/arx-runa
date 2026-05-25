# Test Taxonomy

Arx Runa uses four distinct test layers. Each has a clear scope, toolchain, and access level.

---

## 1. Unit tests

**Location:** In-file `#[cfg(test)]` modules (e.g. `src-tauri/src/auth/ceremonies/change_password.rs`)

**Toolchain:** Cargo / `#[tokio::test(flavor = "multi_thread")]`

**What they test:** A single function or ceremony in isolation. Full access to private items (`super::`, `pub(crate)`) since they compile as part of the same module.

**What they don't do:** Compose ceremonies together or touch the storage/transport layer.

---

## 2. Scenario tests

**Location:** `src-tauri/src/tests/` (`scenarios_auth.rs`, `scenarios_backup.rs`, `scenarios_sync.rs`, `scenarios_sharing.rs`, `scenarios_destinations.rs`, `scenarios_real_cloud.rs`)

**Toolchain:** Cargo (intra-crate module — `mod tests` inside the library crate)

**What they test:** Cross-ceremony flows that can't be captured by individual unit tests — e.g. create vault → add recovery phrase → lock → recover → assert session active. Use real crypto, real SQLCipher, real KDF. Cloud transport is mocked (`MockCloudTransport`).

**Access level:** `pub(crate)` and below — they can reach internal APIs but not private fields.

**Notable:** Organised by use case (UC1 personal backup, UC2 cross-device sync, UC3 auth/recovery, etc.).

---

## 3. Integration tests

**Location:** `src-tauri/tests/integration_cloud_sync.rs`, `src-tauri/tests/rclone_integration.rs`

**Toolchain:** Cargo integration test crate (separate compilation unit, auto-discovered by Cargo from `tests/*.rs`)

**What they test:**
- `integration_cloud_sync.rs` — full encrypt → upload → download → decrypt round trip through the storage stack with real filesystem I/O and real crypto. Also tests concurrent uploads, empty files, multi-chunk files, and corruption detection.
- `rclone_integration.rs` — real `RcloneTransport` upload/list/download/delete against a local rclone remote. Gated behind `ARX_RCLONE_INTEGRATION=1` so it is skipped in normal CI.

**Access level:** Public API only (`pub` items of `arx_runa_tauri_lib`). No access to crate internals.

**Note:** These files live alongside the `e2e/` subdirectory in `src-tauri/tests/` but are unrelated — Cargo owns the `.rs` files; Node.js owns the `e2e/` subtree.

---

## 4. E2E tests

**Location:** `src-tauri/tests/e2e/`

**Toolchain:** WebdriverIO + tauri-driver (Node.js). Run with `npm test` from the `e2e/` directory. Requires the app to be built with `cargo tauri build --debug --no-bundle` (or set `E2E_SKIP_BUILD=1`). On Linux CI, wrap with `xvfb-run`.

**What they test:** The real compiled Tauri app driven via `data-testid` selectors through WebDriver. Covers:
- `file_operations.spec.js` — vault file browser UI state (upload visibility, adversarial auth scenarios)
- `zero_trace.spec.js` — verifies no sensitive data (localStorage, sessionStorage, DOM) survives a vault lock
- `loading_states.spec.js` — loading/spinner states during slow operations
- `video_stream.spec.js` — video stream UI behaviour

**What they don't test:** Cryptographic correctness — that is covered by the integration tests. These focus on what the user sees.

**Constraints:** Vault creation uses Tier 1 (password only) because the key-file file-picker dialog cannot be automated via WebDriver.

---

## Summary

| Layer       | Location                          | Transport  | Access level   |
|-------------|-----------------------------------|------------|----------------|
| Unit        | In-file `#[cfg(test)]`            | —          | Private        |
| Scenario    | `src-tauri/src/tests/`            | Mocked     | `pub(crate)`   |
| Integration | `src-tauri/tests/*.rs`            | Real I/O   | `pub` only     |
| E2E         | `src-tauri/tests/e2e/`            | Real app   | UI only        |
