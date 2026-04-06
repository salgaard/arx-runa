# ADR 003: Sub-Phase Roadmap Workflow for Large Design Documents

**Status**: Accepted  
**Date**: 2026-04-02  
**Decision makers**: Project lead, AI implementation agents

---

## Context

The Arx Runa project uses a three-command workflow for converting architectural vision into working code:
1. `/design <topic>` → creates detailed technical designs in `docs/architecture/designs/`
2. `/plan <topic>` → generates implementation plans in `.claude/plans/`
3. `/implement-plan <file>` → executes via the `rust-implementer` agent

Analysis of the six existing design documents revealed significant size variation:

| Design Document | Lines | Tokens |
|----------------|-------|--------|
| `authentication-and-session-management.md` | 278 | ~5,217 |
| `chunking-and-manifest.md` | 284 | ~4,227 |
| **`cloud-synchronisation.md`** | **722** | **~12,559** |
| `cryptographic-primitives.md` | 343 | ~4,193 |
| `file-sharing.md` | 199 | ~4,233 |
| **`tauri-ipc-and-frontend.md`** | **879** | **~9,224** |

Two designs (`cloud-synchronisation.md` and `tauri-ipc-and-frontend.md`) are 2-3× larger than others and contain:
- 5-7 major components requiring separate files
- Multiple platform-specific implementations (Windows/Linux device monitors)
- Complex integration flows (upload/download, push/pull, conflict detection)
- 8+ distinct error variants requiring separate test coverage

### Problem Statement

When a 700+ line design is fed directly to `/implement-plan`, the implementation agent must maintain mental state across dozens of trait definitions, error paths, and integration points simultaneously. This increases the risk of:
- Inconsistent error handling across components
- Forgotten edge cases in tests
- Integration points that work in isolation but fail when composed
- All-or-nothing validation (user tests only after entire phase complete)

**User requirement**: "manually testable by me before moving on" — need checkpoints between implementation units to validate incrementally before proceeding.

---

## Decision

Introduce **sub-phase roadmaps** as an optional decomposition layer for large or logically separable design documents. Sub-phase roadmaps break a design into 3-5 independently testable implementation units, each with:
- Clear deliverables (specific files, functions, tests)
- Explicit dependencies (which prior sub-phases must complete first)
- Validation checkpoints (manual testing gates before proceeding)
- Manageable implementation scope (~100-200 lines)

### Implementation

1. **Sub-phase organization**: Sub-phase roadmaps live in `docs/architecture/designs/<design-name>/sub-phases/`
   - `roadmap.md` — overview and dependency graph
   - Individual sub-phase files (`4.1-cloud-transport.md`, etc.)
2. **Templates**: `docs/architecture/designs/_templates/` provides standard structures
   - `sub-phase-roadmap-template.md` — for creating roadmap.md
   - `sub-phase-template.md` — for individual sub-phase files
3. **Command updates**:
   - `/plan` detects sub-phase syntax (`/plan 4.1`) and reads from sub-phases/roadmap.md
   - `/implement-plan` recognizes sub-phase plans, checks prerequisites, displays validation checkpoints
4. **Roadmap integration**: `docs/roadmap.md` references sub-roadmaps where they exist
5. **User workflow**:
   ```
   /design phase-4 → (manual decompose) → /plan 4.1 → /implement-plan → [test] → /plan 4.2 → ...
   ```

### Decomposition Triggers

Sub-phase roadmaps are recommended when a design exhibits:
- **Size**: Exceeds ~100-150 lines
- **Trait boundaries**: Multiple trait definitions implementable independently
- **Platform splits**: OS-specific implementations (Windows/Linux)
- **Integration breadth**: Touches 3+ existing modules
- **Error surface**: Defines 8+ distinct error variants
- **Multi-step flows**: Contains 3+ operational flows

**Not just line count**: Decomposition is "more intelligent" — a 120-line design with a single trait + implementation + tests does not warrant decomposition, but a 150-line design with 3 distinct traits does.

---

## Alternatives Considered

### Alternative 1: Iterative Planning (within `/plan`)

Make `/plan` generate multi-checkpoint plans that `/implement-plan` executes iteratively:
```
/plan phase-4 → single plan with [Checkpoint 1] [Checkpoint 2] [Checkpoint 3]
/implement-plan → implements Checkpoint 1 → pauses → implements Checkpoint 2 → ...
```

**Rejected because**:
- Single plan file becomes very long (defeats decomposition purpose)
- User confirmation pauses within `/implement-plan` break flow
- Harder to resume if interrupted mid-implementation
- No natural file boundary for "this sub-phase is complete"

