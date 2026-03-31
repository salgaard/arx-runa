---
applyTo: "src-tauri/src/memory/**"
---

# Memory protection — scoped rules

These rules apply to all files under `src-tauri/src/memory/`.

## Purpose

This module contains platform-specific unsafe code for memory protection (mlock,
VirtualLock, secure allocation). The goal is to isolate unsafe code in minimal
submodules and expose only safe interfaces.

## Contain unsafe in small modules

- Unsafe code lives in the smallest possible submodule (e.g., `mlock.rs`)
- The unsafe submodule exposes only the minimal safe or unsafe-marked functions
  needed by the outer module
- The outer module (`secure_buffer.rs`, `mod.rs`) wraps unsafe operations in
  safe types with RAII cleanup
- Users of this module interact only with the safe wrapper types

Example structure:
```
src-tauri/src/memory/
├── mod.rs              # Public API: SecureBuffer, SecureVec
├── secure_buffer.rs    # Safe wrapper with RAII
└── platform/
    ├── mod.rs          # Platform dispatch
    ├── unix.rs         # unsafe mlock/munlock
    └── windows.rs      # unsafe VirtualLock/VirtualUnlock
```

## SAFETY comments

Every `unsafe` block MUST have a `// SAFETY:` comment that explains:
1. **Why this is sound** — what invariants make this safe
2. **What the caller must uphold** — preconditions
3. **Why a safe alternative is not viable** — justification for unsafe

Example:
```rust
// SAFETY: `data` is a valid allocation from Vec, and will live at least as
// long as SecureBuffer (enforced by ownership). mlock only requires a valid
// pointer and length within the process address space.
unsafe { platform::mlock(data.as_ptr(), data.len())? };
```

## Safe wrapper requirements

Safe wrapper types (`SecureBuffer`, `SecureVec`) must:
- Call lock on construction, unlock on drop
- Implement `ZeroizeOnDrop` to clear memory before unlocking
- NOT expose raw pointers in their public API
- Handle platform errors gracefully (e.g., mlock failure due to ulimit)

## Platform abstraction

- Use `#[cfg(unix)]` and `#[cfg(windows)]` for platform dispatch
- Define a common internal trait or function signature for both platforms
- Test on both platforms in CI (or document platform-specific limitations)

## Error handling

- mlock/VirtualLock can fail (ulimit, insufficient permissions)
- **For session keys (authentication)**: mlock failure is a **hard error** — VoidGate
  refuses to create the session and returns a clear error message explaining how
  to fix it (see `docs/architecture/designs/authentication-and-session-management.md`)
- **Rationale**: a security product that silently degrades memory protection for
  session keys is not trustworthy. The required memory is < 1 KiB, well within
  default limits
- Do NOT panic on mlock failure — return `Result::Err` with actionable guidance
- Do NOT silently continue with unprotected memory for security-critical keys

## Required tests

- SecureBuffer zeroes memory on drop (verify via unsafe pointer inspection)
- SecureBuffer unlocks memory on drop (verify via platform API if possible)
- mlock failure returns error (not panic), error message includes fix instructions
- SecureBuffer cannot be cloned (sensitive data should not be duplicated)
