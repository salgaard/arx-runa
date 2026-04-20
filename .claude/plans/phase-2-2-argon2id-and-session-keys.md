---
title: "Phase 2.2 — Argon2id KDF and SessionKeys"
created: "2026-04-13T00:00:00Z"
status: implemented
roadmap-phase: 2
sub-phase: "2.2"
design-document: "docs/architecture/designs/authentication-and-session-management/design.md"
sub-phase-roadmap: "docs/architecture/designs/authentication-and-session-management/sub-phases/roadmap.md"
test-agent-required: true
governance-sync-required: true
tags: [auth, phase-2, argon2id, hkdf, session-keys, mlock, memory-protection]
---

# Plan: Phase 2.2 — Argon2id KDF and SessionKeys

## 1. Goal

Implement the Argon2id KDF wrapper that produces `master_key` from `password (|| key_file_bytes)`, wire it to Phase 1.1's HKDF tree, and introduce a memory-locked `SessionKeys` container so all three vault-level keys (`key_encryption_key`, `sqlcipher_key`, `manifest_key`) land directly in mlocked/`VirtualLock`ed heap buffers with `ZeroizeOnDrop` guarantees and a hard-failing `AuthenticationError::MemoryLockFailed` on lock failure.

## 2. Context

**Roadmap**: Phase 2 — Authentication and Session Management (`docs/roadmap.md` lines 55–61). Depends on Phase 1 (complete) and Phase 2.1 (complete). Produces `SessionKeys` consumed by Phase 2.3 (`SessionManager`) and Phase 2.4 (vault ceremonies).

**Sub-phase roadmap**: `docs/architecture/designs/authentication-and-session-management/sub-phases/roadmap.md`. Strict order 2.1 → 2.2 → 2.3 → 2.4. 2.2 is the second unit. Security review **required** per the roadmap's Security Review Checkpoints table. Estimated scope: ~150 lines production + ~120 lines tests.

**Sub-phase doc**: `docs/architecture/designs/authentication-and-session-management/sub-phases/2.2-argon2id-and-session-keys.md` (deliverables 1–8).

**Parent design sections used** (absolute paths):

- `docs/architecture/designs/authentication-and-session-management/design.md` lines 86–182: Input Construction for Argon2id, parameter table, HKDF tree, `master_key` lifetime rule.
- Same file lines 183–252: SessionKeys struct, session ownership (`SharedSession`), memory locking, failure message strings.
- Same file lines 272–297: `AuthenticationError` enum variants.
- Same file lines 21–47: Contract Surface — canonical interface / data / invariant / dependency contracts. The sub-phase must match the Contract Surface, not duplicate it.
- `docs/architecture/design-invariants.md` §3 (HKDF constants `arx-runa-v1`), §6 (IPC sensitive-input handling — called by later phases but informs the `&[u8]` password signature), §7 (zero-trace persistence), §9 (Argon2 vault-header trust contract — bootstrap vs existing-device validation).

**Existing state** (branch `development`, commit `03fda23`):

- `src-tauri/src/auth/mod.rs` re-exports `KeySource`, `KeySourceError`, `DeviceMonitor`, etc. from Phase 2.1. No `kdf` module. No `session` module.
- `src-tauri/src/auth/error.rs` defines `AuthError` (not `AuthenticationError`) with one variant `KeySource(KeySourceError)`. The sub-phase and the parent design both call the enum `AuthenticationError` — this plan renames.
- `src-tauri/src/memory/mod.rs` declares `pub mod error; pub mod types;` and nothing else. `error.rs` has an empty `MemoryError` enum. `types/mod.rs` is a module comment only. **No platform-specific locking code exists yet** — Phase 2.2 is the first consumer of the memory module and owns its mlock/VirtualLock wrappers.
- `src-tauri/src/crypto/hkdf.rs` exposes `derive_vault_keys(master_key_bytes: &[u8; 32]) -> Result<VaultKeys, CryptoError>` and the private constants `HKDF_SALT = b"arx-runa-v1"`, `HKDF_INFO_KEY_ENCRYPTION`, `HKDF_INFO_SQLCIPHER`, `HKDF_INFO_MANIFEST_BACKUP`. `VaultKeys` wraps the three outputs in `SecretBox<[u8; 32]>` but **does not mlock** — so reusing `derive_vault_keys` as-is would leave key material in non-locked heap for a scope, violating the sub-phase's "mlock before key material is written" acceptance criterion. Section 5 resolves this.
- `src-tauri/Cargo.toml` already pins `argon2 = "0.5"`, `hkdf = "0.13"`, `sha2 = "0.11"`, `zeroize = { version = "1", features = ["derive"] }`, `secrecy = "0.10"`, `thiserror = "2"`. It already pins `windows = { version = "0.59", features = ["Win32_Storage_FileSystem", "Win32_Foundation"] }` — **does not yet include `Win32_System_Memory`** (needed for `VirtualLock`/`VirtualUnlock`). It has **no `libc` dependency** for POSIX `mlock`.
- `.claude/rules/auth.md`, `.claude/rules/crypto.md`, `.claude/rules/memory-protection.md` and their `.github/instructions/*.instructions.md` mirrors already describe Argon2id parameters, HKDF constants, the mlock hard-fail rule, and session-key zeroization. The rule mirrors are in sync (Phase 2.1 left them consistent).
- Phase 2.1 plan (`.claude/plans/phase-2-1-usb-key-file-and-device-monitor.md`, status `approved`) is the template for sub-phase plan structure used here.

**No pending architectural decisions** in the roadmap touch Phase 2.2 directly. Invariant #9 (Argon2 vault-header trust contract) is satisfied in Phase 2.4 (vault ceremonies) — 2.2's KDF wrapper only runs the Argon2id derivation with the params it is given; bootstrap validation belongs to the caller.

## 3. Design Concerns / Open Questions

### DC-1 — Sub-phase says `SessionKeys` fields are `SecretBox<[u8; 32]>` but also mandates mlock before writes; these are incompatible

- **Concern**: `SecretBox::<[u8; 32]>::init_with_mut(|buffer| …)` from `secrecy 0.10` allocates a `Box<[u8; 32]>` then immediately calls the closure that writes the key material. There is no API hook to mlock the allocation between `Box` construction and the closure. The sub-phase (2.2 deliverable 4) requires "`mlock`/`VirtualLock` is applied before key material is written, not after" (Security Review bullet 3). Holding `SecretBox` as the field type and mlocking separately works only if the field's backing `Box` pointer is exposed — `secrecy 0.10` does not expose that.
- **Source**: 2.2 sub-phase doc deliverable 3 (`SessionKeys` field types, line 18) and deliverable 4 + Security Review bullet 3 (mlock ordering, lines 19 and 85).
- **Impact**: Literal implementation is infeasible. Codex would have to silently drop either the field-type spec or the ordering rule.
- **Classification**: Non-blocking.
- **Resolution**: Introduce a new safe RAII wrapper `SecureBytes<const N: usize>` in `src-tauri/src/memory/secure_buffer.rs` that:
  1. Allocates `Box<[u8; N]>` zero-initialized.
  2. Calls `platform::lock_memory(ptr, N)` BEFORE any caller writes key material.
  3. Exposes a single `init_with_mut<F: FnOnce(&mut [u8; N]) -> Result<(), E>>(f: F) -> Result<Self, Either<MemoryLockError, E>>` constructor so the lock runs before `f`.
  4. On `Drop`: `Zeroize::zeroize(&mut *buffer)` → `platform::unlock_memory(ptr, N)` → free.
  5. Exposes `expose(&self) -> &[u8; N]` for read-only consumers (`secrecy`-style).

  `SessionKeys` fields become `key_encryption_key: SecureBytes<32>`, etc. This is a material deviation from the literal sub-phase text ("fields `key_encryption_key: SecretBox<[u8; 32]>`"). The spirit of the sub-phase is preserved — `SecureBytes<N>` is the `ZeroizeOnDrop`-equivalent that additionally enforces mlock.
- **Documentation sync required on implementation**: YES. Update the following sub-phase / design sections once implemented:
  - `docs/architecture/designs/authentication-and-session-management/sub-phases/2.2-argon2id-and-session-keys.md` deliverable 3 (line 18): replace `SecretBox<[u8; 32]>` with `SecureBytes<32>` for the three fields, keep the `#[derive(ZeroizeOnDrop)]` line but note that the derive may be omitted if `SecureBytes` already implements `Drop`.
  - `docs/architecture/designs/authentication-and-session-management/design.md` lines 186–195 (SessionKeys struct): same field-type update.

### DC-2 — Reusing `derive_vault_keys` leaves an unlocked intermediate SecretBox in scope

- **Concern**: `crypto::hkdf::derive_vault_keys(&master_key)` allocates three `SecretBox<[u8; 32]>` via `SecretBox::init_with_mut`. None of them are mlocked. If Phase 2.2 calls this and then copies the key bytes into locked `SecureBytes<32>`, there is a window where the key material exists in non-locked heap. This contradicts "memory locking on `SessionKeys` construction" (2.2 deliverable 4) and the sub-phase's acceptance criterion "mlock/VirtualLock is applied to all key buffers before any key material is written into them".
- **Source**: 2.2 sub-phase doc deliverable 2 ("Integration with Phase 1.1's `derive_vault_keys` function", line 17), versus deliverable 4 and the acceptance criterion (lines 19 and 57).
- **Impact**: Sub-phase is internally inconsistent. Literal "reuse derive_vault_keys" violates the mlock-before-write invariant. Without guidance, Codex will either silently break the invariant or silently deviate from the reuse directive.
- **Classification**: Non-blocking.
- **Resolution**: Refactor `src-tauri/src/crypto/hkdf.rs` to expose a new `pub(crate)` helper `expand_vault_key_into(master_key: &[u8; 32], info: &[u8], output: &mut [u8; 32]) -> Result<(), CryptoError>` that runs a single HKDF-Expand into a caller-provided mutable buffer, and a `pub(crate)` constants module exporting `HKDF_SALT`, `HKDF_INFO_KEY_ENCRYPTION`, `HKDF_INFO_SQLCIPHER`, `HKDF_INFO_MANIFEST_BACKUP`. The existing `derive_vault_keys` is rewritten to call the helper three times into temporary `Zeroizing<[u8; 32]>` buffers, then wraps each into `SecretBox::new(Box::new(…))` — same observable behavior, same test coverage. Phase 2.2's `SessionKeys::derive` calls the same helper three times into already-locked `SecureBytes<32>` buffers, so the key bytes are only ever written into locked pages.

  Rationale vs literal sub-phase text: the sub-phase's "reuse `derive_vault_keys`" directive is the *intent* (reuse HKDF salt/info constants and SHA-256 construction) not the *letter* (call the exact function). Exposing the helper satisfies the intent without the security gap.
- **Documentation sync required on implementation**: Update `docs/architecture/designs/authentication-and-session-management/sub-phases/2.2-argon2id-and-session-keys.md` deliverable 2 (line 17) to clarify that integration is via the shared HKDF helper / shared constants in `crypto::hkdf`, not by calling `derive_vault_keys` directly.

### DC-3 — Sub-phase test list demands `InvalidCredentials` tests that the Phase 2.2 KDF wrapper alone cannot produce

- **Concern**: 2.2 sub-phase deliverable 8, test bullets 3–5 require tests named "Wrong password (Tier 1) → `InvalidCredentials`", "Wrong password (Tier 2) → `InvalidCredentials`", "Wrong key file bytes (Tier 2) → `InvalidCredentials`". The Phase 2.2 KDF wrapper does not verify credentials — it runs Argon2id and returns 32 bytes. "Wrong" is only detectable by a downstream step (e.g., decrypting a vault-header probe, Phase 2.4). In isolation, 2.2 cannot raise `InvalidCredentials` from the KDF path without pulling in vault-header logic from 2.4.
- **Source**: 2.2 sub-phase doc deliverable 8, bullets 3–5 (lines 26–28), contradicting deliverables 8, bullets 8–10 (lines 31–33) which re-express the same checks as "different inputs produce different `key_encryption_key` values".
- **Impact**: Codex would either (a) couple 2.2 to 2.4's vault header format prematurely, or (b) write a non-executing test, or (c) silently merge tests 3–5 into tests 8–10.
- **Classification**: Non-blocking.
- **Resolution**: For Phase 2.2, tests 3–5 are replaced by the equivalent "different credentials produce different `SessionKeys` fields" tests (already listed as deliverable 8 bullets 8–10). The `AuthenticationError::InvalidCredentials` variant is defined in Phase 2.2 (deliverable 6) but is not raised by the KDF wrapper in Phase 2.2 — it remains available for Phase 2.4 (vault ceremonies) to return after vault-header probe decryption fails. One Phase 2.2 test constructs the variant directly and asserts its `Display` output equals `"authentication failed"` (per design.md line 280) to exercise the variant without simulating a credential check.
- **Documentation sync required on implementation**: Update `docs/architecture/designs/authentication-and-session-management/sub-phases/2.2-argon2id-and-session-keys.md` deliverable 8 (lines 23–35): remove bullets 3–5 or add a note that `InvalidCredentials` is raised by Phase 2.4's vault-header probe, not by Phase 2.2's KDF. Add a bullet for "construct variant → assert Display output" coverage.

### DC-4 — `AuthError` vs `AuthenticationError` enum name mismatch

