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
