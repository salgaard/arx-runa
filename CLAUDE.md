# VoidGate

Zero-knowledge cloud storage. Data leaves encrypted, arrives as opaque blobs, returns readable only on client. Cloud never holds keys, names, or metadata.

## Core
- XChaCha20-Poly1305 AEAD encryption
- Tier 1 (password) or Tier 2 (password + USB key file) — tier selected per vault at creation
- Zero-Trace: RAM-based UI, no temp files
- Fixed-size padded chunks — no file size inference
- BYOC via Rclone

## Stack
- Rust 2024 + Tauri + Leptos/WASM + Tailwind
- Argon2id + HKDF-SHA256 for key derivation (see `docs/architecture/designs/authentication-and-session-management/design.md` for parameters)
- Per-file: random file_key wrapped with key_encryption_key
- XChaCha20-Poly1305 AEAD with mandatory AAD binding (see `docs/architecture/designs/cryptographic-primitives/design.md` for wire format)
- SQLCipher (sqlcipher_key), Rclone (UUID blob names)
- `zeroize`/`secrecy` crates, `mlock`/`VirtualLock` for session keys

## Directories
- `src-tauri/src/{crypto,auth,storage,sharing,memory,ui}/` — Rust modules
- `src/` — Leptos frontend
- `docs/{architecture-decisions,architecture,threat-model,guides}/`
- `.claude/reference/` — rust-patterns.md, code-structure.md

## Naming
No abbreviations: `chunk_index` not `chunk_idx`, `encrypted_buffer` not `enc_buf`. Rust keywords and acronyms (AEAD, KDF, HKDF) exempt.

## Hard rules
- Never write unencrypted sensitive data to disk
- Never commit secrets or key files
- `unsafe` requires `// SAFETY:` comment
- Every chunk AEAD call MUST include AAD (file_id || chunk_index) — see crypto design for construction and exceptions
- Nonces: random CSPRNG only — never sequential/derived
