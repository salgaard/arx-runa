# Arx Runa Implementation Roadmap

Arx Runa is built in ten sequential phases, each delivering a distinct, self-contained piece of the system. The phases are ordered by dependency: cryptographic foundations come first, then authentication, then storage, cloud sync, sharing, and finally the user interface and testing.

| Phase | What gets built | Status |
|-------|----------------|--------|
| 0 — Scaffolding | Project structure, build pipeline, CI | Planned |
| 1 — Cryptographic Primitives | Encryption, key derivation, chunk encryption/decryption | Planned |
| 2 — Authentication & Session | Login flow, USB key file, session lifecycle and timeout | Planned |
| 3 — Storage & Chunking | File splitting, local encrypted database, blob staging | Planned |
| 4 — Cloud Synchronisation | Rclone integration, upload/download, new-device recovery | Planned |
| 5 — File Sharing | Per-file sharing via encrypted share packages, revocation | Planned |
| 6 — Tauri IPC & Frontend | User interface, backend commands, error handling | Planned |
| 7 — Integration Testing | End-to-end tests covering all modules and adversarial scenarios | Planned |
| 8 — Threat Model & Report | Formal threat model, architecture comparison, report consolidation | Planned |
| 9 — Hardening & Submission | Security review, dependency audit, final polish | Planned |

---

The remainder of this page contains the detailed technical specification for each phase, including deliverables, test criteria, and documentation requirements.

---

## Documentation References

**Important**: This roadmap contains implementation logistics, dependencies, and test criteria. **Technical specifications** (algorithms, parameters, schemas) are in the canonical design documents:

- **Project scaffolding**: [`docs/architecture/designs/project-scaffolding/design.md`](architecture/designs/project-scaffolding/design.md)
- **Cryptographic primitives**: [`docs/architecture/designs/cryptographic-primitives/design.md`](architecture/designs/cryptographic-primitives/design.md)
- **Authentication & session**: [`docs/architecture/designs/authentication-and-session-management/design.md`](architecture/designs/authentication-and-session-management/design.md)
- **Chunking & manifest**: [`docs/architecture/designs/chunking-and-manifest/design.md`](architecture/designs/chunking-and-manifest/design.md)
- **Cloud synchronization**: [`docs/architecture/designs/cloud-synchronisation/design.md`](architecture/designs/cloud-synchronisation/design.md)
- **File sharing**: [`docs/architecture/designs/file-sharing/design.md`](architecture/designs/file-sharing/design.md)
- **Tauri IPC & frontend**: [`docs/architecture/designs/tauri-ipc-and-frontend/design.md`](architecture/designs/tauri-ipc-and-frontend/design.md)

When specifications in this roadmap conflict with design documents, **design documents are authoritative**.

## Notation

- **Depends on**: phases that must be complete before this phase can begin.
- **Parallelisable with**: phases that share no blocking dependencies.
- **ADR**: Architecture Decision Record — written to `docs/architecture-decisions/`.
- **Report sections**: maps to the bachelor report structure (Problem, Method, Analysis, Discussion, Conclusion).

---

## Phase 0 — Project Scaffolding and Tauri Initialisation

**Depends on**: nothing

**Design document**: [`docs/architecture/designs/project-scaffolding/design.md`](architecture/designs/project-scaffolding/design.md)

**Sub-phase roadmap**: [`docs/architecture/designs/project-scaffolding/sub-phases/roadmap.md`](architecture/designs/project-scaffolding/sub-phases/roadmap.md) (recommended for incremental implementation)

**Objective**: establish the compilable project skeleton — correct directory structure, dependency declarations, and CI pipeline — so all subsequent phases have a stable foundation.

