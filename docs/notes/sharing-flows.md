# Sharing Flows by Destination Type

## How sharing works (all providers)

Sharing embeds scoped cloud credentials inside an HPKE-encrypted share package (`.arxshare`
file). The recipient uses those credentials in a temporary rclone config to download the
ciphertext blobs they already hold the decryption key for. Blobs are uploaded to
`shared/<file_share_id>/` under the vault's storage root before the package is generated.

The sender's own vault key material never leaves the vault. The recipient downloads opaque
blobs and decrypts them locally.

---

## Backblaze B2

### One-time setup
None. B2 scoped keys are generated programmatically from the master application key already
stored in the rclone config.

### Per-share flow
1. Blobs uploaded to `shared/<file_share_id>/` in the B2 bucket.
2. `generate_share_credentials()` calls the B2 Keys API to create a **scoped application
   key** restricted to `shared/<file_share_id>/`, read-only, with a TTL up to 7 days.
3. The key is embedded in the HPKE envelope as:
   ```json
   { "provider": "b2", "key_id": "…", "application_key": "…",
     "bucket": "…", "path_prefix": "shared/<uuid>/" }
   ```

### Recipient download
The app detects `provider == "b2"`, writes a temp rclone config using the scoped key, and
downloads blobs relative to the path prefix. The temp config is deleted after download.

### Revocation
On last-recipient revoke: blobs are deleted from B2, then the scoped application key is
deleted via the B2 Keys API (`b2_delete_key`). The key immediately stops working for any
recipient who still holds it.

On partial revoke (one of several recipients): DB row only — the scoped key is shared
across all recipients of the same file, so access ends when the last recipient is revoked
and the key is deleted.

### Properties
| Property | Value |
|---|---|
| Per-recipient credential | Yes — each file gets one scoped key shared by all recipients of that file |
| Full revocation | Blobs deleted + key deleted |
| Partial revocation | DB mark only |
| TTL | Up to 7 days |
| Owner setup required | None |

---

## Google Drive

### One-time setup (owner)
The owner creates a GCP Service Account, downloads the SA JSON key, and stores it in the
vault via the "Google Drive Sharing" section on the Destinations page. The SA JSON is stored
encrypted in the vault DB and never logged.

### Per-share flow
1. Blobs uploaded to `shared/<file_share_id>/` in Google Drive via rclone.
2. `generate_share_credentials()`:
   - Refreshes the owner's Drive OAuth token.
   - Walks the path via Drive Files.list to resolve the folder ID (retries once after 2 s
     for Drive propagation lag).
   - Grants the Service Account `reader` permission on the folder with no expiry set
     (Google Drive rejects `expirationTime` on personal My Drive items; revocation instead
     deletes the permission explicitly via the Drive API).
   - Returns a JSON blob embedded in the HPKE envelope:
     ```json
     { "provider": "drive", "folder_id": "…", "sa_credentials_json": "…",
       "path_prefix": "shared/<uuid>/", "permission_id": "…" }
     ```

### Recipient download
The app detects `provider == "drive"`, writes a temp rclone config with
`service_account_credentials` (compact inline JSON) and `root_folder_id` set to the shared
folder. Blobs are addressed as bare UUIDs relative to that root. The temp config is deleted
after download.

### Revocation
On last-recipient revoke: blobs are deleted from Drive, then the SA permission on the folder
is deleted via `DELETE /drive/v3/files/{folder_id}/permissions/{permission_id}`. The owner's
OAuth token is refreshed to perform this call.

On partial revoke: DB mark only — the SA credential is shared across all recipients of the
same file; effective access ends when the last recipient is revoked and the blobs disappear.

### Properties
| Property | Value |
|---|---|
| Per-recipient credential | No — SA JSON shared across all recipients of the same file |
| Full revocation | Blobs deleted + Drive permission deleted |
| Partial revocation | DB mark only |
| TTL | None — revocation is sweep-based (blob deletion + permission deletion) |
| Owner setup required | Yes — GCP Service Account + JSON key |

---

## Unsupported destinations

When sharing is attempted from a destination that does not support credential generation,
the backend returns `SharingError::SharingNotSupported`, which surfaces to the user as an
error message.

### OneDrive Personal
Deferred. Anonymous download via rclone is unreliable and there is no programmatic way to
issue scoped credentials without requiring the recipient to authenticate with Microsoft.

### OneDrive Business / SharePoint
Not feasible without tenant admin consent. The required Graph API permission scopes
(`Files.ReadWrite.All` or equivalent) must be pre-approved at the organisational level.

### Local path and external drive destinations
Not supported — there is no credential model for local filesystem access. Sharing requires
cloud storage reachable by both sender and recipient.

### Custom rclone remotes
Not supported. The credential generation logic is provider-specific (B2 Keys API, Drive
Permissions API). A generic rclone remote has no equivalent API surface.

### Effect on the UI
The share action completes locally (HPKE package is created) but the credential embedding
step fails and the error is shown before the package is saved. No `.arxshare` file is
produced. No blobs are uploaded to a shared prefix for unsupported destinations.

---

## Comparison table

| Destination | Sharing supported | Credential type | Per-recipient | Full revocation |
|---|---|---|---|---|
| Backblaze B2 | Yes | Scoped application key | File-scoped | Key deletion |
| Google Drive | Yes (requires SA setup) | Service Account permission | File-scoped | Permission deletion |
| OneDrive Personal | No | — | — | — |
| OneDrive Business | No | — | — | — |
| Local / external drive | No | — | — | — |
| Custom rclone | No | — | — | — |
