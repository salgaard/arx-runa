# VoidGate — Copilot Instructions

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
- KDF: Argon2id (OWASP minimums: m>=19456, t>=2, p=1)
- Key derivation tree: Argon2id produces master_key; HKDF-SHA256 (RFC 5869)
  derives purpose-specific keys:
  - `hkdf(master, info=b"voidgate-chunk-encryption")` -> chunk_key
  - `hkdf(master, info=b"voidgate-sqlcipher")` -> sqlcipher_key
  - `hkdf(master, info=b"voidgate-manifest-backup")` -> manifest_key
- Encryption: XChaCha20-Poly1305 (AEAD) via `chacha20poly1305` crate
  (`XChaCha20Poly1305` type) — chosen for 192-bit nonce enabling safe random
  nonce generation per chunk without state tracking, and for defensive
  robustness against nonce-reuse (less catastrophic than AES-256-GCM)
- Chunk wire format: [24-byte nonce | ciphertext | 16-byte Poly1305 tag]
  - Nonce: 192-bit, random per chunk via CSPRNG (birthday bound at 2^96)
  - AAD: file_id || chunk_index bound as authenticated associated data to
    prevent chunk reordering/swapping by a malicious cloud provider
  - BLAKE3 checksum per encrypted blob for pre-decrypt integrity verification
- Local DB: SQLite + SQLCipher (keyed via sqlcipher_key, never plaintext)
  - Manifest schema: `nodes` (virtual filesystem tree), `chunks` (blob
    mapping with chunk_index, blob_name, BLAKE3 checksum), `manifest_meta`
    (vault_id, schema_version, snapshot_counter)
- Manifest cloud backup: SQLCipher DB exported and encrypted with
  manifest_key, uploaded as a blob. Enables recovery on new devices
- Vault header: unencrypted JSON alongside manifest blob in cloud — contains
  vault_id, schema_version, argon2_salt, argon2_params. Required for
  bootstrap on new devices (salt needed before key derivation)
- Cloud transport: Rclone
- Blob naming: random UUID v4 per chunk — no relation to file identity
- Memory safety: `zeroize` crate (compiler-resistant zeroing), `secrecy`
  crate (`Secret<T>` wrapper), `mlock` (Linux) / `VirtualLock` (Windows)
- Session model: USB key file read once at login, session keys held in
  mlocked memory. Session timeout zeroes keys, requires re-auth with
  password + USB. USB must be present at session start, not per-operation
- Threat model boundary: mlock does NOT protect against cold boot attacks
  or a compromised OS kernel — explicitly out of scope

## Coding standards
- No `unwrap()` or `expect()` in production paths — use `?` and `thiserror`
- All sensitive types implement `ZeroizeOnDrop`
- Crypto primitives only from audited crates: `chacha20poly1305`, `argon2`,
  `hkdf`, `blake3`, `rand` (all RustCrypto or established ecosystem crates)
- Never log plaintext filenames, file contents, or key material
- Security-critical functions require doc comments explaining the threat model
- Prefer explicit over implicit — no magic

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

## Module design
- Default to private — only `pub` what the module's external API requires.
  Internal helpers, intermediate types, and implementation details stay
  private. Re-export the public surface from `mod.rs` (or the module root)
- Define traits for external boundaries: `CloudTransport` (Rclone),
  `KeySource` (USB key file), `MetadataStore` (SQLCipher). Modules depend
  on the trait, not the concrete implementation — enables mock-based testing
  and implementation swapping without rewiring callers
- Prefer composition via traits over deep type hierarchies — Rust has no
  inheritance; use trait objects (`dyn Trait`) or generics (`impl Trait`)
  where polymorphism is needed

## Documentation standards
- No inline comments (`//`) inside function bodies — code must be
  self-documenting through descriptive names and clear structure
- Every public and private fn, struct, enum, and trait MUST have a doc-comment
  (`///`) explaining: purpose, arguments, return value, and possible errors
- Exception: trivial getters/setters and test helpers may use brief one-liners

## Error handling
- Library/module layer (`src-tauri/src/crypto/`, `src-tauri/src/auth/`,
  `src-tauri/src/storage/`): use `thiserror` with typed error enums per module
- Application layer (Tauri commands in `src-tauri/src/ui/`): use `anyhow` to
  collect and convert errors for IPC responses
- Errors returned to the frontend MUST be sanitised — no partial keys, no
  plaintext file paths, no memory addresses. Return a generic error code +
  user-safe message; log details server-side only if logging is enabled

## I/O and memory
- Never load an entire file into RAM — process in chunks via `BufReader` /
  `BufWriter` and stream through the encrypt/decrypt pipeline
- Use async I/O (`tokio::io`) for all file operations to avoid blocking
  the Tauri UI thread
