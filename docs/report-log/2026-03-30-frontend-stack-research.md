# Frontend Stack Research for VoidGate

> Research conducted: March 2026  
> Purpose: Evaluate frontend options for Phase 6 (Tauri IPC + Frontend)

## Context

VoidGate requires a frontend that:
- Integrates with Tauri's webview
- Supports the **Zero-Trace** pillar (RAM-based UI, no disk writes)
- Is learnable by someone with Blazor/C# background (new to JS frameworks)
- Provides a professional, dark-themed security app aesthetic

---

## Official Tauri Frontend Support

From `create-tauri-app`, Tauri officially supports these templates:

**JavaScript/TypeScript:**
- Vanilla / Vanilla-TS
- React / React-TS
- Vue / Vue-TS
- Svelte / Svelte-TS
- Solid / Solid-TS
- Angular
- Preact / Preact-TS

**Rust (WASM):**
- Leptos ✓
- Yew ✓
- Sycamore ✓

**.NET:**
- Blazor ✓

---

## Option Comparison Matrix

| Criterion | Svelte + Tailwind | React + Tailwind + Radix | Leptos (Rust) | Blazor (.NET) |
|-----------|-------------------|--------------------------|---------------|---------------|
| **Learning curve** | ★★★★★ Lowest | ★★★☆☆ Moderate | ★★★☆☆ Moderate | ★★★★★ Familiar |
| **Tauri integration** | ★★★★★ Official | ★★★★★ Official | ★★★★★ Official | ★★★★☆ Official |
| **Bundle size** | ~50KB | ~100KB | ~200KB (WASM) | ~500KB+ |
| **Component ecosystem** | ★★★☆☆ Growing | ★★★★★ Largest | ★★☆☆☆ Limited | ★★★★☆ Good |
| **Zero-Trace compliance** | ★★★★★ Easy | ★★★★★ Easy | ★★★★★ Native | ★★★☆☆ Harder |
| **Dark theme support** | ★★★★★ Native | ★★★★★ Native | ★★★★☆ Manual | ★★★★☆ Libraries |
| **Single language** | ❌ TS + Rust | ❌ TS + Rust | ✅ Rust only | ❌ C# + Rust |
| **Community size** | Large | Largest | Growing | Medium |

---

## Option 1: Svelte + Tailwind CSS + DaisyUI

**The "Gentle Introduction" Stack**

```
Framework:   Svelte 5 + TypeScript
Styling:     Tailwind CSS v4
Components:  DaisyUI (Tailwind plugin)
Build:       Vite
```

### Why Svelte for a Blazor Developer

Svelte's mental model is closest to Blazor:
- **Components are files** — `Button.svelte` like `Button.razor`
- **Reactive by default** — variables update the DOM automatically
- **No virtual DOM** — compiles to vanilla JS (like Blazor compiles to .NET)
- **HTML-first** — write HTML with sprinkled logic, not JSX

```svelte
<!-- Counter.svelte — feels like Blazor -->
<script lang="ts">
  let count = 0;
  function increment() { count += 1; }
</script>

<button on:click={increment}>
  Count: {count}
</button>

<style>
  button { @apply btn btn-primary; }
</style>
```

### Why DaisyUI

- Pre-styled components via CSS classes — no JS component library to learn
- Built-in dark theme: `data-theme="dark"` on root element
- Semantic class names: `btn`, `card`, `modal`, `input`
- Zero JavaScript — pure Tailwind plugin

### Pros
- ✅ Lowest learning curve for Blazor developers
- ✅ Smallest bundle size (~50KB)
- ✅ Official Tauri template
- ✅ Excellent TypeScript support
- ✅ No React/Vue mental model to unlearn

### Cons
- ⚠️ Smaller ecosystem than React
- ⚠️ DaisyUI is opinionated (less customisation than Radix)
- ⚠️ Fewer third-party libraries

### Zero-Trace Compliance
- No localStorage/IndexedDB by default ✅
- No service workers by default ✅
- Easy to audit (small surface area)

---

## Option 2: React + Tailwind CSS + Radix UI

**The "Industry Standard" Stack**

```
Framework:   React 19 + TypeScript
Styling:     Tailwind CSS v4
Components:  Radix UI (unstyled primitives)
Icons:       Lucide React
Build:       Vite
```

### Why React

- Largest ecosystem — most tutorials, Stack Overflow answers, examples
- Most Tauri projects use React — best community support
- Transferable skill — React knowledge applies everywhere

### Why Radix UI

- Unstyled accessible components — you control the look with Tailwind
- Industry-leading accessibility (WCAG 2.1 AA)
- Predictable, auditable component behaviour
- No "magic" — full control over DOM output

```tsx
// LoginModal.tsx
import * as Dialog from "@radix-ui/react-dialog";

export function LoginModal() {
  return (
    <Dialog.Root>
      <Dialog.Trigger className="btn btn-primary">
        Login
      </Dialog.Trigger>
      <Dialog.Content className="dark:bg-slate-900 p-6 rounded-lg">
        <Dialog.Title>Unlock Vault</Dialog.Title>
        {/* Password input, USB key selector, etc. */}
      </Dialog.Content>
    </Dialog.Root>
  );
}
```

### Pros
- ✅ Largest ecosystem and community
- ✅ Best documentation and learning resources
- ✅ Radix provides excellent accessibility out of the box
- ✅ Most examples for Tauri + React integration
- ✅ Full customisation control

### Cons
- ⚠️ Steeper learning curve (JSX, hooks, component lifecycle)
- ⚠️ More concepts to learn (useEffect, useState, useContext)
- ⚠️ React-specific patterns don't transfer to other frameworks

### Zero-Trace Compliance
- No localStorage/IndexedDB by default ✅
- No service workers by default ✅
- Must audit third-party libraries