- **Concern**: Phase 2.1 introduced `pub enum AuthError` in `src-tauri/src/auth/error.rs` (single variant `KeySource(KeySourceError)`). The parent design and Phase 2.2 sub-phase both call the enum `AuthenticationError`. `CLAUDE.md` forbids abbreviations: "`chunk_index` not `chunk_idx`". `AuthError` is an abbreviation of `AuthenticationError`.
- **Source**: `src-tauri/src/auth/error.rs` line 10; `docs/architecture/designs/authentication-and-session-management/design.md` line 278; 2.2 sub-phase doc deliverable 6 (line 21); `CLAUDE.md` `## Naming` section.
- **Impact**: Keeping `AuthError` diverges from the canonical design and from the sub-phase spec. Renaming is a local refactor (one type, one test using it, no external callers re-exporting it — see `src-tauri/src/auth/mod.rs` which re-exports `KeySourceError` only).
- **Classification**: Non-blocking.
- **Resolution**: Rename `AuthError` → `AuthenticationError` in `src-tauri/src/auth/error.rs`. Add the Phase 2.2 variants. Update the single existing test (`test_auth_error_from_key_source_converts_variant`) and rename to `test_authentication_error_from_key_source_converts_variant` to match the new type name. `src-tauri/src/auth/mod.rs` does not currently re-export `AuthError`, so no `pub use` churn beyond the new `pub use error::AuthenticationError;`.
- **Documentation sync required on implementation**: `docs/architecture/designs/tauri-ipc-and-frontend/sub-phases/roadmap.md` line 53 currently reads "`From` impls for `AuthError`, `StorageError`, `SyncError`". Update to `AuthenticationError`. This is a forward-looking doc fix — the Phase 6.1 plan has not been written yet, so no plan file churn.

### DC-5 — How to inject mlock failures in tests

- **Concern**: 2.2 sub-phase deliverable 8 bullet 6 requires a test that exercises "`mlock` failure → `MemoryLockFailed` (simulate by exhausting lock quota in test harness)". Actually exhausting the lock quota is platform-specific, flaky in CI, and can interfere with other tests running on the same machine. The sub-phase does not name a concrete mechanism.
- **Source**: 2.2 sub-phase doc deliverable 8 bullet 6 (line 29).
- **Impact**: Without a deterministic mechanism, the test is either brittle (real quota exhaustion) or missing.
- **Classification**: Non-blocking.
- **Resolution**: Introduce a test-only fault-injection hook inside the `memory::platform` submodule:

  ```rust
  #[cfg(test)]
  thread_local! {
      static FORCE_LOCK_FAILURE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
  }

  #[cfg(test)]
  pub(crate) fn set_force_lock_failure(value: bool) {
      FORCE_LOCK_FAILURE.with(|cell| cell.set(value));
  }

  pub(crate) fn lock_memory(pointer: *mut c_void, length: usize) -> Result<(), MemoryLockError> {
      #[cfg(test)]
      if FORCE_LOCK_FAILURE.with(|cell| cell.get()) {
          return Err(MemoryLockError::PlatformFailure {
              platform_message: String::from("forced-failure for tests"),
          });
      }
      // platform-specific path below…
  }
  ```

  The `thread_local` ensures tests running in parallel do not cross-contaminate. Tests call `set_force_lock_failure(true)` before constructing `SecureBytes`, assert the construction fails, then set it back to `false`. No real lock quota is touched.
- **Documentation sync required on implementation**: None — this is an implementation/test detail outside the sub-phase text.

### DC-6 — Argon2id `Params::new` can reject OWASP-floor inputs; design invariant #9 mandates accepting them during bootstrap

- **Concern**: The `argon2 = "0.5"` crate's `Params::new(m_cost, t_cost, p_cost, Some(32))` validates m/t/p against internal minimums. OWASP floor `m=19456, t=2, p=1` is permitted by `argon2 0.5` (well above its hard floor of `m=8, t=1, p=1`), so Phase 2.2 does not need special casing. But design invariant #9 says existing vaults with stored params must exactly match local trusted cache — that validation belongs to Phase 2.4, not 2.2.
- **Source**: `docs/architecture/design-invariants.md` lines 69–75; sub-phase deliverable 1 (line 12).
- **Impact**: If Phase 2.2's KDF wrapper silently coerces params to defaults, Phase 2.4 cannot implement bootstrap validation — invariant #9 would be violated.
- **Classification**: Non-blocking.
- **Resolution**: Phase 2.2's KDF wrapper accepts `Argon2Params` as a parameter and passes it verbatim to `argon2::Params::new`. The wrapper does NOT clamp, default, or validate beyond what `argon2::Params::new` natively does. Bootstrap validation is Phase 2.4's responsibility. Add a doc-comment on `derive_master_key` explicitly stating this delegation.
- **Documentation sync required on implementation**: None.

### DC-7 — `mlock` on macOS is POSIX but the roadmap's risk table says macOS has no `mlock` caveat

- **Concern**: Design.md line 221 lists a macOS-specific mlock failure message, implying macOS support. But the sub-phase deliverables only enumerate Linux (`mlock`) and Windows (`VirtualLock`) — macOS is absent from the deliverable text.
- **Source**: 2.2 sub-phase doc deliverable 4 (line 19) vs `design.md` line 221 (macOS error message).
- **Impact**: `CLAUDE.md` "Platform compatibility" rule requires all three targets. If Codex skips macOS, the build breaks on Darwin.
- **Classification**: Non-blocking.
- **Resolution**: macOS uses POSIX `libc::mlock` / `libc::munlock` — identical call signatures to Linux. A single `src-tauri/src/memory/platform/unix.rs` module gated with `#[cfg(unix)]` covers both Linux and macOS. The macOS error message (design.md line 221, "Cannot lock memory. Ensure sufficient physical RAM is available and try again.") is returned only when compiled for macOS via a `#[cfg(target_os = "macos")]` branch inside the error-constructor helper. Linux uses the `ulimit -l` message. No separate `macos.rs` module is needed.
- **Documentation sync required on implementation**: Update `docs/architecture/designs/authentication-and-session-management/sub-phases/2.2-argon2id-and-session-keys.md` Implementation Notes bullet (line 76) to explicitly cover macOS via POSIX `libc::mlock`, closing the gap.

### DC-8 — Governance: `.claude/rules/auth.md` / `.claude/rules/memory-protection.md` do not mention `SessionKeys` or `SecureBytes`

- **Concern**: After Phase 2.2 lands, the canonical type for session keys is `SessionKeys` (and internally `SecureBytes<32>`). `.claude/rules/auth.md` mentions "session keys in mlocked memory" but not the `SessionKeys` struct. `.claude/rules/memory-protection.md` mentions `SecureBuffer` as an example name but 2.2 will introduce `SecureBytes<const N>`. Both rules are informative, not contradictory, but the names will be stale once 2.2 merges.
- **Source**: `.claude/rules/auth.md` lines 28–31, `.claude/rules/memory-protection.md` lines 12–14.
- **Impact**: Low drift — future contributors reading the rules would search for `SecureBuffer` and find nothing, or expect `mlock` to be implemented elsewhere. Rule mirrors must be regenerated via `/copilot-sync`.
- **Classification**: Non-blocking.
- **Resolution**: Governance sync action GS-1 and GS-2 (Section 9) update both rules and regenerate the `.github/instructions/*.instructions.md` mirrors.
- **Documentation sync required on implementation**: covered by Section 9 actions.

### DC-9 — `secrecy 0.10`'s `SecretBox` is not the return type for `expose` on `SecureBytes`; API shape needs to be explicit

- **Concern**: `secrecy 0.10` provides `ExposeSecret::expose_secret(&self) -> &T`. If `SecureBytes<32>` wants to mimic that, it needs its own `expose` inherent method. It should not `impl ExposeSecret for SecureBytes` because `secrecy`'s trait is sealed in some versions. Sub-phase does not specify.
- **Source**: 2.2 sub-phase doc deliverable 3 (line 18) — field type was `SecretBox`, so `ExposeSecret` came for free.
- **Classification**: Non-blocking.
- **Resolution**: `SecureBytes<const N: usize>` exposes an inherent `pub(crate) fn expose(&self) -> &[u8; N]` returning a borrowed reference. This matches the existing convention in `src-tauri/src/crypto/types/mod.rs` (e.g., `KeyEncryptionKey::expose`, line 30). Do not implement `ExposeSecret` — keep the API surface inherent.
- **Documentation sync required on implementation**: None.

## 4. Assumptions

Every non-obvious fact Codex would otherwise guess at. If any is wrong, correct it before handoff.

1. **Module layout**: New files land at absolute paths `C:\Users\chris\source\repos\arx-runa\src-tauri\src\auth\kdf.rs`, `C:\Users\chris\source\repos\arx-runa\src-tauri\src\auth\session.rs`, `C:\Users\chris\source\repos\arx-runa\src-tauri\src\memory\secure_buffer.rs`, `C:\Users\chris\source\repos\arx-runa\src-tauri\src\memory\platform\mod.rs`, `C:\Users\chris\source\repos\arx-runa\src-tauri\src\memory\platform\unix.rs`, `C:\Users\chris\source\repos\arx-runa\src-tauri\src\memory\platform\windows.rs`.
2. **Argon2id crate entry points**: `argon2 = "0.5"`, imported as `use argon2::{Algorithm, Argon2, Params, Version};`. The wrapper calls `Argon2::new(Algorithm::Argon2id, Version::V0x13, params).hash_password_into(password_bytes, salt, &mut output_buffer)?`. `hash_password_into` writes the raw derivation bytes into a caller-provided `&mut [u8; 32]`, which is exactly what we need to land directly into locked memory.
3. **`Argon2Params` newtype**: defined in `src-tauri/src/auth/kdf.rs` as `pub struct Argon2Params { pub memory_cost_kib: u32, pub time_cost: u32, pub parallelism: u32 }` with an associated `DEFAULT` constant `{ memory_cost_kib: 65536, time_cost: 3, parallelism: 4 }` matching design.md line 154 and invariant #9. Conversion to `argon2::Params`: `Params::new(self.memory_cost_kib, self.time_cost, self.parallelism, Some(32))`.
4. **KDF wrapper signature**:

   ```rust
   pub(crate) fn derive_master_key_into(
       password_utf8_bytes: &[u8],
       key_file_bytes: Option<&[u8; 32]>,
       salt: &[u8; 32],
       params: &Argon2Params,
       output: &mut [u8; 32],
   ) -> Result<(), AuthenticationError>;
   ```

   Writes the 32-byte `master_key` directly into `output` so the caller can place `output` inside a locked `Zeroizing<[u8; 32]>`. The function does not allocate, does not log, does not return owned key material. Implementation builds the Argon2id input by concatenating `password_utf8_bytes || key_file_bytes.unwrap_or(&[])` into a single `Zeroizing<Vec<u8>>` whose capacity is exactly `password_utf8_bytes.len() + 32` (Tier 2) or `password_utf8_bytes.len()` (Tier 1), then calls `Argon2::hash_password_into`.
5. **`SessionKeys::derive` signature**:

   ```rust
   pub(crate) fn derive(
       password_utf8_bytes: &[u8],
       key_file_bytes: Option<&[u8; 32]>,
       salt: &[u8; 32],
       params: &Argon2Params,
   ) -> Result<SessionKeys, AuthenticationError>;
   ```

   Step-by-step:
   1. Allocate `key_encryption_key = SecureBytes::<32>::new()?`, `sqlcipher_key = SecureBytes::<32>::new()?`, `manifest_key = SecureBytes::<32>::new()?`. Each `::new()` allocates and immediately mlocks before any write — on any failure, already-constructed `SecureBytes` Drop impls run, unlocking and zeroing partial allocations.
   2. Allocate `let mut master_key = Zeroizing::new([0u8; 32]);` on stack (cheap; Drop zeroes).
   3. Call `derive_master_key_into(password_utf8_bytes, key_file_bytes, salt, params, &mut master_key)?`. If the call fails, `master_key` is zeroed by Drop and the three `SecureBytes` are zeroed + unlocked by Drop.
   4. Call `crypto::hkdf::expand_vault_key_into(&master_key, HKDF_INFO_KEY_ENCRYPTION, key_encryption_key.as_mut())?` — this writes HKDF output directly into the locked buffer.
   5. Repeat for `sqlcipher_key` and `manifest_key`.
   6. `drop(master_key);` — explicit, although Drop would fire at end of scope anyway. Makes the zeroization point visible in source.
   7. Return `Ok(SessionKeys { key_encryption_key, sqlcipher_key, manifest_key })`.

   `master_key` never appears as a field of any struct, never leaves this function, and is zeroed by `Zeroizing` drop on every exit path (success, HKDF error, panic).
6. **`SecureBytes<const N: usize>` public API** (in `src-tauri/src/memory/secure_buffer.rs`):

   ```rust
   #[derive(ZeroizeOnDrop)]
   pub(crate) struct SecureBytes<const N: usize> {
       #[zeroize(skip)] locked_pointer: *mut u8,
       buffer: Box<[u8; N]>,
   }

   impl<const N: usize> SecureBytes<N> {
       pub(crate) fn new() -> Result<Self, MemoryLockError>;
       pub(crate) fn as_mut(&mut self) -> &mut [u8; N];
       pub(crate) fn expose(&self) -> &[u8; N];
   }

   impl<const N: usize> Drop for SecureBytes<N> {
       fn drop(&mut self) {
           // zeroize happens here explicitly before unlock (do not rely on derive only —
           // derived ZeroizeOnDrop runs first, then this Drop, but unlock must be last)
           use zeroize::Zeroize;
           self.buffer.as_mut().zeroize();
           // SAFETY: `locked_pointer` was obtained from `buffer.as_mut_ptr()` in `new`,
           // the allocation is still live (we own the Box), and the length matches `N`.
           unsafe { crate::memory::platform::unlock_memory(self.locked_pointer, N); }
       }
   }
   ```

   Note: `#[derive(ZeroizeOnDrop)]` + manual `Drop` cannot coexist. The correct form is manual `Drop` that performs zeroize + unlock in sequence, and `SecureBytes` implements `Zeroize` via `{ self.buffer.as_mut().zeroize(); }` so parent structs deriving `ZeroizeOnDrop` can still call it. Section 5.4 has the final form.
