---
name: Pending decisions
description: Open design questions not yet resolved
type: project
---

*No pending decisions at this time.*

## Resolved decisions

- **USB key file format**: Resolved in design document. VoidGate does not impose
  a filename — the user may rename or move the file. Auto-detection uses BLAKE3
  fingerprint matching (32-byte file + hash comparison), not filename.
  See: `docs/architecture/designs/authentication-and-session-management.md`
