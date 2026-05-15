# Global Design Invariants

This reference captures cross-phase contracts that must stay consistent across architecture designs and implementation phases. It summarizes invariants only; canonical details remain in the linked design documents.

## Cross-Phase Invariants

### 1) Chunk AEAD AAD rule (`file_id || chunk_index`)

**Invariant**: Every chunk AEAD encrypt/decrypt operation includes AAD as `file_id || chunk_index` (`chunk_index` encoded as big-endian `u32`) to bind ciphertext to both file identity and chunk position.

**Source designs**:
- [Cryptographic Primitives](designs/cryptographic-primitives/design.md)
- [Chunking & Manifest](designs/chunking-and-manifest/design.md)

### 2) Nonce policy (CSPRNG, non-sequential)

**Invariant**: Nonces for XChaCha20-Poly1305 are random 24-byte values generated via CSPRNG. Sequential, counter-based, or derived nonce strategies are disallowed.

**Source designs**:
- [Cryptographic Primitives](designs/cryptographic-primitives/design.md)

### 3) HKDF constants (`arx-runa-v1` + info strings)

**Invariant**: Vault key derivation uses HKDF-SHA256 with fixed salt `b"arx-runa-v1"` and canonical info strings `b"arx-runa-key-encryption"`, `b"arx-runa-sqlcipher"`, and `b"arx-runa-manifest-backup"`. New derived keys must use distinct new info strings without changing existing constants.

**Source designs**:
- [Cryptographic Primitives](designs/cryptographic-primitives/design.md)
- [Authentication & Session Management](designs/authentication-and-session-management/design.md)

### 4) Chunk size contract (`chunk_size_bytes`)

**Invariant**: `chunk_size_bytes` is set at vault creation, stored in `manifest_meta`, and validated at vault open. Default is 4 MiB (4,194,304 bytes). Configurable range is 128 KiB (131,072 bytes) to 64 MiB (67,108,864 bytes).

**Source designs**:
- [Chunking & Manifest](designs/chunking-and-manifest/design.md)

### 5) Vault path validation contract (allowlist + root-escape rejection)

**Invariant**: User-supplied vault-relative paths must pass centralized allowlist validation before use and must reject traversal segments (`..`), absolute paths, and control characters.

**Source designs**:
- [Tauri IPC & Frontend](designs/tauri-ipc-and-frontend/design.md)
- [Cloud Synchronisation](designs/cloud-synchronisation/design.md)

### 6) IPC sensitive-input handling at Rust boundary

**Invariant**: IPC handlers that receive sensitive `String` inputs (passwords) must immediately copy them into `Zeroizing<Vec<u8>>`, scrub the Rust `String` backing bytes, and drop the original `String` before calling deeper services.

**Source designs**:
- [Tauri IPC & Frontend](designs/tauri-ipc-and-frontend/design.md)
- [Authentication & Session Management](designs/authentication-and-session-management/design.md)

### 7) Zero-Trace persistence rule

**Invariant**: Decrypted plaintext may exist transiently in active memory, but plaintext, keys, and passwords must never be persisted or emitted to disk, logs, telemetry, or developer-tooling outputs. Frontend sensitive state must be cleared when the vault/session locks.

**Source designs**:
- [Tauri IPC & Frontend](designs/tauri-ipc-and-frontend/design.md)
- [Authentication & Session Management](designs/authentication-and-session-management/design.md)

### 8) Hybrid epoch routing contract (`epoch_buffer_enabled`)

**Invariant**: `epoch_buffer_enabled` is opt-in at vault creation (default `false`). When enabled, routing is hybrid: files smaller than `chunk_size_bytes` are staged for epoch packing, while files `>= chunk_size_bytes` continue on the immediate standalone chunk path (including trailing partial chunks).

**Source designs**:
- [Chunking & Manifest](designs/chunking-and-manifest/design.md)
- [Tauri IPC & Frontend](designs/tauri-ipc-and-frontend/design.md)

### 9) Argon2 vault-header trust contract

