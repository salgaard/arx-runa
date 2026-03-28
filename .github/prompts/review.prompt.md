---
mode: agent
description: Review code for merge readiness — security, docs, tests, clippy
---

Review the following for merge readiness: ${input:Files or scope to review (e.g. src-tauri/src/crypto/ or a PR diff)}

Steps:
1. Use the `security-reviewer` agent on all modified files in the
   specified scope.
2. If any CRITICAL findings, stop — list them. These must be fixed
   before proceeding.
3. Use the `documentation-writer` agent to check if any `docs/` files
   are stale relative to the changes. Compare current code against
   existing documentation and flag mismatches.
4. Check that `cargo test` and `cargo clippy -- -D warnings` pass.
5. Produce a merge readiness summary:
   - CRITICAL issues (must fix)
   - WARNING issues (should fix)
   - Documentation gaps (`docs/` files that need updating)
   - Test coverage notes (missing boundary tests, untested paths)
   - Verdict: **READY** / **NOT READY**
