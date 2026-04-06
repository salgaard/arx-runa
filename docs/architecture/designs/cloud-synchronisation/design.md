# Arx Runa — Cloud Synchronisation Design

> Status: Design complete. Implementation target: Phase 4.
> Last updated: 2026-03-30

---

## Goals

- Encrypted blobs move between the local staging directory and a cloud remote via a provider-agnostic `CloudTransport` trait
- Rclone is the sole concrete transport, bundled as a Tauri sidecar binary — no cloud SDK dependencies, no provider lock-in
- The vault header is uploaded as plaintext JSON to enable new-device bootstrap before any keys exist
- The SQLCipher manifest is encrypted and uploaded as a cloud backup, enabling full vault recovery from password and USB key file alone
- A monotonic `snapshot_counter` detects conflicting pushes from multiple devices; conflicts require manual resolution
- The cloud provider learns only blob count, uniform blob sizes, and access timing — never file names, folder structure, or file contents

---

## Cloud Storage Layout

```
<remote>:<cloud_root>/
  vault-header.json               -- plaintext JSON, accessible before auth
  manifest/
    manifest-backup.blob          -- encrypted SQLCipher export
  vault/
    <uuid>.blob                   -- owner's encrypted chunks
  shared/                         -- Phase 5: file sharing (reserved)
    <file_share_id>/
      <uuid>.blob
```

### Rationale

**Vault header at root.** The vault header must be downloadable before any key material exists — the new-device recovery flow needs the Argon2id salt and parameters before it can derive keys. Placing it at the cloud root avoids any subdirectory that would require prior auth to enumerate.

**`vault/` is flat.** Blob names are UUID v4. Collision probability at 2³² blobs is below 2⁻⁶¹, negligible in practice. A flat layout avoids any structural metadata that could reveal file organisation to the cloud provider.

**`manifest/manifest-backup.blob` is a single file** overwritten on each push. The `snapshot_counter` inside the encrypted manifest is the logical version. Cloud-provider object versioning (S3 object versioning, etc.) can retain history if the user enables it — Arx Runa does not manage that.

**`shared/` is reserved for Phase 5.** The layout is defined here to establish the full directory contract. Arx Runa does not create `shared/` until the first file share.

---

## CloudTransport Trait

```rust
/// Provider-agnostic cloud storage operations over opaque byte blobs.
///
/// Implementations MUST NOT log blob content. Remote paths may be logged
/// at debug level but MUST NOT appear in user-facing error messages.
///
/// All methods use relative paths within the cloud root
/// (e.g., `"vault/<uuid>.blob"`, `"vault-header.json"`).
/// Callers never construct absolute remote paths.
///
/// Upload and delete operations are idempotent to enable safe retry.
#[async_trait]
pub trait CloudTransport: Send + Sync {
    /// Uploads a file from a local path to the cloud remote.
    ///
    /// `local_path` is the absolute path to the source file on disk.
    /// `remote_path` is relative to the cloud root (forward-slash separated).
    ///
    /// Overwrites if the remote path already exists.
    async fn upload_blob(
        &self,
        local_path: &Path,
        remote_path: &str,
    ) -> Result<(), CloudTransportError>;

    /// Downloads a blob from the cloud remote to a local file.
    ///
    /// `remote_path` is relative to the cloud root.
    /// `local_path` is the absolute destination path.
    ///
    /// Returns `CloudTransportError::NotFound` if the remote path does not exist.
    async fn download_blob(
        &self,
        remote_path: &str,
        local_path: &Path,
    ) -> Result<(), CloudTransportError>;

    /// Deletes a blob from the cloud remote.
    ///
    /// `remote_path` is relative to the cloud root.
    /// No error if the blob does not exist.
    async fn delete_blob(
        &self,
        remote_path: &str,
    ) -> Result<(), CloudTransportError>;

    /// Lists all blob paths under a remote directory prefix.
    ///
    /// `remote_prefix` is relative to the cloud root (e.g., `"vault/"`,
    /// `"shared/<id>/"`). Returns paths relative to the cloud root.
    async fn list_blobs(
        &self,
        remote_prefix: &str,
    ) -> Result<Vec<String>, CloudTransportError>;
}

/// Errors that can arise from cloud transport operations.
#[derive(thiserror::Error, Debug)]
pub enum CloudTransportError {
    /// The requested remote path does not exist.
    #[error("blob not found at remote path")]
    NotFound,

    /// Cloud provider authentication failed (expired token, wrong credentials).
    #[error("cloud transport authentication failed")]
    AuthenticationFailed,

    /// The operation exceeded its time limit.
    #[error("cloud transport operation timed out")]
    Timeout,

    /// Local I/O failed (staging directory, temp file, etc.).
    #[error("cloud transport local I/O error")]
    IoError(#[from] std::io::Error),

    /// Rclone process exited with a non-zero exit code.
    /// `stderr_sanitised` has credential-related lines stripped.
    #[error("rclone process failed with exit code {exit_code}")]
    RcloneProcessFailed {
        exit_code: i32,
        stderr_sanitised: String,
    },

    /// Catch-all for unexpected errors.
    #[error("cloud transport error: {0}")]
    Other(String),
}
```

### Trait design rationale

**Path-based, not stream-based.** Rclone is a file-oriented tool; blobs are already on disk in the staging directory. Passing `&Path` avoids buffering blob content into Rust memory and lets Rclone manage I/O internally.

**Relative paths as `&str`.** Cloud paths are forward-slash-separated strings regardless of the host OS. Using `&str` (not `PathBuf`) prevents accidental OS-specific separator injection.

**Idempotent operations.** `upload_blob` overwrites; `delete_blob` is a no-op on missing paths. Both properties enable safe retry without tracking which operations succeeded.

