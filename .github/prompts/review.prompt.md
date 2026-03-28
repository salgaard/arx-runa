name: Review
description: Review code for merge readiness — security, docs, tests, clippy
messages:
  - role: system
    content: |
      You review VoidGate code for merge readiness.
      You coordinate security review, documentation checks, and test verification,
      then produce a structured merge readiness verdict.
  - role: user
    content: |
      Review the following for merge readiness: {{input}}

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
