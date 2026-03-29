# Development Setup

## Prerequisites

- Windows 10/11
- [Rust toolchain](https://rust-lang.org/learn/get-started) (stable)
- [VS Code](https://code.visualstudio.com/) with recommended extensions
  (see `.vscode/extensions.json`)

---

## 1. Install Rust

Download and run `rustup-init.exe` from the [official site](https://rust-lang.org/learn/get-started/), then verify:

```bash
cargo --version
```

---

## 2. GNU toolchain (required for debugging on Windows)

By default Windows uses the MSVC toolchain, which has limitations for Rust
variable inspection in CodeLLDB. Install the GNU toolchain for full debugger
support:

```bash
rustup toolchain install stable-x86_64-pc-windows-gnu
rustup default stable-x86_64-pc-windows-gnu
```

This ensures CodeLLDB can correctly interpret Rust formatters during debugging.

---

## 3. VS Code extensions

Open the project in VS Code — it will prompt you to install recommended
extensions from `.vscode/extensions.json`:

- `rust-lang.rust-analyzer` — language server, inline hints, completions
- `ms-vscode.cpptools` — native debug adapter
- `vadimcn.vscode-lldb` — CodeLLDB debugger with Rust formatters
- `tauri-apps.tauri-vscode` — Tauri project support

---

## 4. Cargo workflow

```bash
# Build
cargo build

# Run
cargo run

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

1. Set a breakpoint in your code (e.g., `src-tauri/src/main.rs`).
2. Press `F5` or open **Run and Debug** in the VS Code sidebar.
3. Select the **"VoidGate: Debug"** profile.

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

## 7. Encryption stack (for context)

| Component | Technology |
|---|---|
| Encryption | XChaCha20-Poly1305 via `chacha20poly1305` crate |
| KDF | Argon2id → HKDF-SHA256 (three derived keys) |
| MFA | USB key file (32 bytes random entropy) |
| Local DB | SQLite + SQLCipher |
| Cloud transport | Rclone |

See [Architecture Decisions](../architecture-decisions/) and
[System Architecture](../architecture/) for full design rationale.
