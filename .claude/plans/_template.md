---
title: ""
created: ""
status: draft
roadmap-phase: null
sub-phase: null
design-document: null
sub-phase-roadmap: null
implementation-delegation: direct
rust-review-agent-required: false
security-agent-required: false
solution-agent-required: false
test-agent-required: false
governance-sync-required: false
tags: []
---

# Plan: {title}

## Goal

One sentence describing what is being built or changed.

## Context

What exists today. What constraints apply. If roadmap-linked, includes the phase
objective, dependencies, deliverables, and any pending architectural decisions.

## Design Concerns / Open Questions

List blocking and non-blocking concerns discovered during planning.

## Assumptions

List explicit assumptions an implementer must not guess.

## Approach

Step-by-step implementation plan with file paths.

1.
2.
3.

## Rust quality review implications

- **Expected Rust change surface:** [files / directories]
- **Invoke rust-reviewer agent?** YES / NO
  - Rationale: [why]
- **What the reviewer should check:** [single-responsibility, module boundaries, error handling, etc.]

## Security implications

- **Expected sensitive path set:** [files / directories, or "None anticipated"]
- **Invoke security-reviewer agent?** YES / NO
  - Rationale: [why]
- **What the reviewer should check:** [specific focus list]

## Findings-to-fix synthesis implications

- **Invoke problem-solver agent?** YES / NO
  - Rationale: [why]
  - Default: if rust-reviewer or security-reviewer is invoked, set YES.
  - Hard rule: if set to NO while rust-reviewer or security-reviewer is YES, include the required override line below.
- **Solver override justification:** [Required only when problem-solver is NO and any reviewer is YES. Explain why direct reviewer -> rust-implementer handoff is safer here.]
- **When the solver runs:** [e.g., "after reviewer findings in each remediation round"]
- **Handoff contract to implementer:** [Solver mode: require `IMPLEMENTATION_PACK` / `NO_ACTIONABLE_FIXES` / `BLOCKED_SOLUTIONS`; Direct override mode: explicit "reviewer findings -> rust-implementer" statement]

## Execution and testing strategy

**Test scope:**
- [ ] Basic unit tests (written during implementation)
- [ ] Adversarial tests (cryptographic edge cases, corrupted data, wrong keys)
- [ ] Property-based tests (proptest for randomized input validation)
- [ ] Integration tests
- [ ] Boundary cases (0 bytes, 1 byte, chunk_size-1, chunk_size, chunk_size+1, exact multiples)

**Coverage target:** [Specify if >80% required for security-critical modules]

**Boundary cases to cover:**
- [List specific edge cases relevant to this implementation]

**Invoke test-writer agent?**
- [ ] **YES** — requires adversarial or property-based tests
  - Reason: [Explain why — e.g., "crypto module needs AAD mismatch, tag tampering, nonce uniqueness tests"]
- [ ] **NO** — tests written during implementation are sufficient
  - Reason: [Explain why — e.g., "simple CRUD operations, no security-critical edge cases"]

**Test acceptance criteria:**
- [List specific pass/fail criteria — e.g., "All adversarial tests must fail gracefully, not panic"]

## Documentation impact

Which `docs/` files need creating or updating after implementation.

## Governance sync actions (pre-implementation)

Ordered, machine-actionable updates to `.claude/rules`, `.claude/agents`,
`.claude/reference`, or mirrored instruction files. If none, state "None."

## Implementation execution mode

- `direct` (default) or `delegated`
- If `delegated`, list which Approach steps can be delegated to `rust-implementer`
  and which steps remain with the orchestrator.

## Handoff Notes for Implementer

One short paragraph for an agent with zero conversation context: working
directory, execution order, and known traps.
