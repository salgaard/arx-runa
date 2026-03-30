# VoidGate — Copilot CLI Instructions

## Project standards

See `CLAUDE.md` in the repository root. Copilot CLI reads `CLAUDE.md`
directly as agent instructions. All coding standards, naming conventions,
module design rules, error handling, testing standards, and hard rules are
defined there.

## What Copilot CLI uses from this repository

| Resource | Path | Used by CLI? |
|----------|------|--------------|
| Project instructions | `CLAUDE.md` | ✓ Yes |
| Agent personas | `.claude/agents/*.md` | ✓ Yes — via task tool |
| Skills | `.claude/skills/*/SKILL.md` | ✓ Yes — invokable |
| Commands | `.claude/commands/*.md` | ✓ Yes — as skills |
| Path-specific rules | `.github/instructions/*.instructions.md` | ✓ Yes |

## Syncing rules

Path-specific rules live in two locations with different frontmatter:

| Claude Code | Copilot CLI |
|-------------|-------------|
| `.claude/rules/<name>.md` | `.github/instructions/<name>.instructions.md` |

When editing rules, update both files — or run `/copilot-sync` to auto-sync.

The sync is a direct transformation: only the frontmatter key changes
(`paths:` → `applyTo:`), content is identical.

### Current rules

| Rule | Scope |
|------|-------|
| `crypto` | `src-tauri/src/crypto/**` |
| `auth` | `src-tauri/src/auth/**` |
| `storage` | `src-tauri/src/storage/**` |
| `tauri-ui` | `src-tauri/src/ui/**` |
| `rust` | `src-tauri/**/*.rs` |
| `docs` | `docs/**` |

## Claude Code features not available in Copilot CLI

- **`settings.json` hooks** — Copilot CLI has no hook system. Quality gates
  are enforced via CI (`.github/workflows/`).

- **`permissions.deny` / `permissions.ask`** — Copilot CLI has no permission
  model. Sensitive file exclusion is enforced via `.gitignore` and CI.

- **Agent memory persistence** — Copilot CLI does not persist memory across
  sessions. Architecture decisions and gotchas are documented in `docs/` and
  `.claude/memory/MEMORY.md` (which you can read at session start).
