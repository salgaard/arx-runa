---
paths:
  - "src-tauri/src/memory/**"
---

# Memory protection — rules

## Purpose
Platform-specific unsafe code (mlock/VirtualLock). Isolate unsafe in minimal submodules, expose only safe interfaces.

## Unsafe containment
- Unsafe in smallest submodule (e.g., `src-tauri/src/memory/platform/unix.rs`)
- Outer module wraps in safe RAII types (`SecureBytes<N>`)
- Every `unsafe` block needs `// SAFETY:` comment

## Safe wrapper requirements
- Lock on construction, unlock on drop
- `ZeroizeOnDrop` — clear memory before unlock
- No raw pointer exposure in public API

## Error handling
- mlock failure for session keys = **hard error** (no silent degradation)
- Return `Result::Err` with actionable guidance, not panic
- The mlock / VirtualLock wrapper exposes a Result<(), MemoryLockError> surface; callers map it into their own error enum (e.g., auth converts to AuthenticationError::MemoryLockFailed).
