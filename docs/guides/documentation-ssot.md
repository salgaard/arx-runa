# Arx Runa Documentation SSOT Architecture

> **Single Source of Truth (SSOT)** for technical specifications ensures that every data element is authored in exactly one place, eliminating synchronization errors and maintenance burden.

**Date established**: 2026-04-01  
**Status**: Active (reference-based)

---

## Quick Start for Contributors

**When you change a design document**:

1. Edit the design doc in `docs/architecture/designs/`
2. If a rule summary changed, update the relevant `.claude/rules/*.md` file
3. Run `/copilot-sync` to update GitHub Copilot instructions
4. Commit all changes together

That's it. No extraction, no generation, no pipeline.

---

## Architecture Overview

```
┌──────────────────────────────────────┐
│  docs/architecture/designs/<design-name>/design.md      │  ← CANONICAL SOURCE
│  (human-readable technical specs)    │
└──────────────┬───────────────────────┘
               │
               ├─ Referenced by ──▶ .claude/rules/*.md
               │                    (brief summaries + design doc pointers)
               │
               ├─ Synced to ───────▶ .github/instructions/*.instructions.md
               │                    (via /copilot-sync)
               │
               ├─ Referenced by ──▶ docs/roadmap.md
               │                    (implementation logistics)
               │
               └─ Informs ─────────▶ CLAUDE.md
                                     (high-level principles)
```

**Core principle**: Design documents are authoritative. All other documentation references or summarizes them.

---

## Documentation Layers

### 1. Design Documents (Canonical)

**Location**: `docs/architecture/designs/<design-name>/design.md`

**Role**: Authoritative technical specifications with:
- Complete algorithm specifications
- Wire formats and data structures
- Security analysis and threat considerations
- Quantified trade-off analysis
- Citations to standards (OWASP, NIST, RFCs)

**Examples**:
- `docs/architecture/designs/cryptographic-primitives/design.md` — XChaCha20-Poly1305, HKDF, wire format, AAD, nonces
- `docs/architecture/designs/authentication-and-session-management/design.md` — Argon2id params, key derivation tree
- `docs/architecture/designs/chunking-and-manifest/design.md` — 4 MiB chunk size, padding waste analysis

**When to edit**: When technical specifications change.

---

### 2. AI Rules (Reference-Based)

**Location**: `.claude/rules/*.md`

**Role**: Brief constraint summaries that point to design docs for full specifications.

**Structure**:
```markdown
---
paths:
  - "src-tauri/src/crypto/**"
---

# Crypto module — rules

**Design specification**: `docs/architecture/designs/cryptographic-primitives/design.md`

## Cipher
- `XChaCha20Poly1305` only — 192-bit nonce
- AES-GCM rejected

## Nonces
- 24 bytes via CSPRNG — never sequential
```

**Key files**:
| Rule File | Design Reference |
|-----------|------------------|
| `crypto.md` | cryptographic-primitives.md |
| `auth.md` | authentication-and-session-management.md |
| `storage.md` | chunking-and-manifest.md |

**When to edit**: When a brief constraint summary needs updating.

---

### 3. GitHub Copilot Instructions (Synced)

**Location**: `.github/instructions/*.instructions.md`

**Role**: Path-specific rules for GitHub Copilot CLI.

**Generation**: Synced from `.claude/rules/*.md` via `/copilot-sync`.

**Transformation**: Only the frontmatter key changes (`paths:` → `applyTo:`).

---

### 4. Reference-Based Documents

**Roadmap** (`docs/roadmap.md`): References designs, contains implementation logistics.

**CLAUDE.md**: High-level principles, not parameter-level details.

---

## Contract Surface Standard (for `design.md`)

Every phase design should include a compact **Contract Surface** section that acts as the canonical anchor for derivative docs.

**Purpose**:
- Keep roadmap/sub-phase/diagram docs reference-based
- Avoid duplicate contract text drifting out of sync
- Make cross-phase handoffs explicit

**Required contract fields**:
1. **Interface contract**: command names, trait methods, or public API signatures
2. **Data contract**: schema fields, wire-format fields, and canonical names
3. **Invariant contract**: security and behavioral invariants that must hold
4. **Dependency contract**: which upstream phases/contracts are required

**Template**:
```markdown
## Contract Surface

### Interface contract
- ...

### Data contract
- ...

### Invariant contract
- ...

### Dependency contract
- ...
```

Derivative documents (`docs/roadmap.md`, `sub-phases/*.md`, `diagrams/*.md`) should reference this section using heading anchors rather than restating contract details.

---

## Contract-Change Checklist (Same PR)

Use this checklist when any `## Contract Surface` content changes in a design document. This supplements (does not replace) the workflow below.

- [ ] Update the canonical contract in `docs/architecture/designs/<design-name>/design.md` (`## Contract Surface`).
- [ ] Update affected references in the same PR:
  - [ ] `docs/roadmap.md`
  - [ ] `docs/architecture/designs/<design-name>/sub-phases/*.md`
  - [ ] Related diagrams in `docs/architecture/designs/**/diagrams/` or `docs/architecture/diagrams/`
- [ ] Ensure CI consistency checks pass before merge.

Do not merge contract-surface changes without these same-PR updates and green consistency checks.

---

## Workflow

### Updating a Technical Specification

1. **Edit design document**: Update `docs/architecture/designs/<design-name>/design.md`
2. **Update rule summary** (if needed): Update `.claude/rules/*.md`
3. **Sync to Copilot**: Run `/copilot-sync`
4. **Commit**: All changes together

### Adding a New Constraint

1. **Design first**: Add to design document
2. **Add to rules**: Add a brief summary in the relevant rule file
3. **Sync**: Run `/copilot-sync`

---

## Design Consistency Review Cadence (Quarterly)

Run a lightweight review once per quarter:

- [ ] Confirm each active design `design.md` has a current `## Contract Surface`.
- [ ] Verify roadmap/sub-phase/diagram documents still reference contract anchors (no stale duplicate contract text).
- [ ] Run CI consistency checks; open follow-up issues for any drift.

---

## Benefits

1. **Simplicity**: No extraction, no generation, no pipeline
2. **Single source of truth**: Specs appear once in design docs
3. **AI-friendly**: Agents can read design docs directly
4. **Low maintenance**: Rules are brief pointers, not duplicated specs
5. **Clean design docs**: No extraction markers cluttering the source

---

## Migration History

**2026-04-01**: SSOT architecture established (extraction-based)
**2026-04-02**: Simplified to reference-based architecture
- Deleted `.claude/rule-sources/` directory
- Deleted extraction and generation scripts
- Removed EXTRACT markers from design docs
- Rules now reference designs directly
