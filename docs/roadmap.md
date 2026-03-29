# VoidGate Implementation Roadmap

> Master plan for the VoidGate bachelor project. Each phase is designed as a
> self-contained unit suitable for a single `/plan` + `/implement` session.
> Documentation milestones are woven into implementation phases to ensure the
> bachelor report is built incrementally alongside the codebase.

## Notation

- **Depends on**: phases that must be complete before this phase can begin.
- **Parallelisable with**: phases that share no blocking dependencies.
- **ADR**: Architecture Decision Record — written to `docs/architecture-decisions/`.
- **Report sections**: maps to the bachelor report structure (Problem, Method, Analysis, Discussion, Conclusion).

---

## Phase 0 — Project Scaffolding and Tauri Initialisation

**Depends on**: nothing

**Objective**: establish the compilable project skeleton — correct directory structure, dependency declarations, and CI pipeline — so all subsequent phases have a stable foundation.

**Deliverables**:
1. Run `cargo tauri init` to generate `src-tauri/` with `tauri.conf.json`, `src-tauri/src/main.rs`, and the frontend scaffold under `src/`.
2. Populate `src-tauri/Cargo.toml` with all core and dev-dependencies from `CLAUDE.md` (`chacha20poly1305`, `argon2`, `hkdf`, `blake3`, `rand`, `zeroize`, `secrecy`, `rusqlite`, `tokio`, `uuid`, `tauri`, `thiserror`, `anyhow`, `serde`, `serde_json`, `proptest`, `tempfile`, `assert_matches`).
3. Create module directory structure: `src-tauri/src/crypto/mod.rs`, `src-tauri/src/auth/mod.rs`, `src-tauri/src/storage/mod.rs`, `src-tauri/src/ui/mod.rs` — each with a placeholder public API and module-level doc comment.
4. Verify CI passes: `cargo fmt`, `cargo clippy -- -D warnings`, `cargo test`, `cargo build --release` all succeed on the empty skeleton.
5. Remove or repurpose the top-level `src/main.rs` / `Cargo.toml` (the bare Rust binary is superseded by the Tauri workspace).
6. Update `docs/guides/development.md` with Tauri-specific build and run instructions.

**Documentation**:
- ADR `001-project-structure.md` — workspace layout, rationale for Tauri, edition 2024.
- Report-log entry: scaffolding decisions and any edition 2024 surprises.

---

## Phase 1 — Cryptographic Primitives (`src-tauri/src/crypto/`)

**Depends on**: Phase 0

**Objective**: implement the foundational cryptographic operations that all other modules depend on — HKDF key derivation, XChaCha20-Poly1305 AEAD encrypt/decrypt, chunk wire format, and BLAKE3 checksums.

**Deliverables**:
1. HKDF-SHA256 key derivation producing the three purpose-specific keys (`chunk_key`, `sqlcipher_key`, `manifest_key`) with distinct `info` strings as specified in `CLAUDE.md`.
2. `encrypt_chunk` and `decrypt_chunk` implementing the wire format `[24-byte nonce | ciphertext | 16-byte Poly1305 tag]` with mandatory AAD (`file_id || chunk_index`).
3. BLAKE3 checksum computation over encrypted blobs.
4. `ZeroizeOnDrop` and `Secret<T>` wrappers on all key types.
5. Full adversarial test suite: encrypt/decrypt round-trip, AAD mismatch, wrong key, corrupted ciphertext, tag tampering, nonce uniqueness, and zeroize verification.
6. Property-based tests via `proptest` for encrypt/decrypt round-trips across arbitrary inputs.

**Documentation**:
- ADR `002-cipher-selection.md` — XChaCha20-Poly1305 rationale and alternatives considered.
- ADR `003-nonce-strategy.md` — random 192-bit nonce, birthday bound analysis, rejection of sequential nonces.
- ADR `004-key-derivation-tree.md` — HKDF key separation rationale.
- Update `docs/architecture/diagrams/key-derivation-tree.md` if implementation diverges from design.
- Report-log entries: cipher trade-offs, nonce strategy, key separation design.
- Report sections: Method (cryptographic foundations), Analysis (adversarial test results).

---

## Phase 2 — Authentication and Session Management (`src-tauri/src/auth/`)

**Depends on**: Phase 1

**Objective**: implement the full authentication flow — USB key file reading, Argon2id KDF producing `master_key`, session lifecycle with mlocked memory, and session timeout with zeroization.

**Deliverables**:
1. `KeySource` trait and concrete USB key file reader (32-byte random entropy file).
2. `MockKeySource` implementation for deterministic testing without physical hardware.
3. Argon2id KDF combining `password || key_file_bytes` as input, using salt from the vault header, with OWASP-minimum parameters (m >= 19456, t >= 2, p = 1).
4. Session struct holding derived keys in mlocked memory (`mlock` on Linux, `VirtualLock` on Windows), with `Drop` calling `zeroize()`.
5. Session timeout mechanism that zeroes all keys and requires re-authentication with password and USB.
6. Tests: correct credentials succeed, wrong password fails, wrong key file fails, session timeout zeroes memory (verified via `unsafe` pointer inspection).

