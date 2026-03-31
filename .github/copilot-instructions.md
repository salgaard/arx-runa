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
| Memory files | `.claude/memory/*.md` | ✓ Yes — read at session start |

## Using `.claude/memory/` files

The `.claude/memory/` directory contains **read-only reference material** for
agents. These files capture architecture rationale, known gotchas, and pending
decisions that persist across sessions.

**Purpose**: Provide context continuity between sessions — not agent-writable
storage.

**How to use**:

1. **At session start**: Read `.claude/memory/MEMORY.md` for an index of what's
   available, then read specific files as needed for the task at hand.

2. **During implementation**: Reference `architecture_rationale.md` when making
   design decisions to ensure consistency with established patterns.

3. **Before adding new features**: Check `pending_decisions.md` to see if there
   are open questions that affect your work.

4. **When debugging**: Consult `known_gotchas.md` for common pitfalls.

**What these files contain**:

| File | Content |
|------|---------|
| `MEMORY.md` | Index and quick reference for all memory files |
| `architecture_rationale.md` | Why each major design decision was made, rejected alternatives, trade-offs |
| `pending_decisions.md` | Open design questions not yet resolved |
| `known_gotchas.md` | Implementation pitfalls discovered during development |

**Source of truth hierarchy**:

1. `docs/architecture/designs/*.md` — authoritative design specifications
2. `docs/architecture-decisions/*.md` — ADRs with full context
3. `.claude/memory/*.md` — quick reference summaries (derived from above)

If `.claude/memory/` conflicts with `docs/architecture/designs/`, the design
document is correct. Report the inconsistency and update the memory file.

**Do NOT**:
- Treat memory files as writable session state
- Add new memory files without updating `MEMORY.md` index
- Duplicate information already in design documents

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
