---
name: copilot-sync-check
description: >
  Check whether .github/ Copilot counterpart files are in sync with .claude/
  config files after any edit to agents, commands, skills, or CLAUDE.md.
  Invoke proactively after editing .claude/agents/, .claude/commands/,
  .claude/skills/, or CLAUDE.md. Also invokable manually via
  /copilot-sync-check.
---

Check `.github/` counterparts for divergence from `.claude/` config files.

## Counterpart mapping

| Claude Code file | GitHub Copilot counterpart |
|------------------|---------------------------|
| `CLAUDE.md` | `.github/copilot-instructions.md` |
| `.claude/agents/<name>.md` | `.github/agents/<name>.agent.md` |
| `.claude/commands/<name>.md` | `.github/prompts/<name>.prompt.md` |

**Skills are shared** — `.claude/skills/` follows an open standard read by both
Claude Code and GitHub Copilot. `SKILL.md` files in `.claude/skills/` require
no `.github/` counterpart. Do not check or sync skills.

## Steps

### Step 1 — Identify what changed

If invoked proactively (after an edit), note which `.claude/` file was just
modified. Skills (`.claude/skills/`) are shared with Copilot and need no sync —
skip them. Only check agents, commands, and `CLAUDE.md`.

If invoked manually without arguments, check all counterpart pairs (excluding
skills).

### Step 2 — Check counterpart existence

For each `.claude/` file being checked:
1. Compute the expected `.github/` counterpart path using the mapping above
2. Check whether that file exists
3. If it does not exist, flag it: `MISSING counterpart: <path>`

### Step 3 — Diff semantic content

For each pair that exists:
1. Read both files
2. Compare the **intent and content** — not exact text (formats differ)
3. Flag differences in:
   - Behaviour described (steps, rules, output format)
   - Scope or trigger conditions
   - Agent tool access
   - Security rules or coding standards

Do NOT flag superficial format differences (YAML vs markdown frontmatter,
`{{input}}` vs `$ARGUMENTS`, heading styles).

### Step 4 — Report findings

Output a compact report:

```
Copilot sync check:
  IN SYNC:   <filename pairs that are equivalent>
  DIVERGED:  <filename pairs with semantic differences — describe what changed>
  MISSING:   <Claude files with no .github/ counterpart>
  NOTE:      Skills have no Copilot counterpart — this is expected.
```

Do NOT auto-update `.github/` files. The formats differ and auto-updates risk
introducing errors. List what needs updating so the user can review.

### Step 5 — Suggest update

For each DIVERGED pair, briefly describe what the `.github/` counterpart needs:
- What section to add or change
- What the new behaviour should say

Keep suggestions actionable and short (1-3 bullet points per file).
