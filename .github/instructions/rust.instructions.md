---
applyTo: "src-tauri/**/*.rs"
---

# Rust — project-wide scoped instructions

These rules apply to all `.rs` files under `src-tauri/`.

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

## Formatting and linting
- `cargo fmt` before committing
- `cargo clippy -- -D warnings` must pass with zero warnings
