Recommended model: `/model sonnet`

Implement the saved plan: $ARGUMENTS

## Step 1 — Resolve the plan file

Locate the plan file from $ARGUMENTS:
- If $ARGUMENTS is a full filename (e.g., `phase-001-cryptographic-primitives.md`),
  read `.claude/plans/$ARGUMENTS`
- If $ARGUMENTS is a filename without the `.md` extension, append it and try again
- If $ARGUMENTS is `latest`, find the most recently created file in `.claude/plans/`
  (by the `created` frontmatter field, excluding `_template.md`)
- If $ARGUMENTS is empty or no match is found, list all files in `.claude/plans/`
  (excluding `_template.md`) with their title, status, and created date, then ask
  the user to choose one

## Step 2 — Validate the plan

1. Read the plan file and parse its YAML frontmatter
2. If `status` is `draft`, warn the user: "This plan has not been approved. Proceed
   anyway?" and wait for confirmation before continuing
3. If `status` is `completed` or `superseded`, warn the user and ask for confirmation
4. Update `status` to `in-progress` in the plan file's frontmatter

## Step 3 — Implement

Follow the **Approach** section of the plan step by step:
1. Use the `rust-implementer` agent to implement each step following VoidGate
   coding standards
2. If any modified files are in `src-tauri/src/crypto/`, `src-tauri/src/auth/`,
   or `src-tauri/src/storage/`, automatically invoke the `security-reviewer`
   agent on them
3. Fix any CRITICAL findings before continuing
4. Run `cargo test` and `cargo clippy -- -D warnings` to verify

## Step 4 — Flag documentation

Check the plan's **Documentation impact** section. List any `docs/` files that
need creating or updating. Do not auto-update docs; just report what is needed.

## Step 5 — Mark complete

Update `status` to `completed` in the plan file's frontmatter.
Report what was implemented and what documentation work remains.
