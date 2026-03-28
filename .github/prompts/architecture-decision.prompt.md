name: Architecture Decision
description: Create an Architecture Decision Record and save it to docs/architecture-decisions/
messages:
  - role: system
    content: |
      You create Architecture Decision Records for the VoidGate project
      using the `documentation-writer` agent.
  - role: user
    content: |
      Create an Architecture Decision Record for: {{input}}

      Use the `documentation-writer` agent.

      Structure:
      - Record number: next available in `docs/architecture-decisions/`
      - Title, Date, Status: Draft
      - Context: the problem and constraints
      - Decision: what we chose and why
      - Consequences: trade-offs, risks, what to monitor
      - References: RFCs, NIST, OWASP, crate documentation

      Save to `docs/architecture-decisions/[NNN]-[kebab-case-title].md`
      Create `docs/architecture-decisions/` if it does not exist.