**Documentation**:
- ADR `005-usb-key-file-design.md` — key file as cryptographic factor, rejection of device serial numbers and TOTP, resolution of the pending fixed-filename vs. arbitrary-file decision.
- ADR `006-session-model.md` — single-read USB, mlocked session keys, timeout semantics.
- `docs/threat-model/session-boundaries.md` — what `mlock` protects against and what is explicitly out of scope (cold boot, compromised kernel).
- Report-log entries: UX-vs-security trade-off, USB factor design rationale.
- Report sections: Method (authentication design), Analysis (MFA factor strength).

---

## Phase 3 — Storage Layer: Chunking and Manifest (`src-tauri/src/storage/`)

**Depends on**: Phase 1 (chunk encryption), Phase 2 (session keys for SQLCipher)

**Objective**: implement the fixed-size chunking pipeline, the SQLCipher manifest database, and the local file-to-chunk-to-blob workflow (without cloud sync — that is Phase 4).

**Deliverables**:
1. Fixed-size chunking with uniform padding — streaming via `BufReader` / `BufWriter` and `tokio::io`, never loading entire files into memory.
2. `MetadataStore` trait and SQLCipher implementation: `nodes`, `chunks`, and `manifest_meta` tables with `ON DELETE CASCADE`.
3. Encrypt pipeline: file read (streaming) -> chunk -> pad to fixed size -> encrypt -> BLAKE3 -> write encrypted blob to local staging directory -> record in manifest.
4. Decrypt pipeline: read manifest -> fetch blob -> verify BLAKE3 -> decrypt -> unpad -> reassemble (streaming).
5. Resolve the pending chunk size decision (4 MB vs. 8 MB) with quantified padding waste analysis.
6. Tests: chunk boundary cases (0 bytes, 1 byte, chunk\_size-1, chunk\_size, chunk\_size+1, exact multiples), SQLCipher wrong-key rejection, CASCADE deletion, BLAKE3 mismatch rejection, UUID v4 blob naming.

**Documentation**:
- ADR `007-fixed-size-chunking.md` — rejection of content-defined chunking, padding overhead trade-off, chosen chunk size with waste quantification.
- ADR `008-manifest-database.md` — SQLCipher schema design, rejection of JSON and sled alternatives.
- Architecture diagram: chunk pipeline data flow (encrypt path and decrypt path).
- Report-log entries: chunking trade-offs, storage overhead measurements.
- Report sections: Method (chunking and metadata design), Analysis (padding overhead quantification).

---

## Phase 4 — Cloud Synchronisation (Rclone Integration)

**Depends on**: Phase 3

**Objective**: implement the `CloudTransport` trait backed by Rclone, vault header upload/download, manifest cloud backup, and the full upload/download cycle against a real cloud provider.

**Deliverables**:
1. `CloudTransport` trait: `upload_blob`, `download_blob`, `delete_blob`, `list_blobs`.
2. Rclone concrete implementation: subprocess invocation with sanitised arguments — no shell injection, no plaintext logging of remote paths.
3. Vault header: generate, upload (before manifest), download, and parse (`vault_id`, `schema_version`, `argon2_salt`, `argon2_params` — stored as plaintext JSON by design).
4. Manifest cloud backup: encrypt the SQLCipher export with `manifest_key`, upload as a blob; download and decrypt for new-device recovery.
5. `snapshot_counter` increment on each push to enable conflict detection.
6. Tests: mock `CloudTransport` for unit tests; integration test with a local Rclone remote (local filesystem backend or `rclone serve webdav`).

**Documentation**:
- ADR `009-cloud-transport-rclone.md` — Rclone choice rationale, provider-agnostic design, subprocess security model.
- ADR `010-vault-header-design.md` — bootstrap chicken-and-egg problem, what is safe to store in plaintext.
- Architecture diagram: cloud sync sequence showing upload flow and new-device recovery flow.
- Report-log entries: Rclone integration observations, provider testing notes.
- Report sections: Method (cloud synchronisation design), Analysis (provider independence verification).

**Parallelisable with**: frontend UI prototyping can begin here using mock data, provided the Tauri command signatures defined in Phase 5 are drafted first.

---

## Phase 5 — Tauri IPC Layer and Frontend (`src-tauri/src/ui/` + `src/`)

**Depends on**: Phase 2 (auth commands), Phase 3 (storage commands), Phase 4 (sync commands)

**Objective**: expose backend functionality to the frontend through Tauri commands with proper error sanitisation, and build a minimal but functional web UI for authentication, vault browsing, upload, and download.

**Deliverables**:
1. Tauri command definitions: `authenticate`, `lock_session`, `list_vault_contents`, `upload_file`, `download_file`, `delete_file`, `sync_to_cloud`, `recover_from_cloud`.
2. Error sanitisation layer: map `thiserror` enums from library modules to generic user-safe IPC responses via `anyhow`; no partial keys, file paths, or memory addresses reach the frontend.
3. Input validation on all Tauri command parameters.
4. Frontend pages: login screen (password + USB key file selection), vault browser (file list, folder navigation), upload/download controls, session status indicator.
5. Async command handlers using `tokio::spawn` to avoid blocking the Tauri UI thread.

