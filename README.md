# VoidGate

**VoidGate** is a security-critical Zero-Trace gateway designed to bridge the gap between local user files and public cloud storage.

## Project Vision

To provide a hardware-verified encryption layer that ensures no sensitive data, metadata, or cryptographic remnants are ever persisted to the local disk or accessible to cloud providers.

---

## Core Technology Stack

| Component | Technology |
|---|---|
| **Language** | [Rust](https://www.rust-lang.org/) — memory safety and performance |
| **Framework** | [Tauri](https://tauri.app/) — lightweight, secure desktop frontend |
| **Encryption** | AES-256-GCM / ChaCha20-Poly1305 (AEAD) |
| **KDF** | Argon2id |
| **MFA** | Hardware-based identity verification (USB/Hardware-ID) |

---

## Security Paradigms — The VoidGate Protocol

1. **Zero-Trace RAM** — All sensitive variables are overwritten using the `zeroize` trait immediately after use.
2. **Streaming Pipeline** — Files are processed in 5MB chunks. No full-file buffering.
3. **Stateless Gateway** — The application acts as a pipe, not a vault. Decrypted data never touches persistent storage.

---

## Installation and Setup (Windows)

This project requires a correctly installed Rust toolchain to guarantee memory safety and full debugging capabilities.

### 1. Install Rust

Follow the official guide at [rust-lang.org/learn/get-started](https://rust-lang.org/learn/get-started):

1. Download and run `rustup-init.exe`.
2. Follow the on-screen instructions (Standard installation is recommended).
3. Verify the installation:

```bash
cargo --version
```

---

### 2. Development Environment (IDE)

The project is optimized for **VS Code** or **Cursor**. For the best experience, follow the [VS Code Rust Guide](https://code.visualstudio.com/docs/languages/rust).

#### Recommended Extensions — `.vscode/extensions.json`

in this file there are recommended extensions that vscode will recommend to you when you open the project.

```json
{
    "recommendations": [
        "rust-lang.rust-analyzer",
        "anysphere.cpptools",
        "vadimcn.vscode-lldb"
    ]
}
```

#### Editor Settings — `.vscode/settings.json`

Enforces Clippy linting, correct formatting, and vital debugger fixes:

```json
{
    // --- RUST SPECIFIC (Fra dokumentationen) ---
    "rust-analyzer.check.command": "clippy",
    "editor.inlayHints.enabled": "on",
    "editor.semanticTokenColorCustomizations": {
        "rules": {
            "*.mutable": {
                "fontStyle": "underline"
            }
        }
    },
    "editor.formatOnSave": true,
    "editor.codeActionWidget.includeNearbyQuickFixes": true,
    // --- CURSOR & EDITOR OPTIMIZATION ---
    "files.autoSave": "afterDelay",
    "editor.tabCompletion": "on",
    "workbench.startupEditor": "none",
    "explorer.confirmDragAndDrop": false,
    // --- GIT & WORKBENCH ---
    "git.confirmSync": false,
    "git.enableSmartCommit": true,
    "workbench.colorTheme": "Default Dark+",
    "typescript.updateImportsOnFileMove.enabled": "always",
    // --- CLEANUP (Fjernet de "off" indstillinger der blokerer Cursor) ---
    "editor.suggest.showInlineDetails": true,
    "editor.quickSuggestions": {
        "other": true,
        "comments": false,
        "strings": true
    }
}
```

---

### 3. Debugging Configuration (Required for Windows)

By default, Windows uses the MSVC toolchain, which has limitations regarding Rust inspection in the debugger. For full support of variable inspection and Rust-specific data types, the project uses the **GNU toolchain**.

Run the following commands:

```bash
rustup toolchain install stable-x86_64-pc-windows-gnu
rustup default stable-x86_64-pc-windows-gnu
```

This ensures that **CodeLLDB** can correctly interpret Rust formatters during development.

---

## Cargo Workflow

Cargo is the Rust build tool and package manager. Common commands:

```bash
# Build the project
cargo build

# Run the project
cargo run

# Run unit tests
cargo test

# Generate documentation
cargo doc --open

#publish a library to crates.io with 
cargo publish
```

---

## Debugging Process

1. Set a breakpoint in your code (e.g., in `src/main.rs`).
2. Press `F5` or navigate to **Run and Debug** in the side panel.
3. Select the profile **"VoidGate: Debug"**.

---
