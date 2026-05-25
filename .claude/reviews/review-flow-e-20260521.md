---
flow: E
date: 2026-05-21
reviewer: claude-sonnet-4-6
invariants: 13, 14, 15
status: complete
prior_session_context: >
  Flow A confirmed: HKDF constants clean; SessionKeys mlock/zeroize correct;
  master_key dropped immediately after SessionKeys::from_master_key_bytes in
  create_vault and authenticate. One high finding (FLOW-A-001): redundant
  Zeroizing<[u8; 32]> sqlcipher_key copy in create_vault and
  finalize_session_install. The Zeroizing<[u8; 32]>-on-async-stack pattern is
  the established weak point in this codebase.
---

# Flow E — Auth Ceremonies (Vault Create / Unlock / Recover)

## Findings

### [FLOW-E-001] `rotate_key_file`: `current_master_key` not dropped after session keys are derived
**Severity**: medium
**Invariant**: 17 (key material lifetime)
**Location**: `src-tauri/src/auth/ceremonies/rotate_key_file.rs:63–168`

**Observation**: `current_master_key: Zeroizing<[u8; 32]>` is derived then immediately used to build `current_session_keys` via `SessionKeys::from_master_key_bytes`. `drop(current_session_keys)` is called right after the KEK and sqlcipher key are extracted — releasing the mlocked memory. But `current_master_key` itself is NOT dropped at that point. It survives until the end of the function, across `spawn_blocking` (rekey_vault_db), `swap_active_session`, `upload_vault_header`, and `upload_manifest_backup`.

**Violation**: The 32-byte master key buffer lingers on the async stack in non-mlocked memory through multiple network suspension points after its last use. `create_vault` and `change_password` both call `drop(master_key)` / `drop(current_master_key)` immediately after `SessionKeys::from_master_key_bytes` returns — `rotate_key_file` does not follow this pattern.

**Recommendation**: Add `drop(current_master_key);` immediately after `drop(current_session_keys);` (before generating the new key file):
```rust
drop(current_session_keys);
drop(current_master_key); // add this
```

**Test coverage**: none

---

### [FLOW-E-002] `rotate_key_file` and `recover_with_phrase`: `new_master_key` held across async network calls after last use
**Severity**: medium
**Invariant**: 17 (key material lifetime)
**Location**:
- `src-tauri/src/auth/ceremonies/rotate_key_file.rs:128–168`
- `src-tauri/src/auth/ceremonies/recover_with_phrase.rs:185–232`

**Observation**: In both ceremonies, `new_master_key: Zeroizing<[u8; 32]>` is last used for `master_key_from_array(&new_master_key)` (recovery slot rewrap). After `drop(master_key_typed)` that borrow is over and `new_master_key` is no longer needed. It is then held across:

- `rotate_key_file`: `swap_active_session` (installs new session), `upload_vault_header` (network), `upload_manifest_backup` (network)
- `recover_with_phrase`: `upload_vault_header` (network), `upload_manifest_backup` (network), `finalize_session_install`

The same pattern is present in `change_password.rs` — only partially checked here (lines 1–120), but the recovery-slot rewrap block follows an identical structure.

**Violation**: `new_master_key` is a `Zeroizing<[u8; 32]>` (non-mlocked stack/future memory) that outlives its last use by the duration of two or three async suspension points. The mlocked `new_session_keys` is what carries the derived keys forward; there is no reason to retain the raw master key buffer beyond the recovery slot rewrap.

**Recommendation**: Drop `new_master_key` immediately after `drop(master_key_typed)` in each ceremony. Before:
```rust
    drop(master_key_typed);
    // ...header mutation, vault_header upload, manifest upload, finalize...
drop(new_master_key);  // at end
```
After:
```rust
    drop(master_key_typed);
    drop(new_master_key);  // drop here — new_session_keys carries keys forward
    // ...header mutation, uploads, finalize...
```
Apply the same fix to `change_password.rs` after verifying the last use of `new_master_key` there.

**Test coverage**: none

---

## Design Gaps

None new for this flow. The `SecretBox<[u8; 32]>` (non-mlocked) transient-copy pattern flagged in the Flow A design gap applies to `key_encryption_key_from_array` calls throughout ceremonies (KEK extracted from mlocked `SessionKeys` into a `SecretBox` for the `spawn_blocking` closure). This is the accepted protection level per the `SqlcipherKey::from_slice` docstring and the Flow A design-gap assessment. Not re-raised here.

---

## Confirmed invariants (no findings)

