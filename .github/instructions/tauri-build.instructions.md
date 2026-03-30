---
applyTo: "src-tauri/build.rs"
---

# Tauri build script — scoped rules

These rules apply to `src-tauri/build.rs` — the Tauri build-time configuration
that controls command exposure and capability generation.

## Command allowlisting

- Use `AppManifest::commands()` to explicitly whitelist all exposed commands:
  ```rust
  fn main() {
      tauri_build::try_build(
          tauri_build::Attributes::new()
              .app_manifest(
                  tauri_build::AppManifest::new()
                      .commands(&[
                          "unlock_vault",
                          "lock_vault",
                          "list_files",
                          "encrypt_and_upload",
                          "download_and_decrypt",
                      ])
              ),
      )
      .unwrap();
  }
  ```
- This prevents accidental exposure of internal commands
- Commands not in this list cannot be invoked from the frontend

## Why this matters

- By default, all commands registered in `invoke_handler` are callable
- A developer might add a debug command and forget to remove it
- The allowlist in build.rs is the compile-time safety net
- If a command is missing from the allowlist, the build fails (fail-safe)

## Command naming

- Use descriptive full words: `encrypt_and_upload_file`, not `enc_upl`
- Prefix commands by module: `vault_unlock`, `vault_lock`, `file_list`
- Keep the command list alphabetically sorted for easy review

## Build script hygiene

- Do not add complex logic to build.rs — keep it declarative
- Use `tauri_build::try_build` (returns Result) over `tauri_build::build`
- Handle build errors explicitly — do not use `unwrap()` without context

## Integration with capabilities

- Commands must be allowed in BOTH:
  1. `build.rs` via `AppManifest::commands()`
  2. Capability files via permission grants
- This provides defense in depth: build-time + runtime enforcement

## Required commands for VoidGate

Minimum command set (to be expanded as modules are implemented):
- `unlock_vault` — derive keys, open session
- `lock_vault` — zero keys, close session
- `get_vault_status` — check if unlocked (no sensitive data)
- `list_directory` — return decrypted file tree for UI
- `encrypt_and_upload_file` — encrypt, chunk, upload
- `download_and_decrypt_file` — download, verify, decrypt
- `delete_file` — remove from manifest and cloud

## Prohibited commands

Never expose these as Tauri commands:
- Anything that returns raw key material
- Anything that exposes internal file paths
- Anything that runs arbitrary shell commands
- Debug/test commands that bypass security checks