7. **`SessionKeys` derive block**: `#[derive(Zeroize, ZeroizeOnDrop)]` on `SessionKeys` would attempt to call `.zeroize()` recursively, which requires `SecureBytes<32>: Zeroize`. `SecureBytes<N>` provides an explicit `impl<const N: usize> Zeroize for SecureBytes<N>` that calls `self.buffer.as_mut().zeroize()`. `SessionKeys` then has `#[derive(ZeroizeOnDrop)]` which auto-derives a `Drop` calling `Zeroize::zeroize`. When Rust drops the struct after the derived `Drop`, each field's custom `Drop` (zeroize + unlock) runs. Double-zeroize is safe and cheap.

   However, if that double-Drop chain turns out to conflict with `zeroize-derive`'s emitted code (the derive may generate `impl Drop` which forbids a manual `Drop` on the field type — no, it forbids manual `Drop` on the *same* type, not on field types), fall back to: no `ZeroizeOnDrop` on `SessionKeys`; rely on field-level Drop chains for zeroization. Codex should choose whichever form compiles cleanly; both are equivalent at runtime.
8. **`platform::lock_memory` / `platform::unlock_memory` signatures**:

   ```rust
   // src-tauri/src/memory/platform/mod.rs
   use crate::memory::error::MemoryLockError;

   #[cfg(unix)]
   pub(crate) use super::platform::unix::{lock_memory, unlock_memory};

   #[cfg(windows)]
   pub(crate) use super::platform::windows::{lock_memory, unlock_memory};

   // unix.rs
   /// # Safety
   /// `pointer` must be the start of a live allocation of at least `length` bytes.
   pub(crate) unsafe fn lock_memory(pointer: *mut u8, length: usize) -> Result<(), MemoryLockError>;

   /// # Safety
   /// `pointer` / `length` must match a prior successful `lock_memory` call.
   pub(crate) unsafe fn unlock_memory(pointer: *mut u8, length: usize);
   ```

   Unix impl calls `libc::mlock(pointer as *const c_void, length)`; on non-zero return, builds `MemoryLockError::PlatformFailure` with the design-specified message. `unlock_memory` calls `libc::munlock` and ignores the return (best-effort — the allocation is about to be freed anyway).

   Windows impl calls `windows::Win32::System::Memory::VirtualLock(VA, length)` and `VirtualUnlock(VA, length)`. The `VA` comes from `windows::Win32::Foundation::VirtualLock` — actually `VirtualLock` lives in `Win32_System_Memory` and takes `*mut c_void`. Uses `GetLastError` to populate the platform message if `VirtualLock` returns zero.
9. **Platform error message strings** (match design.md lines 219–221 verbatim — these are user-facing and must not drift):
   - Linux: `"Cannot lock memory. Increase the memory lock limit: `ulimit -l unlimited` or edit `/etc/security/limits.conf`."`
   - Windows: `"Cannot lock session keys in memory (system working set quota exceeded). Try closing other applications or restarting Arx Runa."`
   - macOS: `"Cannot lock memory. Ensure sufficient physical RAM is available and try again."`

   The Linux message contains backticks; they land inside a Rust string literal with `\`` if needed (Rust raw string `r"…"` is simpler since there are no `"` characters in the Linux string).
10. **`MemoryLockError` variants** (in `src-tauri/src/memory/error.rs`):

    ```rust
    #[non_exhaustive]
    #[derive(Debug, Error)]
    pub enum MemoryLockError {
        /// The OS refused to lock the buffer into physical memory.
        #[error("{platform_message}")]
        PlatformFailure { platform_message: String },
    }
    ```

    The `Display` impl returns only `platform_message` so the Tauri IPC layer can forward the message verbatim without further sanitisation. No file paths, pointers, or sizes are leaked.
11. **`AuthenticationError` → `MemoryLockError` mapping**: `SessionKeys::derive` converts `MemoryLockError` into `AuthenticationError::MemoryLockFailed(String)` where `String` is the `platform_message` field. Implemented via `impl From<MemoryLockError> for AuthenticationError`.
12. **HKDF helper reuse**: `crypto::hkdf::expand_vault_key_into(master_key: &[u8; 32], info: &[u8], output: &mut [u8; 32]) -> Result<(), CryptoError>` is new public-`pub(crate)` surface for auth. `derive_vault_keys` is refactored to call it internally. Tests from Phase 1.1 continue to pass unchanged.
13. **`AuthenticationError::VaultHeaderInvalid` is added in Phase 2.2 but not raised** — reserved for Phase 2.4, analogous to `InvalidCredentials` (see DC-3). A unit test constructs the variant and asserts its `Display` output to exercise the enum surface.
14. **`libc` crate version**: `libc = "0.2"`, `[target.'cfg(unix)'.dependencies]`. No features. `libc::mlock` / `libc::munlock` are available on all supported Unix targets.
15. **`windows` crate feature addition**: Extend the existing `windows` dependency to `features = ["Win32_Storage_FileSystem", "Win32_Foundation", "Win32_System_Memory"]`. `Win32_System_Memory` is what exposes `VirtualLock`, `VirtualUnlock`, and `GetLastError`-adjacent memory APIs.
16. **Tests placement**: unit tests live in `#[cfg(test)] mod tests` blocks inside each `.rs` file, consistent with the project convention.
17. **No Tauri command is wired up in this sub-phase** — the IPC surface for `authenticate` lands in Phase 6.1.
18. **No frontend / IPC event changes in Phase 2.2** — the 60-second pre-warning event is Phase 2.3's concern.
19. **`KeySource` is NOT read inside the KDF wrapper.** Callers of `SessionKeys::derive` (Phase 2.4 and, temporarily, tests) call `key_source.read_key()?` to obtain `Zeroizing<[u8; 32]>` and pass `Some(&*zeroizing_bytes)` as `key_file_bytes`. This keeps `kdf.rs` independent of `auth::key_source` and makes the function trivial to test with plain byte literals.
20. **`clippy::needless_pass_by_ref_mut` and `clippy::unnecessary_safety_doc`** — new `unsafe` blocks in `platform/unix.rs` and `platform/windows.rs` carry `// SAFETY:` comments sufficient to satisfy the project's `-D warnings` gate.
21. **Test vectors for Argon2id determinism**: two fixed inputs run through the wrapper produce byte-identical outputs across runs (KDF determinism assertion). Values are computed on first run of the test and hard-coded as expected bytes to catch silent algorithm drift. No test depends on any live entropy.

## 5. Approach

All file paths absolute. Each step states types and signatures verbatim.

### 5.1 Add dependencies

**File**: `C:\Users\chris\source\repos\arx-runa\src-tauri\Cargo.toml`

Modify the existing `[target.'cfg(target_os = "windows")'.dependencies]` block from:

```toml
[target.'cfg(target_os = "windows")'.dependencies]
wmi = "0.14"
windows = { version = "0.59", features = ["Win32_Storage_FileSystem", "Win32_Foundation"] }
```

to:

```toml
[target.'cfg(target_os = "windows")'.dependencies]
wmi = "0.14"
windows = { version = "0.59", features = ["Win32_Storage_FileSystem", "Win32_Foundation", "Win32_System_Memory"] }
```

Add a new target block for Unix (covers Linux and macOS):

```toml
[target.'cfg(unix)'.dependencies]
libc = "0.2"
```

Placement: immediately after the existing `[target.'cfg(target_os = "macos")'.dependencies]` block. Do not remove or modify the existing `udev`, `core-foundation`, or `core-foundation-sys` entries.

Verify with `cargo check` (host platform). If possible, `cargo check --target x86_64-pc-windows-msvc` and `cargo check --target x86_64-apple-darwin` catch cross-platform regressions; otherwise rely on CI.

### 5.2 Extend the `memory` module

#### 5.2.1 Update `src-tauri/src/memory/mod.rs`

**File**: `C:\Users\chris\source\repos\arx-runa\src-tauri\src\memory\mod.rs`

Replace current body with:

```rust
//! Arx Runa memory module.
//!
//! Memory protection utilities: mlock/VirtualLock wrappers, zeroisation helpers.
//! Primary consumer: `auth` module (Phase 2) for session key memory locking.

pub mod error;
pub(crate) mod platform;
pub(crate) mod secure_buffer;
pub mod types;

pub(crate) use secure_buffer::SecureBytes;
pub use error::MemoryLockError;
```

#### 5.2.2 Update `src-tauri/src/memory/error.rs`

**File**: `C:\Users\chris\source\repos\arx-runa\src-tauri\src\memory\error.rs`

Replace current body with:

```rust
//! Error types for the memory module.

use thiserror::Error;

/// Errors produced by platform-specific memory locking operations.
#[non_exhaustive]
#[derive(Debug, Error)]
pub enum MemoryLockError {
    /// The OS refused to lock the buffer into physical memory.
    ///
    /// `platform_message` is the user-facing string defined by the
    /// authentication design (see
    /// `docs/architecture/designs/authentication-and-session-management/design.md`
    /// "Memory locking" subsection).
    #[error("{platform_message}")]
    PlatformFailure { platform_message: String },
}

#[cfg(test)]
mod tests {
    use super::MemoryLockError;

    #[test]
    fn test_memory_lock_error_display_forwards_platform_message() {
        let error = MemoryLockError::PlatformFailure {
            platform_message: String::from("Cannot lock memory. Ensure sufficient physical RAM is available and try again."),
        };

        assert_eq!(
            error.to_string(),
            "Cannot lock memory. Ensure sufficient physical RAM is available and try again."
        );
    }
}
```

#### 5.2.3 Create `src-tauri/src/memory/platform/mod.rs`

**File**: `C:\Users\chris\source\repos\arx-runa\src-tauri\src\memory\platform\mod.rs`

```rust
//! Platform-specific memory locking primitives.
//!
//! All unsafe code touching `mlock` / `VirtualLock` lives in the inner
//! submodules. This module only re-exports the two functions consumed by
//! `SecureBytes`.

#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

#[cfg(unix)]
pub(crate) use unix::{lock_memory, unlock_memory};
#[cfg(windows)]
pub(crate) use windows::{lock_memory, unlock_memory};

#[cfg(test)]
mod fault_injection {
    use std::cell::Cell;

    thread_local! {
        static FORCE_LOCK_FAILURE: Cell<bool> = const { Cell::new(false) };
    }

    /// Test-only switch: when set to `true` on the current thread, the next
    /// call to `lock_memory` returns `MemoryLockError::PlatformFailure` without
    /// invoking the real platform syscall.
    pub(crate) fn set_force_lock_failure(value: bool) {
        FORCE_LOCK_FAILURE.with(|cell| cell.set(value));
    }

    pub(crate) fn is_force_lock_failure() -> bool {
        FORCE_LOCK_FAILURE.with(|cell| cell.get())
    }
}

#[cfg(test)]
pub(crate) use fault_injection::set_force_lock_failure;
```

#### 5.2.4 Create `src-tauri/src/memory/platform/unix.rs`

**File**: `C:\Users\chris\source\repos\arx-runa\src-tauri\src\memory\platform\unix.rs`

```rust
//! POSIX `mlock` / `munlock` wrapper.
//!
//! Covers both Linux (`cfg(target_os = "linux")`) and macOS
//! (`cfg(target_os = "macos")`). `libc::mlock` / `libc::munlock` are POSIX
//! and have identical call signatures on both targets.

use std::ffi::c_void;

use crate::memory::error::MemoryLockError;

/// Locks `length` bytes starting at `pointer` into physical memory.
///
/// # Safety
/// `pointer` must be the start of a live allocation of at least `length`
/// bytes. The caller retains ownership of the allocation for the lifetime
/// of the lock.
pub(crate) unsafe fn lock_memory(pointer: *mut u8, length: usize) -> Result<(), MemoryLockError> {
    #[cfg(test)]
    if super::fault_injection::is_force_lock_failure() {
        return Err(MemoryLockError::PlatformFailure {
            platform_message: platform_failure_message(),
        });
    }

    // SAFETY: caller guarantees `pointer` starts a live allocation of at least
    // `length` bytes; `libc::mlock` only reads the page table entries for the
    // range and does not dereference the pointer as data.
    let result = unsafe { libc::mlock(pointer as *const c_void, length) };
    if result == 0 {
        Ok(())
    } else {
        Err(MemoryLockError::PlatformFailure {
            platform_message: platform_failure_message(),
        })
    }
}

/// Unlocks `length` bytes starting at `pointer`.
///
/// # Safety
/// `pointer` and `length` must match a prior successful `lock_memory` call.
pub(crate) unsafe fn unlock_memory(pointer: *mut u8, length: usize) {
    // SAFETY: caller guarantees this matches a prior successful `mlock` call;
    // `libc::munlock` does not dereference the pointer as data. Best-effort —
    // the allocation is about to be freed regardless.
    let _ = unsafe { libc::munlock(pointer as *const c_void, length) };
}

#[cfg(target_os = "linux")]
fn platform_failure_message() -> String {
    String::from(
        "Cannot lock memory. Increase the memory lock limit: `ulimit -l unlimited` or edit `/etc/security/limits.conf`.",
    )
}

#[cfg(target_os = "macos")]
fn platform_failure_message() -> String {
    String::from(
        "Cannot lock memory. Ensure sufficient physical RAM is available and try again.",
    )
}

#[cfg(all(unix, not(any(target_os = "linux", target_os = "macos"))))]
fn platform_failure_message() -> String {
    String::from("Cannot lock memory.")
}
```

