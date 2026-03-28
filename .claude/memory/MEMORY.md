# VoidGate — Agent Memory

_Maintained by subagents. Do not edit manually._
_Append findings under the correct section._
_Curate when file exceeds 200 lines — remove stale entries._

## Pending decisions
- USB key file: fixed filename/format vs. arbitrary-looking file (plausible
  deniability trade-off vs. fingerprinting risk) — open question
- Chunk size: 4MB vs 8MB — balance storage overhead against anonymisation
  (padding waste on small files must be quantified in report). Downstream
  effects: AAD chunk_index range, upload latency, Rclone parallelism

## Architecture decisions made
- Encryption: XChaCha20-Poly1305 via `chacha20poly1305` crate
  (`XChaCha20Poly1305` type)
  - Rejected AES-256-GCM: catastrophic failure on nonce reuse (leaks auth
    key, breaking both confidentiality and integrity); hardware dependency
    (AES-NI) trades portability for speed
  - XChaCha20 chosen for: 192-bit nonce enabling safe random-per-chunk
    generation without state tracking; less catastrophic nonce-reuse failure
    mode (confidentiality loss only, not auth key compromise); naturally
    constant-time ARX operations (no timing side channels in software)
  - Trade-off accepted: slower on AES-NI hardware; better cross-platform,
    defensive posture, and nonce-reuse resilience
  - Report note: document as linked cryptographic design decision — cipher
    choice and nonce strategy are co-dependent
  - Reference: RFC 8439 (ChaCha20-Poly1305), draft-irtf-cfrg-xchacha
- Nonce strategy: random 192-bit nonce per chunk via CSPRNG
  - Birthday bound at 2⁹⁶ — collision probability negligible for any
    practical workload
  - Rejected sequential counters: require persistent state, risk reuse on
    state loss/crash, leak write ordering information
  - Rejected metadata-derived nonces: deterministic derivation causes reuse
    when re-encrypting updated content for the same chunk position
- Chunk wire format: [24-byte nonce | ciphertext | 16-byte Poly1305 tag]
  - Each chunk is self-contained — can be decrypted in isolation without
    external nonce lookup
  - Note: the `chacha20poly1305` crate's `encrypt()` returns ciphertext || tag
    as a single blob; nonce is prepended by our code, not the library
- AAD (Authenticated Associated Data): file_id || chunk_index
  - Bound on every AEAD encrypt/decrypt call
  - Prevents chunk reordering, swapping, or duplication by a malicious cloud
    provider — each chunk is tied to its position within a specific file
  - Without AAD, any chunk decrypts successfully with the correct key
    regardless of placement, enabling silent file corruption
- USB auth: key file (32 bytes random entropy), not device serial number
  - Serial number is an identifier, not crypto material — spoofable
  - Key file combined with password via Argon2id KDF
  - Lost key file recovery: Recovery Phrase
  - Key file is a mandatory cryptographic factor — not optional, not
    downgraded to an authentication gate for convenience
  - Rejected password-only-by-default model: makes the key file an access
    control gate rather than a cryptographic input; attacker with stolen
    password gets full access. TOTP/authenticator apps also rejected — they
    are non-deterministic and cannot be used as KDF input
  - UX mitigation: USB read once at session start, session keys held in
    mlocked memory, session timeout zeroes keys and requires re-auth.
    Same model as KeePassXC with a key file
  - Report note: document the UX-vs-security trade-off and cite KeePassXC
    as prior art for mandatory key file with session caching
- Key derivation tree: HKDF-SHA256 (RFC 5869) from master key
  - master_key = Argon2id(password, key_file, salt)
  - chunk_key     = HKDF(master, info=b"voidgate-chunk-encryption")
  - sqlcipher_key = HKDF(master, info=b"voidgate-sqlcipher")
  - manifest_key  = HKDF(master, info=b"voidgate-manifest-backup")
  - Key separation: compromise of one derived key does not compromise others
  - Cost: three HKDF calls (microseconds) — negligible vs Argon2id
  - Rejected single-key model: using one key for all purposes means a
    vulnerability in one context (e.g. chunk AAD construction) could
    compromise everything. Key separation is standard practice (LUKS, Signal)
  - Reference: RFC 5869 (HKDF)
