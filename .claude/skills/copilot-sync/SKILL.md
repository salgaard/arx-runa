---
name: copilot-sync
description: >
  Sync tool for Claude rules → Copilot instructions.
  Transforms .claude/rules/*.md to .github/instructions/*.instructions.md.
---

Synchronise `.github/instructions/` files with `.claude/rules/` source files.

## Usage

```
/copilot-sync          # default: compare and apply fixes
/copilot-sync check    # compare and report only (no changes)
```

---

## What needs syncing

Copilot CLI reads `.github/instructions/` for path-specific rules. These must
be kept in sync with `.claude/rules/`:

| Claude Code source | Copilot counterpart |
|-------------------|---------------------|
| `.claude/rules/<name>.md` | `.github/instructions/<name>.instructions.md` |

**Shared resources (no sync needed):**
- `CLAUDE.md` — both read directly
- `.claude/agents/*.md` — both read directly
- `.claude/skills/*/SKILL.md` — both read directly
- `.claude/commands/*.md` — both read directly

---

## Transformation rule

This is a **direct transformation**. The Claude rule content is copied verbatim,
with only the frontmatter key changed:

| Claude Code | Copilot Instructions |
|-------------|---------------------|
| `paths:\n  - "<glob>"` | `applyTo: "<glob>"` |
| `paths:\n  - "<glob1>"\n  - "<glob2>"` | `applyTo:\n  - "<glob1>"\n  - "<glob2>"` |

**Sync procedure:**
1. Read `.claude/rules/<name>.md`
2. Replace `paths:` key with `applyTo:` key (preserve YAML list structure)
3. Write to `.github/instructions/<name>.instructions.md`

After transformation, files should be byte-identical except for the frontmatter key name.

---

## Detailed steps

### Step 1 — Read all files

1. All `.claude/rules/*.md` files
2. All `.github/instructions/*.instructions.md` files

### Step 2 — Compare each rule

For each `.claude/rules/<name>.md`:

1. Read Claude rule content
2. Read corresponding `.github/instructions/<name>.instructions.md`
3. Apply frontmatter transformation (`paths:` → `applyTo:`)
4. Compare (after transformation)
5. Classify:
   - **IN_SYNC**: Identical after frontmatter transformation
   - **DIVERGED**: Content differs
   - **MISSING**: Copilot file doesn't exist

### Step 3 — Output by mode

#### Default mode (fix)

For each DIVERGED or MISSING file:
1. Generate the correct Copilot file by transforming the Claude rule
2. Write to `.github/instructions/<name>.instructions.md`

```
Rules → Instructions:
  ✓ crypto: IN_SYNC
  ✓ auth: IN_SYNC
  ✗ storage: DIVERGED → FIXED
  ✗ sharing: MISSING → CREATED
```

#### Check mode

Report only, no changes:

```
Rules → Instructions:
  ✓ crypto: IN_SYNC
  ✓ auth: IN_SYNC
  ✗ storage: DIVERGED (line 15 differs)
  ✗ sharing: MISSING

Run `/copilot-sync` to fix.
```

---

## When to run

- After editing any `.claude/rules/*.md` file
- Before commits that touch rules
- When the PostToolUse hook reminds you
