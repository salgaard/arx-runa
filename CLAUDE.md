# Arx Runa

Zero-knowledge, bring-your-own-cloud file encryption tool with client-side encryption. Data is encrypted on the client before upload, arrives as opaque blobs, and returns readable only on the client. Cloud never holds keys, names, or metadata.

Canonical design sources: phase `design.md` files in `docs/architecture/designs/` and `docs/architecture/design-invariants.md`.
Contract anchors: each phase `design.md` `## Contract Surface` section is the canonical contract.
Propagation: canonical contract changes must be propagated to dependent sub-phases, diagrams, rules, skills, agents, and instructions.

## Naming
No abbreviations: `chunk_index` not `chunk_idx`, `encrypted_buffer` not `enc_buf`. Rust keywords and acronyms (AEAD, KDF, HKDF) exempt.

## Hard rules
 - Never write unencrypted sensitive data to disk
 - Never commit secrets or key files

## Platform compatibility
 - Arx Runa targets Windows, macOS, and Linux.
 - New features and refactors must preserve behavior across all three platforms.
 - Platform-specific code is allowed only with an equivalent implementation on other targets, or an explicit documented limitation in the canonical design.

## After implementing new changes
 - 1. run: cargo clippy --workspace --all-targets --all-features -- -D warnings
 - 2. fix issues
 - 3. run: cargo fmt --all
 - 4. dont git add or commit anything - user does manually