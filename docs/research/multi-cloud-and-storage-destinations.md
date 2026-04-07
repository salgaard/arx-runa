# Arx Runa: Multi-Cloud and Storage Destination Management

> **Document type**: Exploration / feasibility research
> **Status**: Concluded
> **Last updated**: 2026-04-06

Investigates how Arx Runa can support multiple storage destinations (cloud providers, external drives, local paths) with session-based credential management, a cloud file browser, and cross-provider vault backup.

For background on cloud provider cost comparisons, see `compression-and-cloud-cost.md`.
For background on market positioning and Rclone integration, see `market-and-future-directions.md`.

---

## Table of Contents

1. [The Problem](#the-problem)
2. [Scope of Features](#scope-of-features)
3. [Rclone as the Universal Backend](#rclone-as-the-universal-backend)
4. [Session-Based Credential Management](#session-based-credential-management)
5. [Cloud File Browser](#cloud-file-browser)
6. [Vault Backup to a Secondary Destination](#vault-backup-to-a-secondary-destination)
7. [Privacy and Zero-Knowledge Analysis](#privacy-and-zero-knowledge-analysis)
8. [Recommendation](#recommendation)
9. [Decisions](#decisions)
10. [Open Questions](#open-questions)
11. [Sources](#sources)

---

## The Problem

Arx Runa currently supports BYOC via Rclone but the model is vault-centric and single-destination: each vault maps to one configured Rclone remote. This leaves several user needs unmet:

1. **Multiple providers** — power users spread data across Backblaze B2, Google Drive, S3, a NAS, and a USB drive. Today each would require a separate vault with its own Argon2id derivation and master key.
2. **Cloud manager** — there is no UI for browsing what already lives on a configured remote. Users can't inspect, pick, or download individual encrypted files.
3. **Destination flexibility** — "cloud" shouldn't be the only option. A local path (`D:\archive`) or an external drive (`/Volumes/Backup`) should be first-class destinations.
4. **Cross-provider vault backup** — users want a one-click "back my whole vault up somewhere else" without manually rclone-copying blobs and losing the manifest.

---

## Scope of Features

The following feature areas are in scope for this research:

| Feature | Description |
|---|---|
| **Multi-destination vaults** | A single vault can push blobs to more than one destination |
| **Destination sessions** | Named, persisted Rclone remotes stored in encrypted config |
| **Cloud file browser** | Browse blobs on a configured remote, see metadata, decrypt and download |
| **Destination types** | Cloud (Rclone backends), external drive, local path |
| **Full vault backup** | Export an entire vault as a portable archive to a secondary destination |
| **Selective restore** | Browse a remote vault backup and restore individual files |

---

## Rclone as the Universal Backend

Arx Runa's existing BYOC stance uses [Rclone](https://rclone.org/) as the storage abstraction layer. Rclone supports 70+ backends including:

- **Object stores**: S3 (+ all S3-compatible), GCS, Azure Blob, Backblaze B2, Cloudflare R2, Wasabi, IDrive e2
- **Consumer cloud**: Google Drive, OneDrive, Dropbox, Box, iCloud Drive (read-only), pCloud
- **Self-hosted / local**: SFTP, WebDAV, SMB/CIFS, FTP, local filesystem
- **Specialized**: Storj (decentralized), Sia, Tardigrade, Internet Archive

This means Arx Runa does not need to implement provider-specific APIs. Every destination — cloud, NAS, USB drive, local folder — is modelled as an Rclone remote.

**Key Rclone capabilities relevant here:**

- `rclone lsjson <remote>:<path>` — list blobs with metadata (name, size, modified time, hash)
- `rclone copyto <src> <dst>` — copy a single blob between remotes
- `rclone sync <src> <dst>` — mirror a path to another destination
- `rclone config` — manage named remotes with encrypted credential storage

<!-- TODO: verify: rclone's encrypted config uses NaCl secretbox — confirm this is compatible with Arx Runa's threat model for credential storage -->

---

## Session-Based Credential Management

### Concept

A **destination session** is a named, persisted connection to a storage backend. In Arx Runa, this would be stored as an encrypted entry in the existing SQLCipher vault database — similar to how a password manager stores site credentials.

Each session record would contain:
- A human-readable name ("My Backblaze B2", "Home NAS", "USB Archive")
- An Rclone remote configuration blob (serialized Rclone config section)
- A destination type tag (cloud / drive / local)
- An optional default path prefix within the remote

### Credential Storage Security

Rclone's own config file can be encrypted with `rclone config` using NaCl secretbox, but this creates a second key management problem. The better approach for Arx Runa is to store the Rclone remote config as an encrypted blob inside the existing SQLCipher database, unlocked by the user's session key (already derived via Argon2id).

This means:
- No second password to remember
- Credentials at rest are protected by the same Argon2id-hardened key chain
- When the session closes and the master key is zeroized, credentials are inaccessible

### Adding a Destination Session

Proposed UX flow:
1. User clicks **Add Destination**
2. Arx Runa shows a list of backend types (with icons)
3. User fills in provider-specific fields (or pastes an Rclone config snippet)
4. Arx Runa validates by issuing a `rclone lsd` probe call
5. On success, the config is encrypted and saved to the vault DB

---

## Cloud File Browser

### What the User Sees

A file browser that lists files on a configured remote using the vault's manifest to map blob UUIDs to real filenames and folder structure. Requires the vault to be unlocked. Raw blob view is not exposed in the UI — users who want to inspect UUIDs directly can do so in their cloud provider's own interface.

### Upload Flow

Consistent with the Auto-Sync Drop Zone vision in `market-and-future-directions.md`:
- User drops files/folders onto the Arx Runa UI (or clicks to browse)
- Arx Runa reads directly from source, encrypts in RAM, uploads chunks to the configured destination
- No temp files written — zero-trace
- Source file remains unchanged (or optionally deleted after upload)

Re-upload to a secondary destination is handled by the Rclone sync mirror (Option A) — not a manual per-file operation in the browser.

### Operations from the Browser

| Operation | Description |
|---|---|
| **Download + decrypt** | Fetch blob(s), decrypt in RAM, write plaintext to user-chosen path — combined single action |
| **Delete** | Remove blob from remote with manifest update |
| **Verify** | Check AEAD tag integrity without full decrypt — optional, power-user feature |

### Privacy Considerations

The blob browser must not display any identifying information to an unauthenticated user. Raw blob view shows only UUIDs — zero file-name leakage. Manifest-linked view requires an unlocked vault, which requires the user's passphrase.

---

## Vault Backup to a Secondary Destination

### The Need

Users want a "back up everything" option: take the full vault (all encrypted blobs + the manifest) and copy it to a second destination in one action. This guards against primary cloud provider failure, accidental deletion, or account loss.

### Option A: Blob Mirror (Rclone Sync)

Use `rclone sync` to mirror the primary destination to a secondary one. Blobs are already encrypted, so this is safe. The manifest (also encrypted, stored as a blob) goes along for free.

**Pros:**
- Incremental — only new/changed blobs are transferred on subsequent runs
- No additional wrapping or format change
- Backup destination is a fully functional Arx Runa vault — can restore directly

**Cons:**
- Requires the secondary destination to be an Rclone remote
- Does not produce a single self-contained file (no "one file to hand to someone")

### Option B: Vault Archive (Compressed Blob Bundle)

Package all vault blobs + the manifest into a single archive file (e.g., `.tar.zst` or `.zip`), then upload/copy that archive to the secondary destination or an external drive.

**Compression note:** Blobs are already XChaCha20-Poly1305 ciphertext — they are indistinguishable from random bytes and **will not compress**. Applying zstd/zip deflate to ciphertext wastes CPU and gains 0 bytes. However, the archive format itself (tar/zip directory structure) may benefit from zstd as a container. See `compression-and-cloud-cost.md` for the compression/encryption ordering analysis.

**Practical recommendation:** Use `tar` (no compression) or `zip` (store mode, no deflate) as a container. Compression is a no-op on ciphertext.

**Pros:**
- Single portable file — easy to hand off, store on USB, attach to S3 Glacier deep archive
- Familiar format (zip) — extractable without Arx Runa if needed for disaster recovery
- Timestamp in filename provides point-in-time snapshots

**Cons:**
- Full copy every time (unless incremental archives are implemented separately)
- Large vaults → large single file (slow upload, large on destination)
- Not directly mountable as a live vault — requires extraction first

### Option C: Hybrid — Mirror + Periodic Snapshot Archive

- Normal operation: Rclone sync for incremental backup to secondary destination
- On-demand: generate a snapshot archive for offline/airgapped storage

This gives users both: a live mirror that stays current, and a point-in-time portable snapshot.

### Archive Format Recommendation

If Option B or C is implemented:

| Format | Extension | Pros | Cons |
|---|---|---|---|
| tar (no compress) | `.tar` | Simple, no wasted CPU | No native Windows extraction without tooling |
| zip (store mode) | `.zip` | Universal, Windows-native | Slightly larger header overhead |
| tar.zst | `.tar.zst` | Small overhead files compress well | Less familiar to casual users |

**Recommended**: `.zip` in store mode (no deflate). Universally extractable, no compression waste on ciphertext, and the manifest (which is small) compresses well within the zip container.

---

## Privacy and Zero-Knowledge Analysis

| Concern | Analysis |
|---|---|
| Credential exposure | Rclone remote configs stored encrypted in SQLCipher DB, unlocked only during active session |
| Blob names on secondary destination | Same UUID blob naming — no filename leakage at rest |
| Archive filename | Timestamp in archive name leaks backup frequency — consider using a random UUID for the archive filename |
| Browse operation metadata | `rclone lsjson` reveals blob sizes and timestamps to the Arx Runa process — acceptable since the user already has vault access |
| Cross-provider transfer | Blobs pass through the local machine in encrypted form — plaintext never leaves RAM |
| Vault archive integrity | The archive should include a SHA-256 manifest of all included blob hashes so restoration can be verified |

---

## Recommendation

Arx Runa should extend its existing BYOC + Rclone foundation into a first-class multi-destination system. The architecture is straightforward because the hard parts are already in place: Rclone as a sidecar handles all 70+ backends, the manifest is already uploaded as an encrypted blob on the remote, and the vault is already a self-contained unit.

### Destination Sessions

Store named Rclone remote configurations as encrypted entries in the vault's existing SQLCipher database, unlocked by the session key. Each vault maintains its own destination list (vault-scoped). The UI offers guided setup forms for the five most common backends (Backblaze B2, S3-compatible, Google Drive, OneDrive, local path / external drive) with an "Advanced: paste Rclone config" fallback for all other backends.

Each vault has exactly **one primary destination** (where uploads go) and **N backup destinations** (synced on demand or on schedule).

### Cloud File Browser

A manifest-linked file browser (vault must be unlocked) that shows real filenames and folder structure by mapping blob UUIDs via the local manifest. Raw blob view is not exposed in the UI. Supported operations:

- **Download + decrypt** — fetch blob(s), decrypt in RAM, write plaintext to user-chosen path
- **Delete** — remove blob from remote with manifest update
- **Verify** — check AEAD tag integrity without full decrypt (optional, power-user feature)

Upload remains the existing drop zone flow: files are read from source, encrypted in RAM, uploaded as chunks — no temp files, zero-trace.

### Vault Backup

Backup is an `rclone sync` mirror from the primary remote to one or more backup destinations. Because the primary remote already contains `vault-header.json`, `manifest/manifest-backup.blob`, and all `vault/<uuid>.blob` chunks, the backup destination is a complete, immediately restorable vault — no special packaging required.

Sync behaviour:
- **Default**: mirror mode — backup matches current state of primary (deleted files are deleted on backup too)
- **Opt-in per destination**: accumulating mode — blobs are never deleted from backup (historical archive)
- **Trigger**: manual ("Sync now" button) + optional schedule (daily / weekly / monthly)

### Why this works within the zero-knowledge model

- Credentials at rest: encrypted in SQLCipher, inaccessible without the session key
- Blobs on all destinations: UUID-named ciphertext — no filename or structure leakage
- Transfer path: blobs pass through the local machine in encrypted form — plaintext never leaves RAM
- Manifest on backup: already encrypted with `manifest_key` (HKDF-derived), safe to store on any destination

---

## Decisions

> Choices made during this research session. Updated as the session progresses.

| Decision | Alternatives considered | Rationale |
|---|---|---|
| **Vault backup uses Rclone sync mirror (Option A)** | Snapshot archive (Option B), Hybrid (Option C) | Incremental transfers keep the secondary destination current without full re-uploads; backup destination is a fully functional vault that can be restored directly |
| **Destination sessions are vault-scoped** | Global shared credential store, hybrid pool | Keeps each vault self-contained; no shared credential store outside the vault; fits existing SQLCipher-per-vault model |
| **Rclone integration stays sidecar binary** | Go FFI / Rclone as library | Consistent with existing BYOC approach; avoids Go+Rust CGo complexity; `lsjson` JSON output is easy to parse in Rust |
| **Browser shows manifest-linked view only (no raw blob view in UI)** | Raw blob UUID view, both modes | Users who need raw blob inspection can use their cloud provider's UI; manifest-linked is what users actually need |
| **Browser operations: download+decrypt (combined), delete, verify** | Separate download and decrypt steps, re-upload per-file | Download+decrypt is a single user intent; re-upload to secondary is handled by Rclone sync mirror, not per-file; verify is useful for integrity checks |
| **Upload via drop zone (encrypt in RAM, no temp files)** | FUSE virtual drive, filesystem-level sync | Drop zone gives full control over data flow, preserves zero-trace guarantee; consistent with Auto-Sync vision in market-and-future-directions.md |
| **One primary destination + N backup destinations per vault** | Write-all multi-primary | Write-all blocks on slow/offline destinations; one primary keeps uploads simple; backup sync is explicit (manual or scheduled), which handles intermittently connected destinations (e.g. USB drives) |
| **Backup sync trigger: manual + optional schedule (daily/weekly/monthly)** | Manual only, post-upload auto-sync | Manual always available for all users; optional schedule lets power users automate without requiring a full cron UI |
| **Backup sync mode: mirror (delete on backup) by default, accumulating as opt-in per destination** | Always mirror, always accumulate | Mirror keeps backup = current vault state (consistent with rclone sync semantics); accumulating opt-in for users who want a recoverable history |
| **Destination setup: hybrid — guided forms for top backends + paste Rclone config for everything else** | Forms only, paste only | Forms cover the common case (B2, S3-compatible, Google Drive, OneDrive, local path); paste fallback handles all 70+ Rclone backends without maintenance burden |

---

## Open Questions

- ~~Should a vault backup archive include the SQLCipher database?~~ — Resolved: the manifest is already uploaded as `manifest/manifest-backup.blob` on the primary remote (see `docs/architecture/designs/cloud-synchronisation/design.md`). `rclone sync` picks it up automatically; no special handling needed.
- Should destination sessions be vault-scoped (per vault) or global (shared across vaults for the same user)?
- Can Rclone be embedded as a library or must it be a sidecar binary? (The existing BYOC approach uses a sidecar — does that scale to a rich cloud browser UI?)
- What is the UX for selecting which destinations a vault syncs to — primary only, primary + backup, all?
- Should the blob browser support filtering/searching by real filename (requires unlocked vault) vs. UUID only?
- For archive backups: should the archive be encrypted at the archive level (second layer of encryption) or rely solely on the per-blob AEAD encryption already present?
- How should revoked/deleted files be handled in the mirror — should the secondary mirror also delete, or keep tombstones?

---

## Sources

| Source | Topic | URL |
|---|---|---|
| Rclone documentation | Supported backends (70+), lsjson, sync, config | https://rclone.org/docs/ |
| Rclone config encryption | NaCl secretbox for rclone.conf | https://rclone.org/docs/#configuration-encryption |
| Arx Runa: Compression and Cloud Cost | Compression/encryption ordering, provider cost analysis | `compression-and-cloud-cost.md` |
| Arx Runa: Market & Future Directions | BYOC Rclone, provider landscape, Auto-Sync Drop Zone | `market-and-future-directions.md` |
| Arx Runa: Cloud Synchronisation Design | Manifest blob layout, push/pull flow, manifest-backup.blob format | `docs/architecture/designs/cloud-synchronisation/design.md` |
| Arx Runa: Chunking and Manifest Design | SQLCipher manifest schema, blob naming, zero-trace constraints | `docs/architecture/designs/chunking-and-manifest/design.md` |
