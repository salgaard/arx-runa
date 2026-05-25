---
title: "Security Flow Review Plan"
created: "2026-05-17"
status: active
tags: [security, review, invariants]
---

# Security Flow Review Plan

Focused review sessions, one per flow. Each session loads only this section and its flow block, navigates via jcodemunch (no full-file reads except the file being edited), and writes findings to `.claude/reviews/review-<flow-id>-<YYYYMMDD>.md`.

## How to start a session

Tell Claude at session start:

> "Review **Flow X** from `.claude/plans/review-security-flows.md`. Write findings to `.claude/reviews/review-<flow-id>-YYYYMMDD.md`. Use jcodemunch for all navigation. Do not commit anything."

Claude should then:
1. `resolve_repo {"path": "."}` to get the repo identifier
2. `plan_turn` with the flow's query string (listed in each flow)
3. **Enumerate call sites before checking** — each session lists specific `find_references` / `search_text` / `search_symbols` calls to run first; build the complete set of relevant sites across the whole codebase before evaluating any of them. The starting symbol list is an entry point, not a scope boundary. A listed file is where you start navigation — not a declaration that only that file matters.
4. Navigate each enumerated site via `get_symbol_source` / `get_call_hierarchy`
5. Read only symbols that are suspicious or needed to confirm an invariant
6. Write findings as it goes — do not accumulate and dump at end

---

## Session A — Key Derivation & Session Memory Lifecycle

**Invariants**: 3 (HKDF constants), 16 (SQLCipher key handling), 17 (mlock + zeroize)

**plan_turn query**: `"HKDF key derivation, session key mlock zeroize, sqlcipher key handling from protected wrappers"`

**Starting symbols** (entry points only — enumeration widens scope beyond these):
- `with_key_encryption_key` — `src-tauri/src/auth/session/manager.rs`
- `SessionKeys` struct — `src-tauri/src/auth/session/keys.rs` (not manager.rs)
- SQLCipher open/create/rekey — `src-tauri/src/storage/sqlcipher.rs`
- `SecureBuffer` — `src-tauri/src/memory/secure_buffer.rs` (platform mlock/munlock implementation)
- `set_file_owner_only` — `src-tauri/src/platform/permissions.rs` (key file DACL/chmod)

**Enumerate first** (run these before checking any invariant):
- `find_references` on `SqlCipherMetadataStore::open` — build the full list of call sites across all files; every site must be checked for Invariant 16 regardless of which file it is in
- `search_text {"query": "with_exposed|expose()"}` across `src-tauri/src/` — any result that assigns to a local binding before an `.await` is a candidate violation of Invariant 16
- `search_text {"query": "expand_vault_key_into|expand_into_secret_box"}` — find every HKDF expansion call site; each must use the canonical salt and info constants
- `find_references` on `SecureBytes::new` — every construction site should be an mlocked allocation; any skipped site is an Invariant 17 candidate

**What to verify**:

| Check | Pass condition |
|---|---|
| HKDF salt is `b"arx-runa-v1"` at every call site (all sites from enumeration above) | Invariant 3 |
| HKDF info strings are `b"arx-runa-key-encryption"`, `b"arx-runa-sqlcipher"`, `b"arx-runa-manifest-backup"` — no others, no typos — at every call site | Invariant 3 |
| No new info strings share a value with existing ones | Invariant 3 |
| Every `SqlCipherMetadataStore::open` call site (from enumeration) passes `expose()` directly — no intermediate `let key_bytes`/`with_exposed(|b| *b)` binding anywhere in the codebase | Invariant 16 |
| `SessionKeys` has `mlock` / `VirtualLock` called immediately after derivation | Invariant 17 |
| `SessionKeys` implements `Drop` with `zeroize` | Invariant 17 |
| `master_key` is zeroized immediately after session keys are installed — not held in scope | Invariant 17 |
| `SecureBuffer` platform implementations (`memory/platform/unix.rs`, `memory/platform/windows.rs`) call `mlock`/`VirtualLock` and handle failure non-silently | Invariant 17 |
| Windows DACL `D:P(A;;FA;;;OW)(A;;FA;;;SY)(A;;FA;;;BA)` granting admin access to key file is documented as an accepted platform limitation in the threat model | Platform security |

**Finding severity guide**: HKDF constant mismatch = **critical**. Missing zeroize = **high**. Missing mlock = **medium** (OS-dependent, but still a gap). Raw sqlcipher_key copy = **high**. mlock failure silently ignored = **medium**. Admin DACL undocumented = **low**.

**Output file**: `.claude/reviews/review-flow-a-YYYYMMDD.md`

---

## Session B — AEAD Encrypt/Decrypt & Chunk Pipeline

**Invariants**: 1 (AAD = `file_id || chunk_index`), 2 (CSPRNG nonce), 4 (chunk_size validation)