#### 5.2.5 Create `src-tauri/src/memory/platform/windows.rs`

**File**: `C:\Users\chris\source\repos\arx-runa\src-tauri\src\memory\platform\windows.rs`

```rust
//! Windows `VirtualLock` / `VirtualUnlock` wrapper.

use std::ffi::c_void;

use windows::Win32::System::Memory::{VirtualLock, VirtualUnlock};

use crate::memory::error::MemoryLockError;

/// Locks `length` bytes starting at `pointer` into physical memory via
/// `VirtualLock`.
///
/// # Safety
/// `pointer` must be the start of a live allocation of at least `length`
/// bytes.
pub(crate) unsafe fn lock_memory(pointer: *mut u8, length: usize) -> Result<(), MemoryLockError> {
    #[cfg(test)]
    if super::fault_injection::is_force_lock_failure() {
        return Err(MemoryLockError::PlatformFailure {
            platform_message: platform_failure_message(),
        });
    }

    // SAFETY: caller guarantees `pointer` starts a live allocation of at
    // least `length` bytes. `VirtualLock` takes a `*const c_void` and does
    // not dereference it as data.
    let result = unsafe { VirtualLock(pointer as *const c_void, length) };
    if result.as_bool() {
        Ok(())
    } else {
        Err(MemoryLockError::PlatformFailure {
            platform_message: platform_failure_message(),
        })
    }
}

/// Unlocks `length` bytes starting at `pointer`.
///
/// # Safety
/// `pointer` and `length` must match a prior successful `lock_memory` call.
pub(crate) unsafe fn unlock_memory(pointer: *mut u8, length: usize) {
    // SAFETY: caller guarantees this matches a prior successful
    // `VirtualLock`. Best-effort — the allocation is about to be freed.
    let _ = unsafe { VirtualUnlock(pointer as *const c_void, length) };
}

fn platform_failure_message() -> String {
    String::from(
        "Cannot lock session keys in memory (system working set quota exceeded). Try closing other applications or restarting Arx Runa.",
    )
}
```

Note on `VirtualLock` return type: the `windows` crate maps Win32 `BOOL` to `windows::core::BOOL`. `BOOL::as_bool()` returns the native `bool`. If the exact API name differs in `windows = "0.59"` (some versions return `Result<()>` directly), fall back to `VirtualLock(...).ok()` / `.is_ok()`. Codex should try `BOOL::as_bool()` first and swap if the compiler disagrees — the semantic is identical.

#### 5.2.6 Create `src-tauri/src/memory/secure_buffer.rs`

**File**: `C:\Users\chris\source\repos\arx-runa\src-tauri\src\memory\secure_buffer.rs`

```rust
//! Memory-locked, zero-on-drop byte buffer.
//!
//! `SecureBytes<N>` is the canonical container for Arx Runa's session-key
//! bytes. Every instance:
//! - allocates a heap `Box<[u8; N]>` in zero state,
//! - locks its pages via platform `mlock` / `VirtualLock` before any caller
//!   writes secret material,
//! - zeroes the buffer on drop, then unlocks the pages, then frees the
//!   allocation.

use zeroize::Zeroize;

use crate::memory::error::MemoryLockError;
use crate::memory::platform;

/// Fixed-size byte buffer whose backing pages are locked into physical
/// memory and whose contents are zeroed on drop.
pub(crate) struct SecureBytes<const N: usize> {
    buffer: Box<[u8; N]>,
}

impl<const N: usize> SecureBytes<N> {
    /// Allocates a zero-initialized `Box<[u8; N]>`, locks its pages, and
    /// returns the wrapper. Returns `MemoryLockError` if platform locking
    /// fails — the partial allocation is dropped and the zero-initialized
    /// bytes are zeroed on the error path (they are already zero, so this
    /// is effectively a no-op, but stays correct if `N` changes).
    pub(crate) fn new() -> Result<Self, MemoryLockError> {
        let mut buffer: Box<[u8; N]> = Box::new([0u8; N]);
        // SAFETY: `buffer.as_mut_ptr()` returns a valid, aligned pointer to
        // exactly `N` bytes owned by this function. If `lock_memory` fails,
        // the `Box` is dropped below and its pages are not locked.
        unsafe { platform::lock_memory(buffer.as_mut_ptr(), N) }?;
        Ok(Self { buffer })
    }

    /// Returns a mutable reference to the locked buffer so callers can
    /// write key material directly (e.g., HKDF expand output).
    pub(crate) fn as_mut(&mut self) -> &mut [u8; N] {
        &mut self.buffer
    }

    /// Returns a read-only view of the locked buffer.
    pub(crate) fn expose(&self) -> &[u8; N] {
        &self.buffer
    }
}

impl<const N: usize> Zeroize for SecureBytes<N> {
    fn zeroize(&mut self) {
        self.buffer.as_mut().zeroize();
    }
}

impl<const N: usize> Drop for SecureBytes<N> {
    fn drop(&mut self) {
        self.buffer.as_mut().zeroize();
        // SAFETY: `buffer.as_mut_ptr()` matches the pointer/length previously
        // passed to `lock_memory` in `new`. The allocation is still live (we
        // own the `Box`) until this function returns.
        unsafe { platform::unlock_memory(self.buffer.as_mut_ptr(), N); }
    }
}

#[cfg(test)]
mod tests {
    use super::SecureBytes;
    use crate::memory::platform::set_force_lock_failure;

    #[test]
    fn test_secure_bytes_new_zero_initializes_buffer() {
        let buffer = SecureBytes::<32>::new().expect("lock should succeed");
        assert_eq!(*buffer.expose(), [0u8; 32]);
    }

    #[test]
    fn test_secure_bytes_as_mut_writes_survive() {
        let mut buffer = SecureBytes::<32>::new().expect("lock should succeed");
        buffer.as_mut().copy_from_slice(&[0xABu8; 32]);
        assert_eq!(*buffer.expose(), [0xABu8; 32]);
    }

    #[test]
    fn test_secure_bytes_drop_zeroizes_contents() {
        let mut buffer = SecureBytes::<32>::new().expect("lock should succeed");
        buffer.as_mut().copy_from_slice(&[0xCDu8; 32]);
        let pointer = buffer.expose().as_ptr();
        drop(buffer);
        // SAFETY: `pointer` came from a just-dropped Box. The allocation has
        // been freed, so reading the memory is undefined behavior in the
        // strict sense — we rely on the `Drop` impl zeroing the bytes before
        // the deallocation returns the page to the allocator. A stricter
        // variant of this test uses a hand-rolled `ManuallyDrop<Box<...>>`
        // to keep the allocation alive for the read.
        //
        // Cleaner alternative used in the project convention (see
        // `src-tauri/src/crypto/types/mod.rs` `test_file_key_zeroize_trait_clears_memory`):
        // do not drop the buffer; call `Zeroize::zeroize` directly and
        // inspect the pointer while the allocation is still alive.
        let _ = pointer;
    }

    #[test]
    fn test_secure_bytes_zeroize_trait_clears_buffer_in_place() {
        use zeroize::Zeroize;
        let mut buffer = SecureBytes::<32>::new().expect("lock should succeed");
        buffer.as_mut().copy_from_slice(&[0xEFu8; 32]);
        let pointer = buffer.expose().as_ptr();

        // SAFETY: `pointer` comes from `buffer.expose()` and `buffer` is
        // still alive for the read. The allocation is not freed.
        let before = unsafe { std::slice::from_raw_parts(pointer, 32) };
        assert_eq!(before, &[0xEFu8; 32]);

        Zeroize::zeroize(&mut buffer);

        // SAFETY: same allocation, same length, buffer still alive.
        let after = unsafe { std::slice::from_raw_parts(pointer, 32) };
        assert_eq!(after, &[0u8; 32]);
    }

    #[test]
    fn test_secure_bytes_new_returns_platform_failure_when_lock_is_forced_to_fail() {
        set_force_lock_failure(true);
        let result = SecureBytes::<32>::new();
        set_force_lock_failure(false);

        let error = result.expect_err("forced lock failure should propagate");
        let crate::memory::error::MemoryLockError::PlatformFailure { platform_message } = error;
        assert!(!platform_message.is_empty());
    }
}
```

The "drop after capture" test is noted as convention-aligned with `test_file_key_zeroize_trait_clears_memory` — the preferred form is `Zeroize::zeroize(&mut buffer)` while holding the buffer, then read through the still-live pointer. Codex should implement the `test_secure_bytes_zeroize_trait_clears_buffer_in_place` form and skip the unsound "drop then read" variant.

#### 5.2.7 Update `src-tauri/src/memory/types/mod.rs`

No change — types subfolder is reserved for future newtype additions (e.g., an `Argon2Input` newtype if Phase 2.4 introduces one). Keep the existing module comment.

### 5.3 Extend the `auth` module

#### 5.3.1 Rename `AuthError` → `AuthenticationError` and add variants in `src-tauri/src/auth/error.rs`

**File**: `C:\Users\chris\source\repos\arx-runa\src-tauri\src\auth\error.rs`

Replace current body with:

```rust
//! Error types for the auth module.

use std::io;

use thiserror::Error;

use crate::memory::MemoryLockError;

/// Errors produced by the auth module.
#[non_exhaustive]
#[derive(Debug, Error)]
pub enum AuthenticationError {
    /// Authentication failed. Returned for wrong password, wrong key file,
    /// or both — callers cannot distinguish the cases.
    #[error("authentication failed")]
    InvalidCredentials,

    /// No 32-byte file on the mounted volume matched the vault header's
    /// BLAKE3 fingerprint. Does not reveal whether the password would have
    /// been correct.
    #[error("key file not found on the device")]
    KeyFileNotFound,

    /// Memory locking (`mlock` / `VirtualLock`) failed. The inner string is
    /// the user-facing message defined by
    /// `docs/architecture/designs/authentication-and-session-management/design.md`.
    #[error("cannot lock memory for session keys")]
    MemoryLockFailed(String),

    /// The vault header was missing, malformed, or failed integrity checks.
    #[error("vault header is missing or corrupt")]
    VaultHeaderInvalid,

    /// A key-source operation failed.
    #[error(transparent)]
    KeySource(#[from] KeySourceError),
}

impl From<MemoryLockError> for AuthenticationError {
    fn from(error: MemoryLockError) -> Self {
        let MemoryLockError::PlatformFailure { platform_message } = error;
        Self::MemoryLockFailed(platform_message)
    }
}

/// Errors produced by a [`crate::auth::KeySource`] implementation.
#[non_exhaustive]
#[derive(Debug, Error)]
pub enum KeySourceError {
    /// The configured key-file path does not exist.
    #[error("key file not found")]
    NotFound,

    /// The file exists but is not exactly 32 bytes.
    #[error("key file has invalid size: {actual} bytes (expected 32)")]
    InvalidSize { actual: usize },

    /// An unrecoverable I/O error occurred while accessing key material or hints.
    #[error("I/O operation failed")]
    IoFailed(#[source] io::Error),
}

#[cfg(test)]
mod tests {
    use super::{AuthenticationError, KeySourceError};
    use crate::memory::MemoryLockError;

    #[test]
    fn test_authentication_error_from_key_source_converts_variant() {
        let error = AuthenticationError::from(KeySourceError::NotFound);

        let AuthenticationError::KeySource(KeySourceError::NotFound) = error else {
            panic!("expected key source wrapper");
        };
    }

    #[test]
    fn test_authentication_error_from_memory_lock_error_carries_platform_message() {
        let expected = String::from(
            "Cannot lock memory. Ensure sufficient physical RAM is available and try again.",
        );
        let error = AuthenticationError::from(MemoryLockError::PlatformFailure {
            platform_message: expected.clone(),
        });

        let AuthenticationError::MemoryLockFailed(actual) = error else {
            panic!("expected memory-lock wrapper");
        };
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_authentication_error_invalid_credentials_display_matches_design() {
        assert_eq!(
            AuthenticationError::InvalidCredentials.to_string(),
            "authentication failed",
        );
    }

    #[test]
    fn test_authentication_error_key_file_not_found_display_matches_design() {
        assert_eq!(
            AuthenticationError::KeyFileNotFound.to_string(),
            "key file not found on the device",
        );
    }

    #[test]
    fn test_authentication_error_vault_header_invalid_display_matches_design() {
        assert_eq!(
            AuthenticationError::VaultHeaderInvalid.to_string(),
            "vault header is missing or corrupt",
        );
    }

    #[test]
    fn test_authentication_error_memory_lock_failed_display_matches_design() {
        let error = AuthenticationError::MemoryLockFailed(String::from(
            "Cannot lock memory. Ensure sufficient physical RAM is available and try again.",
        ));
        assert_eq!(error.to_string(), "cannot lock memory for session keys");
    }
}
```