**Invariant**: New vaults are created with Argon2id defaults `m=65536 KiB`, `t=3`, `p=4`. On existing devices, downloaded vault-header Argon2 parameters must exactly match locally cached trusted parameters before derivation proceeds. On first bootstrap for a new device (no local cache), OWASP floors (`19456/2/1`) are accepted as a minimum and parameters below Arx defaults must trigger a warning.

**Source designs**:
- [Authentication & Session Management](designs/authentication-and-session-management/design.md)
- [Cloud Synchronisation](designs/cloud-synchronisation/design.md)

### 10) Durable cloud deletion retry (`pending_deletions`)

**Invariant**: File deletion must enqueue blob names into `pending_deletions` within the same committed manifest transaction as node/chunk removal. Sync drains this queue and removes rows only after confirmed cloud deletion, so interrupted deletes are retried.

**Source designs**:
- [Chunking & Manifest](designs/chunking-and-manifest/design.md)
- [Cloud Synchronisation](designs/cloud-synchronisation/design.md)

### 11) Share package/import key-handling contract

**Invariant**: File sharing packages use HPKE (RFC 9180) and include `file_key` plus `sender_public_key` inside the encrypted JSON payload. Recipient import must wrap raw `file_key` immediately into `received_shares.file_key_wrapped` for at-rest storage and zeroize the raw bytes after wrapping. Sharing exposes only the selected file's `file_key` context; vault-wide keys (`master_key`, `sqlcipher_key`, `manifest_key`) are never exposed to share recipients.

**Source designs**:
- [File Sharing](designs/file-sharing/design.md)

### 12) Share revocation semantics (default vs strong)

**Invariant**: Revocation defaults to future-fetch blocking and does not claim recall of plaintext already fetched/decrypted. For stronger revocation, the owner rotates `file_key`, re-encrypts and republishes under a new `file_share_id`, and retires the old shared path.

**Source designs**:
- [File Sharing](designs/file-sharing/design.md)
- [Cloud Synchronisation](designs/cloud-synchronisation/design.md)

### 13) Vault identity ownership and read-only sharing access

**Invariant**: Exactly one `vault_identity` row exists per vault (`id = 1`). Identity creation is owned by `auth::ceremonies::create_vault`, credential rotations re-wrap the existing row in place, and sharing code may read `vault_identity.public_key` only.

**Source designs**:
- [Authentication & Session Management](designs/authentication-and-session-management/design.md)
- [File Sharing](designs/file-sharing/design.md)

### 14) Recovery slot key-handling contract (`arx-runa recovery v1`)

**Invariant**: Recovery slots wrap `master_key` independently of the primary credential slot using XChaCha20-Poly1305 with AAD `b"arx-runa recovery v1" || vault_id_bytes`. The recovery phrase (BIP-39 24-word mnemonic, 256-bit entropy, English wordlist) is returned to the UI exactly once and never stored by Arx Runa. The standard BIP-39 PBKDF2 derivation step is intentionally bypassed — the space-joined mnemonic is passed directly to Argon2id with the same parameters as the primary slot. `recover_with_phrase` is a single atomic ceremony with no intermediate authenticated session. Recovery is opt-in; users without a recovery slot cannot recover a lost vault.

**Source designs**:
- [Authentication & Session Management](designs/authentication-and-session-management/design.md)

### 15) Auth tier input construction and non-oracular failure semantics

**Invariant**: `master_key` derivation input is tier-dependent and fixed: Tier 1 uses password bytes only; Tier 2 concatenates password bytes with exactly 32 key-file bytes. All ceremonies that re-derive `master_key` (create, unlock, change-password, recover-with-phrase) must follow this construction. Authentication failure responses (`InvalidCredentials`) must not distinguish a wrong password from a wrong key file to prevent oracle attacks.

**Source designs**:
- [Authentication & Session Management](designs/authentication-and-session-management/design.md)

### 16) SQLCipher key handling from protected wrappers