**Deliverables**:
1. Run `cargo tauri init` to generate `src-tauri/` with `tauri.conf.json`, `src-tauri/src/main.rs`, and the frontend scaffold under `src/`.
2. Populate `src-tauri/Cargo.toml` with all core and dev-dependencies from `CLAUDE.md` (`chacha20poly1305`, `argon2`, `hkdf`, `blake3`, `rand`, `zeroize`, `secrecy`, `rusqlite`, `tokio`, `uuid`, `tauri`, `thiserror`, `anyhow`, `serde`, `serde_json`, `proptest`, `tempfile`, `assert_matches`).
3. Create module directory structure: `src-tauri/src/crypto/mod.rs`, `src-tauri/src/auth/mod.rs`, `src-tauri/src/storage/mod.rs`, `src-tauri/src/memory/mod.rs`, `src-tauri/src/ui/mod.rs` — each with a placeholder public API and module-level doc comment. The `sharing/` module is created in Phase 5.
4. Verify CI passes: `cargo fmt`, `cargo clippy -- -D warnings`, `cargo test`, `cargo build --release` all succeed on the empty skeleton.
5. Remove or repurpose the top-level `src/main.rs` / `Cargo.toml` (the bare Rust binary is superseded by the Tauri workspace).
6. Update `docs/guides/development.md` with Tauri-specific build and run instructions.

**Documentation**:
- ADR `001-code-structure-and-patterns.md` — module layout, newtype conventions.
- ADR `004-project-scaffolding.md` — workspace layout, Tauri v2, Leptos 0.8, SQLCipher bundling, and edition 2024 decisions.
- Report-log entry: scaffolding decisions and any edition 2024 surprises.

---

## Phase 1 — Cryptographic Primitives (`src-tauri/src/crypto/`)

**Depends on**: Phase 0

**Objective**: Implement the foundational cryptographic operations per [`docs/architecture/designs/cryptographic-primitives/design.md`](architecture/designs/cryptographic-primitives/design.md).

**Sub-phase roadmap**: [`docs/architecture/designs/cryptographic-primitives/sub-phases/roadmap.md`](architecture/designs/cryptographic-primitives/sub-phases/roadmap.md) (recommended for incremental implementation)

**Deliverables**:
1. **HKDF-SHA256 key derivation**: Derive vault-level keys (`key_encryption_key`, `sqlcipher_key`, `manifest_key`) from `master_key` with distinct `info` strings per design specification.
2. **Per-file key management**: Random 256-bit `file_key` generation via CSPRNG; wrapping/unwrapping with `key_encryption_key` using XChaCha20-Poly1305.
3. **Chunk encryption/decryption**: Implement `encrypt_chunk` and `decrypt_chunk` with wire format and AAD binding per design spec (see `docs/architecture/designs/cryptographic-primitives/design.md` for format, AAD construction, nonce generation).
4. **BLAKE3 checksums**: Compute checksums over encrypted blobs.
5. **Memory protection**: `ZeroizeOnDrop` and `Secret<T>` wrappers on all key types.
6. **Adversarial test suite**: encrypt/decrypt round-trip, AAD mismatch, wrong key, corrupted ciphertext, tag tampering, nonce uniqueness, zeroize verification.
7. **Property-based tests**: `proptest` for encrypt/decrypt round-trips across arbitrary inputs.

**Test acceptance criteria**:
- All adversarial tests pass (AAD mismatch must fail authentication)
- Nonce uniqueness: 10,000 sequential encryptions produce 10,000 unique nonces
- Zeroize: memory inspection shows zeroed buffers after key drop (use `miri` or manual inspection)
- Property tests: 1000+ arbitrary inputs round-trip successfully

**Documentation**:
- ADR `002-cipher-selection.md` — XChaCha20-Poly1305 rationale and alternatives considered.
- ADR `003-nonce-strategy.md` — random 192-bit nonce, birthday bound analysis, rejection of sequential nonces.
- ADR `004-key-derivation-tree.md` — HKDF key separation rationale, per-file key model.
- Update `docs/architecture/designs/cryptographic-primitives/diagrams/key-derivation-tree.md` if implementation diverges from design.
- Report-log entries: cipher trade-offs, nonce strategy, key separation design.
- Report sections: Method (cryptographic foundations), Analysis (adversarial test results).

