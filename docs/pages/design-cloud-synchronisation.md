# Cloud Synchronisation

Encrypted blobs move between the local staging directory and a cloud remote through a provider-agnostic `CloudTransport` trait. Rclone is the sole concrete transport, bundled as a Tauri sidecar — no cloud SDK dependencies, no provider lock-in. The cloud learns only blob count, uniform blob sizes, and access timing.

---

## Goals

- Provider-agnostic blob transport via the `CloudTransport` trait
- Rclone bundled as a Tauri sidecar — no cloud SDK dependencies, no provider lock-in
- Vault header uploaded as plaintext JSON to enable new-device bootstrap before any keys exist
- SQLCipher manifest encrypted and backed up to the cloud, enabling full vault recovery from password and key file alone
- Monotonic `snapshot_counter` detects conflicting pushes from multiple devices
- Cloud learns only blob count, uniform blob sizes, and access timing

---

## Contract Surface

### Interface

Cloud transport boundary: `CloudTransport` with `upload_blob`, `download_blob`, `delete_blob`, `list_blobs` over cloud-root-relative paths.

Synchronisation boundary: push, pull, backup sync, and migration flows with typed progress events (`SyncProgress`, `MigrationProgress`).

Endpoint/session configuration: `CloudEndpoint`, `DestinationSession`, `SyncConfig`.

### Data

Canonical cloud layout:
```
<cloud_root>/
  vault-header.json
  manifest/manifest-backup.blob
  vault/<uuid>.blob
  shared/<file_share_id>/<uuid>.blob
```

Canonical bootstrap payload: `VaultHeader` with `Argon2Params` and optional `RecoverySlot` entries.

### Invariants

- Remote paths stay relative to cloud root; path traversal and absolute escapes are rejected.
- `snapshot_counter` is monotonic; conflict checks gate every push before mutation.
- `vault-header.json` remains plaintext; `manifest/manifest-backup.blob` remains AEAD ciphertext under `manifest_key`.

### Dependencies

Depends on the bundled Rclone sidecar and Tauri sidecar invocation model. Consumes storage manifest contracts and crypto contracts. Consumes auth-derived keys and vault header semantics.

---

## Cloud Storage Layout

```
<remote>:<cloud_root>/
  vault-header.json               # plaintext JSON, accessible before auth
  manifest/
    manifest-backup.blob          # encrypted SQLCipher export
  vault/
    <uuid>.blob                   # owner's encrypted chunks
  shared/
    <file_share_id>/
      <uuid>.blob                 # shared file chunks (Phase 5)
```

**Vault header at root.** Must be downloadable before key material exists — new-device recovery needs the Argon2id salt and parameters before keys can be derived.

**`vault/` is flat.** Blob names are UUID v4. Any structural metadata would reveal file organisation to the cloud provider.

**`manifest-backup.blob` is a single file** overwritten on each push. The `snapshot_counter` inside the encrypted manifest is the logical version.

---

## CloudTransport Trait

```rust
#[async_trait]
pub trait CloudTransport: Send + Sync {
    async fn upload_blob(&self, local_path: &Path, remote_path: &str) -> Result<(), CloudTransportError>;
    async fn download_blob(&self, remote_path: &str, local_path: &Path) -> Result<(), CloudTransportError>;
    async fn delete_blob(&self, remote_path: &str) -> Result<(), CloudTransportError>;
    async fn list_blobs(&self, remote_prefix: &str) -> Result<Vec<String>, CloudTransportError>;
}

#[non_exhaustive]
pub enum CloudTransportError {
    NotFound,
    AuthenticationFailed,
    Timeout,
    IoError(#[from] std::io::Error),
    RcloneProcessFailed { exit_code: i32, stderr_sanitised: String },
    Other(String),
}
```

**Path-based, not stream-based.** Blobs are already on disk in staging; `&Path` avoids buffering blob content into Rust memory and lets Rclone manage I/O internally.

**Relative paths as `&str`.** Cloud paths use forward-slash separators regardless of host OS. `&str` prevents accidental OS-specific separator injection.

**Idempotent operations.** `upload_blob` overwrites; `delete_blob` is a no-op on missing paths. Both enable safe retry.

---

## Rclone Sidecar Model

Rclone runs as a Tauri sidecar process:
- No cloud SDK dependencies in the Rust binary
- Any Rclone-supported backend works immediately (S3, GCS, Backblaze B2, SFTP, local, etc.)
- Credentials travel to Rclone via a session-lived temp config file, never via command-line arguments

