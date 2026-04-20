# Sync Sub-Phases

Synchronise sub-phase files with the current state of: $ARGUMENTS

Use when a design document has been updated (after `/review-design` or a manual edit) and its sub-phase files may be out of date, or when sub-phases reproduce design.md content that should instead be a reference.

---

## Argument Parsing

`$ARGUMENTS` can be:
- **Design name**: `project-scaffolding`, `chunking-and-manifest` → fuzzy-match in `docs/architecture/designs/`
- **Path**: `docs/architecture/designs/project-scaffolding/design.md` → use directly

---

## Flow

### 1. Load

1. Read `docs/architecture/designs/<design-name>/design.md` in full
2. Check for `sub-phases/` directory — if absent, report "No sub-phases found for <design-name>" and stop
3. Read `sub-phases/roadmap.md` and each `sub-phases/<N.N-*.md>` file

---

### 2. Diff

For each sub-phase file, identify:

**Type A — Stale duplication**: The sub-phase reproduces a spec verbatim that has since changed in `design.md` (e.g. old dep version, old config block, old class names). These are errors that must be fixed.

**Type B — Fresh duplication**: The sub-phase reproduces a spec that still matches `design.md` — no error, but it will drift on the next design change. Convert to a reference.

**Type C — Own content**: The sub-phase contains implementation steps, commands, validation sequences, and gotchas not present in `design.md`. These are correct and belong here.

Build a table:

```
| File | Location | Type | Current value | Design.md value | Action |
|------|----------|------|---------------|-----------------|--------|
| 0.2  | line 22  | A    | hkdf = "0.12" | hkdf = "0.13"   | Fix    |
| 0.1  | line 27  | B    | [dep block]   | [matches]       | Ref    |
| 0.3  | line 19  | C    | commands      | —               | Keep   |
```

Present this table to the user before making any changes.

---

### 3. Apply

For each **Type A** (stale duplication):
- Update the sub-phase value to match `design.md`

For each **Type B** (fresh duplication):
- Replace the reproduced block with a reference:
  ```markdown
  Use the [<Section Name>](../design.md#<anchor>) from `design.md`. Do not reproduce the spec here — consult `design.md` during implementation.
  ```
- Keep a one-line summary if it helps orientation (e.g. "The hook uses `@tailwindcss/cli` — see design.md for the full `Trunk.toml` block.")

For **Type C** (own content): no change.

---

### 4. Report

State:
```
Sync complete for <design-name>.
  Fixed (stale):     N locations across M files
  Converted to ref:  N locations across M files
  Kept (own):        N locations
```

If no divergences were found: "Sub-phases are in sync with design.md. No changes made."

---

## Rules

- **Never remove step descriptions or validation commands** — these belong to the sub-phase even when the spec detail they reference moves to design.md
- **Preserve implementation notes** — platform quirks, ordering constraints, and non-obvious tips own the sub-phase
- **One sentence summary is OK** — after converting a block to a reference, leave a one-liner so the reader knows what they'll find in design.md without clicking through
- **Don't touch design.md** — this command only flows design.md → sub-phases, never the reverse