**Invariant**: SQLCipher keys (`sqlcipher_key`) must be applied to database open, create, and rekey operations exclusively from protected wrappers. No by-value raw copies of `sqlcipher_key` may appear on the Rust stack during these operations.

**Source designs**:
- [Chunking & Manifest](designs/chunking-and-manifest/design.md)
- [Authentication & Session Management](designs/authentication-and-session-management/design.md)

### 17) Session key memory lifecycle (`mlock` + zeroize)

**Invariant**: Active `SessionKeys` must be memory-locked (`mlock`/`VirtualLock`) immediately after derivation. On vault lock or session timeout, all session key material must be zeroized before deallocation. `master_key` is scope-limited to the derivation ceremony and zeroized immediately after session keys are installed.

**Source designs**:
- [Authentication & Session Management](designs/authentication-and-session-management/design.md)

---

## MVP Scope & Deferred Items

**For complete details**, see [Deferred Items Inventory](deferred-items-inventory.md).

### Single-Vault MVP

**Invariant**: Arx Runa Phase 1–6 is a single-vault-per-device MVP. The session model assumes exactly one active vault at a time (`SessionManager::active_vault_id()`). Multi-vault support (Phase 7+) will require per-vault session coordination and UI switcher infrastructure.

**Impact on Phase 7+**: Multi-vault support is a candidate Phase 7 feature but is not in-scope for Phase 6.

### Out-of-Scope Architectural Limitations

1. **Compromised OS recovery**: Arx Runa assumes the OS is trusted. Cryptography cannot be stronger than the operating system itself.
2. **Malicious cloud provider**: The "bring-your-own-cloud" model trusts provider availability but not integrity. Detection-only via BLAKE3 checksums.
3. **Malicious Rclone sidecar**: The Rclone binary is trusted if obtained from the official release channel. A compromised binary is equivalent to a compromised OS.
4. **TOTP or authenticator apps**: Multi-factor auth must be deterministic for KDF derivation. Hardware keys (Tier 2 USB) satisfy this requirement; time-based codes do not.
5. **Live sharing (always-latest files)**: Requires directory-level share agreements. Current design uses immutable snapshot packages (Phase 5). Live sharing is deferred.

### Intentional MVP Limitations (Preserved in Phase 7+)

| Feature | Status | Phase 7+ Consideration |
|---------|--------|----------------------|
| Directory deletion | Files-only MVP | Candidate for Phase 7 feature (`delete_directory`) |
| File-level conflict detection | Detect-and-block only | Phase 7 research: three-way merge, timestamp comparison |
| EXIF stripping | JPEG/PNG only | Phase 7 candidate: video (MP4/WebM) with two-pass seek or spool |
| Revocation semantics | Default (future-fetch block) | Phase 7 option: strong revocation with key rotation |
| Fingerprint verification | Display-only | Phase 7 candidate: verification history, contact trust model |

### Forward Declarations (All Fulfilled)

All deferred APIs and trait boundaries from Phases 1–6 are **production-ready**:
- `CloudTransport` trait (Phase 4.1) ✅
- `VaultHeader` upload/download (Phase 4.3) ✅
- `destination_sessions` CRUD (Phase 4.2) ✅
- `SharingStore` trait with contacts/shares/received_shares (Phase 5.3 + 6) ✅
- Device monitor event emission (Phase 6.5) ✅

### Backend-Implemented, UI-Deferred Commands

The following IPC commands are **fully wired** but have **no UI consumer** in Phase 6.9:
- `recover_from_cloud` — Phase 7+ recovery UI
- `migrate_vault` — Phase 7+ cross-vault transfer UI
- `sync_backup` — Phase 7+ backup/restore lifecycle
- `get_file_content` — Phase 6.8+ in-app file viewer (backend ready, 50 MiB cap enforced)

These are candidates for Phase 7 UI implementation without backend changes.

---

## Phase 7+ Planning Reference

See [Phase 7 Roadmap](phase-7-roadmap.md) for:
- Prioritized feature candidates
- Effort estimates and design dependencies
- Technical questions needing research
- Success criteria

