---
name: sub-phases
description: >
  Decompose an Arx Runa design document into numbered, sequentially testable
  sub-phases. Use when a design has multiple distinct implementation concerns
  (trait + implementation, schema + pipeline, etc.) that benefit from
  incremental delivery and independent validation. Invokable manually via
  /sub-phases <design-name>.
---

Decompose a design document into a numbered sub-phase roadmap and individual
sub-phase files under `docs/architecture/designs/<design-name>/sub-phases/`.

## When to create sub-phases

Sub-phases are warranted when **any two or more** of these apply:

- Design document exceeds ~120 lines
- Multiple trait/interface boundaries (e.g. trait → mock → real implementation)
- Multi-step flows with distinct, independently testable concerns
- Touches more than two external modules or crates
- Has a security-critical component that warrants isolated review

If only one applies (e.g. a short design with one clear flow), a single
implementation plan is usually sufficient. Don't decompose for its own sake.

---

## Step 1 — Read the design

Read `docs/architecture/designs/<design-name>/design.md` in full.

Also check:
- `docs/roadmap.md` — phase number, deliverables list, dependencies
- `docs/architecture-decisions/` — any ADRs that constrain this design
- The parent phase entry in the roadmap for external dependencies

---

## Step 2 — Identify decomposition points

Look for natural seams in the deliverables list:

| Signal | Likely sub-phase boundary |
|--------|--------------------------|
| Trait definition + mock | Sub-phase 1: trait + mock; Sub-phase 2: real impl |
| Schema + data access logic | Sub-phase 1: schema; Sub-phase 2: pipelines |
| Core logic + error recovery | Sub-phase 2: core; Sub-phase 3: recovery/cleanup |
| Platform-specific branching | One sub-phase per platform if significant |
| Security review required | Always end a sub-phase at a security boundary |

Aim for **2–5 sub-phases**. Fewer than 2 isn't decomposition; more than 5
becomes scheduling overhead.

---

## Step 3 — Assign numbers

Use `<phase>.<sub>` notation matching the parent roadmap phase:

- Phase 0 → 0.1, 0.2, 0.3
- Phase 1 → 1.1, 1.2, 1.3
- Phase 3 → 3.1, 3.2, 3.3

Sub-phases are strictly ordered: each sub-phase must depend only on earlier
sub-phases or earlier roadmap phases.

---

## Step 4 — Create the directory

Create `docs/architecture/designs/<design-name>/sub-phases/` if it doesn't exist.

---

## Step 5 — Write `roadmap.md`

Create `docs/architecture/designs/<design-name>/sub-phases/roadmap.md` using
this structure:

```markdown
# <Design Name> — Sub-Phase Roadmap

**Parent design**: [`design.md`](../design.md)
**Created**: YYYY-MM-DD
**Status**: Draft
**Implementation order**: X.1 → X.2 → X.3 (strict dependencies)

---

## Overview

<One paragraph: what problem this design solves and why it was decomposed.>

**Total sub-phases**: N

**Rationale for decomposition**:
- <Bullet per decomposition signal that applied>

**Implementation strategy**: <One sentence describing the build order logic>

---

## Dependency Graph

```
X.1 (<short name>)
 ↓
X.2 (<short name>)
 ↓