### Credential lifecycle

1. Authenticate → derive `sqlcipher_key` → open SQLCipher
2. Read all `destination_sessions` rows; decrypt `rclone_config_blob` for each
3. Assemble in-memory Rclone config; write to temp file in a process-owned secure temp directory
4. All Rclone calls use `--config <temp_path>`
5. On session close: overwrite + unlink temp file; zeroize in-memory credentials

Credentials are never written to disk in plaintext outside this temp file. Cross-vault isolation: each vault has its own SQLCipher DB with its own credential rows.

---

## Multi-Destination Model

Each vault has exactly **one primary destination** and **zero or more backup destinations**.

- **Primary** — all uploads go here; must be reachable on push/pull
- **Backup destinations** — synced from primary on demand or on schedule; may be intermittently connected (USB drives, secondary cloud accounts)

```rust
pub enum DestinationType { Cloud, ExternalDrive, LocalPath }

pub enum BackupSyncMode {
    Mirror,        // backup matches current state of primary; deletes propagate
    Accumulating,  // blobs are never deleted from backup (recoverable history)
}

pub struct DestinationSession {
    pub destination_id:      String,
    pub label:               String,
    pub destination_type:    DestinationType,
    pub rclone_remote_name:  String,
    pub rclone_config_blob:  String,   // encrypted in SQLCipher
    pub bucket:              String,
    pub path_prefix:         String,
    pub is_primary:          bool,
    pub backup_mode:         Option<BackupSyncMode>,
}
```

---

## New-Device Bootstrap

`cloud-config.json` stores non-sensitive endpoint metadata (remote name, bucket, region, endpoint URL, path prefix) needed to locate the vault header before authentication:

| OS | Path |
|----|------|
| Windows | `%APPDATA%/arx-runa/cloud-config.json` |
| Linux | `~/.local/share/arx-runa/cloud-config.json` |

After authentication, the full `DestinationSession` records (including credentials) are recovered from the decrypted SQLCipher manifest.

### Argon2id parameter validation

`local-vault-params.json` stores trusted local `vault_id`, `argon2_salt`, and `argon2_params`.

- **Existing device**: downloaded vault header must match cached values exactly (downgrade-resistant)
- **New device**: accept OWASP floors as bootstrap minimum; warn if below Arx Runa defaults

---

## Conflict Detection

Each vault has a monotonic `snapshot_counter` in `manifest_meta`. Every push increments the counter. Before any push:

1. Download the current `manifest-backup.blob`
2. Decrypt and read its `snapshot_counter`
3. If the cloud counter is higher than the local counter → conflict; block push and prompt for manual resolution

Conflicts occur when two devices push without a pull in between. Resolution requires the user to choose which device's state is authoritative.

---

## Manifest Backup

The SQLCipher manifest is exported and encrypted with `manifest_key` (XChaCha20-Poly1305) on every successful push:

```
manifest_backup_blob = XChaCha20-Poly1305.encrypt(
    key:       manifest_key,
    plaintext: sqlite3_serialize(manifest_db),
    aad:       []    // singleton blob with a purpose-specific key; no multi-instance context
)
```

The manifest backup is a single file overwritten on each push. A new device recovering from cloud fetches this blob and imports it into a fresh SQLCipher database keyed with `sqlcipher_key`.

---

## Vault Header

Stored as plaintext JSON at `<cloud_root>/vault-header.json`. Public by design — contains no secret material, only bootstrap parameters.

```json
{
  "vault_id": "<uuid-v4>",
  "schema_version": 1,
  "tier": 1,
  "argon2_salt": "<base64-32-bytes>",
  "argon2_params": { "memory_cost": 65536, "time_cost": 3, "parallelism": 4 },
  "key_file_blake3": null,
  "recovery_slots": []
}
```

Tier 2 vaults include `"key_file_blake3": "<hex>"`. Vaults with recovery slots include a `recovery_slots` array entry per slot.

---

## Related Documents

- [Cryptographic Primitives](design-cryptographic-primitives.md) — `manifest_key`, XChaCha20-Poly1305
- [Authentication and Session Management](design-authentication.md) — vault header bootstrap, `manifest_key` origin
- [Chunking and Manifest](design-chunking-and-manifest.md) — staging directory, `pending_deletions` drain
- [File Sharing](design-file-sharing.md) — `shared/` cloud namespace
- [Tauri IPC and Frontend](design-tauri-ipc-and-frontend.md) — `sync_to_cloud`, `recover_from_cloud` commands
