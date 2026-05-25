---
title: "Flow H — File Sharing HPKE & Key Isolation"
date: "2026-05-21"
reviewer: "claude-sonnet-4-6"
invariants: [11, 12]
additional_spec: ["REQ-CRYPTO-016", "sub-phase 5.2"]
status: partial — revocation.rs not reviewed (see note at end)
---

# Flow H Security Review — File Sharing HPKE & Key Isolation

**Reviewed**: 2026-05-21  
**Invariants in scope**: 11 (share package key-handling contract), 12 (share revocation semantics)  
**Additional spec**: REQ-CRYPTO-016 (CTX-ChaCha20-Poly1305 with BLAKE3 CMT-4 commitment), sub-phase 5.2  
**Starting symbols**: `seal`, `open` (`hpke.rs`); `ctx_seal`, `ctx_open` (`ctx_aead.rs`); `create_share_package`, `import_share_package` (`packages.rs`); `revocation.rs` — **not reviewed** (token budget exhausted; see §Incomplete Coverage)

---

## Findings

Two medium findings on transient heap exposure of the base64 file key, and one low informational note on the keystream-offset skip in `ctx_open`.

---

### [FLOW-H-001] `file_key` base64 String not zeroized after JSON serialisation in `create_share_package`
**Severity**: medium  
**Invariant**: 11 (file_key must not appear outside the HPKE-encrypted envelope)  
**Location**: `src-tauri/src/sharing/packages.rs:112`  
**Observation**: `file_key_base64` is a plain `String` inserted into `SharePackagePayload`. The JSON byte vector is correctly wrapped in `Zeroizing`:

```rust
let plaintext = Zeroizing::new(serde_json::to_vec(&payload)...);
```

But `payload` itself — including its `file_key: String` field holding the base64 encoding of the raw 32-byte key — is an ordinary struct. When `payload` drops at end of scope (after `serde_json::to_vec` returns), its heap allocation is freed without zeroize. A memory scanner or heap dump between serialisation and `payload` drop sees the base64 file key bytes unwiped.

`file_key.with_exposed(...)` is correct and brief; the raw `[u8; 32]` never appears in a named unprotected variable. But the base64 representation is a recoverable encoding of those bytes.  
**Violation**: The base64 representation of `file_key` is transiently recoverable from process heap during and after serialisation, until the allocator reuses the memory. Invariant 11 requires the file key to not appear outside the HPKE-encrypted envelope; a heap dump taken in this window can extract it without breaking HPKE.  
**Recommendation**: Derive a `ZeroizingPayload` wrapper (or mark `SharePackagePayload` `#[derive(Zeroize)]` and call `.zeroize()` explicitly before dropping), or move the `file_key_base64` String into a `Zeroizing<String>` field. At minimum, an explicit `drop(payload)` immediately after `serde_json::to_vec` narrows the window to near-zero. The cleanest approach is a `SharePackagePayload` that implements `Zeroize` via `#[derive(Zeroize)]` so that `let plaintext = Zeroizing::new(serde_json::to_vec(&payload)?); drop(payload);` is idiomatic and safe.  
**Test coverage**: none.

---

### [FLOW-H-002] `payload.file_key` base64 String not zeroized after `decode_file_key` in `import_share_package`
**Severity**: medium  
**Invariant**: 11  
**Location**: `src-tauri/src/sharing/packages.rs:155–160`  
**Observation**: On import, `payload.file_key` (a `String` containing base64) is decoded into `file_key_bytes: Zeroizing<[u8; 32]>`. The raw 32 bytes are correctly zeroized on drop. However, `payload.file_key` remains in the deserialized struct — which is a plain `SharePackagePayload` with no zeroize — and it persists on the heap until `payload` drops at the end of the function. The same issue as FLOW-H-001 but on the receive path.