- Manifest: SQLCipher database (local, encrypted via sqlcipher_key)
  - Schema: `nodes` (virtual filesystem tree: node_id, parent_id, node_type,
    name, created_at, modified_at, size_bytes), `chunks` (blob mapping:
    chunk_id, node_id, chunk_index, blob_name, size_padded, BLAKE3 checksum),
    `manifest_meta` (key-value: schema_version, vault_id, snapshot_counter,
    last_synced_at)
  - Rejected alternatives: flat JSON (no query/indexing, not crash-safe),
    custom binary format (reinventing a database), sled/RocksDB (less
    structure, more code for relational queries)
  - Filenames stored as plaintext TEXT inside SQLCipher — rejected double
    encryption (name_enc) as unnecessary complexity; SQLCipher already
    encrypts the entire database
- Manifest cloud backup: encrypted with manifest_key, uploaded as a blob
  - Enables recovery on a new device: download vault header → read salt →
    prompt password + USB key file → Argon2id → HKDF → decrypt manifest
  - Snapshot model: atomic export of full SQLCipher DB after each batch
    of operations. No incremental diffs, no merge logic (bachelor scope)
  - snapshot_counter: monotonic, incremented on each push. Foundation for
    future multi-device conflict detection
- Vault header: unencrypted JSON blob stored alongside manifest in cloud
  - Contains: vault_id, schema_version, argon2_salt, argon2_params
  - Solves bootstrap chicken-and-egg: salt is needed to derive keys, but
    without keys you can't decrypt the manifest where the salt would live
  - Safe to store in plaintext: salt doesn't help without password + key file,
    Argon2 params are public by design (same principle as password hashes)
- Blob naming: random UUID v4 per chunk
  - No relation to file identity, chunk index, or content
  - Cloud provider cannot correlate blobs to files or infer structure
- BLAKE3 checksum: stored per chunk in manifest, computed over encrypted blob
  - Pre-decrypt integrity check — catches cloud storage corruption before
    it surfaces as a cryptic AEAD tag failure
  - Not a security feature (AEAD tag handles authenticity) — operational UX
- File deletion: immediate — delete chunk blobs from cloud + remove node
  and chunk rows from manifest. ON DELETE CASCADE handles chunk cleanup.
  UI should show a confirmation warning before permanent deletion
- Chunking: fixed-size uniform padding (not CDC)
  - CDC leaks size information as a side channel; fixed chunks do not
  - Storage overhead is a known trade-off — must be quantified in report
- Memory protection: mlock/VirtualLock + zeroize crate + secrecy crate
  - zeroize: compiler-resistant zeroing (analogous to OPENSSL_cleanse)
  - secrecy::Secret<T>: prevents accidental logging/printing of key material
  - Explicit threat model boundary: does NOT protect against cold boot attacks
    or a compromised OS kernel — must be stated in threat model section
- Language: Rust
  - Deterministic memory management without GC — critical for Zero-Trace
  - GC languages (C#, Java) may retain plaintext heap copies indefinitely
  - Risk acknowledged: steep learning curve, longer dev time for bachelor scope
  - Mitigation: build a vertical prototype early (login → decrypt one file →
    show in UI) before expanding breadth

## Patterns and conventions discovered
(populated by agents as they work)

## Known gotchas
- The `chacha20poly1305` crate returns ciphertext || tag as one blob from
  `encrypt()` — do not manually append the tag or you will double it
- AAD mismatch between encrypt and decrypt will cause silent auth failure —
  ensure file_id and chunk_index are serialised identically on both paths
- Vault header must be uploaded BEFORE the manifest blob — a new device
  needs the salt first to derive keys
- BLAKE3 checksum is over the encrypted blob (nonce + ciphertext + tag),
  not over plaintext — verify checksum before attempting decryption
(further entries populated by agents as they work)

## Key crate versions
(populated once Cargo.toml is established)