---

## Connection Descriptor

```rust
/// Describes a cloud storage endpoint for Rclone.
///
/// For owner operations, this is loaded from `cloud-config.json` in the
/// Arx Runa application data directory.
///
/// For received file shares (Phase 5), this is deserialized from the
/// `cloud_endpoint` field in the share package — the formats are identical.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudEndpoint {
    /// Rclone remote name as configured in rclone.conf (e.g., `"my-remote"`).
    pub provider: String,

    /// Bucket or container name (provider-specific).
    pub bucket: String,

    /// Cloud region identifier (e.g., `"eu-west-1"`). Empty string if not
    /// applicable (e.g., local filesystem remote during testing).
    pub region: String,

    /// Provider endpoint URL for S3-compatible services. Empty string for
    /// default provider endpoints (AWS S3, Google Drive).
    pub endpoint: String,

    /// Path prefix within the bucket that acts as the cloud root for all
    /// `CloudTransport` operations (e.g., `"arx-runa/"` or
    /// `"shared/<file_share_id>/"`).
    pub path_prefix: String,
}
```

**Where the owner's config is stored.** `%APPDATA%/arx-runa/cloud-config.json` (Windows) or `~/.local/share/arx-runa/cloud-config.json` (Linux). This file contains no secrets — Rclone credentials live in Rclone's own `rclone.conf`. The cloud config must be readable before authentication (required at step 1 of new-device recovery).

---

## Rclone Sidecar Model

### Bundling via Tauri sidecar

Rclone is bundled as a Tauri external binary (sidecar). The `tauri.conf.json` `externalBin` field lists the platform-specific Rclone binaries. Tauri resolves the binary path at runtime via `app.shell().sidecar("rclone")`.

The sidecar approach:
- Ships Rclone with Arx Runa — no user installation step
- Tauri handles extraction, platform detection, and path resolution
- Rclone is MIT-licensed; bundling is permitted without restriction

### Subprocess invocation

```rust
/// Constructs and executes an Rclone command as a sidecar subprocess.
///
/// Arguments are passed as a `Vec<OsString>` to `tokio::process::Command`.
/// No shell interpolation occurs. Callers validate remote paths before
/// passing them to this function.
async fn run_rclone(
    binary_path: &Path,
    args: Vec<OsString>,
    timeout: Duration,
) -> Result<String, CloudTransportError>;
```

**Never through a shell.** `tokio::process::Command::new(binary_path).args(&args)` — each argument is a separate OS string, not concatenated into a shell command. `sh -c` and `cmd /c` are prohibited.

### Rclone commands used

| Operation | Rclone command |
|-----------|---------------|
| Upload | `rclone copyto <local_path> <remote_root>/<remote_path> --quiet --no-traverse` |
| Download | `rclone copyto <remote_root>/<remote_path> <local_path> --quiet --no-traverse` |
| Delete | `rclone deletefile <remote_root>/<remote_path> --quiet` |
| List | `rclone lsjson <remote_root>/<remote_prefix> --recursive --files-only --no-mimetype --no-modtime` |

For `list_blobs`, the JSON array output is parsed; the `"Path"` field from each entry is extracted and prepended with `remote_prefix` to produce full relative paths.

### Remote path sanitisation

Before constructing any Rclone command, `remote_path` is validated against:

```
^[a-zA-Z0-9._/-]+$
```

Reject if the path:
- Contains `..` (path traversal)
- Begins with `/` (absolute path escape)
- Contains any character outside the allowlist

This prevents a crafted manifest from causing Rclone to operate outside the cloud root.
<!-- SOURCE: Path Traversal | OWASP Foundation — https://owasp.org/www-community/attacks/Path_Traversal — "Validate the user's input by only accepting known good – do not sanitize the data" -->

### Exit code mapping

| Exit code | Meaning | `CloudTransportError` |
|-----------|---------|----------------------|
| 0 | Success | — |
| 3 | Directory not found | `NotFound` |
| 4 | File not found | `NotFound` |
| Other | Rclone error | `RcloneProcessFailed` |

### Stderr sanitisation

Rclone stderr is captured. Before including it in `CloudTransportError::RcloneProcessFailed.stderr_sanitised`, all lines containing any of the following substrings (case-insensitive) are removed: `token`, `key`, `secret`, `password`, `credential`, `auth`. This prevents Rclone's `rclone.conf` credentials from reaching the frontend via error messages.

### Rclone retry

Rclone is invoked with `--retries 3` (the default). Arx Runa does not add a separate retry layer on top of individual blob operations; retry at the push/pull flow level is left to the user.

### Rclone configuration file location

Arx Runa uses an **isolated `rclone.conf`** in the Arx Runa application data directory, not the system default:

- **Windows**: `%APPDATA%/arx-runa/rclone.conf`
- **Linux**: `~/.local/share/arx-runa/rclone.conf`

Every Rclone invocation includes `--config <arx_runa_rclone_conf_path>`.

**Rationale**:
- Prevents interference with user's existing Rclone remotes
- Avoids credential conflicts if user has a personal Rclone setup
- Makes Arx Runa self-contained — uninstall removes all config
- User cannot accidentally expose Arx Runa remotes to other Rclone tools

**Trade-off**: Users cannot reuse existing Rclone remotes. This is acceptable — the guided wizard handles initial setup, and Arx Runa remotes have security requirements (no caching, specific flags) that ad-hoc remotes may not meet.

### Operation timeouts

| Operation | Default timeout | Rationale |
|-----------|----------------|-----------|
| Upload (per blob) | 5 minutes | 4 MiB blob completes in < 1 min on 10 Mbps upload; allows for slow connections |
| Download (per blob) | 5 minutes | Same as upload |
| Delete | 30 seconds | Lightweight metadata operation |
| List | 60 seconds | May enumerate thousands of blobs; provider API latency varies |

