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

### context7 MCP

Fetches up-to-date library documentation on demand (Tauri, Leptos, tokio, etc.) so Claude
doesn't rely on potentially stale training data for API signatures.

### RTK

[github.com/rtk-ai/rtk](https://github.com/rtk-ai/rtk) — reasoning toolkit for AI agents.

---

## Pushing releases

### Delete a tag
git tag -d v0.1.0                                                                                                                          
git push origin --delete v0.1.0

### Push new tag
git tag v0.1.0
git push origin v0.1.0

## 4. Encryption stack (for context)

| Component | Technology |
|---|---|
| Encryption | XChaCha20-Poly1305 via `chacha20poly1305` crate |
| KDF | Argon2id → HKDF-SHA256 (three derived keys) |
| MFA | USB key file (32 bytes random entropy) |
| Local DB | SQLite + SQLCipher |
| Cloud transport | Rclone |

See [System Architecture](../architecture/) for full design rationale.
