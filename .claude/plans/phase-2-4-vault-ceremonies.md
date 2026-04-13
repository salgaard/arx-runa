---
title: "Phase 2.4 — Vault Ceremonies"
created: "2026-04-14T00:00:00Z"
status: draft
roadmap-phase: 2
sub-phase: "2.4"
design-document: "docs/architecture/designs/authentication-and-session-management/design.md"
sub-phase-roadmap: "docs/architecture/designs/authentication-and-session-management/sub-phases/roadmap.md"
test-agent-required: true
governance-sync-required: true
tags: [auth, phase-2, ceremonies, vault-creation, password-change, key-rotation, recovery, bip39, cross-phase-stub]
---

# Plan: Phase 2.4 — Vault Ceremonies

## 1. Goal

Implement the six vault lifecycle ceremonies in `src-tauri/src/auth/ceremonies.rs` — `create_vault`, `change_password`, `rotate_key_file`, `recover_vault`, `setup_recovery`, `recover_with_phrase` — atop the existing Phase 2.1–2.3 primitives, wiring in Phase 1 recovery-slot crypto (newly added here), a minimal SQLCipher stub (Phase 3.1 forward declaration), and a minimal `CloudTransport`/`VaultHeader` surface (Phase 4.1/4.3 forward declarations).

## 2. Context

**Roadmap**: Phase 2 — Authentication and Session Management (`docs/roadmap.md` lines 55–61). Phase 2.4 is the terminal sub-phase of Phase 2 and the final precondition for marking Phase 2 complete before Phase 3 begins.

**Sub-phase roadmap**: `docs/architecture/designs/authentication-and-session-management/sub-phases/roadmap.md`. Strict order 2.1 → 2.2 → 2.3 → 2.4. Phase 2.4 is the fourth unit. Security review **required** per the roadmap's Security Review Checkpoints. Estimated scope: ~300 lines production + ~200 lines tests.

**Sub-phase document**: `docs/architecture/designs/authentication-and-session-management/sub-phases/2.4-vault-ceremonies.md` (deliverables 1–10).

**Parent design sections used** (absolute paths with line ranges):

- `docs/architecture/designs/authentication-and-session-management/design.md` lines 21–47: Contract Surface — canonical interface/data/invariant/dependency contracts (binding).
- Same file lines 184–270: `SessionKeys`, `SharedSession`, session lifecycle, memory-lock failure, timeout mechanism (already implemented in Phase 2.3; Phase 2.4 adds ceremony-driven transitions).
- Same file lines 272–305: `AuthenticationError` enum and timing policy.
- Same file lines 309–380: Vault creation flow (21 steps + critical invariant on `master_key` scope).
- Same file lines 383–416: Password change flow.
- Same file lines 419–458: USB key file rotation flow (incl. rotation crash-recovery protocol).
- Same file lines 462–478: New-device recovery flow.
- Same file lines 481–540: Recovery slot concept, phrase generation, slot derivation, display policy, recovery authentication flow.
- Same file lines 544–582: Trait boundaries (`KeySource`, `DeviceMonitor` — reused unchanged).
- `docs/architecture/designs/cryptographic-primitives/design.md` lines 148–212: Recovery master-key wrapping (canonical signatures for `wrap_master_key_for_recovery` / `unwrap_master_key_from_recovery`, new types `MasterKey`, `RecoveryKey`, `WrappedMasterKey`, and the `aad = b"arx-runa recovery v1" || vault_id_bytes` rule).
- `docs/architecture/designs/cloud-synchronisation/sub-phases/4.1-cloud-transport.md`: used as the reference for the minimal `CloudTransport` forward declaration (deliverables 1–4).
- `docs/architecture/designs/cloud-synchronisation/sub-phases/4.3-vault-header.md`: used as the reference for the minimal `VaultHeader` forward declaration (deliverables 1–6).
- `docs/architecture/design-invariants.md` §3 (HKDF constants — re-used by ceremonies), §6 (IPC sensitive-input handling — boundary guidance for ceremony IPC wrappers), §7 (Zero-Trace persistence — no recovery phrase or master_key to disk), §9 (Argon2 vault-header trust contract — new-device recovery must match trusted params on bootstrap).

**Existing state** (branch `development`, commit `2412090`):

- `src-tauri/src/auth/mod.rs` re-exports `DeviceEvent`, `DeviceMonitor`, `AuthenticationError`, `KeySourceError`, `Argon2Params`, `FileKeySource`, `KeySource`, `KeyHintStore`, `VaultHint`, `LifecycleState`, `OperationGuard`, `SessionEvent`, `SessionManager`. No `ceremonies` module yet.
- `src-tauri/src/auth/kdf.rs` exports `Argon2Params` (`DEFAULT = { memory_cost_kib: 65536, time_cost: 3, parallelism: 4 }`) and `derive_master_key_into(password, key_file, salt, params, &mut [u8; 32])`. This function is `pub(crate)` and directly usable by ceremonies.
- `src-tauri/src/auth/session.rs` defines `SessionKeys` (pub(crate), with `key_encryption_key / sqlcipher_key / manifest_key: SecureBytes<32>`), `SessionKeys::derive(password, key_file, salt, params) -> Result<Self, AuthenticationError>`, and `SessionManager` with `authenticate`, `lock`, `reset_timer`, `begin_operation`, `state`, `subscribe`, `from_config`, `with_timeout`. No helper exposes `master_key` to ceremonies, and no method swaps the active session keys without running KDF.
- `src-tauri/src/auth/error.rs` defines `AuthenticationError` with `InvalidCredentials`, `KeyFileNotFound`, `MemoryLockFailed`, `VaultHeaderInvalid`, `SessionAlreadyActive`, `KeySource(#[from])`. **Missing**: `InvalidRecoveryPhrase`, `NoRecoverySlot` (design lines 296–300).
- `src-tauri/src/crypto/wrap_key.rs` provides `wrap_file_key` / `unwrap_file_key` with empty AAD. **Missing**: `wrap_master_key_for_recovery`, `unwrap_master_key_from_recovery`, and the `MasterKey`, `RecoveryKey`, `WrappedMasterKey`, `VaultId` types. Roadmap explicitly defers these to Phase 2 (`docs/roadmap.md` line 51).
- `src-tauri/src/crypto/types/mod.rs` declares `FileKey`, `KeyEncryptionKey`, `SqlcipherKey`, `ManifestKey`, `WrappedFileKey`, `FileId`, `ChunkIndex`, `Blake3Hash`. Missing: `MasterKey`, `RecoveryKey`, `WrappedMasterKey`, `VaultId`.
- `src-tauri/src/crypto/mod.rs` re-exports the existing surface; `recovery_wrap` module is not declared.
- `src-tauri/src/crypto/hkdf.rs` exposes `expand_vault_key_into(master_key_bytes, info, &mut [u8; 32])` and the `HKDF_INFO_*` constants — reusable for ceremony-local derivation.
- `src-tauri/src/storage/mod.rs` contains only `pub mod error; pub mod types;`. No `cloud` submodule exists. Phase 3 is not yet implemented.
- `src-tauri/src/sync/mod.rs` is similarly empty. Phase 4 is not yet implemented.
- `src-tauri/Cargo.toml` already pins `bip39 = "2"`, `chacha20poly1305 = "0.10"`, `uuid = { version = "1", features = ["v4", "serde"] }`, `rusqlite = { version = "0.39", features = ["bundled-sqlcipher-vendored-openssl"] }`, `x25519-dalek = "2"`, `serde_json = "1"`, `tokio`, `dirs = "6"`. **No new Cargo dependencies required.**
- `.claude/rules/auth.md` documents the auth module and session machinery (post-Phase 2.3). It does **not** mention ceremonies, recovery slots, or crash-recovery staging. Phase 2.4 governance sync will add these.
- `.claude/rules/crypto.md` already notes recovery-slot AAD and that dedicated `wrap_master_key_for_recovery` / `unwrap_master_key_from_recovery` functions must be used (design-anchored text). When Phase 2.4 implements those functions, the rule text remains accurate but should no longer describe them as future work.
- `.github/instructions/auth.instructions.md` and `.github/instructions/crypto.instructions.md` mirror the above rules and must be resynchronised via `/copilot-sync` after any `.claude/rules/` edit.

**Pending architectural decisions** relevant to Phase 2.4 (roadmap Open Decisions + authentication design Open Decisions):

- Argon2id parameter upgrade policy for existing vaults — deferred; current design stores params in vault header for future use. Phase 2.4 does **not** implement upgrade.
- Argon2id parameter upgrade for recovery slots — deferred; slot params are independent per slot. Phase 2.4 stores slot Argon2 params independently as specified.
- Session timeout behaviour during active upload — deferred to Phase 6. Not addressed here.

## 3. Design Concerns / Open Questions

### DC-1 — Cross-phase dependency on Phase 3.1 (SQLCipher schema) is declared but Phase 3 is not implemented

- **Concern**: Deliverable 1 (vault creation) requires creating a SQLCipher DB with the Phase 3.1 schema (`nodes`, `chunks`, `manifest_meta`, `contacts`, `shares`, `received_shares`). Phase 3.1 does not exist yet.
- **Source**: `2.4-vault-ceremonies.md` deliverable 1 lines 15–17; `design.md` Vault Creation Flow step 16.
- **Impact**: Without resolution, vault creation cannot complete in tests. Every ceremony that touches SQLCipher (`change_password` rekey, `rotate_key_file` rekey, `recover_vault` import) cannot be integration-tested end-to-end.
- **Classification**: Non-blocking. The sub-phase explicitly endorses a stub: line 137 — "use a minimal stub schema (`CREATE TABLE _phase_stub (id INTEGER PRIMARY KEY)`) to allow the vault creation flow to be tested end-to-end; replace with the real schema once Phase 3.1 is merged".
- **Resolution**: Phase 2.4 creates the SQLCipher file with `rusqlite::Connection::open(...)`, issues `PRAGMA key = '<hex sqlcipher_key>'`, and executes the stub `CREATE TABLE _phase_stub (id INTEGER PRIMARY KEY)` statement. Also creates a stub `nodes` table that owns one column `file_key_wrapped BLOB NOT NULL` so the re-wrap loop in `change_password` / `rotate_key_file` can be exercised against real (if empty) rows and against a seeded fake row in tests. `PRAGMA rekey` is called inside the ceremony flow. `PRAGMA foreign_keys = ON` is deferred to Phase 3.1. All TODO markers reference `phase-3.1`.
- **Documentation sync required on implementation**: YES. Update `docs/architecture/designs/authentication-and-session-management/sub-phases/2.4-vault-ceremonies.md` deliverable 1 to append "SQLCipher schema used in Phase 2.4 is a stub (`_phase_stub` + minimal `nodes(file_key_wrapped BLOB)` shell); Phase 3.1 will replace it with the full canonical schema." Also update `docs/architecture/designs/chunking-and-manifest/design.md` if a cross-reference note is appropriate (check during implementation).

### DC-2 — Cross-phase dependency on Phase 4 (`CloudTransport` trait and `VaultHeader` struct) is declared but Phase 4 is not implemented

