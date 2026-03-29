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
      5. Check Copilot prompt sync: compare `.claude/commands/` against
         `.github/prompts/`. Flag any commands that have been added, removed,
         or changed without a matching update to the corresponding `.prompt.md`.
         Also check that `.github/copilot-instructions.md` translation notes are
         still accurate (it should only contain Copilot-specific notes, not a
         mirror of `CLAUDE.md`).
      6. Check ADR coverage: list any architectural decisions referenced in
         recent commits or in `memory/MEMORY.md` that lack a corresponding
         file in `docs/architecture-decisions/`. Flag these as ADR gaps.
      7. Produce a merge readiness summary:
         - CRITICAL issues (must fix)
         - WARNING issues (should fix)
         - Documentation gaps (`docs/` files that need updating)
         - Test coverage notes (missing boundary tests, untested paths)
         - Copilot prompt gaps (`.github/prompts/` out of sync with commands)
         - ADR gaps (architectural decisions without a formal ADR)
         - Verdict: **READY** / **NOT READY**
