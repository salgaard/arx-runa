# Arx Runa — Copilot CLI

See `CLAUDE.md` for all coding standards.

## Hooks

- **PostToolUse**: `cargo clippy` on Rust edits; sync reminder on rule edits
- **PreToolUse**: blocks `.env` access and pipe-to-shell patterns

## Rule sync

After editing `.claude/rules/`, run `/copilot-sync` to update `.github/instructions/`.