**plan_turn query**: `"AEAD XChaCha20-Poly1305 encrypt decrypt AAD chunk_index nonce CSPRNG chunk_size validation"`

**Starting symbols** (entry points only — enumeration widens scope beyond these):
- Encrypt entrypoint — `encrypt_chunk` in `src-tauri/src/crypto/encrypt_chunk.rs`
- Decrypt entrypoint — `search_symbols` kind=function, pattern `decrypt_chunk` in `src-tauri/src/crypto/`
- Nonce generation — `generate_nonce` in `src-tauri/src/crypto/nonce.rs` (OsRng is abstracted; text search for `OsRng` will miss this)
- Epoch flush AEAD path — `flush_epoch_buffer` / `flush_one_blob` in `src-tauri/src/storage/vault_ops/epoch_flush.rs` (separate AEAD call site not reachable from encrypt_chunk alone)

**Enumerate first** (run these before checking any invariant):
- `find_references` on `encrypt_chunk` and the decrypt equivalent — every call site is an AAD and nonce check target; a site not reachable from the starting symbols is still in scope
- `search_text {"query": "encrypt_in_place|decrypt_in_place|XChaCha20Poly1305"}` across `src-tauri/src/` — find any raw AEAD usage that bypasses the `encrypt_chunk`/`decrypt_chunk` wrappers; each is a critical Invariant 1 and 2 candidate
- `find_references` on `generate_nonce` — confirm it is the single nonce generation path; any second nonce source is a critical Invariant 2 violation
- `search_text {"query": "chunk_size"}` across `src-tauri/src/` — find every site that reads or uses chunk size to confirm all go through the validated value from `manifest_meta`

**What to verify**:

| Check | Pass condition |
|---|---|
| AAD construction is exactly `file_id bytes \|\| chunk_index as u32 big-endian` at every AEAD call site (all sites from enumeration) | Invariant 1 |
| No AEAD call site omits AAD or passes empty AAD | Invariant 1 |
| No raw `encrypt_in_place`/`decrypt_in_place` call exists outside the canonical wrappers | Invariant 1 + 2 |
| Nonce is 24 bytes generated via CSPRNG (`OsRng` or equivalent) — no counter, no derived nonce | Invariant 2 |
| `generate_nonce` is the only nonce generation path (confirmed by `find_references`) | Invariant 2 |
| `chunk_size_bytes` is read from `manifest_meta` at vault open and validated against allowed range (128 KiB – 64 MiB) | Invariant 4 |
| No hardcoded chunk size that bypasses the stored value | Invariant 4 |
| BLAKE3 checksum is computed over ciphertext (not plaintext) — confirm ordering | Design sanity |
| In `flush_one_blob` (`epoch_flush.rs`): plaintext stays `Zeroizing`-wrapped through the encryption call — no `mem::take` or bare `Vec<u8>` assignment before `encrypt_chunk` | Storage rule: plaintext buffers must be `Zeroizing<Vec<u8>>` |
| Epoch buffer entries: `EpochBufferEntry.plaintext` is `Zeroizing<Vec<u8>>` so clones preserve zeroize-on-drop | Storage rule |
| Epoch blob AAD uses the epoch blob's own `FileId` (not individual file IDs) — confirm this is intentional and documented | Design sanity |

**Finding severity guide**: Wrong AAD = **critical** (breaks ciphertext binding). Sequential nonce = **critical**. Raw AEAD bypass outside wrappers = **critical**. Missing chunk_size validation = **high**. Plaintext escaping `Zeroizing` before encrypt = **high**.

**Output file**: `.claude/reviews/review-flow-b-YYYYMMDD.md`

---

## Session C — IPC Boundary & Zero-Trace

**Invariants**: 6 (Zeroizing conversion at IPC boundary), 7 (zero-trace persistence)

**plan_turn query**: `"IPC command handler sensitive string password Zeroizing zeroize logging error sanitisation"`

**Starting symbols** (entry points only — enumeration widens scope beyond these):
- `src-tauri/src/ui/auth_commands.rs` — `get_file_outline` first, then suspicious handlers
- `src-tauri/src/ui/commands_common.rs` — `sanitise_password` helper: this is the actual locus of the Zeroizing conversion; every handler delegates here rather than converting inline
- `src-tauri/src/ui/sync_commands.rs` — `get_file_outline`
- `src-tauri/src/ui/error.rs` — error mapping / sanitisation
- `src-tauri/src/lib.rs` — IPC command registration surface
- `src-tauri/src/ui/shell_commands.rs` — `reveal_in_explorer`, `open_url`, `compose_email_with_attachment` (path/URL injection surface; not password-bearing but exposes the OS shell)