Note: `thiserror`'s `#[error("cannot lock memory for session keys")]` emits a fixed Display string; the `String` payload is available via `Debug` and programmatic access, not via `Display`. The design (design.md line 285) uses the same short `Display` for `MemoryLockFailed`. The full platform message is surfaced to the user via Phase 6.1's IPC sanitisation layer.

#### 5.3.2 Update `src-tauri/src/auth/mod.rs`

**File**: `C:\Users\chris\source\repos\arx-runa\src-tauri\src\auth\mod.rs`

Replace current body with:

```rust
//! Arx Runa auth module.
//!
//! Authentication and session management: Argon2id KDF, USB key file, session
//! lifecycle, memory locking.

pub mod autodetect;
pub mod device_monitor;
pub mod error;
pub mod kdf;
pub mod key_source;
pub mod path_hint;
pub mod session;
pub mod types;

pub use autodetect::find_key_file;
pub use device_monitor::{DeviceEvent, DeviceMonitor};
pub use error::{AuthenticationError, KeySourceError};
pub use kdf::Argon2Params;
pub use key_source::{FileKeySource, KeySource};
pub use path_hint::{KeyHintStore, VaultHint};
pub(crate) use session::SessionKeys;

#[cfg(any(test, feature = "test-utils"))]
pub use device_monitor::MockDeviceMonitor;
#[cfg(any(test, feature = "test-utils"))]
pub use key_source::MockKeySource;

#[cfg(test)]
mod integration_tests {
    // existing Phase 2.1 test block — do not modify
    use std::sync::Arc;

    use tokio_stream::StreamExt;

    use super::{DeviceEvent, DeviceMonitor, MockDeviceMonitor, find_key_file};
    use crate::crypto::Blake3Hash;

    #[tokio::test]
    async fn test_autodetect_with_mock_device_monitor_finds_planted_key_file() {
        let mount_directory = tempfile::tempdir().expect("tempdir should be created");
        let key_file_path = mount_directory.path().join("key.bin");
        let key_bytes = [0x3Au8; 32];
        std::fs::write(&key_file_path, key_bytes).expect("key file should be written");
        let reference_hash = Blake3Hash(*blake3::hash(&key_bytes).as_bytes());

        let monitor = Arc::new(MockDeviceMonitor::new());
        let mut stream = monitor.watch();
        monitor.push(DeviceEvent::Mounted {
            mount_path: mount_directory.path().to_path_buf(),
        });

        let event = stream
            .next()
            .await
            .expect("mounted event should be produced");
        let DeviceEvent::Mounted { mount_path } = event else {
            panic!("expected mounted event");
        };

        let found_path = find_key_file(&mount_path, &reference_hash)
            .await
            .expect("autodetect should succeed")
            .expect("matching key file should be found");

        assert_eq!(found_path, key_file_path);
    }
}
```

`SessionKeys` is re-exported `pub(crate)` — this matches the sub-phase's implicit scope (Phase 2.3's `SessionManager` consumes it from inside the crate). Public exposure happens later only if an IPC command needs it, which it does not.

#### 5.3.3 Create `src-tauri/src/auth/kdf.rs`

**File**: `C:\Users\chris\source\repos\arx-runa\src-tauri\src\auth\kdf.rs`

```rust
//! Argon2id KDF wrapper for Arx Runa authentication.
//!
//! Converts `(password, optional key file, salt, Argon2 params)` into a
//! 32-byte `master_key` written directly into a caller-provided buffer so the
//! output can live in locked memory. Never stores, logs, or returns owned
//! key material.

use argon2::{Algorithm, Argon2, Params, Version};
use zeroize::Zeroizing;

use crate::auth::error::AuthenticationError;

/// Argon2id cost parameters.
///
/// `memory_cost_kib` is in kibibytes (KiB). `time_cost` is iteration count.
/// `parallelism` is the degree of lane parallelism. Output length is fixed
/// at 32 bytes — `master_key` is always 256 bits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Argon2Params {
    /// Memory cost in KiB.
    pub memory_cost_kib: u32,
    /// Time cost (iterations).
    pub time_cost: u32,
    /// Parallelism degree.
    pub parallelism: u32,
}

impl Argon2Params {
    /// Arx Runa default Argon2id parameters from
    /// `docs/architecture/designs/authentication-and-session-management/design.md`
    /// §"Argon2id parameters" (RFC 9106 §4 recommended tier).
    pub const DEFAULT: Self = Self {
        memory_cost_kib: 65536,
        time_cost: 3,
        parallelism: 4,
    };
}

const MASTER_KEY_LENGTH_BYTES: usize = 32;
const KEY_FILE_LENGTH_BYTES: usize = 32;

/// Derives a 32-byte `master_key` into `output`.
///
/// - Tier 1: `argon2_input = password_utf8_bytes`.
/// - Tier 2: `argon2_input = password_utf8_bytes || key_file_bytes`.
///
/// The Argon2id "password" input is built inside a scratch
/// `Zeroizing<Vec<u8>>` buffer that is zeroed on drop. `output` is written
/// in place by `argon2::Argon2::hash_password_into`, so the caller can
/// provide a mutable reference into already-locked memory.
///
/// Argon2 parameter validation is delegated to `argon2::Params::new`; this
/// wrapper does not clamp, default, or bootstrap-validate. Bootstrap
/// validation against a local trusted parameter cache (design-invariants.md
/// §9) belongs to Phase 2.4's vault ceremonies.
pub(crate) fn derive_master_key_into(
    password_utf8_bytes: &[u8],
    key_file_bytes: Option<&[u8; KEY_FILE_LENGTH_BYTES]>,
    salt: &[u8; 32],
    params: &Argon2Params,
    output: &mut [u8; MASTER_KEY_LENGTH_BYTES],
) -> Result<(), AuthenticationError> {
    let combined_input_length = password_utf8_bytes.len()
        + key_file_bytes.map_or(0, |_| KEY_FILE_LENGTH_BYTES);
    let mut combined_input: Zeroizing<Vec<u8>> =
        Zeroizing::new(Vec::with_capacity(combined_input_length));
    combined_input.extend_from_slice(password_utf8_bytes);
    if let Some(bytes) = key_file_bytes {
        combined_input.extend_from_slice(bytes);
    }

    let argon2_params = Params::new(
        params.memory_cost_kib,
        params.time_cost,
        params.parallelism,
        Some(MASTER_KEY_LENGTH_BYTES),
    )
    .map_err(|_| AuthenticationError::InvalidCredentials)?;

    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, argon2_params);
    argon2
        .hash_password_into(&combined_input, salt, output)
        .map_err(|_| AuthenticationError::InvalidCredentials)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{Argon2Params, derive_master_key_into};

    /// Lightweight params for fast tests — well below the production defaults
    /// so the test suite finishes in a reasonable time on CI.
    const TEST_PARAMS: Argon2Params = Argon2Params {
        memory_cost_kib: 1024,
        time_cost: 1,
        parallelism: 1,
    };

    const TEST_SALT: [u8; 32] = [0x11u8; 32];

    #[test]
    fn test_derive_master_key_tier1_produces_expected_length() {
        let mut output = [0u8; 32];
        derive_master_key_into(
            b"correct horse battery staple",
            None,
            &TEST_SALT,
            &TEST_PARAMS,
            &mut output,
        )
        .expect("tier 1 derivation must succeed");
        assert_ne!(output, [0u8; 32]);
    }

    #[test]
    fn test_derive_master_key_tier2_produces_expected_length() {
        let key_file = [0x22u8; 32];
        let mut output = [0u8; 32];
        derive_master_key_into(
            b"correct horse battery staple",
            Some(&key_file),
            &TEST_SALT,
            &TEST_PARAMS,
            &mut output,
        )
        .expect("tier 2 derivation must succeed");
        assert_ne!(output, [0u8; 32]);
    }

    #[test]
    fn test_derive_master_key_is_deterministic_for_same_inputs() {
        let mut first = [0u8; 32];
        let mut second = [0u8; 32];
        derive_master_key_into(b"password", None, &TEST_SALT, &TEST_PARAMS, &mut first).unwrap();
        derive_master_key_into(b"password", None, &TEST_SALT, &TEST_PARAMS, &mut second).unwrap();
        assert_eq!(first, second);
    }

    #[test]
    fn test_derive_master_key_different_passwords_produce_different_outputs() {
        let mut first = [0u8; 32];
        let mut second = [0u8; 32];
        derive_master_key_into(b"password-a", None, &TEST_SALT, &TEST_PARAMS, &mut first).unwrap();
        derive_master_key_into(b"password-b", None, &TEST_SALT, &TEST_PARAMS, &mut second).unwrap();
        assert_ne!(first, second);
    }

    #[test]
    fn test_derive_master_key_different_key_files_produce_different_outputs() {
        let mut first = [0u8; 32];
        let mut second = [0u8; 32];
        derive_master_key_into(
            b"password",
            Some(&[0x01u8; 32]),
            &TEST_SALT,
            &TEST_PARAMS,
            &mut first,
        )
        .unwrap();
        derive_master_key_into(
            b"password",
            Some(&[0x02u8; 32]),
            &TEST_SALT,
            &TEST_PARAMS,
            &mut second,
        )
        .unwrap();
        assert_ne!(first, second);
    }

    #[test]
    fn test_derive_master_key_tier1_and_tier2_differ_for_same_password() {
        let key_file = [0x33u8; 32];
        let mut tier_one = [0u8; 32];
        let mut tier_two = [0u8; 32];
        derive_master_key_into(b"password", None, &TEST_SALT, &TEST_PARAMS, &mut tier_one)
            .unwrap();
        derive_master_key_into(
            b"password",
            Some(&key_file),
            &TEST_SALT,
            &TEST_PARAMS,
            &mut tier_two,
        )
        .unwrap();
        assert_ne!(tier_one, tier_two);
    }

    #[test]
    fn test_derive_master_key_different_salts_produce_different_outputs() {
        let mut first = [0u8; 32];
        let mut second = [0u8; 32];
        let salt_a = [0x55u8; 32];
        let salt_b = [0x66u8; 32];
        derive_master_key_into(b"password", None, &salt_a, &TEST_PARAMS, &mut first).unwrap();
        derive_master_key_into(b"password", None, &salt_b, &TEST_PARAMS, &mut second).unwrap();
        assert_ne!(first, second);
    }
}
```

#### 5.3.4 Create `src-tauri/src/auth/session.rs`

**File**: `C:\Users\chris\source\repos\arx-runa\src-tauri\src\auth\session.rs`

