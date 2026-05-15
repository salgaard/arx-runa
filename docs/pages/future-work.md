# Future Work

Features and capabilities that are designed, documented, or partially implemented but intentionally deferred beyond Phase 6. These are not omissions — they are deliberate scope boundaries with known design paths.

---

## Directory Deletion

**Current state**: file deletion only. `delete_file` removes a single file, queues its blobs in `pending_deletions`, and cascades chunk rows via SQL `CASCADE`.

**Deferred**: `delete_directory` requires recursive enumeration of all children and their associated blobs, then an atomic transaction covering the entire subtree. MVP focuses on per-file operations.

**Design path**: a dedicated `delete_directory` IPC command + `MetadataStore` extension; the recursive enumeration and blob-queue logic would mirror the existing per-file deletion flow.

---

## Multi-Vault Support

**Current state**: one vault per Arx Runa instance. `AppState` holds a single `SharedSession` and a single SQLCipher database handle.

**Deferred**: multi-vault support requires per-vault session coordination, a vault-switcher UI, and an `AppState` refactor to hold a keyed map of sessions.

**Design path**: Phase 7+ architectural extension. The session and auth ceremonies are already vault-scoped by design — the extension is primarily in `AppState` and the frontend routing layer.

---

## In-App File Viewer

**Current state**: `get_file_content` command is implemented with a 50 MiB cap. It decrypts and returns file content as base64-encoded bytes to the frontend.

**Deferred**: in-app rendering (text editor, image preview, PDF viewer) is a frontend concern not tackled in Phase 6.

**Design path**: a viewer component consumes the `get_file_content` response and renders it based on MIME type, inferred from the file extension in the manifest. The backend command surface requires no changes.

---

## Video Metadata Stripping (MP4/QuickTime)

**Current state**: EXIF stripping runs on JPEG, PNG, and TIFF. MP4/QuickTime are passed through unmodified.

**Deferred**: the `moov` atom containing GPS coordinates and all file-level metadata is placed at the end of the file in typical device recordings (`[ftyp][mdat][moov]`). A single-pass streaming read cannot reach `moov` without reading the entire file, breaking the streaming invariant.

**Design path**: a pre-processing step that reads the full file into a temp buffer, strips `moov` metadata (e.g., via `ffmpeg` sidecar or a pure-Rust MP4 parser), then passes the cleaned buffer into the encrypt pipeline.

---

## Upload Order Randomisation

**Current state**: blobs are uploaded in chunk order, which leaks temporal correlation — an observer sees which blobs belong to the same file by their upload timestamps.

**Deferred**: a Fisher-Yates shuffle of the upload queue before upload would eliminate this signal. Not implemented in Phase 4.

**Design path**: the upload queue in the push flow is assembled as a `Vec<(local_path, remote_path)>` before any Rclone calls. Shuffling this vec before iteration is a one-line change.

---

## Epoch Buffer for Small Files

**Current state**: `epoch_buffer_enabled` is a vault-creation option that enables hybrid routing (small files staged and packed; large files uploaded immediately). The flag exists in the manifest schema and vault creation flow.

**Deferred**: the epoch buffer packing and upload logic is not fully implemented. The flag is accepted and stored but packing behaviour is not active.

**Design path**: small files (`size_bytes < chunk_size_bytes`) are accumulated in a staging epoch directory. When the epoch is flushed (on sync trigger or size threshold), files are packed into shared chunks and uploaded together.

---

## Argon2id Parameter Upgrade Policy

**Current state**: Argon2id parameters (`m=65536`, `t=3`, `p=4`) are stored in the vault header and locked at vault creation. Future parameter increases require a re-derivation ceremony.

**Deferred**: no UX or automatic upgrade path exists for existing vaults that wish to adopt stronger parameters.

**Design path**: a credential-rotation ceremony (similar to password change) that accepts current credentials, re-derives under new parameters, re-wraps all file keys, and updates the vault header. Requires user awareness of the re-derivation time cost.

---

## SLIP-39 and Trusted-Contact Recovery

**Current state**: recovery uses a single BIP-39 24-word mnemonic stored as one recovery slot in the vault header.

**Deferred**: SLIP-39 (Shamir's Secret Sharing over a structured mnemonic) and trusted-contact recovery (encrypting the master key to a contact's X25519 public key) were considered but deferred in favour of the simpler BIP-39 single-slot approach.

**Design path**: the `recovery_slots` array in the vault header is extensible. New slot types add a `method` discriminant (`"bip39"`, `"slip39"`, `"trusted_contact"`). The recovery ceremony selects the appropriate derivation path per slot.

---

## Share Receipts and Expiration (Full Implementation)

**Current state**: download receipts and share expiration are designed and the database schema includes `expires_at` and `revoked_at` fields. Receipt blob writing and expiration enforcement are partially implemented.

**Deferred**: the full receipt polling loop (periodic scan of `shared/<file_share_id>/receipts/` on sync) and automatic expiration cleanup are not wired into the sync cycle.

**Design path**: the sync push flow adds a receipt-poll step for each active outgoing share. Expired shares trigger blob deletion of `shared/<file_share_id>/` and `revoked_at` update.