---

## Option 3: Leptos (Rust + WASM)

**The "All Rust" Stack**

```
Framework:   Leptos 0.8+
Styling:     Tailwind CSS (via Trunk)
Components:  Thaw UI (Leptos component library)
Build:       Trunk or cargo-leptos
```

### Why Leptos

- **Single language** — Rust for both frontend and backend
- **Official Tauri template** — first-class support
- **Fine-grained reactivity** — no virtual DOM, efficient updates
- **Type safety** — compiler catches UI bugs

```rust
// counter.rs — Rust components
use leptos::*;

#[component]
fn Counter() -> impl IntoView {
    let (count, set_count) = signal(0);
    
    view! {
        <button
            class="btn btn-primary"
            on:click=move |_| set_count.update(|n| *n += 1)
        >
            "Count: " {count}
        </button>
    }
}
```

### Why Consider This

- You're already learning Rust for the backend
- No context-switching between TypeScript and Rust
- Keys and sensitive data never leave Rust (ultimate Zero-Trace)
- Type safety across the entire stack

### Pros
- ✅ Single language (Rust everywhere)
- ✅ Official Tauri template
- ✅ Ultimate Zero-Trace — sensitive data stays in Rust
- ✅ Fine-grained reactivity (better performance than React)
- ✅ Growing ecosystem with Thaw UI component library

### Cons
- ⚠️ WASM bundle size (~200KB overhead)
- ⚠️ Smaller component ecosystem than React
- ⚠️ Learning Leptos patterns on top of learning Rust
- ⚠️ Fewer tutorials and Stack Overflow answers
- ⚠️ Debugging WASM is harder than debugging JS

### Zero-Trace Compliance
- Native Rust memory management ✅
- `zeroize` works on frontend variables ✅
- No JavaScript layer to audit ✅
- Ideal for security-critical UI

---

## Option 4: Blazor (Keep Familiar)

**The "Stay in Your Lane" Stack**

```
Framework:   Blazor WebAssembly
Styling:     Bootstrap or MudBlazor
Build:       dotnet CLI
```

### Why Blazor

- You already know it
- Component libraries like MudBlazor are mature
- Official Tauri template exists

### Why NOT Blazor for VoidGate

- ❌ **Large bundle size** — ~500KB+ for Blazor WASM runtime
- ❌ **GC unpredictability** — .NET GC may retain plaintext in heap
- ❌ **Not Zero-Trace friendly** — harder to ensure memory is zeroed
- ❌ **Two runtimes** — .NET WASM + Rust backend = complexity
- ❌ **Interop overhead** — calling Rust from C# requires JS bridge

### Verdict

Not recommended for VoidGate due to Zero-Trace requirements. Blazor's
garbage collector makes it difficult to guarantee sensitive data is zeroed
from memory.

---

## Zero-Trace Requirements (All Options)

Regardless of framework choice, VoidGate's frontend must:

### Must Do
- [ ] Disable localStorage for sensitive data
- [ ] Disable IndexedDB entirely
- [ ] Disable service workers (or cache only static assets)
- [ ] Strip source maps in production builds
- [ ] Keep decrypted content in Tauri IPC responses only
- [ ] Clear UI state on session lock

### Tauri Configuration
```json
{
  "app": {
    "security": {
      "csp": "default-src 'self'; script-src 'self' 'wasm-unsafe-eval'"
    }
  },
  "tauri": {
    "allowlist": {
      "clipboard": { "writeText": false, "readText": false },
      "fs": {
        "deny": ["$APPLOCALDATA/EBWebView/**"]
      }
    }
  }
}
```

### Session State Pattern
```typescript
// Keep sensitive data in closures, not storage
let sessionState: SessionState | null = null;

async function decryptFile(fileId: string) {
  // Decrypt in Rust, return plaintext once
  const content = await invoke('decrypt_file', { fileId });
  // Display transiently — never persist
  return content;
}

function lockVault() {
  invoke('lock_session');  // Rust zeros keys
  sessionState = null;     // Clear JS state
}
```

---

## Recommendation

### For VoidGate Specifically

**Primary Recommendation: Svelte + Tailwind + DaisyUI**

Reasons:
1. **Lowest learning curve** — closest mental model to Blazor
2. **Smallest bundle** — fast startup for desktop app
3. **Simple auditing** — small surface area for Zero-Trace compliance
4. **Official Tauri support** — well-documented integration
5. **TypeScript** — type safety without WASM complexity

**Alternative if willing to invest more time: Leptos**

Reasons:
1. **Single language** — Rust everywhere
2. **Ultimate Zero-Trace** — `zeroize` on frontend variables
3. **Type safety** — compiler catches UI bugs
4. **Growing rapidly** — official Tauri template, active community

### Not Recommended
- **Blazor** — GC makes Zero-Trace difficult
- **React** — steeper learning curve, overkill for VoidGate's UI needs

---

## Decision

**Selected: Leptos (Rust + WASM)**

Rationale:
- Single-language architecture — Rust for both frontend and backend
- Ultimate Zero-Trace compliance — `zeroize` works on frontend variables
- Type safety across the entire stack
- Compelling narrative for bachelor report
- Official Tauri template support

Trade-offs accepted:
- Larger bundle size (~200KB WASM overhead)
- Smaller component ecosystem than React/Svelte
- Slower iteration (WASM compile times)
- Harder debugging (WASM stack traces)

See ADR: `docs/architecture-decisions/002-frontend-stack-selection.md`

## Next Steps

1. ✅ Decision made: Leptos
2. ✅ ADR created: `002-frontend-stack-selection.md`
3. Update `docs/guides/development.md` with Leptos setup instructions
4. Begin Phase 6 implementation with Leptos + Tailwind