Timeouts are passed to `tokio::time::timeout` wrapping the Rclone subprocess. If the timeout expires, the Rclone process is killed (`kill` on Unix, `TerminateProcess` on Windows) and `CloudTransportError::Timeout` is returned.

Rclone's internal `--timeout` flag is **not used** — Arx Runa manages timeouts externally for consistent behaviour across all operations.

### Concurrency configuration

```rust
/// Synchronisation behaviour settings, separate from cloud endpoint identity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncConfig {
    /// Maximum concurrent Rclone processes during push/pull.
    /// Default: 4. Range: 1–16.
    pub max_concurrent: u32,

    /// Per-operation timeout in seconds.
    /// Default: 300 (5 minutes). Range: 60–3600.
    pub operation_timeout_seconds: u64,
}

impl Default for SyncConfig {
    fn default() -> Self {
        Self {
            max_concurrent: 4,
            operation_timeout_seconds: 300,
        }
    }
}
```

`SyncConfig` is stored in `%APPDATA%/arx-runa/sync-config.json` (or Linux equivalent). It is user-editable but not exposed in the initial UI — advanced users can modify it directly.

**Why separate from `CloudEndpoint`**: `CloudEndpoint` describes *where* to sync (provider, bucket, credentials). `SyncConfig` describes *how* to sync (concurrency, timeouts). Keeping them separate allows:
- Same sync behaviour across multiple vaults
- Endpoint changes without losing tuning
- Sensible defaults that work for most users

---

## Guided Setup Wizard

Arx Runa provides a provider selection UI rather than requiring the user to run `rclone config` directly. The wizard covers the two supported providers.

### Supported providers

| Provider | Type | Authentication |
|----------|------|---------------|
| S3-compatible (AWS, MinIO, Backblaze B2, Wasabi) | `s3` | Access key ID + secret access key |
| Google Drive | `drive` | OAuth 2.0 (browser flow) |

### S3-compatible flow

1. User selects provider and enters: access key ID, secret access key, bucket name, region, endpoint URL (optional; empty for AWS)
2. Arx Runa generates a remote name: `arx-runa-<uuid>`
3. Calls via sidecar:
   ```
   rclone config create arx-runa-<uuid> s3
     provider=Other
     access_key_id=<id>
     secret_access_key=<secret>
     region=<region>
     endpoint=<endpoint>
     --non-interactive
   ```
4. Stores `CloudEndpoint` in `cloud-config.json`

Arx Runa passes credentials as arguments to `rclone config create`, not as environment variables or config file snippets. Rclone then stores them in its own `rclone.conf`. After the wizard, Arx Runa no longer holds the credentials.

### Google Drive flow

1. User selects Google Drive
2. Arx Runa calls:
   ```
   rclone config create arx-runa-<uuid> drive scope=drive --non-interactive
   ```
3. Rclone prints an OAuth URL; Arx Runa opens it in the default browser via `tauri::api::shell::open`
4. User authorises in browser; Rclone captures the token
5. Arx Runa stores the resulting `CloudEndpoint`

### Wizard security properties

- Credentials are passed as arguments to `rclone config create`, validated against the remote path sanitisation rules
- Credentials are not written to any Arx Runa file, not logged, and not held in memory after the wizard completes
- Rclone's `rclone.conf` is the authoritative credential store

---

## Vault Header

### Structure

The vault header is serialised as UTF-8 JSON and stored at `<cloud_root>/vault-header.json`. It is plaintext by design.

```rust
/// Plaintext vault bootstrap parameters stored in the cloud.
///
/// All fields are intentionally public. See "Security analysis" below.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultHeader {
    /// Random UUID v4 identifying this vault.
    pub vault_id: String,

    /// Schema version for forward compatibility.
    pub schema_version: u32,

    /// Authentication tier: 1 (password only) or 2 (password + USB key file).
    pub tier: u8,

    /// Argon2id salt, base64-encoded. 32 bytes (256 bits).
    /// Public parameter by design (NIST SP 800-132).
    pub argon2_salt: String,

    /// Argon2id parameters used for key derivation.
    pub argon2_params: Argon2Params,

    /// BLAKE3 hash of the USB key file content, hex-encoded.
    /// Present only for Tier 2 vaults; `None` for Tier 1.
    pub key_file_blake3: Option<String>,
}

/// Argon2id tuning parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Argon2Params {
    /// Memory cost in KiB. Minimum 19456 (OWASP).
    pub memory_cost: u32,

    /// Time cost (iterations). Minimum 2 (OWASP).
    pub time_cost: u32,

    /// Parallelism. Always 1 in Arx Runa.
    pub parallelism: u32,
}
```

Wire format (from Phase 2 auth design):

**Tier 1 (password only):**

```json
{
  "vault_id": "<uuid-v4>",
  "schema_version": 1,
  "tier": 1,
  "argon2_salt": "<base64-32-bytes>",
  "argon2_params": { "memory_cost": 19456, "time_cost": 2, "parallelism": 1 },
  "key_file_blake3": null
}
```

**Tier 2 (password + USB key file):**

```json
{
  "vault_id": "<uuid-v4>",
  "schema_version": 1,
  "tier": 2,
  "argon2_salt": "<base64-32-bytes>",
  "argon2_params": { "memory_cost": 19456, "time_cost": 2, "parallelism": 1 },
  "key_file_blake3": "<hex-32-bytes>"
}
```

### Upload flow

```
1. Serialise VaultHeader to JSON bytes
2. Write to temp file in the Arx Runa staging directory
3. upload_blob(temp_path, "vault-header.json")
4. Delete temp file
```

### Download and parse flow

