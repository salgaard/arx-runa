---
name: copilot-sync-check
description: >
  Check whether .github/ Copilot counterpart files are in sync with .claude/
  after a command or rule is added, changed, or removed.
  Invoke proactively after editing any file in .claude/commands/ or .claude/rules/.
  Also invokable manually via /copilot-sync-check.
  Note: CLAUDE.md, .claude/agents/, and .claude/skills/ are read natively
  by Copilot and need no sync.
---

Check `.github/` counterparts for divergence from `.claude/` config files.

## What is shared vs. what needs syncing

GitHub Copilot reads the following directly — no `.github/` counterpart needed:

| Shared resource | Path |
|----------------|------|
| Project instructions | `CLAUDE.md` |
| Agent personas | `.claude/agents/*.md` (VS Code auto-maps tool names) |
| Skills | `.claude/skills/*/SKILL.md` (open standard) |
| Saved plans | `.claude/plans/*.md` (plain markdown — reference directly in Copilot chat) |

**Two things need syncing:**

| Claude Code | GitHub Copilot counterpart | Frontmatter difference |
|-------------|---------------------------|------------------------|
| `.claude/commands/<name>.md` | `.github/prompts/<name>.prompt.md` | `$ARGUMENTS` vs `{{input}}` |
| `.claude/rules/<name>.md` | `.github/instructions/<name>.instructions.md` | `paths: [list]` vs `applyTo: "glob"` |

## Steps

### Step 1 — Identify what changed

If invoked proactively, note which `.claude/` file was just modified — a
command (`.claude/commands/`) or a rule (`.claude/rules/`).

If invoked manually without arguments, check all pairs across both mappings.

### Step 2 — Check counterpart existence

For each `.claude/commands/<name>.md`:
- Expected counterpart: `.github/prompts/<name>.prompt.md`
- Flag missing: `MISSING prompt: .github/prompts/<name>.prompt.md`

For each `.claude/rules/<name>.md`:
- Expected counterpart: `.github/instructions/<name>.instructions.md`
- Flag missing: `MISSING instructions: .github/instructions/<name>.instructions.md`

### Step 3 — Diff semantic content

For each pair that exists, read both files and compare **intent and content**
— not exact text (frontmatter keys and placeholder syntax differ by design).

Flag differences in:
- Rules or steps described
- Glob patterns / file scope
- Security rules or coding standards

Do NOT flag: `paths:` vs `applyTo:`, `$ARGUMENTS` vs `{{input}}`, heading style.

### Step 4 — Report findings

```
Copilot sync check:
  IN SYNC:   <pairs that are semantically equivalent>
  DIVERGED:  <pairs with content differences — describe what changed>
  MISSING:   <.claude/ files with no .github/ counterpart>
```

Do NOT auto-update `.github/` files. Formats differ and auto-updates risk
errors. List what needs updating so the user can review.

### Step 5 — Suggest updates

For each DIVERGED or MISSING pair, describe what the `.github/` counterpart
needs in 1-3 bullet points.