---

## Phase 2 — Authentication and Session Management (`src-tauri/src/auth/`)

**Depends on**: Phase 1

**Objective**: implement the full authentication flow — USB key file generation and auto-detection, Argon2id KDF producing `master_key`, session lifecycle with mlocked memory, session timeout with zeroization, and vault creation/password-change/key-rotation flows.

**Design document**: [`docs/architecture/designs/authentication-and-session-management/design.md`](architecture/designs/authentication-and-session-management/design.md)

**Sub-phase roadmap**: [`docs/architecture/designs/authentication-and-session-management/sub-phases/roadmap.md`](architecture/designs/authentication-and-session-management/sub-phases/roadmap.md) (recommended for incremental implementation)

**Deliverables**:
1. `KeySource` trait and concrete USB key file reader (32-byte random entropy file, selected via file picker or auto-detected).
2. `MockKeySource` implementation for deterministic testing without physical hardware.
3. `DeviceMonitor` trait with OS-native implementations: `WindowsDeviceMonitor` (WMI / `RegisterDeviceNotification`) and `LinuxDeviceMonitor` (`udev` crate). `MockDeviceMonitor` for testing. Auto-detection scans for 32-byte files and matches against `key_file_blake3` in the vault header.
4. Argon2id KDF: input = `password_utf8 || key_file_bytes`, salt from vault header, parameters m≥19456, t≥2, p=1. Output: `master_key` (32 bytes).
5. HKDF expansion of `master_key` → `key_encryption_key`, `sqlcipher_key`, `manifest_key`. Zeroize `master_key` immediately after expansion.
6. `SessionKeys` struct: all three derived keys in mlocked memory (`mlock` on Linux, `VirtualLock` on Windows), `ZeroizeOnDrop`. Hard failure if memory locking is unavailable.
7. Activity-based session timeout (default 15 min, configurable). Background `tokio` task zeroes `SessionKeys` on expiry. Frontend notified 60 seconds before timeout.
8. Vault creation flow: generate key file → write to USB → compute `key_file_blake3` → generate salt → Argon2id + HKDF → create SQLCipher DB → generate X25519 identity keypair (store private key wrapped with `key_encryption_key`) → write vault header.
9. Password change and key file rotation flows: re-derive keys with new credentials, re-wrap all `file_key` values and X25519 private key, re-key SQLCipher, update vault header. No chunk re-encryption required.
10. Tests: correct credentials succeed, wrong password fails, wrong key file fails, session timeout zeroes memory (verified via `unsafe` pointer inspection), `DeviceMonitor` mock triggers auto-detection flow, mlock failure returns error.

**Documentation**:
- ADR `005-usb-key-file-design.md` — key file as cryptographic factor, BLAKE3 auto-detection, rejection of fixed filename and device serial numbers.
- ADR `006-session-model.md` — mlocked session keys, activity-based timeout, hard failure on mlock unavailability.
- `docs/threat-model/session-boundaries.md` — what `mlock` protects against (swap eviction) and what is explicitly out of scope (cold boot, compromised kernel).
- Report-log entries: USB auto-detection design, mlock hard failure rationale, vault creation flow.
- Report sections: Method (authentication design), Analysis (MFA factor strength).

---

## Phase 3 — Storage Layer: Chunking and Manifest (`src-tauri/src/storage/`)

**Depends on**: Phase 1 (chunk encryption), Phase 2 (session keys for SQLCipher)

**Objective**: implement the fixed-size chunking pipeline, the SQLCipher manifest database, and the local file-to-chunk-to-blob workflow (without cloud sync — that is Phase 4).

**Design document**: [`docs/architecture/designs/chunking-and-manifest/design.md`](architecture/designs/chunking-and-manifest/design.md)

**Sub-phase roadmap**: [`docs/architecture/designs/chunking-and-manifest/sub-phases/roadmap.md`](architecture/designs/chunking-and-manifest/sub-phases/roadmap.md) (recommended for incremental implementation)

