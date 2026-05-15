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

### Tailwind CSS v4

Tailwind v4 has no `tailwind.config.js`. The brand token palette (iron, stone,
steel, rune, bone) is declared in `input.css` inside an `@theme` block. Source:
`docs/arx-runa-brand.css`. The Trunk pre-build hook (`Trunk.toml`) compiles
`input.css` → `output.css` on every build.

---

## 2. Debugging

1. Set backend breakpoints in `src-tauri/src/**` (for example `src-tauri/src/lib.rs`).
2. Press `F5` or open **Run and Debug** in the VS Code sidebar.
3. Select **"[One-click] Debug UI + Backend (Windows)"**.

Profiles in `.vscode/launch.json`:

- **`[One-click] Debug UI + Backend (Windows)`** — starts Trunk UI server and attaches backend debugger.
- **`[Run] Tauri dev (no debugger)`** — runs full app without a debugger.
- **`[Debug] Backend core (Windows)`** — backend-only debugger session.

If you place breakpoints in `src/app.rs` or `src/main.rs`, VS Code may show
`No executable code ...` warnings during backend debugging. Those files are
frontend Rust compiled to WASM, not native backend code.

---

## 3. Git hooks (documentation stubs)

A post-commit hook automatically creates a report-log stub whenever a commit
touches `src-tauri/src/`. Configure it once per clone:

```bash
git config core.hooksPath .githooks
chmod +x .githooks/post-commit
```

The hook writes a stub to `docs/report-log/` and stages it for the next
commit. Expand the stub with `/report-note <topic>` or delete it if the
commit does not warrant a report entry.

To skip the hook for a specific commit, include `[skip-docs]` in the
commit message.

---

## 4. Documentation SSOT Architecture

Arx Runa uses a **Single Source of Truth (SSOT)** documentation architecture. Technical specifications appear once in canonical design documents and are referenced elsewhere.

### When changing technical constraints:

1. **Design first**: Update canonical design document in `docs/architecture/designs/`
2. **Update rule summary** (if needed): Update `.claude/rules/*.md`
3. **Sync**: Run `/copilot-sync` to update GitHub Copilot instructions
4. **Commit**: All changes together

### Key documents:

- **Design specifications**: `docs/architecture/designs/<design-name>/design.md` — authoritative source
- **AI rules**: `.claude/rules/*.md` — brief summaries referencing designs
- **Roadmap**: `docs/roadmap.md` — references designs, contains implementation logistics
- **CLAUDE.md**: High-level principles and tech stack (not parameter-level details)

**See**: `docs/guides/documentation-ssot.md` for complete workflow and architecture details.

---

## 5. Working with Sub-Phase Roadmaps

For large design documents (>100 lines or logically separable), Arx Runa uses **sub-phase roadmaps** to decompose implementation into independently testable units with manual validation checkpoints.

### When to Use Sub-Phase Roadmaps

Use a sub-phase roadmap when a design document exhibits:

- **Size**: Exceeds ~100-150 lines
- **Trait boundaries**: Multiple trait definitions implementable independently
- **Platform splits**: OS-specific implementations (Windows/Linux)
- **Integration breadth**: Touches 3+ existing modules
- **Multiple flows**: Contains 3+ distinct operational flows

### Workflow

#### Step 1: Design Phase
```bash
/design phase-4
# Produces: docs/architecture/designs/cloud-synchronisation/design.md (722 lines)
```

#### Step 2: Assess for Decomposition
Read the design document and evaluate against the criteria above. If warranted, create a sub-phase roadmap.

#### Step 3: Create Sub-Phase Roadmap
Use the template in `docs/architecture/designs/_templates/sub-phase-roadmap-template.md`:

```bash
# Create sub-phases directory in the design folder
mkdir -p docs/architecture/designs/cloud-synchronisation/sub-phases

# Copy template and fill in sections
cp docs/architecture/designs/_templates/sub-phase-roadmap-template.md \
   docs/architecture/designs/cloud-synchronisation/sub-phases/roadmap.md
# Edit to decompose the design into 3-5 sub-phases
```

**Decomposition principles**:
- **Dependency-first ordering**: Sub-phases must follow internal dependency chains (trait before impl)
- **Test isolation**: Each sub-phase should be testable without later sub-phases (use mocks)
- **Clear checkpoints**: Each sub-phase ends with a validation gate (`cargo test X passes`, manual verification)
- **3-5 sub-phase target**: Avoid over-decomposition (too granular) or under-decomposition (still too large)

#### Step 4: Plan and Implement Sub-Phases
```bash
/plan 4.1  # Generates plan for Phase 4.1 only
/implement-plan phase-004-1-cloud-transport.md
# [Manual testing checkpoint — verify deliverables work as specified]

/plan 4.2  # Generates plan for Phase 4.2 only
/implement-plan phase-004-2-rclone-integration.md
# [Manual testing checkpoint]

# Continue for remaining sub-phases...
```

#### Step 5: Update Roadmap Reference
Add a line to `docs/roadmap.md` in the relevant phase block:
```markdown
**Sub-phase roadmap**: `docs/architecture/designs/cloud-synchronisation/sub-phases/roadmap.md`
```

### Example

Phase 4 (Cloud Synchronisation, 722 lines) is decomposed into:
- **Phase 4.1**: CloudTransport trait + MockTransport (~150 lines)
- **Phase 4.2**: Rclone integration + provider setup (~350 lines)
- **Phase 4.3**: Vault header upload/download (~120 lines)
- **Phase 4.4**: Manifest cloud backup (~150 lines)
- **Phase 4.5**: Push/pull flows + conflict detection (~400 lines)

Each sub-phase is independently testable, with clear validation checkpoints before proceeding to the next.

### Benefits

- **Incremental validation**: Catch bugs early before they compound
- **Reduced cognitive load**: Implementation agents receive focused 100-150 line contexts
- **Failure isolation**: If a sub-phase fails, prior sub-phases remain intact
- **Natural checkpoints**: Manual testing after each sub-phase catches integration issues early

### See Also

- `docs/architecture/designs/_templates/sub-phase-roadmap-template.md` — standard sub-phase roadmap structure
- `docs/architecture/designs/_templates/sub-phase-template.md` — individual sub-phase file structure
- `docs/architecture/designs/README.md` — when to create sub-phases and folder organization

---

## 6. AI Assistant Setup (Optional)

Arx Runa supports AI assistants via LSP and MCP integrations for enhanced code intelligence and tooling.

### GitHub Copilot CLI

LSP configuration is in `.github/lsp.json`:

```json
{
  "lspServers": {
    "rust": {
      "command": "rust-analyzer",
      "args": [],
      "fileExtensions": {
        ".rs": "rust"
      }
    }
  }
}
```

**Prerequisites**:
- `rust-analyzer`: `rustup component add rust-analyzer`

### Claude CLI

**LSP Plugins** (install once per user):
```bash
claude plugin install rust-analyzer-lsp
claude plugin install typescript-lsp
claude plugin install code-review
claude plugin install security-guidance
claude plugin install commit-commands
```

---

## 7. Encryption stack (for context)

| Component | Technology |
|---|---|
| Encryption | XChaCha20-Poly1305 via `chacha20poly1305` crate |
| KDF | Argon2id → HKDF-SHA256 (three derived keys) |
| MFA | USB key file (32 bytes random entropy) |
| Local DB | SQLite + SQLCipher |
| Cloud transport | Rclone |

See [System Architecture](../architecture/) for full design rationale.
