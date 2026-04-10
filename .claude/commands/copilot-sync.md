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
- `.claude/commands/*.md` — both read directly

---

## Transformation rule

The Claude rule content is copied verbatim, with the frontmatter key changed from `paths:` to `applyTo:`.

**`applyTo` is always a single quoted string** — comma-separated for multiple globs. Never a YAML list.

| Claude Code `paths:` | Copilot `applyTo:` |
|----------------------|-------------------|
| Single path: `paths:\n  - "src/**"` | `applyTo: "src/**"` |
| Multi-path list: `paths:\n  - "a/**"\n  - "b/**"` | `applyTo: "a/**,b/**"` |

**Sync procedure:**
1. Read `.claude/rules/<name>.md`
2. Extract all glob values from the `paths:` list
3. Join them with `,` (no spaces) into a single quoted string
4. Write `applyTo: "<glob1>,<glob2>"` as the frontmatter
5. Copy all content below the frontmatter verbatim
6. Write to `.github/instructions/<name>.instructions.md`

After transformation, content below the frontmatter is byte-identical; only the frontmatter key and format differ.

---

## Detailed steps

### Step 1 — Read all files

1. All `.claude/rules/*.md` files (except README.md)
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
- After editing design documents and updating rule summaries
- Before commits that touch rules
- When the PostToolUse hook reminds you
