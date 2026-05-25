# Decision-001: Code Structure and Patterns

**Date**: 2026-03-30
**Status**: Accepted

---

## Context

Arx Runa is a security-critical application where readability, maintainability,
and correctness are paramount. As the codebase grows through the roadmap phases,
we need consistent conventions for:

1. **Folder and file organisation** — how modules are structured
2. **Rust coding patterns** — idioms that improve readability and safety
3. **Type safety** — preventing accidental misuse of similar-looking values

Without explicit conventions, different phases may adopt inconsistent styles,
making the codebase harder to navigate and review.

The project values:
- Readable, intuitive code and structure
- Explicit over implicit — no magic
- Self-documenting code through descriptive names
- Compile-time safety where possible

---

## Decision

### 1. File Granularity: One-Concern-Per-File

Each `.rs` file focuses on a single type, trait, or function. This improves
navigability and keeps files small.

**Examples:**
- `encrypt_chunk.rs` — contains `encrypt_chunk` function and its private helpers
- `key_source.rs` — contains `KeySource` trait and `FileKeySource` impl
- `types/file_key.rs` — contains `FileKey` newtype only

**Rejected alternative:** Flat files with multiple types/functions. While fewer
files, they become harder to navigate and encourage coupling.

### 2. Module Structure

Each module under `src-tauri/src/` follows:
```
module_name/
├── mod.rs          # Public API re-exports only
├── error.rs        # Module-specific thiserror enum
├── types/          # Newtype wrappers
│   ├── mod.rs
│   └── type_name.rs
└── concern.rs      # One file per concern
```

`mod.rs` is minimal — only `pub use` statements. Internal helpers are not
re-exported.

### 3. Newtype Location: `types/` Subfolder Per Module

Domain-specific newtypes live in `types/` subfolders rather than alongside
their primary use:
- `crypto/types/file_key.rs`
- `storage/types/chunk_index.rs`

**Rationale:** Groups related types, makes imports predictable, separates type
definitions from business logic.

**Rejected alternative:** Top-level shared `types` crate. Adds workspace
complexity for modest benefit at current scale.

### 4. Adopted Patterns

| Pattern | Use Case |
|---------|----------|
| **Newtype** | `FileId`, `ChunkIndex`, `NodeId`, `VaultId`, `BlobName`, all key types |
| **RAII / ZeroizeOnDrop** | All key types, database connections, session handles |
| **Builder** | Complex configs (`VaultConfig`, `SyncOptions`) |
| **Borrowed types** | Function parameters (`&[u8]` not `&Vec<u8>`) |
| **mem::take** | Key rotation, state machine transitions |
| **Trait boundaries** | External deps (`CloudTransport`, `KeySource`, `MetadataStore`) |
| **Default trait** | Partial struct initialisation |

### 5. Timing: Incremental Adoption

Structure is applied as each roadmap phase is implemented — not as a big-bang
refactor. Phase 0 establishes the skeleton; subsequent phases add their
module-specific types and files.

---

## Consequences

### Positive

- **Compile-time safety**: Newtypes prevent passing `ChunkIndex` where `FileId`
  is expected — bugs caught before runtime
- **Navigability**: One-concern-per-file means file names directly indicate
  contents; easy to find code
- **Testability**: Trait boundaries enable mock-based testing without live
  Rclone or USB hardware
- **Consistency**: All modules follow the same structure; contributors know
  where to find things

### Negative

- **More files**: One-concern-per-file creates more `.rs` files than a flat
  approach. Mitigated by clear naming and IDE navigation.
- **Boilerplate for newtypes**: Each newtype requires `new()`, `as_inner()`,
  and possibly trait impls. Accepted trade-off for type safety.
- **Learning curve**: Contributors must learn the conventions. Mitigated by
  documenting in CLAUDE.md and this ADR.

### Risks

- **Over-engineering**: Applying patterns where a simple struct would suffice.
  Mitigation: only introduce patterns when they solve a concrete problem.
- **Inconsistency during transition**: Early phases may not fully adopt the
  structure. Mitigation: incremental adoption with per-phase review.

---

## References

- [Rust Design Patterns — Newtype](https://rust-unofficial.github.io/patterns/patterns/behavioural/newtype.html)
- [Rust Design Patterns — Builder](https://rust-unofficial.github.io/patterns/patterns/creational/builder.html)
- [Rust Design Patterns — RAII Guards](https://rust-unofficial.github.io/patterns/patterns/behavioural/RAII.html)
- [Rust API Guidelines — Type Safety](https://rust-lang.github.io/api-guidelines/type-safety.html)
- Prior art: KeePassXC (type-safe key handling), Signal Protocol (trait boundaries)