```
1. download_blob("vault-header.json", temp_path)
2. Read temp file, deserialise JSON → VaultHeader
3. Validate:
   a. schema_version is supported
   b. tier is 1 or 2
   c. argon2_salt decodes from base64 to exactly 32 bytes
   d. argon2_params.memory_cost >= 19456
   e. argon2_params.time_cost >= 2
   f. If tier == 2: key_file_blake3 decodes from hex to exactly 32 bytes
   g. If tier == 1: key_file_blake3 is null
4. Delete temp file
5. Return VaultHeader
```

### Security analysis of plaintext fields

All vault header fields are intentionally public. Storing them in plaintext does not weaken the authentication model:

| Field | Why it is safe to expose |
|-------|--------------------------|
| `vault_id` | Random UUID v4; reveals only that a vault exists |
| `schema_version` | Integer; no sensitive information |
| `tier` | Integer (1 or 2); reveals only whether hardware MFA is used — not key material |
| `argon2_salt` | Public parameter by design — required before key derivation; NIST SP 800-132 designates salts as public <!-- CITE: NIST SP 800-132 §5.1 --> |
| `argon2_params` | Public tuning parameters; an attacker who has the salt still needs the password (and key file for Tier 2) |
| `key_file_blake3` | Tier 2 only; `null` for Tier 1. BLAKE3 is preimage-resistant; the hash cannot be reversed to recover the 32-byte key file. An attacker gains only the ability to verify a candidate file — equivalent to attempting authentication <!-- CITE: BLAKE3 specification — preimage resistance property --> |

---

## Manifest Cloud Backup

### Purpose

The encrypted manifest backup enables full vault recovery on a new device. Without it, a user who loses their local manifest loses all blob-to-file mappings and cannot reassemble any files, even if the blobs are present in the cloud.

### Encryption scheme

The SQLCipher database file is exported via `VACUUM INTO '<temp_path>'`, producing a consistent read-consistent snapshot. The export is loaded into a buffer (explicit exception to the streaming rule: manifests are typically below 10 MiB, and loading into memory simplifies single-AEAD encryption).

Wire format:

```
[24-byte nonce | ciphertext | 16-byte Poly1305 tag]
```

No AAD is used. The manifest backup is a singleton — there is no `file_id` or `chunk_index` context to bind. The `manifest_key` derived from `master_key` via `hkdf(master, info=b"arx-runa-manifest-backup")` is purpose-specific and not used for any other operation.

### Upload flow

```
1. VACUUM INTO '<staging_dir>/manifest-export.db'
2. Read manifest-export.db into buffer (manifest_buffer)
3. Generate 24-byte nonce via CSPRNG
4. Encrypt: XChaCha20-Poly1305(manifest_key, nonce, manifest_buffer, aad=None)
   → encrypted_manifest
5. Write [nonce | encrypted_manifest] to staging_dir/manifest-backup.blob
6. Zeroize manifest_buffer
7. upload_blob("staging_dir/manifest-backup.blob", "manifest/manifest-backup.blob")
8. Delete both temp files
```

### Download and decrypt flow (new-device recovery)

```
1. download_blob("manifest/manifest-backup.blob", temp_path)
2. Read temp file
3. Extract 24-byte nonce from prefix
4. Decrypt: XChaCha20-Poly1305(manifest_key, nonce, ciphertext, aad=None)
   → manifest_buffer
5. Write manifest_buffer to the Arx Runa data directory as the local SQLCipher DB
6. Zeroize manifest_buffer
7. Open SQLCipher DB with sqlcipher_key to verify integrity
8. Delete temp file
```

---

## Push Flow

The push flow moves all locally staged blobs to the cloud and updates the cloud manifest backup.

```
1.  Read local snapshot_counter from manifest_meta
2.  Download current manifest/manifest-backup.blob → decrypt → read cloud
    snapshot_counter
    - If cloud download fails with NotFound: first push, skip conflict check
    - If decryption fails: treat as conflict; abort
3.  If cloud_counter > local_counter: CONFLICT — abort push, return error
    "Another device has synced changes. Pull the latest changes first."
4.  If cloud_counter == local_counter: safe to push (continue)
5.  Collect all blob_names from the chunks table that have a corresponding
    staging file in staging_dir/<blob_name>.blob
6.  Shuffle blob list (Fisher-Yates) to randomise upload order
7.  Upload in parallel (tokio::JoinSet, max_concurrent = 4):
    For each blob_name:
      a. upload_blob("staging/<blob_name>.blob", "vault/<blob_name>.blob")
      b. On success: delete staging/<blob_name>.blob
      c. On failure: record error, stop issuing new tasks, drain in-flight
8.  If any upload failed: return error with list of successfully uploaded blobs
    (retrying is safe — upload is idempotent)
9.  Increment snapshot_counter via MetadataStore::increment_snapshot_counter()
10. Set manifest_meta.last_synced_at to current Unix timestamp
11. Encrypt and upload manifest backup (Section above)
    - On failure: roll back snapshot_counter to previous value;
      return error (retry will re-attempt from step 2)
12. Upload vault header (idempotent; overwrites previous version)
    - Required after password change or key rotation (vault header fields change)
    - Cheap and safe to upload unconditionally on every push
```

### Upload order randomisation

When a multi-chunk file is uploaded, sequential upload order (chunk 0, chunk 1, …) allows the cloud provider to correlate blobs by temporal proximity. Shuffling the entire pending blob list across all staged files destroys temporal correlation: blobs from different files are interleaved, and no consistent ordering reveals which blobs form a file.