Note that `plaintext` (`Zeroizing<Vec<u8>>`) — the HPKE-decrypted JSON bytes — is also held alive alongside `payload` for the full function body. Together they mean both the raw JSON and the parsed base64 string reside simultaneously in unprotected heap memory.  
**Violation**: Same as FLOW-H-001: base64-encoded file key recoverable from heap until allocator reuse.  
**Recommendation**: After `decode_file_key`, explicitly zeroize or drop `payload.file_key`. A simple `payload.file_key.zeroize()` (if `String` implements `Zeroize`, which it does via the `zeroize` crate's blanket impl) immediately after decoding is sufficient. The broader fix is `#[derive(Zeroize)]` on `SharePackagePayload` as described in FLOW-H-001.  
**Test coverage**: none.

---

### [FLOW-H-003] `ctx_open` keystream block skip relies on undocumented ChaCha20-Poly1305 block-counter alignment
**Severity**: low (informational)  
**Invariant**: design sanity (REQ-CRYPTO-016 correctness)  
**Location**: `src-tauri/src/sharing/ctx_aead.rs:75–78`  
**Observation**: On open, the CTX commitment is verified first (correct), then decryption proceeds by applying the raw XChaCha20 keystream directly rather than calling `XChaCha20Poly1305::decrypt_in_place_detached`. To align with the keystream used during encryption (which starts at block counter 1, because counter 0 is consumed generating the Poly1305 key), 64 bytes are discarded:

```rust
let mut stream = chacha20::XChaCha20::new(key.into(), nonce.into());
let mut discard = Zeroizing::new([0u8; 64]);
stream.apply_keystream(discard.as_mut_slice());
stream.apply_keystream(ciphertext);
```

This is correct. XChaCha20Poly1305 uses the first 64 bytes of the keystream (counter 0) as Poly1305 key material; the actual plaintext encryption starts at counter 1 (byte offset 64). The 64-byte discard is the right alignment.

However, this invariant is load-bearing and undocumented. The comment in the module-level doc explains that the Poly1305 tag is discarded and never serialised, but it does not explain why 64 bytes are skipped rather than 32 (the actual Poly1305 key size) or 0.  
**Violation**: None — the alignment is correct. The risk is future maintainers misreading this as an arbitrary discard and adjusting it to 0 or 32 bytes, silently corrupting decryption (with no panic, since CTX verification has already passed).  
**Recommendation**: Add a one-line comment:  
```rust
// Skip ChaCha20 block 0 (64 bytes): XChaCha20Poly1305 uses it for the
// Poly1305 key; encryption starts at block 1.
stream.apply_keystream(discard.as_mut_slice());
```  
Also add a known-answer test (a fixed key/nonce/plaintext triple) that exercises `ctx_seal` → `ctx_open` and verifies the decrypted bytes exactly, anchoring the 64-byte skip against a fixed keystream vector.  
**Test coverage**: `test_ctx_aead_seal_open_round_trip_recovers_plaintext` covers correctness of the round trip but uses random key/nonce — a vector test would catch offset bugs that happen to cancel out in round-trip tests.

---

## Invariants Confirmed — No Findings

| Check | Result |
|---|---|
| HPKE ciphersuite is `DHKEM(X25519, HKDF-SHA256) + HKDF-SHA256 + CTX-ChaCha20-Poly1305` | ✅ — module doc, `SUITE_ID` constants, and `key_schedule` all confirm. `SUITE_ID = [HPKE, 0x0020, 0x0001, 0x0003]` with a deliberate note that CTX makes it non-interoperable with standard implementations |
| CTX BLAKE3 domain string is exactly `b"arx-runa-ctx-v1"` | ✅ — `CTX_DOMAIN_LABEL` at `ctx_aead.rs:24` |
| BLAKE3 commitment covers key + nonce + full ciphertext (CMT-4) | ✅ — `compute_commitment`: `BLAKE3(domain_label \|\| key \|\| nonce \|\| ciphertext)` at `ctx_aead.rs:34–40`; called after `encrypt_in_place_detached` so `plaintext` argument already holds ciphertext |
| HPKE `info = b"arx-runa-share"` | ✅ — `HPKE_SHARE_INFO` constant at `hpke.rs:32`; used in `key_schedule` `info_hash` derivation |
| HPKE `aad = b""` | ✅ — `ctx_seal(&key, &base_nonce, &mut buffer)` passes no AAD; `encrypt_in_place_detached(nonce_ga, &[], plaintext)` at `ctx_aead.rs:52` |
| `master_key`, `sqlcipher_key`, `manifest_key` absent from share payload | ✅ — `SharePackagePayload` contains only: `share_id`, `file_id`, `file_name`, `chunk_count`, `chunk_size`, `chunk_uuids`, `file_key` (base64), `sender_public_key`, `cloud_endpoint`, `file_size`, `expires_at` |
| All three HPKE failure paths surface as `AuthenticationFailed` | ✅ — confirmed by three dedicated test functions: `test_hpke_wrong_recipient_rejected_with_authentication_failed`, `test_hpke_corrupted_ciphertext_rejected_with_authentication_failed`, `test_hpke_corrupted_enc_rejected_with_authentication_failed`. Short-wire returns `MalformedSharePackage` (structural, not a crypto oracle) |
| `file_key` raw 32 bytes zeroized after wrapping on import | ✅ — `file_key_bytes: Zeroizing<[u8; 32]>` at `packages.rs:155`; zeroized on drop at end of scope |
| Only the selected file's `file_key` is in the share package | ✅ — `create_share_package` reads `file_id` → single node → single `file_key_wrapped`; no iteration over other file keys |
| DH low-order point check | ✅ — `kem_encap` and `kem_decap` both reject an all-zero DH output via constant-time compare and return `AuthenticationFailed` |
| CTX commitment verified before decryption in `ctx_open` | ✅ — `expected_tag` computed before any keystream application; decryption only reached if `ct_eq` passes |

---

## Revocation.rs Review — Session 2 (2026-05-21)

Symbols enumerated: 25 (all symbols in `revocation.rs`). Both entry-points read directly. `reencrypt_file` in `src-tauri/src/storage/vault_ops/reencrypt_file.rs` read for dependency verification (fresh key generation, zeroization, atomicity).

### Checklist

| Check | Result |
|---|---|
| Default revocation: marks `revoked_at` AND removes cloud blobs with correct failure ordering | ✅ — blobs deleted first; `set_share_revoked_at` called only after all deletions succeed |
| Default revocation: blob removal failure does not leave `revoked_at` permanently unset | ✅ — returns `RevocationPartial { failed_index }`; docstring and tests confirm retryability |
| Strong revocation: generates a fresh `file_key` | ✅ — `reencrypt_file` calls `generate_file_key()` before `replace_file_key_and_chunks` |
| Strong revocation: `replace_file_key_and_chunks` in single transaction that enqueues old blob names into `pending_deletions` | ❌ — vault blobs are atomic; old **shared** blob paths are enqueued in a separate call after upload — see FLOW-H-004 |
| Strong revocation: re-publishes under new `file_share_id`; old shared path retired | ✅ — `new_file_share_id = Uuid::new_v4()`; new `ShareRecord` rows inserted; old shares marked revoked |
| Strong revocation: old `file_key` zeroized immediately after re-encryption transaction commits | ✅ — `old_file_key: FileKey` is a stack-local in `reencrypt_file`; drops when that function returns (right after `replace_file_key_and_chunks`); `test_file_key_zeroize_trait_clears_memory` confirms `FileKey` implements `ZeroizeOnDrop` |
| Strong revocation: new packages issued only AFTER re-encryption transaction commits | ✅ — new `ShareRecord` rows inserted (step 3) before old shares revoked (step 6); no window where old path is retired but new is not live |
| Strong revocation uses `wrap_master_key_for_recovery` for recovery slot, NOT `wrap_file_key` | ✅ / N/A — no per-file recovery slot re-wrap in the revocation path; recovery operates at master-key level via `wrap_master_key_for_recovery` in a separate ceremony; calling it here would be incorrect |
| `file_key` does not appear in tracing/log output anywhere in revocation.rs | ✅ — full-text search for `tracing`, `debug!`, `info!`, `warn!`, `error!`, `println!` in revocation.rs returned zero matches |

---

### [FLOW-H-004] Old shared blob paths enqueued outside the `replace_file_key_and_chunks` transaction — non-idempotent retry cascade

**Severity**: high  
**Invariant**: 12  
**Location**: `src-tauri/src/sharing/revocation.rs:232–239` (the `enqueue_pending_deletions` call in `strong_revoke_share`)

**Observation**: `strong_revoke_share` follows this sequence:

1. `reencrypt_file(...)` — atomically commits new file key + new vault chunk records into `replace_file_key_and_chunks`; old vault blob names are enqueued in `pending_deletions` inside that same transaction.
2. Upload new vault and shared blobs to cloud.
3. Create new `ShareRecord` rows for remaining recipients (separate DB writes).
4. `sqlcipher_store.enqueue_pending_deletions(&old_shared_blob_paths, ...)` — **separate DB call**.
5. Best-effort immediate deletion of old shared blobs.
6. `set_share_revoked_at` for all active shares.

`old_shared_blob_paths` is computed from the chunk records fetched before `reencrypt_file` runs, then enqueued at step 4 — a second, independent DB write that is not wrapped in the step-1 transaction.

**Violation**: If step 4 fails (e.g. DB error), the new file key is already committed (step 1), new blobs are uploaded (step 2), new `ShareRecord` rows exist (step 3), but the old shared blob paths are not durably queued for deletion. The function returns an error. On retry, `share.revoked_at` is still `None` so execution proceeds; `reencrypt_file` is called again and generates a **third** file key, overwriting step 1's result. The blobs uploaded in step 2 of the first attempt are now orphaned (not referenced by any chunk record and not in `pending_deletions`). The new `ShareRecord` rows from step 3 of the first attempt also become orphaned (they reference `new_file_share_id` from attempt 1, which no longer maps to any current chunk data). Each retry compounds the orphan accumulation. Invariant 12 is at risk: orphaned `ShareRecord` rows with `revoked_at = None` may be returned to callers as active shares even though the underlying data has been superseded.

**Recommendation**: Move `enqueue_pending_deletions` for `old_shared_blob_paths` inside `reencrypt_file` (or a new `reencrypt_file_for_revocation` variant) so it executes atomically within the same `replace_file_key_and_chunks` transaction. Alternatively, introduce an idempotency guard (e.g. a `strong_revocation_in_progress` marker row or state column on the file node) that is set atomically with `replace_file_key_and_chunks` and cleared at step 6; on retry, the guard prevents a second `reencrypt_file` call and resumes from the last completed step.  
**Test coverage**: `test_strong_revoke_share_on_list_blobs_failure_enqueues_old_shared_blobs_to_pending_deletions` at line 737 tests the list-blobs failure path but does not exercise a failure of `enqueue_pending_deletions` itself, so the cascade is untested.

---

### [FLOW-H-005] Best-effort immediate blob deletion runs before `set_share_revoked_at`, creating an availability inconsistency window

**Severity**: medium  
**Invariant**: 12  
**Location**: `src-tauri/src/sharing/revocation.rs:241–248` (immediate deletion loop), `src-tauri/src/sharing/revocation.rs:250–261` (`set_share_revoked_at` loop)

**Observation**: After `enqueue_pending_deletions` succeeds, `strong_revoke_share` attempts immediate deletion of old shared blobs (best-effort, errors silently ignored via `let _ = ...`). This runs before `set_share_revoked_at` is called for the old shares. If one or more immediate deletions succeed and a subsequent `set_share_revoked_at` call fails, some old share records remain with `revoked_at = None` (appearing active) while their cloud blobs are partially or fully gone.

**Violation**: A recipient holding a share that is in this inconsistent state sees it as active but cannot download — the blobs are gone from the cloud path their share references. From a confidentiality standpoint Invariant 12 is satisfied (data inaccessible), but the share record contracts in sub-phase 5.2 require `revoked_at` to accurately reflect the revocation state; leaving it `None` while the data is deleted violates that contract and can cause silent download failures that are not attributable to revocation.

**Recommendation**: Reorder: call `set_share_revoked_at` for all old shares **before** attempting immediate deletion. Marking shares revoked first ensures the DB state is always at least as conservative as the cloud state. The immediate deletion optimization is then a best-effort post-revocation cleanup, not a pre-revocation operation. `enqueue_pending_deletions` (step 4) already guarantees eventual cleanup, so moving immediate deletion after `set_share_revoked_at` does not weaken the cleanup guarantee.  
**Test coverage**: none for this ordering property.

---

## Summary

*(Cumulative across both sessions — Sessions 1 and 2)*

| Severity | Count |
|---|---|
| Critical | 0 |
| High | 1 |
| Medium | 3 |
| Low | 1 |

**Invariants confirmed with no findings**: REQ-CRYPTO-016 (ciphersuite, domain label, CMT-4 commitment ordering), Invariant 11 (key isolation — structurally; two medium findings on transient heap exposure), sub-phase 5.2 (oracle-free error paths).

**Invariant 12** (revocation semantics): default revocation path is clean; strong revocation has one high finding (FLOW-H-004 — non-atomic shared blob enqueue, non-idempotent retry cascade) and one medium finding (FLOW-H-005 — immediate deletion before `revoked_at` write).

**Flow H is now fully read.** No further follow-up session is required. Resolution priority: FLOW-H-004 (high) → FLOW-H-001/FLOW-H-002 (shared root cause, fix together) → FLOW-H-005 (medium) → FLOW-H-003 (low, comment-only fix).
