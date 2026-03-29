# VoidGate — Copilot Instructions

## Project standards

See `CLAUDE.md` in the repository root. GitHub Copilot reads `CLAUDE.md`
directly as agent instructions. All coding standards, naming conventions,
module design rules, error handling, testing standards, and hard rules are
defined there.

## What Copilot reads natively from this repository

| Resource | Path | Read by Copilot? |
|----------|------|-----------------|
| Project instructions | `CLAUDE.md` | Yes — as agent instructions |
| Agent personas | `.claude/agents/*.md` | Yes — VS Code maps tool names automatically |
| Skills | `.claude/skills/*/SKILL.md` | Yes — open standard |
| Slash commands | `.claude/commands/*.md` | **No** — use `.github/prompts/` instead |

## Copilot-specific notes

The following Claude Code features have no direct Copilot equivalent:

- **`settings.json` hooks** (PostToolUse clippy auto-run, PreToolUse
  pipe-to-shell blocking, sensitive file access blocking) — Copilot has no
  hook system. Quality gates are enforced via CI (`.github/workflows/`).

- **`permissions.deny` / `permissions.ask`** — Copilot has no permission
  model. Sensitive file exclusion is enforced via `.gitignore` and CI.

- **Agent memory** (`.claude/memory/MEMORY.md`) — Copilot agents do not
  persist memory across sessions. Architecture decisions and gotchas are
  documented in `docs/` and inline in agent prompts.

- **Sub-agent routing** (parallel/sequential/background dispatch) — Copilot
  does not support multi-agent orchestration. Prompts in `.github/prompts/`
  reference agents by name but orchestration is manual.

- **Slash command modes** (e.g. `/test adversarial`) — Copilot prompts in
  `.github/prompts/` provide equivalent entry points for each mode.

## Path-specific rules

Path-specific rules are maintained in two parallel locations (same content,
different frontmatter keys):

| Claude Code | GitHub Copilot | Scope |
|------------|----------------|-------|
| `.claude/rules/crypto.md` | `.github/instructions/crypto.instructions.md` | `src-tauri/src/crypto/**` |
| `.claude/rules/auth.md` | `.github/instructions/auth.instructions.md` | `src-tauri/src/auth/**` |
| `.claude/rules/storage.md` | `.github/instructions/storage.instructions.md` | `src-tauri/src/storage/**` |
| `.claude/rules/tauri-ui.md` | `.github/instructions/tauri-ui.instructions.md` | `src-tauri/src/ui/**` |
| `.claude/rules/rust.md` | `.github/instructions/rust.instructions.md` | `src-tauri/**/*.rs` |
| `.claude/rules/docs.md` | `.github/instructions/docs.instructions.md` | `docs/**` |

When editing rules, update both files. The `copilot-sync-check` skill
detects drift between them.
