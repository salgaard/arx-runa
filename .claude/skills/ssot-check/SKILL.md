---
name: ssot-check
description: >
  Validate SSOT architecture: verify rule files reference valid design docs.
  Reports stale references or missing documents.
---

Validate the SSOT (Single Source of Truth) documentation architecture.

## Usage

```
/ssot-check            # validate all rule files
```

---

## What it validates

1. **Design doc references**: Rule files that reference design documents should point to existing files
2. **Rule-instruction sync**: Claude rules should be synced to Copilot instructions

---

## Validation procedure

### Step 1 — Identify design doc references

Scan `.claude/rules/*.md` files for patterns like:
- `**Design specification**: \`docs/...`
- `See \`docs/architecture/designs/...`
- Any backtick-quoted path starting with `docs/`

### Step 2 — Verify referenced files exist

For each referenced design document:
1. Resolve path relative to repository root
2. Check if file exists
3. Report missing files

### Step 3 — Check rule-instruction sync

For each `.claude/rules/<name>.md`:
1. Check if `.github/instructions/<name>.instructions.md` exists
2. Compare content (after `paths:` → `applyTo:` transformation)
3. Report mismatches

---

## Output format

```
SSOT Validation Report
======================

Design Doc References:
  ✓ crypto.md → docs/architecture/designs/cryptographic-primitives/design.md
  ✓ auth.md → docs/architecture/designs/authentication-and-session-management/design.md
  ✓ storage.md → docs/architecture/designs/chunking-and-manifest/design.md

Rule-Instruction Sync:
  ✓ crypto: IN_SYNC
  ✓ auth: IN_SYNC
  ✓ storage: IN_SYNC
  ✓ rust: IN_SYNC
  ✓ tauri: IN_SYNC
  ✓ memory-protection: IN_SYNC
  ✓ leptos: IN_SYNC
  ✓ docs: IN_SYNC

All checks passed.
```

### On failure

```
SSOT Validation Report
======================

Design Doc References:
  ✓ crypto.md → docs/architecture/designs/cryptographic-primitives/design.md
  ✗ auth.md → docs/architecture/designs/auth/design.md (NOT FOUND)
    Expected: docs/architecture/designs/authentication-and-session-management/design.md

Rule-Instruction Sync:
  ✗ storage: DIVERGED

Errors found. Run `/copilot-sync` to fix sync issues.
```

---

## When to run

- After editing `.claude/rules/*.md` files
- After renaming or moving design documents
- Before commits that touch rules or design docs
- When unsure if SSOT is properly maintained

---

## Related commands

- `/copilot-sync` — Fix rule-instruction sync issues
- `docs/guides/documentation-ssot.md` — Full SSOT architecture documentation
