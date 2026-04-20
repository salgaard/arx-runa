# Design Reviews

Critical reviews of Arx Runa's architecture design documents. Each review re-examines a design against academic literature, production systems, and implementation correctness — surfacing missed opportunities, weaker alternatives, and open concerns.

These are not specifications. They are adversarial readings intended to stress-test design choices before implementation.

## Documents

- **[Cryptographic Primitives](cryptographic-primitives-review.md)** — Critical review of cipher selection (XChaCha20-Poly1305), nonce strategy, HKDF key derivation, per-file key model, and AAD construction against academic literature and known production failures.
- **[Authentication and Session Management](authentication-and-session-management-review.md)** — Critical review of the password-based vault unlock flow, Argon2id parameterisation, session token lifecycle, and timeout handling.
- **[Chunking and Manifest](chunking-and-manifest-review.md)** — Critical review of the 4 MiB chunk scheme, manifest encryption, padding strategy, and metadata privacy guarantees.
- **[Project Scaffolding](project-scaffolding-review.md)** — Critical review of the declared Rust dependency set, crate versions, Rust edition 2024 constraints, and ecosystem compatibility.
