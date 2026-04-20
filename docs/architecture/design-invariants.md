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

**Invariant**: File sharing packages use HPKE (RFC 9180) and include `file_key` plus `sender_public_key` inside the encrypted JSON payload. Recipient import must wrap raw `file_key` immediately into `received_shares.file_key_wrapped` for at-rest storage and zeroize the raw bytes after wrapping.

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