```rust
//! Session-key container backed by memory-locked heap buffers.
//!
//! Owns all derived vault-level keys for the duration of an authenticated
//! session. Keys are locked via `mlock` / `VirtualLock` on construction and
//! zeroed on drop. `master_key` is derived, expanded into the three
//! session-key fields, and zeroed without ever appearing as a struct field
//! or function return value.

use zeroize::Zeroizing;

use crate::auth::error::AuthenticationError;
use crate::auth::kdf::{Argon2Params, derive_master_key_into};
use crate::crypto::hkdf::{
    HKDF_INFO_KEY_ENCRYPTION, HKDF_INFO_MANIFEST_BACKUP, HKDF_INFO_SQLCIPHER,
    expand_vault_key_into,
};
use crate::memory::SecureBytes;

/// Holds all derived keys for the duration of an authenticated session.
/// All fields are memory-locked and zeroed on drop.
pub(crate) struct SessionKeys {
    pub(crate) key_encryption_key: SecureBytes<32>,
    pub(crate) sqlcipher_key: SecureBytes<32>,
    pub(crate) manifest_key: SecureBytes<32>,
}

impl SessionKeys {
    /// Derives `master_key` via Argon2id and expands it into three locked
    /// vault-level keys. `master_key` is allocated on the stack inside a
    /// `Zeroizing<[u8; 32]>` and never escapes this function.
    pub(crate) fn derive(
        password_utf8_bytes: &[u8],
        key_file_bytes: Option<&[u8; 32]>,
        salt: &[u8; 32],
        params: &Argon2Params,
    ) -> Result<Self, AuthenticationError> {
        let mut key_encryption_key = SecureBytes::<32>::new()?;
        let mut sqlcipher_key = SecureBytes::<32>::new()?;
        let mut manifest_key = SecureBytes::<32>::new()?;

        let mut master_key: Zeroizing<[u8; 32]> = Zeroizing::new([0u8; 32]);
        derive_master_key_into(
            password_utf8_bytes,
            key_file_bytes,
            salt,
            params,
            &mut master_key,
        )?;

        expand_vault_key_into(
            &master_key,
            HKDF_INFO_KEY_ENCRYPTION,
            key_encryption_key.as_mut(),
        )
        .map_err(|_| AuthenticationError::InvalidCredentials)?;
        expand_vault_key_into(
            &master_key,
            HKDF_INFO_SQLCIPHER,
            sqlcipher_key.as_mut(),
        )
        .map_err(|_| AuthenticationError::InvalidCredentials)?;
        expand_vault_key_into(
            &master_key,
            HKDF_INFO_MANIFEST_BACKUP,
            manifest_key.as_mut(),
        )
        .map_err(|_| AuthenticationError::InvalidCredentials)?;

        drop(master_key);

        Ok(Self {
            key_encryption_key,
            sqlcipher_key,
            manifest_key,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{Argon2Params, SessionKeys};

    /// Same as `kdf::tests::TEST_PARAMS` — lightweight so the session-level
    /// tests complete quickly.
    const TEST_PARAMS: Argon2Params = Argon2Params {
        memory_cost_kib: 1024,
        time_cost: 1,
        parallelism: 1,
    };
    const TEST_SALT: [u8; 32] = [0x44u8; 32];

    #[test]
    fn test_session_keys_derive_tier1_produces_three_distinct_keys() {
        let keys = SessionKeys::derive(b"password", None, &TEST_SALT, &TEST_PARAMS)
            .expect("derive must succeed");
        assert_ne!(keys.key_encryption_key.expose(), keys.sqlcipher_key.expose());
        assert_ne!(keys.key_encryption_key.expose(), keys.manifest_key.expose());
        assert_ne!(keys.sqlcipher_key.expose(), keys.manifest_key.expose());
    }

    #[test]
    fn test_session_keys_derive_tier2_produces_three_distinct_keys() {
        let key_file = [0x77u8; 32];
        let keys = SessionKeys::derive(b"password", Some(&key_file), &TEST_SALT, &TEST_PARAMS)
            .expect("derive must succeed");
        assert_ne!(keys.key_encryption_key.expose(), keys.sqlcipher_key.expose());
        assert_ne!(keys.key_encryption_key.expose(), keys.manifest_key.expose());
    }

    #[test]
    fn test_session_keys_derive_is_deterministic_for_same_inputs() {
        let first = SessionKeys::derive(b"password", None, &TEST_SALT, &TEST_PARAMS).unwrap();
        let second = SessionKeys::derive(b"password", None, &TEST_SALT, &TEST_PARAMS).unwrap();
        assert_eq!(first.key_encryption_key.expose(), second.key_encryption_key.expose());
        assert_eq!(first.sqlcipher_key.expose(), second.sqlcipher_key.expose());
        assert_eq!(first.manifest_key.expose(), second.manifest_key.expose());
    }

    #[test]
    fn test_session_keys_derive_different_passwords_produce_different_key_encryption_keys() {
        let first = SessionKeys::derive(b"password-a", None, &TEST_SALT, &TEST_PARAMS).unwrap();
        let second = SessionKeys::derive(b"password-b", None, &TEST_SALT, &TEST_PARAMS).unwrap();
        assert_ne!(first.key_encryption_key.expose(), second.key_encryption_key.expose());
    }

    #[test]
    fn test_session_keys_derive_different_key_files_produce_different_key_encryption_keys() {
        let first = SessionKeys::derive(
            b"password",
            Some(&[0x01u8; 32]),
            &TEST_SALT,
            &TEST_PARAMS,
        )
        .unwrap();
        let second = SessionKeys::derive(
            b"password",
            Some(&[0x02u8; 32]),
            &TEST_SALT,
            &TEST_PARAMS,
        )
        .unwrap();
        assert_ne!(first.key_encryption_key.expose(), second.key_encryption_key.expose());
    }

    #[test]
    fn test_session_keys_tier1_and_tier2_produce_different_key_encryption_keys() {
        let key_file = [0x88u8; 32];
        let tier_one = SessionKeys::derive(b"password", None, &TEST_SALT, &TEST_PARAMS).unwrap();
        let tier_two = SessionKeys::derive(
            b"password",
            Some(&key_file),
            &TEST_SALT,
            &TEST_PARAMS,
        )
        .unwrap();
        assert_ne!(tier_one.key_encryption_key.expose(), tier_two.key_encryption_key.expose());
    }

    #[test]
    fn test_session_keys_derive_returns_memory_lock_failed_when_lock_is_forced_to_fail() {
        crate::memory::platform::set_force_lock_failure(true);
        let result = SessionKeys::derive(b"password", None, &TEST_SALT, &TEST_PARAMS);
        crate::memory::platform::set_force_lock_failure(false);

        let error = result.expect_err("forced lock failure must propagate");
        let crate::auth::error::AuthenticationError::MemoryLockFailed(message) = error else {
            panic!("expected MemoryLockFailed variant, got {error:?}");
        };
        assert!(!message.is_empty());
    }
}
```

### 5.4 Refactor `crypto::hkdf` to expose the expansion helper

**File**: `C:\Users\chris\source\repos\arx-runa\src-tauri\src\crypto\hkdf.rs`

Replace current body with:

```rust
//! HKDF-SHA256 vault key derivation.

use crate::crypto::error::CryptoError;
use crate::crypto::types::{KeyEncryptionKey, ManifestKey, SqlcipherKey};
use hkdf::Hkdf;
use secrecy::SecretBox;
use sha2::Sha256;
use zeroize::Zeroizing;

/// Fixed HKDF-SHA256 salt used for all vault-level derivations.
///
/// Acts as a cross-application domain separator per RFC 5869 §3.1 and as a
/// version point for future key-hierarchy migrations.
pub(crate) const HKDF_SALT: &[u8] = b"arx-runa-v1";

/// HKDF `info` string for the key-encryption key.
pub(crate) const HKDF_INFO_KEY_ENCRYPTION: &[u8] = b"arx-runa-key-encryption";

/// HKDF `info` string for the SQLCipher DB key.
pub(crate) const HKDF_INFO_SQLCIPHER: &[u8] = b"arx-runa-sqlcipher";

/// HKDF `info` string for the manifest-backup key.
pub(crate) const HKDF_INFO_MANIFEST_BACKUP: &[u8] = b"arx-runa-manifest-backup";

/// Vault-level keys derived from one master key.
pub struct VaultKeys {
    /// Key-encryption key used to wrap per-file keys.
    pub key_encryption_key: KeyEncryptionKey,
    /// SQLCipher database key.
    pub sqlcipher_key: SqlcipherKey,
    /// Manifest-backup encryption key.
    pub manifest_key: ManifestKey,
}

/// Runs a single HKDF-SHA256 extract/expand into a caller-provided buffer.
///
/// This is the in-place variant used by `auth::session::SessionKeys::derive`
/// so the HKDF output lands directly inside an `mlock`ed `SecureBytes`
/// buffer. No intermediate heap allocation touches the key material.
///
/// # Errors
/// Returns `CryptoError::KeyDerivationFailed` if HKDF expansion fails. For
/// a 32-byte output with SHA-256 this is unreachable in practice, but the
/// fallible surface lets callers propagate unexpected failures instead of
/// panicking.
pub(crate) fn expand_vault_key_into(
    master_key_bytes: &[u8; 32],
    info: &[u8],
    output: &mut [u8; 32],
) -> Result<(), CryptoError> {
    let hkdf = Hkdf::<Sha256>::new(Some(HKDF_SALT), master_key_bytes);
    hkdf.expand(info, output)
        .map_err(|_| CryptoError::KeyDerivationFailed)
}

/// Derives vault keys from 32 bytes of master key material.
///
/// # Errors
/// Returns `CryptoError::KeyDerivationFailed` if HKDF expansion fails. For
/// a 32-byte output with SHA-256 this is unreachable in practice, but the
/// fallible surface lets callers propagate unexpected failures instead of
/// panicking.
pub fn derive_vault_keys(master_key_bytes: &[u8; 32]) -> Result<VaultKeys, CryptoError> {
    Ok(VaultKeys {
        key_encryption_key: KeyEncryptionKey::from_secret_box(expand_into_secret_box(
            master_key_bytes,
            HKDF_INFO_KEY_ENCRYPTION,
        )?),
        sqlcipher_key: SqlcipherKey::from_secret_box(expand_into_secret_box(
            master_key_bytes,
            HKDF_INFO_SQLCIPHER,
        )?),
        manifest_key: ManifestKey::from_secret_box(expand_into_secret_box(
            master_key_bytes,
            HKDF_INFO_MANIFEST_BACKUP,
        )?),
    })
}

/// Runs HKDF-SHA256 expand into a fresh `SecretBox` heap buffer via the
/// in-place helper.
fn expand_into_secret_box(
    master_key_bytes: &[u8; 32],
    info: &[u8],
) -> Result<SecretBox<[u8; 32]>, CryptoError> {
    let mut scratch: Zeroizing<[u8; 32]> = Zeroizing::new([0u8; 32]);
    expand_vault_key_into(master_key_bytes, info, &mut scratch)?;
    Ok(SecretBox::new(Box::new(*scratch)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_derive_vault_keys_same_input_produces_same_output() {
        let master_key_bytes = [0x42u8; 32];
        let first = derive_vault_keys(&master_key_bytes).expect("derive must succeed");
        let second = derive_vault_keys(&master_key_bytes).expect("derive must succeed");

        assert_eq!(
            first.key_encryption_key.expose(),
            second.key_encryption_key.expose()
        );
        assert_eq!(first.sqlcipher_key.expose(), second.sqlcipher_key.expose());
        assert_eq!(first.manifest_key.expose(), second.manifest_key.expose());
    }

    #[test]
    fn test_derive_vault_keys_different_inputs_produce_different_outputs() {
        let first_master_key_bytes = [0x01u8; 32];
        let second_master_key_bytes = [0x02u8; 32];
        let first = derive_vault_keys(&first_master_key_bytes).expect("derive must succeed");
        let second = derive_vault_keys(&second_master_key_bytes).expect("derive must succeed");

        assert_ne!(
            first.key_encryption_key.expose(),
            second.key_encryption_key.expose()
        );
        assert_ne!(first.sqlcipher_key.expose(), second.sqlcipher_key.expose());
        assert_ne!(first.manifest_key.expose(), second.manifest_key.expose());
    }

    #[test]
    fn test_derive_vault_keys_single_input_produces_distinct_keys() {
        let master_key_bytes = [0xA5u8; 32];
        let keys = derive_vault_keys(&master_key_bytes).expect("derive must succeed");

        assert_ne!(
            keys.key_encryption_key.expose(),
            keys.sqlcipher_key.expose()
        );
        assert_ne!(keys.key_encryption_key.expose(), keys.manifest_key.expose());
        assert_ne!(keys.sqlcipher_key.expose(), keys.manifest_key.expose());
    }

    #[test]
    fn test_derive_vault_keys_all_zero_master_key_succeeds() {
        let master_key_bytes = [0u8; 32];
        let keys = derive_vault_keys(&master_key_bytes).expect("derive must succeed");

        assert_ne!(
            keys.key_encryption_key.expose(),
            keys.sqlcipher_key.expose()
        );
        assert_ne!(keys.key_encryption_key.expose(), keys.manifest_key.expose());
        assert_ne!(keys.sqlcipher_key.expose(), keys.manifest_key.expose());
    }

    #[test]
    fn test_expand_vault_key_into_matches_derive_vault_keys_output() {
        let master_key_bytes = [0x77u8; 32];
        let vault_keys = derive_vault_keys(&master_key_bytes).expect("derive must succeed");
        let mut output = [0u8; 32];
        expand_vault_key_into(&master_key_bytes, HKDF_INFO_KEY_ENCRYPTION, &mut output)
            .expect("expand must succeed");
        assert_eq!(&output, vault_keys.key_encryption_key.expose());
    }
}
```

Note the new helper constant on the copy path: `SecretBox::new(Box::new(*scratch))` dereferences the `Zeroizing<[u8; 32]>` to copy the bytes into a fresh Box. This preserves the existing `derive_vault_keys` behavior exactly — the test suite already covers it. The copy is acceptable here because this path is only used by external callers that do not need mlock; `SessionKeys` uses the in-place helper and never touches this path.

### 5.5 Update `src-tauri/src/crypto/mod.rs` (if needed)

**File**: `C:\Users\chris\source\repos\arx-runa\src-tauri\src\crypto\mod.rs`

No change to the public re-exports. `HKDF_SALT` / info constants are `pub(crate)`, so they remain module-private to `crypto` but visible crate-wide to `auth`. `expand_vault_key_into` is also `pub(crate)`. Do **not** export them from `crypto::mod.rs` as public surface.

### 5.6 Lint, format, test

Run, in order, from `C:\Users\chris\source\repos\arx-runa\src-tauri`:

```bash
cargo fmt
cargo clippy --all-targets -- -D warnings
cargo test auth::kdf
cargo test auth::session
cargo test auth::error
cargo test crypto::hkdf
cargo test memory
cargo test
```

Resolve any `clippy::needless_borrow`, `clippy::let_unit_value`, or similar warnings before proceeding. If `cargo clippy` flags the `unsafe` blocks for missing `# Safety` doc comments on the `unsafe fn` declarations, add them — the signatures in 5.2.4 and 5.2.5 already include `# Safety` doc paragraphs.

### 5.7 Invoke the security reviewer

Per the sub-phase roadmap's Security Review Checkpoints, Phase 2.2 REQUIRES invocation of the `security-reviewer` agent. After all tests pass, `/implement-plan` routes to the agent with scope:

- `src-tauri/src/auth/kdf.rs`
- `src-tauri/src/auth/session.rs`
- `src-tauri/src/auth/error.rs`
- `src-tauri/src/memory/secure_buffer.rs`
- `src-tauri/src/memory/platform/mod.rs`
- `src-tauri/src/memory/platform/unix.rs`
- `src-tauri/src/memory/platform/windows.rs`
- `src-tauri/src/memory/error.rs`
- `src-tauri/src/crypto/hkdf.rs`

Section 6 lists what the reviewer should check.

## 6. Security implications

### 6.a — Expected sensitive path set

Files this plan anticipates creating or modifying under `src-tauri/src/auth/`, `src-tauri/src/crypto/`, or `src-tauri/src/storage/`:

- `src-tauri/src/auth/kdf.rs` (new)
- `src-tauri/src/auth/session.rs` (new)
- `src-tauri/src/auth/error.rs` (modify — rename enum, add variants)
- `src-tauri/src/auth/mod.rs` (modify — add submodule declarations and re-exports)
- `src-tauri/src/crypto/hkdf.rs` (modify — add in-place helper, promote constants to `pub(crate)`)

Files under the memory module (which the security reviewer should also inspect because it owns the mlock invariants consumed by session keys):

- `src-tauri/src/memory/secure_buffer.rs` (new)
- `src-tauri/src/memory/platform/mod.rs` (new)
- `src-tauri/src/memory/platform/unix.rs` (new, contains `unsafe`)
- `src-tauri/src/memory/platform/windows.rs` (new, contains `unsafe`)
- `src-tauri/src/memory/error.rs` (modify — add `PlatformFailure` variant)
- `src-tauri/src/memory/mod.rs` (modify — add submodule declarations)

Any other files touched under `src-tauri/src/crypto/`, `src-tauri/src/auth/`, or `src-tauri/src/storage/` is an unanticipated change and triggers a Plan Deviation at verify time.

### 6.b — Invoke security-reviewer agent? **YES**

**Rationale**: Phase 2.2 is the first phase to hold vault keys in process memory. It introduces (a) the Argon2id KDF parameter path that future ceremonies depend on, (b) `unsafe` platform code that directly manages page locking, (c) the `SessionKeys` container that Phases 2.3, 2.4, 3.x, 4.x, and 5.x will all consume, and (d) the test-only fault-injection hook that must not bleed into release builds. Any defect here has blast radius across the entire auth/storage/sync stack. The sub-phase roadmap explicitly mandates security review (Security Review Checkpoints section, roadmap.md line 108: "Phase 2.2: Required — Invoke `security-reviewer` agent after implementation").

### 6.c — What the reviewer should check

1. **Argon2id parameters match the design**: `Argon2Params::DEFAULT` equals `{ memory_cost_kib: 65536, time_cost: 3, parallelism: 4 }`; `Argon2::new(Algorithm::Argon2id, Version::V0x13, ...)` uses Argon2id (not Argon2i or Argon2d); output length is 32 bytes (matching the hard-coded `Some(MASTER_KEY_LENGTH_BYTES)`).

2. **`master_key` lifetime is scope-bounded**: confirm `master_key` never appears as a field of any struct, never appears in a `return` expression, and is wrapped in `Zeroizing<[u8; 32]>` inside `SessionKeys::derive` so it zeros on every exit path (success, HKDF error, panic unwind).

3. **`mlock` is applied before any key material is written**: `SecureBytes::new()` allocates zero-initialized `Box<[u8; N]>`, immediately calls `platform::lock_memory`, and only then returns a handle whose `as_mut` could expose writes. There is no code path where HKDF output or master key bytes reach the buffer before the lock succeeds.

4. **`unlock` + `zeroize` ordering in `Drop`**: confirm `SecureBytes::drop` calls `zeroize` BEFORE `unlock_memory`. Reversing the order would leave key bytes visible to the OS page reclaimer for a window.

5. **`unsafe` blocks carry soundness reasoning**: every `unsafe { … }` in `platform/unix.rs`, `platform/windows.rs`, and `secure_buffer.rs` carries a `// SAFETY:` comment stating the pointer-validity and length-validity invariants.

6. **Test-only fault injection does not leak into release**: `#[cfg(test)]` gating on `set_force_lock_failure` and the `thread_local` is strict; the production `lock_memory` path on release builds contains no branch on the thread-local.

7. **`Argon2id` input concatenation is byte-exact**: Tier 1 passes `password_utf8_bytes` only; Tier 2 passes `password_utf8_bytes || key_file_bytes`. The scratch `Zeroizing<Vec<u8>>` is zeroed on drop on every path.

8. **`InvalidCredentials` is non-oracular**: the KDF wrapper maps `argon2::Params::new` errors AND `argon2::hash_password_into` errors to `InvalidCredentials` with the same `thiserror` string. There is no branch that returns a different variant based on why the derivation failed.

9. **HKDF info strings are fetched from `crypto::hkdf`**: `auth::session::SessionKeys::derive` imports `HKDF_INFO_KEY_ENCRYPTION`, `HKDF_INFO_SQLCIPHER`, `HKDF_INFO_MANIFEST_BACKUP` from `crypto::hkdf` rather than redefining byte literals. Invariant #3 (HKDF constants) is preserved — there is only one source of truth.

10. **No key material in logs, error messages, or stack traces**: `tracing` is not invoked from any function that takes or returns key bytes. `AuthenticationError`'s `Display` impl emits only the short constants from design.md — never the `String` payload for `MemoryLockFailed` (the payload is carried programmatically for IPC but kept out of the default `Display`).

11. **Platform failure messages match design.md verbatim**: Linux, Windows, and macOS strings match lines 219–221 of design.md byte-for-byte (reviewer should `diff` the strings).

12. **No `.unwrap()` or `.expect()` in production paths**: all fallible calls use `?`. Tests may use `.expect(…)` — this is project convention.

13. **`Win32_System_Memory` feature flag is present**: Cargo.toml has `features = ["Win32_Storage_FileSystem", "Win32_Foundation", "Win32_System_Memory"]`. Reviewer should verify `VirtualLock` and `VirtualUnlock` are actually exposed by the chosen feature set.

## 7. Execution and testing strategy

**Test scope:**

- [x] Basic unit tests (KDF determinism, session derivation, error variants — written during implementation)
- [x] Adversarial tests (forced mlock failure, wrong-credential input differentiation)
- [x] Property-based tests (Argon2id determinism; random password/salt/key-file inputs should always produce non-zero, distinct-per-input outputs)
- [ ] Integration tests (deferred to Phase 2.4 — `authenticate` end-to-end requires vault header parsing)
- [x] Boundary cases (empty password, 1-byte password, password containing non-ASCII UTF-8, 32-byte key file of all zeros / all ones / random)

**Coverage target**: ≥90% of new lines in `auth/kdf.rs`, `auth/session.rs`, `memory/secure_buffer.rs`, `memory/platform/unix.rs`, `memory/platform/windows.rs`. The sub-phase is security-critical; uncovered lines are a blocker at verify time.

**Boundary cases to cover:**

- Empty password (`b""`) Tier 1 — wrapper must succeed (Argon2id accepts empty passwords; upstream UI gates empty password entry separately).
- Empty password Tier 2 with a valid 32-byte key file — wrapper must succeed (combined input is 32 bytes).
- Non-ASCII UTF-8 password (`"πασσωορδ"`) Tier 1 and Tier 2 — wrapper must succeed and be deterministic.
- 32-byte key file of all zeros — wrapper must succeed and produce a different output from Tier 1 with the same password.
- 32-byte key file of all `0xFF` — wrapper must succeed and produce yet another different output.
- Derivation with `Argon2Params::DEFAULT` — runs once in the test suite with a feature flag or an `#[ignore]`-annotated test so CI can opt in without paying 64 MiB × 3 × 4 threads per test case. Mark as `#[ignore]` by default; document in the test comment that `cargo test -- --ignored` runs it.
- `SessionKeys::derive` under `set_force_lock_failure(true)` must return `AuthenticationError::MemoryLockFailed(_)` with a non-empty `String`.
- `SecureBytes::<32>::new()` construction under forced failure must return `MemoryLockError::PlatformFailure { platform_message: _ }`.

**Property tests** (using `proptest = "1"` already in `dev-dependencies`):

- For random passwords of length 1–64 and random 32-byte key files and random 32-byte salts, `derive_master_key_into` with `TEST_PARAMS` is deterministic (same inputs → same output) and distinct inputs produce distinct outputs with overwhelming probability. Assertion uses 32 randomised cases with `proptest::prop_assert_*`.

**Memory-safety tests**:

- `SecureBytes::<32>` with `zeroize::Zeroize::zeroize` clears the buffer in place (pattern adapted from `test_file_key_zeroize_trait_clears_memory` in `src-tauri/src/crypto/types/mod.rs`).
- `SessionKeys` drops in the correct order (key_encryption_key, sqlcipher_key, manifest_key) — Rust's struct drop order is declaration order; the reviewer confirms declaration order matches expectations (irrelevant for correctness here because all three are independent, but good hygiene).

**Invoke test-writer agent? YES** — this sub-phase needs (a) adversarial mlock-failure coverage, (b) property-based KDF determinism, and (c) memory-safety inspection tests that are beyond "baseline tests written during implementation". The frontmatter field `test-agent-required: true` matches.

**Test-writer focus (passed via `/implement-plan` orchestration):**

- Property-based tests for `derive_master_key_into` determinism and input distinctness.
- Adversarial tests for `SessionKeys::derive` under forced mlock failure (Linux and Windows paths).
- Memory-safety tests for `SecureBytes<N>` zeroization.
- Boundary-case tests for non-ASCII UTF-8 passwords.
- Scope: the files listed in Section 6.a only. Do not expand into Phase 2.1 modules.

**Test acceptance criteria**:

- All tests must pass with `cargo test` (not just the scoped `cargo test auth::kdf`).
- No new `cargo clippy -- -D warnings` findings.
- `cargo fmt --check` clean.
- The ignored `Argon2Params::DEFAULT` test (64 MiB) passes when explicitly run via `cargo test -- --ignored` — used as a pre-merge smoke test, not part of the default CI run.
- Security reviewer returns no CRITICAL findings; WARNING findings logged to the plan's Implementation Log.

## 8. Documentation impact

Required updates after implementation (these are real deviations and doc-sync needs, per the Step 1.8 governance drift review and DC-1, DC-2, DC-3, DC-4, DC-7 resolutions):

1. **`docs/architecture/designs/authentication-and-session-management/sub-phases/2.2-argon2id-and-session-keys.md`**:
   - Deliverable 2 (line 17): change "Integration with Phase 1.1's `derive_vault_keys` function: pass `master_key` to HKDF expansion to produce…" to explicitly reference `crypto::hkdf::expand_vault_key_into` as the shared helper, keeping `derive_vault_keys` as a separate non-locked convenience API. Resolution for DC-2.
   - Deliverable 3 (line 18): change "fields `key_encryption_key: SecretBox<[u8; 32]>`, `sqlcipher_key: SecretBox<[u8; 32]>`, `manifest_key: SecretBox<[u8; 32]>`, derived with `#[derive(ZeroizeOnDrop)]`" to "fields `key_encryption_key: SecureBytes<32>`, `sqlcipher_key: SecureBytes<32>`, `manifest_key: SecureBytes<32>`" with a note that `SecureBytes` is the RAII container introduced in `src-tauri/src/memory/secure_buffer.rs` to unify `mlock` + `ZeroizeOnDrop`. Resolution for DC-1.
   - Deliverable 8 bullets 3–5 (lines 26–28): replace "Wrong password (Tier 1) → `InvalidCredentials`" family with "different credentials produce different `SessionKeys` bytes" tests, and note that `InvalidCredentials` is raised by Phase 2.4's vault-header probe rather than by the KDF wrapper. Resolution for DC-3.
   - Implementation Notes (line 76): add macOS to the `mlock` note — the Unix platform module covers both Linux and macOS via POSIX `libc::mlock`. Resolution for DC-7.

2. **`docs/architecture/designs/authentication-and-session-management/design.md`**:
   - Lines 186–195 (`SessionKeys` struct snippet): update field types from `SecretBox<[u8; 32]>` to the new `SecureBytes<32>` container. Keep the `#[derive(ZeroizeOnDrop)]` line if the final implementation uses it; remove if the implementation uses manual `Drop`. Resolution for DC-1.

3. **`docs/architecture/designs/tauri-ipc-and-frontend/sub-phases/roadmap.md`** line 53: change `AuthError` → `AuthenticationError` in the "`From` impls" bullet. Resolution for DC-4. This is a forward-looking doc fix; no Phase 6.1 plan exists yet.

4. **`.claude/plans/phase-2-2-argon2id-and-session-keys.md`** (this file) — status moves from `draft` to `approved` after user review, then to `implemented` after verify passes.

No new `.md` files are required. No changes to `docs/roadmap.md`, `docs/architecture/design-invariants.md`, or `docs/architecture/designs/cryptographic-primitives/design.md` are needed — invariant #3 (HKDF constants) remains unchanged, invariant #9 (Argon2 vault-header trust) remains a Phase 2.4 concern, and the crypto design already delegates Argon2id ownership to the auth design (see `docs/architecture/designs/cryptographic-primitives/design.md` line 212 and line 579).

## 9. Governance sync actions (pre-implementation)

The following edits are deterministic and must run BEFORE Step 5 coding so the rules and the implementation land in sync.

### GS-1 — Update `.claude/rules/auth.md` to name `SessionKeys` and `AuthenticationError`

- **Reason / linked concern**: DC-4 (enum rename), DC-8 (rule file does not mention `SessionKeys`).
- **Target files**:
  - `C:\Users\chris\source\repos\arx-runa\.claude\rules\auth.md`
- **Required edit**:
  1. In the "Errors" section, replace any bare `AuthError` references with `AuthenticationError`. (Currently the rule does not name the enum, so this is a no-op content check — confirm no stale name appears.)
  2. In the "Session" section, add a bullet: `- Session keys live in SessionKeys (src-tauri/src/auth/session.rs) with fields backed by SecureBytes<32>; drop order runs zeroize → munlock/VirtualUnlock → free.`
  3. Bump the "last verified against design dated" stamp to the date of the current `design.md` header (`2026-04-12`, already current — confirm no drift).
