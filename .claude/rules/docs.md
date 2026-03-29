---
paths:
  - "docs/**"
---

# Documentation — scoped rules

These rules apply to all files under `docs/`.

## Register and tone
- Academic register — this is a bachelor's project in software development
- Assume a technically literate reader — do not over-explain standard concepts
- Define terms on first use in each document
- Do not pad — if the answer is short, keep it short

## Terminology
Use correct established names:
- AEAD, KDF, HKDF, CSPRNG, AAD (Authenticated Associated Data), nonce, IV,
  MAC, zero-knowledge, XChaCha20-Poly1305, Argon2id, SQLCipher
- Reference standards by name when relevant: NIST SP 800-63, OWASP ASVS,
  RFC 5869 (HKDF), RFC 8439 (ChaCha20-Poly1305), draft-irtf-cfrg-xchacha,
  FIPS 197 (AES)

## Citations
- Flag any claim needing a citation with: `<!-- CITE: suggested source -->`
- When making security claims, cite an established standard or prior art

## Architecture Decision Records (docs/architecture-decisions/)
Use this structure for every ADR:

```markdown
## Decision-NNN: Title
**Date / Status**

### Context
What problem are we solving? What constraints apply?

### Decision
What did we choose and why?

### Consequences
What trade-offs did we accept? What risks exist? What should we monitor?

### References
- RFC or NIST reference
- Relevant crate documentation
- Prior art (KeePassXC, LUKS, Signal, etc.)
```

File names: `NNN-kebab-case-title.md` — no abbreviations in the title.
Example: `001-cipher-selection.md`, not `001-aes-vs-xchacha.md`

## Naming
- No abbreviations in file names or headings
- `architecture-decisions`, not `adr`
- `decision-001`, not `adr-001`
- Established acronyms (AEAD, KDF, HKDF) are fine in headings and body text

## What belongs where
- `docs/architecture-decisions/` — every significant design choice with rationale
- `docs/architecture/` — system diagrams, key derivation tree, data flow,
  chunk pipeline, manifest schema
- `docs/threat-model/` — what we protect against, what is explicitly out of
  scope (cold boot, compromised OS kernel), trust boundaries
- `docs/guides/` — development setup, toolchain, debugging, workflows
- `README.md` — concise project overview; detailed content belongs in docs/
