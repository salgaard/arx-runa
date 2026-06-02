# Development Setup

This guide walks through a complete first-time setup. Follow all steps in order for your platform before attempting to build or run tests.

---

## Prerequisites

### Windows 10/11

1. **Visual Studio Build Tools** — required by the Rust MSVC toolchain.
   Download [Visual Studio 2022 Build Tools](https://visualstudio.microsoft.com/downloads/#build-tools-for-visual-studio-2022)
   and select the **Desktop development with C++** workload during install.

2. **Rust (MSVC toolchain)**
   ```powershell
   winget install Rustlang.Rustup
   rustup toolchain install stable-x86_64-pc-windows-msvc
   rustup default stable-x86_64-pc-windows-msvc
   rustup target add wasm32-unknown-unknown
   ```

3. **Strawberry Perl** — required at compile time for vendored OpenSSL inside `src-tauri`. This is a build-time dependency only; end users do not need it.
   ```powershell
   winget install StrawberryPerl.StrawberryPerl
   ```
   Restart your terminal after install so `perl` is on `PATH`. Verify: `perl -v`.

4. **Node.js** v18 or later — the Trunk pre-build hook invokes `node` to compile Tailwind CSS v4:
   ```powershell
   winget install OpenJS.NodeJS.LTS
   ```

5. **Trunk** (Leptos/WASM bundler):
   ```powershell
   cargo install trunk --locked
   ```

6. **jq** — required by Claude Code hooks:
   ```powershell
   winget install jqlang.jq
   ```

7. **VS Code** with recommended extensions.
   Open the project — VS Code will prompt you to install from `.vscode/extensions.json`:
   - `rust-lang.rust-analyzer` — language server, inline hints, completions
   - `ms-vscode.cpptools` — Windows backend debugger (`cppvsdbg`)
   - `tauri-apps.tauri-vscode` — Tauri project support

---

### macOS

1. **Xcode Command Line Tools**:
   ```bash
   xcode-select --install
   ```

2. **Rust**:
   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   source "$HOME/.cargo/env"
   rustup target add wasm32-unknown-unknown
   ```

3. **Node.js** v18 or later — via [Homebrew](https://brew.sh/) or [nvm](https://github.com/nvm-sh/nvm):
   ```bash
   brew install node
   ```

4. **Trunk**:
   ```bash
   cargo install trunk --locked
   ```

5. **jq**:
   ```bash
   brew install jq
   ```

6. **VS Code** with recommended extensions.
   Open the project — VS Code will prompt you to install from `.vscode/extensions.json`.
   Use `vadimcn.vscode-lldb` for backend debugging instead of `ms-vscode.cpptools`.

---

### Linux (Debian / Ubuntu)

Tauri requires several system libraries. Install them first:

```bash
sudo apt update
sudo apt install -y \
  build-essential curl git \
  libssl-dev pkg-config \
  libgtk-3-dev libwebkit2gtk-4.1-dev \
  libayatana-appindicator3-dev librsvg2-dev \
  jq
```

Then:

1. **Rust**:
   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   source "$HOME/.cargo/env"
   rustup target add wasm32-unknown-unknown
   ```

2. **Node.js** v18 or later:
   ```bash
   curl -fsSL https://deb.nodesource.com/setup_lts.x | sudo -E bash -
   sudo apt install -y nodejs
   ```

3. **Trunk**:
   ```bash
   cargo install trunk --locked
   ```

4. **VS Code** with recommended extensions.
   Open the project — VS Code will prompt you to install from `.vscode/extensions.json`.

---

## First-time setup after cloning

```bash
npm install
```

Populates `node_modules/` with `tailwindcss` and `@tailwindcss/cli`. Required before the first
`cargo tauri dev` or `cargo build --release`. `node_modules/` is gitignored.

---

## Rclone sidecar binaries

Arx Runa bundles rclone as a Tauri sidecar for cloud transport. The binaries are **not included
in git** (too large). You must download and place them in `src-tauri/bin/` before building.

### Download and rename

Go to [rclone.org/downloads](https://rclone.org/downloads/) and download the archive for your
platform. For local development you only need your host platform's binary; building release
bundles for all platforms requires all five.

| Platform | Archive to download | Required filename in `src-tauri/bin/` |
|---|---|---|
| Windows x64 | `rclone-*-windows-amd64.zip` | `rclone-x86_64-pc-windows-msvc.exe` |
| Linux x64 | `rclone-*-linux-amd64.zip` | `rclone-x86_64-unknown-linux-gnu` |
| Linux ARM64 | `rclone-*-linux-arm64.zip` | `rclone-aarch64-unknown-linux-gnu` |
| macOS x64 | `rclone-*-osx-amd64.zip` | `rclone-x86_64-apple-darwin` |
| macOS Apple Silicon | `rclone-*-osx-arm64.zip` | `rclone-aarch64-apple-darwin` |

The filename **must match exactly** — Tauri's sidecar loader resolves binaries by target triple.

### Windows example

1. Download `rclone-*-windows-amd64.zip` and extract it.
2. Inside the extracted folder, find `rclone.exe`.
3. Rename it to `rclone-x86_64-pc-windows-msvc.exe`.
4. Move it into `src-tauri/bin/`.

### macOS / Linux

After copying, mark the binary executable:
```bash
chmod +x src-tauri/bin/rclone-*
```

### How the binary is located at runtime

`rclone_binary_path` (`src-tauri/src/ui/commands_common.rs`) resolves the sidecar in this
order: **next to the running executable** (where Tauri installs an `externalBin` sidecar in
production, with the target-triple suffix stripped), then the app resource directory, then the
system `PATH`. In development the binary isn't bundled next to the debug executable, so the app
falls back to `PATH` — cloud features under `cargo tauri dev` therefore need rclone on `PATH`
(or run a built bundle). A local vault never spawns rclone at creation time, which is why local
vaults can be created even when rclone is missing while cloud vaults cannot.

---

## Cloud integration tests (`.env.test`)

The real-cloud integration tests (`cargo test -p arx-runa-tauri -- --ignored`) require a
`.env.test` file in the project root. This file is **not in git**. Create it:

```env
ARX_TEST_B2_KEY_ID=
ARX_TEST_B2_APP_KEY=
ARX_TEST_B2_BUCKET=arx-runa-test
ARX_TEST_GDRIVE_REFRESH_TOKEN=
ARX_TEST_ONEDRIVE_REFRESH_TOKEN=
ARX_TEST_ONEDRIVE_DRIVE_ID=
```

### Backblaze B2

1. Log in to [secure.backblaze.com](https://secure.backblaze.com/b2_buckets.htm).
2. Create a bucket named `arx-runa-test` (Private, no server-side encryption).
3. Go to **App Keys** → **Add a New Application Key**. Scope it to that bucket with
   read/write/delete permissions.
4. Copy the **keyID** → `ARX_TEST_B2_KEY_ID` and the **applicationKey** → `ARX_TEST_B2_APP_KEY`.

### Google Drive

The refresh token is obtained by running an OAuth flow through rclone. Use the **sidecar binary
directly** — it is not in `PATH`:

**Windows:**
```powershell
.\src-tauri\bin\rclone-x86_64-pc-windows-msvc.exe config
```

**macOS (Apple Silicon):**
```bash
./src-tauri/bin/rclone-aarch64-apple-darwin config
```

**macOS (Intel):**
```bash
./src-tauri/bin/rclone-x86_64-apple-darwin config
```

**Linux x64:**
```bash
./src-tauri/bin/rclone-x86_64-unknown-linux-gnu config
```

In the interactive config wizard:

1. Choose **n** → new remote. Name it anything (e.g. `gdrive-test`).
2. Choose **Google Drive** from the storage type list.
3. Leave Client ID and Secret blank (uses rclone's built-in credentials).
4. Follow the browser OAuth flow.
5. When done, open the rclone config file and find the `[gdrive-test]` section:
   - Windows: `%APPDATA%\rclone\rclone.conf`
   - macOS / Linux: `~/.config/rclone/rclone.conf`
6. Copy the `"refresh_token"` value from inside the `token = {...}` JSON → `ARX_TEST_GDRIVE_REFRESH_TOKEN`.

### OneDrive

Repeat the same rclone config flow, choosing **Microsoft OneDrive** as the storage type.
Select the correct drive type (personal OneDrive or business/SharePoint) when prompted.

After the OAuth flow, in `rclone.conf` find the `[onedrive-test]` section and copy:
- `token` → `"refresh_token"` value → `ARX_TEST_ONEDRIVE_REFRESH_TOKEN`
- `drive_id` value → `ARX_TEST_ONEDRIVE_DRIVE_ID`

---

## 1. Cargo workflow

```bash
# Build
cargo build

# Run full app (frontend + backend)
cargo tauri dev

# Production bundle
cargo tauri build

# Frontend only (Leptos/WASM via Trunk)
trunk serve

# Run tests
cargo test

# Backend tests only
cargo test -p arx-runa-tauri

# Lint (enforced on all .rs edits via PostToolUse hook)
cargo clippy -- -D warnings

# Format
cargo fmt

# Security audit
cargo audit
```

Install `cargo-audit` if not present:

```bash
cargo install cargo-audit
```

---

### e2e tests (Zero-Trace frontend verification)

The e2e tests in `src-tauri/tests/e2e/` run the real built app under WebdriverIO + tauri-driver and verify that the frontend leaves no sensitive traces in `localStorage`, `sessionStorage`, the DOM, or the URL after the vault locks.

**One-time setup:**

1. Install `tauri-driver`:
   ```bash
   cargo install tauri-driver
   ```

2. Install **Microsoft Edge WebDriver** (Windows only). Use `msedgedriver-tool` to fetch the version that matches your installed Edge automatically:
   ```powershell
   cargo install --git https://github.com/chippers/msedgedriver-tool
   & "$HOME\.cargo\bin\msedgedriver-tool.exe"
   Move-Item msedgedriver.exe "$HOME\.cargo\bin\msedgedriver.exe"
   ```
   On macOS/Linux, `tauri-driver` uses the system WebKit WebDriver automatically (installed alongside `webkit2gtk`).

3. Install npm test dependencies:
   ```bash
   cd src-tauri/tests/e2e && npm install
   ```

**Running the tests:**

```bash
# First run: npm test builds the app automatically (cargo tauri build --debug --no-bundle).
# This takes a few minutes. Subsequent runs can skip the build:
cd src-tauri/tests/e2e && npm test

# Skip the build if you already ran `cargo tauri build --debug --no-bundle`:
E2E_SKIP_BUILD=1 npm test             # bash / macOS / Linux
$env:E2E_SKIP_BUILD=1; npm test       # PowerShell
```

> **Important:** Do **not** set `E2E_SKIP_BUILD=1` after a plain `cargo build` (the VS Code debug workflow via F5). That binary uses `devUrl: http://localhost:1420` at runtime and shows a blank page when Trunk is not serving. Only `cargo tauri build` embeds the frontend WASM bundle into the binary.

The tests take ~30–60 seconds once the app is built; the app window will open and close automatically.

> **Note:** `cargo build --manifest-path src-tauri/Cargo.toml` alone is not sufficient — it produces a backend-only binary without the frontend WASM bundle embedded, causing the app to crash on startup. `cargo tauri build` (run automatically by `npm test`) compiles the Leptos frontend via Trunk first.

---

### Tailwind CSS v4

Tailwind v4 has no `tailwind.config.js`. The brand token palette (iron, stone,
steel, rune, bone) is declared in `input.css` inside an `@theme` block. Source:
`docs/arx-runa-brand.css`. The Trunk pre-build hook (`Trunk.toml`) compiles
`input.css` → `output.css` on every build.

### cargo bench — performance benchmarks

Benchmarks live in `src-tauri/benches/crypto_benchmarks.rs` and are driven by [Criterion.rs](https://bheisler.github.io/criterion.rs/book/).

```bash
cd src-tauri
cargo bench
```

Two benchmark groups are defined:

| Benchmark | What it measures |
|---|---|
| `argon2id_kdf` | Vault-unlock latency at production parameters (`m=65536 KiB, t=3, p=4`) |
| `xchacha20_poly1305` | Chunk encrypt/decrypt throughput at 512 KiB and 4 MiB |

Criterion writes HTML reports to `src-tauri/target/criterion/`. Open `target/criterion/report/index.html` to browse them. To compare two runs (e.g. before/after a change), install `critcmp`:

```bash
cargo install critcmp
cargo bench -- --save-baseline before
# make your change
cargo bench -- --save-baseline after
critcmp before after
```

The measured results are documented in Bilag C of the bachelor report. The key finding: XChaCha20-Poly1305 throughput is ~989 MiB/s for encryption and ~825 MiB/s for decryption on a modern desktop; the CPU is not the bottleneck for cloud backup workloads.

---

### cargo fuzz — coverage-guided fuzzing

Fuzz targets live in `src-tauri/fuzz/fuzz_targets/`. They exercise the three parsing entry points that process untrusted cloud data.

| Target | What it fuzzes |
|---|---|
| `fuzz_vault_header` | JSON deserialization + structural validation of `VaultHeader` |
| `fuzz_manifest_backup` | Wire-format parsing of manifest backup blob (`[nonce\|ct\|tag]`) |
| `fuzz_parse_chunk_size` | String-to-u64 validation of `chunk_size_bytes` |

**Prerequisites:** cargo-fuzz uses libFuzzer, which is only available on **Linux and macOS**. On Windows, use WSL or run via CI.

```bash
# Install cargo-fuzz (requires nightly — one-time)
cargo install cargo-fuzz

# Run a target until interrupted (fuzzes indefinitely)
cd src-tauri
cargo +nightly fuzz run fuzz_vault_header

# Smoke test: run for 60 seconds then exit
cargo +nightly fuzz run fuzz_vault_header -- -max_total_time=60

# List all targets
cargo +nightly fuzz list
```

Corpus inputs accumulate in `src-tauri/fuzz/corpus/<target>/` and are checked into git so future runs start from known-interesting inputs. Crash artefacts land in `src-tauri/fuzz/artifacts/<target>/` (git-ignored).

The `fuzzing` Cargo feature exposes `pub(crate)` functions to the external fuzz crate. It must never be enabled in production builds.

---

### cargo geiger — unsafe block audit

`cargo geiger` maps every `unsafe` block in the compiled dependency tree, distinguishing between used and unused unsafe code per crate.

```bash
# Install (one-time)
cargo install cargo-geiger

# Run against the backend crate
cd src-tauri
cargo geiger
```

Expected output for Arx Runa:

- `arx-runa-tauri` is marked `!` (unsafe used) — all unsafe is concentrated in `src-tauri/src/memory/` where `mlock` (Unix) and `VirtualLock` (Windows) lock key material in RAM. Every `unsafe` block carries a `// SAFETY:` comment.
- **No unsafe** in `crypto/`, `auth/kdf.rs`, or `sharing/` — the cryptographic core is safe Rust throughout.

Run `cargo geiger` after adding any dependency to verify no new unsafe surfaces have been introduced without review.

---

## 2. Debugging

Set breakpoints in `src-tauri/src/**`, then press `F5` or open **Run and Debug** and select
**`[Debug] Backend core (Windows)`**. This builds the binary via `build:tauri-debug`, then
launches it under `cppvsdbg` with `RUST_LOG=debug` and `RUST_BACKTRACE=1`.

Other profiles in `.vscode/launch.json`:

- **`[Run] Tauri dev (no debugger)`** — full app via `cargo tauri dev`, no debugger attached.
- **`[Run] Frontend dev server (trunk)`** — frontend only (`trunk serve --port 1420`).

---

## 3. AI Assistant Setup (Optional)

The following MCP servers and LSP integrations are used with Claude Code in this project.

### rust-analyzer (LSP)

Provides Rust language intelligence. Install the component once:

```bash
rustup component add rust-analyzer
```

Used by:
- **VS Code** — via the `rust-lang.rust-analyzer` extension (prompted on project open)
- **GitHub Copilot** — LSP config in `.github/lsp.json` wires it to `.rs` files
- **Claude Code** — picked up automatically via the VS Code extension

### jCodemunch / jDocmunch MCP

[github.com/jgravelle/jcodemunch-mcp](https://github.com/jgravelle/jcodemunch-mcp)

Two MCP servers bundled together:
- **jcodemunch** — code symbol search, call hierarchy, blast radius, dead code, dependency graphs
- **jdocmunch** — markdown doc search and section retrieval

Both are configured in `CLAUDE.md` as the preferred navigation tools. They significantly reduce
token usage by returning targeted excerpts rather than full file reads.

#### Indexing

Both servers maintain a local index that must be kept in sync with the repository. Use the
following phrases when talking to Claude to trigger re-indexing:

| What you say | What Claude calls | When to use |
|---|---|---|
| `index jcodemunch` | `resolve_repo` → `index_folder` on the repo root | After adding/removing source files or pulling large changes |
| `index jdocmunch` | `index_local` on `docs/` (incremental by default) | After adding or editing markdown docs |
| `force re-index jcodemunch` | `index_folder` with `incremental: false` | When incremental misses changes (e.g. stale mtime cache) |
| `force re-index jdocmunch` | `index_local` with `incremental: false` | Same — forces all files to be re-parsed |

**Incremental vs full:**
- **Incremental** (default) — only re-indexes files whose mtime changed. Fast, but can miss files if the cache is stale.
- **Full** — re-parses every file. Use after `git pull` with many changes or if search results feel wrong.

The jdocmunch index covers `docs/` only (162 `.md` files, ~3 800 sections). jcodemunch covers the full
repository source tree.

### context7 MCP

Fetches up-to-date library documentation on demand (Tauri, Leptos, tokio, etc.) so Claude
doesn't rely on potentially stale training data for API signatures.

### RTK

[github.com/rtk-ai/rtk](https://github.com/rtk-ai/rtk) — reasoning toolkit for AI agents.

---

## Building release installers locally

Use this when you need to test the packaged installer without going through GitHub Actions.

### Prerequisites

Ensure you have completed the [First-time setup](#first-time-setup-after-cloning) and have the rclone sidecar in place (see [Rclone sidecar binaries](#rclone-sidecar-binaries)).

If you haven't installed the Tauri CLI yet:

```powershell
cargo install tauri-cli --locked
```

### Windows — one-liner rclone sidecar download

If you haven't downloaded the sidecar yet, this fetches and places it automatically:

```powershell
$ver = "v1.68.2"
Invoke-WebRequest "https://github.com/rclone/rclone/releases/download/$ver/rclone-$ver-windows-amd64.zip" -OutFile rclone.zip
Expand-Archive rclone.zip -DestinationPath rclone-tmp
Copy-Item (Get-ChildItem rclone-tmp -Recurse -Filter rclone.exe | Select-Object -First 1).FullName src-tauri\bin\rclone-x86_64-pc-windows-msvc.exe
Remove-Item rclone.zip, rclone-tmp -Recurse
```

### Build

`createUpdaterArtifacts` is enabled, so a full bundle build signs the updater manifest and
therefore needs the updater **private** key in the environment. Without it, `cargo tauri build`
fails with *"A public key has been found, but no private key"*.

```powershell
$env:TAURI_SIGNING_PRIVATE_KEY = Get-Content -Raw src-tauri\arxruna-updater.key
$env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = "<password, or '' if none>"
cargo tauri build
```

This compiles the Leptos frontend via Trunk, embeds the WASM bundle, and produces the installers
plus the updater artifacts (`latest.json` + `.sig`).

Notes:
- This is the **updater** signature (proves an update came from us), **not** OS code signing.
  Installers are unsigned, so Windows SmartScreen / macOS Gatekeeper warnings appear on first install.
- Contributors who only need to test the installer (not the update flow) can generate a throwaway
  key with `cargo tauri signer generate -w throwaway.key` and point the env vars at it.
- Builds that skip bundling (`cargo tauri build --no-bundle`, used by the e2e suite) do not need the key.

**Output locations:**

| Format | Path |
|---|---|
| NSIS installer (`.exe`) | `src-tauri\target\release\bundle\nsis\` |
| MSI installer (`.msi`) | `src-tauri\target\release\bundle\msi\` |
| macOS disk image (`.dmg`) | `target/universal-apple-darwin/release/bundle/dmg/` |
| Linux AppImage | `src-tauri/target/release/bundle/appimage/` |
| Linux `.deb` | `src-tauri/target/release/bundle/deb/` |

---

## Releases & versioning

Releases are built and published by `.github/workflows/release.yml`, triggered by pushing a
`v*` tag. The workflow builds per-OS bundles, signs the updater manifest, and publishes the
artifacts to the public **`salgaard/arx-runa-releases`** repo.

### Version is single-sourced

The application version lives in **one place**: `[workspace.package].version` in the root
`Cargo.toml`. Both crates inherit it via `version.workspace = true`, and Tauri reads the resolved
`arx-runa-tauri` version — `tauri.conf.json` intentionally has **no** `version` field. The in-app
version and the auto-updater both come from this build version, **not** from the git tag (the tag
only triggers the workflow and names the release).

Per release:

1. Bump the version in the root `Cargo.toml` (`[workspace.package]` → `version`), e.g. `0.1.0` → `0.1.1`.
2. Commit.
3. Tag with the **matching** version and push:
   ```bash
   git tag v0.1.1
   git push origin v0.1.1
   ```

Rules:
- Tag and workspace version must match (`vX.Y.Z` ↔ `version = "X.Y.Z"`).
- The version must strictly increase (semver) or the updater won't offer the update — a tag bump
  alone, with the version unchanged, ships a "release" that no installed app recognizes as newer.

Re-tagging (delete then re-push):
```bash
git tag -d v0.1.0
git push origin --delete v0.1.0
git tag v0.1.0
git push origin v0.1.0
```

### Auto-updater

On startup the app checks for an update and offers to install it via a native dialog
(`spawn_update_check` in `src-tauri/src/lib.rs`). The check is best-effort — any failure is logged
at `warn!` and never blocks launch.

**One-time setup (maintainer):**

1. Generate the updater keypair (free — this is an *update* signature, not OS code signing):
   ```
   cargo tauri signer generate -w src-tauri/arxruna-updater.key
   ```
2. Paste the printed public key into `tauri.conf.json` → `plugins.updater.pubkey`.
3. Add GitHub Actions secrets (Settings → Secrets → Actions):
   - `TAURI_SIGNING_PRIVATE_KEY` — full contents of `arxruna-updater.key`
   - `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` — the key password (empty string if none)
4. Back up the private key (e.g. password manager). **Losing it means existing installs can never
   be updated again.** `*.key` is gitignored — never commit it.

**How updates reach users:** the build emits `latest.json` + `.sig` files. The updater endpoint is
`…/arx-runa-releases/releases/latest/download/latest.json`. Because `tauri-action` writes
`latest.json` with URLs pointing at the *build* repo, `release.yml` rewrites them to the public
releases repo before uploading (the `sed` step in "Publish artifacts to public releases repo").
After the first release, verify the URLs resolve to public, downloadable assets:
```bash
curl -sL https://github.com/salgaard/arx-runa-releases/releases/latest/download/latest.json
```

### Installer behaviour (NSIS)

- `installMode: currentUser` — installs under `%LOCALAPPDATA%`, no admin/UAC prompt.
- No OS code signing → SmartScreen ("Windows protected your PC") on first install and during the
  updater's installer run. Document the *More info → Run anyway* click-through for end users.

### CI action pinning

All GitHub Actions across `.github/workflows/` are pinned to commit SHAs (with a trailing
`# version` comment) so a mutable tag can't silently swap the code that runs with repo secrets —
the secret-bearing `release.yml` and the credential-bearing `mirror.yml` in particular.
`.github/dependabot.yml` opens weekly grouped PRs to bump these SHAs; review and merge to adopt a
new version deliberately. To pin a new action or re-pin by hand, resolve the SHA:
```bash
gh api repos/<owner>/<repo>/commits/<tag> --jq .sha
```

## 4. Encryption stack (for context)

| Component | Technology |
|---|---|
| Encryption | XChaCha20-Poly1305 via `chacha20poly1305` crate |
| KDF | Argon2id → HKDF-SHA256 (three derived keys) |
| MFA | USB key file (32 bytes random entropy) |
| Local DB | SQLite + SQLCipher |
| Cloud transport | Rclone |

See [System Architecture](../architecture/) for full design rationale.
