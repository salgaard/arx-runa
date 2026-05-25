# Security Review Session Prompts

Eight sessions, run in order. Plan file: `.claude/plans/review-security-flows.md`.

**Date placeholder**: replace `YYYYMMDD` with today's date before running each session.

**Starting symbols are entry points, not scope boundaries.** Every prompt includes an "Enumerate before checking" block. Run those steps first — they build the complete set of relevant call sites across the whole codebase. Any site not in the listed starting symbols is still in scope if enumeration finds it.

**Design discrepancy rule** (applies to every session): if code deviates from what the design doc specifies for this flow — even if the code looks more correct — record it as a `[DESIGN-GAP]` note rather than a finding. Format:
```
[DESIGN-GAP] Short description
Design says: …
Code does: …
Which is correct: code / design / unclear
```
This keeps findings reserved for genuine invariant violations and prevents design doc drift going unnoticed.

---

## Flow A — Key Derivation & Session Memory Lifecycle

```
Review Flow A from `.claude/plans/review-security-flows.md`.
Write findings to `.claude/reviews/review-flow-a-YYYYMMDD.md`.
Use jcodemunch for all navigation. Do not commit anything.

Enumerate before checking:
1. find_references on SqlCipherMetadataStore::open — build every call site across the whole
   codebase; each must pass expose() directly with no intermediate `let key_bytes` / `with_exposed`
   binding. This is Invariant 16 and applies to ALL call sites regardless of which file they are in.
2. search_text {"query": "with_exposed|\.expose\(\)"} across src-tauri/src/ with context_lines=3 —
   any result that assigns the return value to a local binding before .await is a candidate violation.
3. search_text {"query": "expand_vault_key_into|expand_into_secret_box"} — find every HKDF
   expansion call site; each must use the canonical salt and info constants (Invariant 3).
4. find_references on SecureBytes::new — every construction site should be an mlocked allocation;
   any skipped site is an Invariant 17 candidate.

Then navigate the listed starting symbols for Invariants 3 and 17 as described in the plan.
If code deviates from the design doc, record it as [DESIGN-GAP] — see preamble.
```

---

## Flow C — IPC Boundary & Zero-Trace

```
Review Flow C from `.claude/plans/review-security-flows.md`.
Write findings to `.claude/reviews/review-flow-c-YYYYMMDD.md`.
Use jcodemunch for all navigation. Do not commit anything.

Enumerate before checking:
1. search_text {"query": "#\\[tauri::command\\]"} across src-tauri/src/ui/ — build the complete,
   authoritative list of IPC commands; cross-reference with the invoke_handler in lib.rs; any
   command not in the listed starting files is still in scope for all Invariant 6 and 7 checks.
2. search_text {"query": "password|passphrase|master_key|sqlcipher_key|manifest_key"} across
   src-tauri/src/ui/ with context_lines=2 — any hit inside a tracing:: macro or error struct
   field is a critical Invariant 7 candidate.
3. search_text {"query": "tracing::|log::"} in src-tauri/src/ — scan for log macros that include
   variable interpolation of session or key identifiers.

The password-to-Zeroizing conversion is centralised in sanitise_password
(src-tauri/src/ui/commands_common.rs) — verify ALL enumerated handlers call it rather than
converting inline.
If code deviates from the design doc, record it as [DESIGN-GAP] — see preamble.
```

---

## Flow G — UI Zero-Trace & Frontend Security

```
Review Flow G from `.claude/plans/review-security-flows.md`.
Write findings to `.claude/reviews/review-flow-g-YYYYMMDD.md`.
Use jcodemunch for all navigation. Do not commit anything.

Enumerate before checking:
1. search_text {"query": "invoke("} across frontend src/ — build the complete list of IPC calls
   the frontend makes; cross-reference each with the backend command registration in lib.rs; any
   call to a command not in the listed starting files is still in scope for data-minimisation and
   error-sanitisation checks.
2. find_references on VaultActions — enumerate every component that holds reactive state; confirm
   each calls VaultActions::clear() on lock, or explicitly documents why it is exempt. A component
   that skips it is an Invariant 7 candidate.
3. search_text {"query": "create_rw_signal|create_signal"} across src/ — find all reactive signals
   holding sensitive data (passwords, keys, file content) that are not explicitly cleared on lock.

Do not re-verify properties already covered by security_audit.rs tests — only check their scope
and the listed gaps.
If code deviates from the design doc, record it as [DESIGN-GAP] — see preamble.
```

---

## Flow F — Zero-Knowledge Boundary

```
Review Flow F from `.claude/plans/review-security-flows.md`.
Write findings to `.claude/reviews/review-flow-f-YYYYMMDD.md`.
Use jcodemunch for all navigation. Do not commit anything.

Enumerate before checking:
1. find_references on all rclone copy/upload primitive functions — every call site is a potential
   plaintext boundary crossing; any site not traced through an encryption step first is a critical
   ZK violation. Do not assume only the primary entry point matters.
2. search_text {"query": "staging|\\.tmp|tempfile|NamedTempFile"} across src-tauri/src/storage/
   — find every staging-area write; each must write only ciphertext; a plaintext write before AEAD
   is a critical violation.
3. find_references on strip_exif — must be called at every encrypt-file entry point; any code path
   through encryption that does not call it for supported formats is a ZK metadata leak.
4. search_text {"query": "blob_name|chunk_name|object_key|remote_path"} — find all sites that
   construct the cloud object name; each must produce an opaque identifier, not a derivative of
   the original filename or plaintext content.

Note: upload_vault_header uploads a plaintext JSON blob by design — the vault header is
intentionally public (Argon2 params + wrapped key slots only). The check is about its contents,
not that it is encrypted.
If code deviates from the design doc, record it as [DESIGN-GAP] — see preamble.
```