X.3 (<short name>)
```

**Legend**:
- `→` strict dependency (must complete predecessor before starting)

---

## Sub-Phases

1. **[Phase X.1: <Title>](<X.1-kebab-name.md>)**
   - <Key deliverable 1>
   - <Key deliverable 2>
   - **Estimated**: ~NNN lines production code, ~NNN lines tests

2. **[Phase X.2: <Title>](<X.2-kebab-name.md>)**
   ...

---

## Testing Strategy

### Per-Sub-Phase Testing
<How tests are structured and what must pass before advancing.>

**Test types**:
- **Unit tests**: ...
- **Property-based tests**: ... (if applicable)
- **Integration tests**: ...

### Regression Testing
After completing each sub-phase, run:
```bash
cargo test <module>
cargo clippy -- -D warnings
```

### Manual Testing Checklist
- Phase X.1: <Manual check>
- Phase X.2: <Manual check>

---

## Security Review Checkpoints

- **Phase X.N**: Requires `security-reviewer` agent review (<what to check>)
- **Phase X.M**: No security review required (<why safe>)

---

## Documentation Impact

**Files to create/update after sub-phase completion**:
- Phase X.1: <doc impact>
- Phase X.N (final): Update `docs/roadmap.md` to mark Phase X complete

---

## Notes

<Implementation-specific gotchas, cross-cutting concerns, or platform notes.>

---

## References

- **Parent design**: `docs/architecture/designs/<design-name>/design.md`
- **Roadmap entry**: `docs/roadmap.md` Phase X
- **Related phases**: <upstream and downstream phase dependencies>
```

---

## Step 6 — Write individual sub-phase files

Create one file per sub-phase: `<X.N-kebab-title.md>`

### SSOT principle

Sub-phase files describe **how** to implement; `design.md` describes **what** to implement.

- **Reference, don't reproduce**: For anything specified in design.md (dep versions, schemas, wire formats, config values, code blocks), link to the section and instruct the implementer to follow it. Do not copy the spec into the sub-phase file.
- **Own the steps**: Commands to run, files to create, validation sequences, and test commands belong in the sub-phase.
- **Own the gotchas**: Non-obvious constraints, platform quirks, and ordering notes belong in Implementation Notes.

**Reference style** (correct):
> Populate `src-tauri/Cargo.toml` using the [Cryptography dependencies table in `design.md`](../design.md#cryptography-phase-1). Use the exact versions listed there — do not reproduce the list here.

**Reproduction style** (avoid — creates drift):
> - `hkdf = "0.12"`, `sha2 = "0.10"`, `rand = "0.9"` ...

This rule makes sub-phases resilient to design changes: when a spec detail changes in `design.md`, sub-phases need no update because they reference rather than repeat.

Use this structure for each:

```markdown
# Phase X.N: <Title>

**Parent roadmap**: [roadmap.md](roadmap.md)
**Design sections**: <Section names linking to design.md anchors>
**Depends on**: <Previous sub-phase and/or cross-phase dependencies>

---

## Deliverables

1. <Concrete deliverable — file path, struct name, function signature, or test coverage>
2. ...

---

## Validation Checkpoint

**Automated tests**:
```bash
cargo test <module>::<submodule>
```
All tests must pass.

**Manual verification**:
- <Specific thing to inspect or run manually>

**Acceptance criteria**:
- <Pass/fail criterion>

---

## Estimated Scope

- **Production code**: ~NNN lines
- **Test code**: ~NNN lines

---

## Implementation Notes

- <Concrete tip — specific API, gotcha, or non-obvious constraint>

---

## Security Review

**Required** / **Not required** — <reason>.

If required: Invoke `security-reviewer` agent after implementation. Check:
- <Specific thing to verify>

---

## Next Sub-Phase

**[Phase X.N+1: <Title>](<filename.md>)**
- Depends on: <this sub-phase + any other deps>
- Implements: <one-line summary>
```

Omit the "Next Sub-Phase" section on the final sub-phase.

---

## Step 7 — Update the design document

Add a reference to the sub-phase roadmap in `design.md`, directly below the
status/date header:

```markdown
> **Sub-phase roadmap**: [`sub-phases/roadmap.md`](sub-phases/roadmap.md)
```

---

## Step 8 — Update `docs/roadmap.md`

Find the phase entry and add a sub-phase roadmap reference below the design
doc reference line:

```markdown
**Sub-phase roadmap**: [`docs/architecture/designs/<design-name>/sub-phases/roadmap.md`](architecture/designs/<design-name>/sub-phases/roadmap.md) (recommended for incremental implementation)
```

---

## Step 9 — Confirm

Output:
```
Sub-phases created: X.1, X.2, X.3
Roadmap: docs/architecture/designs/<design-name>/sub-phases/roadmap.md
```
