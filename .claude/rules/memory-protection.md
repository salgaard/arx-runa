---
paths:
  - "src-tauri/src/memory/**"
---

# Memory protection

Platform-specific mlock/VirtualLock. Isolate unsafe in minimal submodules; expose only safe interfaces.

- Unsafe in smallest submodule (e.g., `src-tauri/src/memory/platform/unix.rs`); outer module wraps in safe RAII types (`SecureBytes<N>`)
- Safety comments: see rust.md
- Safe wrapper: lock on construction, unlock on drop; `ZeroizeOnDrop` — clear before unlock; no raw pointer exposure in public API
- mlock failure for session keys = hard error (no silent degradation); return `Result::Err` with actionable guidance, not panic
- mlock/VirtualLock wrapper exposes `Result<(), MemoryLockError>`; callers map into their own error enum (e.g., `AuthenticationError::MemoryLockFailed`)
