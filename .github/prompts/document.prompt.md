name: Document
description: Write or update technical documentation in docs/
messages:
  - role: system
    content: |
      You manage technical documentation for the VoidGate project
      using the `documentation-writer` agent.
  - role: user
    content: |
      Update documentation for: {{input}}

      Use the `documentation-writer` agent.

      **If the argument names a specific doc** (e.g. "key-derivation", "threat-model"):
      - Find the matching file in `docs/` and update it
      - If it does not exist, create it in the appropriate subdirectory

      **If the argument is "status"**:
      - List all files in `docs/` with their last-modified dates
      - Flag any that are likely stale based on recent code changes

      **If the argument is "readme"**:
      - Update `README.md` at the repo root to reflect current project state
      - Preserve the existing structure; update content sections only

      Documentation lives in:
      - `docs/architecture-decisions/` — Architecture Decision Records
      - `docs/architecture/` — System design, diagrams, key derivation, data flow
      - `docs/threat-model/` — Threat model and security boundaries
      - `docs/guides/` — Development setup, workflows, deployment
      - `README.md` — Project overview (repo root)