**Enumerate first** (run these before checking any invariant):
- `search_text {"query": "#\\[tauri::command\\]"}` across `src-tauri/src/ui/` — build the **complete list** of IPC-exposed functions; cross-reference with the `invoke_handler` registration in `lib.rs` to confirm every registered command is in scope; any command not covered by the starting symbol files must be individually checked for Invariants 6 and 7
- `search_text {"query": "password|passphrase|master_key|sqlcipher_key|manifest_key"}` across `src-tauri/src/ui/` with context — any hit inside a `tracing::` macro or an error struct field is a candidate Invariant 7 violation
- `search_text {"query": "tracing::|log::"}` in `src-tauri/src/` — scan for log macros that include variable interpolation of session or key identifiers

**What to verify**:

| Check | Pass condition |
|---|---|
| Every IPC handler receiving a password `String` (from the complete enumerated list) immediately copies to `Zeroizing<Vec<u8>>` and scrubs the `String` backing bytes before calling deeper services | Invariant 6 |
| No IPC handler outside `auth_commands.rs` / `commands_common.rs` performs its own password handling — all route through `sanitise_password` | Invariant 6 |
| No password or key material appears in `tracing`/`log` macros at any level (all files, not just starting symbols) | Invariant 7 |
| Error responses from auth handlers do not distinguish wrong-password from wrong-key-file (`InvalidCredentials` is opaque) | Invariant 15 (cross-check) |
| No sensitive field is serialized into a Tauri event or IPC return value beyond what the UI strictly needs | Invariant 7 |
| Frontend-facing error strings contain no internal key identifiers or raw error messages that leak implementation detail | Invariant 7 |
| `src-tauri/src/ui/error.rs` strips sensitive context before returning to frontend | Invariant 7 |
| `reveal_in_explorer` validates that the supplied path is within the app's data/staging directory — not an arbitrary filesystem path | Path disclosure |
| `open_url` enforces an allowlist of permitted schemes (e.g. `https:` only) — `file:`, `javascript:`, and internal network URLs are rejected | SSRF / scheme abuse |
| `compose_email_with_attachment` on Linux: `package_path` and the constructed `mailto:` URL are passed as separate argument-list entries to `xdg-email` (not shell-interpolated) — confirm no injection via `recipient_email` embedded in the mailto string | Command injection |
| `reveal_in_explorer`, `open_url`, `compose_email_with_attachment` are listed in `tauri.conf.json` capabilities with appropriately scoped permissions — not unguarded | Attack surface |

**Finding severity guide**: Sensitive data in logs = **critical**. Missing Zeroizing at IPC boundary = **critical**. Oracular error response = **high**. Internal detail in error string = **medium**. Unvalidated path to OS shell = **high**. Unvalidated URL scheme = **medium**. Commands absent from capabilities = **high**.

**Output file**: `.claude/reviews/review-flow-c-YYYYMMDD.md`

---

## Session D — Cloud Sync & Rclone Subprocess

**Invariants**: 5 (vault path validation), 9 (Argon2 vault-header trust), 10 (pending_deletions durable retry)

**plan_turn query**: `"rclone subprocess cloud sync vault path validation Argon2 vault header trust pending_deletions"`

**Starting symbols** (entry points only — enumeration widens scope beyond these):
- `src-tauri/src/storage/cloud/rclone.rs` — rclone transport and invocation path (primary; `rclone_subprocess.rs` is the subprocess runner underneath it — check both)
- `src-tauri/src/storage/cloud/sync.rs` — sync orchestration
- `src-tauri/src/storage/cloud/wizard.rs` — config/credential handling
- `src-tauri/src/storage/vault_ops/prepare_vault_storage.rs` — vault storage prep
- `src-tauri/src/storage/cloud/destination_session.rs` — per-destination credential sessions; OAuth tokens and rclone config blobs are managed here
- `src-tauri/src/storage/vault_ops/delete_file.rs` — deletion entry point; the pending_deletions transaction order must be verified here, not just in sync.rs

**Enumerate first** (run these before checking any invariant):
- `find_references` on the rclone subprocess invocation function (the one that actually spawns rclone) — every caller is a command-injection check target; any call site that builds arguments via string formatting rather than a list is a critical finding
- `find_references` on the vault path validation function — every code path that accepts a user-supplied vault-relative path must flow through it; find callers that don't
- `search_text {"query": "pending_deletions"}` across `src-tauri/src/storage/` — enumerate every write site and confirm each follows the required transaction order
- `search_text {"query": "oauth|access_token|refresh_token|rclone.*password|config.*secret"}` across `src-tauri/src/` — any hit outside an encrypted store or a properly guarded temp file is an Invariant 7 candidate

**What to verify**:

| Check | Pass condition |
|---|---|
| Rclone command arguments are never constructed via string interpolation with user-supplied data — confirmed at every invocation site from enumeration, not just the primary one | No command injection |
| Cloud credentials / OAuth tokens are not logged or written to disk in plaintext | Invariant 7 |
| Vault-relative paths from user input pass centralized allowlist validation — `..`, absolute paths, control chars rejected — at every entry point from enumeration | Invariant 5 |
| On vault-header download for an existing device: Argon2 params are compared byte-for-byte against locally cached params before any derivation proceeds | Invariant 9 |
| On first-device bootstrap: Argon2 params below OWASP floors (`19456/2/1`) cause rejection, not silent acceptance | Invariant 9 |
| Every `pending_deletions` write site (from enumeration) follows the storage rule: read blob names → enqueue `pending_deletions` → delete node (CASCADE) → commit → delete local staging blobs — no reordering | Invariant 10 |
| Sync drains `pending_deletions` and removes rows only after confirmed cloud deletion | Invariant 10 |
| rclone stderr is sanitised before being surfaced in errors (credentials may appear in stderr) | Invariant 7 |
| Cloud credentials reach rclone via a temp file in a process-owned directory — not as command-line arguments or environment variables visible in `ps` output | Destination session storage security property |
| `destination_session.rs` does not serialize OAuth tokens or rclone config passwords into SQLite or any log sink in plaintext | Invariant 7 |

**Finding severity guide**: Command injection = **critical**. Credentials in logs = **critical**. Path traversal bypass = **critical**. Argon2 param not validated = **high**. pending_deletions not transactional = **high**. OAuth token written in plaintext = **critical**.

**Output file**: `.claude/reviews/review-flow-d-YYYYMMDD.md`

---

## Session E — Auth Ceremonies (Vault Create / Unlock / Recover)

**Invariants**: 13 (single vault_identity), 14 (recovery slot), 15 (non-oracular failure & tier input construction)

**plan_turn query**: `"vault create unlock recover auth ceremony vault_identity recovery slot mnemonic Argon2id tier input"`

**Starting symbols** (entry points only — enumeration widens scope beyond these):
- `auth::ceremonies::create_vault` — vault creation ceremony
- Unlock ceremony — search `search_symbols` kind=function, pattern `unlock_vault`
- `recover_with_phrase` or equivalent recovery entrypoint
- `vault_identity` insert/read — `find_references` on `vault_identity`
- `rotate_key_file` — `src-tauri/src/auth/ceremonies/rotate_key_file.rs` (full key-rotation ceremony; re-wraps all file keys and vault_identity under new master key — same invariants as create/unlock apply here)

**Enumerate first** (run these before checking any invariant):
- `search_text {"query": "INSERT.*vault_identity|vault_identity.*INSERT"}` across `src-tauri/src/` — any INSERT outside `create_vault` is a critical Invariant 13 violation; there must be exactly one
- `search_text {"query": "derive_master_key|kdf_tier|argon2"}` across `src-tauri/src/auth/` — find every KDF derivation call; each must use the same tier-construction function; any inline re-implementation is a critical Invariant 15 violation
- `find_references` on the tier KDF construction function — confirm all ceremonies (create, unlock, change-password, recover, rotate_key_file) use the same path; a ceremony that doesn't call it is an Invariant 15 candidate
- `find_references` on `wrap_master_key_for_recovery` — confirm only `create_vault` and `rotate_key_file` call it; any additional caller is an Invariant 14 candidate
- `search_text {"query": "recovery_phrase|mnemonic"}` across `src-tauri/src/` — any persistence or log hit is a critical Invariant 14 violation

**What to verify**:

| Check | Pass condition |
|---|---|
| `create_vault` inserts exactly one `vault_identity` row (`id = 1`); no path allows two rows — confirmed by `search_text` enumeration showing no other INSERT | Invariant 13 |
| Only `auth::ceremonies::create_vault` owns identity creation; sharing code only reads `vault_identity.public_key` | Invariant 13 |
| `rotate_key_file` re-wraps `vault_identity.wrapped_private_key` (not inserts/deletes the row) — `UPDATE vault_identity SET wrapped_private_key = ? WHERE id = 1` only | Invariant 13 |
| Recovery phrase is returned to UI exactly once and never written to any store or log (confirmed by `search_text` enumeration) | Invariant 14 |
| Recovery slot uses AAD `b"arx-runa recovery v1" \|\| vault_id_bytes` — verify constant and byte encoding | Invariant 14 |
| BIP-39 PBKDF2 derivation step is intentionally bypassed — space-joined mnemonic goes directly to Argon2id | Invariant 14 |
| Argon2id parameters for recovery slot match primary slot defaults | Invariant 14 |
| Tier 1 KDF input = password bytes only; Tier 2 KDF input = password bytes `\|\|` exactly 32 key-file bytes | Invariant 15 |
| All ceremonies that re-derive `master_key` (create, unlock, change-password, recover, **rotate_key_file**) use the same tier-construction function — confirmed by `find_references` enumeration showing no inline re-implementations | Invariant 15 |
| Auth failure responses do not distinguish wrong password from wrong key file | Invariant 15 |
| `recover_with_phrase` is a single atomic ceremony — no intermediate authenticated session is established | Invariant 14 |
| In `rotate_key_file`: `key_encryption_key_from_array(current_session_keys.key_encryption_key.expose())` — confirm return type is `Zeroizing`-wrapped or otherwise does not leave a bare copy of the KEK on the stack beyond the transaction closure | Invariant 17 |
| In `rotate_key_file`: explicit `drop()` calls come after `swap_active_session` and `upload_vault_header` — confirm no `master_key` copy is held in a moved binding or closure across the network call | Invariant 17 |
| `rotate_key_file` re-wraps the recovery slot using `wrap_master_key_for_recovery` with the same AAD — not `wrap_file_key` | Invariant 14 |

