---
mode: agent
description: Run a security audit on a file or module against the VoidGate threat model
---

Run a security audit on: ${input:File(s) or module to audit (e.g. src-tauri/src/crypto/)}

Use the `security-reviewer` agent.

Audit the specified file(s) or module against the VoidGate threat model.
Return findings in CRITICAL / WARNING / NOTE format.
After the audit, note any issues worth documenting in the bachelor's report
as design decisions or known limitations.