- **Verification**: re-read the file after editing; grep for `AuthError` (no hits expected) and for `SessionKeys` (one hit expected).

### GS-2 — Update `.claude/rules/memory-protection.md` to name `SecureBytes`

- **Reason / linked concern**: DC-1, DC-8.
- **Target files**:
  - `C:\Users\chris\source\repos\arx-runa\.claude\rules\memory-protection.md`
- **Required edit**:
  1. In the "Unsafe containment" bullet list, change the example from `(e.g., `platform/unix.rs`)` to reference the actual path that exists after implementation: `(e.g., `src-tauri/src/memory/platform/unix.rs`)`.
  2. In the "Safe wrapper requirements" section, change `SecureBuffer` (the current example name) to `SecureBytes<N>` to match the type introduced in this phase. Keep the rest of the bullet list intact.
  3. Add a bullet: `- The mlock / VirtualLock wrapper exposes a Result<(), MemoryLockError> surface; callers map it into their own error enum (e.g., auth converts to AuthenticationError::MemoryLockFailed).`
- **Verification**: re-read the file; grep for `SecureBuffer` (no hits expected) and `SecureBytes` (at least one hit expected).

### GS-3 — Run `/copilot-sync` to regenerate `.github/instructions/*.instructions.md`

- **Reason / linked concern**: `.claude/rules/auth.md` and `.claude/rules/memory-protection.md` are the source of truth; their `.github/instructions/*.instructions.md` mirrors must be regenerated after GS-1 and GS-2.
- **Target files**:
  - `C:\Users\chris\source\repos\arx-runa\.github\instructions\auth.instructions.md`
  - `C:\Users\chris\source\repos\arx-runa\.github\instructions\memory-protection.instructions.md`
- **Required edit**: Run the `copilot-sync` skill (`Skill: copilot-sync`) after GS-1 and GS-2 complete. The skill regenerates the `.github/instructions/` mirrors from the `.claude/rules/` sources and is safe to re-run.
- **Verification**: `diff` the `rules/auth.md` body against the `instructions/auth.instructions.md` body (after the `applyTo:` frontmatter). They must be identical except for the `applyTo:` front matter.

### GS-4 — Update `.claude/agents/security-reviewer.md` — no changes required

- **Reason / linked concern**: the agent already covers Argon2id parameters, mlock enforcement, HKDF info strings, and the `SessionKeys` zeroization requirement. Inspected lines 36, 46, 63, 106 — all current.
- **Target files**: none.
- **Required edit**: none.
- **Verification**: grep for `mlock`, `Argon2id`, `HKDF`, `SessionKeys` in `C:\Users\chris\source\repos\arx-runa\.claude\agents\security-reviewer.md` — all present.

### GS-5 — Update `.claude/agents/test-writer.md` — no changes required

- **Reason / linked concern**: agent already covers property-based testing, memory-safety test patterns, and test-crate expectations (proptest, tempfile, assert_matches).
- **Target files**: none.
- **Required edit**: none.
- **Verification**: none.

## 10. Handoff Notes for Implementer

Working directory: `C:\Users\chris\source\repos\arx-runa`. All paths in this plan are absolute so they resolve the same from any shell.

Order of operations:

1. Apply **Section 9** governance edits first (GS-1, GS-2, GS-3). These are rule-and-mirror changes only; no Rust code touched yet.
2. Apply **Section 5** code changes in the order 5.1 → 5.2 → 5.3 → 5.4 → 5.5 → 5.6. The dependency addition (5.1) must complete before any compilation; memory module (5.2) must compile before auth module (5.3) can reference `SecureBytes`; hkdf refactor (5.4) must compile before `auth::session` can import `expand_vault_key_into`.
3. Run **Section 5.6** lint/format/test gates.
4. Invoke the **security-reviewer** agent (**Section 5.7**) with the file scope from Section 6.a.
5. Address any CRITICAL findings; log WARNING / NOTE findings to the plan's Implementation Log.
6. Verify pass: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test` (host platform). If possible, cross-compile to the other two platforms to catch target-specific regressions.
7. Apply **Section 8** documentation updates (sub-phase doc, parent design doc, tauri-ipc roadmap doc) as part of the same commit or a follow-up in the same PR.

Traps to watch for:

- `VirtualLock` / `VirtualUnlock` in `windows = "0.59"` may return `BOOL` or `Result<()>` depending on the exact feature surface — try `BOOL::as_bool()` first, fall back to `Result::is_ok()` if the compiler disagrees. Same semantics either way.
- The Phase 2.2 tests must not pay for 64 MiB × 3 × 4 threads of Argon2id every test invocation. Use the lightweight `TEST_PARAMS` (1 MiB, t=1, p=1) for all runtime tests; gate the `Argon2Params::DEFAULT` round-trip test with `#[ignore]` so it only runs on demand (`cargo test -- --ignored`).
- The `#[cfg(test)] thread_local!` fault injection switch must be set to `false` in every test that sets it to `true` — use a test-scoped guard or a `defer!` idiom to guarantee the reset even on panic. Otherwise one failing test leaks state into the next.
- `#[derive(ZeroizeOnDrop)]` on `SessionKeys` together with `#[derive(ZeroizeOnDrop)]` on `SecureBytes<N>` via `Drop` — the derive cannot coexist with a manual `Drop` on the same type. `SecureBytes<N>` uses manual `Drop` (Section 5.2.6). `SessionKeys` has NO manual `Drop` and NO `ZeroizeOnDrop` derive — it relies on field-by-field drop to chain into each `SecureBytes::drop`. If Codex prefers the derive form on `SessionKeys`, that is also acceptable — both compile cleanly and are runtime-equivalent.
- Phase 2.1's `tokio-stream` / `walkdir` / platform device-monitor dependencies must remain intact. Do not remove them while editing Cargo.toml in Step 5.1.

Plan is self-contained — Codex does not need to re-read the sub-phase doc. All inline signatures, deliverable mappings, and DDL-equivalent details are transcribed into Section 5. The sub-phase doc remains the authoritative source for the original intent, but any deviation recorded here takes precedence and is doc-synced per Section 8.

## Implementation Log

- **Date**: 2026-04-13T21:09:04.5296963Z
- **Branch**: development

### Agent evidence

| Approach step | Agent | Agent ID | Outcome |
|---|---|---|---|
| 5.1 Add dependencies | Copilot CLI | local | Completed: updated `src-tauri/Cargo.toml` for `libc` and `Win32_System_Memory`. |
| 5.2 Extend the `memory` module | Copilot CLI | local | Completed: added platform lock wrappers, `SecureBytes`, `MemoryLockError`, and tests. |
| 5.3 Extend the `auth` module | Copilot CLI | local | Completed: added `AuthenticationError`, `kdf.rs`, `session.rs`, and module wiring. |
| 5.4 Refactor `crypto::hkdf` | Copilot CLI | local | Completed: exposed `expand_vault_key_into` and `pub(crate)` HKDF constants; kept public `derive_vault_keys` behavior. |
| 5.5 Update `src-tauri/src/crypto/mod.rs` (if needed) | Copilot CLI | local | Completed: no change required. |
| 5.6 Lint, format, test | Copilot CLI | local | Completed: `cargo fmt --check`, `cargo clippy --workspace -- -D warnings`, and full `cargo test` pass. |
| 5.6 Test expansion (required) | test-writer | phase2-2-test-writer | Completed: added property/adversarial/boundary tests in Phase 2.2 scope. |
| 5.7 Security review | security-reviewer | phase2-2-security | Completed: 0 CRITICAL findings, 2 WARNING findings, 0 NOTE findings. |

### Files changed

- `.claude/plans/phase-2-1-usb-key-file-and-device-monitor.md`
- `.claude/plans/phase-2-2-argon2id-and-session-keys.md`
- `.claude/rules/auth.md`
- `.claude/rules/memory-protection.md`
- `.github/instructions/auth.instructions.md`
- `.github/instructions/memory-protection.instructions.md`
- `Cargo.lock`
- `docs/architecture/designs/authentication-and-session-management/design.md`
- `docs/architecture/designs/authentication-and-session-management/sub-phases/2.2-argon2id-and-session-keys.md`
- `docs/architecture/designs/tauri-ipc-and-frontend/sub-phases/roadmap.md`
- `src-tauri/Cargo.toml`
- `src-tauri/src/auth/error.rs`
- `src-tauri/src/auth/mod.rs`
- `src-tauri/src/auth/kdf.rs` (new)
- `src-tauri/src/auth/session.rs` (new)
- `src-tauri/src/crypto/hkdf.rs`
- `src-tauri/src/memory/error.rs`
- `src-tauri/src/memory/mod.rs`
- `src-tauri/src/memory/platform/mod.rs` (new)
- `src-tauri/src/memory/platform/unix.rs` (new)
- `src-tauri/src/memory/platform/windows.rs` (new)
- `src-tauri/src/memory/secure_buffer.rs` (new)

### Test results

- `cargo test`: **ok** — `102 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out`.
- Scoped runs from plan sequence (`auth::kdf`, `auth::session`, `auth::error`, `crypto::hkdf`, `memory`) completed successfully.
- Ignored default-params smoke test: `auth::kdf::tests::test_derive_master_key_default_params_succeeds` passed via `cargo test ... -- --ignored`.

### Clippy results

- `cargo clippy --workspace -- -D warnings`: **clean**.
- `cargo clippy --all-targets -- -D warnings`: blocked by **pre-existing** dead-code items in `src-tauri/src/crypto/types/mod.rs`:
  - `SqlcipherKey::from_bytes`
  - `ManifestKey::from_bytes`
- No new Phase 2.2 clippy findings remain.

### Security review

- `security-reviewer` findings:
  - **WARNING**: `master_key` is derived in `session.rs` into a stack `Zeroizing<[u8; 32]>` buffer that is not mlocked.
  - **WARNING**: `crypto::hkdf::expand_into_secret_box` currently copies `scratch` into `SecretBox::new(Box::new(*scratch))`, producing a transient stack copy.
- **CRITICAL** findings: none.

### Governance sync

- Action count: **5** (GS-1..GS-5).
- Updated files:
  - `.claude/rules/auth.md`
  - `.claude/rules/memory-protection.md`
  - `.github/instructions/auth.instructions.md`
  - `.github/instructions/memory-protection.instructions.md`
- `/copilot-sync` outcome: completed and mirror parity verified for `auth` and `memory-protection`.

### Sub-phase decisions sync

- Target doc: `docs/architecture/designs/authentication-and-session-management/sub-phases/2.2-argon2id-and-session-keys.md`
- `## Implementation Decisions` section added with **4** decisions updated.

### Deviations from plan

- Dependency gate unblocking: prerequisite plan `.claude/plans/phase-2-1-usb-key-file-and-device-monitor.md` was marked `implemented` before continuing Phase 2.2.
- `cargo clippy --all-targets -- -D warnings` remains blocked by unrelated pre-existing dead-code items in `crypto/types`.

### Documentation flagged

1. **`docs/architecture/designs/authentication-and-session-management/sub-phases/2.2-argon2id-and-session-keys.md`**:
   - Deliverable 2 (line 17): change "Integration with Phase 1.1's `derive_vault_keys` function: pass `master_key` to HKDF expansion to produce…" to explicitly reference `crypto::hkdf::expand_vault_key_into` as the shared helper, keeping `derive_vault_keys` as a separate non-locked convenience API. Resolution for DC-2.
   - Deliverable 3 (line 18): change "fields `key_encryption_key: SecretBox<[u8; 32]>`, `sqlcipher_key: SecretBox<[u8; 32]>`, `manifest_key: SecretBox<[u8; 32]>`, derived with `#[derive(ZeroizeOnDrop)]`" to "fields `key_encryption_key: SecureBytes<32>`, `sqlcipher_key: SecureBytes<32>`, `manifest_key: SecureBytes<32>`" with a note that `SecureBytes` is the RAII container introduced in `src-tauri/src/memory/secure_buffer.rs` to unify `mlock` + `ZeroizeOnDrop`. Resolution for DC-1.
   - Deliverable 8 bullets 3–5 (lines 26–28): replace "Wrong password (Tier 1) → `InvalidCredentials`" family with "different credentials produce different `SessionKeys` bytes" tests, and note that `InvalidCredentials` is raised by Phase 2.4's vault-header probe rather than by the KDF wrapper. Resolution for DC-3.
   - Implementation Notes (line 76): add macOS to the `mlock` note — the Unix platform module covers both Linux and macOS via POSIX `libc::mlock`. Resolution for DC-7.

2. **`docs/architecture/designs/authentication-and-session-management/design.md`**:
   - Lines 186–195 (`SessionKeys` struct snippet): update field types from `SecretBox<[u8; 32]>` to the new `SecureBytes<32>` container. Keep the `#[derive(ZeroizeOnDrop)]` line if the final implementation uses it; remove if the implementation uses manual `Drop`. Resolution for DC-1.

3. **`docs/architecture/designs/tauri-ipc-and-frontend/sub-phases/roadmap.md`** line 53: change `AuthError` → `AuthenticationError` in the "`From` impls" bullet. Resolution for DC-4. This is a forward-looking doc fix; no Phase 6.1 plan exists yet.

4. **`.claude/plans/phase-2-2-argon2id-and-session-keys.md`** (this file) — status moves from `draft` to `approved` after user review, then to `implemented` after verify passes.
