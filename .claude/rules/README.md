# AI Rules — VoidGate

This directory contains AI agent rules that enforce constraints during code generation and review.

## Architecture: Reference-Based SSOT

VoidGate uses a **Single Source of Truth (SSOT)** architecture where:

1. **Design documents** (`docs/architecture/designs/<design-name>/design.md`) are the canonical source
2. **Rule files** (this directory) reference design docs and state key constraints
3. **GitHub Copilot instructions** (`.github/instructions/`) are synced from rules via `/copilot-sync`

```
docs/architecture/designs/<design-name>/design.md (CANONICAL SOURCE)
    │
    └─ Referenced by ─▶ .claude/rules/*.md (THIS DIRECTORY)
                            │
                            └─ Synced to ─▶ .github/instructions/*.instructions.md
```

**Key principle**: Design documents are authoritative. Rules point to them for full specifications.

## Rule File Structure

Each `.md` file in this directory contains:

1. **YAML frontmatter**: `paths` defining which files the rule applies to
2. **Design doc reference**: Points to the canonical specification
3. **Constraint summaries**: Brief statements of key requirements

### Example

```markdown
---
paths:
  - "src-tauri/src/crypto/**"
---

# Crypto module — rules

**Design specification**: `docs/architecture/designs/cryptographic-primitives/design.md`

## Cipher
- `XChaCha20Poly1305` only (not `ChaCha20Poly1305`) — 192-bit nonce
- AES-GCM rejected for this project

## Nonces
- 24 bytes via CSPRNG per chunk — never sequential/derived
```

## Current Rule Files

| File | Scope | Design Reference |
|------|-------|------------------|
| `crypto.md` | `src-tauri/src/crypto/**` | cryptographic-primitives.md |
| `auth.md` | `src-tauri/src/auth/**` | authentication-and-session-management.md |
| `storage.md` | `src-tauri/src/storage/**` | chunking-and-manifest.md |
| `rust.md` | `src-tauri/**/*.rs` | General Rust patterns |
| `tauri.md` | `src-tauri/src/ui/**`, tauri.conf.json | IPC and security |
| `memory-protection.md` | `src-tauri/src/memory/**` | Memory safety |
| `leptos.md` | `src/**/*.rs` (frontend) | Leptos patterns |
| `docs.md` | `docs/**` | Documentation standards |

## Workflow

### Modifying Constraints

1. **Edit design document**: Update `docs/architecture/designs/<design-name>/design.md`
2. **Update rule summary**: If the change affects the brief constraint summary, update the rule file
3. **Sync to Copilot**: Run `/copilot-sync` to update GitHub instructions
4. **Commit**: All changes together

### Adding New Constraints

1. **Design first**: Add specification to appropriate design document
2. **Reference in rules**: Add a brief summary pointing to the design doc section
3. **Sync**: Run `/copilot-sync`

## Benefits

1. **Single source of truth**: Specifications appear once in design docs
2. **Minimal maintenance**: Rules are brief summaries, not full specs
3. **Traceability**: Every rule points to its canonical source
4. **AI-friendly**: Agents can read design docs directly for full context

---

**See**: `docs/guides/documentation-ssot.md` for the full SSOT architecture guide.