**Deliverables**:
1. Fixed-size chunking with user-selected `chunk_size_bytes` (128 KiB–64 MiB, default 4 MiB), zero-pad to `chunk_size`, and truncate on reassembly using `size_bytes` — streaming via `BufReader`/`BufWriter` and `tokio::io`, never loading entire files into memory.
2. `MetadataStore` trait and SQLCipher implementation: `nodes` (with `file_key_wrapped`), `chunks`, `manifest_meta` tables with `ON DELETE CASCADE`. `MockMetadataStore` for testing.
3. Encrypt pipeline: `encrypt_file(source, file_id, file_key, chunk_size, staging_dir)` → `Vec<ChunkRecord>`. Per chunk: read → zero-pad → encrypt → BLAKE3 → write blob to staging → return `ChunkRecord`.
4. Decrypt pipeline: `decrypt_file(destination, file_id, file_key, file_size, chunks, blob_dir)`. Per chunk: read blob → verify BLAKE3 → decrypt → write (last chunk truncated to `file_size mod chunk_size`).
5. File key lifecycle: generate `file_key` → wrap with `key_encryption_key` → store `file_key_wrapped` in nodes table → use `file_key` for all chunks → zeroize.
6. Staging directory management: app data subdirectory, cleanup orphaned blobs on startup.
7. Error recovery: SQLCipher transactions wrap all manifest mutations; orphan blob scan on startup removes blobs not in `chunks` table.
8. Hybrid routing support when `epoch_buffer_enabled` is on: files smaller than `chunk_size_bytes` route to epoch packing, while files `>= chunk_size_bytes` stay on immediate standalone chunk uploads.
9. Tests: chunk boundary cases (0 bytes, 1 byte, 4 MiB-1, 4 MiB, 4 MiB+1, exact multiples), hybrid routing behavior, SQLCipher wrong-key rejection, CASCADE deletion, BLAKE3 mismatch rejection before decrypt, UUID v4 blob naming, 0-byte file edge case, staging orphan cleanup.

**Documentation**:
- ADR `007-fixed-size-chunking.md` — rejection of content-defined chunking, user-selected chunk size bounds with default 4 MiB, and hybrid auto-routing semantics for optional epoch buffering.
- ADR `008-manifest-database.md` — SQLCipher schema design, `file_key_wrapped` on nodes table, rejection of JSON and sled alternatives.
- Architecture diagram: chunk pipeline data flow (encrypt path and decrypt path).
- Report-log entries: chunk size decision with waste analysis, schema design rationale.
- Report sections: Method (chunking and metadata design), Analysis (padding overhead quantification).

---

## Phase 4 — Cloud Synchronisation (Rclone Integration)

**Depends on**: Phase 3

**Objective**: implement the `CloudTransport` trait backed by Rclone, vault header upload/download, manifest cloud backup, and the full upload/download cycle against a real cloud provider.

**Design document**: [`docs/architecture/designs/cloud-synchronisation/design.md`](architecture/designs/cloud-synchronisation/design.md)

**Sub-phase roadmap**: [`docs/architecture/designs/cloud-synchronisation/sub-phases/roadmap.md`](architecture/designs/cloud-synchronisation/sub-phases/roadmap.md) (recommended for incremental implementation)

