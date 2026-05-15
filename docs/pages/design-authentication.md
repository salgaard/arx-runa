# Authentication and Session Management

Arx Runa's authentication layer derives session keys from user credentials, manages their lifecycle in mlocked memory, and provides credential rotation ceremonies. Two authentication tiers are supported, selected per vault at creation time — the cloud never receives keys, plaintext, or file names.

---

## Goals

- **Tier 1** — password only: Argon2id-hardened, suitable for most users
- **Tier 2** — password + USB key file: password alone cannot compromise data; the key file is a mandatory cryptographic input and a physical hardware factor
- USB key file auto-detected on insertion via OS-native device events — no manual path selection after first setup
- Session keys held in mlocked RAM, zeroized on timeout or manual lock
- Vault creation generates all initial material: key file, salt, vault header, SQLCipher DB, X25519 identity keypair
- Password and key file rotation without re-encrypting cloud blobs

---

## Contract Surface

### Interface

Authentication ceremonies: vault creation, login, password change, USB key file rotation, recovery authentication.

Each ceremony is implemented in its own file under `src-tauri/src/auth/ceremonies/`. The session access boundary is `SharedSession = Arc<RwLock<Option<SessionKeys>>>`.

Trait boundaries: `KeySource::read_key` (key file I/O) and `DeviceMonitor::watch` (USB events), each with production and mock implementations.

### Data

Canonical session container: `SessionKeys { key_encryption_key, sqlcipher_key, manifest_key }`.

Canonical vault header fields: `vault_id`, `schema_version`, `tier`, `argon2_salt`, `argon2_params`, `key_file_blake3`, `recovery_slots`.

Recovery slots: `method`, slot-local Argon2 parameters, `wrapped_master_key` (base64 of the 72-byte wrap format).

### Invariants

- Tier 1 Argon2id input: `password_utf8_bytes`. Tier 2: `password_utf8_bytes || key_file_bytes`.
- `InvalidCredentials` must not distinguish wrong password from wrong key file (non-oracular).
- Session keys remain mlocked and zeroized on lock or timeout; `master_key` is scope-limited during derivation.

### Dependencies

Depends on Phase 1 crypto primitives (Argon2id/HKDF chain, key wrapping, recovery-slot AEAD). Produces session keys consumed by storage (`sqlcipher_key`), key wrapping (`key_encryption_key`), and sync (`manifest_key`).

---

## USB Key File (Tier 2)

The key file is 32 bytes of CSPRNG-generated random data. It has no internal structure — it is raw entropy written to a user-chosen location on a removable drive.

### Auto-detection

At vault creation, `blake3::hash(key_file_content)` is stored in the vault header. At login:

1. OS device events fire when a drive mounts
2. Arx Runa scans mounted drives for files that are exactly 32 bytes
3. Each candidate is hashed and compared against `key_file_blake3` in the vault header
4. A match auto-populates the key file field in the login UI — the user still enters their password and confirms

The 32-byte size filter makes scanning near-instant. The BLAKE3 hash in the vault header is public (vault header is plaintext JSON); BLAKE3 preimage resistance means the hash cannot be reversed to recover key file content.

### OS device monitoring

- **Windows**: `RegisterDeviceNotification` or WMI `Win32_VolumeChangeEvent`
- **Linux**: `udev` crate monitoring for block device add events
- **macOS**: `DiskArbitration` framework via `core-foundation` FFI

```rust
trait DeviceMonitor: Send + Sync {
    fn watch(&self) -> Pin<Box<dyn Stream<Item = DeviceEvent> + Send>>;
}

enum DeviceEvent {
    Mounted { mount_path: PathBuf },
    Unmounted { mount_path: PathBuf },
}
```

---

## Argon2id Key Derivation

### Argon2id input

| Tier | Input |
|------|-------|
| Tier 1 | `password_utf8_bytes` |
| Tier 2 | `password_utf8_bytes || key_file_bytes` |

Tier 2 concatenation is unambiguous because the key file is always exactly 32 bytes.

### Parameters

| Parameter | Value |
|-----------|-------|
| Memory (m) | 65 536 KiB (64 MiB) — RFC 9106 §4 recommended tier |
| Iterations (t) | 3 |
| Parallelism (p) | 4 |
| Output length | 32 bytes |
| Salt | 32 bytes CSPRNG, stored in vault header |

Parameters are stored in the vault header; future increases do not break existing vaults.

### HKDF expansion

After Argon2id produces `master_key`, HKDF-SHA256 derives three keys (see [Cryptographic Primitives](design-cryptographic-primitives.md)):

```
master_key → key_encryption_key  (wraps file keys)
           → sqlcipher_key       (SQLCipher database)
           → manifest_key        (manifest cloud backup)
```

`master_key` is typed `Zeroizing<[u8; 32]>` — it is zeroed on drop and never stored, logged, or passed beyond the derivation scope.

---

## Session Management

```rust
struct SessionKeys {
    key_encryption_key: SecureBytes<32>,
    sqlcipher_key: SecureBytes<32>,
    manifest_key: SecureBytes<32>,
}

type SharedSession = Arc<RwLock<Option<SessionKeys>>>;
```

`SessionKeys` is held in mlocked memory. File operations borrow keys under a read lock; timeout/manual lock acquires a write lock and sets the `Option` to `None`, triggering `ZeroizeOnDrop`.

`SessionManager` tracks a lifecycle enum (`NoSession`, `Active`, `Expired`) so callers can distinguish "never authenticated" from "session expired".

### Timeout

Activity-based, default 15 minutes. The timer resets on every IPC command invocation. A `tokio` task fires on expiry, closes the operation gate (waiting for in-flight operations to complete), then zeroes `SessionKeys`.

