# Deep Dives

In-depth technical explorations of specific design decisions in Arx Runa — covering cryptographic primitive choices, key recovery trade-offs, and padding strategies. Each document surveys alternatives, evaluates them against the zero-knowledge threat model, and records the rationale for what was chosen.

---

- [Cryptographic Primitive Rationale](../research/cryptographic-primitive-rationale.md) — Justification and alternative analysis for every cryptographic primitive in the design: XChaCha20-Poly1305, HKDF-SHA256, Argon2id, per-file key wrapping, BLAKE3 checksums, and `ZeroizeOnDrop` + `Secret<T>` memory protection.
- [File Sharing Cryptography](../research/file-sharing-cryptography.md) — Cryptographic decisions for Phase 5 file sharing: HPKE (RFC 9180) over ad-hoc ECIES, X25519 curve confirmation, CTX-ChaCha20-Poly1305 as the committing AEAD, and simplification of the share package envelope.
- [Password and Key Recovery](../research/password-and-key-recovery.md) — Feasibility survey of every known vault recovery mechanism (recovery phrases, Shamir's SSS, SLIP-39 shares, trusted-contact key wrapping, platform biometrics, cloud escrow) evaluated against the zero-knowledge threat model.
- [Reducing Padding Overhead](../research/padding-overhead-reduction.md) — Survey of all known techniques for reducing per-file padding waste: Padmé padding, tiered chunk sizes, smaller uniform chunk size, content-defined chunking (rejected — fingerprinting attacks), and epoch-based deferred batching.
