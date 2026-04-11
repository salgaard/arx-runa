# Tauri IPC and Frontend — Sub-Phase Roadmap

**Parent design**: [`design.md`](../design.md)  
**Contract anchor**: [`design.md#contract-surface`](../design.md#contract-surface) is canonical for command/type/error contracts; sub-phases should reference it rather than duplicate full contract payloads.  
**Created**: 2026-04-04  
**Status**: Draft  
**Implementation order**: 6.1 → 6.2 → 6.3 → 6.4 (strict dependencies)

---

## Overview

This sub-phase roadmap decomposes the Tauri IPC and frontend design (1016 lines — the largest design document) into 4 independently testable implementation units, establishing a secure IPC surface before building frontend state and pages on top of it.

**Total sub-phases**: 4 (Phases 6.1 through 6.4)

**Rationale for decomposition**:
-  **Size**: Exceeds ~100-150 lines (1016 lines total)
-  **Trait boundaries**: IPC error/type definitions are independently implementable before frontend consumers exist
-  **Integration breadth**: Touches auth, storage, sync, and sharing modules on the backend; Leptos state contexts and page components on the frontend
-  **Error surface**: Defines 7 distinct `IpcError` variants plus `From` impls for 3 domain error types
-  **Multi-step flows**: Authentication flow, vault browse flow, file transfer with progress streaming, Zero-Trace state-clearing flow

**Implementation strategy**: Define the IPC contract and error sanitisation boundary first → build frontend state contexts and type-safe invoke wrapper → build page components against the contexts → harden with Zero-Trace compliance and CSP

**Profile framing**:
- **Full profile (canonical)** command surface: [`design.md#canonical-command-surface-normative`](../design.md#canonical-command-surface-normative)
- **MVP profile (optional)**: may narrow frontend UX slices, but must not change the canonical command surface

---

## Dependency Graph

```
6.1 (IPC core + error sanitisation)
 ↓
6.2 (Frontend state + invoke wrapper)
 ↓
6.3 (Frontend pages)
 ↓
6.4 (Zero-Trace + security hardening)
```

**Legend**:
- `→` strict dependency (must complete predecessor before starting)

---

## Sub-Phases

1. **[Phase 6.1: IPC Core, Error Sanitisation, and Types](6.1-ipc-core-and-error-sanitisation.md)**
   - `IpcError` enum with `serde(tag = "kind")` serialisation
   - `From` impls for `AuthError`, `StorageError`, `SyncError`
   - All IPC response types (`AuthResponse`, `FileEntry`, `ProgressUpdate`, etc.)
   - `AppState` struct
   - Input validation functions
   - Tauri command signatures and registration aligned with [`design.md#canonical-command-surface-normative`](../design.md#canonical-command-surface-normative)
   - **Estimated**: ~400 lines production code, ~150 lines tests

2. **[Phase 6.2: Frontend State Contexts and Tauri Invoke Wrapper](6.2-frontend-state-and-invoke-wrapper.md)**
   - `invoke_command<A, R>` type-safe wrapper over `window.__TAURI__.core.invoke`
   - `SessionProvider` with 5-second polling and clean shutdown
   - `VaultState` and `VaultActions` contexts
   - `SyncContext` for sync status tracking
   - **Estimated**: ~250 lines production code, ~50 lines tests

3. **[Phase 6.3: Frontend Pages](6.3-frontend-pages.md)**
   - `LoginPage` with password input and `KeyFileIndicator`
   - `VaultBrowser` with `FileList`, `Breadcrumbs`, and `UploadButton`
   - `ProgressModal` with real-time progress from `tauri::ipc::Channel`
   - `AppShell` layout with `SessionStatus` display
   - Vault creation UX for `chunk_size_bytes` and `epoch_buffer_enabled` with hybrid-routing explanation
   - Generic UI components (`Button`, `Input`, `Modal`, `Spinner`)
   - Leptos routing: login view when locked, vault browser when unlocked
   - **Estimated**: ~400 lines production code, ~50 lines tests

4. **[Phase 6.4: Zero-Trace Compliance and Security Hardening](6.4-zero-trace-and-security-hardening.md)**
   - CSP configuration in `tauri.conf.json`
   - State clearing on lock across all contexts
   - Password `zeroize` verification after `authenticate` IPC call
   - Clipboard capability denial in Tauri config
   - Exponential backoff on failed authentication
   - Integration tests for Zero-Trace behaviour
   - **Estimated**: ~100 lines production code, ~80 lines tests

---

## Testing Strategy

### Per-Sub-Phase Testing
Each sub-phase includes its own test suite. Tests must pass before proceeding to the next sub-phase.

**Test types**:
- **Unit tests**: Error mapping correctness, input validation, state transitions
- **Mock-based tests**: Frontend state tests use mock IPC responses (Phases 6.2, 6.3)
- **Property-based tests**: Input validation rejects adversarial paths (Phase 6.1)
- **Integration tests**: Zero-Trace compliance (no localStorage, state cleared after lock) in Phase 6.4

### Regression Testing
After completing each sub-phase, run:
```bash
cargo test           # All tests must pass
cargo clippy -- -D warnings  # No new warnings
trunk build          # Frontend must compile (from Phase 6.2 onwards)
```

### Manual Testing Checklist
- Phase 6.2: State contexts provide/consume without panic in browser
- Phase 6.3: Login → vault browser navigation works end to end
- Phase 6.4: Locking the vault clears all visible file data; browser DevTools show no localStorage entries

---

## Security Review Checkpoints

- **Phase 6.1**: Requires `security-reviewer` agent review — error sanitisation completeness, no key material in error messages, no sensitive internal filesystem paths leaked through IPC errors
- **Phase 6.2**: No security review needed (frontend state, no crypto)
- **Phase 6.3**: No security review needed (page components, no crypto)
- **Phase 6.4**: Requires `security-reviewer` agent review — Zero-Trace verification, CSP completeness, brute-force protection via auth backoff

---

## Implementation Workflow

```bash
# Phase 6.1
/plan 6.1
/implement-plan phase-006-1-ipc-core.md
cargo test ui::error
cargo test ui::validation
cargo build  # all commands must register without error

# Phase 6.2
/plan 6.2
/implement-plan phase-006-2-frontend-state.md
trunk build

# Phase 6.3
/plan 6.3
/implement-plan phase-006-3-frontend-pages.md
trunk build
# [Manual verification — login → vault browser flow]

# Phase 6.4
/plan 6.4
/implement-plan phase-006-4-zero-trace.md
cargo test ui::security
# [Manual verification — lock clears all state]
```

---

## Documentation Impact

**Files to create/update after sub-phase completion**:
- Phase 6.1: Create `docs/architecture-decisions/011-ipc-error-sanitisation.md`
- Phase 6.2: No documentation updates required
- Phase 6.3: No documentation updates required
- Phase 6.4: Add threat model entries for compromised WebView, clipboard attacks, and brute-force authentication; update `docs/roadmap.md` Phase 6 to mark complete; create architecture diagram for IPC command surface

---

## Notes

- **`withGlobalTauri: true`**: Required in `tauri.conf.json` for the Leptos WASM IPC wrapper to access `window.__TAURI__.core.invoke`. Treat this as a Phase 6.1/6.2 precondition; Phase 6.4 verifies and hardens the surrounding CSP/capabilities.
- **Password zeroization**: The design mandates zeroing the password string immediately after the `authenticate` IPC call completes. This is introduced in Phase 6.3 (`LoginPage`) and verified in Phase 6.4.
- **No localStorage anywhere**: The CSP in Phase 6.4 does not block `localStorage` at the HTTP level (CSP has no such directive), but the Zero-Trace audit verifies absence at the application level. Document this distinction.
- **Polling cleanup**: `SessionProvider` polling must stop on `on_cleanup` to avoid stale intervals after the component unmounts during logout.
- **Risks**:

| Risk | Mitigation |
|------|------------|
| Error messages inadvertently leak sensitive internal paths or key material | Exhaustive mapping in `From` impls; sanitisation unit tests with path/key fixture inputs |
| Leptos `use_context` panics if provider missing | Enforce context hierarchy in routing: all consumers are children of all providers |
| CSP blocks WASM execution | `'wasm-unsafe-eval'` in `script-src` is required; verify in Phase 6.4 |
| Auth backoff state lost on process restart | Backoff tracked in `SessionManager` in memory; acceptable — process restart resets attempt counter |

---

## References

- **Parent design**: `docs/architecture/designs/tauri-ipc-and-frontend/design.md`
- **Roadmap entry**: `docs/roadmap.md` Phase 6
- **Related ADRs**: `docs/architecture-decisions/011-ipc-error-sanitisation.md` (created in Phase 6.1)
