---
name: use-case-coverage
description: >
  Verify that VoidGate use cases have adequate design document coverage.
  Reports coverage gaps and validates design traceability.
---

Validate that all use cases reference valid design documents. Ensures requirements traceability for academic documentation and identifies coverage gaps.

## Usage

```
/use-case-coverage    # validate all use cases
```

---

## What it validates

1. **Design references**: Use cases should reference existing design documents in "Related Designs" section
2. **Coverage completeness**: Every use case should link to at least one design
3. **Link integrity**: Referenced design files should exist in `docs/architecture/designs/`

---

## Validation procedure

### Step 1 — Find use case files

Scan `docs/use-cases/*.md` for use case files:
- Include all `UC-*.md` files
- Exclude `README.md` and `_template.md`

### Step 2 — Extract design references

For each use case:
1. Parse the "Related Designs" section (markdown heading level 2: `## Related Designs`)
2. Extract all markdown links in that section: `[Design Name](../architecture/designs/<design-name>/design.md)`
3. Parse design names from link paths (e.g., `authentication-and-session-management` from path)

### Step 3 — Validate references

For each referenced design:
1. Resolve path relative to repository root: `docs/architecture/designs/<design-name>/design.md`
2. Check if file exists
3. Track: valid references, invalid references, total references

### Step 4 — Categorize use cases

- ✅ **Complete Coverage**: All referenced designs exist (at least 1 reference)
- ⚠️ **Partial Coverage**: Some referenced designs exist, but some are broken/invalid
- ❌ **No Coverage**: No design references found or all references invalid

### Step 5 — Generate report

List use cases in each category with:
- UC-ID (e.g., `UC-IND-001`)
- Title extracted from filename or first heading
- Reference counts (valid/total)
- Invalid design names (for partial coverage)

Include summary statistics:
- Total use cases analyzed
- Percentage with complete coverage
- Total design references validated

---

## Output format

```
Use Case Coverage Report
========================

✅ Complete Coverage (9 use cases):
  - UC-IND-001: Personal File Backup (4 designs referenced)
  - UC-IND-002: Cross-Device Access (4 designs referenced)
  ...

⚠️ Partial Coverage (1 use case):
  - UC-BIZ-002: Secure File Sharing (3 valid, 1 invalid)
    Invalid: file-sharing-v2

❌ No Coverage (1 use case):
  - UC-NEW-001: Untitled Use Case (no design references found)

Summary: 9/11 use cases have complete design coverage (82%)

Design Coverage Matrix:
  - authentication-and-session-management: 8 use cases
  - cryptographic-primitives: 11 use cases
  - chunking-and-manifest: 10 use cases
  - cloud-synchronisation: 11 use cases
  - file-sharing: 1 use case
  - tauri-ipc-and-frontend: 2 use cases
```

### On failure

```
Use Case Coverage Report
========================

✅ Complete Coverage (10 use cases)

⚠️ Partial Coverage (1 use case):
  - UC-BIZ-002: Secure File Sharing (4 valid, 1 invalid)
    Invalid: file-sharing-v2 → Expected: file-sharing

❌ No Coverage (1 use case):
  - UC-TEST-001: Test Use Case (no Related Designs section found)

Summary: 10/12 use cases have complete design coverage (83%)

Recommendations:
  - Fix broken reference in UC-BIZ-002
  - Add design references to UC-TEST-001
```

---

## When to run

- After adding or modifying use cases
- After creating or restructuring design documents
- Before major milestones or bachelor's report generation
- When unsure if requirements traceability is maintained

---

## Related commands

- `/docs-sync` — Validate SUMMARY.md completeness and detect orphaned files
- `/ssot-check` — Validate rule files reference valid design docs
- `docs/use-cases/README.md` — Use case documentation overview
