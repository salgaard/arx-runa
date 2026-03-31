# VoidGate

## What this is
VoidGate is a zero-knowledge cloud storage system. The core philosophy: data
leaves the client encrypted, arrives at the cloud as opaque blobs, and comes
back readable only on the client. The gate is the trust boundary — the cloud
never holds keys, filenames, folder structure, or metadata.

Core pillars:
- Client-side encryption: XChaCha20-Poly1305 (AEAD)
- Hardware MFA via USB key file — password alone cannot compromise data
- Zero-Trace — minimise forensic artifacts on host (RAM-based UI, no temp files)
- Fixed-size uniformly padded chunks — cloud cannot infer file sizes or count
- Bring Your Own Cloud via Rclone — broad provider support, no lock-in

## Stack
- Language: Rust (edition 2024)
- UI: Tauri (web frontend + Rust backend)
- Frontend: Leptos (Rust + WASM) + Tailwind CSS — see ADR-002 for rationale
- KDF: Argon2id (OWASP minimums: m≥19456, t≥2, p=1)
- Key derivation tree: HKDF-SHA256 (RFC 5869) from Argon2id master_key:
  - `hkdf(master, info=b"voidgate-key-encryption")`  → key_encryption_key
  - `hkdf(master, info=b"voidgate-sqlcipher")`        → sqlcipher_key
  - `hkdf(master, info=b"voidgate-manifest-backup")`  → manifest_key
  - Per-file: random 256-bit `file_key` via CSPRNG, wrapped with key_encryption_key
- Encryption: XChaCha20-Poly1305 via `chacha20poly1305` crate (`XChaCha20Poly1305` type)
- Chunk wire format: [24-byte nonce | ciphertext | 16-byte Poly1305 tag]
  - AAD: file_id || chunk_index bound on every AEAD call
  - BLAKE3 checksum over encrypted blob for pre-decrypt integrity check
- Local DB: SQLite + SQLCipher (keyed via sqlcipher_key)
- Manifest cloud backup: encrypted with manifest_key, uploaded as blob
- Vault header: unencrypted JSON in cloud (vault_id, argon2_salt, argon2_params,
  key_file_blake3) — needed to bootstrap on new devices
- Cloud transport: Rclone; blob names: random UUID v4
- Memory safety: `zeroize` + `secrecy` crates; `mlock`/`VirtualLock` for session keys
- Session: USB key file read once at login, keys zeroed on timeout or re-auth
- Threat model boundary: mlock does NOT protect against cold boot or compromised kernel
- Rationale for all decisions: `.claude/memory/architecture_rationale.md`
- Code patterns and examples: `.claude/reference/rust-patterns.md`
- Module layout and target structure: `.claude/reference/code-structure.md`

## Naming conventions
- No abbreviations in names — code, files, commands, agents, directories
  must all use full readable words. Examples:
  - `implement`, not `impl` (except Rust's `impl` keyword)
  - `architecture-decisions`, not `adr`
  - `documentation-writer`, not `doc-writer`
  - `chunk_index`, not `chunk_idx`
  - `encrypted_buffer`, not `enc_buf`
- Rust language keywords (`impl`, `fn`, `pub`) are obviously exempt
- Well-established acronyms used as proper nouns are fine: AEAD, KDF,
  HKDF, CSPRNG, UUID, AAD, BLAKE3

## Key directories
- `src-tauri/src/crypto/`   — encryption, KDF, key management
- `src-tauri/src/auth/`     — USB key file, Argon2id, session keys
- `src-tauri/src/storage/`  — chunking, SQLCipher metadata, sync
- `src-tauri/src/sharing/`  — identity (X25519), contacts, share packages, revocation
- `src-tauri/src/memory/`   — secure buffers, mlock/VirtualLock, platform-specific memory protection
- `src-tauri/src/ui/`       — Tauri commands and frontend bridge
- `src/`                    — frontend (web UI, ignore for Rust context)
- `docs/architecture-decisions/`   — Architecture Decision Records
- `docs/architecture/`             — system design, key derivation, data flow
- `docs/threat-model/`             — threat model and security boundaries
- `docs/guides/`                   — development setup, workflows
- `docs/architecture/diagrams/`    — Mermaid diagrams (use `/diagram` to generate)
- `docs/architecture/designs/`     — detailed per-phase design documents
- `docs/report-log/`               — bachelor report log entries (use `/report-note` to capture)

## Sub-agent routing
Parallel dispatch — ALL must be true:
- 3+ independent tasks with no shared state
- Clear file boundaries, no overlap

Sequential dispatch — ANY triggers it:
- Task B depends on output from task A
- Shared files or mutable state
- Scope unclear — explore first

Background dispatch:
- Research, doc lookups, security audits
- Results are not immediately blocking

## Hard rules
- Never write sensitive data to /tmp or any persistent path unencrypted
- Never commit secrets, key files, or test credentials
- Do not use `unsafe` without an explicit safety comment explaining why it
  is sound and what the invariants are
- Every AEAD encrypt/decrypt call MUST include AAD (file_id || chunk_index)
- Nonces MUST be generated randomly via CSPRNG — never sequential or derived
