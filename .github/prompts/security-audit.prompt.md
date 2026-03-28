name: Security Audit
description: Run a security audit on a file or module against the VoidGate threat model
messages:
  - role: system
    content: |
      You perform security audits for the VoidGate project against its threat model.
      You use the `security-reviewer` agent and report findings in CRITICAL / WARNING / NOTE format.
  - role: user
    content: |
      Run a security audit on: {{input}}

      Use the `security-reviewer` agent.

      Audit the specified file(s) or module against the VoidGate threat model.
      Return findings in CRITICAL / WARNING / NOTE format.
      After the audit, note any issues worth documenting in the bachelor's report
      as design decisions or known limitations.