**Deliverables**:
1. `CloudTransport` trait: `upload_blob`, `download_blob`, `delete_blob`, `list_blobs`.
2. `RcloneTransport` concrete implementation: Rclone bundled as a Tauri sidecar binary, invoked via `tokio::process::Command` (no shell), with remote path sanitisation and stderr sanitisation.
3. Guided setup wizard: provider selection UI calling `rclone config create` for S3-compatible providers (AWS S3, MinIO, Backblaze B2, Wasabi) and Google Drive (OAuth flow).
4. Vault header: generate, upload, download, and parse (`vault_id`, `schema_version`, `argon2_salt`, `argon2_params`, `key_file_blake3` — plaintext JSON by design).
5. Manifest cloud backup: `VACUUM INTO` export, encrypt with `manifest_key` (XChaCha20-Poly1305, random nonce, no AAD), upload as `manifest/manifest-backup.blob`; download and decrypt for new-device recovery.
6. `snapshot_counter` increment on each push; detect-and-block conflict detection (abort if cloud counter exceeds local).
7. Upload order randomisation (Fisher-Yates shuffle) to prevent temporal correlation of blobs.
8. Tests: `MockCloudTransport` for unit tests; integration test with a local Rclone remote (`rclone config create testremote local`).

**Documentation**:
- ADR `009-cloud-transport-rclone.md` — Rclone choice rationale, provider-agnostic design, subprocess security model.
- ADR `010-vault-header-design.md` — bootstrap chicken-and-egg problem, what is safe to store in plaintext.
- Architecture diagram: cloud sync sequence showing upload flow and new-device recovery flow.
- Report-log entries: Rclone integration observations, provider testing notes.
- Report sections: Method (cloud synchronisation design), Analysis (provider independence verification).

**Parallelisable with**: frontend UI prototyping can begin here using mock data, provided the Tauri command signatures defined in Phase 6 are drafted first.

---

## Phase 5 — Identity and File Sharing (`src-tauri/src/sharing/`)

**Depends on**: Phase 1 (per-file keys, HKDF, XChaCha20-Poly1305), Phase 3 (manifest schema, MetadataStore), Phase 4 (CloudTransport)

**Objective**: implement the file sharing layer — X25519 local identity, contact management, ECIES share package creation and import, shared blob cloud layout, and revocation.

**Sub-phase roadmap**: [`docs/architecture/designs/file-sharing/sub-phases/roadmap.md`](architecture/designs/file-sharing/sub-phases/roadmap.md) (recommended for incremental implementation)

**Deliverables**:
1. X25519 keypair generation on first run; store private key in SQLCipher wrapped with `key_encryption_key`.
2. Contact management: import a contact's X25519 public key (from file or QR code), store in `contacts` table with display name and optional email label.
3. ECIES share package creation:
   - Retrieve `file_key` for the target file from SQLCipher (unwrap with `key_encryption_key`)
   - Generate ephemeral X25519 keypair → ECDH with recipient's public key → HKDF → symmetric key
   - Encrypt `file_key` with that symmetric key (XChaCha20-Poly1305)
   - Assemble share package JSON: `share_id`, `file_id`, `file_name`, `chunk_count`, `chunk_size`, `chunk_uuids`, `file_key_wrapped`, optional `expires_at` (derived from `share_file(expiration_days)` when provided), `cloud_endpoint`
   - Encode ECIES wire bytes with header fields `ephemeral_public_key` (32B) and `nonce` (24B) before ciphertext/tag; `ephemeral_public_key` is a wire-header field, not a JSON payload field
   - Encrypt the entire package as an ECIES envelope; export as a file
4. Share package import: parse and decrypt the ECIES envelope using the local X25519 private key; store in the recipient's `received_shares` table (including optional `expires_at`); fetch blobs via Rclone.
5. Cloud layout: copy shared chunks to `shared/<file_share_id>/` with public read access; record `file_share_id` in the `shares` table.
6. Revocation: delete `shared/<file_share_id>/` from cloud; set `revoked_at` in the `shares` table. Optional re-encryption flow: generate new `file_key`, re-encrypt all chunks, upload under new `file_share_id`, issue new share packages to remaining recipients.
7. `shares`, `contacts`, and `received_shares` schema additions via SQLCipher migration.
8. Tests: ECIES round-trip (encrypt with recipient public key, decrypt with private key), wrong-recipient rejection, revocation verification (blobs deleted, share marked revoked), corrupted share package, MITM-substituted ephemeral key rejection.

