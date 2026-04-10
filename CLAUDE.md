# Arx Runa

Zero-knowledge, bring-your-own-cloud file encryption tool with client-side encryption. Data is encrypted on the client before upload, arrives as opaque blobs, and returns readable only on the client. Cloud never holds keys, names, or metadata.

Full specs: `docs/architecture/designs/` — crypto, auth/session, chunking/manifest, cloud sync, file sharing, and Tauri IPC/frontend.
Reference: `.claude/reference/rust-patterns.md`, `.claude/reference/code-structure.md`

## Naming
No abbreviations: `chunk_index` not `chunk_idx`, `encrypted_buffer` not `enc_buf`. Rust keywords and acronyms (AEAD, KDF, HKDF) exempt.

## Hard rules
- Never write unencrypted sensitive data to disk
- Never commit secrets or key files
- `unsafe` requires `// SAFETY:` comment
- Every chunk AEAD call MUST include AAD (file_id || chunk_index) — see crypto design for construction and exceptions
- Nonces: random CSPRNG only — never sequential/derived