**UX**: 60 seconds before timeout the frontend shows a warning. On timeout the login screen is shown and all open file views are cleared from memory.

### Memory locking

`mlock` (Linux/macOS) / `VirtualLock` (Windows) pins key buffers into physical RAM. If locking fails, Arx Runa refuses session creation with a clear error — a security product must not silently degrade its memory protection.

---

## Vault Creation Flow

1. User sets password and selects tier
2. [Tier 2] DeviceMonitor detects USB insertion; user confirms target drive
3. [Tier 2] Arx Runa generates 32-byte key file via CSPRNG, writes to USB, stores `blake3(key_file)` in vault header
4. Generate 32-byte Argon2id salt and `vault_id` (UUID v4)
5. Argon2id → `master_key` → HKDF → `key_encryption_key`, `sqlcipher_key`, `manifest_key`
6. `master_key` zeroized
7. Create SQLCipher DB; generate X25519 identity keypair; wrap private key with `key_encryption_key`
8. Write vault header JSON to local staging; upload to cloud
9. Session begins with `SessionKeys` in mlocked memory

**Critical invariant**: `master_key` exists for exactly one scope — between step 5 and step 6.

---

## Password Change

1. Full Argon2id re-derivation of `current_master_key` (not from session state)
2. [If recovery slot exists] Prompt for recovery phrase; verify it decrypts `current_master_key`
3. [Tier 2] Require USB key file present
4. Generate new salt; Argon2id with new password → `new_master_key` → new HKDF keys
5. SQLCipher transaction: re-wrap all `file_key` values and X25519 private key; commit
6. SQLCipher `PRAGMA rekey` with new `sqlcipher_key`
7. Update and upload vault header; upload manifest backup
8. Zeroize old keys; replace `SessionKeys`

File keys themselves do not change — only their wrapping changes. Cloud blobs are unaffected.

---

## Key File Rotation (Tier 2)

Same structure as password change, but generates a new 32-byte key file:

1. Full Argon2id re-derivation with current credentials
2. Generate new key file and new salt
3. Argon2id with `password || new_key_file` → new keys
4. SQLCipher transaction: re-wrap all file keys and X25519 private key
5. Update and upload vault header

**Sharing relationships survive rotation.** The X25519 identity keypair is re-wrapped under the new `key_encryption_key` but the keypair itself does not change.

### Crash recovery

A `pending-vault-header.json` staging file is written before the cloud upload. On startup, Arx Runa checks for this file and retries the upload if present, closing the crash window between local re-wrap commit and cloud header update.

---

## Recovery Slot

A recovery slot wraps `master_key` independently of the primary password, allowing vault access if primary credentials are lost. It is opt-in and requires re-entering current credentials.

### Derivation

```
recovery_entropy = CSPRNG(32 bytes)
recovery_phrase  = bip39::encode(recovery_entropy)  // 24-word mnemonic
recovery_salt    = CSPRNG(32 bytes)
recovery_key     = Argon2id(phrase_words_space_joined, recovery_salt, same_params_as_primary)
wrapped          = XChaCha20-Poly1305.encrypt(
                     key:       recovery_key,
                     plaintext: master_key,
                     aad:       b"arx-runa recovery v1" || vault_id_bytes
                   )
```

The AAD binds the slot to its vault, preventing cross-vault transplant attacks.

BIP-39 is used only for its wordlist encoding and checksum — the standard BIP-39 PBKDF2 seed derivation step is intentionally skipped. The phrase is passed directly to Argon2id.

### Recovery flow

1. Fetch vault header; validate `recovery_slots` is non-empty
2. User enters 24-word phrase; BIP-39 checksum validation (fast-fail)
3. Argon2id(phrase) → `recovery_key` → decrypt `wrapped_master_key`
4. HKDF → session keys; `master_key` zeroized
5. Prompt user to set a new password

After recovery + password change, the recovery slot is re-wrapped under the new `master_key` using the same phrase.

---

## Trait Boundaries

```rust
trait KeySource: Send + Sync {
    fn read_key(&self) -> Result<Zeroizing<[u8; 32]>, KeySourceError>;
}

trait DeviceMonitor: Send + Sync {
    fn watch(&self) -> Pin<Box<dyn Stream<Item = DeviceEvent> + Send>>;
}
```

Production implementations: `FileKeySource`, `WindowsDeviceMonitor`, `LinuxDeviceMonitor`, `MacOsDeviceMonitor`.

Test implementations: `MockKeySource`, `MockDeviceMonitor`.

---

## Threat Model

Arx Runa protects against:
- Untrusted cloud providers (all blobs are opaque AEAD ciphertext)
- Network eavesdropping (HTTPS + AEAD + per-file key isolation)
- Disk theft (files encrypted at rest; keys in mlocked memory only)
- Lost credentials (BIP-39 recovery slots as opt-in post-creation)

Arx Runa does **not** protect against:
- Malicious or compromised OS (cryptography is weaker than OS security)
- Cold-boot attacks (kernel-level memory access)
- Malware in the Arx Runa binary distribution (standard software supply-chain assumptions)

This threat model is consistent with Tahoe-LAFS, Cryptomator, and other desktop zero-knowledge tools.

---

## Related Documents

- [Cryptographic Primitives](design-cryptographic-primitives.md) — Argon2id/HKDF, key wrapping wire formats
- [Chunking and Manifest](design-chunking-and-manifest.md) — SQLCipher keyed with `sqlcipher_key`
- [Cloud Synchronisation](design-cloud-synchronisation.md) — vault header bootstrap, manifest backup
- [Tauri IPC and Frontend](design-tauri-ipc-and-frontend.md) — auth command surface