Cost: a `Fisher-Yates` shuffle on a `Vec<String>` is O(n) with negligible overhead.
<!-- SOURCE: Chunking Attacks on File Backup Services using Content-Defined Chunking — Alexeev, Percival, Zhang (2025) — https://eprint.iacr.org/2025/532.pdf — adversary observing upload order in encrypted backup storage can infer file identity -->
<!-- SOURCE: Randomize blob to pack file assignment — restic/restic PR #5295 — https://github.com/restic/restic/pull/5295 — "To prevent an attacker [from guessing] which chunks of a file are in a given pack file, restic can instead randomly assign chunks" -->

### Concurrency

Uploads are issued as parallel `tokio::JoinSet` tasks. The default concurrency is 4 simultaneous Rclone processes. This is configurable via `SyncConfig.max_concurrent`.

### First Push (Vault Initialisation)

When a user creates a new vault locally and pushes for the first time, the cloud has no prior state. The push flow handles this via step 2: "If cloud download fails with NotFound: first push, skip conflict check."

**First push sequence**:

1. User creates vault locally (Phase 2 auth flow):
   - Generate key file, salt, vault header struct
   - Create SQLCipher DB with `sqlcipher_key`
   - `snapshot_counter` initialised to 0 in `manifest_meta`

2. User adds files locally (Phase 3 storage flow):
   - Files encrypted, chunks staged in `staging_dir/`
   - Manifest updated with node and chunk rows

3. User triggers first push:
   - Step 2: `download_blob("manifest/manifest-backup.blob")` returns `NotFound`
   - Conflict check skipped (no prior cloud state)
   - Steps 5–8: upload all staged blobs to `vault/`
   - Step 9: `snapshot_counter` incremented to 1
   - Step 11: manifest backup uploaded (first cloud manifest)
   - Step 12: vault header uploaded (first cloud header)

4. Cloud now contains:
   - `vault-header.json` (plaintext, accessible for new-device bootstrap)
   - `manifest/manifest-backup.blob` (encrypted, `snapshot_counter = 1`)
   - `vault/<uuid>.blob` files (encrypted chunks)

**Invariant**: after the first successful push, the cloud is in a consistent state. A new device can pull the vault header, authenticate, and restore the full vault.

---

## Pull Flow

The pull flow is used for new-device recovery or re-synchronisation after another device has pushed changes.

```
1.  download_blob("vault-header.json", temp_path)
2.  Parse VaultHeader → obtain salt, argon2_params, key_file_blake3
3.  User authenticates: password + USB key file detected via DeviceMonitor
    → Argon2id(password || key_file, salt, params) → master_key
    → HKDF → key_encryption_key, sqlcipher_key, manifest_key
4.  Decrypt and import manifest (Section "Manifest Cloud Backup" download flow)
5.  Open local SQLCipher with sqlcipher_key
6.  Read all chunk rows from manifest → list of (blob_name, blake3_checksum)
7.  Filter to blobs not already present in staging_dir or local blob store
8.  Download in parallel (tokio::JoinSet, max_concurrent = 4):
    For each blob_name:
      a. download_blob("vault/<blob_name>.blob", staging_dir/<blob_name>.blob)
      b. Verify: blake3::hash(downloaded_file) == blake3_checksum from manifest
         On mismatch: delete downloaded file, record error (do not abort)
9.  Report all verification failures to the caller
```

BLAKE3 verification is mandatory before any blob is passed to `decrypt_chunk`. A corrupted or tampered blob is rejected at step 8b. The Poly1305 tag in `decrypt_chunk` (Phase 1) provides a second layer of integrity verification, but the BLAKE3 check is faster and avoids wasting CPU on known-bad data.

---

## Conflict Detection

### Mechanism

`manifest_meta.snapshot_counter` starts at 0 and is incremented by 1 on each successful push. Before pushing, the client compares its local counter against the counter in the current cloud manifest backup.

| Condition | Meaning | Action |
|-----------|---------|--------|
| `cloud == local` | No other device has pushed | Safe to push |
| `cloud > local` | Another device pushed since last sync | Abort push; user must pull first |
| `cloud < local` | Cloud manifest is older than local (rollback or corruption) | Treat as conflict; abort |
| `cloud not found` | First push; no prior manifest | Skip conflict check; proceed |

### Resolution

Arx Runa does not attempt automatic merge. When a conflict is detected:

1. Push is aborted with a user-facing message: "Another device has synced changes since your last sync. Pull first, then push again."
2. The user runs a pull to update the local manifest and `snapshot_counter`
3. If there are any file-level conflicts (e.g., the same file was modified on both devices), the user resolves them manually (keep local, keep remote, or keep both by renaming)
4. The user pushes again

**Rationale for no auto-merge.** The manifest is a SQLCipher database. Merging two encrypted databases requires decrypting both, performing a three-way diff of the `nodes` and `chunks` tables, and resolving ambiguous cases (renamed files, deleted files, concurrent modifications). This is a significant feature with complex edge cases. Detect-and-block with manual resolution is correct, honest about scope, and avoids silent data loss.

---

## Cloud Garbage Collection

When a file is deleted from the vault:

```
1. MetadataStore::delete_node(node_id) returns list of blob_names
   (chunk rows CASCADE-deleted from manifest)
2. For each blob_name:
   delete_blob("vault/<blob_name>.blob")
3. Delete any corresponding staging file if it still exists
4. Failures in step 2 are logged but do not fail the local delete
```

**Best-effort semantics.** An orphaned blob in the cloud is opaque ciphertext with no manifest entry to contextualise it. It costs storage space but does not compromise security. The user can clean orphans via Rclone directly if desired.

---

## Vault Deletion (Full Cloud Cleanup)

When the user permanently deletes a vault, all cloud data must be removed. This is distinct from file-level deletion — it removes the entire vault from the cloud.

