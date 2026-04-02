# Phase [X.Y]: [Sub-Phase Title]

**Parent roadmap**: [roadmap.md](roadmap.md)  
**Design sections**: [Reference specific sections or line ranges from parent design.md]  
**Depends on**: [List prerequisite sub-phases with links, or "None" if this is the first]

---

## Deliverables

1. [Specific deliverable with file path, e.g., "`CloudTransport` trait definition in `src-tauri/src/storage/cloud/mod.rs`"]
2. [Another deliverable, e.g., "`CloudError` thiserror enum with specific variants"]
3. [Test deliverable, e.g., "Full test suite for `MockTransport` covering all trait methods"]

---

## Validation Checkpoint

**Automated tests**:
```bash
cargo test [specific test path or pattern]
```
[Coverage target if applicable]

**Manual verification** (if needed):
- [Manual test step 1, e.g., "Upload a test file to MinIO bucket"]
- [Manual test step 2, e.g., "Verify blob appears with correct UUID name"]

**Acceptance criteria**:
- [What must be true for this sub-phase to be considered complete]
- [Another acceptance criterion]

---

## Estimated Scope

- **Production code**: ~[N] lines
- **Test code**: ~[M] lines

---

## Implementation Notes

- [Note 1, e.g., "Use `#[async_trait]` for the trait"]
- [Note 2, e.g., "Temp files in VoidGate staging directory, not system temp"]
- [Gotcha or design clarification]

---

## Security Review

**[Required/Not required]** — [If required: "Invoke `security-reviewer` agent after implementation" + list security concerns to check]

---

## Next Sub-Phase

**[Phase [X.Y+1]: [Next Sub-Phase Title]]([X.Y+1]-filename.md)**
- Depends on: Phase [X.Y] ([this sub-phase])
- Implements: [Brief description of what comes next]

<!-- OR if this is the final sub-phase: -->

## Completion

This is the final sub-phase for Phase [X] ([Design Name]). After completion:
- [Key capability 1]
- [Key capability 2]
- Ready to proceed to Phase [X+1] ([Next Phase Name])
