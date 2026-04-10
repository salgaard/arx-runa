# Arx Runa: Authentication and Session Management — Critical Review

> **Document type**: Exploration / feasibility research
> **Status**: Concluded
> **Last updated**: 2026-04-07

Critical review of `docs/architecture/designs/authentication-and-session-management/design.md` against
academic literature, production systems, and implementation correctness.
Each design decision is re-examined for correctness, completeness, and
missed opportunities.

For the canonical design, see `docs/architecture/designs/authentication-and-session-management/design.md`.

---

## Table of Contents

1. [The Problem](#the-problem)
2. [Argon2id Parameters](#argon2id-parameters)
3. [Password and Key File Input Construction](#password-and-key-file-input-construction)
4. [HKDF Consistency with Crypto Review](#hkdf-consistency-with-crypto-review)
5. [impl Trait in DeviceMonitor Trait](#impl-trait-in-devicemonitor-trait)
6. [master_key Type-Level Zeroization](#master_key-type-level-zeroization)
7. [X25519 Private Key Wrapping — Missing AAD](#x25519-private-key-wrapping--missing-aad)
8. [Password Change Atomicity](#password-change-atomicity)
9. [Session State Concurrency Model](#session-state-concurrency-model)
10. [Memory Protection: mlock vs memfd_secret](#memory-protection-mlock-vs-memfd_secret)
11. [Recommendation](#recommendation)
12. [Decisions](#decisions)
13. [Open Questions](#open-questions)
14. [Sources](#sources)

---

## The Problem

The authentication and session management design was written before implementation. This document re-examines each design choice against current standards, Rust-specific correctness concerns, and security properties — to find gaps, confirm correct choices, and identify any changes worth making before Phase 2 implementation begins.

---

## USB Key File Generation — rand API

### What the design chose (original)

The key file generation section documented the CSPRNG call as `rand::thread_rng().fill_bytes(...)`. This is the `rand` 0.8 API.

### The problem

The scaffolding design pins `rand = "0.9"` — the same version already required by Rust edition 2024, where `gen` is a reserved keyword and `rand` 0.8's `.gen()` method causes a compile error (covered in the cryptographic primitives review). In `rand` 0.9, `thread_rng()` was renamed to `rng()` and the fill interface changed:

| API | `rand` version | Compiles in edition 2024 |
|-----|---------------|--------------------------|
| `rand::thread_rng().fill_bytes(&mut buf)` | 0.8 | No — `thread_rng` removed |
| `rand::rng().fill(&mut buf)` | 0.9 | Yes — correct |
| `rand::rng().random::<[u8; 32]>()` | 0.9 | Yes — array form |

`rand::rng().fill(&mut buf)` is the idiomatic choice for writing random bytes into an existing buffer. `random::<[u8; 32]>()` is clean for fixed-size array construction, but since key file generation writes into a buffer that is immediately written to disk, the `fill` form more clearly expresses the intent.

**Status: Fixed. CSPRNG call updated to `rand::rng().fill` in the design.**

---

## DeviceMonitor Trait — dyn Dispatch Compatibility

### What the design chose (original)

The `DeviceMonitor` trait was defined with a return-position `impl Trait` (RPITIT) method:

```rust
trait DeviceMonitor: Send + Sync {
    fn watch(&self) -> impl Stream<Item = DeviceEvent>;
}
```

RPITIT was stabilised in Rust 1.75 (December 2023) and is valid Rust, but it has a critical consequence: any trait with an `impl Trait` return is **not dyn-safe**. Attempting to use `Box<dyn DeviceMonitor>` fails to compile, because the vtable cannot represent an opaque return type whose size and layout are unknown until monomorphisation.

### Why dyn dispatch is required

The design has three concrete implementations of `DeviceMonitor`: `WindowsDeviceMonitor`, `LinuxDeviceMonitor`, and `MockDeviceMonitor`. The OS-specific implementation is selected at runtime — this is the canonical use case for trait objects. Without `Box<dyn DeviceMonitor>`, the OS selection would have to be encoded as a generic parameter that propagates through every type that holds a monitor, producing significant complexity for no benefit.

### Alternatives evaluated

| Pattern | dyn-safe | Ergonomics | Notes |
|---------|----------|------------|-------|
| `Pin<Box<dyn Stream<…> + Send>>` return | Yes | Minimal boxing per call | **Chosen** — keeps trait simple and dyn-safe |
| Associated type (`type WatchStream: Stream<…>`) | Yes | More verbose impls | Works, but each impl must name its stream type explicitly |
| `where Self: Sized` on the method | Partially | Method unavailable on dyn objects | Not useful — `watch` is the only method |
| Generic at call site (`SessionManager<M: DeviceMonitor>`) | N/A — no dyn | Propagates generic up the stack | Awkward for runtime OS selection |

### The fix

Boxing the return type at the trait boundary is the standard Rust idiom for dyn-safe async/stream methods. Each concrete implementation boxes its own stream type internally; callers work uniformly with `Pin<Box<dyn Stream<Item = DeviceEvent> + Send>>`. The allocation is per `watch()` call — once at session start — so the overhead is negligible.

```rust
trait DeviceMonitor: Send + Sync {
    fn watch(&self) -> Pin<Box<dyn Stream<Item = DeviceEvent> + Send>>;
}
```

**Status: Fixed. Both `DeviceMonitor` trait definitions updated in the design.**

---

## HKDF Derivation — Missing Salt

### What the design chose (original)

The auth design's HKDF derivation diagram showed only the `info` strings:

```
master_key → HKDF(info: "arx-runa-key-encryption")  → key_encryption_key
           → HKDF(info: "arx-runa-sqlcipher")        → sqlcipher_key
           → HKDF(info: "arx-runa-manifest-backup")  → manifest_key
```

No salt was shown.

### The gap

The cryptographic primitives design review (concluded, see `docs/research/cryptographic-primitives-review.md`) identified that RFC 5869 §3.1 recommends a fixed salt even when the IKM (here, `master_key`) has high entropy, to act as a domain separator. The fix applied in that review set the HKDF salt to `b"arx-runa-v1"` throughout the crypto design. The auth design's diagrams and vault creation flow steps were not updated to match, leaving an inconsistency between the two canonical design documents.

The auth design is where `master_key` is produced and where an implementer will first encounter the HKDF derivation call. If they read only the auth design, they would derive keys with an empty salt — producing different keys than an implementation that read the crypto design first.

### Fix

The HKDF diagram was updated to show the salt explicitly, and vault creation flow steps 11-13 were updated to include `salt: b"arx-runa-v1"`. A brief rationale note cross-references the crypto design for the full justification.

**Status: Fixed. Salt added to HKDF diagram and vault creation flow in the auth design.**

---

## Password Change Atomicity

### What the design chose (original)

The password change flow listed re-wrapping file_keys (step 8), re-wrapping the X25519 private key (step 9), and re-keying SQLCipher (step 10) as sequential independent operations with no transaction boundary specified.

### The gap

If the process is interrupted between steps 8 and 10 — power failure, OS kill, application crash — the vault can be left in an inconsistent state:

- **Scenario A**: transaction commits partway (some `file_key_wrapped` rows updated, others not). Re-opening the vault with the old credentials would fail for any file whose key was already re-wrapped with the new KEK.
- **Scenario B**: all `file_key_wrapped` rows updated but `PRAGMA rekey` not yet executed. The vault opens with the old SQLCipher key, reads file_keys wrapped under the new KEK (which it cannot unwrap — the old KEK is gone), and all files become inaccessible.

Either scenario leaves the vault unrecoverable without a prior backup.

### Fix: SQLCipher transaction + staged rekey

The correct order is:

1. **Transaction**: re-wrap all `file_key_wrapped` rows and the X25519 private key within a single SQLCipher `BEGIN`/`COMMIT`. SQLCipher's WAL journal means a crash before `COMMIT` leaves the old rows intact. On failure, rollback — the vault is fully usable with the old credentials.
2. **After commit**: call `PRAGMA rekey`. This is a separate SQLCipher operation that re-encrypts the database file. At this point all wrapped keys have already been transitioned; the database content is consistent with the new KEK regardless of which key opens the database.

The key insight is that `PRAGMA rekey` changes which key opens the database, while the transaction changes what the database contains. These must happen in that order, with the transaction committing first.

**Status: Fixed. Password change and key file rotation flows updated with transaction boundary in the design.**

---

## Session Concurrency Model

### What the design chose (original)

The design described the timeout mechanism as: "The timer runs as a `tokio` task. When it fires, it sends a signal to the session manager to zero keys." No ownership type, locking primitive, or sharing model was specified.

### The gap

`SessionKeys` must be accessible to:
1. Tauri command handlers (concurrent — multiple IPC calls can be in flight)
2. The background timeout `tokio` task
3. Manual lock commands

Without a specified model, an implementer must guess how `SessionKeys` is shared and how "wait for in-progress operations" is mechanically enforced. Common wrong choices include:

- `Arc<Mutex<SessionKeys>>` without an `Option` — zeroing would require a custom `locked` flag, and `ZeroizeOnDrop` can't be relied on while the `Arc` has live clones
- Channels for zeroing signals — the timeout task sends a message, the receiver zeroes keys, but in-flight command handlers may still hold borrowed keys after the message is processed

### The fix: `Arc<RwLock<Option<SessionKeys>>>`

The correct model combines three properties:

| Property | Mechanism |
|----------|-----------|
| Concurrent reads | `RwLock` read guards — multiple command handlers can hold keys simultaneously |
| Serialised write (zero/authenticate) | `RwLock` write guard — exclusive, blocks until all readers release |
| Automatic zeroization on drop | `Option` set to `None` — dropping `SessionKeys` triggers `ZeroizeOnDrop` |

The "wait for in-progress operations" guarantee is not a separate mechanism — it falls out naturally from the write lock. When the timeout task acquires the write lock, it blocks until every active read guard is dropped. Read guards are held only for the duration of key access (not the entire file I/O), so the wait is brief.

**Status: Fixed. `SharedSession` type alias and usage pattern added to Session Management section of the design.**

---

## X25519 Private Key Wrapping

### What the design chose (original)

Step 18 of the vault creation flow stated "Wrap X25519 private key with `key_encryption_key` → store in SQLCipher" with no wire format, cipher, or AAD specification.

### The gap

The X25519 private key is a 32-byte secret stored in SQLCipher, wrapped under `key_encryption_key` — structurally identical to a `file_key_wrapped` record. The crypto design fully specifies the `wrap_file_key` format: XChaCha20-Poly1305, empty AAD, producing a 72-byte blob (24-byte nonce | 32-byte ciphertext | 16-byte tag). Without an explicit reference, an implementer reading only the auth design has no specification for the wire format. The empty AAD is acceptable here for the same reason as `file_key_wrapped`: the wrapped key is self-contained and stored in SQLCipher, so substitution requires first breaking the database encryption.

### Fix

A parenthetical note was added to step 18 directing the implementer to the `wrap_file_key` format in the crypto design. No separate format is defined — reusing the existing wrapping primitive keeps the implementation consistent and avoids proliferating wire formats.

**Status: Fixed. Step 18 now references the crypto design's `wrap_file_key` wire format.**

---

## master_key Type-Level Zeroization

### What the design chose (original)

The design described `master_key` informally as "32 bytes, Argon2id output, held in mlocked memory" and specified that it is zeroed with `zeroize(master_key)` immediately after HKDF expansion.

### The gap

A manual `zeroize()` call only runs if that specific line of code is reached. If the HKDF expansion returns an error between deriving the first key and the last — or if an early `?` operator propagates before the manual call — `master_key` remains in memory until the stack frame is cleaned up by the allocator (with no guarantee of zeroing). This is not a theoretical concern: the `hkdf` crate can return an error if the output length exceeds the HKDF maximum.

The crypto design already uses `Zeroizing<[u8; 32]>` for all sensitive key buffers. Typing `master_key` as `Zeroizing<[u8; 32]>` closes this gap at the language level — the `ZeroizeOnDrop` impl runs unconditionally when the variable goes out of scope, including all error paths.

### Why Zeroizing<T> is the right choice

`Zeroizing<T>` from the `zeroize` crate wraps any type implementing `Zeroize` and adds `Drop` that calls `zeroize()`. For `[u8; 32]`, this overwrites the buffer with zeros before the memory is reclaimed. The wrapper has no runtime overhead beyond the zeroing itself and does not prevent the value from being used normally.

The alternative — a custom `MasterKey` newtype with `ZeroizeOnDrop` — would also work but is more boilerplate for a value that lives in exactly one scope and is never passed across a function boundary.

**Status: Fixed. `master_key` typed as `Zeroizing<[u8; 32]>` in the HKDF diagram and explanatory note added.**

---

## Memory Protection: mlock vs memfd_secret

### What the design chose

`mlock` on Linux, `VirtualLock` on Windows. Both prevent the OS from swapping session key pages to disk.

### memfd_secret — stronger Linux alternative

Linux 5.14 (August 2021) introduced the `memfd_secret()` syscall, which creates a "secretmem" region with a stronger protection property: the pages are removed from the kernel's direct-map page tables. This means even a compromised kernel module or `ptrace`-capable process cannot read the memory by walking the direct map. `mlock` only prevents eviction — the pages remain kernel-readable.

The `secure-types` crate on crates.io abstracts over both: it allocates with `memfd_secret` when the kernel supports it (Linux 5.14+) and falls back to `mlock` on older kernels or other platforms. This would provide a transparent upgrade for Linux users without any API changes to Arx Runa.

### Why no change is needed now

`mlock`/`VirtualLock` is the correct choice for the current design:
- Cross-platform (Windows and Linux covered by the existing model)
- Well-understood semantics with an established Rust ecosystem (`secrecy` crate)
- `memfd_secret` is Linux-only and requires kernel 5.14+; most current desktop Linux distributions qualify, but it adds a platform-specific code path

`memfd_secret` is worth revisiting as an opt-in hardening measure for the Linux path in Phase 9 (hardening). The upgrade would be a crate change with no design impact.

**Verdict: mlock is correct for the current design. memfd_secret noted as a Phase 9 hardening candidate.**

---

## Argon2id Parameters

### What the design chose

m=19456 KiB (19 MiB), t=2, p=1. The design states these are OWASP minimums.

### Verification against current standards

| Parameter set | Source | m | t | p |
|---------------|--------|---|---|---|
| Design (minimum) | Arx Runa auth design | 19456 KiB | 2 | 1 |
| OWASP minimum (2024-2025) | Password Storage Cheat Sheet | 19456 KiB | 2 | 1 |
| OWASP alternative | Password Storage Cheat Sheet | 46592 KiB | 1 | 1 |

The design's parameters exactly match the current OWASP minimum. The cheat sheet has not changed these values since 2022. The alternative configuration (46 MiB, 1 iteration) trades memory for iterations at equivalent security level.

**NIST note**: NIST SP 800-63B expresses a preference for BALLOON (which uses NIST-approved hash primitives) over Argon2id for strict federal compliance contexts. This is not relevant to Arx Runa, which targets personal cloud storage, not federal certification.

**Verdict: Argon2id parameters are correct and current. No change.**

---

<!-- Sections written during the session below -->

---

## Recommendation

The authentication and session management design is **well-structured and security-conscious** in its fundamentals: two-tier authentication, Argon2id with correct parameters, HKDF-SHA256 key separation, mlocked session keys, hard failure on mlock unavailability, and a clean separation between credential change and blob re-encryption. No conceptual rethinking is required.

Seven actionable findings were identified and resolved:

| # | Finding | Severity | Resolution |
|---|---------|----------|------------|
| 1 | `rand::thread_rng().fill_bytes` — stale rand 0.8 API | Bug | Updated to `rand::rng().fill` |
| 2 | `DeviceMonitor::watch` returning `impl Stream` — not dyn-safe | Bug | Updated to `Pin<Box<dyn Stream<…> + Send>>` |
| 3 | HKDF diagram missing `b"arx-runa-v1"` salt | Gap | Salt added to diagram and vault creation flow |
| 4 | Password change re-wrap not transactional | Gap | Transaction + staged rekey pattern specified |
| 5 | Session concurrency model unspecified | Gap | `Arc<RwLock<Option<SessionKeys>>>` added to design |
| 6 | X25519 private key wrapping format unspecified | Improvement | Cross-reference to `wrap_file_key` wire format added |
| 7 | `master_key` relies on manual zeroize — misses error paths | Improvement | Typed as `Zeroizing<[u8; 32]>` |
| 8 | memfd_secret stronger than mlock on Linux 5.14+ | Note | Documented as Phase 9 hardening candidate |
| 9 | Argon2id parameters confirmed against OWASP 2024-2025 | Note | No change — parameters are current |

The design is ready for Phase 2 implementation.

---

## Decisions

> Choices made during this research session. Updated as the session progresses.

| Decision | Alternatives considered | Rationale |
|---|---|---|
| CSPRNG call updated to `rand::rng().fill(&mut buf)` | `rand::rng().random::<[u8; 32]>()` (array form — also correct) | `fill` is idiomatic when writing into an existing buffer; consistent with `rand` 0.9 already required by edition 2024 |
| `DeviceMonitor::watch` returns `Pin<Box<dyn Stream<Item = DeviceEvent> + Send>>` | RPITIT (`impl Stream`) — not dyn-safe; associated type — more verbose; generics — propagates up the stack | Boxed return keeps the trait dyn-safe for OS-specific runtime dispatch; allocation cost is negligible (called once at session start) |
| HKDF salt `b"arx-runa-v1"` added to auth design diagrams and vault creation flow | Cross-reference only (note pointing to crypto design) | Auth design is the first place an implementer encounters the HKDF call; inconsistency with the crypto design would produce different key material depending on which doc was read |
| Password change: re-wrap all file_keys in a SQLCipher transaction; call `PRAGMA rekey` only after commit | Document as known limitation; staged backup approach | Transaction + staged rekey is the only approach that leaves the vault recoverable on any failure mode; SQLCipher WAL makes this reliable with no extra complexity |
| Session shared as `Arc<RwLock<Option<SessionKeys>>>` | `Arc<Mutex<…>>` (serialises reads unnecessarily) | `RwLock` allows concurrent command handler reads; write lock for timeout/auth enforces "wait for operations" without a separate signal mechanism; `Option` drop triggers `ZeroizeOnDrop` |
| X25519 private key wrapping references `wrap_file_key` wire format (XChaCha20-Poly1305, empty AAD, 72 bytes) | Separate format with AAD binding to vault_id; leave unspecified | Same structure as `file_key_wrapped`; reusing the existing primitive avoids proliferating formats; empty AAD acceptable because the wrapped key is inside SQLCipher |
| `master_key` typed as `Zeroizing<[u8; 32]>` | Manual `zeroize()` call; custom newtype with `ZeroizeOnDrop` | `Zeroizing` wrapper guarantees zeroing on all drop paths including early returns on HKDF error; no boilerplate; consistent with `zeroize` usage in the crypto design |

---

## Open Questions

---

## Sources

| Source | Topic | URL |
|---|---|---|
| OWASP Password Storage Cheat Sheet (2024-2025) | Argon2id minimum parameters: m=19456, t=2, p=1 | https://cheatsheetseries.owasp.org/cheatsheets/Password_Storage_Cheat_Sheet.html |
| NIST SP 800-63B, §5.1.1.2 | Memory-hard KDF SHOULD requirement; BALLOON preference for federal compliance | https://pages.nist.gov/800-63-3/sp800-63b.html |
| RFC 5869, "HMAC-based Extract-and-Expand Key Derivation Function (HKDF)" | Fixed salt recommendation for high-entropy IKM; domain separator rationale | https://datatracker.ietf.org/doc/html/rfc5869 |
| argon2 crate v0.5.3 (RustCrypto) | `hash_password_into` API for raw-bytes output | https://docs.rs/argon2/latest |
| rand crate v0.9 (RustCrypto) | `rng().fill()` replaces `thread_rng().fill_bytes()`; edition 2024 compatibility | https://docs.rs/rand/0.9 |
| Rust Blog: "async fn and return-position impl Trait in traits" (Dec 2023) | RPITIT stabilised in Rust 1.75; dyn-safety limitation | https://blog.rust-lang.org/2023/12/21/async-fn-rpit-in-traits/ |
| Linux man page: memfd_secret(2) | secretmem design; removal from kernel direct-map | https://man7.org/linux/man-pages/man2/memfd_secret.2.html |
| LWN.net: "Secret memory for userspace" (2021) | memfd_secret motivation and kernel implementation | https://lwn.net/Articles/865256/ |
| secure-types crate | `memfd_secret` + `mlock` abstraction with automatic fallback | https://crates.io/crates/secure-types |
| zeroize crate (RustCrypto) | `Zeroizing<T>` wrapper; `ZeroizeOnDrop` semantics | https://docs.rs/zeroize/latest |
