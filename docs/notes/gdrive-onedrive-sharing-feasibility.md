# Google Drive / OneDrive Sharing Feasibility

## Context

Sharing in Arx Runa works by generating scoped credentials that a recipient embeds in an
rclone config to download ciphertext blobs. Backblaze B2 implements this cleanly via a
scoped application key (limited to a specific bucket prefix, read-only, time-limited).

**Sharing is currently blocked for non-B2 destinations** — attempting to share from a Google
Drive or OneDrive primary destination returns a clear error. This note records what
implementing sharing for those providers would actually require.

---

## Google Drive

### Why it doesn't work today

The rclone `drive` backend has no `--drive-api-key` option — it always requires OAuth or a
Service Account JSON. "Anyone with the link" (`type=anyone`) permissions via the Drive API
cannot be time-limited (`expirationTime` only applies to `type=user` and `type=group`).
`rclone link` does not honour `--expire` on Drive.

### Best feasible approach: Service Account JSON in share package

This is the closest analogue to B2's scoped key.

**One-time owner setup:**
1. Create a GCP project, enable the Drive API.
2. Create a Service Account (IAM → Service Accounts → Create).
3. Generate and download the Service Account JSON key.
4. Store the SA email and JSON in the app's persistent config.

**Per-share flow:**
1. Owner calls `POST /drive/v3/files/{share_folder_id}/permissions` with:
   ```json
   {
     "type": "user",
     "role": "reader",
     "emailAddress": "sa@project.iam.gserviceaccount.com",
     "expirationTime": "2025-08-01T00:00:00.000Z"
   }
   ```
   `type=user` supports `expirationTime` (max 1 year).
2. Embed `{ sa_credentials_json, root_folder_id: share_folder_id }` inside the HPKE-encrypted
   share package.
3. Recipient's app decrypts, constructs rclone config:
   ```ini
   [share_remote]
   type = drive
   service_account_credentials = {"type":"service_account","private_key":"..."}
   root_folder_id = SHARE_FOLDER_ID
   scope = drive.readonly
   ```
4. Recipient runs `rclone copy share_remote: /local/destination/`.

**Properties:**
- ✅ Recipient needs no Google account
- ✅ Full rclone folder listing and recursive download
- ✅ Time-limited via `expirationTime` on `type=user` permission (max 1 year)
- ✅ Zero-knowledge: SA JSON lives inside the HPKE envelope; SA only sees the shared folder
- ✅ SA can be revoked (delete the SA key) at any time to revoke access
- ⚠️ Owner must perform one-time GCP project + SA setup before sharing is available
- ⚠️ SA JSON is a long-lived credential; compromise of the HPKE private key would expose it
  (mitigate by deleting the SA key after share TTL, or by using short-lived token exchange)

The SA approach is solid enough to implement. The main friction is the owner setup UX.

**Alternative (per-file only, no rclone listing):** Set `type=anyone` and use
`https://www.googleapis.com/drive/v3/files/{id}?alt=media&key={API_KEY}` via `rclone copyurl`
per blob. No folder listing, no time-limit, not recommended.

---

## Microsoft OneDrive

### OneDrive Personal

`createLink` with `scope=anonymous, expirationDateTime` works. The resulting `webUrl`
can be embedded in the share package. Recipient can attempt `rclone copyurl` with
`?download=1` appended, but this relies on Microsoft's redirect chain behaving
consistently for automation — not verified and subject to change.

**Properties:**
- ⚠️ Anonymous download via `rclone copyurl` is unreliable in automation
- ✅ `expirationDateTime` supported with Microsoft 365 subscription
- ✅ No OAuth needed by recipient (in theory)
- ❌ No native rclone remote that works without OAuth

### OneDrive Business / SharePoint

**Fundamentally broken for anonymous access.** From the Graph API documentation:
> "For OneDrive for Business and SharePoint, the Shares API always requires authentication
>  and can't be used to access anonymously shared content without a user context."

Even if an anonymous `createLink` is created, a Graph API call to download it requires
a `Bearer` token. Tenant admins frequently disable anonymous links entirely.

**Application permissions (Azure AD App Registration)** require `Files.ReadWrite.All`
which needs tenant admin consent — not viable for general consumer use.

### rclone serve http (P2P, works for both)

The owner runs `rclone serve http` pointing at the share folder while the recipient
downloads. The endpoint URL + credentials are embedded in the share package.

```bash
# Owner
rclone serve http onedrive:shared/{file_share_id} \
  --addr :8443 --cert cert.pem --key key.pem \
  --user shareuser --pass RANDOM_PASSWORD --read-only

# Recipient rclone config
[share_http]
type = http
url = https://shareuser:RANDOM_PASSWORD@owner-host:8443
```

**Properties:**
- ✅ Works with any rclone backend (Drive, OneDrive, local)
- ✅ Recipient needs no cloud account
- ❌ Owner machine must be online and reachable during transfer
- ❌ NAT/firewall traversal required (open port, or Tailscale/ngrok/Cloudflare Tunnel)
- ❌ Synchronous only — async sharing not possible

---

## Consolidated Summary

| Property                         | B2 (current) | Google Drive SA | OneDrive Personal | OneDrive Business |
|----------------------------------|:---:|:---:|:---:|:---:|
| Recipient needs cloud account    | ❌  | ❌  | ❌  | ✅ (or ❌ Business) |
| Time-limited credential          | ✅  | ✅ (1yr max) | ⚠️ M365 only | ⚠️ M365 + admin |
| Full rclone folder listing       | ✅  | ✅  | ❌  | ❌  |
| Works today in Arx Runa          | ✅  | ❌  | ❌  | ❌  |
| Feasibility of implementation    | —   | Medium | Low | Very Low |

---

## Recommendations

1. **Google Drive** — implement via Service Account JSON. Feasible with moderate effort.
   The main design question is UX for the one-time GCP setup (wizard step in the destination
   configuration screen). The share-time flow is largely mechanical once the SA JSON is stored.

2. **OneDrive Personal** — defer. The anonymous link download path is unreliable for
   automation. Revisit if Microsoft stabilises the `rclone copyurl` behaviour for OneDrive
   sharing URLs, or if rclone adds native support for OneDrive anonymous sharing.

3. **OneDrive Business** — not feasible without tenant admin involvement. Out of scope
   for the foreseeable future.

4. **rclone serve http** — useful escape hatch but violates the async sharing design
   (recipient can't download when owner is offline). Consider only as an explicit
   "online transfer" mode distinct from the normal share flow.

---

## Key Sources

- Google Drive permissions API: `POST /drive/v3/files/{fileId}/permissions`
  — `expirationTime` docs: https://developers.google.com/workspace/drive/api/reference/rest/v3/permissions#Permission.FIELDS.expirationTime
- rclone drive backend (no `--drive-api-key`): https://rclone.org/drive/
- rclone serve http: https://rclone.org/commands/rclone_serve_http/
- rclone HTTP backend (no-config usage): https://rclone.org/http/
- OneDrive createLink: https://learn.microsoft.com/en-us/graph/api/driveitem-createlink
- OneDrive Shares API auth requirement: https://learn.microsoft.com/en-us/graph/api/shares-get
