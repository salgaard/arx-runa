---
timestamp: "2026-03-30T02:25:02+0200"
type: decision
report-sections:
  - method
  - discussion
tags:
  - cloud-sync
  - rclone
  - transport
  - conflict-detection
source: agent
commit: "5d71df7"
---

## Phase 4 Cloud Synchronisation Design Decisions

## Context

The Phase 4 cloud synchronisation design document was written to define how VoidGate moves encrypted blobs between local staging and a remote cloud backend. The design covers five interconnected choices: how Rclone is distributed to end users, how the cloud remote is configured, how upload order affects metadata privacy, how concurrent device usage is handled, and how the manifest backup is encrypted. Each decision has security and usability implications relevant to the zero-knowledge guarantee.

## Substance

### Rclone distributed as a Tauri sidecar

Rclone is bundled with VoidGate as a Tauri external binary (sidecar), rather than requiring users to install it separately. Tauri's sidecar mechanism handles platform detection, binary extraction, and path resolution at runtime. Rclone is MIT-licensed, which permits bundling without restriction.

The subprocess invocation uses `tokio::process::Command` with arguments passed as a `Vec<OsString>`. No shell interpolation occurs at any stage. Remote paths are validated against a strict allowlist (`^[a-zA-Z0-9._/-]+$`) before being passed to Rclone, preventing path traversal from crafted manifest data.
<!-- SOURCE: Path Traversal | OWASP Foundation — https://owasp.org/www-community/attacks/Path_Traversal — "Validate the user's input by only accepting known good – do not sanitize the data" -->

Rclone's stderr is sanitised before inclusion in error messages: lines containing credential-related keywords are stripped, preventing `rclone.conf` secrets from reaching the frontend via error propagation.

### Guided setup wizard (S3-compatible and Google Drive)

Rather than requiring users to run `rclone config` manually, VoidGate provides a provider selection wizard that calls `rclone config create` via the sidecar. Initial provider support covers S3-compatible services (AWS S3, MinIO, Backblaze B2, Wasabi) and Google Drive (OAuth 2.0 browser flow).

Credentials are passed as arguments to `rclone config create` and not retained by VoidGate after the wizard completes. Rclone's `rclone.conf` is the authoritative credential store. The `CloudEndpoint` struct stored in `cloud-config.json` contains only the remote name, bucket, region, endpoint URL, and path prefix — no secrets.

### Upload order randomisation

Blob upload order is randomised using Fisher-Yates shuffle before each push. When a file produces multiple chunks, sequential upload order (chunk 0, 1, 2, …) allows a cloud provider to correlate blobs by temporal proximity — an observer who records upload timestamps can infer which blobs belong to the same file. Shuffling the full pending blob list across all staged files interleaves blobs from different files and destroys this correlation.
<!-- SOURCE: Randomize blob to pack file assignment — restic/restic PR #5295 — https://github.com/restic/restic/pull/5295 — "Restic currently sequentially assembles all fully processed chunks into a single pack file… To prevent an attacker [from guessing] which chunks of a file are in a given pack file, restic can instead assemble two pack files in parallel and randomly assign chunks to those pack files" -->
<!-- SOURCE: Chunking Attacks on File Backup Services using Content-Defined Chunking — Alexeev, Percival, Zhang (2025) — https://eprint.iacr.org/2025/532.pdf — abstract verified: adversary observing chunk sizes and upload order in encrypted backup storage can infer file identity; full text PDF returns 403 -->

The cost is O(n) on a `Vec<String>` of blob names — negligible relative to network I/O.

### Conflict detection: detect-and-block with manual resolution

A monotonic `snapshot_counter` (stored in `manifest_meta`) is compared before each push. If the cloud manifest backup reflects a higher counter than the local manifest, another device has pushed since the last sync. The push is aborted with an explicit message. No automatic merge is attempted.

Automatic manifest merge would require decrypting two SQLCipher databases, performing a three-way diff of the `nodes` and `chunks` tables, and resolving ambiguous cases. This is out of scope for the bachelor project. Detect-and-block is correct and avoids silent data loss.

### Manifest backup: full-buffer encryption

The SQLCipher export is loaded into a single buffer before encryption with `manifest_key` (XChaCha20-Poly1305, random nonce, no AAD). This is a deliberate exception to VoidGate's streaming rule. The manifest is expected to remain below 10 MiB throughout the project scope, making a single AEAD operation simpler and correct. No AAD is applied: the manifest backup is a singleton — there is no `file_id` or `chunk_index` context to bind.

## Alternatives considered

**System Rclone dependency**: requiring users to install Rclone separately avoids bundling concerns but introduces a setup step and version uncertainty. The sidecar approach eliminates this friction and pins the Rclone version to a known-good release.

**Shell-based Rclone invocation**: passing arguments via a shell string (e.g., `sh -c "rclone copyto ..."`) is simpler to construct but introduces shell injection risk. Direct `Command::args()` is marginally more code but is unconditionally safe.

**Automatic conflict resolution (last-writer-wins)**: would eliminate the manual pull step but risks silent data loss when two devices modify the same file concurrently. Detect-and-block is the correct baseline; merge logic is a future extension.

**Streaming manifest encryption**: applying the chunk pipeline to the manifest backup would be consistent with the rest of VoidGate's I/O model but adds unnecessary complexity for a file that remains small throughout this project's scope.

## Implications

The sidecar model couples VoidGate's release pipeline to Rclone releases: VoidGate must be rebuilt when a new Rclone version is required. The wizard's provider list is finite; adding new providers requires code changes (though the `CloudEndpoint` descriptor format is extensible).

Upload order randomisation partially mitigates temporal correlation but does not eliminate all access-pattern leakage. Blob count, upload timing windows, and vault header presence remain observable by the cloud provider. These are documented limitations in the threat model.

The manifest replay threat — a cloud provider serving an older manifest backup to force a stale vault state — is an inherent limitation of the BYOC (bring your own cloud) model. The provider is trusted for availability but not integrity; this is declared out of scope.
<!-- SOURCE: Tahoe – The Least-Authority Filesystem — Wilcox-O'Hearn & Warner, StorageSS '08 — https://eprint.iacr.org/2012/524 — "The only thing you ask of the servers is that they can (usually) provide the shares when you ask for them: you aren't relying upon them for confidentiality, integrity, or absolute availability." -->

## References

<!-- SOURCE: Path Traversal | OWASP Foundation — https://owasp.org/www-community/attacks/Path_Traversal — "Validate the user's input by only accepting known good – do not sanitize the data" -->
<!-- SOURCE: Randomize blob to pack file assignment — restic/restic PR #5295 — https://github.com/restic/restic/pull/5295 — "To prevent an attacker [from guessing] which chunks of a file are in a given pack file, restic can instead assemble two pack files in parallel and randomly assign chunks to those pack files" -->
<!-- SOURCE: Chunking Attacks on File Backup Services using Content-Defined Chunking — Alexeev, Percival, Zhang (2025) — https://eprint.iacr.org/2025/532.pdf — abstract verified: adversary observing upload order in encrypted backup storage can infer file identity; full text PDF returns 403 -->
<!-- SOURCE: Tahoe – The Least-Authority Filesystem — Wilcox-O'Hearn & Warner, StorageSS '08 — https://eprint.iacr.org/2012/524 — "The only thing you ask of the servers is that they can (usually) provide the shares when you ask for them: you aren't relying upon them for confidentiality, integrity, or absolute availability." -->
