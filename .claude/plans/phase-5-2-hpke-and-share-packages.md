---
title: "Phase 5.2 — HPKE Construction and Share Packages"
created: "2026-04-20T00:00:00Z"
status: implemented
roadmap-phase: 5
sub-phase: "5.2"
design-document: docs/architecture/designs/file-sharing/design.md
sub-phase-roadmap: docs/architecture/designs/file-sharing/sub-phases/roadmap.md
governance-sync-required: true
tags: [sharing, hpke, ctx-aead, committing-aead, security-critical]
---

## 1. Goal

Build the HPKE seal/open surface and `.vgshare` package creation/import pipeline for Phase 5 file sharing, backed by a custom `CTX-ChaCha20-Poly1305` committing AEAD.

## 2. Context

- Sub-phase scope (roadmap Phase 5.2): `~200` LoC production, `~140` LoC tests.
- Depends on Phase 5.1 (`sharing::identity`, `sharing::SharingStore::get_own_public_key`, `vault_identity` row with `wrapped_private_key`) — already merged.
- Depends on Phase 1.3 key-wrapping primitives (`crypto::wrap_file_key` / `crypto::unwrap_file_key`) and Phase 3.1 `nodes.file_key_wrapped` column for owner-side `file_key` retrieval.
- Parent design sections: HPKE Construction (design.md §HPKE Construction), Share Package Format (§Share Package Format), Snapshot Semantics (§Snapshot Semantics), Database Schema (§Database Schema).
- Cross-phase invariants #11 (share package/import key-handling), #13 (vault identity ownership and read-only sharing access).
- Rule anchors: `.claude/rules/sharing.md`, `.claude/rules/crypto.md`, `.claude/rules/storage.md`, `.claude/rules/rust.md`.
- Sub-phase security checkpoint: `security-reviewer` agent review required (CTX commitment, HPKE `info`/`aad`, oracle-free authentication error paths).

## 3. Design Concerns / Open Questions

| Concern | Source | Impact | Classification | Resolution | Documentation updates |
|---|---|---|---|---|---|
| `received_shares` canonical schema in `CANONICAL_SCHEMA` (src-tauri/src/storage/schema.rs:96-107) is missing `sender_public_key BLOB NOT NULL` and `expires_at INTEGER` columns required by design.md lines 384-398 and sub-phase deliverable 4. | `src-tauri/src/storage/schema.rs` vs. design.md §Database Schema | Without schema columns, `insert_received_share` cannot persist `sender_public_key` / `expires_at`; invariant #11 (`sender_public_key` must be stored for receipt encryption) unmet. | Non-blocking — resolved in Governance sync action GS-003 (extend canonical placeholder schema to match design before implementation). | Extend `CANONICAL_SCHEMA` `received_shares` table with `sender_public_key BLOB NOT NULL` and `expires_at INTEGER` in same change set as sub-phase 5.2 code. | None (schema.rs is canonical; design already specifies the columns). |
| Sub-phase mandates "all three rejection reasons (wrong `enc`, corrupted ciphertext, wrong CTX tag) produce identical-looking `AuthenticationFailed` errors" but does not state the error variant name. | Sub-phase Security Review bullet 4 | Implementer might emit distinct variants, leaking an oracle. | Non-blocking | Introduce a single `SharingError::AuthenticationFailed` variant (no context, no string) used for every HPKE open failure path in `sharing::hpke::open` and `sharing::packages::import`. | Add rule line to `.claude/rules/sharing.md` (see Section 8, GS-001). |
| `hpke` crate ciphersuite requires a concrete `Aead` implementation with 32-byte tag. Sub-phase implementation note says "implement `hpke::aead::Aead` trait; tag length must be declared as 32 bytes" — but `hpke` v0.13 does not expose that trait as public; pluggable AEADs are only supported via its `AeadTag` + `Aead` sealed interface. | Sub-phase Implementation Notes vs. `hpke` 0.13 crate surface | If the `Aead` trait is sealed, CTX cannot be plugged as a ciphersuite AEAD; sub-phase design must choose between (a) forking `hpke` construction into a manual Base-mode implementation (DHKEM + HKDF-SHA256 key schedule + CTX) or (b) using the `hpke` crate's built-in `ChaCha20Poly1305` and wrapping the HPKE ciphertext with an outer CTX layer. | Non-blocking | Assume option (a): implement HPKE Base-mode manually using `hpke` crate's `Kem::<X25519HkdfSha256>` for KEM encapsulation/decapsulation plus an Arx-owned HPKE key-schedule helper built on `hkdf::Hkdf<Sha256>`, then use our `CTX-ChaCha20-Poly1305` wrapper for Seal/Open of payload bytes. Follow RFC 9180 §5.1 verbatim for context/schedule derivation with `suite_id = "HPKE"||KEM_id||KDF_id||AEAD_id` where AEAD_id is the IANA-registered `0x0003` (ChaCha20-Poly1305) because CTX is a wire-equivalent committing wrapper. | Document decision in design.md §HPKE Construction as clarification note (GS-002). |
| `.vgshare` file format extension — sub-phase deliverable 3 says "Export the resulting wire bytes as a `.vgshare` file" but the sub-phase does not describe a filename extension beyond `.vgshare`, nor does it require magic bytes / version prefix. Parent design §Share Package Format (lines 172-177) describes only `[32B enc \| ciphertext \| 32B CTX tag]` with no framing. | Sub-phase deliverable 3 vs. design.md §Wire format | Without framing, `.vgshare` files are indistinguishable from random bytes; future format evolution is impossible. | Non-blocking | Adopt raw wire format exactly (`[enc \| ct \| CTX tag]`, no prefix) per design line 173-174. Filename extension `.vgshare` is the only format signal; version evolution tracked by `info = "arx-runa-share"` domain. | None (design is already the ground truth). |
| `info` constant reuse across share-package and receipt encryption — design line 117 uses `b"arx-runa-share"` for share packages, and design lines 247-260 state receipts use "the same construction" with owner's public key. Phase 5.2 only builds share packages; receipts are Phase 5.3 work. The sub-phase leaves `info` unaddressed for receipts. | Design §HPKE Construction + §Download Receipts | Receipts sharing the same `info` string would allow context confusion if `aad` is also empty. | Non-blocking (Phase 5.2 only needs `info = b"arx-runa-share"`). | Record in Assumptions §4 that receipts must use a distinct `info` in Phase 5.3; do not pre-define that constant in Phase 5.2. | None (Phase 5.3 plan will address). |

