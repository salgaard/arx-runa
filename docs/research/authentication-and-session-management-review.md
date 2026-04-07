# Arx Runa: Authentication and Session Management — Critical Review

> **Document type**: Exploration / feasibility research
> **Status**: Draft
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

<!-- Sections written during the session below -->

---

## Recommendation

_Written at close-out._

---

## Decisions

> Choices made during this research session. Updated as the session progresses.

| Decision | Alternatives considered | Rationale |
|---|---|---|
| CSPRNG call updated to `rand::rng().fill(&mut buf)` | `rand::rng().random::<[u8; 32]>()` (array form — also correct) | `fill` is idiomatic when writing into an existing buffer; consistent with `rand` 0.9 already required by edition 2024 |
| `DeviceMonitor::watch` returns `Pin<Box<dyn Stream<Item = DeviceEvent> + Send>>` | RPITIT (`impl Stream`) — not dyn-safe; associated type — more verbose; generics — propagates up the stack | Boxed return keeps the trait dyn-safe for OS-specific runtime dispatch; allocation cost is negligible (called once at session start) |
| HKDF salt `b"arx-runa-v1"` added to auth design diagrams and vault creation flow | Cross-reference only (note pointing to crypto design) | Auth design is the first place an implementer encounters the HKDF call; inconsistency with the crypto design would produce different key material depending on which doc was read |
| Password change: re-wrap all file_keys in a SQLCipher transaction; call `PRAGMA rekey` only after commit | Document as known limitation; staged backup approach | Transaction + staged rekey is the only approach that leaves the vault recoverable on any failure mode; SQLCipher WAL makes this reliable with no extra complexity |

---

## Open Questions

---

## Sources

| Source | Topic | URL |
|---|---|---|