- Perform encryption and decryption in-place on mutable buffers where
  possible — minimise extra copies of plaintext in memory

## Testing standards
- Unit tests: inline `#[cfg(test)]` module at the bottom of each source file
- Integration tests: `tests/` directory at crate root for cross-module flows
  (e.g., full encrypt -> upload -> download -> decrypt pipeline)
- Naming: `test_<unit>_<scenario>_<expected_outcome>` — descriptive enough
  to understand the test without reading the body
- `unwrap()` and `expect()` are permitted in test code (not production)
- Every `thiserror` error variant must have at least one test that triggers it
- Crypto modules require adversarial tests: corrupted ciphertext, truncated
  chunks, AAD mismatch, wrong key, tag tampering
- Memory-sensitive code requires zeroize verification tests: assert buffers
  are zeroed after `drop()` via `unsafe` pointer inspection in tests
- Chunk boundary cases: 0 bytes, 1 byte, chunk_size-1, chunk_size,
  chunk_size+1, exact multiples
- Use `proptest` for property-based testing of crypto round-trips
- Use `tempfile` for filesystem tests — never write to real paths in tests
- Mock external boundaries via trait implementations, not internal functions

## Test dependencies (dev-dependencies in src-tauri/Cargo.toml)
- `proptest` — property-based testing (random inputs, automatic shrinking)
- `tempfile` — temporary files and directories for I/O tests
- `assert_matches` — ergonomic error variant assertions

## Key directories
(update after `cargo tauri init` scaffolding)
- `src-tauri/src/crypto/`   — encryption, KDF, key management
- `src-tauri/src/auth/`     — USB key file, Argon2id, session keys
- `src-tauri/src/storage/`  — chunking, SQLCipher metadata, sync
- `src-tauri/src/ui/`       — Tauri commands and frontend bridge
- `src/`                    — frontend (web UI, ignore for Rust context)
- `docs/architecture-decisions/` — Architecture Decision Records
- `docs/architecture/`     — system design, key derivation, data flow
- `docs/threat-model/`     — threat model and security boundaries
- `docs/guides/`           — development setup, workflows

## Core dependencies (src-tauri/Cargo.toml)
Crypto & security:
- `chacha20poly1305` — XChaCha20-Poly1305 AEAD encryption
- `argon2` — Argon2id KDF
- `hkdf` — HKDF-SHA256 key derivation (RFC 5869)
- `blake3` — BLAKE3 checksums for blob integrity
- `rand` — CSPRNG for nonce and UUID generation
- `zeroize` — compiler-resistant zeroing of sensitive memory
- `secrecy` — `Secret<T>` wrapper to prevent accidental exposure

Storage & I/O:
- `rusqlite` (feature `bundled-sqlcipher`) — SQLCipher-encrypted SQLite
- `tokio` (features `rt-multi-thread`, `io-util`, `fs`) — async runtime
- `uuid` (feature `v4`) — random UUID generation for node_id, chunk_id,
  blob_name

Application:
- `tauri` — application framework
- `thiserror` — typed errors for library modules
- `anyhow` — error handling for Tauri command layer
- `serde` + `serde_json` — serialisation (vault header, IPC)

## Hard rules
- Never write sensitive data to /tmp or any persistent path unencrypted
- Never commit secrets, key files, or test credentials
- Do not use `unsafe` without an explicit safety comment explaining why it
  is sound and what the invariants are
- Every AEAD encrypt/decrypt call MUST include AAD (file_id || chunk_index)
- Nonces MUST be generated randomly via CSPRNG — never sequential or derived

## Note on `.claude/skills/`
The `.claude/skills/` directory does not exist in this project. No skills
to reference.

## Translation notes (from Claude Code environment)
This file mirrors CLAUDE.md. The following Claude Code concepts have no
direct Copilot equivalent and are documented here for awareness:
- **settings.json hooks** (PostToolUse clippy auto-run, PreToolUse
  pipe-to-shell blocking, sensitive file access blocking) — Copilot has no
  hook system. Rely on CI and pre-commit hooks instead.
- **permissions.deny / permissions.ask** — Copilot has no permission model.
  Sensitive file exclusion must be enforced via `.gitignore` and CI checks.
- **output-styles** (report-mode.md) — Copilot has no output style switching.
  Use the report-writing prompt instead when academic register is needed.
- **agent memory** (`.claude/memory/MEMORY.md`) — Copilot agents do not
  persist memory across sessions. Architecture decisions and gotchas are
  documented in `docs/` and inline in instructions instead.
- **Sub-agent routing** (parallel/sequential/background dispatch) — Copilot
  does not support multi-agent orchestration. Prompts reference agents by
  name but orchestration is manual.
