---
mode: agent
description: Run, write, or audit tests for a VoidGate module
---

Run or write tests for: ${input:Module name, "all", "coverage", "write <target>", or "adversarial"}

**If the argument names a module** (e.g., "crypto", "auth", "storage"):
1. Use the `test-writer` agent to audit existing tests for that module
2. Identify gaps: missing adversarial tests, missing boundary cases, untested
   error variants
3. Write the missing tests
4. Run `cargo test` and report results

**If the argument is "all" or empty**:
1. Run `cargo test` across the workspace
2. Report: total tests, failures, and which modules have no tests at all
   (`cargo test -- --list` to enumerate)
3. Flag modules in `src-tauri/src/crypto/`, `src-tauri/src/auth/`, or
   `src-tauri/src/storage/` with zero tests — these are high priority

**If the argument is "coverage"**:
1. Run coverage via `cargo tarpaulin` or `cargo llvm-cov` if installed
2. Report per-module coverage percentages
3. Flag any module below 80% coverage in the `crypto/`, `auth/`, `storage/` paths

**If the argument starts with "write"** (e.g., "write crypto::encrypt_chunk"):
1. Use the `test-writer` agent to write comprehensive tests for the named
   function or module
2. Include: round-trip, adversarial, boundary, and error path tests as
   appropriate for the target
3. Run `cargo test` after writing

**If the argument is "adversarial"**:
1. Use the `test-writer` agent to generate crypto-specific adversarial tests
   across all modules in `src-tauri/src/crypto/` and `src-tauri/src/auth/`
2. Cover: corrupted ciphertext, truncated chunks, AAD mismatch, wrong key,
   tag tampering, nonce reuse detection

**Post-test rule**: if any tests in `src-tauri/src/crypto/`,
`src-tauri/src/auth/`, or `src-tauri/src/storage/` fail, use the
`security-reviewer` agent to assess whether the failure indicates a security
issue or an implementation bug.
