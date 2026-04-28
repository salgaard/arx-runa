---
name: copilot-sync
description: Synchronise `.github/instructions/` files with `.claude/rules/` source files.
---

## Usage
```
/copilot-sync          # compare and apply fixes
/copilot-sync check    # compare and report only
```
Also the canonical mirror step in `/implement-plan` governance-sync when planned updates touch `.claude/rules/*.md`.

## Sync scope
| Claude Code source | Copilot counterpart |
|---|---|
| `.claude/rules/<name>.md` | `.github/instructions/<name>.instructions.md` |

Shared (no sync needed): `CLAUDE.md`, `.claude/agents/*.md` — both tools read directly.

## Transformation rule
Copy rule content verbatim; change frontmatter key `paths:` → `applyTo:` as a single quoted comma-separated string (no spaces):

| `paths:` | `applyTo:` |
|---|---|
| `- "src/**"` | `applyTo: "src/**"` |
| `- "a/**"` + `- "b/**"` | `applyTo: "a/**,b/**"` |

Procedure:
1. Read `.claude/rules/<name>.md`
2. Extract all `paths:` globs, join with `,`
3. Write `applyTo: "<glob1>,<glob2>"` as frontmatter
4. Copy all content below frontmatter verbatim
5. Write to `.github/instructions/<name>.instructions.md`

Content below frontmatter is byte-identical after transformation.

## Comparison & output
For each `.claude/rules/<name>.md`: read both files, apply transformation, classify: **IN_SYNC** / **DIVERGED** / **MISSING**

Default mode: fix DIVERGED and MISSING. Check mode: report only.

```
Rules → Instructions:
  ✓ crypto: IN_SYNC
  ✗ storage: DIVERGED → FIXED   (check mode: DIVERGED (line 15 differs))
  ✗ sharing: MISSING → CREATED  (check mode: MISSING / Run `/copilot-sync` to fix.)
```

## When to run
- After editing `.claude/rules/*.md`
- After editing design docs and updating rule summaries
- Before commits touching rules
- When PostToolUse hook prompts
- During `/implement-plan` governance sync when plan includes rule updates