**Documentation**:
- ADR `011-ipc-error-sanitisation.md` — what is filtered, the mapping strategy, and the rationale.
- Architecture diagram: IPC command surface and data flow between frontend and backend.
- Report-log entries: IPC design decisions, frontend technology trade-offs.
- Report sections: Analysis (user interface design), Discussion (RAM-based UI vs. virtual filesystem — sub-question 4).

---

## Phase 6 — End-to-End Integration Testing

**Depends on**: Phases 1 through 5

**Objective**: validate the complete pipeline from authentication through file upload, cloud sync, and recovery on a simulated new device, confirming all modules interoperate correctly and adversarial scenarios are handled.

**Deliverables**:
1. Integration test: authenticate -> upload file -> verify manifest -> sync to cloud -> verify blobs exist -> simulate new device (fresh local state) -> download vault header -> re-authenticate -> recover manifest -> download and decrypt file -> verify content matches original.
2. Integration test: session timeout during an operation -> verify keys are zeroed -> verify partial state is cleaned up.
3. Integration test: file deletion -> verify blob removal from cloud and manifest CASCADE cleanup.
4. Adversarial integration tests: corrupted blob (BLAKE3 mismatch), swapped blob (AAD mismatch), tampered vault header.
5. CI pipeline verification: all integration tests pass in GitHub Actions.

**Documentation**:
- `docs/threat-model/attack-scenarios.md` — document the adversarial integration tests as concrete attack scenario mitigations.
- Report-log entries: integration findings, edge cases discovered during testing.
- Report sections: Analysis (end-to-end verification), Discussion (limitations discovered during integration).

---

## Phase 7 — Threat Model, Architectural Comparison, and Report Consolidation

**Depends on**: Phase 6

**Objective**: complete the formal documentation deliverables — threat model, architectural comparison with OneDrive and Cryptomator, and consolidate report-log entries into the bachelor report structure.

**Deliverables**:
1. `docs/threat-model/threat-model.md` — trust boundaries, threat actors, attack vectors, mitigations, and explicit out-of-scope declarations (cold boot, compromised OS kernel, multi-device conflict resolution).
2. `docs/architecture/system-overview.md` — consolidated architecture document with all diagrams, module descriptions, and data flows.
3. Architectural comparison: VoidGate vs. OneDrive (provider-trust model) vs. Cryptomator (client-side encryption, virtual filesystem) — addressing sub-question 5.
4. Run `/report-note compile` to aggregate all report-log entries into a structured report outline.
5. Verify all ADRs are complete and cross-referenced.

**Documentation**:
- Threat model documents (primary deliverable of this phase).
- Architecture overview (primary deliverable of this phase).
- Report sections: Discussion (comparison and extensibility — sub-question 5), Conclusion (sub-conclusions per module).

**Parallelisable with**: documentation accumulation in this phase can begin as early as Phase 4; the formal consolidation depends on Phase 6 results.

---

## Phase 8 — Hardening, Polish, and Submission Preparation

**Depends on**: Phase 7

**Objective**: final quality pass — resolve remaining open decisions, address security review findings, ensure CI is clean, and prepare for submission.

**Deliverables**:
1. Security review pass: run `/review` on all security-critical modules (`crypto/`, `auth/`, `storage/`); address all CRITICAL findings.
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
Phase 1  (crypto primitives)
    │
    v
Phase 2  (auth + session)
    │
    v
Phase 3  (chunking + manifest)
    │
    v
Phase 4  (cloud sync)       ← frontend mock work can begin here
    │
    v
Phase 5  (Tauri IPC + frontend)
    │
    v
Phase 6  (integration testing)
    │
    v
Phase 7  (threat model + report consolidation)
    │
    v
Phase 8  (hardening + submission)
```

---

## Pending Architectural Decisions

The following decisions remain open and must be resolved during the indicated phases:

| Decision | Resolution phase | Impact |
|----------|-----------------|--------|
| USB key file: fixed filename vs. arbitrary file (plausible deniability) | Phase 2 | ADR required; affects `KeySource` trait design |
| Chunk size: 4 MB vs. 8 MB | Phase 3 | ADR required; affects padding waste analysis and upload latency |

---

## Report Section Mapping

Each phase contributes to specific sections of the bachelor report, ensuring the report is constructed incrementally rather than written under time pressure at the end:

| Report section | Contributing phases |
|---------------|-------------------|
| Problem formulation | Phase 0 (existing), refined in Phase 7 |
| Method and scientific foundation | Phases 1, 2, 3, 4 |
| Analysis and application | Phases 1, 2, 3, 4, 5, 6 |
| Discussion and recommendations | Phases 5, 6, 7 |
| Conclusion | Phase 7 |
