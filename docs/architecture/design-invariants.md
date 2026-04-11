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

**Invariant**: Vault key derivation uses HKDF-SHA256 with fixed salt `b"arx-runa-v1"` and fixed info strings:
- `b"arx-runa-key-encryption"`
- `b"arx-runa-sqlcipher"`
- `b"arx-runa-manifest-backup"`

New derived keys must use distinct new info strings without changing existing constants.

**Source designs**:
- [Cryptographic Primitives](designs/cryptographic-primitives/design.md)
- [Authentication & Session Management](designs/authentication-and-session-management/design.md)

### 4) Chunk size contract (`chunk_size_bytes`)

**Invariant**: `chunk_size_bytes` is set at vault creation, stored in `manifest_meta`, and validated at vault open. Default is 4 MiB (4,194,304 bytes). Configurable range is 128 KiB (131,072 bytes) to 64 MiB (67,108,864 bytes).

**Source designs**:
- [Chunking & Manifest](designs/chunking-and-manifest/design.md)

### 5) Cloud endpoint/path contract (`path_prefix`)

**Invariant**: `CloudEndpoint.path_prefix` defines the cloud root. All `CloudTransport` remote paths and prefixes are relative to that root, and path validation must prevent root-escape patterns (for example `..` and absolute paths).

**Source designs**:
- [Cloud Synchronisation](designs/cloud-synchronisation/design.md)

### 6) Zero-Trace rule (transient memory vs persisted/logged data)

**Invariant**: Decrypted plaintext may exist transiently in runtime memory while actively processing or rendering. Decrypted plaintext, keys, and passwords must not be persisted or emitted to disk, logs, telemetry, or developer-tooling outputs. Zero-trace means no persisted plaintext artifacts under application control; it does not claim plaintext is never in memory. Frontend state must be cleared when the vault/session locks.

**Source designs**:
- [Tauri IPC & Frontend](designs/tauri-ipc-and-frontend/design.md)
- [Authentication & Session Management](designs/authentication-and-session-management/design.md)

### 7) Hybrid epoch routing contract (`epoch_buffer_enabled`)

**Invariant**: `epoch_buffer_enabled` is opt-in at vault creation (default `false`). When enabled, routing is hybrid: files smaller than `chunk_size_bytes` are staged for epoch packing, while files `>= chunk_size_bytes` continue on the immediate standalone chunk path (including trailing partial chunks).

**Source designs**:
- [Chunking & Manifest](designs/chunking-and-manifest/design.md)
- [Tauri IPC & Frontend](designs/tauri-ipc-and-frontend/design.md)

