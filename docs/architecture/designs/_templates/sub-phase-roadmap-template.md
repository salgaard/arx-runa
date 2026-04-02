# Phase [N] Sub-Roadmap: [Design Topic]

---
**Parent design**: `docs/architecture/designs/<design-name>/design.md`  
**Created**: [YYYY-MM-DDTHH:MM:SSZ]  
**Status**: Draft  
**Implementation order**: [List sub-phases with strict/flexible dependency indicator]

---

## Overview

**Purpose**: This sub-phase roadmap decomposes the [design topic] design into independently testable implementation units, each with clear deliverables and validation checkpoints.

**Total sub-phases**: [N] (Phases [X.1] through [X.N])

**Rationale for decomposition**:
<!-- Explain why this design was decomposed. Check all that apply: -->
- [ ] **Size**: Exceeds ~100-150 lines ([actual line count])
- [ ] **Trait boundaries**: Multiple trait definitions implementable independently
- [ ] **Platform splits**: OS-specific implementations (Windows/Linux)
- [ ] **Integration breadth**: Touches [N]+ existing modules
- [ ] **Error surface**: Defines [N]+ distinct error variants
- [ ] **Multi-step flows**: Contains [N]+ operational flows

**Implementation strategy**: [Brief description of the overall approach, e.g., "Build foundational trait → mock implementation → concrete implementation → integration flows"]

---

## Dependency Graph

```
[X.1] → [X.2] → [X.3]
         ↓
        [X.4] → [X.5]
```

**Legend**:
- `→` strict dependency (must complete predecessor before starting)
- `↓` optional/flexible dependency (can start but may need predecessor output for full testing)

---

## Sub-Phase Definitions

### Phase [X.1]: [Sub-Phase Title]

**Design sections**: [Reference specific sections or line ranges from parent design]  
Example: "CloudTransport Trait, Error Handling (lines 47-135)"

**Depends on**: [List prerequisite sub-phases, or "None" if this is the first]

**Deliverables**:
1. [Specific deliverable with file path, e.g., "`CloudTransport` trait definition in `src-tauri/src/storage/cloud/mod.rs`"]
2. [Another deliverable, e.g., "`CloudError` thiserror enum with `NetworkError`, `NotFound`, `PermissionDenied` variants"]
3. [Test deliverable, e.g., "Full test suite for `MockTransport` covering all trait methods"]

**Validation checkpoint**:
<!-- Clear, measurable success criteria that the user can verify before moving to next sub-phase -->
- **Automated tests**: `cargo test [specific test path or pattern]` passes with [coverage target if applicable]
- **Manual verification**: [If needed, e.g., "Upload a test file to MinIO bucket; verify blob appears with correct UUID name"]
- **Acceptance criteria**: [What must be true for this sub-phase to be considered complete]

**Estimated scope**: ~[N] lines of production code, ~[M] lines of test code

**Implementation notes**:
<!-- Optional: Any specific guidance, gotchas, or design clarifications -->
- [Note 1]
- [Note 2]

---

### Phase [X.2]: [Sub-Phase Title]

**Design sections**: [Reference specific sections]

**Depends on**: Phase [X.1] ([brief reason, e.g., "needs CloudTransport trait definition"])

**Deliverables**:
1. [Deliverable]
2. [Deliverable]
3. [Deliverable]

**Validation checkpoint**:
- **Automated tests**: [Test command]
- **Manual verification**: [If needed]
- **Acceptance criteria**: [What must be true]

**Estimated scope**: ~[N] lines of production code, ~[M] lines of test code

**Implementation notes**:
- [Note]

---

### Phase [X.3]: [Sub-Phase Title]

<!-- Repeat structure as above -->

---

## Testing Strategy

### Per-Sub-Phase Testing
Each sub-phase includes its own test suite. Tests must pass before proceeding to the next sub-phase.

**Test types**:
- **Unit tests**: Core functionality in isolation
- **Mock-based tests**: Use mocks/stubs for dependencies not yet implemented
- **Property-based tests** (where applicable): Use `proptest` for adversarial inputs
- **Integration tests**: Once all sub-phases complete, end-to-end validation

### Regression Testing
After completing each sub-phase, run:
```bash
cargo test           # All tests must pass
cargo clippy --D warnings  # No new warnings
```

This ensures new code doesn't break earlier sub-phases.

---

## Security Review Checkpoints

<!-- Mark any sub-phases that require security review -->
- Phase [X.Y]: Requires `security-reviewer` agent review (touches crypto/auth/storage)
- Phase [X.Z]: No security review needed

---

## Documentation Impact

**Files to create/update after sub-phase completion**:
- Phase [X.1]: [doc file updates]
- Phase [X.2]: [doc file updates]
- Final: Update `docs/roadmap.md` to mark phase complete

---

## Notes and Considerations

### Design Clarifications
<!-- Any ambiguities in the parent design that need resolution during implementation -->
- [Clarification 1]

### Future Work
<!-- Items deferred or out-of-scope for this roadmap -->
- [Deferred item]

### Risks and Mitigations
<!-- Known risks and how to address them -->
| Risk | Mitigation |
|------|------------|
| [Risk description] | [How to handle] |

---

## References

- **Parent design**: `docs/architecture/designs/<design-name>/design.md`
- **Roadmap entry**: `docs/roadmap.md` Phase [N]
- **Related ADRs**: [If applicable]