```
1. Authenticate (user must have valid session to delete)
2. list_blobs("vault/") → all_vault_blobs
3. For each blob in all_vault_blobs:
   delete_blob("vault/<blob_name>")
4. delete_blob("manifest/manifest-backup.blob")
5. delete_blob("vault-header.json")
6. (Optional) Delete shared/<file_share_id>/ directories if Phase 5 sharing is active
7. Delete local SQLCipher DB
8. Delete local staging directory
9. Delete local cloud-config.json and sync-config.json
10. Zero and drop session keys
```

**Failure handling**: If any cloud deletion fails, Arx Runa reports partial deletion status. The user can retry or manually clean via Rclone. A partially deleted vault cannot be recovered — the manifest backup and vault header are deleted early in the flow, so even if some blobs remain, they are orphaned ciphertext.

**Confirmation UX (Phase 6)**: Vault deletion is irreversible. The UI must require explicit confirmation (e.g., type vault name to confirm). This is a Phase 6 concern.

**Rclone remote cleanup**: The `rclone.conf` entry for this vault's remote is optionally deleted. If the user has other vaults on the same remote, the remote should be preserved. This requires tracking which vaults use which remotes.

---

## Vault Migration

When a user wants to switch cloud providers (e.g., from Google Drive to Backblaze B2), Arx Runa migrates all encrypted blobs without re-encryption. Blobs are opaque ciphertext — they are identical regardless of which provider stores them.

### Migration flow

```
1.  User configures a new Rclone remote via the guided wizard (Section above)
2.  Arx Runa creates a new CloudEndpoint for the target remote
3.  list_blobs("vault/") on the source remote → source_blobs
4.  list_blobs("shared/") on the source remote → source_shared (if Phase 5 sharing is active)
5.  For each blob in source_blobs:
    a. download_blob(source, "vault/<blob_name>.blob", staging_dir/<blob_name>.blob)
    b. upload_blob(staging_dir/<blob_name>.blob, target, "vault/<blob_name>.blob")
    c. Delete staging copy
    d. Report progress via MigrationProgress channel
6.  Repeat step 5 for source_shared blobs (preserving the shared/<file_share_id>/ structure)
7.  Download vault-header.json from source → upload to target
8.  Download manifest/manifest-backup.blob from source → upload to target
9.  Update local cloud-config.json to point to the target remote
10. User verifies migration (browse vault, spot-check files)
11. User optionally deletes blobs from the source remote
```

### Properties

- **No re-encryption**: blobs are transferred as-is. UUID blob names and AEAD ciphertext are unchanged.
- **No key material changes**: the vault header, manifest, and all file keys remain identical.
- **Resumable**: if migration is interrupted, blobs already transferred to the target are valid. Retry re-downloads missing blobs (download/upload are idempotent).
- **Non-destructive**: source blobs are not deleted automatically. The user explicitly decommissions the old remote after verification.

### Concurrency

Migration uses the same `tokio::JoinSet` concurrency model as push/pull, bounded by `SyncConfig.max_concurrent`. Each transfer is one download + one upload, serialised per blob.

### Tauri command

```rust
#[tauri::command]
async fn migrate_vault(
    new_endpoint: CloudEndpointConfig,
    progress: tauri::ipc::Channel<MigrationProgress>,
    state: tauri::State<'_, AppState>,
) -> Result<(), IpcError>;
```

### Scope

Implementation target: Phase 4 (optional enhancement). Not blocking for core push/pull operations.

---

## Error Recovery

### Interrupted push (steps 1–7)

- Blobs already uploaded to `vault/` are harmless (opaque ciphertext, random names)
- `snapshot_counter` has not been incremented (step 9 is after all uploads)
- Staging copies of successfully uploaded blobs have been deleted (step 7b); they will not be re-uploaded on retry
- Remaining staging blobs are uploaded on retry; `upload_blob` is idempotent

### Failed manifest backup upload (step 11)

- `snapshot_counter` was incremented in step 9
- The cloud manifest backup still reflects the old counter
- On retry: step 2 downloads the old manifest backup (lower counter); conflict check (`cloud < local`) would incorrectly trigger
- **Mitigation:** on failure at step 11, roll back `snapshot_counter` to its previous value before returning the error. The next retry starts fresh from step 1.

### Interrupted pull

- Partially downloaded blob files may exist on disk
- On retry, `download_blob` overwrites (idempotent)
- BLAKE3 verification at step 8b catches truncated downloads
- Manifest import (step 4) is based on a single file overwrite; partial import is not possible

### Rclone process crash

- `tokio::process::Command` returns an error or a non-zero exit code
- Maps to `CloudTransportError::RcloneProcessFailed`
- The calling push/pull flow handles the error; the operation can be retried

---

## Security Analysis

### What the cloud provider can observe

| Observable | Mitigation | Notes |
|-----------|------------|-------|
| Total blob count in `vault/` | None (inherent) | Reveals approximate vault size in 4 MiB increments |
| Individual blob size | Uniform padding | All blobs are 4,194,304 + 40 bytes. File sizes cannot be inferred from individual blob sizes |
| Upload/download timing | Partial (randomised order) | Access patterns reveal when user is active; upload order randomisation masks which blobs belong to the same file |
| Blob names | UUID v4 | Random 128-bit names; no relation to file identity, name, or chunk index |
| Blob-to-file association | Partial | Randomised upload order breaks temporal correlation; blob names are random; no manifest link is exposed |
| Vault header contents | By design | Contains only public parameters (see vault header security analysis) |
| Manifest backup existence | None | Provider knows a manifest backup exists; its content is AEAD-encrypted |
| File names, folder structure, content | Full mitigation | All inside SQLCipher (encrypted with `sqlcipher_key`) or XChaCha20-Poly1305 ciphertext |

### Rclone as a trusted component