**Documentation**:
- ADR `012-sharing-architecture.md` — per-file keys, ECIES construction, shared blob storage, snapshot share semantics.
- ADR `013-identity-model.md` — X25519 local identity, no central server, out-of-band key exchange, trust assumptions.
- Threat model addition: MITM on key exchange (fingerprint verification as mitigation), ciphertext exposure via public blobs.
- [`docs/architecture/designs/file-sharing/design.md`](architecture/designs/file-sharing/design.md) — primary design document (already written).
- Report sections: Method (sharing design), Analysis (sharing verification), Discussion (comparison with OneDrive/Cryptomator sharing — sub-question 5).

---

## Phase 6 — Tauri IPC Layer and Frontend (`src-tauri/src/ui/` + `src/`)

**Depends on**: Phase 2 (auth commands), Phase 3 (storage commands), Phase 4 (sync commands), Phase 5 (sharing commands)

**Objective**: expose backend functionality to the frontend through Tauri commands with proper error sanitisation, and build a functional web UI for authentication, vault browsing, transfer, sync, destination management, and sharing workflows.

**Design document**: [`docs/architecture/designs/tauri-ipc-and-frontend/design.md`](architecture/designs/tauri-ipc-and-frontend/design.md)

**Sub-phase roadmap**: [`docs/architecture/designs/tauri-ipc-and-frontend/sub-phases/roadmap.md`](architecture/designs/tauri-ipc-and-frontend/sub-phases/roadmap.md) (recommended for incremental implementation)

