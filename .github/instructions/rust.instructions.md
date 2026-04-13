---
applyTo: "src-tauri/**/*.rs"
---


# Rust — project-wide rules

## Structure
- One concern per file (e.g., `encrypt_chunk.rs`, `key_source.rs`)
- Module layout: `mod.rs` (re-exports only) + `error.rs` + `types/` subfolder
- Newtypes in `types/`: `FileKey`, `ChunkIndex`, `NodeId`, `VaultId`, `BlobName`

## Patterns
- Newtype wrappers for domain types — see `.claude/reference/rust-patterns.md`
- `ZeroizeOnDrop` + `SecretBox<T>` for all key types
- Borrowed signatures: `&[u8]` not `&Vec<u8>`, `&str` not `&String`
- Trait boundaries: `CloudTransport`, `KeySource`, `MetadataStore`

## Error handling
- No `unwrap()`/`expect()` in production — use `?`
- Library modules: `thiserror` enums; UI layer: `anyhow::Result`

## Documentation
- Every `fn`, `struct`, `enum`, `trait` needs `///` doc-comment
- No inline `//` comments in function bodies — code must be self-documenting

## Memory
- Sensitive types: `ZeroizeOnDrop` + `secrecy::SecretBox<T>`
- Encrypt/decrypt in-place on `&mut [u8]` — no plaintext copies

## I/O
- Stream via `BufReader`/`BufWriter` — never load entire files into memory
- All I/O async (`tokio::io`) — never block Tauri thread

## Unsafe
- Requires `// SAFETY:` comment explaining soundness and invariants

## Testing
- Name: `test_<unit>_<scenario>_<expected_outcome>`
- `unwrap()`/`expect()` allowed only in `#[cfg(test)]`
- Every `thiserror` variant must have a test that triggers it

## Formatting
- `cargo fmt` + `cargo clippy -- -D warnings` before commit