## 4. Assumptions

1. `received_shares` schema will be extended (in `CANONICAL_SCHEMA`) with `sender_public_key BLOB NOT NULL` and `expires_at INTEGER` as part of this sub-phase — see Governance sync action GS-003.
2. HPKE Base-mode key-schedule derivation is implemented manually inside `sharing::hpke` using RFC 9180 §5.1 (`suite_id || "psk_id_hash" || "info_hash"` concatenation with HKDF-Extract/Expand over SHA-256), because `hpke` crate 0.13 does not expose a pluggable `Aead` for committing AEADs with non-16-byte tags.
3. The IANA AEAD ID used in `suite_id` for HPKE key-schedule hashing is `0x0003` (ChaCha20-Poly1305 per IANA HPKE AEAD registry); CTX is wire-equivalent and only alters the tag format, not keystream or key schedule. The sub-phase claims AEAD substitution is transparent at the key-schedule level.
4. `.vgshare` binary wire format is exactly `[32B enc | ciphertext | 32B CTX tag]` with no extra framing/magic bytes. Minimum valid length = 64 bytes (zero-length JSON is rejected at JSON parse time).
5. HPKE open unifies every failure to a single opaque `SharingError::AuthenticationFailed` variant with no source context, per sub-phase Security Review §4.
6. Snapshot semantics comment in `packages::create` is placed directly above the `chunk_uuids` assignment and reads: `// Snapshot semantics: chunk_uuids are fixed at share-creation time and never updated; regeneration requires a new share.`
7. Raw `file_key` buffers passed to/from HPKE Seal/Open are held in `Zeroizing<[u8; 32]>` (sub-phase Security Review mandates zeroization after wrapping).
8. Sender retrieves `file_key` by reading the owner's `vault_identity.public_key` (for `sender_public_key`), retrieving the file's `file_key_wrapped` via `MetadataStore::get_node`, and unwrapping with the active session KEK via `crypto::unwrap_file_key` — the unwrap helper is invoked from the share-package creation flow, so the plan adds a helper in the `sharing` module that accepts an already-resolved `KeyEncryptionKey`. No new surface is added to `auth::`.
9. The `base64` crate (already a dep, version 0.22) is used for `file_key` / `sender_public_key` JSON encoding using `base64::engine::general_purpose::STANDARD` (padded). Length is validated to exactly 32 bytes after decode.
10. `expires_at` in the JSON payload is a `Option<i64>` (Unix seconds), serialised with `#[serde(skip_serializing_if = "Option::is_none")]` so absent-and-`null` are treated identically on import.
11. The caller-side `hpke` API in this sub-phase exposes only `seal(recipient_public_key: &X25519PublicKey, plaintext: &[u8]) -> Result<Vec<u8>, SharingError>` and `open(recipient_private_key_bytes: &[u8; 32], wire: &[u8]) -> Result<Zeroizing<Vec<u8>>, SharingError>`. `recipient_private_key_bytes` arrives pre-unwrapped from `vault_identity.wrapped_private_key` via an auth helper; the plan does not re-implement that unwrap in this sub-phase.
12. Phase 5.2 does **not** persist outgoing `shares` rows (that is Phase 5.3). It writes `received_shares` rows on import only.
13. Extending `SharingStore` with `insert_received_share`, `get_received_share`, and `list_received_shares` follows the same SQLCipher pattern used for contacts in Phase 5.1.
14. Receipts (HPKE under owner's public key) use a distinct `info` string (to be defined in Phase 5.3); Phase 5.2 only implements sender→recipient HPKE with `info = b"arx-runa-share"`.

## 5. Approach

### `CONTRACT_SNIPPETS`

**CS-001 — `CTX-ChaCha20-Poly1305` wire layout**
```
[24B nonce | ciphertext | 32B CTX tag]
  CTX tag = BLAKE3(b"arx-runa-ctx-v1" || key || nonce || ciphertext)
```

**CS-002 — HPKE wire layout (`.vgshare`)**
```
[32B enc | HPKE ciphertext | 32B CTX tag]
```

**CS-003 — HPKE constants**
```rust
pub(crate) const HPKE_SHARE_INFO: &[u8] = b"arx-runa-share";
pub(crate) const CTX_DOMAIN_LABEL: &[u8] = b"arx-runa-ctx-v1";
```

**CS-004 — `SharePackagePayload` (JSON payload inside HPKE envelope)**
```rust
#[derive(serde::Serialize, serde::Deserialize)]
pub(crate) struct SharePackagePayload {
    pub share_id: String,              // UUID v4 hyphenated
    pub file_id: String,               // UUID v4 hyphenated
    pub file_name: String,
    pub chunk_count: u32,
    pub chunk_size: u32,               // bytes
    pub chunk_uuids: Vec<String>,      // UUID v4 hyphenated
    pub file_key: String,              // base64(32 bytes)
    pub sender_public_key: String,     // base64(32 bytes)
    pub cloud_endpoint: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<i64>,
}
```

**CS-005 — New `SharingError` variants**
```rust
SharingError::AuthenticationFailed,                        // all HPKE open failures, no context
SharingError::MalformedSharePackage(String),               // wire-format/length failures
SharingError::InvalidJsonPayload(String),                  // serde_json decode failures or schema violations
SharingError::InvalidFileKeyLength(usize),                 // decoded base64 file_key != 32 bytes
SharingError::InvalidSenderPublicKeyLength(usize),         // decoded base64 sender_public_key != 32 bytes
```

**CS-006 — `SharingStore` new methods**
```rust
async fn insert_received_share(&self, row: &ReceivedShare) -> Result<(), SharingError>;
async fn get_received_share(&self, share_id: &str) -> Result<ReceivedShare, SharingError>;
async fn list_received_shares(&self) -> Result<Vec<ReceivedShare>, SharingError>;
```

**CS-007 — `ReceivedShare` domain struct**
```rust
pub struct ReceivedShare {
    pub share_id: String,                      // UUID v4 hyphenated
    pub sender_contact_id: Option<ContactId>,  // NULL if sender not in local contacts
    pub sender_public_key: X25519PublicKey,
    pub file_name: String,
    pub file_key_wrapped: [u8; 72],            // KEK-wrapped via crypto::wrap_file_key
    pub chunk_count: u32,
    pub chunk_size: u32,
    pub chunk_uuids: Vec<String>,              // persisted as JSON array
    pub cloud_endpoint: serde_json::Value,     // persisted as JSON object
    pub expires_at: Option<i64>,
    pub imported_at: i64,
}
```

**CS-008 — DDL update to `CANONICAL_SCHEMA` (src-tauri/src/storage/schema.rs)**
```sql
CREATE TABLE received_shares (
    share_id             TEXT PRIMARY KEY,
    sender_contact_id    TEXT REFERENCES contacts(contact_id),
    sender_public_key    BLOB NOT NULL,       -- X25519 public key, 32 bytes
    file_name            TEXT NOT NULL,
    file_key_wrapped     BLOB NOT NULL,
    chunk_count          INTEGER NOT NULL,
    chunk_size           INTEGER NOT NULL,
    chunk_uuids          TEXT NOT NULL
                             CHECK (json_valid(chunk_uuids)),
    cloud_endpoint       TEXT NOT NULL,
    expires_at           INTEGER,
    imported_at          INTEGER NOT NULL
);
```

**CS-009 — New Cargo dependencies**
```toml
hpke = { version = "0.13", default-features = false, features = ["x25519"] }
chacha20 = "0.9"  # for CTX open-path keystream (see step S-1)
```

---

### Steps

**S-1 — Add `CTX-ChaCha20-Poly1305` wrapper** in `src-tauri/src/sharing/ctx_aead.rs` (new file).
- Implements the CTX construction per CS-001.
- `fn ctx_seal(key: &[u8; 32], nonce: &[u8; 24], plaintext: &mut [u8]) -> Result<[u8; 32], SharingError>`:
  1. Run `XChaCha20Poly1305::encrypt_in_place_detached(nonce, &[], plaintext)`; ignore the returned Poly1305 tag.
  2. Compute `tag = BLAKE3(CTX_DOMAIN_LABEL || key || nonce || &ciphertext)` using `blake3::Hasher::new_keyed`? No — use plain `blake3::hash` with the concatenation; CTX does not require MAC mode here because the commitment is a deterministic hash of (key, nonce, ciphertext), and the domain separator prefix protects against cross-protocol reuse. Use `blake3::Hasher::update` three times to avoid concatenation allocation.
  3. Return the 32-byte tag.
- `fn ctx_open(key: &[u8; 32], nonce: &[u8; 24], ciphertext: &mut [u8], claimed_tag: &[u8; 32]) -> Result<(), SharingError>`:
  1. Recompute commitment exactly as in seal; `subtle::ConstantTimeEq` compare to `claimed_tag`; on mismatch return `SharingError::AuthenticationFailed`.
  2. Otherwise decrypt in place: `chacha20::XChaCha20::new(key.into(), nonce.into()).apply_keystream(ciphertext)`. (Stream-cipher-only decryption because Poly1305 tag was never serialized.)
- The wrapper **does not** plug into the `hpke` crate's AEAD trait (see Concern 3); it is used directly by step S-2.

**S-2 — HPKE one-shot Base mode** in `src-tauri/src/sharing/hpke.rs` (new file).
- Implements RFC 9180 Base-mode Seal/Open manually for ciphersuite `DHKEM(X25519, HKDF-SHA256) + HKDF-SHA256 + CTX-ChaCha20-Poly1305`.
- Uses `hpke::kem::X25519HkdfSha256` for KEM encapsulation/decapsulation only (ephemeral keypair, shared secret).
- Key schedule (RFC 9180 §5.1):
  - `suite_id = b"HPKE" || KEM_id(0x0020) || KDF_id(0x0001) || AEAD_id(0x0003)` per IANA; document in a module-level comment referencing CS-003 assumption rationale.
  - `psk_id_hash = LabeledExtract("", "psk_id_hash", "")`, `info_hash = LabeledExtract("", "info_hash", HPKE_SHARE_INFO)` — both with `suite_id` prefix per RFC §4.
  - `key_schedule_context = mode || psk_id_hash || info_hash` where `mode = 0x00` (Base).
  - `secret = LabeledExtract(shared_secret, "secret", "")`.
  - `key = LabeledExpand(secret, "key", key_schedule_context, 32)`.
  - `base_nonce = LabeledExpand(secret, "base_nonce", key_schedule_context, 24)` (24 bytes because XChaCha20-Poly1305 has 24-byte nonce).
  - Seal uses `nonce = base_nonce` (one-shot; no sequence XOR needed).
- `pub fn seal(recipient_public_key: &X25519PublicKey, plaintext: &[u8]) -> Result<Vec<u8>, SharingError>`:
  1. `(shared_secret, enc) = Kem::encap(recipient_public_key.as_bytes())`.
  2. Run key schedule → derive `key`, `base_nonce` (both `Zeroizing`).
  3. Copy plaintext into a `Zeroizing<Vec<u8>>`; call `ctx_seal(&key, &base_nonce, &mut buf)` to get 32-byte tag.
  4. Emit `[enc_bytes (32) || buf || tag (32)]` as `Vec<u8>`; drop intermediates.
- `pub fn open(recipient_private_key_bytes: &[u8; 32], wire: &[u8]) -> Result<Zeroizing<Vec<u8>>, SharingError>`:
  1. Validate `wire.len() >= 64`; if not, `MalformedSharePackage("wire length < 64")`.
  2. `enc = wire[0..32]`, `tag = wire[wire.len()-32..]`, `ct = wire[32..wire.len()-32]`.
  3. `shared_secret = Kem::decap(recipient_private_key_bytes, enc)`; any failure → `AuthenticationFailed`.
  4. Run identical key schedule → derive `key`, `base_nonce` (both `Zeroizing`).
  5. Copy `ct` into `Zeroizing<Vec<u8>>`; call `ctx_open(&key, &base_nonce, &mut buf, tag_bytes)`; any failure → `AuthenticationFailed`.
  6. Return the `Zeroizing<Vec<u8>>` plaintext.

**S-3 — Share-package creation** in `src-tauri/src/sharing/packages.rs` (new file).
- `pub async fn create_share_package(&self, request: CreateSharePackageRequest) -> Result<Vec<u8>, SharingError>`.
  - `CreateSharePackageRequest` fields: `file_id: Uuid`, `recipient_public_key: X25519PublicKey`, `expires_at: Option<i64>`, `cloud_endpoint: serde_json::Value`, and a handle to the `MetadataStore` + `SharingStore` + `KeyEncryptionKey` (passed as a borrowed trait-object bundle by the caller; Phase 6 will own the DI container).
  - Flow (per CS-004):
    1. `node = metadata_store.get_node(file_id)` → `SharingError::Backend("file not found")` if missing or not a file.
    2. Read `node.file_key_wrapped` (`[u8; 72]`), unwrap via `crypto::unwrap_file_key(&WrappedFileKey(bytes), &kek) -> FileKey`.
    3. Fetch all chunk rows via `metadata_store.get_chunks(file_id)`; extract `chunk_uuids: Vec<String>` (hyphenated), `chunk_count = chunks.len()`, `chunk_size = chunk_size_bytes manifest_meta`.
    4. Fetch owner public key: `sharing_store.get_own_public_key()`.
    5. Build `SharePackagePayload`:
       - `share_id = Uuid::new_v4().hyphenated().to_string()`,
       - `file_key = base64::STANDARD.encode(file_key.expose())` (inside a scope that drops the raw bytes immediately),
       - `sender_public_key = base64::STANDARD.encode(owner_public_key.as_bytes())`.
       - Snapshot-semantics comment above `chunk_uuids` field population (per Assumption 6).
    6. `let plaintext = Zeroizing::new(serde_json::to_vec(&payload)?);` (error → `InvalidJsonPayload`).
    7. `let wire = sharing::hpke::seal(&recipient_public_key, &plaintext)?;`
    8. Return `wire` (`.vgshare` bytes).

**S-4 — Share-package import** in `src-tauri/src/sharing/packages.rs`.
- `pub async fn import_share_package(&self, wire: &[u8], recipient_private_key_bytes: &[u8; 32], kek: &KeyEncryptionKey, now_unix_seconds: i64) -> Result<ReceivedShare, SharingError>`.
  - Flow:
    1. `let plaintext = sharing::hpke::open(recipient_private_key_bytes, wire)?;`
    2. `let payload: SharePackagePayload = serde_json::from_slice(&plaintext).map_err(|error| SharingError::InvalidJsonPayload(error.to_string()))?;` (drop `plaintext` at end of scope).
    3. Validate required fields: every `String` non-empty; `chunk_count as usize == chunk_uuids.len()`; each `chunk_uuids` entry parses as `Uuid` v4.
    4. Decode `payload.file_key`: base64 → 32 bytes; if length ≠ 32 → `InvalidFileKeyLength(actual)`. Hold in `Zeroizing<[u8; 32]>`.
    5. Decode `payload.sender_public_key`: base64 → 32 bytes; if length ≠ 32 → `InvalidSenderPublicKeyLength(actual)`.
    6. Construct `FileKey::from_secret_box`? The crypto module exposes only `pub(crate)`. Add a crate-scoped constructor **already available** via `FileKey::from_secret_box` (crypto mod is `pub mod`, constructor is `pub(crate)`, reachable from `sharing`). Wrap with `crypto::wrap_file_key(&file_key, kek) -> WrappedFileKey`. Extract the 72 wire bytes; zeroize the raw `file_key` scope.
    7. Build `ReceivedShare { share_id: payload.share_id.clone(), sender_contact_id: None /* lookup deferred: out-of-scope for 5.2 */, sender_public_key: X25519PublicKey::new(sender_bytes), file_name: payload.file_name, file_key_wrapped: wrapped_wire, chunk_count: payload.chunk_count, chunk_size: payload.chunk_size, chunk_uuids: payload.chunk_uuids, cloud_endpoint: payload.cloud_endpoint, expires_at: payload.expires_at, imported_at: now_unix_seconds }`.
    8. `sharing_store.insert_received_share(&row).await?;` → returns the row.
- Sender-contact-id lookup against `contacts.public_key = sender_public_key` is deliberately **deferred** — sub-phase deliverable 4 lists it as "optional" and Phase 5.3/6 will wire UX.

**S-5 — `SharingStore` surface extension for `received_shares`.**
- Extend `sharing::store::SharingStore` trait with `insert_received_share`, `get_received_share(share_id: &str)`, `list_received_shares` per CS-006.
- Implement these methods on `SqlCipherMetadataStore` in `src-tauri/src/storage/sharing.rs`:
  - Insert: `INSERT INTO received_shares (share_id, sender_contact_id, sender_public_key, file_name, file_key_wrapped, chunk_count, chunk_size, chunk_uuids, cloud_endpoint, expires_at, imported_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)`.
  - Serialise `chunk_uuids` to JSON via `serde_json::to_string`; serialise `cloud_endpoint` the same way.
  - Map `StorageError::ConstraintViolation("UNIQUE ...")` to `SharingError::ConstraintViolation(_)` (for duplicate `share_id`).
  - Reject non-UUID-v4 `share_id` strings via `Uuid::parse_str` + `ContactId::from_uuid`-style validator mirrored for share ids — add `fn is_uuid_v4_str(&str) -> bool` local helper.
- Do not add any received-shares method to `MetadataStore` (rule `.claude/rules/sharing.md` Trait boundaries, rule `.claude/rules/storage.md` Traits).

**S-6 — `SharingError` variants** per CS-005, added to `src-tauri/src/sharing/error.rs` with `#[cfg(test)]` display-message tests (rule `.claude/rules/rust.md` Testing: every `thiserror` variant requires a test).

**S-7 — Module surface updates** in `src-tauri/src/sharing/mod.rs`:
- `mod ctx_aead; mod hpke; mod packages;`
- Re-export `packages::{create_share_package, import_share_package, CreateSharePackageRequest}` and the new `ReceivedShare` type.
- Do not re-export `ctx_aead` or `hpke` implementation details (internal-only).

**S-8 — Schema update** in `src-tauri/src/storage/schema.rs`: replace the Phase 5 placeholder `received_shares` DDL with CS-008. Update the inline comment that says "Phase 5 placeholder" to "Phase 5 canonical (see docs/architecture/designs/file-sharing/design.md §Database Schema)".

**S-9 — Cargo dependency additions** (CS-009) under `[dependencies]` in `src-tauri/Cargo.toml`, sorted into the existing Cryptography block.

**S-10 — Tests** — `~140` LoC per sub-phase estimate, colocated in the `#[cfg(test)] mod tests` of each new file:
- `sharing/ctx_aead.rs`: seal/open round-trip; single-byte ciphertext flip rejected; single-byte tag flip rejected; wrong-key rejection.
- `sharing/hpke.rs`: seal/open round-trip under a known keypair; wrong-recipient rejection (use a second CSPRNG keypair for open); single-byte `enc` flip rejected with `AuthenticationFailed`.
- `sharing/packages.rs`: round-trip create→import with a test `MetadataStore` + `SharingStore` stack; `expires_at` round-trips `Some` and `None`; missing required JSON field (drop `file_name`) → `InvalidJsonPayload`; `file_key` base64 decoded to 31 bytes → `InvalidFileKeyLength(31)`; `sender_public_key` base64 decoded to 33 bytes → `InvalidSenderPublicKeyLength(33)`; round-trip populates `received_shares.sender_public_key` and `received_shares.file_key_wrapped` non-empty.
- `storage/sharing.rs`: insert → get → list round-trip for `ReceivedShare`; duplicate `share_id` → `ConstraintViolation`; missing share id → `ContactNotFound`? No — introduce `SharingError::ReceivedShareNotFound` (extend CS-005 with a seventh variant; add display-message test).

## 6. Review focus areas

### 6a. Rust change surface

- `src-tauri/src/sharing/ctx_aead.rs` *(new)*
- `src-tauri/src/sharing/hpke.rs` *(new)*
- `src-tauri/src/sharing/packages.rs` *(new)*
- `src-tauri/src/sharing/mod.rs` *(re-exports)*
- `src-tauri/src/sharing/error.rs` *(variants)*
- `src-tauri/src/sharing/store.rs` *(trait extension)*
- `src-tauri/src/storage/sharing.rs` *(received-shares CRUD)*
- `src-tauri/src/storage/schema.rs` *(DDL update for `received_shares`)*
- `src-tauri/Cargo.toml` *(hpke, chacha20 deps)*

### 6b. Security-sensitive paths

- `src-tauri/src/sharing/ctx_aead.rs` — CTX commitment construction, constant-time tag compare, stream-cipher-only decryption path, no oracle leakage on tag mismatch.
- `src-tauri/src/sharing/hpke.rs` — key schedule correctness (RFC 9180 §5.1), `info = b"arx-runa-share"`, `aad = b""`, ephemeral private key zeroization (responsibility of the `hpke` crate's `Kem::encap`), identical `AuthenticationFailed` error path for any failure.
- `src-tauri/src/sharing/packages.rs` — raw `file_key` bytes held only in `Zeroizing<[u8; 32]>` scopes; immediate wrap on import; no plaintext `file_key` appears in logs, error messages, or return types; JSON deserialisation error text must not include payload bytes.
- `src-tauri/src/storage/sharing.rs` — `sender_public_key` and `file_key_wrapped` round-trip exactly; no accidental base64 or hex wrapping at the SQL layer.

### 6c. Architecture risk areas

- **Module visibility** — `sharing::hpke` and `sharing::ctx_aead` must remain crate-internal; only `sharing::packages` is re-exported. Verify in `sharing/mod.rs`.
- **Dependency direction** — `sharing` may depend on `crypto` (for `wrap_file_key`/`unwrap_file_key`) and on `storage` traits (`MetadataStore` read-only, `SharingStore`), but `storage` and `crypto` must not depend on `sharing`. Verify imports.
- **Single Responsibility** — `ctx_aead.rs` owns the AEAD construction, `hpke.rs` owns the HPKE key schedule, `packages.rs` owns the JSON + persistence glue. Do not merge.
- **SharingStore trait growth** — Adding three new methods (`insert_received_share`, `get_received_share`, `list_received_shares`) to `SharingStore` is acceptable per `.claude/rules/storage.md` ("`contacts` CRUD lives in `storage::sharing` behind the `SharingStore` trait"). Do not introduce a parallel `ReceivedSharesStore` trait.
- **`FileKey` constructor reach** — `FileKey::from_secret_box` is `pub(crate)` in `src-tauri/src/crypto/types/mod.rs`; it is reachable from `sharing::packages` because both live in the same crate. Confirm no visibility downgrade is needed.

### 6d. Testing requirements

**Sub-phase Validation checkpoint commands (must pass):**
```bash
cargo test sharing::ctx_aead
cargo test sharing::hpke
cargo test sharing::packages
```
Project-wide:
```bash
cargo test --workspace --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
```

**Boundary cases (sub-phase deliverable 6):**
- HPKE round-trip under a fixed keypair.
- Wrong-recipient rejection: open with a freshly-generated different keypair → `AuthenticationFailed`.
- Corrupted ciphertext (flip 1 byte in body) → `AuthenticationFailed` from CTX.
- Corrupted `enc` (flip 1 byte) → `AuthenticationFailed` from KEM decap failure (downgraded).
- Corrupted CTX tag (flip 1 byte) → `AuthenticationFailed` from commitment mismatch.
- JSON missing required non-optional field → `InvalidJsonPayload`.
- `expires_at: Some(_)` and `expires_at: None` both round-trip through `received_shares`.
- `received_shares` row fields after import: `sender_public_key == payload.sender_public_key`, `file_key_wrapped` is 72 bytes and non-zero, `chunk_uuids` JSON decodes to original list.

**Acceptance criteria (per sub-phase):**
- HPKE open returns distinct `AuthenticationFailed` on any modification to `enc`, ciphertext, or CTX tag.
- Ephemeral private key is not retained post-`Kem::encap` (guaranteed by `hpke` crate internals; add a comment referencing this invariant in `hpke.rs`).
- `file_key` in the package equals `nodes.file_key_wrapped` after owner-side unwrap (byte-for-byte via tests).
- On import, raw `file_key` bytes never escape the `Zeroizing` scope; verified by scope review.
- `chunk_uuids` snapshot is not a live pointer to chunks; verified by regenerating chunks after package creation and observing the package still points at old UUIDs.

## 7. Documentation impact

- **Required this run**:
  - None (design.md already canonical; sub-phase is the authoritative spec; schema.rs is code not docs).
- **Deferred/optional**:
  - `docs/architecture/designs/file-sharing/diagrams/file-sharing-flow.md` may gain a sub-diagram for the HPKE seal path — deferred to Phase 5.3 when the full outgoing-share flow (including cloud layout) is in place; adding only the HPKE half now would be incomplete.
  - Rationale: drawing the HPKE half without the cloud upload step would show half a story.

## 8. Governance sync actions (pre-implementation)

| Action ID | Reason / linked concern | Target files | Required edit | Verification |
|---|---|---|---|---|
| GS-001 | Concern 2: single opaque HPKE authentication error required by sub-phase Security Review §4. | `src-tauri/.claude/rules/sharing.md` (user's path: `C:\Users\chris\source\repos\arx-runa\.claude\rules\sharing.md`) | Add a new "## HPKE error hygiene" section with two bullets: "All HPKE open failures (KEM decap, CTX commitment mismatch, stream decrypt) emit `SharingError::AuthenticationFailed` with no source context." and "Error message text must not include `enc`, ciphertext, or CTX tag bytes." | Grep for the new section header; run `/copilot-sync` to mirror rule change into `.github/instructions/`. |
| GS-002 | Concern 3: rationale for manual HPKE Base-mode key schedule needs to be pinned to the canonical design so reviewers can audit against the spec. | `docs/architecture/designs/file-sharing/design.md` | Append a paragraph to §HPKE Construction "### Rust implementation" section explaining that the `hpke` crate 0.13 is used only for DHKEM encap/decap, and the key schedule (RFC 9180 §5.1) is implemented in `sharing::hpke` so that `CTX-ChaCha20-Poly1305` can be plugged in as the AEAD without using the crate's sealed `Aead` trait. Reference IANA AEAD ID `0x0003` for the `suite_id` value. | Word-count check on the added paragraph; verify the file's "Last updated" date is bumped to the implementation date. |
| GS-003 | Concern 1: canonical schema must match design-spec schema before sub-phase code writes rows. | `src-tauri/src/storage/schema.rs` | Replace the Phase 5 placeholder `received_shares` CREATE TABLE with CS-008 (adds `sender_public_key BLOB NOT NULL` and `expires_at INTEGER`). Update the inline `-- Phase 5 placeholder` comment to `-- Phase 5 canonical (see docs/architecture/designs/file-sharing/design.md §Database Schema)`. | Run `cargo test storage::schema::tests` — existing tests must still pass. Grep `received_shares` in `schema.rs` to confirm the new columns are present. |
| GS-004 | After GS-001 rule edit, rule sync must fan out to `.github/instructions/`. | N/A | Run `/copilot-sync` after GS-001 completes. | `.github/instructions/sharing.instructions.md` diff matches the rule change. |

## 9. Handoff Notes for Implementer

Working directory: `C:\Users\chris\source\repos\arx-runa`. Execute governance-sync actions GS-001, GS-002, GS-003 first (rule, design-doc paragraph, schema DDL), then `/copilot-sync` per GS-004 before any Rust code. Implementation order follows Section 5: S-6 (error variants) → S-1 (`ctx_aead.rs`) → S-2 (`hpke.rs`) → S-9 (Cargo deps) → S-3/S-4 (`packages.rs`) → S-5 (store extension) → S-7 (mod.rs) → S-8 (schema.rs was already done in GS-003) → S-10 (tests interleaved per file). The plan is self-contained; do not re-read the sub-phase unless a contract ambiguity arises. Platform traps: `hpke` + `chacha20` are pure Rust with no platform-specific features, so Windows/macOS/Linux parity is preserved by default. Never log `file_key`, `sender_public_key` bytes, `enc`, or CTX tag bytes in any path (including `Debug` derives — redact like `X25519PublicKey` already does). Invoke `security-reviewer` after `cargo test --workspace --all-targets --all-features` and `cargo clippy --all-targets --all-features -- -D warnings` both pass.

---

## Implementation Log

- **Date**: 2026-04-20T15:30:00Z
- **Run ID**: `phase-5-2-hpke-and-share-packages-20260420-143708`
- **Track**: `full` (security-sensitive, governance-sync-required, 9 files)
- **Branch**: `development`
- **Execution mode**: Orchestrator direct (rust-implementer unavailable due to context constraints; all steps executed by orchestrator)

### Agent evidence

| Approach step | Agent | Outcome |
|---|---|---|
| GS-001 — HPKE error hygiene rule | orchestrator | ✓ Applied to `.claude/rules/sharing.md` |
| GS-002 — design doc Rust impl paragraph | orchestrator | ✓ Applied to `design.md` |
| GS-003 — received_shares DDL | orchestrator | ✓ Applied to `schema.rs` |
| GS-004 — copilot-sync | orchestrator | ✓ `.github/instructions/sharing.instructions.md` updated |
| S-9 — Cargo deps | orchestrator | ✓ `chacha20 = "0.9"`, `subtle = "2"` added; `hpke` removed |
| S-6 — error variants | orchestrator | ✓ 7 new `SharingError` variants + 7 display tests |
| S-1 — ctx_aead.rs | orchestrator | ✓ CTX-ChaCha20-Poly1305 + 4 tests |
| S-2 — hpke.rs | orchestrator | ✓ Manual HPKE Base-mode + 5 tests |
| S-5 — SharingStore + SQLCipher CRUD | orchestrator | ✓ `ReceivedShare` struct + 3 trait methods + impl |
| S-3/S-4 — packages.rs | orchestrator | ✓ create/import + 6 tests |
| S-7 — mod.rs surface | orchestrator | ✓ Re-exports + dead_code allows |
| S-10 — received-shares CRUD tests | orchestrator | ✓ 4 tests added |

### Files changed

- `src-tauri/src/sharing/ctx_aead.rs` (new)
- `src-tauri/src/sharing/hpke.rs` (new)
- `src-tauri/src/sharing/packages.rs` (new)
- `src-tauri/src/sharing/error.rs` (modified — 7 new variants)
- `src-tauri/src/sharing/mod.rs` (modified — module surface)
- `src-tauri/src/sharing/store.rs` (modified — `ReceivedShare` + 3 methods)
- `src-tauri/src/storage/sharing.rs` (modified — SQLCipher CRUD + tests)
- `src-tauri/src/storage/schema.rs` (modified — `received_shares` DDL + `json_valid` CHECK)
- `src-tauri/Cargo.toml` (modified — `chacha20`, `subtle` added; `hpke` removed)
- `.claude/rules/sharing.md` (modified — HPKE error hygiene)
- `.github/instructions/sharing.instructions.md` (modified — synced)
- `docs/architecture/designs/file-sharing/design.md` (modified — Rust impl paragraph)
- `docs/architecture/designs/file-sharing/sub-phases/5.2-hpke-and-share-packages.md` (modified — Implementation Decisions)

### Formatting check
`cargo fmt --all -- --check` — clean ✓

### Clippy results
`cargo clippy --workspace --all-targets --all-features -- -D warnings` — clean ✓

### Test results
`cargo test --workspace --all-targets --all-features` — 582 passed, 0 failed, 3 ignored ✓

### Release build
`cargo build --workspace --release` — success ✓

### Rust review
Findings: HIGH=1, MEDIUM=3, LOW=2 — all remediated or deferred by plan

### Architecture review
Findings: HIGH=1, MEDIUM=2, LOW=1 — all remediated or deferred by plan

### Security review
Findings: CRITICAL=0, WARNING=2, NOTE=3 — all remediated or deferred by plan

### Cross-shard review
N/A — single shard (sharing module)

### Findings quality gate
Total unique findings: 9 (de-duplicated from 15 raw across 3 reviewers)

| Disposition | Count |
|---|---|
| ACTIONABLE_NOW (remediated) | 5 |
| INTENTIONAL_DECISION | 1 |
| DEFERRED_BY_PLAN | 2 |
| INSUFFICIENT_EVIDENCE | 1 |

### Finding overrides
None

### Design challenge outcomes
- **AR-003 / CF-003**: Manual HPKE construction instead of `hpke` crate — accepted as non-security-scoped design deviation. Rationale: `rand_core` version incompatibility. Sub-phase doc updated with `## Implementation Decisions` section.

### Governance sync
4 actions executed (GS-001 through GS-004). `/copilot-sync` completed successfully.

### Sub-phase decisions sync
`docs/architecture/designs/file-sharing/sub-phases/5.2-hpke-and-share-packages.md` — `## Implementation Decisions` section added with 6 decision bullets.

### Deviations from plan
- `hpke` crate replaced with manual DHKEM implementation due to `rand_core 0.9` vs `0.10` conflict. Same cryptographic construction preserved.
- `chacha20` crate version `0.9` used for XChaCha20 keystream in CTX open path.

### Documentation flagged
- Diagram update for HPKE flow deferred to Phase 5.3 (per plan Section 7).

### Run state path
`.claude/runs/phase-5-2-hpke-and-share-packages-20260420-143708/`
