---
applyTo: "src-tauri/src/memory/**"
---

# Memory protection — rules

## Purpose
Platform-specific unsafe code (mlock/VirtualLock). Isolate unsafe in minimal submodules, expose only safe interfaces.

## Unsafe containment
- Unsafe in smallest submodule (e.g., `platform/unix.rs`)
- Outer module wraps in safe RAII types (`SecureBuffer`)
- Every `unsafe` block needs `// SAFETY:` comment

## Safe wrapper requirements
- Lock on construction, unlock on drop
- `ZeroizeOnDrop` — clear memory before unlock
- No raw pointer exposure in public API

## Error handling
- mlock failure for session keys = **hard error** (no silent degradation)
- Return `Result::Err` with actionable guidance, not panic