| Check | Result | Notes |
|---|---|---|
| `create_vault` inserts exactly one `vault_identity` row (`id = 1`) | **PASS** | `INSERT INTO vault_identity (id, public_key, wrapped_private_key) VALUES (1, ?, ?)` — hardcoded id, single call inside `spawn_blocking` |
| No path allows two `vault_identity` rows | **PASS** | No UPSERT, no second INSERT path; `SessionAlreadyActive` guard fires before DB open |
| `rekey_vault_db` uses `UPDATE vault_identity SET wrapped_private_key = ? WHERE id = 1` only | **PASS** | `helpers.rs:432` — UPDATE only; no INSERT or DELETE on `vault_identity` |
| `rotate_key_file` re-wraps `wrapped_private_key`, not inserts/deletes | **PASS** | Delegates to `rekey_vault_db` (same function used by change_password and recover_with_phrase) |
| Recovery phrase returned to UI exactly once in `Zeroizing<String>` | **PASS** | `setup_recovery` returns phrase wrapped in `Zeroizing<String>`; phrase bytes never appear in `RecoverySlot` fields, never logged; `test_recovery_phrase_never_appears_in_any_persistent_writer_output` verifies against vault header and DB |
| Recovery phrase never written to any store or log | **PASS** | `phrase_string: Zeroizing<String>` is the only binding; canonical form goes to `derive_recovery_key_into` which writes only the derived key, not the phrase bytes |
| Recovery slot AAD is `b"arx-runa recovery v1" \|\| vault_id_bytes` | **PASS** | `recovery_wrap.rs:26`: `const AAD_PREFIX: &[u8] = b"arx-runa recovery v1"` — 20 bytes + 16-byte `vault_id`; `build_aad` assembles the fixed-size buffer; `test_wrap_recovery_uses_non_empty_aad_scope_separation_from_file_key` exercises this |
| BIP-39 PBKDF2 derivation step intentionally bypassed | **PASS** | `derive_recovery_key_into` (`helpers.rs:278`) calls `derive_master_key_into(phrase_canonical_bytes, None, ...)` directly — pure Argon2id; no PBKDF2 step; confirmed by the function body |
| Argon2id params for recovery slot match primary slot | **PASS** | `setup_recovery` uses `current_params` read from `vault_header.argon2_params` (the current primary slot params); `test_setup_recovery_preserves_trusted_non_default_argon2_params_without_migration` covers the non-default param case |
| `recover_with_phrase` is a single atomic ceremony — no intermediate session | **PASS** | `finalize_session_install` called at the very end; no session state change until all rewraps and uploads are complete |
| `recover_with_phrase` uses `wrap_master_key_for_recovery` (not `wrap_file_key`) for recovery slot | **PASS** | `recover_with_phrase.rs:185`: `wrap_master_key_for_recovery(&new_master_key_typed, recovery_key, &vault_id)` |
| `rotate_key_file` uses `wrap_master_key_for_recovery` for recovery slot | **PASS** | `rotate_key_file.rs:133`: `wrap_master_key_for_recovery(&master_key, recovery_key, vault_id)` |
| Tier 1 KDF input = password bytes only | **PASS** | `kdf.rs:51`: `combined_input = password_bytes` (key_file_bytes is None); all tier-1 ceremony calls pass `None` / `key_file_bytes.as_deref()` with None |
| Tier 2 KDF input = password bytes `\|\|` exactly 32 key-file bytes | **PASS** | `kdf.rs:54`: `combined_input.extend_from_slice(bytes)` where `bytes: &[u8; 32]`; all tier-2 ceremony calls pass `Some(&[u8; 32])` |
| All ceremonies using `derive_master_key_into` follow same tier construction | **PASS** | `create_vault` ✓, `unlock_vault` (via `authenticate`) ✓, `recover_with_phrase` ✓, `rotate_key_file` ✓, `change_password` ✓ — all use the shared `derive_master_key_into(pw, key_file_opt, salt, params, out)` |
| Auth failure responses do not distinguish wrong password from wrong key file | **PASS** | All ceremonies surface `InvalidCredentials` on decryption failure regardless of which input was wrong; `KeyFileNotFound` is returned only when the path does not exist (OS-level absence), not when bytes are wrong — this is a UX affordance, not a crypto oracle |
| `drop()` ordering in `rotate_key_file` does not hold a `master_key` copy in a moved closure across network calls | **PASS** | `current_kek` and `new_kek` (SecretBox, heap) are moved into `spawn_blocking` closure and dropped inside it; neither is held as a borrow across the subsequent `upload_vault_header` or `upload_manifest_backup` async suspension points (see FLOW-E-001/002 for the raw `Zeroizing<[u8; 32]>` master key buffer issue — separate from this check) |

---

## Summary

| Severity | Count |
|---|---|
| Critical | 0 |
| High | 0 |
| Medium | 2 |
| Low | 0 |

**Invariant 13** (single vault_identity): fully confirmed — create inserts once with `id = 1`; rekey/rotate use UPDATE only; no duplicate-row path exists.

**Invariant 14** (recovery slot): fully confirmed — phrase returned once in `Zeroizing<String>`, AAD constant is correct, PBKDF2 bypass is in place, params match primary slot, rewrap uses `wrap_master_key_for_recovery` in all ceremonies.

**Invariant 15** (tier input construction & non-oracular failure): fully confirmed — all five ceremonies use the shared `derive_master_key_into`; tier-1/tier-2 input construction is correct; auth failures are non-oracular.

The two medium findings ([FLOW-E-001], [FLOW-E-002]) are extensions of the `Zeroizing<[u8; 32]>` late-drop pattern already identified as [FLOW-A-001]. `rotate_key_file` fails to drop `current_master_key` early, and both `rotate_key_file` and `recover_with_phrase` hold `new_master_key` across async network calls past the last use. All three are one-line or two-line fixes. No follow-up session required; the fixes are straightforward and low-risk.