**Phase 6 profiles**:
- **Full profile (canonical)**: command surface defined in [`design.md#canonical-command-surface-normative`](architecture/designs/tauri-ipc-and-frontend/design.md#canonical-command-surface-normative)
- **MVP profile (optional)**: frontend UX slicing is allowed, but command inclusion/exclusion stays fixed to the canonical full profile

**Deliverables**:
1. Tauri command definitions and registration for the canonical full-profile command surface in [`design.md#canonical-command-surface-normative`](architecture/designs/tauri-ipc-and-frontend/design.md#canonical-command-surface-normative), with `src-tauri/src/lib.rs` (`tauri::generate_handler![]`) and `src-tauri/build.rs` (`AppManifest::commands`) kept in lockstep.
2. Error sanitisation layer: map `thiserror` enums from backend modules to user-safe `IpcError` responses via explicit `From` impls; no partial keys, sensitive internal filesystem paths, memory addresses, stack traces, or crypto internals reach the frontend in error payloads.
3. Input validation on all Tauri command parameters.
4. Frontend pages: login screen (password + USB key file selection), vault browser (file list, folder navigation), upload/download controls, session status indicator, and vault-creation controls for `chunk_size_bytes` + `epoch_buffer_enabled` with hybrid-routing UX copy.
5. Async command handlers using `tokio::spawn` to avoid blocking the Tauri UI thread.
6. `withGlobalTauri: true` configured in `tauri.conf.json` before Phase 6.2 invoke-wrapper work; Phase 6.4 treats this as verification/hardening alongside CSP and capabilities.

**Documentation**:
- ADR `011-ipc-error-sanitisation.md` — what is filtered, the mapping strategy, and the rationale.
- Architecture diagram: IPC command surface and data flow between frontend and backend.
- Report-log entries: IPC design decisions, frontend technology trade-offs.
- Report sections: Analysis (user interface design), Discussion (RAM-based UI vs. virtual filesystem — sub-question 4).

---

## Phase 7 — End-to-End Integration Testing

**Depends on**: Phases 1 through 6

**Objective**: validate the complete pipeline from authentication through file upload, cloud sync, sharing, and recovery on a simulated new device, confirming all modules interoperate correctly and adversarial scenarios are handled.

**Deliverables**:
1. Integration test: authenticate -> upload file -> verify manifest -> sync to cloud -> verify blobs exist -> simulate new device (fresh local state) -> download vault header -> re-authenticate -> recover manifest -> download and decrypt file -> verify content matches original.
2. Integration test: share a file -> recipient imports share package -> recipient fetches and decrypts blobs -> verify content matches original.
3. Integration test: revoke a share -> verify blobs deleted from cloud -> verify share package is now inoperative.
4. Integration test: session timeout during an operation -> verify keys are zeroed -> verify partial state is cleaned up.
5. Integration test: file deletion -> verify blob removal from cloud and manifest CASCADE cleanup.
6. Adversarial integration tests: corrupted blob (BLAKE3 mismatch), swapped blob (AAD mismatch), tampered vault header, wrong-recipient share package.
7. CI pipeline verification: all integration tests pass in GitHub Actions.

**Documentation**:
- `docs/threat-model/attack-scenarios.md` — document the adversarial integration tests as concrete attack scenario mitigations.
- Report-log entries: integration findings, edge cases discovered during testing.
- Report sections: Analysis (end-to-end verification), Discussion (limitations discovered during integration).

---

## Phase 8 — Threat Model, Sharing Architecture Comparison, and Report Consolidation

**Depends on**: Phase 7

**Objective**: complete the formal documentation deliverables — threat model, sharing architecture comparison with OneDrive and Cryptomator, and consolidate report-log entries into the bachelor report structure.

**Deliverables**:
1. `docs/threat-model/threat-model.md` — trust boundaries, threat actors, attack vectors, mitigations, and explicit out-of-scope declarations (cold boot, compromised OS kernel, multi-device conflict resolution, MITM on key exchange without fingerprint verification).
2. `docs/architecture/system-overview.md` — consolidated architecture document with all diagrams, module descriptions, and data flows.
3. Sharing architecture comparison: Arx Runa sharing (ECIES, per-file keys, shared blob storage) vs. OneDrive sharing links (server-side ACL, provider holds keys) vs. Cryptomator shared vaults (vault-level sharing, not file-granularity) — addressing sub-question 5.
4. Run `/report-note compile` to aggregate all report-log entries into a structured report outline.
5. Verify all ADRs are complete and cross-referenced.

**Documentation**:
- Threat model documents (primary deliverable of this phase).
- Architecture overview (primary deliverable of this phase).
- Report sections: Discussion (sharing comparison — sub-question 5), Conclusion (sub-conclusions per module).

**Parallelisable with**: documentation accumulation in this phase can begin as early as Phase 4; the formal consolidation depends on Phase 7 results.

---

## Phase 9 — Hardening, Polish, and Submission Preparation

**Depends on**: Phase 8

**Objective**: final quality pass — resolve remaining open decisions, address security review findings, ensure CI is clean, and prepare for submission.

**Deliverables**:
1. Security review pass: run `/review` on all security-critical modules (`crypto/`, `auth/`, `storage/`, `sharing/`); address all CRITICAL findings.
2. `cargo audit` — resolve any known vulnerabilities in dependencies.
3. Resolve any remaining open decisions accumulated during development.
4. Final `cargo clippy -- -D warnings`, `cargo fmt`, `cargo test` — CI clean.
5. Update `README.md`: accurate project description, build instructions, architecture summary.
6. Final report-log review: confirm all significant decisions and trade-offs are captured for the bachelor report.

**Documentation**:
- Updated `README.md`.
- Final report-log entries for any late-stage decisions.
- Report sections: all sections reviewed for completeness and coherence.

---

## Dependency Graph

```
Phase 0  (scaffolding)
    │
    v
Phase 1  (crypto primitives + per-file keys)
    │
    v
Phase 2  (auth + session)
    │
    v
Phase 3  (chunking + manifest)
    │
    v
Phase 4  (cloud sync)            ← frontend mock work can begin here
    │
    v
Phase 5  (identity + file sharing)
    │
    v
Phase 6  (Tauri IPC + frontend)
    │
    v
Phase 7  (integration testing)
    │
    v
Phase 8  (threat model + report consolidation)
    │
    v
Phase 9  (hardening + submission)
```

