# VoidGate

Zero-knowledge cloud storage. Data leaves encrypted, arrives as opaque blobs, returns readable only on client. Cloud never holds keys, names, or metadata.

## Core
- XChaCha20-Poly1305 AEAD encryption
- USB key file + password (hardware MFA) — password alone cannot compromise
- Zero-Trace: RAM-based UI, no temp files
- Fixed-size padded chunks — no file size inference
- BYOC via Rclone

## Stack
- Rust 2024 + Tauri + Leptos/WASM + Tailwind
- Argon2id (m≥19456, t≥2, p=1) → HKDF-SHA256 → {key_encryption_key, sqlcipher_key, manifest_key}
- Per-file: random file_key wrapped with key_encryption_key
- Wire: [24B nonce | ciphertext | 16B tag], AAD=file_id||chunk_index, BLAKE3 pre-check
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
- Every AEAD call MUST include AAD (file_id || chunk_index)
- Nonces: random CSPRNG only — never sequential/derived
