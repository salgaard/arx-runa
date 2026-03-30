---
name: Architecture rationale
description: Why each major design decision was made — rejected alternatives, trade-offs, references
type: project
---

## Encryption: XChaCha20-Poly1305
- Rejected AES-256-GCM: catastrophic failure on nonce reuse (leaks auth
  key, breaking both confidentiality and integrity); hardware dependency
  (AES-NI) trades portability for speed
- XChaCha20 chosen for: 192-bit nonce enabling safe random-per-chunk
  generation without state tracking; less catastrophic nonce-reuse failure
  mode (confidentiality loss only, not auth key compromise); naturally
  constant-time ARX operations (no timing side channels in software)
- Trade-off accepted: slower on AES-NI hardware; better cross-platform,
  defensive posture, and nonce-reuse resilience
- Reference: RFC 8439 (ChaCha20-Poly1305), draft-irtf-cfrg-xchacha

## Nonce strategy: random 192-bit per chunk via CSPRNG
- Birthday bound at 2⁹⁶ — collision probability negligible for any practical workload
- Rejected sequential counters: require persistent state, risk reuse on
  state loss/crash, leak write ordering information
- Rejected metadata-derived nonces: deterministic derivation causes reuse
  when re-encrypting updated content for the same chunk position

## Chunk wire format: [24-byte nonce | ciphertext | 16-byte Poly1305 tag]
- Each chunk is self-contained — can be decrypted in isolation without
  external nonce lookup
- Note: the `chacha20poly1305` crate's `encrypt()` returns ciphertext || tag
  as a single blob; nonce is prepended by our code, not the library

## AAD (Authenticated Associated Data): file_id || chunk_index
- Prevents chunk reordering, swapping, or duplication by a malicious cloud
  provider — each chunk is tied to its position within a specific file
- Without AAD, any chunk decrypts successfully with the correct key
  regardless of placement, enabling silent file corruption

## USB auth: key file (32 bytes random entropy), not device serial number
- Serial number is an identifier, not crypto material — spoofable
- Key file combined with password via Argon2id KDF
- Lost key file recovery: Recovery Phrase
- Rejected password-only-by-default model: makes the key file an access
  control gate rather than a cryptographic input; attacker with stolen
  password gets full access. TOTP/authenticator apps also rejected — they
  are non-deterministic and cannot be used as KDF input
- UX mitigation: USB read once at session start, session keys held in
  mlocked memory, session timeout zeroes keys and requires re-auth.
  Same model as KeePassXC with a key file

## Key derivation tree: HKDF-SHA256 (RFC 5869) from master key
- master_key = Argon2id(password, key_file, salt)
- key_encryption_key = HKDF(master, info=b"voidgate-key-encryption")
- sqlcipher_key       = HKDF(master, info=b"voidgate-sqlcipher")
- manifest_key        = HKDF(master, info=b"voidgate-manifest-backup")
- Key separation: compromise of one derived key does not compromise others
- Rejected single-key model: using one key for all purposes means a
  vulnerability in one context could compromise everything. Key separation
  is standard practice (LUKS, Signal)
- Reference: RFC 5869 (HKDF)

## Manifest: SQLCipher database (local, encrypted via sqlcipher_key)
- Rejected alternatives: flat JSON (no query/indexing, not crash-safe),
  custom binary format (reinventing a database), sled/RocksDB (less
  structure, more code for relational queries)
- Filenames stored as plaintext TEXT inside SQLCipher — rejected double
  encryption (name_enc) as unnecessary complexity; SQLCipher already
  encrypts the entire database

## Manifest cloud backup: encrypted with manifest_key, uploaded as a blob
- Enables recovery on a new device: download vault header → read salt →
  prompt password + USB key file → Argon2id → HKDF → decrypt manifest
- Snapshot model: atomic export of full SQLCipher DB after each batch
  of operations. No incremental diffs, no merge logic (bachelor scope)
- snapshot_counter: monotonic, incremented on each push. Foundation for
  future multi-device conflict detection

## Vault header: unencrypted JSON blob stored alongside manifest in cloud
- Contains: vault_id, schema_version, argon2_salt, argon2_params, key_file_blake3
- Solves bootstrap chicken-and-egg: salt is needed to derive keys, but
  without keys you can't decrypt the manifest where the salt would live
- Safe to store in plaintext: salt doesn't help without password + key file,
  Argon2 params are public by design (same principle as password hashes)

## Blob naming: random UUID v4 per chunk
- No relation to file identity, chunk index, or content
- Cloud provider cannot correlate blobs to files or infer structure

## BLAKE3 checksum: stored per chunk in manifest, over encrypted blob
- Pre-decrypt integrity check — catches cloud storage corruption before
  it surfaces as a cryptic AEAD tag failure
- Not a security feature (AEAD tag handles authenticity) — operational UX

## File deletion: immediate
- Delete chunk blobs from cloud + remove node and chunk rows from manifest.
  ON DELETE CASCADE handles chunk cleanup.
- UI should show a confirmation warning before permanent deletion

## Chunking: fixed-size uniform padding (not CDC)
- CDC leaks size information as a side channel; fixed chunks do not
- Storage overhead is a known trade-off — must be quantified in report

## Memory protection: mlock/VirtualLock + zeroize + secrecy crates
- zeroize: compiler-resistant zeroing (analogous to OPENSSL_cleanse)
- secrecy::Secret<T>: prevents accidental logging/printing of key material
- Explicit threat model boundary: does NOT protect against cold boot attacks
  or a compromised OS kernel — must be stated in threat model section

## Language: Rust
- Deterministic memory management without GC — critical for Zero-Trace
- GC languages (C#, Java) may retain plaintext heap copies indefinitely
- Risk acknowledged: steep learning curve, longer dev time for bachelor scope
- Mitigation: build a vertical prototype early (login → decrypt one file →
  show in UI) before expanding breadth
