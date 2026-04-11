# File Sharing — Sub-Phase Roadmap

**Parent design**: [`design.md`](../design.md)  
**Created**: 2026-04-04  
**Status**: Draft  
**Implementation order**: 5.1 → 5.2 → 5.3 (strict dependencies)

---

## Overview

This sub-phase roadmap decomposes the file sharing design (282 lines) into 3 independently testable implementation units, enabling incremental validation of the identity layer, cryptographic sharing construction, and cloud operations before the full Phase 5 integration.

**Total sub-phases**: 3

**Rationale for decomposition**:
-  **Size**: Exceeds ~100-150 lines (282 lines total)
-  **Trait boundaries**: Identity/contact management implementable independently of ECIES construction and cloud operations
-  **Integration breadth**: Touches crypto module (ECIES, HKDF), storage module (SQLCipher schema extension, CloudTransport), and auth module (key wrapping)
-  **Error surface**: Defines distinct error domains across identity, cryptographic, and cloud operations
-  **Multi-step flows**: Key exchange flow, share package creation flow, revocation flow

**Implementation strategy**: Build identity layer with contact management → implement ECIES construction and share packages → add cloud layout and revocation operations

---

## Dependency Graph

```
5.1 (X25519 identity + contact management)
 ↓
5.2 (ECIES construction + share packages)
 ↓
5.3 (Cloud layout + revocation)
```

**Legend**:
- `→` strict dependency (must complete predecessor before starting)

---

## Sub-Phases

1. **[Phase 5.1: X25519 Identity and Contact Management](5.1-identity-and-contacts.md)**
   - X25519 keypair generation and private key wrapping
   - Public key export (file and QR code data)
   - Contact import and CRUD via SQLCipher
   - Fingerprint display (first 16 hex chars of SHA-256)
   - **Estimated**: ~120 lines production code, ~80 lines tests

2. **[Phase 5.2: ECIES Construction and Share Packages](5.2-ecies-and-share-packages.md)**
   - ECIES encrypt/decrypt using ephemeral X25519 + HKDF-SHA256 + XChaCha20-Poly1305
   - Share package creation: retrieve file_key → encrypt → assemble JSON envelope (including optional `expires_at`)
   - Share package import: parse ECIES envelope → extract file_key → store in received_shares (preserving optional `expires_at`)
   - Snapshot semantics: static chunk_uuids at time of sharing
   - **Estimated**: ~180 lines production code, ~120 lines tests

3. **[Phase 5.3: Cloud Layout and Revocation](5.3-cloud-layout-and-revocation.md)**
   - Blob copy from `vault/` to `shared/<file_share_id>/` via CloudTransport
   - `shares` table management with file_share_id and cloud_path
   - Revocation by blob deletion and revoked_at timestamp
   - Optional re-encryption flow for stronger revocation guarantees
   - `received_shares` blob fetching via Rclone
   - **Estimated**: ~150 lines production code, ~100 lines tests

---

## Testing Strategy

### Per-Sub-Phase Testing

Each sub-phase includes its own test suite. Tests must pass before proceeding to the next sub-phase.

**Test types**:
- **Unit tests**: Core functionality in isolation (keypair generation, ECIES round-trips, fingerprint computation)
- **Mock-based tests**: Use `MockTransport` (from Phase 4.1) for cloud operations in Phases 5.2 and 5.3
- **Property-based tests** (where applicable): ECIES wrong-recipient rejection, corrupted package detection
- **Integration tests**: Once all sub-phases complete, end-to-end share creation → import → decrypt round-trip

### Regression Testing

After completing each sub-phase, run:
```bash
cargo test           # All tests must pass
cargo clippy -- -D warnings  # No new warnings
```

This ensures new code does not break earlier sub-phases.

### Manual Testing Checklist

- Phase 5.1: Export public key file and verify it imports cleanly on a second Arx Runa instance
- Phase 5.2: Create a share package, inspect the wire format bytes (first 32B is ephemeral key, next 24B is nonce)
- Phase 5.3: Share a file, verify blobs appear under `shared/<file_share_id>/`; revoke and verify blobs are deleted

---

## Security Review Checkpoints

- **Phase 5.1**: Requires `security-reviewer` agent review (private key storage in SQLCipher, fingerprint correctness)
- **Phase 5.2**: Requires `security-reviewer` agent review (ECIES correctness, ephemeral key disposal, HKDF salt construction)
- **Phase 5.3**: Requires `security-reviewer` agent review (revocation correctness, public blob exposure per threat model)

---

## Implementation Workflow

```bash
# Phase 5.1
/plan 5.1
/implement-plan phase-005-1-identity-and-contacts.md
cargo test sharing::identity
cargo test sharing::contacts

# Phase 5.2
/plan 5.2
/implement-plan phase-005-2-ecies-and-share-packages.md
cargo test sharing::ecies
cargo test sharing::packages

# Phase 5.3
/plan 5.3
/implement-plan phase-005-3-cloud-layout-and-revocation.md
cargo test sharing::cloud
cargo test sharing::revocation
```

---

## Documentation Impact

**Files to create/update after sub-phase completion**:
- Phase 5.1: No doc updates required
- Phase 5.2: No doc updates required
- Phase 5.3: Update `docs/threat-model/` with MITM-on-key-exchange and ciphertext-exposure-via-public-blobs threat model additions; update `docs/roadmap.md` to mark Phase 5 complete

---

## Notes

### Design Clarifications

- **file_share_id vs share_id**: `file_share_id` groups all blob copies for a given file in cloud storage (one per file, shared by all recipients). `share_id` identifies a per-recipient–file relationship in the `shares` table. These must not be conflated.
- **file_key_wrapped in share package**: the `file_key_wrapped` JSON field inside the ECIES envelope is the `file_key` re-encrypted with the ECDH-derived symmetric key — it is not the same ciphertext as the one stored in the `nodes` table (which is wrapped with `key_encryption_key`).
- **No schema addition for nodes**: the `file_key_wrapped` column is already established in Phase 3. Phase 5 only adds `contacts`, `shares`, and `received_shares` tables.

### Future Work

- Enterprise key distribution: IT-distributed JSON file of employee public keys or optional internal key directory server (deferred; not blocking Phase 5)
- Fingerprint verification UX: display format and placement in the UI (deferred; display-only implementation in Phase 5.1 is sufficient)
- Folder sharing: snapshot model applies per file; multi-file folder sharing is a future extension of the same mechanism
- Live sharing (recipient always sees latest version) requires directory-level share agreements; deferred

### Risks and Mitigations

| Risk | Mitigation |
|------|------------|
| ECIES implementation error produces ciphertext decryptable by wrong party | Dedicate a test to wrong-recipient rejection and MITM-substituted ephemeral key |
| Ephemeral private key retained in memory after ECDH | Wrap in `zeroize::Zeroizing` and drop immediately; verified by code review and security-reviewer agent |
| Revocation perceived as complete when recipient has already fetched | Explicit report statement in design (lines 165-169); limitation must surface in UI and report log |
| Public blob discovery reveals share existence | Accepted per threat model (design lines 349-355); mitigated by fixed-size padding and UUID v4 blob names |

---

## References

- **Parent design**: `docs/architecture/designs/file-sharing/design.md`
- **Roadmap entry**: `docs/roadmap.md` Phase 5
- **Related phases**: Phase 1.3 (key wrapping pattern), Phase 3.1 (SQLCipher schema), Phase 4 (CloudTransport)
- **Reference implementation**: `age` encryption tool (same ECIES construction, audited)