Rclone runs as a subprocess with the same OS permissions as Arx Runa. It has access to:
- Blob files passed as file path arguments (AEAD ciphertext only — never plaintext)
- The vault header file (plaintext JSON with public parameters only)
- Rclone's `rclone.conf` (cloud credentials)

Rclone does **not** have access to:
- Arx Runa's session keys (in mlocked memory, never passed to a subprocess)
- The SQLCipher database key
- Plaintext file content at any stage

**Threat: malicious Rclone binary.** If an attacker replaces the sidecar binary, they could exfiltrate blob files or modify them in transit. This is equivalent to a compromised OS, which is explicitly out of scope in Arx Runa's threat model. The sidecar binary should be verified against the official Rclone release checksums as part of the Arx Runa release build process.

### Path traversal via crafted manifest

A compromised manifest could contain `blob_name` values such as `../../etc/passwd`. The remote path sanitisation regex (`^[a-zA-Z0-9._/-]+$`, reject `..`) prevents this: no blob name that would escape the cloud root can pass validation.

### Manifest backup replay attack

An attacker who controls the cloud storage could replace `manifest/manifest-backup.blob` with an older version. On recovery, the user would see a stale vault state. The `snapshot_counter` inside the encrypted manifest is monotonic — a push following recovery would detect a divergence if new blobs had been added since the stale snapshot. However, if the attacker also withholds newer blobs, the replay could be internally consistent.

This is an inherent limitation of the "bring your own cloud" model: the cloud provider is trusted for availability but not integrity. Arx Runa declares this out of scope in the threat model, consistent with Tahoe-LAFS's server threat model.
<!-- SOURCE: Tahoe – The Least-Authority Filesystem — Wilcox-O'Hearn & Warner, StorageSS '08 — https://eprint.iacr.org/2012/524 — "The only thing you ask of the servers is that they can (usually) provide the shares when you ask for them: you aren't relying upon them for confidentiality, integrity, or absolute availability." -->

---

## Progress Reporting (Phase 6 Stub)

For large vaults with many blobs, users need feedback during push/pull operations. This section defines the data model; UI implementation is Phase 6.

```rust
/// Progress update sent from the sync layer to the Tauri frontend.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum SyncProgress {
    /// Push/pull operation started.
    Started {
        operation: SyncOperation,
        total_blobs: u64,
        total_bytes: u64,
    },

    /// A single blob completed (uploaded or downloaded).
    BlobCompleted {
        blob_index: u64,       // 1-based for display
        total_blobs: u64,
        bytes_transferred: u64,
    },

    /// Operation finished successfully.
    Completed {
        operation: SyncOperation,
        blobs_transferred: u64,
        total_bytes: u64,
        duration_seconds: f64,
    },

    /// Operation failed.
    Failed {
        operation: SyncOperation,
        blobs_completed: u64,
        error_message: String,  // Sanitised, no credentials
    },
}

#[derive(Debug, Clone, Copy, Serialize)]
pub enum SyncOperation {
    Push,
    Pull,
}
```

**Tauri IPC**: The push/pull Tauri commands accept a `tauri::ipc::Channel<SyncProgress>` parameter. The Rust backend sends progress updates via `channel.send(&progress)?`. The frontend subscribes and updates the UI.

**Frequency**: One `BlobCompleted` event per blob. For very large vaults (1000+ blobs), consider batching (e.g., every 10 blobs) to reduce IPC overhead — this is a Phase 6 tuning decision.

---

## Conflict Representation (Phase 6 Stub)

When a conflict is detected (step 3 of push flow), the user must be informed and guided to resolution. This section defines the data model; UI implementation is Phase 6.

```rust
/// Conflict detected during push attempt.
#[derive(Debug, Clone, Serialize)]
pub struct SyncConflict {
    /// Local snapshot counter at time of push attempt.
    pub local_counter: u64,

    /// Cloud snapshot counter (from downloaded manifest backup).
    pub cloud_counter: u64,

    /// Timestamp of last successful sync on this device.
    pub local_last_synced: Option<i64>,  // Unix timestamp

    /// Timestamp in the cloud manifest (when another device pushed).
    pub cloud_last_synced: Option<i64>,
}
```

**Resolution flow (Phase 6 UI)**:

1. Push returns `Err(SyncError::Conflict(SyncConflict { ... }))`
2. Frontend displays: "Another device synced changes. Your local changes: X files. Pull to see remote changes, then push again."
3. User clicks "Pull" — manifest is updated, `snapshot_counter` synced
4. If file-level conflicts exist (same `node_id` modified in both), Arx Runa lists them:
   - "File `documents/report.pdf` was modified on both devices"
   - Options: "Keep Local", "Keep Remote", "Keep Both (rename local)"
5. User resolves each conflict
6. User pushes again

**File-level conflict detection** (future enhancement): After pull, compare `modified_at` timestamps for nodes that exist in both local (pre-pull) and remote manifests. If both changed since last sync, flag as file-level conflict. This is **out of scope** for the bachelor project — the current design only detects manifest-level conflicts via `snapshot_counter`.

---

## Testing Strategy

### MockCloudTransport

```rust
/// In-memory CloudTransport for unit and integration tests.
///
/// Stores blobs in a HashMap keyed by remote path. Optionally injects
/// failures for specific paths to test error recovery.
struct MockCloudTransport {
    /// Remote path → blob content.
    blobs: Arc<Mutex<HashMap<String, Vec<u8>>>>,

    /// Paths for which operations should return a simulated error.
    failure_paths: Arc<Mutex<HashSet<String>>>,
}
```

### Unit tests (using MockCloudTransport)

