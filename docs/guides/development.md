# Development Setup

## Prerequisites

- Windows 10/11
- [Rust toolchain](https://rust-lang.org/learn/get-started) (stable, MSVC)
- `wasm32-unknown-unknown` target (`rustup target add wasm32-unknown-unknown`)
- [Trunk](https://trunkrs.dev/) (`cargo install trunk --locked`)
- [Strawberry Perl](https://strawberryperl.com/) (Windows source builds only; required for vendored OpenSSL in `src-tauri`)
- [VS Code](https://code.visualstudio.com/) with recommended extensions
  (see `.vscode/extensions.json`)

---

## 1. Install Rust

Download and run `rustup-init.exe` from the [official site](https://rust-lang.org/learn/get-started/), then verify:

```bash
cargo --version
```

---

## 2. MSVC toolchain (required for the VS Code one-click debugger)

This workspace uses the VS Code Windows debugger (`cppvsdbg`) for backend
breakpoints, so keep the default toolchain on MSVC:

```bash
rustup toolchain install stable-x86_64-pc-windows-msvc
rustup default stable-x86_64-pc-windows-msvc
```

Verify:

```bash
rustup show
```

If you build the backend from source on Windows, `rusqlite` with
`bundled-sqlcipher-vendored-openssl` requires Perl on `PATH` during compile:

```bash
winget install StrawberryPerl.StrawberryPerl
perl -v
```

This is a build-time prerequisite only; end users running packaged binaries do
not need Perl.

---

## 3. VS Code extensions

Open the project in VS Code — it will prompt you to install recommended
extensions from `.vscode/extensions.json`:

- `rust-lang.rust-analyzer` — language server, inline hints, completions
- `ms-vscode.cpptools` — Windows backend debugger (`cppvsdbg`)
- `vadimcn.vscode-lldb` — optional backend debugger (especially useful on macOS/Linux)
- `tauri-apps.tauri-vscode` — Tauri project support

---

## 4. Cargo workflow

```bash
# Build
cargo build

# Run full app (frontend + backend)
cargo tauri dev

# Run tests
cargo test

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

## 5. Debugging

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

## 6. Git hooks (documentation stubs)

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

## 7. Hook prerequisites

Claude Code hooks use `jq` to parse tool input. Install it:

```bash
# winget
winget install jqlang.jq

# or scoop
scoop install jq
```

---

## 8. Documentation SSOT Architecture

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

## 9. Working with Sub-Phase Roadmaps

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

## 10. AI Assistant Setup (Optional)

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

## 11. Encryption stack (for context)

| Component | Technology |
|---|---|
| Encryption | XChaCha20-Poly1305 via `chacha20poly1305` crate |
| KDF | Argon2id → HKDF-SHA256 (three derived keys) |
| MFA | USB key file (32 bytes random entropy) |
| Local DB | SQLite + SQLCipher |
| Cloud transport | Rclone |

See [System Architecture](../architecture/) for full design rationale.
