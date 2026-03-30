---
paths:
  - "src-tauri/**/*.rs"
---

# Rust — project-wide scoped rules

These rules apply to all `.rs` files under `src-tauri/`.

## Code structure
- One-concern-per-file: each `.rs` file focuses on a single type, trait, or
  function (e.g., `encrypt_chunk.rs`, `key_source.rs`)
- Module layout: `mod.rs` + `error.rs` + `types/` subfolder
- `mod.rs` re-exports public API only — internal helpers stay private
- Newtypes live in `types/` subfolders: `crypto/types/file_key.rs`

## Patterns
- Newtype pattern for domain types: `FileId`, `ChunkIndex`, `NodeId`, `VaultId`,
  `BlobName`, `FileKey`, `KeyEncryptionKey`
- RAII guards with `ZeroizeOnDrop` for all key types
- Builder pattern for complex configs: `VaultConfig::builder().build()`
- Borrowed types in signatures: `&[u8]` not `&Vec<u8>`, `&str` not `&String`
- `mem::take` for key rotation and state transitions
- Trait boundaries for external deps: `CloudTransport`, `KeySource`, `MetadataStore`
- Return consumed argument on error: if a fallible function moves an argument,
  return it inside the error so callers can retry without cloning
- Pass variables to closure with rebinding: use a scoped block to control what
  closures capture (move, clone, or borrow) — keeps async task setup clear
- Temporary mutability: after setup, rebind `let mut x` to `let x` so the
  compiler enforces immutability for the rest of the scope
- On-stack dynamic dispatch: use `&mut dyn Trait` instead of `Box<dyn Trait>`
  when both branches return the same trait — avoids heap allocation

## Error handling
- No `unwrap()` or `expect()` in production code — use `?` and propagate
- `unwrap()` and `expect()` are permitted in `#[cfg(test)]` code
- Library modules (`crypto/`, `auth/`, `storage/`): `thiserror` with typed
  error enums
- Tauri command layer (`ui/`): `anyhow::Result` to collect and forward errors

## Documentation
- No inline comments (`//`) inside function bodies — names and structure
  must be self-documenting
- Every `fn`, `struct`, `enum`, `trait` — public or private — requires a
  `///` doc-comment explaining: purpose, arguments, return value, errors
- Exception: trivial getters/setters and test helpers may use brief one-liners
- For crypto functions: include in the doc-comment which threat is addressed
  and what the caller's invariants must be

## Naming
- No abbreviations — `chunk_index` not `chunk_idx`, `encrypted_buffer` not
  `enc_buf`, `master_key` not `mk`
- Rust keywords (`impl`, `fn`, `pub`, `mod`) are exempt
- Established acronyms (AEAD, KDF, HKDF, CSPRNG, UUID, AAD, BLAKE3) are fine
- Full readable words for module names, file names, variable names

## Visibility
- Default to `pub(crate)` or private — only use `pub` for the module's
  external API surface
- Re-export the public API from `mod.rs`
- Use `#[non_exhaustive]` on public error enums and config structs to allow
  adding variants/fields without breaking downstream — external code must
  handle unknown variants with `_` wildcard

## Memory
- Sensitive types must implement `zeroize::ZeroizeOnDrop`
- Wrap key material in `secrecy::Secret<T>`
- Never store key material in `String` or `Vec<u8>` without zeroize protection
- Encrypt/decrypt in-place on mutable `&mut [u8]` buffers

## I/O
- Stream files via `BufReader`/`BufWriter` — never read a complete file into
  a `Vec<u8>`
- All file I/O must be async (`tokio::io`) — never block the Tauri thread

## Unsafe
- Do not use `unsafe` without a `// SAFETY:` comment that explains:
  - Why this is sound
  - What invariants the caller must uphold
  - Why a safe alternative is not viable

## Module design
- Define traits for external boundaries: `CloudTransport`, `KeySource`,
  `MetadataStore`
- Code depends on the trait, not the concrete type
- Prefer `impl Trait` or `dyn Trait` over deep struct hierarchies
- Struct decomposition: if borrow checker complains about borrowing multiple
  fields, consider splitting the struct into smaller logical units

## Testing
- Naming: `test_<unit>_<scenario>_<expected_outcome>`
- Unit tests: inline `#[cfg(test)]` module at the bottom of each source file
- Integration tests: `tests/` directory at crate root for cross-module flows
- `unwrap()` and `expect()` are permitted in `#[cfg(test)]` code only
- Every `thiserror` error variant must have at least one test that triggers it
- Use `proptest` for property-based testing of crypto round-trips
- Use `tempfile` for filesystem tests — never write to real paths in tests
- Mock external boundaries via trait implementations, not internal functions
- Verify sensitive buffers contain zeros after drop via unsafe pointer inspection
- Test chunk boundary cases: smaller than chunk_size, exactly chunk_size, one byte over

## Formatting and linting
- `cargo fmt` before committing
- `cargo clippy -- -D warnings` must pass with zero warnings
- Do NOT use `#![deny(warnings)]` in source code — new compiler versions may
  add warnings and break builds. Use `RUSTFLAGS="-D warnings"` in CI instead

## Anti-patterns to avoid
- **Clone to satisfy borrow checker**: don't use `.clone()` to make borrow
  errors disappear — understand ownership and fix the design
- **Deref polymorphism**: do NOT implement `Deref` to "inherit" methods from
  another type. `Deref` is for smart pointers, not fake inheritance