- `test_upload_blob_stores_content_at_remote_path`
- `test_download_blob_retrieves_previously_uploaded_content`
- `test_download_blob_not_found_returns_error_variant`
- `test_delete_blob_removes_content_from_remote`
- `test_delete_blob_nonexistent_path_is_idempotent`
- `test_list_blobs_returns_paths_matching_prefix`
- `test_push_flow_uploads_all_staged_blobs`
- `test_push_flow_increments_snapshot_counter_after_upload`
- `test_push_flow_deletes_staging_copies_on_success`
- `test_push_flow_aborts_on_conflict_when_cloud_counter_exceeds_local`
- `test_push_flow_rolls_back_snapshot_counter_on_manifest_upload_failure`
- `test_push_flow_randomises_blob_upload_order`
- `test_pull_flow_downloads_missing_blobs`
- `test_pull_flow_rejects_blob_with_blake3_checksum_mismatch`
- `test_pull_flow_continues_after_single_blob_verification_failure`
- `test_vault_header_round_trip_serialise_upload_download_parse`
- `test_vault_header_validation_rejects_undersized_salt`
- `test_vault_header_validation_rejects_argon2_params_below_owasp_minimum`
- `test_manifest_backup_encrypt_upload_download_decrypt_round_trip`
- `test_remote_path_sanitisation_rejects_path_traversal`
- `test_remote_path_sanitisation_rejects_absolute_path`

### Integration test (local Rclone remote)

```
Setup:
  1. tempfile::tempdir() as the local remote root
  2. rclone config create testremote local nounc true (via sidecar)
  3. Initialise CloudEndpoint pointing to tempdir

Test: full push → pull → decrypt cycle
  1. Create test file (1 byte, 1 chunk, 8 MiB + 1 byte spanning two chunks)
  2. Encrypt file → staging blobs
  3. Push: upload blobs, upload manifest backup, upload vault header
  4. Verify vault-header.json exists and parses correctly at remote root
  5. Verify vault/<blob_names>.blob exist at remote
  6. Verify manifest/manifest-backup.blob exists at remote
  7. Delete local manifest and staging blobs (simulating new device)
  8. Pull: download vault header, authenticate (mock), decrypt manifest
  9. Download blobs, verify BLAKE3 checksums
  10. Decrypt file, verify content matches original

Cleanup:
  Drop tempdir (Rclone remote data gone with it)
```

---

## Open Decisions

| Decision | Options | Status |
|----------|---------|--------|
| Rclone minimum version | Require a specific minimum version vs. best-effort compatibility with whatever is bundled | Not blocking — sidecar pins the version |
| Manifest backup compression | Compress with zstd before encryption to reduce upload size vs. encrypt directly (simpler) | Deferred; manifests are small in the bachelor project scope |
| Upload order within a single file's chunks | Already randomised globally; consider always uploading chunk 0 last to delay any partial-file recovery attempts | Extension point |
| Multi-device concurrent push handling | Beyond detect-and-block, explore optimistic locking or conditional writes where the provider supports them | Out of scope for bachelor project |

---

## Decisions Made

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Rclone distribution | Tauri sidecar (bundled binary) | No user installation step; Tauri handles platform detection and path resolution; MIT license permits bundling |
| Rclone invocation | `tokio::process::Command`, no shell | Prevents shell injection; arguments are separate OS strings |
| Rclone config location | Arx Runa-specific `%APPDATA%/arx-runa/rclone.conf` | Isolated from system Rclone; prevents credential conflicts; self-contained uninstall |
| Remote path validation | Regex allowlist `^[a-zA-Z0-9._/-]+$`, reject `..` | Prevents path traversal from crafted manifest data |
| Cloud storage layout | `vault-header.json` at root, `vault/` for blobs, `manifest/` for backup | Vault header accessible before auth; clean separation of concerns |
| Upload order | Randomised (Fisher-Yates shuffle) | Breaks temporal correlation that could link blobs to files |
| Upload concurrency | `tokio::JoinSet`, default 4 concurrent tasks | Maximises throughput; each task runs one Rclone sidecar process |
| Concurrency/timeout config | Separate `SyncConfig` struct | Separates "where" (CloudEndpoint) from "how" (SyncConfig); allows same tuning across vaults |
| Operation timeouts | 5 min (upload/download), 30 sec (delete), 60 sec (list) | Generous for slow connections; managed by Tokio, not Rclone internal timeout |
| Manifest backup format | Single file, overwritten on each push | Simple; logical version is `snapshot_counter` inside the manifest |
| Manifest backup encryption | XChaCha20-Poly1305, `manifest_key`, random nonce, no AAD | Manifest is a singleton; no file_id or chunk_index for AAD binding |
| Manifest I/O | Full buffer (explicit streaming exception) | Manifests are small (< 10 MiB); single AEAD operation is simpler and correct |
| Conflict detection | `snapshot_counter` comparison, detect-and-block | Correct; auto-merge is out of scope for this project |
| Conflict resolution | Manual — user pulls, resolves, then pushes | Avoids silent data loss; honest scope declaration |
| Remote configuration | Guided wizard calls `rclone config create` | Better UX than raw `rclone config`; Arx Runa never holds credentials |
| Initial provider support | S3-compatible + Google Drive | Covers the most common use cases (privacy-focused and consumer) |
| Stderr sanitisation | Strip lines containing credential keywords | Prevents `rclone.conf` leakage through error messages surfaced to frontend |
| Cloud blob deletion | Best-effort; failures logged, not blocking | Orphaned ciphertext is harmless; availability is more important than strict cleanup |
| Vault header upload on every push | Yes (unconditional, idempotent) | Ensures cloud header stays current after password change or key rotation without a separate trigger |
| Vault deletion | Explicit flow: delete all blobs, manifest backup, vault header | Clean cloud state; user must confirm (Phase 6 UX) |