**Finding severity guide**: Recovery phrase persisted = **critical**. Wrong AAD on recovery slot = **critical**. PBKDF2 not bypassed = **critical** (changes KDF output). Oracular error = **high**. Tier construction inconsistency = **high**. Bare KEK copy escaping mlocked memory in rotate = **high**.

**Output file**: `.claude/reviews/review-flow-e-YYYYMMDD.md`

---

## Finding format (all sessions)

Each finding in the output file:

```
### [FLOW-X-NNN] Short title
**Severity**: critical / high / medium / low
**Invariant**: N (or "design sanity")
**Location**: `path/to/file.rs:line`
**Observation**: What the code does.
**Violation**: How it violates the invariant or contract.
**Recommendation**: Concrete fix.
```

End each session file with a `## Summary` section: counts by severity, any invariants fully confirmed (no findings), and whether a follow-up fix session is recommended.

---

---

## Session F — Zero-Knowledge Boundary (What the Cloud Actually Sees)

**Core question**: Does the cloud ever receive anything other than opaque encrypted blobs? This is the product's primary promise and is not fully covered by other flows.

**plan_turn query**: `"cloud upload blob naming manifest encryption vault header content metadata plaintext boundary rclone transfer"`

**Starting symbols** (entry points only — enumeration widens scope beyond these):
- Sync/upload entrypoint — `search_symbols` kind=function, pattern `upload` or `sync_push` in `src-tauri/src/storage/cloud/`
- Chunk blob naming — trace how chunk filenames/keys are constructed before being passed to rclone
- Manifest upload — `search_symbols` pattern `manifest_backup` or `upload_manifest`
- Vault header upload — `upload_vault_header` in `src-tauri/src/storage/cloud/vault_header_io.rs` (note: function comment says "plaintext vault header JSON" — this is correct by design; the vault header is public, containing only Argon2 params and wrapped key slots; the check is about its *contents*, not that it's encrypted)
- `prepare_vault_storage` — `src-tauri/src/storage/vault_ops/prepare_vault_storage.rs`
- EXIF stripping integration — `strip_exif` call site in `src-tauri/src/storage/pipeline/encrypt_file.rs` (confirm stripping is unconditionally invoked for JPEG/PNG before AEAD; check whether it can be skipped via any flag or code path)

**Enumerate first** (run these before checking any invariant):
- `find_references` on all rclone copy/upload primitive functions — every call site is a potential plaintext boundary crossing; any site not traced through an encryption step first is a critical ZK violation
- `search_text {"query": "staging|\.tmp|tempfile|NamedTempFile"}` across `src-tauri/src/storage/` — find every staging-area write; each must write only ciphertext; a write of plaintext bytes before AEAD is a critical violation
- `find_references` on `strip_exif` — must be called at every encrypt-file entry point; any path through encryption that doesn't call it for supported formats is a ZK metadata leak
- `search_text {"query": "blob_name|chunk_name|object_key|remote_path"}` — find all sites that construct the cloud object name; each must produce an opaque identifier, not a derivative of the original filename or plaintext content

**What to verify**:

| Check | Pass condition |
|---|---|
| Every byte written to the cloud via rclone is ciphertext — confirmed at all rclone upload call sites from enumeration, not just the primary entry point | ZK core |
| Chunk blob names are opaque (random UUIDs or hashes of ciphertext) — not derived from original filenames or plaintext content — confirmed at all blob-naming sites from enumeration | ZK metadata |
| Blob names are not derived from plaintext content hash (would enable convergent encryption / cloud deduplication attacks revealing file identity) | ZK metadata |
| The manifest (file tree, names, sizes, timestamps) is encrypted before upload — cloud sees only ciphertext blob | ZK metadata |
| Vault header uploaded to cloud contains only: Argon2 params + wrapped `master_key` slot(s) — no filenames, directory structure, or user identity | Invariant 9 cross-check |
| `pending_deletions` blob names are the opaque chunk identifiers, not original filenames | ZK metadata |
| Staging directory (if used) contains only ciphertext — confirmed at all staging write sites from enumeration | Invariant 7 + ZK |
| File sizes are not leaked through blob sizes (padded chunks) — or documented as an accepted limitation | ZK / threat model |
| Modification timestamps of uploaded blobs do not reveal access patterns beyond what the threat model accepts | ZK / threat model |
| No sync-state file written to cloud reveals internal manifest structure in plaintext | ZK metadata |
| `strip_exif` is called unconditionally in `encrypt_file` for JPEG/PNG (detected by magic bytes) before the AEAD stage — confirmed at all `find_references` call sites; no flag or caller can bypass it for supported formats | ZK metadata |
| TIFF, HEIC, MP4/QuickTime exclusion from EXIF stripping is documented as an accepted limitation in the threat model | ZK / threat model |

**Accepted limitations to confirm (not findings)**:
- Blob count on cloud reveals approximate file count (accepted, documented in threat model)
- Blob sizes leak chunk size if not padded (check whether this is accepted or mitigated)
- Cloud provider sees upload/download timestamps (accepted)

**Finding severity guide**: Plaintext file reaching rclone = **critical**. Original filename in blob name = **critical**. Unencrypted manifest uploaded = **critical**. Plaintext staging temp file = **high**. Vault header leaking filenames = **high**. Content-derived blob names = **high** (convergent encryption).

**Output file**: `.claude/reviews/review-flow-f-YYYYMMDD.md`

---

---

## Session G — UI Zero-Trace & Frontend Security

**Context**: `src-tauri/src/ui/security_audit.rs` already contains 15 static-analysis tests covering localStorage, sessionStorage, IndexedDB, service workers, clipboard plugin, tempfiles, key material in logs, CSP presence, and state-clearing router wiring. This session does **not** re-verify what those tests already enforce. Instead it checks: (1) whether the tests themselves are correctly scoped for current code, and (2) the gaps those tests do not cover.

**plan_turn query**: `"frontend UI Leptos state signal clear vault lock password input IPC response data minimisation VaultActions clear modified transfer settings destinations"`

**Starting symbols** (entry points only — enumeration widens scope beyond these):
- `src-tauri/src/ui/security_audit.rs` — `get_file_outline` to confirm test scope
- `src/state/vault_context.rs` — what `VaultActions::clear()` actually clears
- `src/state/session_context.rs` — session state cleared on lock event
- `src/auth.rs`, `src/transfer.rs`, `src/settings.rs`, `src/destinations.rs` — modified frontend files
- `tauri.conf.json` — CSP directive values (not just presence)
- `src-tauri/src/ui/video_stream.rs` — `arxvault://` custom URI scheme handler; decrypts and streams plaintext video content to the WebView (not covered by security_audit.rs static tests)

**Enumerate first** (run these before checking any invariant):
- `search_text {"query": "invoke("}` across frontend `src/` — build the complete list of IPC calls the frontend makes; cross-reference each with the backend command registration in `lib.rs`; any call to a command not in the starting file list must be checked for data-minimisation and error-sanitisation
- `find_references` on `VaultActions` — enumerate every component that holds reactive state; confirm each component either calls `VaultActions::clear()` on lock or explicitly documents why it is not needed; a component that skips it is an Invariant 7 candidate
- `search_text {"query": "use_context.*Vault|provide_context.*Vault|create_rw_signal|create_signal"}` across `src/` — find all reactive signals; any signal holding sensitive data (passwords, keys, file content) that is not explicitly cleared on lock is a candidate violation

**What to verify**:

| Check | Pass condition |
|---|---|
| `security_audit.rs` static scans cover the correct source directories — confirm they include `src/transfer.rs`, `src/settings.rs`, `src/destinations.rs` (all modified in current branch) | Audit test completeness |
| CSP directives in `tauri.conf.json` match design spec exactly: `script-src` uses `'wasm-unsafe-eval'` not `'unsafe-eval'`; no `'unsafe-inline'` in `script-src`; `connect-src` limited to `ipc:` and `http://ipc.localhost` | Design spec (`6.4` / CSP section) |
| Password Leptos signal in `src/auth.rs` is explicitly overwritten/cleared after IPC dispatch — not just the Rust `String` on the backend side | Invariant 7 frontend |
| `VaultActions::clear()` covers ALL reactive signals across ALL components (confirmed by `find_references` enumeration) — no signal holding sensitive data escapes the clear on lock | Invariant 7 |
| Every IPC call from frontend `invoke()` enumeration maps to a registered backend command; no undeclared or debug-only commands reachable | Attack surface |
| IPC responses sent to the frontend carry minimum necessary data — file listings do not include internal chunk IDs, blob names, or SQLite row IDs | Data minimisation |
| Progress event payloads carry only percentages and byte counts — no file content, no internal paths | Design security analysis |
| `withGlobalTauri: true` is present — confirm only expected IPC commands are registered in `src-tauri/src/lib.rs`; no internal/debug commands accidentally exposed | Attack surface |
| Error variants returned via `IpcError` to the frontend do not carry file system paths, internal key identifiers, or raw error chain text | Invariant 7 + design security analysis |
| In-app file viewer (if wired): plaintext content signal is cleared when the user navigates away and on vault lock, not just on explicit close | Invariant 7 |
| `video_stream.rs`: decrypted `bytes` returned from `download_file_range_to_memory` are placed in an HTTP response body as plain `Vec<u8>` — confirm the buffer is zeroized after `responder.respond()` or that the Tauri runtime owns and drops it promptly; assess whether `Zeroizing` is practical here | Invariant 7 |
| `video_stream.rs`: `node_uuid` lookup (`db.get_node`) uses the active vault's DB — confirm a valid UUID from a different vault cannot be served if two vaults exist on the same device | Access control |
| `video_stream.rs` on Windows: `http://arxvault.localhost` origin — confirm the Tauri WebView isolation prevents other localhost origins (e.g. a browser tab) from issuing range requests to this scheme | ZK / attack surface |
| `video_stream.rs`: `mime_from_name` derives MIME type from the stored filename — confirm the filename is not included in response headers or logs in a way that leaks it to the network layer | Invariant 7 |

**Accepted as already tested — verify tests are correctly scoped, not re-check the property**:
- No `localStorage` / `sessionStorage` / `IndexedDB` calls in frontend source ✅
- No service worker registration ✅
- No clipboard plugin in `Cargo.toml` ✅
- No `tempfile` crate in decrypt pipeline or file commands ✅
- No key material in `storage` module logs ✅
- State-clearing wired on lock transition in router ✅
- CSP field is populated in `tauri.conf.json` ✅ (but check *content* here)

**Finding severity guide**: `'unsafe-eval'` in CSP = **critical**. Password signal not cleared = **high**. New state context not in `VaultActions::clear()` = **high**. IPC response carrying blob names/chunk IDs = **medium**. Audit test scanning wrong directory = **high** (silent regression risk). Video plaintext not zeroized = **medium**. Cross-origin access to `arxvault://` scheme = **high**.

**Output file**: `.claude/reviews/review-flow-g-YYYYMMDD.md`

---

---

## Session H — File Sharing HPKE & Key Isolation

**Invariants**: 11 (share package key-handling contract), 12 (share revocation semantics)

**Additional spec**: REQ-CRYPTO-016 (CTX-ChaCha20-Poly1305 with BLAKE3 CMT-4 commitment), sub-phase 5.2 security review

**plan_turn query**: `"HPKE share package file_key wrap zeroize CTX ChaCha20 BLAKE3 commitment revocation vault key isolation sharing packages"`

**Starting symbols** (entry points only — enumeration widens scope beyond these):
- `src-tauri/src/sharing/packages.rs` — `create_share_package` and `import_share_package` (seal/open entry points)
- HPKE seal/open — `seal` in `src-tauri/src/sharing/hpke.rs` (use `get_symbol_source`; text search for `hpke`/`HPKE` returns no results due to indexer behaviour — use `search_symbols` instead)
- CTX-ChaCha20-Poly1305 — `src-tauri/src/sharing/ctx_aead.rs` (use `get_file_outline`; same caveat: text search for `arx-runa-ctx`/`CTX` misses this file)
- `file_key` wrapping on import — `find_references` on `file_key_wrapped` in `src-tauri/src/sharing/`
- `src-tauri/src/sharing/revocation.rs` — default and strong revocation implementations (30 symbols; strong revocation rotates `file_key` and re-encrypts — must be audited independently, not inferred from `packages.rs`)

**Enumerate first** (run these before checking any invariant):
- `find_references` on the HPKE `seal` function — confirm it is the single HPKE encryption path; any second seal call site means the ciphersuite and `info`/`aad` checks must also apply there
- `find_references` on `file_key` type across `src-tauri/src/sharing/` — every site that holds a `file_key` value must either keep it in a `Zeroizing`-wrapped binding or pass it directly into an encryption call; any intermediate bare copy is an Invariant 11 violation
- `search_symbols {"kind": "function", "pattern": "wrap_file_key|unwrap_file_key"}` — find all call sites; confirm none are used where `wrap_master_key_for_recovery` is required (recovery slot context), and vice versa
- `find_references` on `replace_file_key_and_chunks` (strong revocation) — must be the single re-encryption path; confirm the old `file_key` zeroize call follows immediately after it returns at every call site

**What to verify**:

| Check | Pass condition |
|---|---|
| HPKE ciphersuite is `DHKEM(X25519, HKDF-SHA256) + HKDF-SHA256 + CTX-ChaCha20-Poly1305` — not the standard Poly1305 variant — confirmed at all seal call sites from enumeration | REQ-CRYPTO-016 |
| CTX BLAKE3 domain string is exactly `b"arx-runa-ctx-v1"` | REQ-CRYPTO-016 |
| BLAKE3 commitment covers key + nonce + full ciphertext body — not a partial commit | REQ-CRYPTO-016 (CMT-4) |
| HPKE `info = b"arx-runa-share"`, `aad = b""` — no other values, no deviations — confirmed at all seal call sites from enumeration | Invariant 11 |
| On recipient import: raw `file_key` bytes are wrapped into `file_key_wrapped` immediately; raw bytes zeroized after wrapping — confirmed at all `file_key` hold sites from enumeration | Invariant 11 |
| `master_key`, `sqlcipher_key`, and `manifest_key` are absent from the HPKE plaintext payload and from any share package serialisation path | Invariant 11 |
| `file_key` does not appear in `tracing`/`log` output, error messages, or the exported `.vgshare` file outside the HPKE-encrypted envelope | Invariant 11 + Invariant 7 |
| All three HPKE failure paths (wrong `enc`, corrupted ciphertext, wrong CTX tag) surface as identical `AuthenticationFailed` — no oracle distinguishing them | Sub-phase 5.2 security review |
| Default revocation in `revocation.rs`: marks `revoked_at` in `shares` table and removes cloud blobs atomically — blob removal failure does not leave `revoked_at` unset | Invariant 12 |
| Strong revocation in `revocation.rs`: generates a fresh `file_key`, calls `replace_file_key_and_chunks` (single transaction enqueuing old blob names into `pending_deletions`), re-publishes under new `file_share_id`, retires old shared path | Invariant 12 |
| Strong revocation: old `file_key` is zeroized immediately after `replace_file_key_and_chunks` completes — confirmed at all `find_references` call sites | Invariant 12 + Invariant 17 |
| Strong revocation: new share packages are only issued after the re-encryption transaction commits — no window where the old path is retired but the new one is not yet live | Invariant 12 |
| Only the selected file's `file_key` context is included in the share package; no path exists to include keys for other files | Invariant 11 |

**Finding severity guide**: Wrong HPKE ciphersuite variant (standard vs CTX) = **critical** (breaks CMT-4 binding). Wrong CTX domain string = **critical**. Partial BLAKE3 commitment = **critical**. `master_key`/`sqlcipher_key` in share payload = **critical**. `file_key` not zeroized after wrapping = **high**. Oracular HPKE error = **high**. `file_key` in logs = **critical**. Strong revocation race window = **high**. Default revocation non-atomic = **high**.

**Output file**: `.claude/reviews/review-flow-h-YYYYMMDD.md`

---

## On tests: should review flows also hunt for missing tests?

**Recommendation: no — keep flows focused on invariant violations, not test coverage.**

Reasons:
- The codebase already has `security_audit.rs` as a dedicated static-analysis test layer. Mixing test-hunting into security review sessions dilutes both.
- A review session finding a violation should note whether a test exists for it (one line), but actively searching for coverage gaps is a separate scope.
- Sessions that try to do both end up doing neither thoroughly within context limits.

**What each flow *should* do**: when a finding is raised, append a one-line note — `Test coverage: none / partial / covered by <test name>` — to the finding block. This gives you a prioritised test-gap list as a by-product without blowing up session scope.

**If you want a dedicated test-coverage pass** after the security review: that is a separate plan, scoped to "for each confirmed finding with no test coverage, propose a test". Run it after all seven sessions are complete and findings are consolidated.

---

## Sequencing recommendation

Run in this order — later flows depend on earlier ones being clean:

1. **A** (key derivation) — foundation for everything else
2. **C** (IPC boundary) — fast, high signal-to-noise, catches log leaks early
3. **G** (UI zero-trace) — fast, mostly confirms existing tests are correctly scoped
4. **F** (zero-knowledge boundary) — the product's core promise; run before D since D assumes ZK holds
5. **D** (cloud/rclone) — transport security, command injection, Argon2 header trust
6. **B** (AEAD pipeline) — deepest crypto, most likely to need time
7. **E** (ceremonies) — requires understanding from A and B to evaluate correctly
8. **H** (file sharing HPKE) — isolated subsystem; run after B since it shares AEAD/CTX concepts