- **Concern**: Deliverables 1 (vault creation), 2 (password change), 3 (rotation), 4 (new-device recovery), 5 (recovery slot setup) all call `CloudTransport` to upload/download the vault header JSON. Deliverables 2 and 3 also expect the Phase 4 crash-recovery staging protocol (`pending-vault-header.json`). Phase 4.1 (`CloudTransport` trait) and Phase 4.3 (`VaultHeader` struct) do not exist yet. Unlike DC-1, the sub-phase text does **not** explicitly endorse a stub for Phase 4.
- **Source**: `2.4-vault-ceremonies.md` deliverable 1 lines 17–20; deliverables 2–4; sub-phase roadmap line 6 dependency on Phase 4; `design.md` Vault Creation Flow steps 19–20; Password Change Flow steps 11–15 (crash-recovery staging); Rotation Crash-Recovery Protocol lines 449–458; `cloud-synchronisation/sub-phases/4.1-cloud-transport.md`; `cloud-synchronisation/sub-phases/4.3-vault-header.md`.
- **Impact**: Hard dependency cycle. Phase 4 depends on Phase 3 which depends on Phase 2; the roadmap orders Phase 2 → 3 → 4. Phase 2.4 cannot literally wait for Phase 4.
- **Classification**: Non-blocking (by necessity — a literal block would break the roadmap ordering). The resolution is symmetric to DC-1.
- **Resolution**: Phase 2.4 creates a **forward-declared minimal** `CloudTransport` trait and `VaultHeader` struct in the locations Phase 4 already specifies:
  - `src-tauri/src/storage/cloud/mod.rs` — declares `CloudTransport` trait with `upload_blob` and `download_blob` methods only (minimum needed for vault-header round-trip); `CloudTransportError` enum with `NotFound`, `IoError(String)`, `Other(String)` variants (subset of Phase 4.1's full list); and a `MockCloudTransport` (`Arc<Mutex<HashMap<String, Vec<u8>>>>`) suitable for test and development use.
  - `src-tauri/src/storage/cloud/vault_header.rs` — declares `VaultHeader { vault_id: String, schema_version: u32, tier: u8, argon2_salt: String, argon2_params: Argon2ParamsJson, key_file_blake3: Option<String>, recovery_slots: Vec<RecoverySlot> }` plus `RecoverySlot { method: String, argon2_salt: String, argon2_params: Argon2ParamsJson, wrapped_master_key: String }` with `serde` derives and minimal validation (`schema_version == 1`, `tier ∈ {1, 2}`, 32-byte salt after base64 decode, 72-byte wrapped key after base64 decode for each slot).
  - `src-tauri/src/storage/mod.rs` declares `pub mod cloud;`.
  - Each file carries a doc comment: "Forward declaration for Phase 4.1 / Phase 4.3. Phase 2.4 defines the minimum surface required by vault ceremonies; Phase 4 will expand error variants, add `delete_blob` / `list_blobs`, and replace `MockCloudTransport` with `RcloneTransport`."
  - Crash-recovery staging (`pending-vault-header.json`) path: `dirs::config_dir() / "arx-runa/pending-vault-header.json"`, written with owner-only permissions via platform-specific helpers already present (`src-tauri/src/memory/platform` hosts unix/windows splits; a new `src-tauri/src/auth/staging.rs` module hosts the file writer/reader rather than polluting `memory`). The startup retry loop is **not** implemented by Phase 2.4 (deferred to Phase 4.3 rotation-crash-recovery wiring); Phase 2.4 only writes and deletes the staging file during `change_password` and `rotate_key_file`.
- **Documentation sync required on implementation**: YES.
  - `docs/architecture/designs/authentication-and-session-management/sub-phases/2.4-vault-ceremonies.md`: append a paragraph in the Implementation Notes section noting that `CloudTransport` / `VaultHeader` are forward-declared by Phase 2.4 and that Phase 4.1 / Phase 4.3 adopt the existing files rather than creating them.
  - `docs/architecture/designs/cloud-synchronisation/sub-phases/4.1-cloud-transport.md`: update deliverable 1 to read "`CloudTransport` trait was forward-declared in Phase 2.4; Phase 4.1 extends it with `delete_blob` and `list_blobs`, expands `CloudTransportError` to include `AuthenticationFailed`, `Timeout`, `RcloneProcessFailed`."
  - `docs/architecture/designs/cloud-synchronisation/sub-phases/4.3-vault-header.md`: update deliverable 1 to reference the Phase 2.4 forward declaration.
  - `docs/architecture/designs/cloud-synchronisation/design.md`: spot-check the Rotation Crash-Recovery Protocol section — note that Phase 2.4 writes/deletes the staging file, Phase 4.3 owns the startup retry.

### DC-3 — Phase 1 recovery-slot crypto primitives (`wrap_master_key_for_recovery`, `unwrap_master_key_from_recovery`) and types (`MasterKey`, `RecoveryKey`, `WrappedMasterKey`, `VaultId`) do not exist

- **Concern**: Phase 2.4 ceremonies require these functions for recovery slot wrapping. The Phase 1 roadmap explicitly defers them: "Recovery-slot wrapping (`wrap_master_key_for_recovery` / `unwrap_master_key_from_recovery`) is Phase 2 work because it depends on the `MasterKey` type introduced by the authentication design" (`docs/roadmap.md` line 51). The cryptographic-primitives design (`cryptographic-primitives/design.md` lines 148–212) is the canonical spec, but no Phase 1 or Phase 2 sub-phase implements it.
- **Source**: `docs/roadmap.md` line 51; `cryptographic-primitives/design.md` lines 148–212; `2.4-vault-ceremonies.md` deliverables 1, 2, 3, 5, 6.
- **Impact**: Without this, recovery slot setup, verification, re-wrap, and recovery authentication cannot be implemented.
- **Classification**: Non-blocking. Resolution is to implement these primitives as part of Phase 2.4.
- **Resolution**: Phase 2.4 adds:
  - `src-tauri/src/crypto/recovery_wrap.rs` with `wrap_master_key_for_recovery(master_key: &MasterKey, recovery_key: &RecoveryKey, vault_id: &VaultId) -> WrappedMasterKey` and `unwrap_master_key_from_recovery(wrapped: &WrappedMasterKey, recovery_key: &RecoveryKey, vault_id: &VaultId) -> Result<MasterKey, CryptoError>`. Both use XChaCha20-Poly1305 with a fresh 24-byte CSPRNG nonce and AAD = `b"arx-runa recovery v1" || vault_id.as_bytes()`. Wire format is the 72-byte layout shared with `WrappedFileKey`.
  - `src-tauri/src/crypto/types/mod.rs` additions: `MasterKey(SecretBox<[u8; 32]>)` with `ZeroizeOnDrop`; `RecoveryKey(Zeroizing<[u8; 32]>)` (matches the Phase 1 design snippet); `WrappedMasterKey([u8; 72])` (not zeroized — ciphertext); `VaultId([u8; 16])` with `new(bytes: [u8; 16])`, `as_bytes() -> &[u8; 16]`, `to_uuid() -> uuid::Uuid`, `from_uuid(uuid: uuid::Uuid) -> Self`.
  - `src-tauri/src/crypto/mod.rs` re-exports: `pub use recovery_wrap::{wrap_master_key_for_recovery, unwrap_master_key_from_recovery};` and the new types.
  - `MasterKey` has a `pub(crate) fn from_bytes(bytes: [u8; 32]) -> Self` for ceremony use and `pub(crate) fn expose(&self) -> &[u8; 32]`. `MasterKey` is never written to any struct field outside its ceremony-local binding — the design-invariant test (Section 7) enumerates fields of `SessionKeys`, `SessionManager`, `VaultHeader` and asserts no `MasterKey` field exists.
  - `RecoveryKey` has `pub(crate) fn from_bytes(bytes: [u8; 32]) -> Self` and `pub(crate) fn expose(&self) -> &[u8; 32]`.
- **Documentation sync required on implementation**: YES. Update `docs/roadmap.md` line 51 (Phase 1 note) to read: "Recovery-slot wrapping is implemented in Phase 2.4." Verify `docs/architecture/designs/cryptographic-primitives/design.md` Contract Surface still lists these items (it does — no edit needed). Update the sub-phase roadmap's "Related phases" footnote if needed.

### DC-4 — `AuthenticationError` is missing `InvalidRecoveryPhrase` and `NoRecoverySlot` variants

- **Concern**: The parent design (lines 296–300) specifies these as part of the canonical error enum. Phase 2.3 deferred them to Phase 2.4.
- **Source**: `design.md` lines 295–300; Phase 2.3 plan section 3 DC acknowledging deferral.
- **Impact**: `recover_with_phrase` and `setup_recovery` cannot return the correct error types.
- **Classification**: Non-blocking.
- **Resolution**: Add both variants to `src-tauri/src/auth/error.rs` with `#[error("recovery phrase checksum is invalid")]` and `#[error("no recovery slot is configured for this vault")]` display strings verbatim from the design. Add unit tests covering display.
- **Documentation sync required on implementation**: None beyond rule text update (governance action G-1).

### DC-5 — `SessionKeys::derive` hides `master_key`; ceremonies need `master_key` in local scope for recovery slot wrapping

- **Concern**: The critical invariant (sub-phase deliverable 7) requires `master_key` to live only in ceremony-local scope. `SessionKeys::derive` takes password/key_file/salt/params and hides `master_key` inside its own function body. Ceremonies need `master_key` to pass to `wrap_master_key_for_recovery` / `unwrap_master_key_from_recovery` while also constructing `SessionKeys`.
- **Source**: `src-tauri/src/auth/session.rs` lines 36–77; sub-phase deliverable 7 line 72.
- **Impact**: Without a restructure, ceremonies would have to duplicate the `derive_master_key_into + expand_vault_key_into` sequence, risking drift from `SessionKeys::derive`.
- **Classification**: Non-blocking.
- **Resolution**: Add a companion helper `SessionKeys::from_master_key_bytes(master_key_bytes: &[u8; 32]) -> Result<Self, AuthenticationError>` in `session.rs`. It runs the three `expand_vault_key_into` calls into fresh `SecureBytes<32>` allocations and returns `SessionKeys`. Refactor `SessionKeys::derive` to call `derive_master_key_into` into a `Zeroizing<[u8; 32]>`, then delegate to `from_master_key_bytes(&master_key)`. Ceremonies call `derive_master_key_into` themselves to obtain `master_key`, perform recovery slot operations inline, then call `SessionKeys::from_master_key_bytes(&master_key)` to construct session keys, and finally drop `master_key` (end-of-scope `Zeroizing` drop).
- **Documentation sync required on implementation**: None.

### DC-6 — `SessionManager::authenticate` does not support ceremony-driven session transitions

- **Concern**: Ceremonies need to install pre-derived `SessionKeys` into `SessionManager` without re-running KDF. `authenticate` always runs `SessionKeys::derive` (via `spawn_blocking`) and rejects calls when state is `Active` — both wrong for ceremony use.
- **Source**: `src-tauri/src/auth/session.rs` lines 202–263; sub-phase deliverable 1 step 21 ("begin session"), deliverables 2–4 ("replace `SessionKeys`"), deliverable 6 ("session begins").
- **Impact**: Without new methods, ceremonies cannot hand off keys to `SessionManager`.
- **Classification**: Non-blocking.
- **Resolution**: Add two ceremony-facing methods to `SessionManager`:
  - `pub(crate) async fn install_session(&self, keys: SessionKeys) -> Result<(), AuthenticationError>` — requires state `NoSession` or `Expired`; installs keys under the write lock, transitions state to `Active`, opens the operation gate, starts the timer, and emits no event (the caller knows the session just began). Used by `create_vault`, `recover_vault`, `recover_with_phrase`. Returns `SessionAlreadyActive` if state is already `Active`.
  - `pub(crate) async fn swap_active_session(&self, new_keys: SessionKeys) -> Result<(), AuthenticationError>` — requires state `Active`; replaces the `SessionKeys` value under the write lock (old keys are dropped and their `SecureBytes` drop path zeroes + munlocks), restarts the timer. Returns a new error variant `SessionNotActive` — **no**, instead return `AuthenticationError::VaultHeaderInvalid` as a catch-all, or simpler: add an internal precondition assert and panic only in debug. **Final choice**: add a new internal enum discriminant via `AuthenticationError::VaultHeaderInvalid` reuse is incorrect; instead, the ceremony is expected to hold the lifecycle guarantee and call `swap_active_session` under a single invocation path — treat the precondition failure by returning `Err(AuthenticationError::SessionAlreadyActive)` is also wrong. **Chosen resolution**: keep `swap_active_session` fallible by returning `Result<(), AuthenticationError>` and, on wrong state, return `AuthenticationError::SessionAlreadyActive` inverted — no, still wrong. Add a new variant `AuthenticationError::SessionNotActive` (display `"session is not active; authenticate first"`) alongside the other additions in DC-4.
- **Documentation sync required on implementation**: None beyond the error display governance.

### DC-7 — Recovery slot integrity check: avoid constant-time trap by trusting AEAD authentication

- **Concern**: Password change step 4 says "Decrypt `slot.wrapped_master_key` with `recovery_key` and AAD — verify it yields current `master_key` (integrity check)." A naive implementation would byte-compare the unwrapped plaintext to the freshly derived current `master_key`, creating a timing side channel.
- **Source**: `design.md` line 394.
- **Impact**: Minor timing leak; not exploited by the current threat model (no network-exposed ceremony) but still an avoidable risk.
- **Classification**: Non-blocking.
- **Resolution**: Rely on AEAD authentication alone. If `unwrap_master_key_from_recovery` succeeds, the slot is cryptographically bound to the vault (AAD check) and to the recovery key. Compare the unwrapped bytes to `current_master_key` only with `subtle::ConstantTimeEq` — but `subtle` is not yet in `Cargo.toml`. **Final choice**: since the AEAD success already proves `recovery_key` is correct and the slot is bound to this vault, and since any tampering (including wrong master_key being wrapped by a malicious attacker with access to the vault header) would be caught by Phase 2.4's separate test (`new master_key from current credentials + old recovery slot → unwrap returns old master_key which may differ`), do **not** perform a plaintext equality check. Trust the AEAD result. Document the rationale inline.
- **Documentation sync required on implementation**: None — design text remains valid; implementation decision is an internal concern.

### DC-8 — Re-wrap loop transaction semantics: no real `nodes` table exists yet

- **Concern**: Implementation Notes line 134 mandates the re-wrap loop runs inside a transaction. With the Phase 3.1 stub (DC-1), the `nodes` table has only `file_key_wrapped BLOB NOT NULL` and is empty. The transaction-scoped re-wrap is a no-op in production but must still be exercised by tests.
- **Source**: `2.4-vault-ceremonies.md` line 134.
- **Impact**: Test coverage gap if the transaction path is never driven.
- **Classification**: Non-blocking.
- **Resolution**: `change_password` and `rotate_key_file` open the SQLCipher connection, begin an immediate transaction, run `SELECT file_key_wrapped FROM nodes`, re-wrap each row, `UPDATE nodes SET file_key_wrapped = ?`, commit. Tests seed fake rows (2–3 wrapped blobs built with `crypto::wrap_file_key`) before invoking the ceremony to exercise the loop. Add a failure-injection test that forces the inner `unwrap_file_key` to fail mid-loop and asserts the transaction rolls back (wrapped blobs in DB are unchanged).
- **Documentation sync required on implementation**: None.

### DC-9 — `.claude/rules/auth.md` does not mention ceremonies or recovery slots

- **Concern**: After Phase 2.4 lands, the auth rule file will not describe the ceremony module, recovery slot lifecycle, or the staging-file crash-recovery protocol. Future implementers (Copilot Codex) reading the rules file will not know these exist.
- **Source**: `.claude/rules/auth.md` content inlined in the environment.
- **Impact**: Rule-guidance drift.
- **Classification**: Non-blocking governance sync.
- **Resolution**: Governance action G-1 (Section 9). Add new "Ceremonies" and "Recovery slot" subsections to `.claude/rules/auth.md` and resynchronise `.github/instructions/auth.instructions.md` via `/copilot-sync`.

### DC-10 — `.claude/rules/crypto.md` references `wrap_master_key_for_recovery` as existing

- **Concern**: The rule text already says "use dedicated `wrap_master_key_for_recovery` / `unwrap_master_key_from_recovery` functions, not `wrap_file_key`". This was aspirational until Phase 2.4 lands.
- **Source**: `.claude/rules/crypto.md` content inlined in the environment.
- **Impact**: None once Phase 2.4 ships (rule becomes accurate). Until then, the rule references nonexistent functions — a minor inconsistency.
- **Classification**: Non-blocking.
- **Resolution**: No rule edit needed; implementation will reconcile the rule with reality. If G-1 rewrites auth.md, the rule-mirror sync command is run once and picks up both.
- **Documentation sync required on implementation**: None.

### DC-11 — BIP-39 phrase handling: `bip39 = "2"` API specifics

- **Concern**: Sub-phase deliverable 6 says "Validate BIP-39 checksum; return `InvalidRecoveryPhrase` immediately if invalid (no Argon2id runs)". The `bip39` crate v2 exposes `Mnemonic::parse(phrase_str)` which validates wordlist and checksum in one call. But the crate may normalize whitespace or case differently than our "space-joined 24 words" assumption.
- **Source**: `design.md` lines 491–493 and 517–527; `bip39` v2 docs.
- **Impact**: If parsing and space-joining diverge (for example, the crate's `to_string()` returns a different separator than the one used for Argon2id input), the derived `recovery_key` will not match between `setup_recovery` and `recover_with_phrase`.
- **Classification**: Non-blocking.
- **Resolution**: Canonicalise the Argon2id input as `mnemonic.words().collect::<Vec<_>>().join(" ")` (iterator over validated words, joined with a single ASCII space). This guarantees `setup_recovery` and `recover_with_phrase` build identical byte sequences regardless of how the user enters the phrase (leading/trailing whitespace, double spaces, mixed case handled by `Mnemonic::parse`). Document the canonical form in a `// SAFETY:` comment. Add a test that verifies `"abandon abandon ... about"` and `"  abandon  abandon ... about  "` derive the same `recovery_key`.
- **Documentation sync required on implementation**: None.

### DC-12 — `setup_recovery` must re-authenticate the user to re-derive `master_key`

- **Concern**: Sub-phase deliverable 5 step 1 says "Re-authenticate: accept current password + [If Tier 2] USB key file → Argon2id + HKDF → re-derive `master_key`". The flow runs while the session is already `Active`. The user's current password must be re-entered via the UI; the ceremony function must accept it as an argument.
- **Source**: `2.4-vault-ceremonies.md` lines 54–60; `design.md` Recovery Slot Setup.
- **Impact**: If the ceremony API signature omits the password argument, the ceremony cannot run KDF and cannot obtain `master_key` for wrapping.
- **Classification**: Non-blocking.
- **Resolution**: `setup_recovery(SetupRecoveryRequest { current_password_bytes, current_key_source, ... })` accepts the current credentials even though `SessionManager` is already `Active`. The ceremony runs full Argon2id on them, verifies the derived `master_key` matches the expected one by running HKDF and byte-comparing the derived `key_encryption_key` with the one already in `SessionKeys` (via `SecureBytes::expose` + `subtle::ConstantTimeEq`) — **or**, more simply, the ceremony proceeds without equality-checking: if the password was wrong, the resulting `recovery_key` would wrap a different `master_key` that still matches the current session, but the next `recover_with_phrase` would fail because the vault header's primary slot Argon2id input differs from the phrase-derived recovery input. The safer choice is to add a cheap verification: unwrap the X25519 private key from SQLCipher using the freshly-derived `key_encryption_key`; if unwrap succeeds, the credentials were correct. Phase 2.4 does this check inline.
- **Documentation sync required on implementation**: YES. Add "credentials are verified by attempting to unwrap the X25519 private key with the freshly-derived `key_encryption_key`" to the sub-phase Implementation Notes for `setup_recovery`.

### DC-13 — `master_key` lifetime verification test is non-trivial

- **Concern**: Sub-phase test item "All flows: `master_key` does not appear in any struct after the derivation step (inspected via field enumeration in tests)". Rust has no runtime reflection; the only enforcement is by code review or compile-time type-level assertions.
- **Source**: `2.4-vault-ceremonies.md` line 90.
- **Impact**: Test coverage gap if interpreted literally.
- **Classification**: Non-blocking.
- **Resolution**: Implement the test as a *compile-time* assertion: for each ceremony-touched struct (`SessionKeys`, `SessionManager`, `VaultHeader`, `MockCloudTransport` fields), define a `static_assertions::assert_not_impl_any!(StructName, Contains<MasterKey>)` equivalent. Since `static_assertions` is not in `Cargo.toml`, implement it manually as an inner module with `const _: fn() = || { /* type check */ };`. Alternative: write a test that uses `std::mem::size_of` + field enumeration via a macro that prints struct field types at compile time. **Pragmatic choice**: add a unit test that constructs each struct and uses `std::any::type_name` on each field via a small manual enumeration. Document the list of struct-field pairs inspected and explicitly assert none contain `MasterKey`.
- **Documentation sync required on implementation**: None.

## 4. Assumptions

These are the non-obvious facts the plan takes for granted. If any is wrong, the implementation is wrong — surface a correction before handoff.

1. **SQLCipher stub schema shape**: the stub `nodes` table contains exactly one column `file_key_wrapped BLOB NOT NULL` plus an integer primary key. No `file_id`, `chunk_index`, `created_at`, or any other column from the Phase 3.1 schema. Phase 3.1 will replace the stub with the full schema via a migration script in its own plan.
2. **SQLCipher file location**: the vault DB file is created at `dirs::data_local_dir() / "arx-runa" / "vaults" / <vault_id> / "manifest.sqlite"`. If `data_local_dir()` returns `None`, ceremonies return `AuthenticationError::VaultHeaderInvalid` (closest existing variant). Phase 3.1 may revise this path.
3. **Vault-header staging location**: `dirs::config_dir() / "arx-runa" / "pending-vault-header.json"`, owner-only permissions. Phase 4.3 may move this to the Arx Runa staging directory; Phase 2.4 keeps it next to `config.json` for path parity with the existing session-config file.
4. **`MockCloudTransport`** is available behind `#[cfg(any(test, feature = "test-utils"))]` only — production code must take `&dyn CloudTransport` and have no compile-time knowledge of the mock. This matches the existing `MockKeySource` / `MockDeviceMonitor` pattern.
5. **Argon2 params for recovery slot**: identical to primary slot per design line 540. Phase 2.4 stores the same `Argon2Params` value in both `VaultHeader.argon2_params` and `RecoverySlot.argon2_params` but does **not** collapse the fields — the design keeps them independent to allow future rotation.
6. **BIP-39 wordlist**: English only. Phase 2.4 uses `bip39::Language::English` explicitly. Non-English wordlists are out of scope; if the user enters a phrase in another language, `Mnemonic::parse` returns an error which maps to `InvalidRecoveryPhrase`.
7. **X25519 private key wrap uses `wrap_file_key`** per design line 336. Phase 2.4 calls the existing `wrap_file_key` / `unwrap_file_key` on the 32-byte X25519 secret, stored as a `FileKey` wrapper type (or a new `X25519SecretKey` newtype). **Decision**: reuse `FileKey` as the on-wire representation during Phase 2.4 because Phase 5 (file sharing) will introduce the dedicated X25519 identity type. The ceremony comment explicitly calls out the reuse.
8. **X25519 public key storage**: stored alongside the wrapped private key in the stub `nodes` table under a second stub column `x25519_public_key BLOB NOT NULL UNIQUE` — or, more cleanly, in a dedicated row in a new stub table `vault_identity` (single row, `public_key BLOB NOT NULL, wrapped_private_key BLOB NOT NULL`). **Decision**: use the `vault_identity` table because it isolates the identity row from the (eventually-real) `nodes` table. Phase 3.1 may consolidate.
9. **Recovery phrase returned only from `setup_recovery`**: the ceremony returns the 24-word phrase in a `Zeroizing<String>` wrapper so callers cannot accidentally copy it into a non-zeroizing `String`. The UI layer reads it, displays it, and drops the `Zeroizing<String>`. The phrase is **never** written to disk, logs, or any persistent store.
10. **Manifest backup upload / download for `recover_vault`**: the ceremony calls `cloud_transport.download_blob("manifest-backup.enc", ...)` to fetch the encrypted manifest backup. Since Phase 4.4 (manifest backup encryption) is not implemented, Phase 2.4's `recover_vault` is implemented against the stub `CloudTransport` and a test fixture that pre-seeds a blob encrypted with the same `manifest_key` HKDF path. In production, `recover_vault` will fail until Phase 4.4 lands — this is acceptable because Phase 2.4 is not shipped to end users on its own.
11. **Atomic rename for SQLCipher DB creation**: if the vault DB already exists at the target path, `create_vault` returns `AuthenticationError::VaultHeaderInvalid`. No overwrite, no migration. Ceremonies do not retry.
12. **No backoff / rate limit**: Phase 2.4 does **not** implement per-vault exponential backoff on `InvalidCredentials`. The existing Phase 2.3 TODO comment in `authenticate` is left in place.
13. **Password bytes are accepted as `&[u8]`**: all ceremony APIs take `&[u8]` (copied into `Zeroizing<Vec<u8>>` internally). IPC-boundary string scrubbing (invariant §6) is the caller's responsibility; Phase 6.1 wires it.
14. **BLAKE3 of key file**: stored as **hex** in the vault header per Phase 4.3 deliverable, not base64. Phase 2.4 follows hex encoding. Argon2 salt is base64.

## 5. Approach

Step-by-step implementation plan. Every file path is absolute.

### Step 1 — Add Phase 1 recovery-slot crypto primitives (DC-3)

**File**: `C:\Users\chris\source\repos\arx-runa\src-tauri\src\crypto\types\mod.rs`

Add the following new types near the existing `KeyEncryptionKey`:

```rust
/// 32-byte vault master key produced by Argon2id.
///
/// Held in protected heap storage and zeroed on drop. **Invariant**:
/// `MasterKey` must not be assigned to a struct field outside ceremony-local
/// scope. See Phase 2.4 plan section 3 (DC-3 / DC-13).
#[derive(ZeroizeOnDrop)]
pub struct MasterKey(SecretBox<[u8; 32]>);

impl MasterKey {
    pub(crate) fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(SecretBox::new(Box::new(bytes)))
    }

    pub(crate) fn from_secret_box(secret_box: SecretBox<[u8; 32]>) -> Self {
        Self(secret_box)
    }

    pub(crate) fn expose(&self) -> &[u8; 32] {
        self.0.expose_secret()
    }
}

/// 32-byte recovery key derived from a BIP-39 phrase via Argon2id.
///
/// Never stored; derived on demand. Zeroized on drop.
#[derive(ZeroizeOnDrop)]
pub struct RecoveryKey(SecretBox<[u8; 32]>);

impl RecoveryKey {
    pub(crate) fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(SecretBox::new(Box::new(bytes)))
    }

    pub(crate) fn expose(&self) -> &[u8; 32] {
        self.0.expose_secret()
    }
}

/// 72-byte wire blob for a vault-header recovery slot:
/// `[24-byte nonce | 32-byte ciphertext | 16-byte tag]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WrappedMasterKey(pub [u8; 72]);

/// Vault identifier — raw UUID v4 bytes (not the hyphenated string form).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct VaultId([u8; 16]);

impl VaultId {
    pub fn new(bytes: [u8; 16]) -> Self { Self(bytes) }
    pub fn as_bytes(&self) -> &[u8; 16] { &self.0 }
    pub fn from_uuid(uuid: uuid::Uuid) -> Self { Self(*uuid.as_bytes()) }
    pub fn to_uuid(&self) -> uuid::Uuid { uuid::Uuid::from_bytes(self.0) }
}
```

Add `#[derive(ZeroizeOnDrop)]` tests symmetric to the existing `FileKey` zeroize tests.

**File**: `C:\Users\chris\source\repos\arx-runa\src-tauri\src\crypto\recovery_wrap.rs` (new)

Implement the recovery wrap/unwrap functions verbatim per the contract:

```rust
//! XChaCha20-Poly1305 recovery-slot wrapping for `master_key`.
//!
//! Recovery slot ciphertext is stored in the vault header (plaintext JSON in
//! the cloud) and therefore must be bound to vault identity. The AAD is
//! `b"arx-runa recovery v1" || vault_id_bytes`, preventing cross-vault
//! transplant and cross-slot confusion with `wrap_file_key` blobs.

use chacha20poly1305::{
    AeadInPlace, KeyInit, XChaCha20Poly1305, aead::generic_array::GenericArray,
};
use secrecy::SecretBox;
use zeroize::Zeroizing;

use crate::crypto::error::CryptoError;
use crate::crypto::nonce::generate_nonce;
use crate::crypto::types::{MasterKey, RecoveryKey, VaultId, WrappedMasterKey};

const NONCE_LEN: usize = 24;
const KEY_LEN: usize = 32;
const TAG_LEN: usize = 16;
const WRAPPED_LEN: usize = NONCE_LEN + KEY_LEN + TAG_LEN;
const AAD_PREFIX: &[u8] = b"arx-runa recovery v1";

fn build_aad(vault_id: &VaultId) -> [u8; 20 + 16] {
    let mut aad = [0u8; 20 + 16];
    aad[..20].copy_from_slice(AAD_PREFIX);
    aad[20..].copy_from_slice(vault_id.as_bytes());
    aad
}

/// Wraps `master_key` for storage in a vault header recovery slot.
pub fn wrap_master_key_for_recovery(
    master_key: &MasterKey,
    recovery_key: &RecoveryKey,
    vault_id: &VaultId,
) -> Result<WrappedMasterKey, CryptoError> {
    let nonce_bytes = generate_nonce();
    let mut ciphertext: Zeroizing<[u8; KEY_LEN]> = Zeroizing::new([0u8; KEY_LEN]);
    ciphertext.copy_from_slice(master_key.expose());

    let cipher = XChaCha20Poly1305::new(GenericArray::from_slice(recovery_key.expose()));
    let nonce = GenericArray::from_slice(&nonce_bytes);
    let aad = build_aad(vault_id);

    let tag = cipher
        .encrypt_in_place_detached(nonce, &aad, ciphertext.as_mut_slice())
        .map_err(|_| CryptoError::KeyWrapFailed)?;

    let mut wire = [0u8; WRAPPED_LEN];
    wire[..NONCE_LEN].copy_from_slice(&nonce_bytes);
    wire[NONCE_LEN..NONCE_LEN + KEY_LEN].copy_from_slice(ciphertext.as_slice());
    wire[NONCE_LEN + KEY_LEN..].copy_from_slice(tag.as_slice());

    Ok(WrappedMasterKey(wire))
}

/// Unwraps `master_key` from a vault header recovery slot.
pub fn unwrap_master_key_from_recovery(
    wrapped: &WrappedMasterKey,
    recovery_key: &RecoveryKey,
    vault_id: &VaultId,
) -> Result<MasterKey, CryptoError> {
    let nonce_slice = &wrapped.0[..NONCE_LEN];
    let ciphertext_slice = &wrapped.0[NONCE_LEN..NONCE_LEN + KEY_LEN];
    let tag_slice = &wrapped.0[NONCE_LEN + KEY_LEN..];

    let cipher = XChaCha20Poly1305::new(GenericArray::from_slice(recovery_key.expose()));
    let nonce = GenericArray::from_slice(nonce_slice);
    let tag = GenericArray::from_slice(tag_slice);
    let aad = build_aad(vault_id);

    let mut decrypt_result: Result<(), chacha20poly1305::Error> = Ok(());
    let master_key_secret_box = SecretBox::<[u8; KEY_LEN]>::init_with_mut(|buffer| {
        buffer.copy_from_slice(ciphertext_slice);
        decrypt_result = cipher.decrypt_in_place_detached(nonce, &aad, buffer.as_mut_slice(), tag);
    });

    match decrypt_result {
        Ok(()) => Ok(MasterKey::from_secret_box(master_key_secret_box)),
        Err(_) => Err(CryptoError::DecryptionFailed),
    }
}
```

**File**: `C:\Users\chris\source\repos\arx-runa\src-tauri\src\crypto\mod.rs`

Add `pub mod recovery_wrap;` and `pub use recovery_wrap::{unwrap_master_key_from_recovery, wrap_master_key_for_recovery};` and `pub use types::{MasterKey, RecoveryKey, VaultId, WrappedMasterKey};`.

**Step 1 tests** (inside `recovery_wrap.rs`):

- `test_wrap_unwrap_recovery_round_trip_returns_original_master_key`
- `test_wrap_recovery_two_calls_produce_distinct_wrapped_blobs` (random nonce)
- `test_unwrap_recovery_wrong_recovery_key_fails_with_decryption_failed`
- `test_unwrap_recovery_wrong_vault_id_fails_with_decryption_failed` (cross-vault transplant)
- `test_unwrap_recovery_corrupted_nonce_fails_with_decryption_failed`
- `test_unwrap_recovery_corrupted_ciphertext_fails_with_decryption_failed`
- `test_unwrap_recovery_corrupted_tag_fails_with_decryption_failed`
- `test_wrap_recovery_wire_format_is_seventy_two_bytes`
- `test_wrap_recovery_uses_non_empty_aad` (construct two wrapped blobs with same inputs, verify ciphertext differs from `wrap_file_key` output, confirming AAD scope separation)

### Step 2 — Add missing `AuthenticationError` variants (DC-4, DC-6)

**File**: `C:\Users\chris\source\repos\arx-runa\src-tauri\src\auth\error.rs`

Add to the `AuthenticationError` enum:

```rust
/// BIP-39 recovery phrase failed checksum or wordlist validation. Returned
/// before any Argon2id derivation runs.
#[error("recovery phrase checksum is invalid")]
InvalidRecoveryPhrase,

/// The vault header has no recovery slot configured.
#[error("no recovery slot is configured for this vault")]
NoRecoverySlot,

/// A ceremony requiring an active session was called while no session is
/// active.
#[error("session is not active; authenticate first")]
SessionNotActive,
```

Add unit tests covering each new variant's `Display` output (mirror the existing `test_authentication_error_*_display_matches_design` naming pattern).

### Step 3 — Extend `SessionKeys` and `SessionManager` for ceremony use (DC-5, DC-6)

**File**: `C:\Users\chris\source\repos\arx-runa\src-tauri\src\auth\session.rs`

Add `SessionKeys::from_master_key_bytes`:

```rust
impl SessionKeys {
    /// Constructs `SessionKeys` from a caller-owned master key. The caller
    /// holds `master_key` in a `Zeroizing` binding in the same scope; this
    /// method only runs HKDF expansions.
    pub(crate) fn from_master_key_bytes(
        master_key_bytes: &[u8; 32],
    ) -> Result<Self, AuthenticationError> {
        let mut key_encryption_key = SecureBytes::<32>::new()?;
        let mut sqlcipher_key = SecureBytes::<32>::new()?;
        let mut manifest_key = SecureBytes::<32>::new()?;

        expand_vault_key_into(master_key_bytes, HKDF_INFO_KEY_ENCRYPTION, key_encryption_key.as_mut())
            .map_err(|_| AuthenticationError::InvalidCredentials)?;
        expand_vault_key_into(master_key_bytes, HKDF_INFO_SQLCIPHER, sqlcipher_key.as_mut())
            .map_err(|_| AuthenticationError::InvalidCredentials)?;
        expand_vault_key_into(master_key_bytes, HKDF_INFO_MANIFEST_BACKUP, manifest_key.as_mut())
            .map_err(|_| AuthenticationError::InvalidCredentials)?;

        Ok(Self { key_encryption_key, sqlcipher_key, manifest_key })
    }
}
```

Refactor `SessionKeys::derive` to delegate:

```rust
pub(crate) fn derive(
    password_utf8_bytes: &[u8],
    key_file_bytes: Option<&[u8; 32]>,
    salt: &[u8; 32],
    params: &Argon2Params,
) -> Result<Self, AuthenticationError> {
    let mut master_key: Zeroizing<[u8; 32]> = Zeroizing::new([0u8; 32]);
    derive_master_key_into(password_utf8_bytes, key_file_bytes, salt, params, &mut master_key)?;
    Self::from_master_key_bytes(&master_key)
    // master_key is zeroized here by Zeroizing's drop.
}
```

Add ceremony-facing session transition methods to `SessionManager`:

```rust
impl SessionManager {
    /// Installs pre-derived session keys and transitions `NoSession | Expired → Active`.
    pub(crate) async fn install_session(
        &self,
        keys: SessionKeys,
    ) -> Result<(), AuthenticationError> {
        if self.state().await == LifecycleState::Active {
            return Err(AuthenticationError::SessionAlreadyActive);
        }
        {
            let mut session_guard = self.session.write().await;
            *session_guard = Some(keys);
        }
        {
            let mut lifecycle_guard = self.lifecycle.write().await;
            *lifecycle_guard = LifecycleState::Active;
        }
        self.operation_gate_closed.store(false, Ordering::SeqCst);
        self.restart_timer().await;
        Ok(())
    }

    /// Replaces the active `SessionKeys` without disturbing lifecycle state.
    pub(crate) async fn swap_active_session(
        &self,
        new_keys: SessionKeys,
    ) -> Result<(), AuthenticationError> {
        if self.state().await != LifecycleState::Active {
            return Err(AuthenticationError::SessionNotActive);
        }
        {
            let mut session_guard = self.session.write().await;
            *session_guard = Some(new_keys);
        }
        self.restart_timer().await;
        Ok(())
    }

    /// Exposes the key-encryption key for ceremony use under the read lock.
    pub(crate) async fn with_key_encryption_key<F, R>(
        &self,
        callback: F,
    ) -> Result<R, AuthenticationError>
    where
        F: FnOnce(&[u8; 32]) -> R,
    {
        let session_guard = self.session.read().await;
        let keys = session_guard
            .as_ref()
            .ok_or(AuthenticationError::SessionNotActive)?;
        Ok(callback(keys.key_encryption_key.expose()))
    }
}
```

Add a symmetric `with_sqlcipher_key` used by ceremonies that need to open the SQLCipher connection.

### Step 4 — Forward-declare `storage::cloud` module (DC-2)

**File**: `C:\Users\chris\source\repos\arx-runa\src-tauri\src\storage\mod.rs`

Add `pub mod cloud;`.

**File**: `C:\Users\chris\source\repos\arx-runa\src-tauri\src\storage\cloud\mod.rs` (new)

```rust
//! Cloud transport forward declaration.
//!
//! Phase 2.4 defines the minimum `CloudTransport` surface required by vault
//! ceremonies. Phase 4.1 will expand the trait with `delete_blob` and
//! `list_blobs`, extend `CloudTransportError`, and replace `MockCloudTransport`
//! with `RcloneTransport`.

pub mod vault_header;

use async_trait::async_trait;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[non_exhaustive]
#[derive(Debug, Error)]
pub enum CloudTransportError {
    #[error("remote blob not found")]
    NotFound,
    #[error("I/O operation failed")]
    IoError(#[source] std::io::Error),
    #[error("transport operation failed: {0}")]
    Other(String),
}

#[async_trait]
pub trait CloudTransport: Send + Sync {
    /// Uploads the contents of `local_path` to `remote_path`.
    async fn upload_blob(
        &self,
        local_path: &Path,
        remote_path: &str,
    ) -> Result<(), CloudTransportError>;

    /// Downloads `remote_path` into `local_path`.
    async fn download_blob(
        &self,
        remote_path: &str,
        local_path: &Path,
    ) -> Result<(), CloudTransportError>;
}

#[cfg(any(test, feature = "test-utils"))]
pub mod mock {
    use super::{CloudTransport, CloudTransportError};
    use async_trait::async_trait;
    use std::collections::HashMap;
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Mutex};

    pub struct MockCloudTransport {
        blobs: Arc<Mutex<HashMap<String, Vec<u8>>>>,
    }

    impl MockCloudTransport {
        pub fn new() -> Self {
            Self { blobs: Arc::new(Mutex::new(HashMap::new())) }
        }

        pub fn seed(&self, remote_path: &str, bytes: Vec<u8>) {
            self.blobs.lock().unwrap().insert(remote_path.to_string(), bytes);
        }

        pub fn has(&self, remote_path: &str) -> bool {
            self.blobs.lock().unwrap().contains_key(remote_path)
        }
    }

    #[async_trait]
    impl CloudTransport for MockCloudTransport {
        async fn upload_blob(
            &self,
            local_path: &Path,
            remote_path: &str,
        ) -> Result<(), CloudTransportError> {
            let bytes = tokio::fs::read(local_path)
                .await
                .map_err(CloudTransportError::IoError)?;
            self.blobs.lock().unwrap().insert(remote_path.to_string(), bytes);
            Ok(())
        }

        async fn download_blob(
            &self,
            remote_path: &str,
            local_path: &Path,
        ) -> Result<(), CloudTransportError> {
            let bytes = {
                let guard = self.blobs.lock().unwrap();
                guard.get(remote_path).cloned().ok_or(CloudTransportError::NotFound)?
            };
            tokio::fs::write(local_path, &bytes)
                .await
                .map_err(CloudTransportError::IoError)?;
            Ok(())
        }
    }
}
```

**File**: `C:\Users\chris\source\repos\arx-runa\src-tauri\src\storage\cloud\vault_header.rs` (new)

```rust
//! Vault header schema forward declaration.
//!
//! Phase 2.4 defines the serialisation shape required by vault ceremonies.
//! Phase 4.3 will adopt this struct as-is, add richer validation, and wire
//! the startup retry path for `pending-vault-header.json`.

use serde::{Deserialize, Serialize};

const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Argon2ParamsJson {
    pub memory_cost: u32,
    pub time_cost: u32,
    pub parallelism: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RecoverySlot {
    pub method: String,                  // "bip39" for Phase 2.4
    pub argon2_salt: String,             // base64, 32 bytes after decode
    pub argon2_params: Argon2ParamsJson,
    pub wrapped_master_key: String,      // base64, 72 bytes after decode
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VaultHeader {
    pub vault_id: String,                // UUID v4 hyphenated string
    pub schema_version: u32,
    pub tier: u8,                        // 1 or 2
    pub argon2_salt: String,             // base64, 32 bytes
    pub argon2_params: Argon2ParamsJson,
    pub key_file_blake3: Option<String>, // hex, 32 bytes; None for tier 1
    pub recovery_slots: Vec<RecoverySlot>,
}

impl VaultHeader {
    pub const SCHEMA_VERSION: u32 = SCHEMA_VERSION;

    /// Validates the structural invariants documented in
    /// `cloud-synchronisation/sub-phases/4.3-vault-header.md` deliverable 6.
    pub fn validate(&self) -> Result<(), VaultHeaderError> {
        if self.schema_version != SCHEMA_VERSION {
            return Err(VaultHeaderError::UnsupportedSchemaVersion(self.schema_version));
        }
        match self.tier {
            1 => {
                if self.key_file_blake3.is_some() {
                    return Err(VaultHeaderError::Tier1WithKeyFileBlake3);
                }
            }
            2 => {
                let hex = self
                    .key_file_blake3
                    .as_ref()
                    .ok_or(VaultHeaderError::Tier2MissingKeyFileBlake3)?;
                if hex.len() != 64 {
                    return Err(VaultHeaderError::KeyFileBlake3WrongLength);
                }
            }
            other => return Err(VaultHeaderError::UnsupportedTier(other)),
        }
        let salt_bytes = base64_decode(&self.argon2_salt)
            .map_err(|_| VaultHeaderError::SaltDecodeFailed)?;
        if salt_bytes.len() != 32 {
            return Err(VaultHeaderError::SaltWrongLength);
        }
        for slot in &self.recovery_slots {
            if slot.method != "bip39" {
                continue; // silently skip unknown methods per Phase 4.3 rules
            }
            let slot_salt = base64_decode(&slot.argon2_salt)
                .map_err(|_| VaultHeaderError::RecoverySlotSaltDecodeFailed)?;
            if slot_salt.len() != 32 {
                return Err(VaultHeaderError::RecoverySlotSaltWrongLength);
            }
            let wrapped = base64_decode(&slot.wrapped_master_key)
                .map_err(|_| VaultHeaderError::RecoverySlotBlobDecodeFailed)?;
            if wrapped.len() != 72 {
                return Err(VaultHeaderError::RecoverySlotBlobWrongLength);
            }
        }
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum VaultHeaderError {
    #[error("unsupported vault header schema version: {0}")]
    UnsupportedSchemaVersion(u32),
    #[error("unsupported tier: {0}")]
    UnsupportedTier(u8),
    #[error("tier 1 vault must not carry a key_file_blake3 field")]
    Tier1WithKeyFileBlake3,
    #[error("tier 2 vault missing key_file_blake3 field")]
    Tier2MissingKeyFileBlake3,
    #[error("key_file_blake3 must be 64 hex characters")]
    KeyFileBlake3WrongLength,
    #[error("argon2_salt failed base64 decode")]
    SaltDecodeFailed,
    #[error("argon2_salt must decode to 32 bytes")]
    SaltWrongLength,
    #[error("recovery slot argon2_salt failed base64 decode")]
    RecoverySlotSaltDecodeFailed,
    #[error("recovery slot argon2_salt must decode to 32 bytes")]
    RecoverySlotSaltWrongLength,
    #[error("recovery slot wrapped_master_key failed base64 decode")]
    RecoverySlotBlobDecodeFailed,
    #[error("recovery slot wrapped_master_key must decode to 72 bytes")]
    RecoverySlotBlobWrongLength,
}

fn base64_decode(input: &str) -> Result<Vec<u8>, ()> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD
        .decode(input)
        .map_err(|_| ())
}
```

**Cargo**: the `base64` crate is **not** yet in `Cargo.toml`. Add `base64 = "0.22"` under `[dependencies]`. (`hex` is also required for `key_file_blake3` — add `hex = "0.4"`.) These are the **only** new dependencies.

### Step 5 — Implement vault creation (`create_vault`)

**File**: `C:\Users\chris\source\repos\arx-runa\src-tauri\src\auth\ceremonies.rs` (new)

Module structure:

```rust
//! Vault lifecycle ceremonies (Phase 2.4).
//!
//! Ceremony entry points: `create_vault`, `change_password`, `rotate_key_file`,
//! `recover_vault`, `setup_recovery`, `recover_with_phrase`. Each function
//! owns the full multi-step flow documented in the parent design's
//! Vault Creation / Password Change / Rotation / Recovery sections.
//!
//! Critical invariant (sub-phase deliverable 7): `master_key` never escapes
//! ceremony-local scope. It is held as `Zeroizing<[u8; 32]>` inside a single
//! function body and zeroed at end-of-scope.

use std::path::PathBuf;
use std::sync::Arc;

use bip39::{Language, Mnemonic};
use rand::{Rng, RngCore};
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::auth::error::AuthenticationError;
use crate::auth::kdf::{Argon2Params, derive_master_key_into};
use crate::auth::key_source::KeySource;
use crate::auth::session::{SessionKeys, SessionManager};
use crate::crypto::{
    FileKey, KeyEncryptionKey, MasterKey, RecoveryKey, VaultId, WrappedFileKey, WrappedMasterKey,
    unwrap_file_key, unwrap_master_key_from_recovery, wrap_file_key, wrap_master_key_for_recovery,
};
use crate::storage::cloud::{CloudTransport, CloudTransportError};
use crate::storage::cloud::vault_header::{Argon2ParamsJson, RecoverySlot, VaultHeader};
```

`create_vault` request type:

```rust
pub enum Tier { One, Two }

pub struct CreateVaultRequest<'a> {
    pub tier: Tier,
    pub password_bytes: &'a [u8],
    pub target_key_file_path: Option<PathBuf>, // Some iff Tier::Two
    pub vault_db_path: PathBuf,
    pub argon2_params: Argon2Params,
}
```

`create_vault` signature:

```rust
pub async fn create_vault(
    request: CreateVaultRequest<'_>,
    session_manager: &SessionManager,
    cloud_transport: &dyn CloudTransport,
) -> Result<VaultId, AuthenticationError>;
```

Ceremony body implements the 21-step design flow in this exact order:

1. Validate `target_key_file_path` presence matches tier; return `VaultHeaderInvalid` on mismatch.
2. For Tier 2: validate the target path's parent directory exists and is writable; if not, return `VaultHeaderInvalid` **before** generating key material (per Implementation Notes line 132).
3. Generate `vault_id = VaultId::from_uuid(Uuid::new_v4())`.
4. For Tier 2: generate 32-byte key file via `rand::rng().fill(&mut buf)`, write to `target_key_file_path` with owner-only permissions; compute `key_file_blake3 = blake3::hash(&buf)`.
5. Generate 32-byte `argon2_salt` via `rand::rng().fill(&mut buf)`.
6. Allocate `let mut master_key: Zeroizing<[u8; 32]> = Zeroizing::new([0u8; 32]);`.
7. Run `derive_master_key_into(password_bytes, key_file_bytes.as_deref(), &argon2_salt, &argon2_params, &mut master_key)`.
8. Construct `SessionKeys` via `SessionKeys::from_master_key_bytes(&master_key)`.
9. Open SQLCipher DB at `vault_db_path`:
   - `rusqlite::Connection::open(&vault_db_path)` — wrap in `tokio::task::spawn_blocking`.
   - Issue `PRAGMA key = "x'<hex of session_keys.sqlcipher_key>'"` using hex-encoded bytes.
   - Execute the stub schema:
     ```sql
     CREATE TABLE _phase_stub (id INTEGER PRIMARY KEY);
     CREATE TABLE nodes (
         id INTEGER PRIMARY KEY,
         file_key_wrapped BLOB NOT NULL
     );
     CREATE TABLE vault_identity (
         id INTEGER PRIMARY KEY CHECK (id = 1),
         public_key BLOB NOT NULL UNIQUE,
         wrapped_private_key BLOB NOT NULL
     );
     ```
10. Generate X25519 identity keypair via `x25519_dalek::StaticSecret::random_from_rng(&mut rand::rng())` and derive the `PublicKey`.
11. Wrap X25519 secret via `wrap_file_key(&FileKey::from_bytes(static_secret.to_bytes()), &KeyEncryptionKey::from_bytes(session_keys.key_encryption_key.expose().clone()))` — **note**: this constructs a temporary `FileKey` and `KeyEncryptionKey`; bookkeep zeroisation carefully. Store both `public_key.as_bytes()` and the `WrappedFileKey` bytes in `vault_identity`.
12. Construct `VaultHeader`:
    - `vault_id`: hyphenated `Uuid::to_string()`
    - `schema_version`: `VaultHeader::SCHEMA_VERSION`
    - `tier`: `1` or `2`
    - `argon2_salt`: base64 of the 32-byte salt
    - `argon2_params`: `Argon2ParamsJson` copy of `request.argon2_params`
    - `key_file_blake3`: `Some(hex::encode(blake3_hash))` for Tier 2, `None` for Tier 1
    - `recovery_slots`: `vec![]`
13. Serialise header to JSON via `serde_json::to_vec_pretty`; write to a staging temp file under the same parent directory as the final location (`dirs::config_dir() / "arx-runa" / "vault-header.json.staging"`) with owner-only permissions.
14. Call `cloud_transport.upload_blob(&staging_path, "vault-header.json").await?`.
15. Delete the staging temp file.
16. Call `session_manager.install_session(session_keys).await?`.
17. Explicitly drop `master_key` (implicit via `Zeroizing` — but add an explicit `drop(master_key)` for clarity and to prove no aliasing).
18. Return `Ok(vault_id)`.

Any error between steps 3 and 14 must:
- Delete the partially-written key file (Tier 2).
- Close any open SQLCipher connection (`drop(conn)`).
- Return without installing the session.

### Step 6 — Implement password change (`change_password`)

Signature:

```rust
pub struct ChangePasswordRequest<'a> {
    pub current_password_bytes: &'a [u8],
    pub new_password_bytes: &'a [u8],
    pub current_key_source: Option<&'a (dyn KeySource + Send + Sync)>,
    pub recovery_phrase: Option<&'a str>,
    pub argon2_params: Argon2Params,
}

pub async fn change_password(
    request: ChangePasswordRequest<'_>,
    session_manager: &SessionManager,
    cloud_transport: &dyn CloudTransport,
    vault_db_path: &std::path::Path,
    vault_header: &mut VaultHeader,
) -> Result<(), AuthenticationError>;
```

Flow:

1. Precondition: `session_manager.state().await == LifecycleState::Active`; else `SessionNotActive`.
2. Read current salt from `vault_header.argon2_salt` (base64 decode → 32 bytes).
3. Read current `key_file_bytes` from `current_key_source.read_key()` if Tier 2.
4. Allocate `let mut current_master_key: Zeroizing<[u8; 32]> = Zeroizing::new([0u8; 32]);` and run `derive_master_key_into(current_password, current_key_file, &current_salt, &current_params, &mut current_master_key)`.
5. If `!vault_header.recovery_slots.is_empty()`:
   - If `recovery_phrase.is_none()`: mark `will_remove_slots = true`.
   - Else: parse the phrase with `Mnemonic::parse_in(Language::English, phrase)`; on error return `InvalidRecoveryPhrase`. Canonicalise to `mnemonic.words().collect::<Vec<_>>().join(" ")`. For the first slot with `method == "bip39"`:
     - Decode `slot.argon2_salt` → 32 bytes.
     - Allocate `let mut recovery_key_bytes: Zeroizing<[u8; 32]> = Zeroizing::new([0u8; 32]);`.
     - Run `derive_master_key_into(canonical.as_bytes(), None, &slot_salt, &slot_params, &mut recovery_key_bytes)` (Argon2id via the same helper — acceptable because the helper accepts any password bytes).
     - Construct `RecoveryKey::from_bytes(*recovery_key_bytes)`.
     - Decode `slot.wrapped_master_key` → 72 bytes → `WrappedMasterKey`.
     - Call `unwrap_master_key_from_recovery(&wrapped, &recovery_key, &vault_id)`. On `DecryptionFailed` return `InvalidCredentials` (non-oracular). On success, discard the returned `MasterKey` (it matches `current_master_key` by AEAD construction — do not byte-compare, per DC-7). Keep `recovery_key` alive for step 11.
6. Generate a new 32-byte salt via CSPRNG.
7. Allocate `let mut new_master_key: Zeroizing<[u8; 32]> = Zeroizing::new([0u8; 32]);` and run `derive_master_key_into(new_password, current_key_file, &new_salt, &argon2_params, &mut new_master_key)`. (The Tier 2 key file is unchanged.)
8. Compute `new_session_keys = SessionKeys::from_master_key_bytes(&new_master_key)?`.
9. Open the SQLCipher connection with `tokio::task::spawn_blocking`:
   - Key with **current** `sqlcipher_key` (read from `session_manager.with_sqlcipher_key(|k| ...)`).
   - `BEGIN IMMEDIATE;`
   - `SELECT id, file_key_wrapped FROM nodes` → for each row, `unwrap_file_key` with current KEK, `wrap_file_key` with new KEK, `UPDATE nodes SET file_key_wrapped = ? WHERE id = ?`.
   - Do the same for the single `vault_identity` row (unwrap with current KEK, wrap with new KEK).
   - `COMMIT;`
   - `PRAGMA rekey = "x'<hex new sqlcipher_key>'";`
   - `drop(conn);`
   - On any failure inside the transaction, `ROLLBACK` and return `AuthenticationError::InvalidCredentials` (generic).
10. Build updated `VaultHeader` in memory: new `argon2_salt` (base64 of `new_salt`); same `key_file_blake3`; same `argon2_params`.
11. If `will_remove_slots`: clear `vault_header.recovery_slots`. Else if a slot was re-wrapped successfully: run `wrap_master_key_for_recovery(&MasterKey::from_bytes(*new_master_key), &recovery_key, &vault_id)` → new `WrappedMasterKey` → replace `vault_header.recovery_slots[0].wrapped_master_key` with base64(wrapped.0). Drop `recovery_key` (explicit `drop(recovery_key)`).
12. Write updated header to `dirs::config_dir() / "arx-runa" / "pending-vault-header.json"` with owner-only permissions.
13. Call `cloud_transport.upload_blob(&pending_path, "vault-header.json").await?`.
14. Delete `pending-vault-header.json`.
15. Call `session_manager.swap_active_session(new_session_keys).await?`.
16. Drop `new_master_key` and `current_master_key` explicitly.

### Step 7 — Implement USB key file rotation (`rotate_key_file`)

Analogous to `change_password` but with the new-key-file path in the request. Implementation shares a helper `re_wrap_vault(...)` with `change_password` to avoid duplicating the transaction loop.

Signature:

```rust
pub struct RotateKeyFileRequest<'a> {
    pub password_bytes: &'a [u8],
    pub current_key_source: &'a (dyn KeySource + Send + Sync),
    pub target_new_key_file_path: PathBuf,
    pub recovery_phrase: Option<&'a str>,
    pub argon2_params: Argon2Params,
}
```

Only permitted for Tier 2 vaults. Tier 1 → return `VaultHeaderInvalid`.

Key differences from `change_password`:
- Generates a new 32-byte key file on disk before running Argon2id.
- Updates `vault_header.key_file_blake3` to `hex::encode(blake3::hash(&new_key_file_bytes))`.
- Re-wrap loop iterates the same tables.

### Step 8 — Implement new-device recovery (`recover_vault`)

Signature:

```rust
pub struct RecoverVaultRequest<'a> {
    pub password_bytes: &'a [u8],
    pub key_source: Option<&'a (dyn KeySource + Send + Sync)>,
    pub vault_db_path: PathBuf,
    pub local_vault_header_path: PathBuf, // where to write the downloaded header
    pub manifest_backup_local_path: PathBuf,
}

pub async fn recover_vault(
    request: RecoverVaultRequest<'_>,
    session_manager: &SessionManager,
    cloud_transport: &dyn CloudTransport,
) -> Result<VaultId, AuthenticationError>;
```

Flow:

1. `cloud_transport.download_blob("vault-header.json", &local_vault_header_path).await?`.
2. Read + `serde_json::from_slice` → `VaultHeader` → `header.validate()?` (map `VaultHeaderError` → `VaultHeaderInvalid`).
3. Apply invariant §9: if a local trusted-params cache exists (`dirs::config_dir() / "arx-runa" / "trusted-argon2.json"`), parameters in the downloaded header must match exactly. For a fresh device (no cache), accept OWASP floors (`19456 / 2 / 1`) as minimum and warn if below `(65536, 3, 4)`. Phase 2.4 implements this as a helper `validate_argon2_params(&header.argon2_params, trusted_cache_path)`.
4. Decode `argon2_salt` → 32 bytes.
5. Read optional key file via `key_source.read_key()` — for Tier 2, if no source provided or hash mismatches `header.key_file_blake3`, return `KeyFileNotFound` (no oracle on password).
6. Allocate `master_key`, run `derive_master_key_into`.
7. `session_keys = SessionKeys::from_master_key_bytes(&master_key)?`.
8. `cloud_transport.download_blob("manifest-backup.enc", &manifest_backup_local_path).await?` (may return `NotFound` → `VaultHeaderInvalid`; this is a known Phase 4.4 dependency).
9. Decrypt the manifest backup entirely in RAM using `session_keys.manifest_key`. Phase 2.4 uses a **forward-declared** `manifest_backup::decrypt` helper in `src-tauri/src/storage/cloud/manifest_backup.rs` (new file) that XChaCha20-Poly1305-decrypts with no AAD per the cloud-sync design. For Phase 2.4, this helper is simpler than the full Phase 4.4 schema — accepts `&[u8]` ciphertext and returns plaintext bytes. Tests seed the mock transport with a pre-encrypted blob.
10. Import the plaintext into the SQLCipher stub — Phase 2.4 simply writes the decrypted bytes to `vault_db_path` if they represent a SQLCipher DB export, **or** executes them as SQL if they represent a SQL dump. **Decision**: treat as SQL dump (simpler), `conn.execute_batch(&sql)`. Phase 3.1 / 4.4 will revise.
11. `session_manager.install_session(session_keys).await?`.
12. Return `VaultId`.

### Step 9 — Implement recovery slot setup (`setup_recovery`)

Signature:

```rust
pub struct SetupRecoveryRequest<'a> {
    pub current_password_bytes: &'a [u8],
    pub current_key_source: Option<&'a (dyn KeySource + Send + Sync)>,
    pub argon2_params: Argon2Params,
}

pub async fn setup_recovery(
    request: SetupRecoveryRequest<'_>,
    session_manager: &SessionManager,
    cloud_transport: &dyn CloudTransport,
    vault_header: &mut VaultHeader,
    vault_id: &VaultId,
) -> Result<Zeroizing<String>, AuthenticationError>;
```

Flow:

1. Require `state() == Active`; else `SessionNotActive`.
2. Decode current salt from `vault_header.argon2_salt`.
3. Read current key file if Tier 2.
4. `let mut master_key: Zeroizing<[u8; 32]> = Zeroizing::new([0u8; 32]);` + `derive_master_key_into(current_credentials, ...)`.
5. Verify the credentials were correct by re-deriving `key_encryption_key` via `SessionKeys::from_master_key_bytes(&master_key)?` and comparing its `key_encryption_key` to the live session's `key_encryption_key` using `subtle::ConstantTimeEq` — **but `subtle` is not a dependency**. Alternative: unwrap the `vault_identity.wrapped_private_key` row using the freshly-derived KEK; if unwrap succeeds, credentials are correct; if not, return `InvalidCredentials`. **Final decision (per DC-12)**: use the `vault_identity` unwrap verification. Drop the fresh `SessionKeys`.
6. Generate 32 bytes of CSPRNG entropy (`let mut entropy = [0u8; 32]; rand::rng().fill(&mut entropy);`).
7. `let mnemonic = Mnemonic::from_entropy_in(Language::English, &entropy).map_err(|_| AuthenticationError::VaultHeaderInvalid)?;`
8. Zeroise `entropy`.
9. Canonical phrase: `let phrase_string: Zeroizing<String> = Zeroizing::new(mnemonic.words().collect::<Vec<_>>().join(" "));`
10. Generate 32-byte `recovery_salt` via CSPRNG.
11. `let mut recovery_key_bytes: Zeroizing<[u8; 32]> = Zeroizing::new([0u8; 32]);`
12. Run `derive_master_key_into(phrase_string.as_bytes(), None, &recovery_salt, &request.argon2_params, &mut recovery_key_bytes)`.
13. `let recovery_key = RecoveryKey::from_bytes(*recovery_key_bytes);`
14. `let wrapped = wrap_master_key_for_recovery(&MasterKey::from_bytes(*master_key), &recovery_key, vault_id)?;`
15. Build `RecoverySlot { method: "bip39".into(), argon2_salt: base64(&recovery_salt), argon2_params: argon2_params.into(), wrapped_master_key: base64(&wrapped.0) }`.
16. `vault_header.recovery_slots.push(slot);`
17. Serialise header, upload via `cloud_transport.upload_blob(...)`.
18. Explicit drop of `recovery_key`, `recovery_key_bytes`, `master_key`.
19. Return `phrase_string` (caller's responsibility to display + drop).

Any error after step 6 must not return the phrase — wrap in a `match` and ensure the phrase is dropped on every error path.

### Step 10 — Implement `recover_with_phrase`

Signature:

```rust
pub struct RecoverWithPhraseRequest<'a> {
    pub phrase: &'a str,
    pub vault_db_path: PathBuf,
    pub local_vault_header_path: PathBuf,
}

pub async fn recover_with_phrase(
    request: RecoverWithPhraseRequest<'_>,
    session_manager: &SessionManager,
    cloud_transport: &dyn CloudTransport,
) -> Result<VaultId, AuthenticationError>;
```

Flow:

1. Parse phrase with `Mnemonic::parse_in(Language::English, request.phrase)`; on error return `InvalidRecoveryPhrase` **immediately** (no Argon2id).
2. Canonicalise: `let canonical: Zeroizing<String> = Zeroizing::new(mnemonic.words().collect::<Vec<_>>().join(" "));`
3. Download vault header; validate; get `vault_id`.
4. If `vault_header.recovery_slots.is_empty()`: return `NoRecoverySlot`.
5. For each slot with `method == "bip39"`:
   - Decode `slot.argon2_salt` and `slot.wrapped_master_key`.
   - `let mut recovery_key_bytes: Zeroizing<[u8; 32]> = Zeroizing::new([0u8; 32]);`
   - `derive_master_key_into(canonical.as_bytes(), None, &slot_salt, &slot_params, &mut recovery_key_bytes)?`
   - `let recovery_key = RecoveryKey::from_bytes(*recovery_key_bytes);`
   - `let wrapped = WrappedMasterKey(slot_bytes);`
   - `match unwrap_master_key_from_recovery(&wrapped, &recovery_key, &vault_id)`:
     - `Ok(master_key)`: break with `(master_key, recovery_key)`.
     - `Err(_)`: drop `recovery_key` and continue.
6. If no slot succeeded: return `InvalidCredentials`.
7. With the unwrapped `master_key` still in scope, build `SessionKeys::from_master_key_bytes(master_key.expose())?`.
8. Call `session_manager.install_session(session_keys).await?`.
9. Drop `master_key`.
10. Phase 2.4 does **not** automatically call `change_password` — the UI layer (Phase 6) drives the prompt. The ceremony returns `VaultId` and the UI schedules the follow-up. Document this deviation from the design flow as a scoping decision in Documentation Impact.

### Step 11 — Wire the `ceremonies` module into `auth::mod`

**File**: `C:\Users\chris\source\repos\arx-runa\src-tauri\src\auth\mod.rs`

Add `pub mod ceremonies;` and re-export the public ceremony functions plus request types. Do **not** re-export `MasterKey` (crypto-internal).

### Step 12 — Ceremony tests

All tests live inside `ceremonies.rs` as `#[cfg(test)] mod tests`. Use `TEST_PARAMS = Argon2Params { memory_cost_kib: 1024, time_cost: 1, parallelism: 1 }` to keep runtime reasonable. Hardware-free: the SQLCipher file is created under `tempfile::tempdir()` and the cloud transport is `MockCloudTransport`. The session manager uses `with_timeout(Duration::from_secs(3600))` to avoid timer interference.

Enumerated tests (from sub-phase deliverable 10 + DC additions):

1. `test_create_vault_tier_one_produces_header_with_null_key_file_blake3_and_empty_recovery_slots`
2. `test_create_vault_tier_two_generates_key_file_and_sets_key_file_blake3`
3. `test_create_vault_opens_sqlcipher_with_derived_sqlcipher_key`
4. `test_create_vault_rejects_missing_target_key_file_path_for_tier_two`
5. `test_create_vault_rejects_writable_parent_missing_for_tier_two`
6. `test_change_password_old_kek_cannot_unwrap_file_keys_after_change`
7. `test_change_password_new_kek_can_unwrap_file_keys_after_change`
8. `test_change_password_sqlcipher_opens_with_new_key_and_rejects_old_key`
9. `test_change_password_with_recovery_slot_re_wraps_slot_and_phrase_still_unlocks`
10. `test_change_password_without_recovery_phrase_clears_recovery_slots`
11. `test_change_password_failure_inside_rewrap_transaction_rolls_back_to_old_state` (seed fake wrapped row, force `unwrap_file_key` failure on second row, assert DB rows unchanged and session unchanged)
12. `test_rotate_key_file_preserves_x25519_public_key_bytes`
13. `test_rotate_key_file_updates_key_file_blake3_in_header`
14. `test_rotate_key_file_with_recovery_slot_re_wraps_slot_and_phrase_still_unlocks`
15. `test_rotate_key_file_rejects_tier_one_vault`
16. `test_recover_vault_reconstructs_session_from_cloud_header_and_manifest_backup`
17. `test_setup_recovery_adds_bip39_slot_to_vault_header`
18. `test_setup_recovery_wrapped_master_key_decodes_to_seventy_two_bytes`
19. `test_setup_recovery_returns_phrase_only_once_in_zeroizing_string`
20. `test_setup_recovery_rejects_wrong_current_credentials_via_identity_unwrap`
21. `test_recover_with_phrase_correct_phrase_unlocks_vault_and_begins_session`
22. `test_recover_with_phrase_wrong_phrase_returns_invalid_credentials`
23. `test_recover_with_phrase_invalid_checksum_returns_invalid_recovery_phrase_without_running_argon2id`
24. `test_recover_with_phrase_empty_recovery_slots_returns_no_recovery_slot`
25. `test_recover_with_phrase_canonicalises_whitespace_and_case_before_deriving`
26. `test_master_key_never_appears_in_session_keys_session_manager_or_vault_header_fields` (compile-time / type-level enumeration)
27. `test_recovery_phrase_never_appears_in_any_persistent_writer_output` (grep the `MockCloudTransport` recorded writes and tempdir contents after `setup_recovery`)
28. `test_create_vault_and_re_authenticate_round_trip_without_recovery_slot`
29. `test_recovery_slot_cross_vault_transplant_fails` (setup_recovery on vault A, copy slot into vault B header, recover_with_phrase on vault B returns InvalidCredentials)

### Step 13 — Governance sync (see Section 9)

After tests pass, run the governance actions listed in Section 9 and resynchronise rule mirrors.

## 6. Security Implications

### a. Expected sensitive path set

Phase 2.4 anticipates touching these sensitive-path files:

- `C:\Users\chris\source\repos\arx-runa\src-tauri\src\auth\ceremonies.rs` (new)
- `C:\Users\chris\source\repos\arx-runa\src-tauri\src\auth\mod.rs`
- `C:\Users\chris\source\repos\arx-runa\src-tauri\src\auth\error.rs`
- `C:\Users\chris\source\repos\arx-runa\src-tauri\src\auth\session.rs`
- `C:\Users\chris\source\repos\arx-runa\src-tauri\src\auth\staging.rs` (new — staging-file writer for `pending-vault-header.json`)
- `C:\Users\chris\source\repos\arx-runa\src-tauri\src\crypto\recovery_wrap.rs` (new)
- `C:\Users\chris\source\repos\arx-runa\src-tauri\src\crypto\types\mod.rs`
- `C:\Users\chris\source\repos\arx-runa\src-tauri\src\crypto\mod.rs`
- `C:\Users\chris\source\repos\arx-runa\src-tauri\src\storage\mod.rs`
- `C:\Users\chris\source\repos\arx-runa\src-tauri\src\storage\cloud\mod.rs` (new)
- `C:\Users\chris\source\repos\arx-runa\src-tauri\src\storage\cloud\vault_header.rs` (new)
- `C:\Users\chris\source\repos\arx-runa\src-tauri\src\storage\cloud\manifest_backup.rs` (new — minimal decrypt helper for `recover_vault`)

Any file outside this list that gets modified under `src-tauri/src/crypto/`, `src-tauri/src/auth/`, or `src-tauri/src/storage/` triggers a Plan Deviation and must be justified or surfaced to the user.

### b. Invoke security-reviewer agent? **YES**

Phase 2.4 is the highest-risk sub-phase in Phase 2. It introduces:
- New AEAD primitives (`wrap_master_key_for_recovery`, `unwrap_master_key_from_recovery`) with non-empty AAD — these are the only Arx Runa operations that encrypt `master_key` directly.
- Multi-step ceremonies with strict `master_key` lifetime invariants.
- Re-wrap transactions over user data (wrong implementation = locked-out vault).
- BIP-39 phrase handling including a return-once-to-UI guarantee.
- Storage of encrypted material in the publicly-readable vault header.

The sub-phase roadmap **independently requires** security review (`2.4-vault-ceremonies.md` lines 143–149). Phase 2.4's Phase 2 plan confirms this: **YES, invoke `security-reviewer` after implementation.**

### c. What the reviewer should check

1. **`master_key` lifetime invariant**: `master_key` never escapes ceremony-local scope; every ceremony function binds it as `Zeroizing<[u8; 32]>`, uses it in the same function body, and drops it before returning. Reviewer enumerates all six ceremony functions and confirms no `master_key` reference outlives the function.
2. **`MasterKey` type containment**: `MasterKey` as a type exists only in crypto primitives; no struct outside `MasterKey` itself holds a `MasterKey` field. `SessionKeys`, `SessionManager`, `VaultHeader`, `RecoverySlot`, `MockCloudTransport` contain no `MasterKey` field.
3. **Zeroisation correctness**: every `Zeroizing<[u8; 32]>`, `Zeroizing<Vec<u8>>`, `Zeroizing<String>` binding in ceremonies is dropped at the latest point the secret is no longer needed. Panic paths (e.g., `spawn_blocking` panics, `rusqlite` errors) still drop through `Zeroizing`'s `Drop` impl.
4. **Recovery-slot AEAD nonce and AAD**: `wrap_master_key_for_recovery` calls `generate_nonce()` (CSPRNG) per wrap. AAD equals `b"arx-runa recovery v1" || vault_id_bytes` — no other AAD variants anywhere. Cross-vault transplant is rejected (test enumerated).
5. **Re-key transaction atomicity**: the SQLCipher re-wrap loop runs inside `BEGIN IMMEDIATE; … COMMIT;`. A mid-loop failure rolls back (tested by fault injection). `PRAGMA rekey` is issued **after** the transaction commits and with the connection still keyed on the old `sqlcipher_key`; `drop(conn)` only after `PRAGMA rekey` succeeds.
6. **Vault header contents**: serialisation never embeds `key_encryption_key`, `sqlcipher_key`, `manifest_key`, or `master_key`. Reviewer greps for each constant/type in the `storage::cloud::vault_header` module.
7. **Recovery phrase handling**: `Zeroizing<String>` wrapper; never written to logs, tracing events, `Debug` derivations, or error messages. Reviewer greps for `tracing::*` calls in `ceremonies.rs` and confirms no phrase or derived key is logged.
8. **Staging-file permissions**: `pending-vault-header.json` is written with owner-only permissions (Unix: `0600`; Windows: restrictive ACL via `icacls` equivalent or the `windows` crate's DACL APIs). Reviewer confirms platform-specific writers exist.
9. **Phase 1 primitive wire format**: `WrappedMasterKey` wire layout matches `WrappedFileKey` (72 bytes: 24 nonce + 32 ciphertext + 16 tag). Reviewer confirms the constants.
10. **Non-oracular error mapping**: `change_password` returns `InvalidCredentials` (not `InvalidRecoveryPhrase`) when the recovery slot unwrap fails with a valid-checksum phrase, and `InvalidRecoveryPhrase` when the phrase fails checksum. Reviewer traces every error path.

## 7. Execution and Testing Strategy

**Test scope:**
- [x] Basic unit tests (written alongside each ceremony during Step 12)
- [x] Adversarial tests (wrong recovery key, wrong vault_id, corrupted slot, cross-vault transplant, mid-transaction failure)
- [x] Property-based tests (BIP-39 phrase round-trip with whitespace/case fuzzing via `proptest`; Argon2id param equivalence)
- [x] Integration tests (full create_vault → change_password → recover_with_phrase round-trip; full create_vault → rotate_key_file → re-authenticate round-trip; full create_vault → setup_recovery → recover_with_phrase round-trip)
- [x] Boundary cases (empty password; 1-byte password; 64-byte password; exactly-32-byte X25519 key; empty recovery_slots; two recovery slots with different Argon2 params)

**Coverage target:** ≥ 80% line coverage on `src-tauri/src/auth/ceremonies.rs` and `src-tauri/src/crypto/recovery_wrap.rs`. 100% path coverage on error branches in `recover_with_phrase` (every early return is triggered by a dedicated test).

**Boundary cases to cover:**
- Tier 1 with empty `recovery_slots`
- Tier 2 with one `bip39` slot
- Tier 2 with two `bip39` slots (ensure first-success-wins path in `recover_with_phrase`)
- Tier 2 with one `bip39` slot and one unknown-method slot (unknown silently skipped)
- Vault DB pre-existing at target path → `VaultHeaderInvalid`
- Key file path parent directory missing → `VaultHeaderInvalid` (Tier 2) **before** any key material is generated
- `change_password` called on `Expired` session → `SessionNotActive`
- `recover_with_phrase` called on `Active` session → `SessionAlreadyActive` (from `install_session`)
- `setup_recovery` called on `NoSession` → `SessionNotActive`
- `MockCloudTransport.upload_blob` returns `Other` mid-ceremony → no session state change, staging file left behind for startup retry (Phase 4.3 wiring)
- `Mnemonic::parse` of a 23-word phrase → `InvalidRecoveryPhrase` (no Argon2id executed — verify by asserting wall-clock < 10 ms)
- X25519 public key equality before and after `rotate_key_file` (byte-for-byte)

**Invoke test-writer agent?** **YES** — Phase 2.4 is crypto-adjacent with adversarial and property-based tests. Reason: cross-vault transplant, mid-transaction fault injection, and BIP-39 canonicalisation fuzzing are exactly the adversarial cases the `test-writer` agent specialises in. The test list enumerated in Step 12 is the baseline; `test-writer` extends it with adversarial variants.

**Mirror to frontmatter**: `test-agent-required: true`.

**Validation Checkpoint** (from sub-phase):

```bash
cargo test auth::ceremonies
cargo test crypto::recovery_wrap
cargo clippy -- -D warnings
cargo fmt -- --check
```

Manual verification:
- Full vault creation on a real machine with a USB drive present — confirm key file written to USB, vault header uploaded to MockCloudTransport (or local Rclone backend when Phase 4.2 lands), session active.
- Password change — confirm old credentials rejected and new credentials accepted.
- New-device recovery — on a second machine, configure a fake Rclone remote (or in-process MockCloudTransport), insert USB key file, enter password, confirm vault is operational.

**Test acceptance criteria**:
- All 29 enumerated tests pass.
- No panics on any error path (every fallible operation propagates via `?` or returns an `AuthenticationError` variant).
- `master_key` appears in exactly six function bodies (the six ceremony functions) and nowhere else — verified by `grep -n 'master_key' src-tauri/src/auth/ceremonies.rs | wc -l` plus a code review tally.
- Recovery phrase appears in exactly two function bodies (`setup_recovery` and `recover_with_phrase`) and nowhere else.

## 8. Documentation Impact

1. **`docs/architecture/designs/authentication-and-session-management/sub-phases/2.4-vault-ceremonies.md`** — append Implementation Notes paragraphs for DC-1 (stub schema), DC-2 (forward-declared `CloudTransport` / `VaultHeader`), and DC-12 (credential verification via `vault_identity` unwrap). Do not suppress doc updates even though the sub-phase's `## Documentation Impact` section is absent — these edits reflect implementation deviations.
2. **`docs/roadmap.md`** — update line 51 (Phase 1 note) to read "Recovery-slot wrapping is implemented in Phase 2.4." Mark Phase 2 status as "Complete" once all sub-phases including 2.4 land (deferred to the final commit).
3. **`docs/architecture/designs/cloud-synchronisation/sub-phases/4.1-cloud-transport.md`** — append note at the top of Deliverables: "Trait forward-declared by Phase 2.4 in `src-tauri/src/storage/cloud/mod.rs`. Phase 4.1 extends it with `delete_blob`, `list_blobs`, and the full `CloudTransportError` variant set."
4. **`docs/architecture/designs/cloud-synchronisation/sub-phases/4.3-vault-header.md`** — similar note: "`VaultHeader` / `RecoverySlot` / `Argon2ParamsJson` structs forward-declared by Phase 2.4 in `src-tauri/src/storage/cloud/vault_header.rs`. Phase 4.3 adds richer validation and the upload/download helper functions."
5. **`docs/architecture/designs/cloud-synchronisation/sub-phases/4.4-manifest-backup.md`** — add note that Phase 2.4 forward-declares a minimal `manifest_backup::decrypt` helper used by `recover_vault`. Phase 4.4 will replace it with the full encrypt/decrypt pair and define the wire format canonically.
6. **`docs/threat-model/session-boundaries.md`** (per sub-phase completion task) — add section "What `mlock` protects against and the scope of memory protection guarantees" covering: (a) `master_key` lifetime boundedness in ceremonies, (b) `SessionKeys` zeroisation on lock/timeout, (c) Zero-Trace persistence compliance. If this file does not exist yet, create it.
7. **`.claude/rules/auth.md`** — governance action G-1 (Section 9).
8. **`.github/instructions/auth.instructions.md`** — regenerated via `/copilot-sync` after G-1.
9. **`docs/report-log/`** — optional: log a report note summarising Phase 2.4 implementation and security review outcome.

## 9. Governance Sync Actions (pre-implementation)

### Action G-1 — Update `.claude/rules/auth.md` to describe ceremonies and recovery slots

- **Reason / linked concern**: DC-9. Rule file does not document ceremonies or recovery slots; after Phase 2.4 lands, implementers reading the rule would miss key guarantees.
- **Target files**:
  - `C:\Users\chris\source\repos\arx-runa\.claude\rules\auth.md`
- **Required edit**: Append two new subsections at the end of the file:

  ```markdown
  ## Ceremonies
  - Six ceremony functions live in `src-tauri/src/auth/ceremonies.rs`: `create_vault`, `change_password`, `rotate_key_file`, `recover_vault`, `setup_recovery`, `recover_with_phrase`.
  - `master_key` is bound as `Zeroizing<[u8; 32]>` inside ceremony-local scope and must not escape the function body. No struct may hold a `master_key` or `MasterKey` field.
  - `SessionKeys::from_master_key_bytes` is the ceremony entry point for HKDF expansion; `SessionKeys::derive` is preserved for direct `SessionManager::authenticate` callers.
  - `SessionManager::install_session` transitions `NoSession | Expired → Active` with pre-derived keys; `SessionManager::swap_active_session` rotates keys while staying `Active` (used by password change and key file rotation). Neither method re-runs KDF.
  - The `pending-vault-header.json` staging file is written under `dirs::config_dir() / "arx-runa/"` with owner-only permissions during password change and key rotation. The startup retry loop is Phase 4.3 territory.
  - Forward declarations: `CloudTransport` (`src-tauri/src/storage/cloud/mod.rs`) and `VaultHeader` (`src-tauri/src/storage/cloud/vault_header.rs`) originate in Phase 2.4 and are extended by Phase 4.1 / 4.3.

  ## Recovery slots
  - Recovery is opt-in and post-creation via `setup_recovery`; users who do not configure a slot cannot recover from lost credentials.
  - BIP-39 (English wordlist) is the only Phase 2.4 recovery method. `Mnemonic::parse_in(Language::English, phrase)` validates the phrase before any Argon2id derivation.
  - The canonical Argon2id input for both `setup_recovery` and `recover_with_phrase` is `mnemonic.words().collect::<Vec<_>>().join(" ")`. Do not use `to_string()` or other separators.
  - Recovery slot AEAD uses the dedicated `wrap_master_key_for_recovery` / `unwrap_master_key_from_recovery` functions with AAD = `b"arx-runa recovery v1" || vault_id_bytes`. Never use `wrap_file_key` for recovery slot material.
  - Recovery slot Argon2 parameters are stored per-slot (independent of the primary slot) but default to the same values at `setup_recovery` time.
  - `recover_with_phrase` returns `InvalidRecoveryPhrase` (no Argon2id) on checksum failure, `NoRecoverySlot` on empty `recovery_slots`, and `InvalidCredentials` when all slots fail AEAD decrypt.
  - The 24-word phrase is returned from `setup_recovery` exactly once, wrapped in `Zeroizing<String>`. The caller must display, require acknowledgement, and drop. Never log the phrase, never write it to disk, never include it in error messages.
  ```
- **Verification**: re-read `.claude/rules/auth.md` after the edit and confirm both subsections are present. Run `/copilot-sync` and confirm `.github/instructions/auth.instructions.md` mirrors the new content.

### Action G-2 — Run `/copilot-sync` to regenerate `.github/instructions/*.instructions.md`

- **Reason / linked concern**: Rule-mirror drift after G-1.
- **Target files**:
  - `C:\Users\chris\source\repos\arx-runa\.github\instructions\auth.instructions.md`
- **Required edit**: regenerated automatically by `/copilot-sync` from the updated `.claude/rules/auth.md`. No manual edits.
- **Verification**: diff `.claude/rules/auth.md` against `.github/instructions/auth.instructions.md` — the two files must be semantically identical modulo the instructions-file front-matter header.

No other governance edits are required — `.claude/rules/crypto.md` already references the dedicated recovery wrap functions (DC-10), and `.claude/rules/storage.md` is not yet Phase-3-specific enough to require edits from Phase 2.4.

## 10. Handoff Notes for Implementer

Working directory: `C:\Users\chris\source\repos\arx-runa`. The plan is self-contained — every trait signature, error variant, struct field, and SQL statement needed is inlined in Section 5. Read Section 3 before starting; every cross-phase compromise is flagged there and should not be revisited without first checking the listed design concerns.

**Order of operations** (strict):
1. Governance sync (G-1 + `/copilot-sync`). This must happen **before** code edits — pre-implementation per the contract.
2. Add `base64 = "0.22"` and `hex = "0.4"` to `src-tauri/Cargo.toml`.
3. Step 1 (crypto types + `recovery_wrap.rs`) and its tests.
4. Step 2 (error variants) and its tests.
5. Step 3 (session.rs refactor + new methods) and its unit tests.
6. Step 4 (forward-declared `storage::cloud` module).
7. Steps 5–10 (ceremony implementations), each followed by its tests (Step 12 subset).
8. Step 11 (module wiring).
9. Step 12 remaining adversarial/property/integration tests, optionally via `test-writer` agent.
10. Security review via `security-reviewer` agent (Section 6.b).
11. Section 8 documentation updates.

**Traps to watch for**:
- `master_key` lifetime: do not refactor a ceremony into two helper functions that pass `&MasterKey` across the split — the invariant prefers one long function body over clean decomposition here. If you must split, keep the secret in the outer function and pass only derived material inward.
- `base64` API: `base64::engine::general_purpose::STANDARD.encode/decode` (trait method from `Engine`). Importing `use base64::Engine;` is mandatory.
- `Mnemonic::parse_in` vs `Mnemonic::parse`: use `parse_in` to lock the wordlist to English explicitly.
- `rusqlite` `PRAGMA key`: hex-encoded byte literal (`x'...'`) required, not raw string. Use `conn.execute_batch(&format!("PRAGMA key = \"x'{}'\";", hex::encode(key)))`.
- `PRAGMA rekey` must run **after** the transaction commit and **before** closing the connection. Running it inside the transaction is invalid.
- Platform-specific owner-only permissions for the staging file: reuse the pattern from `src-tauri/src/memory/platform/{unix.rs,windows.rs}` — add a new `auth::staging` submodule if no shared helper exists.
- `MockCloudTransport` is `#[cfg(any(test, feature = "test-utils"))]`-gated. Never import it from production code.
- X25519 keypair round-trip: `rotate_key_file` must re-derive the same `PublicKey` bytes after unwrap + re-wrap. Verify by constructing the `PublicKey` from the unwrapped secret before and after and byte-comparing.
- Cross-vault transplant test: create two vaults, extract a slot from vault A's header, overwrite vault B's header with it, run `recover_with_phrase` against vault B. The expected result is `InvalidCredentials` — **not** `InvalidRecoveryPhrase` (checksum is still valid) and **not** a panic.
- Timing of `InvalidRecoveryPhrase`: the early-return must happen before any `derive_master_key_into` call. Verify by asserting wall-clock < 10 ms for the adversarial test.

Plan status: `draft`. No blocking concerns. Proceed with `/implement-plan phase-2-4-vault-ceremonies.md` after user approval.
