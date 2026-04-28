---
paths:
  - "src-tauri/**/*.rs"
---

# Rust

- Layout: one concern per file; `mod.rs` re-exports only + `error.rs` + `types/` subfolder
- Newtypes in `types/`: `FileKey`, `ChunkIndex`, `NodeId`, `VaultId`, `BlobName` — see `.claude/reference/rust-patterns.md`
- `ZeroizeOnDrop` + `secrecy::SecretBox<T>` for all key types; encrypt/decrypt in-place on `&mut [u8]`
- Borrowed signatures: `&[u8]` not `&Vec<u8>`, `&str` not `&String`
- Trait boundaries: `CloudTransport`, `KeySource`, `MetadataStore`
- No `unwrap()`/`expect()` in production — use `?`; library: `thiserror` enums; UI: `anyhow::Result`
- `///` on every `fn`, `struct`, `enum`, `trait`; no `//` in function bodies
- Stream via `BufReader`/`BufWriter`; all I/O async (`tokio::io`) — never block Tauri thread
- `unsafe` requires `// SAFETY:` comment explaining soundness and invariants
- Test naming: `test_<unit>_<scenario>_<expected_outcome>`; `unwrap()`/`expect()` allowed only in `#[cfg(test)]`
- Every `thiserror` variant must have a test that triggers it
- `cargo fmt` + `cargo clippy -- -D warnings` before commit
