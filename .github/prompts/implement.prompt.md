---
mode: agent
description: Implement a new feature or module following VoidGate coding standards
---

Implement the following: ${input:What should be implemented?}

Steps:
1. Use the `rust-implementer` agent to implement following VoidGate coding
   standards.
2. If any modified files are in `src-tauri/src/crypto/`, `src-tauri/src/auth/`,
   or `src-tauri/src/storage/`, invoke the `security-reviewer` agent on them.
3. Fix any CRITICAL findings before considering the task done.
4. Run `cargo test` and `cargo clippy -- -D warnings` to verify.
5. Check if changes affect anything documented in `docs/` — if so, list which
   files need updating. Do not auto-update docs; just flag them.