---

## Flow D — Cloud Sync & Rclone Subprocess

```
Review Flow D from `.claude/plans/review-security-flows.md`.
Write findings to `.claude/reviews/review-flow-d-YYYYMMDD.md`.
Use jcodemunch for all navigation. Do not commit anything.

Enumerate before checking:
1. find_references on the rclone subprocess invocation function (the one that actually spawns
   rclone) — every caller is a command-injection check target; any call site that builds arguments
   via string formatting rather than an argument list is a critical finding.
2. find_references on the vault path validation function — every code path that accepts a
   user-supplied vault-relative path must flow through it; callers that don't are path-traversal
   candidates.
3. search_text {"query": "pending_deletions"} across src-tauri/src/storage/ — enumerate every
   write site; each must follow the required transaction order: read blob names → enqueue
   pending_deletions → delete node (CASCADE) → commit → delete local staging blobs.
4. search_text {"query": "oauth|access_token|refresh_token|rclone.*password|config.*secret"} across
   src-tauri/src/ with context_lines=2 — any hit outside an encrypted store or guarded temp file
   is an Invariant 7 candidate.

Start with get_file_outline on src-tauri/src/storage/cloud/rclone.rs (transport layer);
rclone_subprocess.rs is the runner underneath it — check both for credential delivery and
command-injection checks.
If code deviates from the design doc, record it as [DESIGN-GAP] — see preamble.
```

---

## Flow B — AEAD Encrypt/Decrypt & Chunk Pipeline

```
Review Flow B from `.claude/plans/review-security-flows.md`.
Write findings to `.claude/reviews/review-flow-b-YYYYMMDD.md`.
Use jcodemunch for all navigation. Do not commit anything.

Enumerate before checking:
1. find_references on encrypt_chunk and the decrypt equivalent — every call site is an AAD and
   nonce check target; a site not reachable from the listed starting symbols is still in scope.
2. search_text {"query": "encrypt_in_place|decrypt_in_place|XChaCha20Poly1305"} across
   src-tauri/src/ — find any raw AEAD usage that bypasses the encrypt_chunk/decrypt_chunk wrappers;
   each is a critical Invariant 1 and 2 candidate.
3. find_references on generate_nonce — must be the single nonce generation path; any second nonce
   source is a critical Invariant 2 violation.
   Note: OsRng is abstracted behind generate_nonce() in src-tauri/src/crypto/nonce.rs — do not
   search for OsRng directly, it will return no results.

If code deviates from the design doc, record it as [DESIGN-GAP] — see preamble.
```

*If context fills before the decrypt path: start a second session with the same prompt but add "focus on the decrypt path only — the encrypt path was reviewed in a prior session."*

---

## Flow E — Auth Ceremonies

```
Review Flow E from `.claude/plans/review-security-flows.md`.
Write findings to `.claude/reviews/review-flow-e-YYYYMMDD.md`.
Use jcodemunch for all navigation. Do not commit anything.

Enumerate before checking:
1. search_text {"query": "INSERT.*vault_identity|vault_identity.*INSERT"} across src-tauri/src/
   — any INSERT outside create_vault is a critical Invariant 13 violation; there must be exactly one.
2. find_references on the tier KDF construction function — all ceremonies (create, unlock,
   change-password, recover, rotate_key_file) must use the same function; a ceremony that calls
   its own inline derivation is a critical Invariant 15 violation.
3. find_references on wrap_master_key_for_recovery — only create_vault and rotate_key_file should
   call it; any additional caller is an Invariant 14 candidate.
4. search_text {"query": "recovery_phrase|mnemonic"} across src-tauri/src/ — any persistence or
   log hit is a critical Invariant 14 violation.

Before starting, read the summary section of `.claude/reviews/review-flow-a-YYYYMMDD.md` so you
do not re-derive what Flow A already established about key handling.
unlock_vault is in src-tauri/src/auth/ceremonies/unlock.rs.
If code deviates from the design doc, record it as [DESIGN-GAP] — see preamble.
```

---

## Flow H — File Sharing HPKE & Key Isolation

```
Review Flow H from `.claude/plans/review-security-flows.md`.
Write findings to `.claude/reviews/review-flow-h-YYYYMMDD.md`.
Use jcodemunch for all navigation. Do not commit anything.

Enumerate before checking:
1. find_references on the HPKE seal function — must be the single HPKE encryption path; any
   second seal call site means the ciphersuite and info/aad checks must also apply there.
2. find_references on the file_key type across src-tauri/src/sharing/ — every site that holds a
   file_key value must keep it in a Zeroizing-wrapped binding or pass it directly into an
   encryption call; any intermediate bare copy is an Invariant 11 violation.
3. find_references on replace_file_key_and_chunks (strong revocation) — the old file_key must be
   zeroized immediately after it returns at every call site.
4. search_symbols {"kind": "function", "pattern": "wrap_file_key|unwrap_file_key"} — confirm none
   are used where wrap_master_key_for_recovery is required (recovery slot context), and vice versa.

Before starting, read the summary section of `.claude/reviews/review-flow-b-YYYYMMDD.md` — Flow H
shares AEAD and CTX concepts with the chunk pipeline.
HPKE is implemented across src-tauri/src/sharing/hpke.rs and src-tauri/src/sharing/ctx_aead.rs.
Text search for "hpke", "HPKE", "arx-runa-ctx", or "CTX" returns no results — use search_symbols
and get_file_outline instead.
If code deviates from the design doc, record it as [DESIGN-GAP] — see preamble.
```