### Alternative 2: Automated `/decompose` Command

A new command automatically analyzes a design and proposes sub-phases:
```
/decompose phase-4
  ↓ analyzes sections, dependencies
  ↓ generates draft sub-roadmap
  ↓ user reviews and edits
```

**Deferred (not rejected)**: Automatic decomposition may miss logical boundaries that a human would identify. Decided to start with manual decomposition using a template; if this proves tedious in practice, revisit automatic decomposition as a future enhancement.

### Alternative 3: No Change (Single-Session Implementation)

Continue feeding full designs to `/implement-plan`, regardless of size.

**Rejected because**:
- User explicitly requested manual testing checkpoints between implementation units
- Phase 4 and Phase 6 have already demonstrated complexity beyond single-session feasibility
- Risk of implementation errors increases with design size

---

## Consequences

### Positive

1. **Incremental validation**: User can test each sub-phase before moving to the next, catching bugs early
2. **Reduced cognitive load**: Implementation agents receive focused 100-150 line contexts instead of 700+ lines
3. **Failure isolation**: If Phase 4.3 implementation fails, Phases 4.1-4.2 remain intact and don't need to be re-implemented
4. **Natural checkpoints**: Manual testing after each sub-phase catches integration issues early
5. **Thesis documentation**: Each checkpoint produces a test log entry for the bachelor report
6. **Flexibility**: If a sub-phase implementation strategy proves flawed, pivot without throwing away earlier sub-phases
7. **Alignment with existing structure**: Arx Runa already uses phase-based roadmap structure (`docs/roadmap.md`); sub-phase roadmaps extend this naturally

### Negative

1. **Slightly more upfront planning**: Requires manual decomposition step after `/design` before `/plan`
2. **Additional artifact**: Sub-phase roadmap files must be created and maintained
3. **Command complexity**: `/plan` and `/implement-plan` have additional logic for sub-phase detection

### Neutral

1. **Backward compatible**: Phases without sub-roadmaps continue using single-session implementation (no breaking changes)
2. **Optional**: Sub-roadmaps are not mandatory — only used when warranted by design size or logical separability

---

## Validation

### Phase 4 Example

The first sub-phase roadmap (`phase-4-cloud-synchronisation.md`) decomposes the 722-line cloud synchronisation design into:
- **Phase 4.1**: CloudTransport trait + MockTransport (~150 lines)
- **Phase 4.2**: Rclone integration + provider setup (~350 lines)
- **Phase 4.3**: Vault header upload/download (~120 lines)
- **Phase 4.4**: Manifest cloud backup (~150 lines)
- **Phase 4.5**: Push/pull flows + conflict detection (~400 lines)

Each sub-phase:
- Has clear deliverables (specific files, trait definitions, test suites)
- Declares dependencies (4.2 depends on 4.1, 4.3 depends on 4.2, etc.)
- Defines validation checkpoint (automated tests + manual verification)
- Is independently testable (4.1 uses `MockTransport`, 4.2 integration-tests with local Rclone)

### Success Criteria

The decision is validated when:
1. ✅ Template and README guide users through decomposition process
2. ✅ `/plan 4.1` generates focused plan using only Phase 4.1 design sections
3. ✅ `/implement-plan` displays validation checkpoints after sub-phase completion
4. ✅ User can execute `/plan 4.1` → `/implement-plan` → test → `/plan 4.2` workflow
5. ⏳ (Future) User completes Phase 4 using sub-phase roadmap and reports workflow was helpful

---

## References

- Research document: `c:\Users\chris\.copilot\session-state\8716c287-5ac4-4251-85e3-dd614115b636\research\right-now-the-workflow-idea-is-c-users-chris-sourc.md`
- Phase 4 sub-roadmap: `docs/architecture/designs/cloud-synchronisation/sub-phases/roadmap.md` (+ 5 individual sub-phase files)
- Template: `docs/architecture/designs/_templates/sub-phase-roadmap-template.md`
- Usage guide: `docs/architecture/designs/README.md` (decomposition heuristics)
- Development workflow: `docs/guides/development.md` § Working with Sub-Phase Roadmaps

---

## Notes

- If Phase 4 implementation validates the sub-phase workflow, apply to Phase 6 (tauri-ipc-and-frontend, 879 lines)
- Consider creating `/create-subroadmap` helper command in future if manual decomposition proves tedious
- Sub-phase roadmaps enable **Test-Driven Development (TDD)** at the module level: write tests for Phase 4.1, implement until tests pass, validate, move to 4.2
