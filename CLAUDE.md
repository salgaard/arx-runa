# Arx Runa

Zero-knowledge, bring-your-own-cloud file encryption tool with client-side encryption. Data is encrypted on the client before upload, arrives as opaque blobs, and returns readable only on the client. Cloud never holds keys, names, or metadata.

Full specs: `docs/architecture/designs/` — crypto, auth/session, chunking/manifest, cloud sync, file sharing, and Tauri IPC/frontend.
References: `.claude/reference/rust-patterns.md`, `.claude/reference/code-structure.md`
Cross-phase invariants: `docs/architecture/design-invariants.md`
Contract anchors: each phase `design.md` `## Contract Surface` section is canonical; sub-phases should reference it instead of duplicating.

## Naming
No abbreviations: `chunk_index` not `chunk_idx`, `encrypted_buffer` not `enc_buf`. Rust keywords and acronyms (AEAD, KDF, HKDF) exempt.

## Hard rules
- Never write unencrypted sensitive data to disk
- Never commit secrets or key files
- `unsafe` requires `// SAFETY:` comment
